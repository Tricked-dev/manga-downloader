use crate::{
    AppState,
    api::{
        dto::{SetSourceEnabledResponse, SourceSettingsResponse},
        error::AppError,
    },
    app::route_snapshot_invalidation,
};
use anyhow::Context as _;
use autometrics::autometrics;
use backend_persistence::{Database, SourceRecordInput};
use backend_sources::{SourceInfo, SourceRegistry};
use backend_telemetry::trace;
use std::{sync::Arc, time::Instant};
use tracing::Instrument as _;

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
        let mut plugins = state.source_registry.write().await;
        let _entered = plugin_span.enter();
        plugins.set_source_enabled(name, enabled)?;
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

#[autometrics]
#[tracing::instrument(name = "app.sources.settings.update", skip_all, fields(source = %name, hide_nsfw = tracing::field::Empty, outcome = tracing::field::Empty))]
pub async fn update_source_settings(
    state: &Arc<AppState>,
    name: &str,
    hide_nsfw: Option<bool>,
    auto_upscale: Option<bool>,
) -> Result<SourceSettingsResponse, AppError> {
    let span = tracing::Span::current();
    let plugins = state.source_registry.read().await;
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
    if let Some(enabled) = auto_upscale {
        state
            .db
            .set_setting(
                &format!("source.{name}.auto_upscale"),
                if enabled { "true" } else { "false" },
            )
            .await?;
    }
    source_settings_changed(state, name);
    state
        .telemetry
        .metrics
        .record_source_plugin_change(name, "settings_update", "success");
    trace::record_outcome(&span, "success");

    Ok(SourceSettingsResponse {
        auto_upscale: super::settings::source_auto_upscale(&state.db, name).await?,
        name: name.to_owned(),
        hide_nsfw: state.db.get_source_hide_nsfw(name).await?,
    })
}

#[autometrics]
#[tracing::instrument(name = "app.sources.startup.sync_catalog", skip_all, fields(item_count = tracing::field::Empty))]
pub(crate) async fn sync_startup_source_catalog(
    source_registry: &SourceRegistry,
    db: &Database,
) -> anyhow::Result<usize> {
    let source_records = source_registry
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

fn source_enabled_changed(state: &AppState, source: &str, enabled: bool) {
    route_snapshot_invalidation::source_enabled_changed(state, source, enabled);
}

fn source_settings_changed(state: &AppState, source: &str) {
    route_snapshot_invalidation::source_settings_changed(state, source);
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
