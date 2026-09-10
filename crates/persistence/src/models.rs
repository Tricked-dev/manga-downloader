#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct MangaRow {
    pub id: String,
    pub source: String,
    pub source_base_url: String,
    pub source_id: String,
    pub title: String,
    pub cover_url: String,
    pub cover_fetch_spec: Option<String>,
    pub description: String,
    pub author: String,
    pub genres: String,
    pub status: String,
    pub category: String,
    pub is_nsfw: bool,
    pub auto_download: Option<bool>,
    pub total_chapters: usize,
    pub downloaded_chapters: usize,
    pub last_updated: String,
    pub chapters_initialized: bool,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct MangaInsert<'a> {
    pub source: &'a str,
    pub source_id: &'a str,
    pub title: &'a str,
    pub cover_url: &'a str,
    pub cover_fetch_spec: Option<&'a str>,
    pub description: &'a str,
    pub author: &'a str,
    pub genres: &'a str,
    pub status: &'a str,
    pub category: &'a str,
    pub is_nsfw: bool,
    pub language: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct ChapterInsert {
    pub source_id: String,
    pub title: String,
    pub chapter_number: f64,
    pub date_uploaded: String,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct ChapterRow {
    pub id: String,
    pub manga_id: String,
    pub source_id: String,
    pub title: String,
    pub chapter_number: f64,
    pub date_uploaded: String,
    pub fetched_at: String,
    pub downloaded: bool,
    pub is_new: bool,
    pub pages_read: usize,
    pub read_completed: bool,
    pub last_read_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct DownloadRow {
    pub upscaled_at: Option<String>,
    pub upscale_model: Option<String>,
    pub upscale_scale: Option<u32>,
    pub id: String,
    pub chapter_id: String,
    pub manga_id: String,
    pub status: String,
    pub progress: f64,
    pub error: Option<String>,
    pub chapter_title: String,
    pub chapter_number: f64,
    pub manga_title: String,
    pub chapter_source_id: String,
    pub manga_source: String,
    pub manga_source_id: String,
}

#[derive(Debug, Clone)]
pub struct DownloadMetricRow {
    pub status: String,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct SourceCountRow {
    pub source: String,
    pub count: u64,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct StatsOverview {
    pub totals: StatsTotals,
    pub cache: StatsCacheSummary,
    pub storage: StatsStorageSummary,
    pub activity: Vec<StatsActivityPoint>,
    pub source_breakdown: Vec<StatsSourceBreakdown>,
    pub recent_reads: Vec<StatsRecentChapter>,
}

#[derive(Debug, Clone, Default, serde::Serialize, utoipa::ToSchema)]
pub struct StatsCacheSummary {
    pub hits: u64,
    pub misses: u64,
    pub requests: u64,
    pub hit_rate: f64,
}

#[derive(Debug, Clone, Default, serde::Serialize, utoipa::ToSchema)]
pub struct StatsStorageSummary {
    pub available: bool,
    pub used_bytes: u64,
    pub limit_bytes: Option<u64>,
    pub usage_rate: f64,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct StatsTotals {
    pub pages_read: usize,
    pub pages_downloaded: usize,
    pub chapters_read: usize,
    pub chapters_downloaded: usize,
    pub library_series: usize,
    pub library_chapters: usize,
    pub active_downloads: usize,
    pub failed_downloads: usize,
    pub plugin_sources: usize,
    pub new_chapters: usize,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct StatsActivityPoint {
    pub day: u64,
    pub chapters_read: usize,
    pub pages_downloaded: usize,
    pub chapters_downloaded: usize,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct StatsSourceBreakdown {
    pub source: String,
    pub series: usize,
    pub chapters: usize,
    pub chapters_read: usize,
    pub chapters_downloaded: usize,
    pub pages_read: usize,
    pub pages_downloaded: usize,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct StatsRecentChapter {
    pub chapter_id: String,
    pub manga_id: String,
    pub manga_title: String,
    pub chapter_title: String,
    pub source: String,
    pub pages_read: usize,
    pub read_completed: bool,
    pub last_read_at: String,
}

#[derive(Debug, Clone)]
pub struct SourceRecordInput {
    pub key: String,
    pub display_name: String,
    pub base_url: String,
    pub version: String,
    pub plugin_api_version: u32,
    pub capabilities: Vec<String>,
    pub enabled: bool,
}
