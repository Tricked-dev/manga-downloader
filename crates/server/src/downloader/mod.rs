use anyhow::{Context, Result};
use autometrics::autometrics;
use backend_persistence::{ChapterRow, MangaRow};
use backend_sources::{
    fetch::RequestProfile,
    media::{MediaRefSpec, decode_media_spec},
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_graceful::{ShutdownGuard, WeakShutdownGuard};
use uuid::Uuid;

mod cancellation;
mod comicinfo;
mod pages;
mod storage_usage;

pub(crate) use cancellation::{DownloadCancellation, request_download_cancel};
pub(crate) use comicinfo::{comicinfo_age_rating, comicinfo_count_for_series};
pub(crate) use storage_usage::{
    cached_download_storage_usage_bytes, get_download_storage_usage_bytes,
    invalidate_download_storage_usage, negative_storage_delta,
    refresh_download_storage_usage_if_stale, update_download_storage_usage_delta,
};

use cancellation::{
    cancelled_error, is_cancelled_error, register_active_download, unregister_active_download,
};
use pages::{fetch_chapter_pages, get_page_refs_with_retry, try_fetch_google_drive_pages};
use storage_usage::{
    enforce_download_storage_limit, move_archive_into_library,
    reserve_download_storage_for_archive, rollback_download_storage_reservation,
};

use crate::{
    AppState,
    app::{
        download_work_state::DownloadWorkStateProgression, downloaded_archive_lifecycle, settings,
        source_chapter_sync,
    },
};

fn elapsed_ms(started_at: Instant) -> u64 {
    u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
}

struct CompletedDownload {
    source: String,
    total_pages: usize,
    archive_path: PathBuf,
    archive_size_bytes: u64,
}

#[derive(Clone)]
struct DownloadPipeline {
    fetch_slots: Arc<Semaphore>,
    processing_slots: Arc<Semaphore>,
}

impl DownloadPipeline {
    fn new(worker_count: usize) -> Self {
        Self {
            fetch_slots: Arc::new(Semaphore::new(worker_count.max(1))),
            processing_slots: Arc::new(Semaphore::new(worker_count.max(1))),
        }
    }
}

async fn stage_page_bytes(staging_dir: &Path, filename: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = staging_dir.join(filename);
    backend_fs::write_bytes(&path, bytes).await?;
    Ok(path)
}

fn create_download_staging_dir(download_id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("manga-download-{download_id}-{}", Uuid::new_v4()))
}

async fn cleanup_download_staging_dir(path: &Path) {
    if backend_fs::path_exists(path)
        && let Err(error) = backend_fs::remove_dir_all(path).await
    {
        tracing::warn!(
            path = %path.display(),
            error = %error,
            "Download Staging Cleanup Failed",
        );
    }
}

async fn fetch_series_cover_archive_entry(
    state: &Arc<AppState>,
    manga: &backend_persistence::MangaRow,
    cancel_flag: &DownloadCancellation,
) -> Result<Option<(String, Vec<u8>)>> {
    ensure_not_cancelled(cancel_flag)?;

    if manga.cover_url.trim().is_empty() {
        return Ok(None);
    }

    let media = if let Some(spec) = manga.cover_fetch_spec.as_deref() {
        decode_media_spec(spec)?
    } else {
        MediaRefSpec {
            url: manga.cover_url.clone(),
            request: None,
            transform: None,
        }
    };

    let media_client = {
        let pm = state.source_registry.read().await;
        pm.media_client(&manga.source)?
    };
    let (cover_bytes, _) = tokio::select! {
        () = cancel_flag.cancelled() => return Err(cancelled_error()),
        result = media_client.fetch_media(&media, RequestProfile::ImageHotlink) => result?,
    };

    ensure_not_cancelled(cancel_flag)?;

    Ok(Some(("cover.image".to_string(), cover_bytes.to_vec())))
}

fn ensure_not_cancelled(flag: &DownloadCancellation) -> Result<()> {
    if flag.is_cancelled() {
        Err(cancelled_error())
    } else {
        Ok(())
    }
}

/// Background task that continuously processes queued downloads.
#[autometrics(track_concurrency)]
pub async fn process_downloads(state: Arc<AppState>, shutdown: ShutdownGuard) {
    let worker_count = download_concurrent_chapters(&state).await;
    tracing::info!(workers = worker_count, "Download Processor Started");
    let pipeline = DownloadPipeline::new(worker_count);
    let mut workers = tokio::task::JoinSet::new();

    for worker in 0..worker_count {
        let worker_state = Arc::clone(&state);
        let worker_pipeline = pipeline.clone();
        let worker_shutdown = shutdown.clone_weak();
        workers.spawn(async move {
            process_download_worker(worker_state, worker_pipeline, worker_shutdown, worker).await;
        });
    }

    while let Some(result) = workers.join_next().await {
        if let Err(error) = result {
            tracing::error!(
                error = %error,
                outcome = "error",
                "Download Processor Worker Exited Unexpectedly",
            );
        }
    }

    tracing::info!("Download Processor Stopped");
}

async fn process_download_worker(
    state: Arc<AppState>,
    pipeline: DownloadPipeline,
    shutdown: WeakShutdownGuard,
    worker: usize,
) {
    loop {
        let fetch_permit = match tokio::select! {
            () = shutdown.cancelled() => {
                tracing::info!(worker, "Download Processor Worker Stopping");
                break;
            }
            permit = pipeline.fetch_slots.clone().acquire_owned() => permit
        } {
            Ok(permit) => permit,
            Err(error) => {
                tracing::error!(
                    worker,
                    error = %error,
                    "Download Fetch Gate Closed",
                );
                return;
            }
        };

        match process_next_download(&state, &pipeline, fetch_permit).await {
            Ok(true) => {}
            Ok(false) => {
                tokio::select! {
                    () = shutdown.cancelled() => {
                        tracing::info!(worker, "Download Processor Worker Stopping");
                        break;
                    }
                    () = state.download_queue_notify.notified() => {}
                }
            }
            Err(error) => {
                tracing::error!(
                    worker,
                    error = %error,
                    outcome = "error",
                    "Download Processor Loop Failed",
                );
                tokio::select! {
                    () = shutdown.cancelled() => {
                        tracing::info!(worker, "Download Processor Worker Stopping");
                        break;
                    }
                    () = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {}
                }
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
#[autometrics(track_concurrency)]
async fn process_next_download(
    state: &Arc<AppState>,
    pipeline: &DownloadPipeline,
    fetch_permit: OwnedSemaphorePermit,
) -> Result<bool> {
    let Some(download) = state.db.get_next_queued_download().await? else {
        return Ok(false);
    };
    let started_at = Instant::now();

    let cancel_flag = register_active_download(state, &download.id).await;
    state
        .telemetry
        .metrics
        .download_started(&download.manga_source);

    tracing::trace!(
        download_id = %download.id,
        manga_id = %download.manga_id,
        chapter_id = %download.chapter_id,
        manga_title = %download.manga_title,
        chapter_title = %download.chapter_title,
        "Chapter Download Started",
    );

    let result = download_chapter(state, pipeline, &download, &cancel_flag, fetch_permit).await;
    unregister_active_download(state, &download.id).await;
    state
        .telemetry
        .metrics
        .download_finished(&download.manga_source);

    match result {
        Ok(completed) => {
            state.telemetry.metrics.record_download_completed(
                &completed.source,
                "success",
                started_at.elapsed(),
                completed.total_pages,
                Some(completed.archive_size_bytes),
            );
            downloaded_archive_lifecycle::downloaded_archive_created(
                state,
                &download.id,
                completed.total_pages,
                completed.archive_path.clone(),
                completed.archive_size_bytes,
            )
            .await?;
            tracing::info!(
                outcome = "success",
                download_id = %download.id,
                manga_id = %download.manga_id,
                chapter_id = %download.chapter_id,
                source = %completed.source,
                manga_title = %download.manga_title,
                chapter_title = %download.chapter_title,
                total_pages = completed.total_pages,
                archive_bytes = completed.archive_size_bytes,
                archive_path = %completed.archive_path.display(),
                duration_ms = elapsed_ms(started_at),
                "Chapter Download Completed",
            );
        }
        Err(error) => {
            let err_msg = format!("{error:#}");
            let work_state = DownloadWorkStateProgression::new(&state.db, &download.id);
            if is_cancelled_error(&error) {
                state.telemetry.metrics.record_download_completed(
                    &download.manga_source,
                    "cancelled",
                    started_at.elapsed(),
                    0,
                    None,
                );
                work_state.cancelled().await?;
                tracing::info!(
                    outcome = "cancelled",
                    download_id = %download.id,
                    manga_id = %download.manga_id,
                    chapter_id = %download.chapter_id,
                    manga_title = %download.manga_title,
                    chapter_title = %download.chapter_title,
                    duration_ms = elapsed_ms(started_at),
                    "Chapter Download Completed",
                );
            } else {
                state.telemetry.metrics.record_download_completed(
                    &download.manga_source,
                    "error",
                    started_at.elapsed(),
                    0,
                    None,
                );
                work_state.failed(&err_msg).await?;
                tracing::error!(
                    outcome = "error",
                    download_id = %download.id,
                    manga_id = %download.manga_id,
                    chapter_id = %download.chapter_id,
                    manga_title = %download.manga_title,
                    chapter_title = %download.chapter_title,
                    error = %err_msg,
                    duration_ms = elapsed_ms(started_at),
                    "Chapter Download Completed",
                );
            }
        }
    }

    Ok(true)
}

#[allow(clippy::too_many_lines)]
// This function is the end-to-end chapter download pipeline; keeping the stages together makes
// cancellation, progress, and cleanup behavior easier to audit.
#[autometrics(track_concurrency)]
async fn download_chapter(
    state: &Arc<AppState>,
    pipeline: &DownloadPipeline,
    download: &backend_persistence::DownloadRow,
    cancel_flag: &Arc<DownloadCancellation>,
    fetch_permit: OwnedSemaphorePermit,
) -> Result<CompletedDownload> {
    let started_at = Instant::now();
    let staging_dir = create_download_staging_dir(&download.id);
    backend_fs::create_dir_all(&staging_dir).await?;

    let result = async {
        ensure_not_cancelled(cancel_flag)?;

        let download_path = settings::download_path(&state.db).await?;
        enforce_download_storage_limit(state, download, &download_path).await?;

        let manga = crate::app::library::refresh_manga_metadata_if_incomplete(
            &state.db,
            &state.source_registry,
            &download.manga_id,
        )
        .await
        .context("Failed to refresh manga metadata")?;

        let mut chapter = state
            .db
            .get_chapter_by_id(&download.chapter_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Chapter not found"))?;

        let media_client = {
            let pm = state.source_registry.read().await;
            pm.media_client(&manga.source)?
        };
        let page_fetch_concurrency = settings::download_page_fetch_concurrency(&state.db).await?;
        let work_state = DownloadWorkStateProgression::new(&state.db, &download.id);

        work_state.fetching_pages().await?;

        let pages = if let Some(pages) = try_fetch_google_drive_pages(
            state,
            &manga,
            &chapter,
            &media_client,
            cancel_flag,
            &staging_dir,
        )
        .await?
        {
            let page_count = pages.len();
            let mut fetch_progress = work_state.fetch_progress(page_count);
            fetch_progress.refresh(page_count).await?;
            tracing::trace!(
                download_id = %download.id,
                chapter_id = %download.chapter_id,
                source = %manga.source,
                page_count,
                page_source = "google_drive",
                elapsed_ms = elapsed_ms(started_at),
                "Chapter Pages Resolved",
            );
            pages
        } else {
            let page_refs =
                get_page_refs_refreshing_stale_chapter(state, &manga, &mut chapter, cancel_flag)
                    .await?;
            let total_pages = page_refs.len();
            if total_pages == 0 {
                return Err(anyhow::anyhow!("No pages found for chapter"));
            }

            ensure_not_cancelled(cancel_flag)?;
            tracing::trace!(
                download_id = %download.id,
                chapter_id = %download.chapter_id,
                source = %manga.source,
                page_count = total_pages,
                page_source = "reader",
                elapsed_ms = elapsed_ms(started_at),
                "Chapter Pages Resolved",
            );

            fetch_chapter_pages(
                state,
                download,
                &page_refs,
                &media_client,
                cancel_flag,
                started_at,
                &staging_dir,
                page_fetch_concurrency,
            )
            .await?
        };

        drop(fetch_permit);

        let fetched_page_count = pages.len();

        let _processing_permit = tokio::select! {
            () = cancel_flag.cancelled() => return Err(cancelled_error()),
            permit = pipeline.processing_slots.clone().acquire_owned() => permit
                .map_err(|error| anyhow::anyhow!("Download processing gate closed: {error}"))?,
        };

        work_state.transforming_assets_completed().await?;
        let archive_path = backend_core::download_archive_path(
            &download_path,
            &manga.source,
            &download.manga_title,
            chapter.chapter_number,
        );
        let staged_archive_path = staging_dir.join(backend_core::download_archive_filename(
            chapter.chapter_number,
        ));

        work_state.archiving().await?;
        let archive_started_at = Instant::now();

        ensure_not_cancelled(cancel_flag)?;

        let comicinfo_credits = tokio::select! {
            () = cancel_flag.cancelled() => return Err(cancelled_error()),
            credits = crate::comicinfo::resolve_comicinfo_credits(&manga) => credits,
        };
        ensure_not_cancelled(cancel_flag)?;
        let comicinfo_xml = backend_core::build_comicinfo_xml(
            &manga.title,
            &manga.description,
            &manga.author,
            &manga.genres,
            &backend_core::ComicInfoChapterMetadata {
                title: &chapter.title,
                number: chapter.chapter_number,
                count: comicinfo_count_for_series(&manga.status, manga.total_chapters),
                date_uploaded: &chapter.date_uploaded,
                page_count: fetched_page_count,
                age_rating: Some(comicinfo_age_rating(manga.is_nsfw)),
                source_url: None,
                language: manga
                    .language
                    .as_deref()
                    .map(str::trim)
                    .filter(|language| !language.is_empty())
                    .or(Some(backend_core::DEFAULT_LANGUAGE_ISO)),
                credits: comicinfo_credits.as_borrowed(),
            },
        );
        let cover_entry = fetch_series_cover_archive_entry(state, &manga, cancel_flag).await?;
        let cover = if let Some((name, bytes)) = cover_entry {
            Some(stage_page_bytes(&staging_dir, &name, &bytes).await?)
        } else {
            None
        };
        let archive_cancel_flag = Arc::clone(cancel_flag);
        let archive_result = backend_storage::write_originals(
            staged_archive_path.clone(),
            backend_storage::OriginalChapter {
                pages: pages.into_iter().map(|(_, path)| path).collect(),
                comicinfo_xml,
                cover,
            },
            move || archive_cancel_flag.is_cancelled(),
        )
        .await;
        match archive_result {
            Ok(()) => {}
            Err(_) if cancel_flag.is_cancelled() => return Err(cancelled_error()),
            Err(error) => return Err(error),
        }
        let archive_size = backend_fs::file_size(&staged_archive_path).await?;
        let storage_reservation = reserve_download_storage_for_archive(
            state,
            download,
            &download_path,
            &archive_path,
            archive_size,
        )
        .await?;

        if cancel_flag.is_cancelled() {
            rollback_download_storage_reservation(state, &download_path, storage_reservation).await;
            let _ = backend_fs::remove_file_if_present(&staged_archive_path).await;
            return Err(cancelled_error());
        }

        if let Err(error) = move_archive_into_library(&staged_archive_path, &archive_path).await {
            rollback_download_storage_reservation(state, &download_path, storage_reservation).await;
            return Err(error);
        }

        if cancel_flag.is_cancelled() {
            let _ = backend_fs::remove_file_if_present(&archive_path).await;
            update_download_storage_usage_delta(
                state,
                &download_path,
                negative_storage_delta(storage_reservation.archive_size_bytes),
            )
            .await;
            return Err(cancelled_error());
        }

        ensure_not_cancelled(cancel_flag)?;

        tracing::trace!(
            download_id = %download.id,
            chapter_id = %download.chapter_id,
            path = %archive_path.display(),
            pages = fetched_page_count,
            archive_bytes = archive_size,
            archive_duration_ms = elapsed_ms(archive_started_at),
            elapsed_ms = elapsed_ms(started_at),
            "Chapter Archive Created",
        );

        Ok(CompletedDownload {
            source: manga.source,
            total_pages: fetched_page_count,
            archive_path,
            archive_size_bytes: archive_size,
        })
    }
    .await;

    cleanup_download_staging_dir(&staging_dir).await;
    result
}

async fn get_page_refs_refreshing_stale_chapter(
    state: &Arc<AppState>,
    manga: &MangaRow,
    chapter: &mut ChapterRow,
    cancel_flag: &Arc<DownloadCancellation>,
) -> Result<Vec<crate::app::chapter_pages::SourceChapterPageReference>> {
    match get_page_refs_with_retry(state, &manga.source, &chapter.source_id, cancel_flag).await {
        Ok(page_refs) => Ok(page_refs),
        Err(original_error) => {
            let original_error_chain = format!("{original_error:#}");
            if let Some(refreshed_chapter) =
                refresh_stale_chapter_source_id(state, manga, chapter, &original_error_chain)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to refresh chapter list after page resolution failed: {original_error_chain}"
                        )
                    })?
            {
                *chapter = refreshed_chapter;
                return get_page_refs_with_retry(
                    state,
                    &manga.source,
                    &chapter.source_id,
                    cancel_flag,
                )
                .await
                .with_context(|| {
                    format!(
                        "Failed to resolve chapter pages after refreshing stale chapter source id; original error: {original_error_chain}"
                    )
                });
            }

            Err(original_error).with_context(|| {
                "Failed to resolve chapter pages; chapter source id refresh did not change the stored source id"
            })
        }
    }
}

async fn refresh_stale_chapter_source_id(
    state: &Arc<AppState>,
    manga: &MangaRow,
    chapter: &ChapterRow,
    original_error_chain: &str,
) -> Result<Option<ChapterRow>> {
    let Some(refreshed) = source_chapter_sync::refresh_stale_chapter_source_id(
        &state.db,
        &state.source_registry,
        manga,
        chapter,
    )
    .await?
    else {
        return Ok(None);
    };

    tracing::warn!(
        manga_id = %manga.id,
        source = %manga.source,
        manga_source_id = %manga.source_id,
        chapter_id = %chapter.id,
        stale_chapter_source_id = %chapter.source_id,
        refreshed_chapter_source_id = %refreshed.source_id,
        chapter_number = chapter.chapter_number,
        original_error = %original_error_chain,
        "Chapter Source Id Refreshed After Page Resolution Failure",
    );

    Ok(Some(refreshed))
}

async fn download_concurrent_chapters(state: &AppState) -> usize {
    match settings::download_concurrent_chapters(&state.db).await {
        Ok(worker_count) => worker_count,
        Err(error) => {
            tracing::warn!(
                error = %error,
                default = settings::DEFAULT_DOWNLOAD_CONCURRENT_CHAPTERS,
                "Download Concurrency Setting Read Failed",
            );
            settings::DEFAULT_DOWNLOAD_CONCURRENT_CHAPTERS
        }
    }
}
