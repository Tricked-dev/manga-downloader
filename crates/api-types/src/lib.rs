use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Serialize, Deserialize, Clone, ToSchema, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct MangaResponse {
    pub id: String,
    pub title: String,
    pub cover_url: String,
    pub cover_proxy_url: Option<String>,
    pub cover_fetch_spec: Option<String>,
    pub description: String,
    pub author: String,
    pub genres: Vec<String>,
    pub status: String,
    pub source_base_url: Option<String>,
    pub is_nsfw: bool,
}

#[derive(
    Serialize, Deserialize, Clone, ToSchema, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct LibraryMangaResponse {
    pub id: String,
    pub source: String,
    pub source_base_url: Option<String>,
    pub source_id: String,
    pub title: String,
    pub cover_url: String,
    pub cover_proxy_url: Option<String>,
    pub description: String,
    pub author: String,
    pub genres: Vec<String>,
    pub status: String,
    pub is_nsfw: bool,
    pub language: Option<String>,
    pub comic_info: ComicInfoResponse,
    pub category: String,
    pub auto_download: Option<bool>,
    pub total_chapters: usize,
    pub downloaded_chapters: usize,
    pub last_updated: String,
}

#[derive(
    Serialize, Deserialize, Clone, ToSchema, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ComicInfoResponse {
    pub title: Option<String>,
    pub series: Option<String>,
    pub summary: Option<String>,
    pub writer: Option<String>,
    pub genre: Option<String>,
    pub age_rating: Option<String>,
    pub language_iso: Option<String>,
    pub web: Option<String>,
}

#[derive(
    Serialize, Deserialize, Clone, ToSchema, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct SearchResponse {
    pub mangas: Vec<MangaResponse>,
    pub has_next_page: bool,
}

#[derive(
    Serialize, Deserialize, Clone, ToSchema, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ChapterResponse {
    pub id: String,
    pub title: String,
    pub chapter_number: f64,
    pub date_uploaded: String,
}

#[derive(
    Serialize, Deserialize, Clone, ToSchema, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ApiListResponse<T> {
    pub items: Vec<T>,
}

impl<T> ApiListResponse<T> {
    /// Wraps a collection in the standard list response envelope.
    pub fn new(items: Vec<T>) -> Self {
        Self { items }
    }
}

#[derive(Serialize, ToSchema)]
pub struct CreatedResourceResponse {
    pub id: String,
}

impl CreatedResourceResponse {
    /// Builds a response for a newly created resource id.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[derive(Serialize, ToSchema)]
pub struct DownloadEnqueueResponse {
    pub id: String,
    pub chapter_id: String,
    pub manga_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct DownloadBulkEnqueueResponse {
    pub enqueued: usize,
}

#[derive(Serialize, ToSchema)]
pub struct LibraryUpdateResponse {
    pub new_chapters: usize,
}

#[derive(Serialize, ToSchema)]
pub struct UploadPluginResponse {
    pub filename: String,
    pub source: String,
    pub replaced_existing: bool,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct PluginArtifactResponse {
    pub id: String,
    pub plugin_key: String,
    pub plugin_version: String,
    pub artifact_path: String,
    pub plugin_api_version: u32,
    pub is_active: bool,
    pub installed_at: String,
    pub replaced_at: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct SetSourceEnabledResponse {
    pub ok: bool,
    pub name: String,
    pub enabled: bool,
}

#[derive(Serialize, ToSchema)]
pub struct SourceSettingsResponse {
    pub name: String,
    pub hide_nsfw: bool,
}

#[derive(Serialize, ToSchema)]
pub struct SettingsResponse {
    pub settings: std::collections::HashMap<String, String>,
}

#[derive(Serialize, ToSchema)]
pub struct OperationStatusResponse {
    pub ok: bool,
}

impl OperationStatusResponse {
    /// Builds the standard success response.
    pub const fn ok() -> Self {
        Self { ok: true }
    }
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct ArchiveIndexJobResponse {
    pub running: bool,
    pub trigger: String,
    pub indexed_archives: usize,
    pub skipped_archives: usize,
    pub failed_archives: usize,
    pub removed_rows: usize,
    pub message: String,
}

impl Default for ArchiveIndexJobResponse {
    fn default() -> Self {
        Self {
            running: false,
            trigger: "idle".to_string(),
            indexed_archives: 0,
            skipped_archives: 0,
            failed_archives: 0,
            removed_rows: 0,
            message: "Archive index idle".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct ArchiveIndexStatusResponse {
    pub indexed_archives: usize,
    pub indexed_pages: usize,
    pub index_blob_bytes: usize,
    pub stale_rows: usize,
    pub memory_entries: usize,
    pub job: ArchiveIndexJobResponse,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct ArchiveIndexCleanupResponse {
    pub ok: bool,
    pub removed_rows: usize,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct HealthCheckResponse {
    pub component: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct HealthResponse {
    pub ok: bool,
    pub checks: Vec<HealthCheckResponse>,
}

impl HealthCheckResponse {
    /// Builds a passing component health check with detail text.
    pub fn ok(component: &'static str, detail: impl Into<String>) -> Self {
        Self {
            component: component.to_string(),
            ok: true,
            detail: Some(detail.into()),
        }
    }

    /// Builds a failing component health check with detail text.
    pub fn failed(component: &'static str, detail: impl Into<String>) -> Self {
        Self {
            component: component.to_string(),
            ok: false,
            detail: Some(detail.into()),
        }
    }
}

impl HealthResponse {
    /// Builds an aggregate health response from component checks.
    ///
    /// The aggregate is healthy only when every component check is healthy.
    pub fn from_checks(checks: Vec<HealthCheckResponse>) -> Self {
        let ok = checks.iter().all(|check| check.ok);
        Self { ok, checks }
    }
}

#[derive(Serialize, ToSchema)]
pub struct RefreshLibraryMetadataResponse {
    pub ok: bool,
    pub files_rewritten: usize,
}

impl RefreshLibraryMetadataResponse {
    /// Builds a successful metadata refresh response.
    pub const fn ok(files_rewritten: usize) -> Self {
        Self {
            ok: true,
            files_rewritten,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ReencodeDownloadResponse {
    pub ok: bool,
    pub images_reencoded: usize,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct ServerBuildInfoResponse {
    pub name: String,
    pub version: String,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
    pub commit_short_hash: Option<String>,
    pub commit_date: Option<String>,
    pub build_time: Option<String>,
    pub build_channel: Option<String>,
    pub build_target: Option<String>,
    pub git_clean: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorEnvelopeResponse {
    pub error: ErrorPayloadResponse,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorPayloadResponse {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}
