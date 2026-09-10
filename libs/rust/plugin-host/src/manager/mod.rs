use std::collections::{HashMap, HashSet};
use std::future::{Ready, ready};
use std::path::{Path as StdPath, PathBuf};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context as TaskContext, Poll};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tower::Service;
use url::Url;
use wasmtime::Engine;
use wasmtime::component::{Component, HasSelf, Linker};

use super::fetch::{PluginHttpClient, RequestProfile};
use super::host::{PluginState, create_store};
use super::media::MediaRefSpec;
use super::registry::{SourcePluginRegistryClient, SourcePluginRegistryVersion};

mod media_client;
mod metadata_cache;
mod plugin_file;
mod wasmtime_cache;

pub use media_client::PluginMediaClient;

use super::runtime::{MangaSource, manga};
use metadata_cache::{
    plugin_metadata_cache_path, read_plugin_metadata_cache, write_plugin_metadata_cache,
};
use plugin_file::{is_wasm_file, sanitize_plugin_filename};
use wasmtime_cache::{load_wasmtime_cache, map_wasmtime_err};

const PLUGIN_EPOCH_TICK_INTERVAL: Duration = Duration::from_millis(100);
// 100ms * 300 ticks gives each plugin call a 30 second Wasmtime budget.
const PLUGIN_EXECUTION_DEADLINE_EPOCH_TICKS: u64 = 300;

#[allow(clippy::struct_excessive_bools)]
#[derive(serde::Serialize, serde::Deserialize, Clone, utoipa::ToSchema)]
pub struct SourceInfo {
    pub name: String,
    pub display_name: String,
    pub base_url: String,
    pub plugin_version: String,
    pub plugin_api_version: u32,
    pub capabilities: Vec<String>,
    pub search_categories: Vec<String>,
    pub default_search_category: Option<String>,
    pub supports_search_popularity: bool,
    pub homepage: Option<String>,
    pub source_repository: Option<String>,
    pub build_metadata: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceCapability {
    Search,
    MangaDetails,
    ChapterList,
    PageList,
}

pub struct PluginInstallResult {
    pub source: SourceInfo,
    pub replaced_existing: bool,
}

pub struct PluginRegistryInstallResult {
    pub source: SourceInfo,
    pub artifact_path: String,
    pub replaced_existing: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PluginManagerError {
    #[error("Plugin '{name}' not found")]
    NotFound { name: String },
    #[error("Plugin '{name}' is disabled")]
    Disabled { name: String },
    #[error("Plugin '{name}' does not support '{capability}'")]
    UnsupportedCapability { name: String, capability: String },
}

#[derive(Debug)]
pub struct PluginRuntimeError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl PluginRuntimeError {
    #[must_use]
    /// Returns whether the runtime error looks like an upstream anti-bot block.
    pub fn is_upstream_blocked(&self) -> bool {
        let message = self.message.to_ascii_lowercase();
        message.contains("cloudflare blocked access")
            || message.contains("sorry, you have been blocked")
            || message.contains("attention required! | cloudflare")
    }
}

impl std::fmt::Display for PluginRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PluginRuntimeError {}

impl PluginManagerError {
    fn not_found(name: &str) -> Self {
        Self::NotFound {
            name: name.to_string(),
        }
    }

    fn disabled(name: &str) -> Self {
        Self::Disabled {
            name: name.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PluginHealth {
    pub loaded_sources: usize,
    pub enabled_sources: usize,
}

#[derive(Clone)]
pub struct PluginRuntimeActivity {
    call_tracker: Arc<PluginCallTracker>,
}

impl PluginRuntimeActivity {
    /// Waits until no source plugin call is executing.
    pub async fn wait_for_idle(&self) {
        self.call_tracker.wait_for_all_idle().await;
    }
}

struct PluginExecutionBudget {
    epoch_worker: JoinHandle<()>,
}

impl PluginExecutionBudget {
    fn start(engine: &Engine) -> Self {
        let engine = engine.weak();
        let epoch_worker = tokio::spawn(async move {
            let mut interval = tokio::time::interval(PLUGIN_EPOCH_TICK_INTERVAL);
            loop {
                interval.tick().await;
                let Some(engine) = engine.upgrade() else {
                    break;
                };
                engine.increment_epoch();
            }
        });

        Self { epoch_worker }
    }
}

impl Drop for PluginExecutionBudget {
    fn drop(&mut self) {
        self.epoch_worker.abort();
    }
}

struct PluginCallTracker {
    active_calls: Mutex<HashMap<String, usize>>,
    activity_version: Mutex<u64>,
    activity_changed: watch::Sender<u64>,
}

struct PluginCallPermit {
    tracker: Arc<PluginCallTracker>,
    plugin_name: String,
}

impl PluginCallTracker {
    fn new() -> Self {
        let (activity_changed, _) = watch::channel(0);
        Self {
            active_calls: Mutex::new(HashMap::new()),
            activity_version: Mutex::new(0),
            activity_changed,
        }
    }

    fn begin(self: &Arc<Self>, plugin_name: &str) -> PluginCallPermit {
        {
            let mut active_calls = self.active_calls();
            *active_calls.entry(plugin_name.to_string()).or_default() += 1;
        }

        PluginCallPermit {
            tracker: Arc::clone(self),
            plugin_name: plugin_name.to_string(),
        }
    }

    async fn wait_for_all_idle(&self) {
        loop {
            let mut activity_changed = self.activity_changed.subscribe();
            if self.active_calls().is_empty() {
                return;
            }
            if activity_changed.changed().await.is_err() {
                return;
            }
        }
    }

    async fn wait_for_plugin_idle(&self, plugin_name: &str) {
        loop {
            let mut activity_changed = self.activity_changed.subscribe();
            if !self.active_calls().contains_key(plugin_name) {
                return;
            }
            if activity_changed.changed().await.is_err() {
                return;
            }
        }
    }

    fn active_calls(&self) -> MutexGuard<'_, HashMap<String, usize>> {
        self.active_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn notify_activity_changed(&self) {
        let mut activity_version = self
            .activity_version
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *activity_version = activity_version.wrapping_add(1);
        let _ = self.activity_changed.send(*activity_version);
    }

    #[cfg(test)]
    fn active_call_count(&self, plugin_name: &str) -> usize {
        self.active_calls()
            .get(plugin_name)
            .copied()
            .unwrap_or_default()
    }
}

impl Drop for PluginCallPermit {
    fn drop(&mut self) {
        let mut active_calls = self.tracker.active_calls();
        if let Some(count) = active_calls.get_mut(&self.plugin_name) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                active_calls.remove(&self.plugin_name);
            }
        }
        drop(active_calls);
        self.tracker.notify_activity_changed();
    }
}

struct LoadedPlugin {
    info: SourceInfo,
    path: PathBuf,
    component: OnceLock<std::result::Result<Arc<Component>, String>>,
}

struct InspectedPlugin {
    source: SourceInfo,
    runtime: LoadedPlugin,
}

#[derive(Clone)]
struct PluginCallService {
    engine: Engine,
    http: PluginHttpClient,
}

struct PluginCall {
    plugin_name: String,
    component: Arc<Component>,
    operation: PluginOperation,
}

enum PluginOperation {
    Search(manga::source::types::SearchQuery),
    MangaDetails(String),
    ChapterList(String),
    PageList(String),
}

enum PluginCallOutput {
    Search(manga::source::types::SearchResults),
    MangaDetails(Box<manga::source::types::Manga>),
    ChapterList(Vec<manga::source::types::Chapter>),
    PageList(Vec<manga::source::types::Page>),
}

impl SourceCapability {
    const fn wire_name(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::MangaDetails => "manga_details",
            Self::ChapterList => "chapter_list",
            Self::PageList => "page_list",
        }
    }

    const fn from_runtime(capability: &manga::source::types::SourceCapability) -> Self {
        match capability {
            manga::source::types::SourceCapability::Search => Self::Search,
            manga::source::types::SourceCapability::MangaDetails => Self::MangaDetails,
            manga::source::types::SourceCapability::ChapterList => Self::ChapterList,
            manga::source::types::SourceCapability::PageList => Self::PageList,
        }
    }
}

pub struct PluginManager {
    engine: Engine,
    _execution_budget: PluginExecutionBudget,
    call_tracker: Arc<PluginCallTracker>,
    plugins_dir: PathBuf,
    plugins: HashMap<String, LoadedPlugin>,
    known_sources: HashMap<String, SourceInfo>,
    disabled_plugins: HashSet<String>,
    http: PluginHttpClient,
}

impl PluginManager {
    /// Creates a plugin manager rooted at `plugins_dir` and loads existing plugins.
    pub async fn new(plugins_dir: impl Into<PathBuf>) -> Result<Self> {
        let plugins_dir = plugins_dir.into();
        let plugins_dir_exists = backend_fs::path_exists(&plugins_dir);
        backend_fs::create_dir_all(&plugins_dir).await?;

        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        config.epoch_interruption(true);
        config.parallel_compilation(true);
        config.cache(Some(load_wasmtime_cache(&plugins_dir).await?));

        let engine = map_wasmtime_err(Engine::new(&config))?;
        let execution_budget = PluginExecutionBudget::start(&engine);

        let mut manager = Self {
            engine,
            _execution_budget: execution_budget,
            call_tracker: Arc::new(PluginCallTracker::new()),
            plugins_dir: plugins_dir.clone(),
            plugins: HashMap::new(),
            known_sources: HashMap::new(),
            disabled_plugins: HashSet::new(),
            http: PluginHttpClient::new()?,
        };

        if plugins_dir_exists {
            manager.load_plugins_from_dir().await?;
        } else {
            tracing::info!(
                path = %plugins_dir.display(),
                "Plugins Directory Created",
            );
        }

        Ok(manager)
    }

    /// Validates and installs a plugin WebAssembly component.
    ///
    /// Returns the inspected source metadata and whether an existing source was
    /// replaced.
    pub async fn install_plugin(
        &mut self,
        file_name: &str,
        wasm: &[u8],
    ) -> Result<PluginInstallResult> {
        self.install_plugin_checked(file_name, wasm, None).await
    }

    /// Downloads, verifies, and installs a plugin artifact from a registry entry.
    ///
    /// The installed component metadata must match the registry plugin id.
    pub async fn install_registry_plugin(
        &mut self,
        client: &SourcePluginRegistryClient,
        plugin_id: &str,
        version: &SourcePluginRegistryVersion,
    ) -> Result<PluginRegistryInstallResult> {
        let file_name = registry_artifact_file_name(plugin_id, version)?;
        let wasm = client.fetch_artifact(plugin_id, version).await?;
        let install = self
            .install_plugin_checked(&file_name, &wasm, Some(plugin_id))
            .await?;

        Ok(PluginRegistryInstallResult {
            source: install.source,
            artifact_path: file_name,
            replaced_existing: install.replaced_existing,
        })
    }

    async fn install_plugin_checked(
        &mut self,
        file_name: &str,
        wasm: &[u8],
        expected_source_id: Option<&str>,
    ) -> Result<PluginInstallResult> {
        let file_name = sanitize_plugin_filename(file_name)?;
        backend_fs::create_dir_all(&self.plugins_dir).await?;
        let path = self.plugins_dir.join(&file_name);

        let component = map_wasmtime_err(Component::new(&self.engine, wasm))?;
        let inspected = self.inspect_component(component, path.clone())?;
        if let Some(expected_source_id) = expected_source_id
            && inspected.source.name != expected_source_id
        {
            anyhow::bail!(
                "Registry plugin '{expected_source_id}' artifact metadata reported source id '{actual_source_id}'",
                actual_source_id = inspected.source.name,
            );
        }
        let replaced_existing = self.known_sources.contains_key(&inspected.source.name);

        backend_fs::write_bytes(&path, wasm)
            .await
            .with_context(|| {
                format!(
                    "Failed to write plugin to {path_display}",
                    path_display = path.display()
                )
            })?;
        if let Err(error) = write_plugin_metadata_cache(&path, &inspected.source).await {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "Plugin Metadata Cache Write Failed",
            );
        }

        let source = inspected.source.clone();
        self.register_inspected_plugin(inspected);

        tracing::info!(
            plugin = %source.name,
            path = %path.display(),
            replaced_existing,
            "Plugin Installed",
        );

        Ok(PluginInstallResult {
            source,
            replaced_existing,
        })
    }

    fn register_inspected_plugin(&mut self, inspected: InspectedPlugin) {
        Self::register_inspected_plugin_into(
            inspected,
            &mut self.plugins,
            &mut self.known_sources,
            &self.disabled_plugins,
        );
    }

    fn register_inspected_plugin_into(
        inspected: InspectedPlugin,
        plugins: &mut HashMap<String, LoadedPlugin>,
        known_sources: &mut HashMap<String, SourceInfo>,
        disabled_plugins: &HashSet<String>,
    ) {
        let key = inspected.source.name.clone();
        known_sources.insert(
            key.clone(),
            source_with_enabled(&inspected.runtime.info, disabled_plugins, true),
        );
        plugins.insert(key, inspected.runtime);
    }

    async fn load_plugin_from_path(&mut self, path: &StdPath) -> Result<String> {
        let inspected = self.inspect_plugin_from_path(path).await?;
        let name = inspected.source.name.clone();
        self.register_inspected_plugin(inspected);
        Ok(name)
    }

    async fn load_plugins_from_dir(&mut self) -> Result<()> {
        for path in backend_fs::list_paths_sorted(&self.plugins_dir).await? {
            if !is_wasm_file(&path) {
                continue;
            }
            match self.load_plugin_from_path(&path).await {
                Ok(name) => {
                    tracing::info!(
                        plugin = %name,
                        path = %path.display(),
                        "Plugin Loaded",
                    );
                }
                Err(err) => tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "Plugin Load Failed",
                ),
            }
        }

        Ok(())
    }

    /// Reloads all plugin components from the configured plugin directory.
    ///
    /// Disabled state is preserved for plugins that still exist.
    pub async fn reload_plugins_from_dir(&mut self) -> Result<()> {
        let disabled_plugins = self.disabled_plugins.clone();
        let mut plugins = HashMap::new();
        let mut known_sources = HashMap::new();
        for path in backend_fs::list_paths_sorted(&self.plugins_dir).await? {
            if !is_wasm_file(&path) {
                continue;
            }
            match self.inspect_plugin_from_path(&path).await {
                Ok(inspected) => {
                    let name = inspected.source.name.clone();
                    Self::register_inspected_plugin_into(
                        inspected,
                        &mut plugins,
                        &mut known_sources,
                        &disabled_plugins,
                    );
                    tracing::info!(
                        plugin = %name,
                        path = %path.display(),
                        "Plugin Reloaded",
                    );
                }
                Err(err) => tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "Plugin Reload Failed",
                ),
            }
        }

        self.call_tracker.wait_for_all_idle().await;
        self.plugins = plugins;
        self.known_sources = known_sources;
        self.disabled_plugins = disabled_plugins
            .into_iter()
            .filter(|name| self.plugins.contains_key(name))
            .collect();

        Ok(())
    }

    async fn inspect_plugin_from_path(&self, path: &StdPath) -> Result<InspectedPlugin> {
        if let Some(source) = read_plugin_metadata_cache(path).await? {
            return Ok(InspectedPlugin {
                runtime: LoadedPlugin::new_lazy(source.clone(), path.to_path_buf()),
                source,
            });
        }

        let component = map_wasmtime_err(Component::from_file(&self.engine, path))?;
        let inspected = self.inspect_component(component, path.to_path_buf())?;
        if let Err(error) = write_plugin_metadata_cache(path, &inspected.source).await {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "Plugin Metadata Cache Write Failed",
            );
        }
        Ok(inspected)
    }

    fn inspect_component(&self, component: Component, path: PathBuf) -> Result<InspectedPlugin> {
        let (mut store, instance) = self.instantiate_component(&component)?;
        let metadata = map_wasmtime_err(instance.call_metadata(&mut store))?;
        validate_metadata(&metadata)?;
        let capabilities = capabilities_from_runtime(&metadata.capabilities);

        let source = SourceInfo {
            name: metadata.id.clone(),
            display_name: metadata.name.clone(),
            base_url: metadata.base_url.clone(),
            plugin_version: metadata.version.clone(),
            plugin_api_version: metadata.api_version,
            capabilities,
            search_categories: metadata.search_options.categories.clone(),
            default_search_category: metadata.search_options.default_category.clone(),
            supports_search_popularity: metadata.search_options.supports_popular_sort,
            homepage: metadata.homepage.clone(),
            source_repository: metadata.repository.clone(),
            build_metadata: metadata.build_metadata.clone(),
            enabled: true,
        };

        let runtime = LoadedPlugin::new_eager(source.clone(), path, component);

        Ok(InspectedPlugin { source, runtime })
    }

    fn plugin_component(
        &self,
        plugin_name: &str,
        capability: SourceCapability,
    ) -> Result<Arc<Component>> {
        if self.disabled_plugins.contains(plugin_name) {
            return Err(PluginManagerError::disabled(plugin_name).into());
        }

        let plugin = self
            .plugins
            .get(plugin_name)
            .ok_or_else(|| PluginManagerError::not_found(plugin_name))?;

        plugin.require_capability(plugin_name, capability)?;
        plugin.component_arc(&self.engine)
    }

    fn instantiate_component(
        &self,
        component: &Component,
    ) -> Result<(wasmtime::Store<PluginState>, MangaSource)> {
        instantiate_component(&self.engine, self.http.clone(), component)
    }

    fn plugin_call_service(&self) -> PluginCallService {
        PluginCallService {
            engine: self.engine.clone(),
            http: self.http.clone(),
        }
    }

    fn call_plugin(
        &self,
        plugin_name: &str,
        operation: PluginOperation,
    ) -> Result<PluginCallOutput> {
        let component = self.plugin_component(plugin_name, operation.required_capability())?;
        let _call_permit = self.call_tracker.begin(plugin_name);
        let request = PluginCall {
            plugin_name: plugin_name.to_string(),
            component,
            operation,
        };
        self.plugin_call_service().execute(request)
    }

    /// Fetches media on behalf of a specific plugin.
    pub async fn fetch_media(
        &self,
        plugin_name: &str,
        media: &MediaRefSpec,
        default_profile: RequestProfile,
    ) -> Result<(Vec<u8>, String)> {
        self.media_client(plugin_name)?
            .fetch_media(media, default_profile)
            .await
    }

    /// Fetches media without requiring the source plugin to be installed.
    pub async fn fetch_media_unscoped(
        &self,
        media: &MediaRefSpec,
        default_profile: RequestProfile,
    ) -> Result<(Vec<u8>, String)> {
        self.unscoped_media_client()
            .fetch_media(media, default_profile)
            .await
    }

    /// Fetches an image URL using image-hotlink request defaults.
    pub async fn fetch_image_url(&self, plugin_name: &str, url: &str) -> Result<(Vec<u8>, String)> {
        self.fetch_media(
            plugin_name,
            &MediaRefSpec {
                url: url.to_string(),
                request: None,
                transform: None,
            },
            RequestProfile::ImageHotlink,
        )
        .await
    }

    /// Fetches a binary asset URL using binary-asset request defaults.
    pub async fn fetch_binary_url(
        &self,
        plugin_name: &str,
        url: &str,
    ) -> Result<(Vec<u8>, String)> {
        self.fetch_media(
            plugin_name,
            &MediaRefSpec {
                url: url.to_string(),
                request: None,
                transform: None,
            },
            RequestProfile::BinaryAsset,
        )
        .await
    }

    /// Returns a media client scoped to an installed plugin.
    pub fn media_client(&self, plugin_name: &str) -> Result<PluginMediaClient> {
        if !self.plugins.contains_key(plugin_name) {
            return Err(PluginManagerError::not_found(plugin_name).into());
        }

        Ok(PluginMediaClient {
            http: self.http.clone(),
        })
    }

    #[must_use]
    /// Returns a media client that is not scoped to a plugin source.
    pub fn unscoped_media_client(&self) -> PluginMediaClient {
        PluginMediaClient {
            http: self.http.clone(),
        }
    }

    #[must_use]
    /// Lists known sources in stable source-name order.
    pub fn sources(&self) -> Vec<SourceInfo> {
        let mut sources: Vec<SourceInfo> = self
            .known_sources
            .values()
            .map(|source| {
                source_with_enabled(
                    source,
                    &self.disabled_plugins,
                    self.plugins.contains_key(&source.name),
                )
            })
            .collect();
        sources.sort_by(|a, b| a.name.cmp(&b.name));
        sources
    }

    /// Verifies that enabled plugin components can be loaded.
    pub fn health_check(&self) -> Result<PluginHealth> {
        for (name, plugin) in &self.plugins {
            if self.disabled_plugins.contains(name) {
                continue;
            }

            plugin
                .component(&self.engine)
                .with_context(|| format!("plugin '{name}' failed to load"))?;
        }

        Ok(PluginHealth {
            loaded_sources: self.plugins.len(),
            enabled_sources: self
                .plugins
                .keys()
                .filter(|name| !self.disabled_plugins.contains(*name))
                .count(),
        })
    }

    /// Replaces the disabled-plugin set, ignoring unknown plugin names.
    pub fn set_disabled_plugins(&mut self, disabled_plugins: &[String]) {
        self.disabled_plugins.clear();

        for name in disabled_plugins {
            if self.plugins.contains_key(name) {
                self.disabled_plugins.insert(name.clone());
            }
        }
    }

    /// Enables or disables one installed plugin.
    pub fn set_plugin_enabled(&mut self, plugin_name: &str, enabled: bool) -> Result<()> {
        if !self.plugins.contains_key(plugin_name) {
            return Err(PluginManagerError::not_found(plugin_name).into());
        }

        if enabled {
            self.disabled_plugins.remove(plugin_name);
        } else {
            self.disabled_plugins.insert(plugin_name.to_string());
        }

        Ok(())
    }

    #[must_use]
    /// Lists disabled plugin names in stable order.
    pub fn disabled_plugins(&self) -> Vec<String> {
        let mut disabled: Vec<String> = self.disabled_plugins.iter().cloned().collect();
        disabled.sort();
        disabled
    }

    #[must_use]
    /// Returns a handle for observing plugin runtime activity.
    pub fn runtime_activity(&self) -> PluginRuntimeActivity {
        PluginRuntimeActivity {
            call_tracker: Arc::clone(&self.call_tracker),
        }
    }

    /// Runs a plugin search operation.
    pub fn search_manga(
        &self,
        plugin_name: &str,
        query: &str,
        page: u32,
        category: Option<&str>,
        popular: bool,
    ) -> Result<manga::source::types::SearchResults> {
        let request = manga::source::types::SearchQuery {
            text: query.to_string(),
            page,
            category: category.map(ToString::to_string),
            popular,
        };
        match self.call_plugin(plugin_name, PluginOperation::Search(request))? {
            PluginCallOutput::Search(results) => Ok(results),
            _ => anyhow::bail!("plugin '{plugin_name}' returned an unexpected search response"),
        }
    }

    /// Loads manga details from a plugin.
    pub fn get_manga_details(
        &self,
        plugin_name: &str,
        manga_id: &str,
    ) -> Result<manga::source::types::Manga> {
        match self.call_plugin(
            plugin_name,
            PluginOperation::MangaDetails(manga_id.to_string()),
        )? {
            PluginCallOutput::MangaDetails(manga) => Ok(*manga),
            _ => anyhow::bail!("plugin '{plugin_name}' returned an unexpected manga response"),
        }
    }

    /// Loads the chapter list for a manga from a plugin.
    pub fn get_chapter_list(
        &self,
        plugin_name: &str,
        manga_id: &str,
    ) -> Result<Vec<manga::source::types::Chapter>> {
        match self.call_plugin(
            plugin_name,
            PluginOperation::ChapterList(manga_id.to_string()),
        )? {
            PluginCallOutput::ChapterList(chapters) => Ok(chapters),
            _ => anyhow::bail!("plugin '{plugin_name}' returned an unexpected chapter response"),
        }
    }

    /// Loads the page list for a chapter from a plugin.
    pub fn get_page_list(
        &self,
        plugin_name: &str,
        chapter_id: &str,
    ) -> Result<Vec<manga::source::types::Page>> {
        match self.call_plugin(
            plugin_name,
            PluginOperation::PageList(chapter_id.to_string()),
        )? {
            PluginCallOutput::PageList(pages) => Ok(pages),
            _ => anyhow::bail!("plugin '{plugin_name}' returned an unexpected page response"),
        }
    }

    /// Returns the base URL declared by a plugin source.
    pub fn source_base_url(&self, plugin_name: &str) -> Result<String> {
        let plugin = self
            .plugins
            .get(plugin_name)
            .ok_or_else(|| PluginManagerError::not_found(plugin_name))?;

        Ok(plugin.info.base_url.clone())
    }

    /// Removes a plugin from memory and deletes the supplied artifact files.
    pub async fn delete_plugin(
        &mut self,
        plugin_name: &str,
        artifact_paths: &[String],
    ) -> Result<()> {
        self.call_tracker.wait_for_plugin_idle(plugin_name).await;
        self.plugins.remove(plugin_name);
        self.known_sources.remove(plugin_name);
        self.disabled_plugins.remove(plugin_name);

        for artifact_path in artifact_paths {
            let path = self.plugin_path(artifact_path);
            if backend_fs::path_exists(&path) {
                backend_fs::remove_file(&path).await.with_context(|| {
                    format!(
                        "Failed to remove plugin artifact {path_display}",
                        path_display = path.display()
                    )
                })?;
            }
            let metadata_path = plugin_metadata_cache_path(&path);
            if backend_fs::path_exists(&metadata_path) {
                backend_fs::remove_file(&metadata_path)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to remove plugin metadata cache {path_display}",
                            path_display = metadata_path.display()
                        )
                    })?;
            }
        }

        Ok(())
    }

    fn plugin_path(&self, artifact_path: &str) -> PathBuf {
        let path = PathBuf::from(artifact_path);
        if path.is_absolute() {
            path
        } else {
            self.plugins_dir.join(path)
        }
    }
}

impl LoadedPlugin {
    fn new_lazy(info: SourceInfo, path: PathBuf) -> Self {
        Self {
            info,
            path,
            component: OnceLock::new(),
        }
    }

    fn new_eager(info: SourceInfo, path: PathBuf, component: Component) -> Self {
        let component_cache = OnceLock::new();
        let _ = component_cache.set(Ok(Arc::new(component)));
        Self {
            info,
            path,
            component: component_cache,
        }
    }

    fn component(&self, engine: &Engine) -> Result<&Component> {
        match self.component.get_or_init(|| {
            map_wasmtime_err(Component::from_file(engine, &self.path))
                .map(Arc::new)
                .map_err(|error| error.to_string())
        }) {
            Ok(component) => Ok(component.as_ref()),
            Err(error) => Err(anyhow::anyhow!(error.clone())),
        }
    }

    fn component_arc(&self, engine: &Engine) -> Result<Arc<Component>> {
        match self.component.get_or_init(|| {
            map_wasmtime_err(Component::from_file(engine, &self.path))
                .map(Arc::new)
                .map_err(|error| error.to_string())
        }) {
            Ok(component) => Ok(Arc::clone(component)),
            Err(error) => Err(anyhow::anyhow!(error.clone())),
        }
    }

    fn require_capability(&self, plugin_name: &str, capability: SourceCapability) -> Result<()> {
        if self
            .info
            .capabilities
            .iter()
            .any(|candidate| candidate == capability.wire_name())
        {
            return Ok(());
        }

        Err(PluginManagerError::UnsupportedCapability {
            name: plugin_name.to_string(),
            capability: capability.wire_name().to_string(),
        }
        .into())
    }
}

impl PluginCallService {
    fn execute(&self, request: PluginCall) -> Result<PluginCallOutput> {
        let started_at = Instant::now();
        let operation = request.operation.name();
        let (mut store, instance) =
            instantiate_component(&self.engine, self.http.clone(), &request.component)?;

        let output = match request.operation {
            PluginOperation::Search(query) => PluginCallOutput::Search(plugin_result(
                map_wasmtime_err(instance.call_search(&mut store, &query))?,
            )?),
            PluginOperation::MangaDetails(manga_id) => {
                PluginCallOutput::MangaDetails(Box::new(plugin_result(map_wasmtime_err(
                    instance.call_get_manga(&mut store, &manga_id),
                )?)?))
            }
            PluginOperation::ChapterList(manga_id) => PluginCallOutput::ChapterList(plugin_result(
                map_wasmtime_err(instance.call_get_chapters(&mut store, &manga_id))?,
            )?),
            PluginOperation::PageList(chapter_id) => PluginCallOutput::PageList(plugin_result(
                map_wasmtime_err(instance.call_get_pages(&mut store, &chapter_id))?,
            )?),
        };

        tracing::trace!(
            plugin = %request.plugin_name,
            operation,
            elapsed_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
            "Plugin Call Completed",
        );

        Ok(output)
    }
}

impl Service<PluginCall> for PluginCallService {
    type Response = PluginCallOutput;
    type Error = anyhow::Error;
    type Future = Ready<Result<PluginCallOutput>>;

    fn poll_ready(&mut self, _cx: &mut TaskContext<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: PluginCall) -> Self::Future {
        ready(self.execute(request))
    }
}

impl PluginOperation {
    const fn name(&self) -> &'static str {
        match self {
            Self::Search(_) => "search",
            Self::MangaDetails(_) => "manga_details",
            Self::ChapterList(_) => "chapter_list",
            Self::PageList(_) => "page_list",
        }
    }

    const fn required_capability(&self) -> SourceCapability {
        match self {
            Self::Search(_) => SourceCapability::Search,
            Self::MangaDetails(_) => SourceCapability::MangaDetails,
            Self::ChapterList(_) => SourceCapability::ChapterList,
            Self::PageList(_) => SourceCapability::PageList,
        }
    }
}

fn instantiate_component(
    engine: &Engine,
    http: PluginHttpClient,
    component: &Component,
) -> Result<(wasmtime::Store<PluginState>, MangaSource)> {
    let mut store = create_store(engine, http);
    store.set_epoch_deadline(PLUGIN_EXECUTION_DEADLINE_EPOCH_TICKS);
    let mut linker = Linker::new(engine);
    map_wasmtime_err(wasmtime_wasi::p2::add_to_linker_sync(&mut linker))?;
    map_wasmtime_err(MangaSource::add_to_linker::<
        PluginState,
        HasSelf<PluginState>,
    >(&mut linker, |state| state))?;

    let instance = map_wasmtime_err(MangaSource::instantiate(&mut store, component, &linker))?;

    Ok((store, instance))
}

fn plugin_result<T>(
    result: std::result::Result<T, manga::source::types::PluginError>,
) -> Result<T> {
    result.map_err(|err| {
        PluginRuntimeError {
            code: err.code,
            message: err.message,
            retryable: err.retryable,
        }
        .into()
    })
}

fn validate_metadata(metadata: &manga::source::types::SourceMetadata) -> Result<()> {
    if metadata.id.trim().is_empty() {
        anyhow::bail!("Plugin id is required");
    }
    if metadata.name.trim().is_empty() {
        anyhow::bail!("Plugin name is required");
    }
    if metadata.base_url.trim().is_empty() {
        anyhow::bail!("Plugin base URL is required");
    }
    if metadata.version.trim().is_empty() {
        anyhow::bail!("Plugin version is required");
    }
    if metadata.api_version != crate::PLUGIN_API_VERSION {
        anyhow::bail!(
            "Plugin API version {plugin_api_version} does not match host API version {host_api_version}",
            plugin_api_version = metadata.api_version,
            host_api_version = crate::PLUGIN_API_VERSION,
        );
    }
    if let Some(default_category) = &metadata.search_options.default_category
        && !metadata
            .search_options
            .categories
            .iter()
            .any(|category| category == default_category)
    {
        anyhow::bail!("Default search category must be present in search_categories");
    }
    let capabilities = capabilities_from_runtime(&metadata.capabilities);
    if metadata.search_options.supports_popular_sort
        && !capabilities.iter().any(|capability| capability == "search")
    {
        anyhow::bail!("Search popularity requires search support");
    }
    if !metadata.search_options.categories.is_empty()
        && !capabilities.iter().any(|capability| capability == "search")
    {
        anyhow::bail!("Search categories require search support");
    }
    Ok(())
}

fn capabilities_from_runtime(
    capabilities: &[manga::source::types::SourceCapability],
) -> Vec<String> {
    capabilities
        .iter()
        .map(|capability| {
            SourceCapability::from_runtime(capability)
                .wire_name()
                .to_string()
        })
        .collect()
}

fn source_with_enabled(
    source: &SourceInfo,
    disabled_plugins: &HashSet<String>,
    loaded: bool,
) -> SourceInfo {
    let mut source = source.clone();
    source.enabled = loaded && !disabled_plugins.contains(&source.name);
    source
}

fn registry_artifact_file_name(
    plugin_id: &str,
    version: &SourcePluginRegistryVersion,
) -> Result<String> {
    if let Ok(url) = Url::parse(&version.artifact.url)
        && let Some(file_name) = url.path_segments().and_then(Iterator::last)
        && let Ok(file_name) = sanitize_plugin_filename(file_name)
    {
        return Ok(file_name);
    }

    sanitize_plugin_filename(&format!(
        "{plugin_id}-{version}.wasm",
        plugin_id = registry_filename_segment(plugin_id),
        version = registry_filename_segment(&version.version),
    ))
}

fn registry_filename_segment(value: &str) -> String {
    backend_core::sanitize_filename_segment(value, "plugin")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::SourcePluginRegistryArtifact;

    fn source_info(capabilities: Vec<String>) -> SourceInfo {
        SourceInfo {
            name: "demo".to_string(),
            display_name: "Demo".to_string(),
            base_url: "https://example.com".to_string(),
            plugin_version: "1.0.0".to_string(),
            plugin_api_version: crate::PLUGIN_API_VERSION,
            capabilities,
            search_categories: Vec::new(),
            default_search_category: None,
            supports_search_popularity: false,
            homepage: None,
            source_repository: None,
            build_metadata: None,
            enabled: true,
        }
    }

    #[test]
    fn loaded_plugin_enforces_declared_capabilities() {
        let plugin = LoadedPlugin::new_lazy(
            source_info(vec![SourceCapability::Search.wire_name().to_string()]),
            PathBuf::from("demo.wasm"),
        );

        plugin
            .require_capability("demo", SourceCapability::Search)
            .unwrap();

        let error = plugin
            .require_capability("demo", SourceCapability::PageList)
            .unwrap_err();
        let plugin_error = error.downcast_ref::<PluginManagerError>().unwrap();

        assert!(matches!(
            plugin_error,
            PluginManagerError::UnsupportedCapability { name, capability }
                if name == "demo" && capability == SourceCapability::PageList.wire_name()
        ));
    }

    #[tokio::test]
    async fn runtime_activity_waits_for_in_flight_calls() {
        let tracker = Arc::new(PluginCallTracker::new());
        let activity = PluginRuntimeActivity {
            call_tracker: Arc::clone(&tracker),
        };
        let permit = tracker.begin("demo");

        assert_eq!(tracker.active_call_count("demo"), 1);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), activity.wait_for_idle())
                .await
                .is_err()
        );

        drop(permit);

        tokio::time::timeout(Duration::from_millis(10), activity.wait_for_idle())
            .await
            .unwrap();
        assert_eq!(tracker.active_call_count("demo"), 0);
    }

    #[test]
    fn registry_artifact_filename_prefers_safe_url_basename() {
        let version = SourcePluginRegistryVersion {
            version: "1.2.3".to_string(),
            plugin_api_version: crate::PLUGIN_API_VERSION,
            artifact: SourcePluginRegistryArtifact {
                url: "https://example.com/downloads/comix.wasm?ignored=true".to_string(),
                sha256: String::new(),
                signature: None,
            },
            deprecated: false,
            deprecation_reason: None,
        };

        assert_eq!(
            registry_artifact_file_name("comix", &version).unwrap(),
            "comix.wasm"
        );
    }

    #[test]
    fn registry_artifact_filename_falls_back_to_plugin_and_version() {
        let version = SourcePluginRegistryVersion {
            version: "1.2.3+build/5".to_string(),
            plugin_api_version: crate::PLUGIN_API_VERSION,
            artifact: SourcePluginRegistryArtifact {
                url: "https://example.com/download".to_string(),
                sha256: String::new(),
                signature: None,
            },
            deprecated: false,
            deprecation_reason: None,
        };

        assert_eq!(
            registry_artifact_file_name("comix/source", &version).unwrap(),
            "comix-source-1.2.3-build-5.wasm"
        );
    }
}
