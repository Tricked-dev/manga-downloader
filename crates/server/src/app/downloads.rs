use std::{cmp::Ordering, collections::HashSet, path::PathBuf, sync::Arc};

use crate::{
    AppState,
    api::{
        dto::{ApiListResponse, DownloadEnqueueResponse, OperationStatusResponse},
        error::AppError,
    },
    app::{
        download_work_state::{
            DownloadWorkStateProgression, external_status_is_completed, external_status_is_failed,
            external_status_requires_cancellation,
        },
        downloaded_archive_lifecycle, downloaded_archive_resolution, route_snapshot_invalidation,
        source_chapter_sync,
    },
};
use autometrics::autometrics;
use backend_persistence::{Database, DownloadRow};
use backend_sources::SourceRegistry;
use tokio::sync::RwLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DownloadEnqueueOrigin {
    LibraryUpdate,
    Manual,
}

impl DownloadEnqueueOrigin {
    const fn metric_label(self) -> &'static str {
        match self {
            Self::LibraryUpdate => "library_update",
            Self::Manual => "manual",
        }
    }
}

pub struct EnqueueDownloadResult {
    pub response: DownloadEnqueueResponse,
    pub manga_source: String,
    pub manga_title: String,
}

pub(crate) struct EnqueueLocalLibraryChaptersResult {
    pub(crate) enqueued: usize,
}

pub struct DeleteDownloadPreview {
    pub download: DownloadRow,
    pub download_path: String,
    pub archive_path: PathBuf,
    pub requires_cancellation: bool,
}

pub struct DeleteDownloadResult {
    pub id: String,
    pub chapter_id: String,
    pub download_path: String,
    pub archive_path: PathBuf,
    pub removed_size_bytes: u64,
}

pub struct ClearFailedDownloadsResult {
    pub cleared: usize,
    pub download_path: String,
    pub removed_size_bytes: u64,
}

pub struct DownloadArchive {
    pub body: axum::body::Bytes,
    pub filename: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadRemovalOutcome {
    CancellationRequested,
    IncompleteDeleted,
    DownloadedArchiveDeleted,
}

impl DownloadRemovalOutcome {
    pub const fn is_async_completion(self) -> bool {
        matches!(self, Self::CancellationRequested)
    }
}

#[autometrics]
pub async fn enqueue_manual(
    state: &Arc<AppState>,
    chapter_ref: &str,
    manga_ref: &str,
) -> Result<EnqueueDownloadResult, AppError> {
    let result = enqueue(&state.db, &state.source_registry, chapter_ref, manga_ref).await?;
    finish_download_enqueue(state, &result.manga_source, DownloadEnqueueOrigin::Manual);
    Ok(result)
}

#[autometrics]
pub(crate) async fn enqueue_local_library_chapters(
    state: &Arc<AppState>,
    manga: &backend_persistence::MangaRow,
    chapter_ids: &[String],
    origin: DownloadEnqueueOrigin,
) -> Result<EnqueueLocalLibraryChaptersResult, AppError> {
    if chapter_ids.is_empty() {
        return Ok(EnqueueLocalLibraryChaptersResult { enqueued: 0 });
    }

    let ordered_chapter_ids = order_chapter_ids_for_chronological_download(
        state.db.get_chapters(&manga.id).await?,
        chapter_ids,
    );
    let enqueued = state
        .db
        .enqueue_downloads(&manga.id, &ordered_chapter_ids)
        .await?
        .len();
    finish_download_enqueue_batch(state, &manga.source, origin, enqueued);

    Ok(EnqueueLocalLibraryChaptersResult { enqueued })
}

fn order_chapter_ids_for_chronological_download(
    chapters: impl IntoIterator<Item = backend_persistence::ChapterRow>,
    chapter_ids: &[String],
) -> Vec<String> {
    let requested_ids = chapter_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut ordered_chapters = chapters
        .into_iter()
        .filter(|chapter| requested_ids.contains(chapter.id.as_str()))
        .collect::<Vec<_>>();
    ordered_chapters.sort_by(compare_chapter_rows_chronologically);

    let mut found_ids = HashSet::with_capacity(ordered_chapters.len());
    let mut ordered_ids = Vec::with_capacity(chapter_ids.len());
    for chapter in ordered_chapters {
        found_ids.insert(chapter.id.clone());
        ordered_ids.push(chapter.id);
    }

    ordered_ids.extend(
        chapter_ids
            .iter()
            .filter(|chapter_id| !found_ids.contains(chapter_id.as_str()))
            .cloned(),
    );
    ordered_ids
}

fn compare_chapter_rows_chronologically(
    left: &backend_persistence::ChapterRow,
    right: &backend_persistence::ChapterRow,
) -> Ordering {
    left.chapter_number
        .total_cmp(&right.chapter_number)
        .then_with(|| left.date_uploaded.cmp(&right.date_uploaded))
        .then_with(|| left.fetched_at.cmp(&right.fetched_at))
        .then_with(|| left.id.cmp(&right.id))
}

fn finish_download_enqueue(
    state: &Arc<AppState>,
    manga_source: &str,
    origin: DownloadEnqueueOrigin,
) {
    finish_download_enqueue_batch(state, manga_source, origin, 1);
}

fn finish_download_enqueue_batch(
    state: &Arc<AppState>,
    manga_source: &str,
    origin: DownloadEnqueueOrigin,
    enqueued: usize,
) {
    if enqueued == 0 {
        return;
    }

    state.download_queue_notify.notify_waiters();
    for _ in 0..enqueued {
        state
            .telemetry
            .metrics
            .record_download_enqueued(manga_source, origin.metric_label());
    }
    route_snapshot_invalidation::download_list_changed(state);
}

#[autometrics]
pub async fn request_active_cancellation(
    state: &Arc<AppState>,
    download: &DownloadRow,
) -> Result<(), AppError> {
    crate::downloader::request_download_cancel(state, &download.id).await;
    DownloadWorkStateProgression::new(&state.db, &download.id)
        .cancel_requested(download.progress)
        .await?;
    route_snapshot_invalidation::download_list_changed(state);
    Ok(())
}

#[autometrics(track_concurrency)]
pub async fn delete_incomplete(
    state: &Arc<AppState>,
    preview: DeleteDownloadPreview,
) -> Result<DeleteDownloadResult, AppError> {
    let deleted = delete(&state.db, preview).await?;
    route_snapshot_invalidation::download_list_changed(state);
    crate::downloader::update_download_storage_usage_delta(
        state,
        &deleted.download_path,
        crate::downloader::negative_storage_delta(deleted.removed_size_bytes),
    )
    .await;
    Ok(deleted)
}

#[autometrics(track_concurrency)]
pub async fn remove_or_cancel(
    state: &Arc<AppState>,
    id: &str,
) -> Result<DownloadRemovalOutcome, AppError> {
    let preview = prepare_delete(&state.db, id).await?;

    if preview.requires_cancellation {
        request_active_cancellation(state, &preview.download).await?;
        tracing::info!(
            download_id = %preview.download.id,
            chapter_id = %preview.download.chapter_id,
            status = %preview.download.status,
            progress = preview.download.progress,
            "Download Cancel Requested",
        );
        return Ok(DownloadRemovalOutcome::CancellationRequested);
    }

    if external_status_is_completed(&preview.download.status) {
        downloaded_archive_lifecycle::downloaded_archive_deleted(state, &preview.download.id)
            .await?;
        return Ok(DownloadRemovalOutcome::DownloadedArchiveDeleted);
    }

    let deleted = delete_incomplete(state, preview).await?;

    tracing::info!(
        download_id = %deleted.id,
        chapter_id = %deleted.chapter_id,
        archive_bytes = deleted.removed_size_bytes,
        path = %deleted.archive_path.display(),
        was_active = false,
        "Download Deleted",
    );

    Ok(DownloadRemovalOutcome::IncompleteDeleted)
}

#[autometrics(track_concurrency)]
pub async fn clear_failed_work_records(
    state: &Arc<AppState>,
) -> Result<ClearFailedDownloadsResult, AppError> {
    let result = clear_failed(&state.db).await?;
    route_snapshot_invalidation::download_list_changed(state);
    crate::downloader::update_download_storage_usage_delta(
        state,
        &result.download_path,
        crate::downloader::negative_storage_delta(result.removed_size_bytes),
    )
    .await;
    Ok(result)
}

#[autometrics]
pub async fn retry_work_record(
    state: &Arc<AppState>,
    id: &str,
) -> Result<OperationStatusResponse, AppError> {
    let response = retry(&state.db, id).await?;
    state.download_queue_notify.notify_waiters();
    route_snapshot_invalidation::download_list_changed(state);
    Ok(response)
}

#[autometrics]
pub async fn enqueue(
    db: &Database,
    source_registry: &RwLock<SourceRegistry>,
    chapter_ref: &str,
    manga_ref: &str,
) -> Result<EnqueueDownloadResult, AppError> {
    let manga = if let Some(manga) = db.get_manga_by_id(manga_ref).await? {
        manga
    } else if let Some(manga) = db.get_manga_by_source_id(manga_ref).await? {
        manga
    } else {
        return Err(AppError::not_found(anyhow::anyhow!(
            "Manga not found in library"
        )));
    };

    let chapter =
        source_chapter_sync::ensure_local_library_chapter(db, source_registry, &manga, chapter_ref)
            .await?;
    let chapter_id = chapter.id;

    let id = db.enqueue_download(&chapter_id, &manga.id).await?;
    Ok(EnqueueDownloadResult {
        response: DownloadEnqueueResponse {
            id,
            chapter_id,
            manga_id: manga.id,
        },
        manga_source: manga.source,
        manga_title: manga.title,
    })
}

#[autometrics]
pub(crate) async fn enqueue_library_chapters(
    state: &Arc<AppState>,
    manga_id: &str,
    chapter_ids: &[String],
) -> Result<EnqueueLocalLibraryChaptersResult, AppError> {
    let manga = state
        .db
        .get_manga_by_id(manga_id)
        .await?
        .ok_or_else(|| AppError::not_found(anyhow::anyhow!("Manga not found in library")))?;
    enqueue_local_library_chapters(state, &manga, chapter_ids, DownloadEnqueueOrigin::Manual).await
}

#[autometrics]
pub async fn list(
    db: &Database,
    manga_id: Option<&str>,
) -> Result<ApiListResponse<DownloadRow>, AppError> {
    let downloads = match manga_id {
        Some(manga_id) => db.get_downloads_for_manga(manga_id).await?,
        None => db.get_downloads().await?,
    };
    Ok(ApiListResponse::new(downloads))
}

#[autometrics]
pub async fn prepare_delete(db: &Database, id: &str) -> Result<DeleteDownloadPreview, AppError> {
    let archive_resolver = downloaded_archive_resolution::path_resolver(db).await?;
    let download = downloaded_archive_resolution::require_download(db, id).await?;
    let archive_path = archive_resolver.archive_path_for_download(&download);

    Ok(DeleteDownloadPreview {
        requires_cancellation: download_status_requires_cancellation(&download.status),
        download,
        download_path: archive_resolver.download_path().to_string(),
        archive_path,
    })
}

#[autometrics(track_concurrency)]
pub async fn delete(
    db: &Database,
    preview: DeleteDownloadPreview,
) -> Result<DeleteDownloadResult, AppError> {
    let removed_size_bytes = backend_fs::remove_file_if_present(&preview.archive_path).await?;

    db.delete_download_and_reset_chapter(&preview.download.id, &preview.download.chapter_id)
        .await?;

    Ok(DeleteDownloadResult {
        id: preview.download.id,
        chapter_id: preview.download.chapter_id,
        download_path: preview.download_path,
        archive_path: preview.archive_path,
        removed_size_bytes,
    })
}

#[autometrics(track_concurrency)]
pub async fn clear_failed(db: &Database) -> Result<ClearFailedDownloadsResult, AppError> {
    let archive_resolver = downloaded_archive_resolution::path_resolver(db).await?;
    let failed_downloads: Vec<_> = db
        .get_downloads()
        .await?
        .into_iter()
        .filter(|download| external_status_is_failed(&download.status))
        .collect();

    let cleared = failed_downloads.len();
    let mut removed_size_bytes = 0;

    for download in failed_downloads {
        let archive_path = archive_resolver.archive_path_for_download(&download);
        removed_size_bytes += backend_fs::remove_file_if_present(&archive_path).await?;

        db.delete_download_and_reset_chapter(&download.id, &download.chapter_id)
            .await?;
    }

    Ok(ClearFailedDownloadsResult {
        cleared,
        download_path: archive_resolver.download_path().to_string(),
        removed_size_bytes,
    })
}

#[autometrics]
pub async fn retry(db: &Database, id: &str) -> Result<OperationStatusResponse, AppError> {
    db.retry_download(id).await?;
    Ok(OperationStatusResponse::ok())
}

#[autometrics]
pub async fn open_archive(db: &Database, id: &str) -> Result<DownloadArchive, AppError> {
    let archive = downloaded_archive_resolution::require_existing_completed_archive(db, id).await?;

    Ok(DownloadArchive {
        body: backend_storage::read_archive(archive.archive_path.clone()).await?,
        filename: backend_core::download_archive_filename(archive.download.chapter_number),
    })
}

fn download_status_requires_cancellation(status: &str) -> bool {
    external_status_requires_cancellation(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter_row(
        id: &str,
        chapter_number: f64,
        date_uploaded: &str,
    ) -> backend_persistence::ChapterRow {
        backend_persistence::ChapterRow {
            id: id.to_string(),
            manga_id: "manga".to_string(),
            source_id: format!("remote-{id}"),
            title: id.to_string(),
            chapter_number,
            date_uploaded: date_uploaded.to_string(),
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
            downloaded: false,
            is_new: false,
            pages_read: 0,
            read_completed: false,
            last_read_at: None,
        }
    }

    #[test]
    fn queued_download_delete_does_not_request_cancellation() {
        assert!(!download_status_requires_cancellation("queued"));
        assert!(download_status_requires_cancellation("fetch"));
        assert!(download_status_requires_cancellation("conversion"));
        assert!(download_status_requires_cancellation("archive"));
        assert!(download_status_requires_cancellation("canceling"));
    }

    #[test]
    fn download_enqueue_origins_map_to_metric_labels() {
        assert_eq!(DownloadEnqueueOrigin::Manual.metric_label(), "manual");
        assert_eq!(
            DownloadEnqueueOrigin::LibraryUpdate.metric_label(),
            "library_update"
        );
    }

    #[test]
    fn bulk_chapter_enqueue_order_is_chronological() {
        let requested = vec![
            "chapter-5".to_string(),
            "chapter-2-newer".to_string(),
            "chapter-1-5".to_string(),
            "chapter-2-older".to_string(),
        ];
        let chapters = vec![
            chapter_row("chapter-5", 5.0, "2024-05-01T00:00:00Z"),
            chapter_row("chapter-2-newer", 2.0, "2024-02-01T00:00:00Z"),
            chapter_row("chapter-1-5", 1.5, "2024-01-15T00:00:00Z"),
            chapter_row("chapter-2-older", 2.0, "2024-01-01T00:00:00Z"),
            chapter_row("unrequested", 1.0, "2024-01-01T00:00:00Z"),
        ];

        assert_eq!(
            order_chapter_ids_for_chronological_download(chapters, &requested),
            vec![
                "chapter-1-5".to_string(),
                "chapter-2-older".to_string(),
                "chapter-2-newer".to_string(),
                "chapter-5".to_string(),
            ]
        );
    }

    #[test]
    fn bulk_chapter_enqueue_keeps_missing_ids_after_sorted_ids() {
        let requested = vec![
            "chapter-2".to_string(),
            "missing-1".to_string(),
            "chapter-1".to_string(),
            "missing-2".to_string(),
        ];
        let chapters = vec![
            chapter_row("chapter-2", 2.0, "2024-02-01T00:00:00Z"),
            chapter_row("chapter-1", 1.0, "2024-01-01T00:00:00Z"),
        ];

        assert_eq!(
            order_chapter_ids_for_chronological_download(chapters, &requested),
            vec![
                "chapter-1".to_string(),
                "chapter-2".to_string(),
                "missing-1".to_string(),
                "missing-2".to_string(),
            ]
        );
    }
}
