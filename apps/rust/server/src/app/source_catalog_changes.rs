use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use autometrics::autometrics;
use backend_persistence::{Database, PluginArtifactRecordInput, SourceRecordInput};
use backend_plugin_host::{
    PLUGIN_API_VERSION, PluginManager, SourceInfo,
    registry::{SourcePluginRegistry, SourcePluginRegistryClient, SourcePluginRegistryVersion},
};
use backend_telemetry::trace;
use tracing::Instrument as _;

use crate::{
    AppState,
    api::{
        dto::{
            OperationStatusResponse, SetSourceEnabledResponse, SourceSettingsResponse,
            UploadPluginResponse,
        },
        error::AppError,
    },
    app::route_snapshot_invalidation,
};

const BOOTSTRAP_REGISTRY_PLUGINS: &[&str] = &["comix"];

#[autometrics]
#[tracing::instrument(name = "app.sources.plugin.set_enabled", skip_all, fields(source = %name, enabled, outcome = tracing::field::Empty))]
pub async fn set_source_enabled(
    state: &Arc<AppState>,
    name: &str,
    enabled: bool,
) -> Result<SetSourceEnabledResponse, AppError> {
    let span = tracing::Span::current();
    let plugin_span = trace::plugin_operation_span(name, "set_enabled");
    let plugin_started_at = Instant::now();
    {
        let mut plugins = state.plugin_manager.write().await;
        let _entered = plugin_span.enter();
        plugins.set_plugin_enabled(name, enabled)?;
    }
    trace::record_outcome(&plugin_span, "success");
    trace::record_duration(&plugin_span, plugin_started_at.elapsed());

    let db_span = trace::db_operation_span("set_source_enabled", "source");
    let db_started_at = Instant::now();
    async { state.db.set_source_enabled(name, enabled).await }
        .instrument(db_span.clone())
        .await?;
    trace::record_outcome(&db_span, "success");
    trace::record_duration(&db_span, db_started_at.elapsed());
    source_enabled_changed(state, name, enabled);
    state.telemetry.metrics.record_source_plugin_change(
        name,
        if enabled { "enable" } else { "disable" },
        "success",
    );
    trace::record_outcome(&span, "success");

    Ok(SetSourceEnabledResponse {
        ok: true,
        name: name.to_owned(),
        enabled,
    })
}

#[autometrics(track_concurrency)]
#[tracing::instrument(name = "app.sources.plugin.upload", skip_all, fields(file_name = %file_name, wasm_bytes = wasm.len(), source = tracing::field::Empty, outcome = tracing::field::Empty))]
pub async fn upload_plugin(
    state: &Arc<AppState>,
    file_name: &str,
    wasm: &[u8],
) -> Result<UploadPluginResponse, AppError> {
    let span = tracing::Span::current();
    let plugin_span = trace::plugin_operation_span("unknown", "install");
    let plugin_started_at = Instant::now();
    let install = async {
        let mut plugins = state.plugin_manager.write().await;
        plugins
            .install_plugin(file_name, wasm)
            .await
            .map_err(AppError::bad_request)
    }
    .instrument(plugin_span.clone())
    .await?;
    span.record("source", tracing::field::display(&install.source.name));
    trace::record_outcome(&plugin_span, "success");
    trace::record_duration(&plugin_span, plugin_started_at.elapsed());

    sync_current_sources(state).await?;
    let upload_outcome = if install.replaced_existing {
        "replaced"
    } else {
        "installed"
    };
    let db_span = trace::db_operation_span("record_plugin_artifact", "plugin_artifact");
    let db_started_at = Instant::now();
    async {
        state
            .db
            .record_plugin_artifact(&PluginArtifactRecordInput {
                key: install.source.name.clone(),
                artifact_path: file_name.to_owned(),
                plugin_version: install.source.plugin_version.clone(),
                plugin_api_version: install.source.plugin_api_version,
            })
            .await
    }
    .instrument(db_span.clone())
    .await?;
    trace::record_outcome(&db_span, "success");
    trace::record_duration(&db_span, db_started_at.elapsed());
    source_plugin_uploaded(state, &install.source.name);
    state.telemetry.metrics.record_source_plugin_change(
        &install.source.name,
        "upload",
        upload_outcome,
    );
    trace::record_outcome(&span, upload_outcome);

    Ok(UploadPluginResponse {
        filename: file_name.to_owned(),
        source: install.source.name,
        replaced_existing: install.replaced_existing,
    })
}

#[autometrics(track_concurrency)]
#[tracing::instrument(name = "app.sources.plugin.reload", skip_all, fields(outcome = tracing::field::Empty, item_count = tracing::field::Empty))]
pub async fn reload_sources(state: &Arc<AppState>) -> Result<OperationStatusResponse, AppError> {
    let span = tracing::Span::current();
    let plugin_span = trace::plugin_operation_span("all", "reload");
    let plugin_started_at = Instant::now();
    let source_records = async {
        let mut plugins = state.plugin_manager.write().await;
        plugins.reload_plugins_from_dir().await?;
        Ok::<Vec<_>, AppError>(
            plugins
                .sources()
                .iter()
                .map(source_record_input)
                .collect::<Vec<_>>(),
        )
    }
    .instrument(plugin_span.clone())
    .await?;
    trace::record_outcome(&plugin_span, "success");
    trace::record_item_count(&plugin_span, source_records.len());
    trace::record_duration(&plugin_span, plugin_started_at.elapsed());

    let db_span = trace::db_operation_span("sync_sources", "source");
    let db_started_at = Instant::now();
    async { state.db.sync_sources(&source_records).await }
        .instrument(db_span.clone())
        .await?;
    trace::record_outcome(&db_span, "success");
    trace::record_rows(&db_span, source_records.len());
    trace::record_duration(&db_span, db_started_at.elapsed());
    state.cache.clear().await?;
    source_plugins_reloaded(state);
    state
        .telemetry
        .metrics
        .record_source_plugin_change("all", "reload", "success");
    trace::record_outcome(&span, "success");
    trace::record_item_count(&span, source_records.len());

    Ok(OperationStatusResponse::ok())
}

#[autometrics]
#[tracing::instrument(name = "app.sources.settings.update", skip_all, fields(source = %name, hide_nsfw = tracing::field::Empty, outcome = tracing::field::Empty))]
pub async fn update_source_settings(
    state: &Arc<AppState>,
    name: &str,
    hide_nsfw: Option<bool>,
) -> Result<SourceSettingsResponse, AppError> {
    let span = tracing::Span::current();
    let plugins = state.plugin_manager.read().await;
    plugins.source_base_url(name)?;
    drop(plugins);

    if let Some(value) = hide_nsfw {
        span.record("hide_nsfw", value);
        let db_span = trace::db_operation_span("set_source_hide_nsfw", "source_settings");
        let db_started_at = Instant::now();
        async { state.db.set_source_hide_nsfw(name, value).await }
            .instrument(db_span.clone())
            .await?;
        trace::record_outcome(&db_span, "success");
        trace::record_duration(&db_span, db_started_at.elapsed());
    }
    source_settings_changed(state, name);
    state
        .telemetry
        .metrics
        .record_source_plugin_change(name, "settings_update", "success");
    trace::record_outcome(&span, "success");

    Ok(SourceSettingsResponse {
        name: name.to_owned(),
        hide_nsfw: state.db.get_source_hide_nsfw(name).await?,
    })
}

#[autometrics(track_concurrency)]
#[tracing::instrument(name = "app.sources.plugin.delete", skip_all, fields(source = %name, dependency_count = tracing::field::Empty, artifact_count = tracing::field::Empty, outcome = tracing::field::Empty))]
pub async fn delete_source(state: &Arc<AppState>, name: &str) -> Result<(), AppError> {
    let span = tracing::Span::current();
    let dependency_span = trace::db_operation_span("source_library_dependency_count", "source");
    let dependency_started_at = Instant::now();
    let dependency_count = async { state.db.source_library_dependency_count(name).await }
        .instrument(dependency_span.clone())
        .await?;
    trace::record_outcome(&dependency_span, "success");
    trace::record_rows(&dependency_span, dependency_count);
    trace::record_duration(&dependency_span, dependency_started_at.elapsed());
    span.record(
        "dependency_count",
        u64::try_from(dependency_count).unwrap_or(u64::MAX),
    );
    if dependency_count > 0 {
        trace::record_outcome(&span, "conflict");
        return Err(AppError::new(
            http::StatusCode::CONFLICT,
            "conflict",
            "Cannot delete plugin while library series still depend on it",
            Some(serde_json::json!({
                "source": name,
                "dependent_series_count": dependency_count,
            })),
        ));
    }

    let artifact_span = trace::db_operation_span("list_plugin_artifacts", "plugin_artifact");
    let artifact_started_at = Instant::now();
    let artifacts = async { state.db.list_plugin_artifacts(name).await }
        .instrument(artifact_span.clone())
        .await?;
    trace::record_outcome(&artifact_span, "success");
    trace::record_rows(&artifact_span, artifacts.len());
    trace::record_duration(&artifact_span, artifact_started_at.elapsed());
    span.record(
        "artifact_count",
        u64::try_from(artifacts.len()).unwrap_or(u64::MAX),
    );
    let artifact_paths = artifacts
        .into_iter()
        .map(|artifact| artifact.artifact_path)
        .collect::<Vec<_>>();

    let plugin_span = trace::plugin_operation_span(name, "delete");
    let plugin_started_at = Instant::now();
    async {
        let mut plugins = state.plugin_manager.write().await;
        plugins.delete_plugin(name, &artifact_paths).await
    }
    .instrument(plugin_span.clone())
    .await?;
    trace::record_outcome(&plugin_span, "success");
    trace::record_duration(&plugin_span, plugin_started_at.elapsed());

    let delete_span = trace::db_operation_span("delete_source_and_artifacts", "source");
    let delete_started_at = Instant::now();
    async { state.db.delete_source_and_artifacts(name).await }
        .instrument(delete_span.clone())
        .await?;
    trace::record_outcome(&delete_span, "success");
    trace::record_duration(&delete_span, delete_started_at.elapsed());
    source_plugin_deleted(state, name);
    state
        .telemetry
        .metrics
        .record_source_plugin_change(name, "delete", "success");
    trace::record_outcome(&span, "success");
    Ok(())
}

#[autometrics(track_concurrency)]
#[tracing::instrument(name = "app.sources.startup.install_registry_plugins", skip_all, fields(registry_configured = registry_url.is_some()))]
pub(crate) async fn install_configured_registry_plugins(
    plugin_manager: &mut PluginManager,
    db: &Database,
    registry_url: Option<&str>,
) -> anyhow::Result<()> {
    let Some(registry_url) = registry_url.map(str::trim).filter(|url| !url.is_empty()) else {
        return Ok(());
    };

    let client = SourcePluginRegistryClient::new()
        .context("failed to initialize source plugin registry client")?;
    let registry = client.fetch_manifest(registry_url).await.with_context(|| {
        format!("failed to fetch source plugin registry manifest from {registry_url}")
    })?;

    for plugin_id in BOOTSTRAP_REGISTRY_PLUGINS {
        install_registry_plugin_if_needed(plugin_manager, db, &client, &registry, plugin_id)
            .await?;
    }

    Ok(())
}

#[autometrics]
#[tracing::instrument(name = "app.sources.startup.sync_catalog", skip_all, fields(item_count = tracing::field::Empty))]
pub(crate) async fn sync_startup_source_catalog(
    plugin_manager: &PluginManager,
    db: &Database,
) -> anyhow::Result<usize> {
    let source_records = plugin_manager
        .sources()
        .iter()
        .map(source_record_input)
        .collect::<Vec<_>>();
    tracing::Span::current().record(
        "item_count",
        u64::try_from(source_records.len()).unwrap_or(u64::MAX),
    );
    db.sync_sources(&source_records)
        .await
        .context("failed to sync startup source catalog")?;
    Ok(source_records.len())
}

async fn install_registry_plugin_if_needed(
    plugin_manager: &mut PluginManager,
    db: &Database,
    client: &SourcePluginRegistryClient,
    registry: &SourcePluginRegistry,
    plugin_id: &str,
) -> anyhow::Result<()> {
    let version = registry
        .latest_compatible_version(plugin_id, PLUGIN_API_VERSION)
        .with_context(|| {
            format!("source plugin registry has no compatible '{plugin_id}' artifact")
        })?;
    if loaded_registry_plugin_is_current(plugin_manager, plugin_id, version) {
        tracing::info!(
            plugin = plugin_id,
            version = %version.version,
            plugin_api_version = version.plugin_api_version,
            "Source Plugin Registry Install Skipped",
        );
        return Ok(());
    }

    let install = plugin_manager
        .install_registry_plugin(client, plugin_id, version)
        .await
        .with_context(|| format!("failed to install source plugin '{plugin_id}' from registry"))?;
    db.record_plugin_artifact(&PluginArtifactRecordInput {
        key: install.source.name.clone(),
        artifact_path: install.artifact_path.clone(),
        plugin_version: install.source.plugin_version.clone(),
        plugin_api_version: install.source.plugin_api_version,
    })
    .await
    .with_context(|| {
        format!(
            "failed to record registry-installed artifact for source plugin '{}'",
            install.source.name
        )
    })?;

    tracing::info!(
        plugin = %install.source.name,
        artifact = %install.artifact_path,
        plugin_version = %install.source.plugin_version,
        plugin_api_version = install.source.plugin_api_version,
        replaced_existing = install.replaced_existing,
        "Source Plugin Installed From Registry",
    );

    Ok(())
}

fn loaded_registry_plugin_is_current(
    plugin_manager: &PluginManager,
    plugin_id: &str,
    registry_version: &SourcePluginRegistryVersion,
) -> bool {
    plugin_manager.sources().iter().any(|source| {
        source.name == plugin_id
            && source.plugin_version == registry_version.version
            && source.plugin_api_version == registry_version.plugin_api_version
    })
}

#[tracing::instrument(name = "app.sources.sync_current", skip_all, fields(item_count = tracing::field::Empty, outcome = tracing::field::Empty))]
async fn sync_current_sources(state: &Arc<AppState>) -> Result<(), AppError> {
    let span = tracing::Span::current();
    let source_records = {
        let plugins = state.plugin_manager.read().await;
        plugins
            .sources()
            .iter()
            .map(source_record_input)
            .collect::<Vec<_>>()
    };
    span.record(
        "item_count",
        u64::try_from(source_records.len()).unwrap_or(u64::MAX),
    );
    let db_span = trace::db_operation_span("sync_sources", "source");
    let db_started_at = Instant::now();
    async { state.db.sync_sources(&source_records).await }
        .instrument(db_span.clone())
        .await?;
    trace::record_outcome(&db_span, "success");
    trace::record_rows(&db_span, source_records.len());
    trace::record_duration(&db_span, db_started_at.elapsed());
    trace::record_outcome(&span, "success");
    Ok(())
}

pub(crate) fn source_record_input(source: &SourceInfo) -> SourceRecordInput {
    SourceRecordInput {
        key: source.name.clone(),
        display_name: source.display_name.clone(),
        base_url: source.base_url.clone(),
        version: source.plugin_version.clone(),
        plugin_api_version: source.plugin_api_version,
        capabilities: source.capabilities.clone(),
        enabled: source.enabled,
    }
}

fn source_plugin_uploaded(state: &AppState, source: &str) {
    state.downloaded_page_transform_cache.clear();
    route_snapshot_invalidation::source_plugin_uploaded(state, source);
}

fn source_plugins_reloaded(state: &AppState) {
    state.downloaded_page_transform_cache.clear();
    route_snapshot_invalidation::source_plugins_reloaded(state);
}

fn source_enabled_changed(state: &AppState, source: &str, enabled: bool) {
    state.downloaded_page_transform_cache.clear();
    route_snapshot_invalidation::source_enabled_changed(state, source, enabled);
}

fn source_settings_changed(state: &AppState, source: &str) {
    state.downloaded_page_transform_cache.clear();
    route_snapshot_invalidation::source_settings_changed(state, source);
}

fn source_plugin_deleted(state: &AppState, source: &str) {
    state.downloaded_page_transform_cache.clear();
    route_snapshot_invalidation::source_plugin_deleted(state, source);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_record_mapping_preserves_catalog_fields() {
        let source = SourceInfo {
            name: "demo".to_string(),
            display_name: "Demo".to_string(),
            base_url: "https://example.com".to_string(),
            plugin_version: "1.2.3".to_string(),
            plugin_api_version: 4,
            capabilities: vec!["search".to_string(), "manga".to_string()],
            search_categories: vec!["all".to_string()],
            default_search_category: Some("all".to_string()),
            supports_search_popularity: true,
            homepage: Some("https://example.com/home".to_string()),
            source_repository: Some("https://example.com/repo".to_string()),
            build_metadata: Some("local-dev".to_string()),
            enabled: true,
        };

        let record = source_record_input(&source);

        assert_eq!(record.key, "demo");
        assert_eq!(record.display_name, "Demo");
        assert_eq!(record.base_url, "https://example.com");
        assert_eq!(record.version, "1.2.3");
        assert_eq!(record.plugin_api_version, 4);
        assert_eq!(record.capabilities, ["search", "manga"]);
        assert!(record.enabled);
    }
}
