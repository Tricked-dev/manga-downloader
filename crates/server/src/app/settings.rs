use std::{collections::HashMap, sync::Arc};

use crate::{
    AppState,
    api::{
        dto::{OperationStatusResponse, SettingsResponse},
        error::AppError,
    },
};
use anyhow::{Context as _, Result};
use autometrics::autometrics;
use backend_cache::{MangaCache, parse_max_memory_bytes};
use backend_core::{parse_positive_byte_size, settings::SettingKey};
use backend_persistence::Database;
use secrecy::SecretString;
use tokio::time::Duration;

const DEFAULT_UPDATE_INTERVAL: Duration = Duration::from_hours(1);
const SECONDS_PER_HOUR: f64 = 3600.0;
pub(crate) const DEFAULT_DOWNLOAD_CONCURRENT_CHAPTERS: usize = 2;
pub(crate) const DEFAULT_DOWNLOAD_PAGE_FETCH_CONCURRENCY: usize = 2;
const MAX_DOWNLOAD_CONCURRENCY: usize = 32;

pub(crate) struct SettingsUpdateResult {
    pub(crate) response: OperationStatusResponse,
    pub(crate) changes: SettingsChangeSet,
}

pub(crate) struct SettingsInterface<'a> {
    db: &'a Database,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AutoDownloadSettings {
    pub(crate) enabled: bool,
    pub(crate) category: Option<String>,
}

#[derive(Debug)]
pub(crate) struct SettingsChangeSet {
    pub(crate) updated_keys: Vec<String>,
    changed_families: Vec<SettingsChangeFamily>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsChangeFamily {
    CacheRuntime,
    DownloadPath,
    DownloadRuntime,
    DownloadStoragePolicy,
    LibraryUpdatePolicy,
}

impl SettingsChangeSet {
    fn from_settings(settings: &HashMap<String, String>) -> Self {
        let mut updated_keys = settings.keys().cloned().collect::<Vec<_>>();
        updated_keys.sort_unstable();
        let keys = settings
            .keys()
            .filter_map(|key| SettingKey::from_key(key))
            .collect::<Vec<_>>();
        let has_key = |key| keys.contains(&key);
        let mut changed_families = Vec::new();
        if has_key(SettingKey::CacheDiskPath) || has_key(SettingKey::CacheMaxMemoryBytes) {
            changed_families.push(SettingsChangeFamily::CacheRuntime);
        }
        if has_key(SettingKey::DownloadPath) {
            changed_families.push(SettingsChangeFamily::DownloadPath);
        }
        if has_key(SettingKey::DownloadConcurrentChapters)
            || has_key(SettingKey::DownloadPageFetchConcurrency)
        {
            changed_families.push(SettingsChangeFamily::DownloadRuntime);
        }
        if has_key(SettingKey::DownloadPath) || has_key(SettingKey::MaxDownloadStorageBytes) {
            changed_families.push(SettingsChangeFamily::DownloadStoragePolicy);
        }
        if has_key(SettingKey::UpdateIntervalHours)
            || has_key(SettingKey::AutoDownloadNewChapters)
            || has_key(SettingKey::AutoDownloadCategory)
        {
            changed_families.push(SettingsChangeFamily::LibraryUpdatePolicy);
        }

        Self {
            updated_keys,
            changed_families,
        }
    }

    pub(crate) fn updated_count(&self) -> usize {
        self.updated_keys.len()
    }

    pub(crate) fn contains_family(&self, family: SettingsChangeFamily) -> bool {
        self.changed_families.contains(&family)
    }

    pub(crate) fn download_path_changed(&self) -> bool {
        self.contains_family(SettingsChangeFamily::DownloadPath)
    }
}

pub(crate) fn interface(db: &Database) -> SettingsInterface<'_> {
    SettingsInterface { db }
}

impl SettingsInterface<'_> {
    async fn raw(&self, key: SettingKey) -> Result<Option<String>> {
        self.db.get_setting(key.as_str()).await
    }

    async fn set_many(&self, settings: &HashMap<String, String>) -> Result<()> {
        for (key, value) in settings {
            self.db.set_setting(key, value).await?;
        }
        Ok(())
    }

    async fn string_or_default(&self, key: SettingKey) -> Result<String> {
        Ok(self
            .raw(key)
            .await?
            .unwrap_or_else(|| key.default_value().to_string()))
    }

    async fn non_empty(&self, key: SettingKey) -> Result<Option<String>> {
        Ok(self
            .raw(key)
            .await?
            .filter(|value| !value.trim().is_empty()))
    }

    async fn non_empty_secret(&self, key: SettingKey) -> Result<Option<SecretString>> {
        Ok(self.non_empty(key).await?.map(SecretString::from))
    }

    pub(crate) async fn download_path(&self) -> Result<String> {
        self.string_or_default(SettingKey::DownloadPath).await
    }

    pub(crate) async fn cache_disk_path(&self) -> Result<String> {
        self.string_or_default(SettingKey::CacheDiskPath).await
    }

    pub(crate) async fn cache_max_memory_bytes(&self) -> Result<usize> {
        Ok(parse_max_memory_bytes(
            self.raw(SettingKey::CacheMaxMemoryBytes).await?.as_deref(),
        ))
    }

    pub(crate) async fn backend_api_key(
        &self,
        fallback_api_key: Option<SecretString>,
    ) -> Result<Option<SecretString>> {
        Ok(self
            .non_empty_secret(SettingKey::BackendApiKey)
            .await?
            .or(fallback_api_key))
    }

    pub(crate) async fn library_update_interval(&self) -> Duration {
        match self.raw(SettingKey::UpdateIntervalHours).await {
            Ok(Some(value)) => {
                update_interval_from_hours(&value).unwrap_or(DEFAULT_UPDATE_INTERVAL)
            }
            Ok(None) => DEFAULT_UPDATE_INTERVAL,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "Library Update Interval Setting Read Failed",
                );
                DEFAULT_UPDATE_INTERVAL
            }
        }
    }

    pub(crate) async fn auto_download(&self) -> Result<AutoDownloadSettings> {
        let enabled = self
            .raw(SettingKey::AutoDownloadNewChapters)
            .await?
            .is_some_and(|value| value == "true");
        let category = self
            .raw(SettingKey::AutoDownloadCategory)
            .await?
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        Ok(AutoDownloadSettings { enabled, category })
    }

    pub(crate) async fn download_concurrent_chapters(&self) -> Result<usize> {
        self.bounded_positive_usize(
            SettingKey::DownloadConcurrentChapters,
            DEFAULT_DOWNLOAD_CONCURRENT_CHAPTERS,
            MAX_DOWNLOAD_CONCURRENCY,
        )
        .await
    }

    pub(crate) async fn download_page_fetch_concurrency(&self) -> Result<usize> {
        self.bounded_positive_usize(
            SettingKey::DownloadPageFetchConcurrency,
            DEFAULT_DOWNLOAD_PAGE_FETCH_CONCURRENCY,
            MAX_DOWNLOAD_CONCURRENCY,
        )
        .await
    }

    async fn bounded_positive_usize(
        &self,
        key: SettingKey,
        default: usize,
        max: usize,
    ) -> Result<usize> {
        Ok(self
            .raw(key)
            .await?
            .as_deref()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(default)
            .min(max.max(1)))
    }

    pub(crate) async fn max_download_storage_bytes(&self) -> Result<Option<u64>> {
        let Some(value) = self.raw(SettingKey::MaxDownloadStorageBytes).await? else {
            return Ok(None);
        };

        parse_storage_limit_bytes(&value)
    }
}

#[autometrics]
pub async fn get(state: &Arc<AppState>) -> Result<SettingsResponse, AppError> {
    Ok(SettingsResponse {
        settings: state.db.get_all_settings().await?,
    })
}

#[autometrics]
pub async fn update(
    state: &Arc<AppState>,
    settings: &HashMap<String, String>,
) -> Result<SettingsUpdateResult, AppError> {
    let changes = SettingsChangeSet::from_settings(settings);
    interface(&state.db).set_many(settings).await?;

    crate::app::route_snapshot_invalidation::settings_changed(state);

    if changes.download_path_changed() {
        crate::downloader::invalidate_download_storage_usage(state).await;
    }

    Ok(SettingsUpdateResult {
        response: OperationStatusResponse::ok(),
        changes,
    })
}

#[autometrics(track_concurrency)]
pub async fn clear_cache(cache: &MangaCache) -> Result<OperationStatusResponse, AppError> {
    cache.clear().await?;
    Ok(OperationStatusResponse::ok())
}

pub(crate) async fn download_path(db: &Database) -> Result<String> {
    interface(db).download_path().await
}

pub(crate) async fn library_update_interval(db: &Database) -> Duration {
    interface(db).library_update_interval().await
}

pub(crate) async fn auto_download(db: &Database) -> Result<AutoDownloadSettings> {
    interface(db).auto_download().await
}

pub(crate) async fn download_concurrent_chapters(db: &Database) -> Result<usize> {
    interface(db).download_concurrent_chapters().await
}

pub(crate) async fn download_page_fetch_concurrency(db: &Database) -> Result<usize> {
    interface(db).download_page_fetch_concurrency().await
}

pub(crate) async fn max_download_storage_bytes(db: &Database) -> Result<Option<u64>> {
    interface(db).max_download_storage_bytes().await
}

fn update_interval_from_hours(value: &str) -> Option<Duration> {
    let interval_hours = value.parse::<f64>().ok()?;
    if !interval_hours.is_finite() || interval_hours < 0.0 {
        return None;
    }

    Some(
        Duration::try_from_secs_f64((interval_hours * SECONDS_PER_HOUR).round())
            .unwrap_or(Duration::MAX),
    )
}

fn parse_storage_limit_bytes(value: &str) -> Result<Option<u64>> {
    parse_positive_byte_size(value).with_context(|| {
        let normalized = value.trim();
        format!(
            "Invalid max_download_storage_bytes value: {normalized}. Use bytes or a KiB/MiB/GiB value."
        )
    })
}

/// A source can disable automatic upscaling while retaining the global default.
pub(crate) async fn source_auto_upscale(db: &Database, source: &str) -> Result<bool> {
    Ok(db
        .get_setting(&format!("source.{source}.auto_upscale"))
        .await?
        .is_none_or(|value| value == "true"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_change_set_classifies_known_setting_families() {
        let changes = SettingsChangeSet::from_settings(&HashMap::from([
            (
                SettingKey::DownloadPath.as_str().to_string(),
                "/tmp/downloads".to_string(),
            ),
            (
                SettingKey::AutoDownloadCategory.as_str().to_string(),
                "tracked".to_string(),
            ),
        ]));

        assert!(changes.download_path_changed());
        assert!(!changes.contains_family(SettingsChangeFamily::DownloadRuntime));
        assert!(changes.contains_family(SettingsChangeFamily::DownloadStoragePolicy));
        assert!(changes.contains_family(SettingsChangeFamily::LibraryUpdatePolicy));
        assert!(!changes.contains_family(SettingsChangeFamily::CacheRuntime));
        assert_eq!(changes.updated_count(), 2);
    }

    #[test]
    fn settings_change_set_classifies_download_runtime_settings() {
        let changes = SettingsChangeSet::from_settings(&HashMap::from([
            (
                SettingKey::DownloadConcurrentChapters.as_str().to_string(),
                "4".to_string(),
            ),
            (
                SettingKey::DownloadPageFetchConcurrency
                    .as_str()
                    .to_string(),
                "8".to_string(),
            ),
        ]));

        assert!(changes.contains_family(SettingsChangeFamily::DownloadRuntime));
        assert_eq!(changes.updated_count(), 2);
    }

    #[test]
    fn storage_limit_parser_treats_blank_and_zero_as_unlimited() {
        assert_eq!(parse_storage_limit_bytes("").unwrap(), None);
        assert_eq!(parse_storage_limit_bytes("0").unwrap(), None);
        assert_eq!(parse_storage_limit_bytes("1 KiB").unwrap(), Some(1024));
    }
}
