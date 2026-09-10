use crate::{
    app::{chapter_pages::DownloadedPageTransformCache, library_update},
    archive_index::ArchiveIndexService,
    downloader,
};
use backend_cache::MangaCache;
use backend_discord::{DiscordBot, DiscordServerState};
use backend_page_extraction::DownloadedPageExtractionScheduler;
use backend_plugin_host::PluginManager;
use backend_telemetry::Telemetry;
use secrecy::SecretString;
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;
use tokio::sync::{Mutex, Notify, RwLock};

#[derive(Clone)]
pub(crate) struct AppConfig {
    pub(crate) db_path: PathBuf,
    pub(crate) cache_disk_path: PathBuf,
    pub(crate) backend_api_key: Option<SecretString>,
}

#[derive(Debug, Default)]
pub(crate) struct DownloadStorageUsage {
    pub(crate) path: Option<PathBuf>,
    pub(crate) bytes: u64,
    pub(crate) refreshed_at: Option<Instant>,
    pub(crate) refresh_in_progress: bool,
}

pub(crate) struct AppState {
    pub(crate) config: AppConfig,
    pub(crate) db: backend_persistence::Database,
    pub(crate) plugin_manager: RwLock<PluginManager>,
    pub(crate) cache: MangaCache,
    pub(crate) archive_index: Arc<ArchiveIndexService>,
    pub(crate) extraction_scheduler: Arc<DownloadedPageExtractionScheduler>,
    pub(crate) downloaded_page_transform_cache: DownloadedPageTransformCache,
    pub(crate) telemetry: Telemetry,
    pub(crate) discord: Option<DiscordBot>,
    pub(crate) download_queue_notify: Notify,
    pub(crate) active_download_cancellations:
        Mutex<HashMap<String, Arc<downloader::DownloadCancellation>>>,
    pub(crate) download_storage_usage: Mutex<DownloadStorageUsage>,
    pub(crate) shutdown_drain: ShutdownDrain,
}

pub(crate) struct AppStateParts {
    pub(crate) config: AppConfig,
    pub(crate) db: backend_persistence::Database,
    pub(crate) plugin_manager: PluginManager,
    pub(crate) cache: MangaCache,
    pub(crate) telemetry: Telemetry,
    pub(crate) discord: Option<DiscordBot>,
}

pub(crate) fn build_app_state(parts: AppStateParts) -> Arc<AppState> {
    let AppStateParts {
        config,
        db,
        plugin_manager,
        cache,
        telemetry,
        discord,
    } = parts;

    Arc::new(AppState {
        config,
        db,
        plugin_manager: RwLock::new(plugin_manager),
        cache,
        archive_index: Arc::new(ArchiveIndexService::new()),
        extraction_scheduler: Arc::new(DownloadedPageExtractionScheduler::default()),
        downloaded_page_transform_cache: DownloadedPageTransformCache::default(),
        telemetry,
        discord,
        download_queue_notify: Notify::new(),
        active_download_cancellations: Mutex::new(HashMap::new()),
        download_storage_usage: Mutex::new(DownloadStorageUsage::default()),
        shutdown_drain: ShutdownDrain::default(),
    })
}

#[cfg(test)]
pub(crate) async fn build_test_app_state(
    label: &str,
    telemetry_name: &'static str,
) -> Arc<AppState> {
    let root = crate::test_support::temp_path(label);
    let db_path = root.join("test.sqlite3");
    let cache_path = root.join("cache");
    let plugins_path = root.join("plugins");
    let download_path = root.join("downloads");

    std::fs::create_dir_all(&root).expect("test root should be created");
    std::fs::create_dir_all(&download_path).expect("test download path should be created");

    let db = backend_persistence::Database::new(&db_path.to_string_lossy())
        .await
        .expect("database should initialize");
    db.set_setting("download_path", &download_path.to_string_lossy())
        .await
        .expect("download path setting should be stored");

    let plugin_manager = PluginManager::new(plugins_path)
        .await
        .expect("plugin manager should initialize");
    let cache = MangaCache::new(&cache_path.to_string_lossy(), 1024 * 1024)
        .await
        .expect("cache should initialize");

    build_app_state(AppStateParts {
        config: AppConfig {
            db_path,
            cache_disk_path: cache_path,
            backend_api_key: None,
        },
        db,
        plugin_manager,
        cache,
        telemetry: Telemetry::for_test(telemetry_name),
        discord: None,
    })
}

#[derive(Debug, Default)]
pub(crate) struct ShutdownDrain {
    draining: AtomicBool,
}

impl ShutdownDrain {
    pub(crate) fn begin(&self) {
        self.draining.store(true, Ordering::Release);
    }

    pub(crate) fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Acquire)
    }
}

impl DiscordServerState for AppState {
    fn discord_bot(&self) -> Option<DiscordBot> {
        self.discord.clone()
    }

    fn db(&self) -> &backend_persistence::Database {
        &self.db
    }

    fn plugin_manager(&self) -> &RwLock<PluginManager> {
        &self.plugin_manager
    }

    fn record_discord_notification(&self, kind: &str, outcome: &str) {
        self.telemetry
            .metrics
            .record_discord_notification(kind, outcome);
    }

    fn check_for_updates(
        self: Arc<Self>,
        trigger: &'static str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<usize>> + Send>> {
        Box::pin(async move { Ok(library_update::run(self, trigger).await?.new_chapters) })
    }
}
