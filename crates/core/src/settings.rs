#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKey {
    AuthEnabled,
    AuthOidcIssuerUrl,
    AuthOidcClientId,
    AuthOidcClientSecret,
    AuthOidcScopes,
    AuthOidcProviderId,
    BackendApiKey,
    UpdateIntervalHours,
    AutoDownloadNewChapters,
    AutoDownloadCategory,
    DownloadPath,
    DownloadConcurrentChapters,
    DownloadPageFetchConcurrency,
    MaxDownloadStorageBytes,
    LibraryCategories,
    CacheDiskPath,
    CacheMaxMemoryBytes,
    AvifConversionWorkers,
}

impl SettingKey {
    #[must_use]
    /// Returns the persisted database key for this setting.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AuthEnabled => "auth_enabled",
            Self::AuthOidcIssuerUrl => "auth_oidc_issuer_url",
            Self::AuthOidcClientId => "auth_oidc_client_id",
            Self::AuthOidcClientSecret => "auth_oidc_client_secret",
            Self::AuthOidcScopes => "auth_oidc_scopes",
            Self::AuthOidcProviderId => "auth_oidc_provider_id",
            Self::BackendApiKey => "backend_api_key",
            Self::UpdateIntervalHours => "update_interval_hours",
            Self::AutoDownloadNewChapters => "auto_download_new_chapters",
            Self::AutoDownloadCategory => "auto_download_category",
            Self::DownloadPath => "download_path",
            Self::DownloadConcurrentChapters => "download_concurrent_chapters",
            Self::DownloadPageFetchConcurrency => "download_page_fetch_concurrency",
            Self::MaxDownloadStorageBytes => "max_download_storage_bytes",
            Self::LibraryCategories => "library_categories",
            Self::CacheDiskPath => "cache_disk_path",
            Self::CacheMaxMemoryBytes => "cache_max_memory_bytes",
            Self::AvifConversionWorkers => "avif_conversion_workers",
        }
    }

    #[must_use]
    /// Returns the built-in default value for this setting.
    pub const fn default_value(self) -> &'static str {
        match self {
            Self::AuthEnabled | Self::AutoDownloadNewChapters => "false",
            Self::AuthOidcIssuerUrl
            | Self::AuthOidcClientId
            | Self::AuthOidcClientSecret
            | Self::BackendApiKey
            | Self::AutoDownloadCategory
            | Self::MaxDownloadStorageBytes => "",
            Self::DownloadConcurrentChapters | Self::DownloadPageFetchConcurrency => "2",
            Self::AuthOidcScopes => "openid profile email",
            Self::AuthOidcProviderId => "oidc",
            Self::UpdateIntervalHours => "1",
            Self::DownloadPath => "./data/downloads",
            Self::LibraryCategories => "default,downloaded",
            Self::CacheDiskPath => "./data/cache",
            Self::CacheMaxMemoryBytes => "268435456",
            Self::AvifConversionWorkers => "5",
        }
    }

    #[must_use]
    /// Returns the environment variable that can override this setting.
    pub const fn env_var(self) -> &'static str {
        match self {
            Self::AuthEnabled => "AUTH_ENABLED",
            Self::AuthOidcIssuerUrl => "OIDC_ISSUER_URL",
            Self::AuthOidcClientId => "OIDC_CLIENT_ID",
            Self::AuthOidcClientSecret => "OIDC_CLIENT_SECRET",
            Self::AuthOidcScopes => "OIDC_SCOPES",
            Self::AuthOidcProviderId => "OIDC_PROVIDER_ID",
            Self::BackendApiKey => "BACKEND_API_KEY",
            Self::UpdateIntervalHours => "UPDATE_INTERVAL_HOURS",
            Self::AutoDownloadNewChapters => "AUTO_DOWNLOAD_NEW_CHAPTERS",
            Self::AutoDownloadCategory => "AUTO_DOWNLOAD_CATEGORY",
            Self::DownloadPath => "DOWNLOAD_PATH",
            Self::DownloadConcurrentChapters => "DOWNLOAD_CONCURRENT_CHAPTERS",
            Self::DownloadPageFetchConcurrency => "DOWNLOAD_PAGE_FETCH_CONCURRENCY",
            Self::MaxDownloadStorageBytes => "MAX_DOWNLOAD_STORAGE_BYTES",
            Self::LibraryCategories => "LIBRARY_CATEGORIES",
            Self::CacheDiskPath => "CACHE_DISK_PATH",
            Self::CacheMaxMemoryBytes => "CACHE_MAX_MEMORY_BYTES",
            Self::AvifConversionWorkers => "AVIF_CONVERSION_WORKERS",
        }
    }

    #[must_use]
    /// Looks up a setting key by its persisted database name.
    pub fn from_key(key: &str) -> Option<Self> {
        SETTING_DEFINITIONS
            .iter()
            .find(|definition| definition.key == key)
            .map(|definition| definition.key_enum)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SettingDefinition {
    pub key_enum: SettingKey,
    pub key: &'static str,
    pub default: &'static str,
    pub env_var: &'static str,
}

impl SettingDefinition {
    const fn new(key: SettingKey) -> Self {
        Self {
            key_enum: key,
            key: key.as_str(),
            default: key.default_value(),
            env_var: key.env_var(),
        }
    }
}

pub const SETTING_DEFINITIONS: &[SettingDefinition] = &[
    SettingDefinition::new(SettingKey::AuthEnabled),
    SettingDefinition::new(SettingKey::AuthOidcIssuerUrl),
    SettingDefinition::new(SettingKey::AuthOidcClientId),
    SettingDefinition::new(SettingKey::AuthOidcClientSecret),
    SettingDefinition::new(SettingKey::AuthOidcScopes),
    SettingDefinition::new(SettingKey::AuthOidcProviderId),
    SettingDefinition::new(SettingKey::BackendApiKey),
    SettingDefinition::new(SettingKey::UpdateIntervalHours),
    SettingDefinition::new(SettingKey::AutoDownloadNewChapters),
    SettingDefinition::new(SettingKey::AutoDownloadCategory),
    SettingDefinition::new(SettingKey::DownloadPath),
    SettingDefinition::new(SettingKey::DownloadConcurrentChapters),
    SettingDefinition::new(SettingKey::DownloadPageFetchConcurrency),
    SettingDefinition::new(SettingKey::MaxDownloadStorageBytes),
    SettingDefinition::new(SettingKey::LibraryCategories),
    SettingDefinition::new(SettingKey::CacheDiskPath),
    SettingDefinition::new(SettingKey::CacheMaxMemoryBytes),
    SettingDefinition::new(SettingKey::AvifConversionWorkers),
];

#[must_use]
/// Returns the built-in default value for a persisted setting key.
pub fn setting_default(key: &str) -> Option<&'static str> {
    SettingKey::from_key(key).map(SettingKey::default_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_definitions_match_key_methods() {
        for definition in SETTING_DEFINITIONS {
            assert_eq!(definition.key, definition.key_enum.as_str());
            assert_eq!(definition.default, definition.key_enum.default_value());
            assert_eq!(definition.env_var, definition.key_enum.env_var());
            assert_eq!(
                SettingKey::from_key(definition.key),
                Some(definition.key_enum)
            );
            assert_eq!(
                setting_default(definition.key),
                Some(definition.key_enum.default_value())
            );
        }
    }
}
