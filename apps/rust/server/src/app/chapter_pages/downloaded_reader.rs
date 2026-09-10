use std::{sync::Arc, time::Duration};

use backend_cache::MangaCache;
use backend_page_extraction::DownloadedPageExtractionScheduler;
use backend_persistence::Database;

use crate::{
    AppState,
    api::{dto::ApiListResponse, error::AppError},
};

use super::{
    DOWNLOADED_PAGE_READ_AHEAD_WINDOW, DownloadedChapterPage, DownloadedChapterPageReference,
    DownloadedChapterPageReferenceOptions, DownloadedPageExtractResult, DownloadedPageReadOptions,
    NEXT_DOWNLOADED_CHAPTER_READ_AHEAD_PAGES, apply_downloaded_source_transform,
    downloaded_chapter_page_references, downloaded_page_bucket, downloaded_page_window_end,
    extract_downloaded_page_window,
};

#[derive(Clone, Copy)]
struct DownloadedPageReader<'a> {
    state: &'a AppState,
    db: &'a Database,
    cache: &'a MangaCache,
    archive_index: &'a crate::archive_index::ArchiveIndexService,
    extraction_scheduler: &'a DownloadedPageExtractionScheduler,
    metrics: &'a backend_telemetry::Metrics,
}

pub(super) async fn downloaded_chapter_page_references_for_reader(
    state: &Arc<AppState>,
    chapter_id: &str,
    options: DownloadedChapterPageReferenceOptions,
) -> Result<ApiListResponse<DownloadedChapterPageReference>, AppError> {
    let references = downloaded_chapter_page_references(
        &state.db,
        &state.archive_index,
        &state.telemetry.metrics,
        chapter_id,
    )
    .await?;

    if !options.skip_page_cache {
        warm_next_downloaded_chapter(Arc::clone(state), chapter_id.to_string());
    }

    Ok(references)
}

pub(super) async fn read_downloaded_chapter_page(
    state: &AppState,
    chapter_id: &str,
    page: usize,
    options: DownloadedPageReadOptions,
) -> Result<DownloadedChapterPage, AppError> {
    let reader = DownloadedPageReader {
        state,
        db: &state.db,
        cache: &state.cache,
        archive_index: &state.archive_index,
        extraction_scheduler: &state.extraction_scheduler,
        metrics: &state.telemetry.metrics,
    };
    let mut retried_stale_identity = false;
    loop {
        match read_downloaded_chapter_page_once(reader, chapter_id, page, options).await {
            Ok(page_response) => {
                if retried_stale_identity {
                    tracing::debug!(
                        chapter_id,
                        page,
                        "Downloaded Chapter Page Read Retried After Stale Index",
                    );
                }
                return Ok(page_response);
            }
            Err(error) if error.is_downloaded_page_conflict() && !retried_stale_identity => {
                retried_stale_identity = true;
            }
            Err(error) => {
                if retried_stale_identity && error.is_downloaded_page_conflict() {
                    tracing::warn!(
                        chapter_id,
                        page,
                        "Downloaded Chapter Page Read Still Stale After Retry",
                    );
                }
                reader
                    .metrics
                    .record_downloaded_page_request(downloaded_page_bucket(page), "error");
                return Err(error);
            }
        }
    }
}

async fn read_downloaded_chapter_page_once(
    reader: DownloadedPageReader<'_>,
    chapter_id: &str,
    page: usize,
    options: DownloadedPageReadOptions,
) -> Result<DownloadedChapterPage, AppError> {
    if !options.skip_page_cache
        && let Some(pages) = reader.archive_index.cached_chapter_pages(chapter_id)
        && let Some(page_entry) = pages.get(page)
        && let Some(cached) = reader.cache.get_image(&page_entry.cache_key).await
    {
        reader
            .metrics
            .record_downloaded_page_request(downloaded_page_bucket(page), "cache_hit");
        let page_response = DownloadedChapterPage {
            body: cached.body,
            content_type: page_entry.content_type,
        };
        return apply_downloaded_source_transform(
            reader.state,
            Some(chapter_id),
            page,
            page_response,
        )
        .await;
    }

    let archive = reader
        .archive_index
        .get_for_chapter(reader.db, reader.metrics, chapter_id, "lazy")
        .await?;
    let window_size = if options.skip_page_cache {
        1
    } else {
        reader
            .extraction_scheduler
            .foreground_read_window_size(
                archive.identity.archive_path.as_str(),
                page,
                DOWNLOADED_PAGE_READ_AHEAD_WINDOW,
            )
            .await
    };
    if !options.skip_page_cache {
        let page_entry = archive
            .page(page)
            .ok_or_else(|| AppError::not_found(anyhow::anyhow!("page {page} out of bounds")))?;
        if let Some(cached) = reader.cache.get_image(&page_entry.cache_key).await {
            reader
                .metrics
                .record_downloaded_page_request(downloaded_page_bucket(page), "cache_hit");
            let page_response = DownloadedChapterPage {
                body: cached.body,
                content_type: page_entry.content_type,
            };
            return apply_downloaded_source_transform(
                reader.state,
                Some(chapter_id),
                page,
                page_response,
            )
            .await;
        }
    }

    let extracted = extract_downloaded_page_window(
        reader.state,
        reader.cache,
        reader.extraction_scheduler,
        reader.metrics,
        Arc::clone(&archive),
        Some(chapter_id),
        page,
        window_size,
        window_size > 1,
        None,
        "foreground",
        options.skip_page_cache,
    )
    .await?;
    let DownloadedPageExtractResult::Completed {
        requested,
        extracted_pages,
    } = extracted
    else {
        return Err(AppError::internal(anyhow::anyhow!(
            "foreground downloaded page extraction was dropped"
        )));
    };
    let outcome = if extracted_pages == 0 {
        "coalesced_hit"
    } else {
        "extracted"
    };
    reader
        .metrics
        .record_downloaded_page_request(downloaded_page_bucket(page), outcome);

    Ok(DownloadedChapterPage {
        body: requested.body,
        content_type: requested.content_type,
    })
}

#[autometrics::autometrics(track_concurrency)]
pub(super) async fn warm_downloaded_chapter_start(
    state: &Arc<AppState>,
    chapter_id: &str,
    ttl: Duration,
) -> Result<usize, AppError> {
    let archive = state
        .archive_index
        .get_for_chapter(&state.db, &state.telemetry.metrics, chapter_id, "lazy")
        .await?;
    let page_count = archive.page_count();
    let page_limit = page_count.min(NEXT_DOWNLOADED_CHAPTER_READ_AHEAD_PAGES);

    let mut next_page = 0usize;
    let mut cached_pages = 0usize;

    while next_page < page_limit {
        let Some(page_entry) = archive.page(next_page) else {
            break;
        };
        if state.cache.get_image(&page_entry.cache_key).await.is_some() {
            next_page += 1;
            continue;
        }

        let extracted = extract_downloaded_page_window(
            state,
            &state.cache,
            &state.extraction_scheduler,
            &state.telemetry.metrics,
            Arc::clone(&archive),
            Some(chapter_id),
            next_page,
            DOWNLOADED_PAGE_READ_AHEAD_WINDOW,
            false,
            Some(ttl),
            "read_ahead",
            false,
        )
        .await?;
        cached_pages += extracted.extracted_pages();
        next_page =
            downloaded_page_window_end(next_page, page_limit, DOWNLOADED_PAGE_READ_AHEAD_WINDOW);
    }

    Ok(cached_pages)
}

fn warm_next_downloaded_chapter(state: Arc<AppState>, chapter_id: String) {
    tokio::spawn(async move {
        if let Err(error) = super::warm_next_downloaded_chapter_pages(&state, &chapter_id).await {
            tracing::debug!(
                chapter_id = %chapter_id,
                error = %error,
                "Next Downloaded Chapter Page Warming Failed",
            );
        }
    });
}
