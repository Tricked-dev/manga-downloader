use std::path::{Path as StdPath, PathBuf};

use anyhow::{Context, Result};
use wasmtime::Cache;

const WASMTIME_CACHE_DIR_NAME: &str = ".wasmtime-cache";
const WASMTIME_CACHE_CONFIG_NAME: &str = "config.toml";

pub(super) async fn load_wasmtime_cache(plugins_dir: &StdPath) -> Result<Cache> {
    let cache_config_path = wasmtime_cache_config_path(plugins_dir);
    let expected_config = wasmtime_cache_config_contents(plugins_dir)?;
    if backend_fs::path_exists(&cache_config_path) {
        let current_config =
            backend_fs::read_to_string_sync(&cache_config_path).with_context(|| {
                format!(
                    "failed to read cache config: {}",
                    cache_config_path.display()
                )
            })?;

        if current_config != expected_config {
            tracing::info!(
                path = %cache_config_path.display(),
                "Resetting Wasmtime Cache Config",
            );
            backend_fs::write_bytes(&cache_config_path, expected_config.as_bytes()).await?;
        }
    } else {
        if let Some(parent) = cache_config_path.parent() {
            backend_fs::create_dir_all(parent).await?;
        }
        backend_fs::write_bytes(&cache_config_path, expected_config.as_bytes()).await?;
    }

    Ok(Cache::from_file(Some(&cache_config_path))?)
}

fn wasmtime_cache_config_path(plugins_dir: &StdPath) -> PathBuf {
    plugins_dir
        .join(WASMTIME_CACHE_DIR_NAME)
        .join(WASMTIME_CACHE_CONFIG_NAME)
}

fn wasmtime_cache_directory(plugins_dir: &StdPath) -> PathBuf {
    plugins_dir.join(WASMTIME_CACHE_DIR_NAME).join("cache")
}

fn wasmtime_cache_config_contents(plugins_dir: &StdPath) -> Result<String> {
    let cache_dir = backend_fs::absolute_path(&wasmtime_cache_directory(plugins_dir))?;
    let cache_dir = cache_dir.to_string_lossy();
    let cache_dir = cache_dir.replace('\\', "\\\\").replace('"', "\\\"");
    Ok(format!("[cache]\ndirectory = \"{cache_dir}\"\n"))
}

pub(super) fn map_wasmtime_err<T>(
    result: std::result::Result<T, wasmtime::Error>,
) -> anyhow::Result<T> {
    result.map_err(Into::into)
}
