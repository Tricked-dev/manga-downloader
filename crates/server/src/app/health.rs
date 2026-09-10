use std::path::Path;

use backend_cache::{CacheKey, MangaCache};
use uuid::Uuid;

use crate::{
    AppState,
    api::dto::{HealthCheckResponse, HealthResponse},
    server::ShutdownDrain,
};

const DEFAULT_DOWNLOAD_PATH: &str = "./data/downloads";

pub async fn readiness(state: &AppState) -> HealthResponse {
    HealthResponse::from_checks(vec![
        process_readiness_check(&state.shutdown_drain),
        report_download_state(state).await,
    ])
}

pub async fn diagnostics(state: &AppState) -> HealthResponse {
    let mut checks = Vec::new();
    let settings = match state.db.get_all_settings().await {
        Ok(settings) => {
            checks.push(HealthCheckResponse::ok(
                "database",
                "database settings query succeeded",
            ));
            Some(settings)
        }
        Err(error) => {
            tracing::warn!(error = %error, "Health Check Database Probe Failed");
            checks.push(HealthCheckResponse::failed(
                "database",
                "database settings query failed",
            ));
            None
        }
    };

    let plugin_check = {
        let plugin_manager = state.plugin_manager.read().await;
        match plugin_manager.health_check() {
            Ok(health) => HealthCheckResponse::ok(
                "plugins",
                format!(
                    "{} enabled sources loaded ({} total)",
                    health.enabled_sources, health.loaded_sources
                ),
            ),
            Err(error) => {
                tracing::warn!(error = %error, "Health Check Plugin Probe Failed");
                HealthCheckResponse::failed("plugins", "plugin runtime validation failed")
            }
        }
    };
    checks.push(plugin_check);

    checks.push(probe_cache(&state.cache).await);
    checks.push(report_download_state(state).await);
    checks.push(probe_writable_directory("temporary_storage", &std::env::temp_dir()).await);
    checks.push(probe_writable_directory("cache_storage", &state.config.cache_disk_path).await);
    checks.push(probe_database_path(&state.config.db_path).await);

    if let Some(settings) = settings {
        let download_path = settings
            .get("download_path")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(DEFAULT_DOWNLOAD_PATH);
        checks.push(probe_writable_directory("download_storage", Path::new(download_path)).await);
    }

    HealthResponse::from_checks(checks)
}

async fn probe_cache(cache: &MangaCache) -> HealthCheckResponse {
    let key = CacheKey::Details {
        plugin: "__healthcheck__".to_string(),
        id: Uuid::new_v4().to_string(),
    };
    let value = "ok".to_string();

    cache.insert_typed(key.clone(), &value);
    let result = cache.get_typed::<String>(&key).await;
    cache.remove(&key);

    match result {
        Some(read_value) if read_value == value => {
            HealthCheckResponse::ok("cache", "cache read/write probe succeeded")
        }
        Some(_) => {
            tracing::warn!("Health Check Cache Probe Returned Unexpected Value");
            HealthCheckResponse::failed("cache", "cache read/write probe returned unexpected value")
        }
        None => {
            tracing::warn!("Health Check Cache Probe Failed");
            HealthCheckResponse::failed("cache", "cache read/write probe failed")
        }
    }
}

async fn report_download_state(state: &AppState) -> HealthCheckResponse {
    let active_downloads = state.active_download_cancellations.lock().await.len();
    let storage_usage = state.download_storage_usage.lock().await;
    let storage_detail = match storage_usage.refreshed_at {
        Some(refreshed_at) => format!(
            "cached download storage usage is {} bytes ({}s old)",
            storage_usage.bytes,
            refreshed_at.elapsed().as_secs()
        ),
        None => "download storage usage has not been measured yet".to_string(),
    };

    HealthCheckResponse::ok(
        "downloads",
        format!("{active_downloads} active downloads tracked; {storage_detail}"),
    )
}



fn process_readiness_check(shutdown_drain: &ShutdownDrain) -> HealthCheckResponse {
    if shutdown_drain.is_draining() {
        return HealthCheckResponse::failed(
            "process",
            "server is draining before graceful shutdown",
        );
    }

    HealthCheckResponse::ok("process", "server task is accepting requests")
}

async fn probe_database_path(path: &Path) -> HealthCheckResponse {
    match backend_fs::metadata(path).await {
        Ok(metadata) if metadata.is_file() => {
            HealthCheckResponse::ok("database_storage", "database file is present")
        }
        Ok(_) => {
            tracing::warn!(path = %path.display(), "Health Check Database Path Is Not A File");
            HealthCheckResponse::failed("database_storage", "database path is not a file")
        }
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "Health Check Database Path Probe Failed",
            );
            HealthCheckResponse::failed("database_storage", "database file is missing")
        }
    }
}

async fn probe_writable_directory(component: &'static str, path: &Path) -> HealthCheckResponse {
    match backend_fs::metadata(path).await {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            tracing::warn!(
                component,
                path = %path.display(),
                "Health Check Path Is Not A Directory",
            );
            return HealthCheckResponse::failed(component, "path is not a directory");
        }
        Err(error) => {
            tracing::warn!(
                component,
                path = %path.display(),
                error = %error,
                "Health Check Directory Probe Failed",
            );
            return HealthCheckResponse::failed(component, "directory is unavailable");
        }
    }

    let probe_path = path.join(format!(".healthcheck-{}", Uuid::new_v4()));
    if let Err(error) = backend_fs::write_bytes(&probe_path, b"ok").await {
        tracing::warn!(
            component,
            path = %path.display(),
            error = %error,
            "Health Check Directory Write Probe Failed",
        );
        return HealthCheckResponse::failed(component, "directory is not writable");
    }

    if let Err(error) = backend_fs::remove_file(&probe_path).await {
        tracing::warn!(
            component,
            path = %path.display(),
            error = %error,
            "Health Check Directory Cleanup Failed",
        );
        return HealthCheckResponse::failed(component, "directory probe cleanup failed");
    }

    HealthCheckResponse::ok(component, "directory is writable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_readiness_reports_draining_shutdown() {
        let shutdown_drain = ShutdownDrain::default();
        assert!(process_readiness_check(&shutdown_drain).ok);

        shutdown_drain.begin();

        let check = process_readiness_check(&shutdown_drain);
        assert!(!check.ok);
        assert_eq!(check.component, "process");
        assert_eq!(
            check.detail.as_deref(),
            Some("server is draining before graceful shutdown")
        );
    }
}
