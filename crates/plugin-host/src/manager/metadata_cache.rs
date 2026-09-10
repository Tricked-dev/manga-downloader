use std::path::{Path as StdPath, PathBuf};

use anyhow::{Context, Result};

use super::SourceInfo;

#[derive(serde::Serialize, serde::Deserialize)]
struct PluginMetadataCache {
    host_api_version: u32,
    source: SourceInfo,
}

pub(super) async fn read_plugin_metadata_cache(path: &StdPath) -> Result<Option<SourceInfo>> {
    let cache_path = plugin_metadata_cache_path(path);
    if !backend_fs::path_exists(&cache_path) || !plugin_metadata_cache_is_fresh(path, &cache_path)?
    {
        return Ok(None);
    }

    let bytes = match backend_fs::read_bytes(&cache_path).await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::debug!(
                path = %cache_path.display(),
                error = %error,
                "Plugin Metadata Cache Read Failed",
            );
            return Ok(None);
        }
    };

    let cache: PluginMetadataCache = match serde_json::from_slice(&bytes) {
        Ok(cache) => cache,
        Err(error) => {
            tracing::debug!(
                path = %cache_path.display(),
                error = %error,
                "Plugin Metadata Cache Parse Failed",
            );
            return Ok(None);
        }
    };

    if cache.host_api_version != crate::PLUGIN_API_VERSION {
        return Ok(None);
    }

    Ok(Some(cache.source))
}

pub(super) async fn write_plugin_metadata_cache(path: &StdPath, source: &SourceInfo) -> Result<()> {
    let bytes = serde_json::to_vec(&PluginMetadataCache {
        host_api_version: crate::PLUGIN_API_VERSION,
        source: source.clone(),
    })?;
    backend_fs::write_bytes(&plugin_metadata_cache_path(path), &bytes).await
}

fn plugin_metadata_cache_is_fresh(plugin_path: &StdPath, cache_path: &StdPath) -> Result<bool> {
    let plugin_modified = backend_fs::metadata_sync(plugin_path)?
        .modified()
        .context("plugin modified timestamp is unavailable")?;
    let cache_modified = backend_fs::metadata_sync(cache_path)?
        .modified()
        .context("cache modified timestamp is unavailable")?;
    Ok(cache_modified >= plugin_modified)
}

pub(super) fn plugin_metadata_cache_path(path: &StdPath) -> PathBuf {
    let file_name = path.file_name().map_or_else(
        || "plugin.wasm".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    path.with_file_name(format!("{file_name}.metadata.json"))
}
