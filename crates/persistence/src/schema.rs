#[derive(Clone, Debug, toasty::Model)]
#[table = "app_settings"]
pub(crate) struct AppSetting {
    #[key]
    pub(crate) key: String,
    pub(crate) value: String,
}

#[derive(Clone, Debug, toasty::Model)]
#[table = "sources"]
pub(crate) struct Source {
    #[key]
    pub(crate) key: String,
    pub(crate) display_name: String,
    pub(crate) base_url: String,
    pub(crate) version: String,
    pub(crate) enabled: bool,
    pub(crate) capabilities: toasty::Json<Vec<String>>,
    pub(crate) plugin_api_version: i64,
    pub(crate) hide_nsfw: bool,
    #[default(crate::now_timestamp())]
    pub(crate) installed_at: String,
    #[update(crate::now_timestamp())]
    pub(crate) updated_at: String,
}

#[derive(Clone, Debug, toasty::Model)]
#[table = "library_series"]
#[index(source_key, remote_series_id)]
pub(crate) struct LibrarySeries {
    #[key]
    #[default(crate::new_id())]
    pub(crate) id: String,
    #[index]
    pub(crate) source_key: String,
    #[index]
    pub(crate) remote_series_id: String,
    pub(crate) title: String,
    pub(crate) cover_url: String,
    pub(crate) cover_fetch_spec: Option<String>,
    pub(crate) description: String,
    pub(crate) author: String,
    pub(crate) genres: toasty::Json<Vec<String>>,
    pub(crate) status: String,
    pub(crate) category: String,
    pub(crate) is_nsfw: bool,
    pub(crate) auto_download_new: Option<bool>,
    pub(crate) language: Option<String>,
    pub(crate) chapters_initialized: bool,
    #[default(crate::now_timestamp())]
    pub(crate) created_at: String,
    #[update(crate::now_timestamp())]
    pub(crate) updated_at: String,
}

#[derive(Clone, Debug, toasty::Model)]
#[allow(
    clippy::duplicated_attributes,
    reason = "Toasty composite indexes intentionally share columns"
)]
#[table = "chapters"]
#[index(series_id, number, published_at)]
#[index(series_id, remote_chapter_id)]
#[index(published_at, number, fetched_at)]
pub(crate) struct Chapter {
    #[key]
    #[default(crate::new_id())]
    pub(crate) id: String,
    #[index]
    pub(crate) series_id: String,
    #[index]
    pub(crate) remote_chapter_id: String,
    pub(crate) title: String,
    pub(crate) number: i64,
    pub(crate) published_at: String,
    pub(crate) is_new: bool,
    pub(crate) pages_read: i64,
    pub(crate) read_completed: bool,
    pub(crate) last_read_at: Option<String>,
    #[default(crate::now_timestamp())]
    pub(crate) fetched_at: String,
}

#[derive(Clone, Debug, toasty::Model)]
#[allow(
    clippy::duplicated_attributes,
    reason = "Toasty composite indexes intentionally share columns"
)]
#[table = "downloads"]
#[index(status, queued_at)]
#[index(chapter_id, status, queued_at)]
#[index(series_id, status)]
pub(crate) struct Download {
    #[key]
    #[default(crate::new_id())]
    pub(crate) id: String,
    #[index]
    pub(crate) series_id: String,
    #[index]
    pub(crate) chapter_id: String,
    #[index]
    pub(crate) status: String,
    pub(crate) progress_percent: i64,
    pub(crate) stage: String,
    pub(crate) error_code: Option<String>,
    pub(crate) error_message: Option<String>,
    pub(crate) upscaled_at: Option<String>,
    pub(crate) upscale_model: Option<String>,
    pub(crate) upscale_scale: Option<i64>,
    pub(crate) file_path: Option<String>,
    pub(crate) file_size_bytes: Option<i64>,
    pub(crate) page_count: i64,
    pub(crate) attempt_count: i64,
    #[index]
    #[default(crate::now_timestamp())]
    pub(crate) queued_at: String,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
}

#[derive(Clone, Debug, toasty::Model)]
#[table = "stats_events"]
#[index(created_at, kind)]
pub(crate) struct StatsEvent {
    #[key]
    #[default(crate::new_id())]
    pub(crate) id: String,
    #[index]
    pub(crate) kind: String,
    #[index]
    pub(crate) source: Option<String>,
    #[index]
    pub(crate) series_id: Option<String>,
    #[index]
    pub(crate) chapter_id: Option<String>,
    pub(crate) amount: i64,
    #[index]
    #[default(crate::now_timestamp())]
    pub(crate) created_at: String,
}

#[derive(Clone, Debug, toasty::Model)]
#[table = "download_events"]
#[index(download_id, created_at)]
pub(crate) struct DownloadEvent {
    #[key]
    #[default(crate::new_id())]
    pub(crate) id: String,
    #[index]
    pub(crate) download_id: String,
    pub(crate) event_type: String,
    pub(crate) message: String,
    pub(crate) payload_json: Option<String>,
    #[default(crate::now_timestamp())]
    pub(crate) created_at: String,
}

#[derive(Clone, Debug, toasty::Model)]
#[table = "background_jobs"]
pub(crate) struct BackgroundJob {
    #[key]
    #[default(crate::new_id())]
    pub(crate) id: String,
    #[index]
    pub(crate) queue: String,
    pub(crate) payload_json: String,
    #[index]
    pub(crate) status: String,
    pub(crate) attempt_count: i64,
    pub(crate) max_attempts: i64,
    #[index]
    pub(crate) run_after: String,
    pub(crate) locked_by: Option<String>,
    pub(crate) locked_at: Option<String>,
    pub(crate) last_error: Option<String>,
    #[default(crate::now_timestamp())]
    pub(crate) created_at: String,
    #[update(crate::now_timestamp())]
    pub(crate) updated_at: String,
}

#[derive(Clone, Debug, toasty::Model)]
#[table = "auth_sessions"]
pub(crate) struct AuthSession {
    #[key]
    pub(crate) token_hash: String,
    pub(crate) config_hash: String,
    pub(crate) subject: String,
    pub(crate) name: Option<String>,
    pub(crate) email: Option<String>,
    #[index]
    pub(crate) expires_at: String,
}

#[derive(Clone, Debug, toasty::Model)]
#[table = "public_shares"]
pub(crate) struct PublicShare {
    #[key]
    #[default(crate::new_id())]
    pub(crate) id: String,
    #[unique]
    pub(crate) series_id: String,
    #[default(crate::now_timestamp())]
    pub(crate) created_at: String,
}

#[derive(Clone, Debug, toasty::Model)]
#[table = "upscale_progress"]
pub(crate) struct UpscaleProgress {
    #[key]
    pub(crate) download_id: String,
    pub(crate) status: String,
    pub(crate) completed_pages: i64,
    pub(crate) total_pages: i64,
    pub(crate) message: String,
    pub(crate) updated_at: String,
}
