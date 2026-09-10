use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use autometrics::autometrics;
use axum::body::Bytes;
use backend_sources::SourceMediaClient;
use backon::{ExponentialBuilder, Retryable};
use futures_util::{StreamExt, stream};

use crate::{
    AppState,
    app::{
        chapter_pages::{self, SourceChapterPageReference},
        download_work_state::DownloadWorkStateProgression,
    },
};

use super::cancellation::is_cancelled_error;
use super::{
    DownloadCancellation, cancelled_error, elapsed_ms, ensure_not_cancelled, stage_page_bytes,
};

const PAGE_FETCH_MAX_ATTEMPTS: usize = 3;
const PAGE_FETCH_RETRY_BASE_DELAY: Duration = Duration::from_secs(2);
const PAGE_REFS_MAX_ATTEMPTS: usize = 3;
const PAGE_REFS_RETRY_BASE_DELAY: Duration = Duration::from_millis(500);

struct FetchedPage {
    index: usize,
    filename: String,
    data: Bytes,
    output_size: u64,
    page_elapsed_ms: u64,
}

struct SkippedPage {
    index: usize,
    url: String,
    error: anyhow::Error,
    page_elapsed_ms: u64,
}

enum PageFetchOutcome {
    Fetched(FetchedPage),
    Skipped(SkippedPage),
}

struct PageFetchAttemptError {
    attempt: usize,
    error: anyhow::Error,
}

struct PageRefsAttemptError {
    attempt: usize,
    error: anyhow::Error,
}

fn image_extension_from_url(url: &str) -> &str {
    let parsed = url::Url::parse(url).ok();
    let path = parsed.as_ref().map_or(url, |parsed| parsed.path());
    let Some(content_type) = mime_guess::from_path(path).first_raw() else {
        return "jpg";
    };

    preferred_image_extension(content_type)
}

fn preferred_image_extension(content_type: &str) -> &'static str {
    match content_type {
        "image/avif" => "avif",
        "image/gif" => "gif",
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "jpg",
    }
}

#[allow(clippy::too_many_arguments)]
#[autometrics(track_concurrency)]
pub(super) async fn fetch_chapter_pages(
    state: &Arc<AppState>,
    download: &backend_persistence::DownloadRow,
    page_refs: &[SourceChapterPageReference],
    media_client: &SourceMediaClient,
    cancel_flag: &DownloadCancellation,
    started_at: Instant,
    staging_dir: &Path,
    page_fetch_concurrency: usize,
) -> Result<Vec<(String, PathBuf)>> {
    let total_pages = page_refs.len();
    let mut pages = vec![None; total_pages];
    let concurrency_limit = total_pages.min(page_fetch_concurrency.max(1)).max(1);
    let mut completed_page_refs = 0usize;
    let mut fetched_page_count = 0usize;
    let mut fetch_progress =
        DownloadWorkStateProgression::new(&state.db, &download.id).fetch_progress(total_pages);
    let source = download.manga_source.as_str();

    let mut page_fetches = stream::iter(page_refs.iter().cloned().enumerate())
        .map(|(index, reference)| {
            let state = Arc::clone(state);
            let media_client = media_client.clone();
            let source = source.to_string();
            async move { fetch_page(state, media_client, &source, index, reference).await }
        })
        .buffer_unordered(concurrency_limit);

    while let Some(result) = tokio::select! {
        () = cancel_flag.cancelled() => {
            return Err(cancelled_error());
        }
        result = page_fetches.next() => result,
    } {
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                state.telemetry.metrics.record_page_fetch_error(source);
                return Err(error);
            }
        };

        match outcome {
            PageFetchOutcome::Fetched(fetched) => {
                state.telemetry.metrics.record_page_fetch_success(
                    source,
                    fetched.page_elapsed_ms,
                    fetched.output_size,
                );
                let page_number = fetched.index + 1;
                let path = stage_page_bytes(staging_dir, &fetched.filename, &fetched.data).await?;
                pages[fetched.index] = Some((fetched.filename, path));
                fetched_page_count += 1;
                tracing::trace!(
                    download_id = %download.id,
                    chapter_id = %download.chapter_id,
                    page = page_number,
                    total_pages,
                    fetched_pages = fetched_page_count,
                    bytes = fetched.output_size,
                    page_elapsed_ms = fetched.page_elapsed_ms,
                    total_elapsed_ms = elapsed_ms(started_at),
                    "Chapter Page Stored",
                );
            }
            PageFetchOutcome::Skipped(skipped) => {
                state.telemetry.metrics.record_page_fetch_error(source);
                let error_chain = format!("{:#}", skipped.error);
                tracing::warn!(
                    download_id = %download.id,
                    chapter_id = %download.chapter_id,
                    page = skipped.index + 1,
                    total_pages,
                    url = %skipped.url,
                    page_elapsed_ms = skipped.page_elapsed_ms,
                    error = %error_chain,
                    "Chapter Page Skipped",
                );
            }
        }

        completed_page_refs += 1;
        fetch_progress.refresh(completed_page_refs).await?;
    }

    let fetched_pages: Vec<_> = pages.into_iter().flatten().collect();
    if fetched_pages.is_empty() {
        return Err(anyhow::anyhow!("All chapter pages failed to fetch"));
    }

    Ok(fetched_pages)
}

#[autometrics(track_concurrency)]
async fn fetch_page(
    state: Arc<AppState>,
    media_client: SourceMediaClient,
    source: &str,
    index: usize,
    reference: SourceChapterPageReference,
) -> Result<PageFetchOutcome> {
    let fetch_started_at = Instant::now();
    let mut attempt = 0usize;
    let fetch_result = (|| {
        attempt += 1;
        let attempt = attempt;
        let media_client = media_client.clone();
        let reference = reference.clone();
        let state = Arc::clone(&state);
        async move {
            let page_started_at = Instant::now();
            chapter_pages::read_source_chapter_page(
                &state.cache,
                &state.telemetry.metrics,
                &media_client,
                reference,
            )
            .await
            .map(|page| (page, elapsed_ms(page_started_at)))
            .map_err(|error| PageFetchAttemptError { attempt, error })
        }
    })
    .retry(page_fetch_backoff())
    .sleep(tokio::time::sleep)
    .when(|error| is_retriable_page_fetch_error(&error.error))
    .notify(|error, retry_delay| {
        state.telemetry.metrics.record_page_fetch_retry(source);
        tracing::debug!(
            page = index + 1,
            url = %reference.fallback_url(),
            attempt = error.attempt,
            max_attempts = PAGE_FETCH_MAX_ATTEMPTS,
            retry_in_ms = u64::try_from(retry_delay.as_millis()).unwrap_or(u64::MAX),
            error = %error.error,
            "Chapter Page Fetch Retrying",
        );
    })
    .await;

    match fetch_result {
        Ok((page, page_elapsed_ms)) => {
            let output_size = u64::try_from(page.body.len()).unwrap_or(u64::MAX);

            Ok(PageFetchOutcome::Fetched(FetchedPage {
                index,
                filename: format!(
                    "{:04}.{}",
                    index + 1,
                    image_extension_from_url(page.reference.fallback_url())
                ),
                data: page.body,
                output_size,
                page_elapsed_ms,
            }))
        }
        Err(error) => Ok(skipped_page(
            index,
            reference.fallback_url(),
            error.error.context(format!(
                "Failed to fetch page {} from {}",
                index + 1,
                reference.fallback_url()
            )),
            fetch_started_at,
        )),
    }
}

fn page_fetch_backoff() -> ExponentialBuilder {
    ExponentialBuilder::default()
        .with_min_delay(PAGE_FETCH_RETRY_BASE_DELAY)
        .with_factor(2.0)
        .with_max_times(PAGE_FETCH_MAX_ATTEMPTS.saturating_sub(1))
}

fn skipped_page(
    index: usize,
    url: &str,
    error: anyhow::Error,
    started_at: Instant,
) -> PageFetchOutcome {
    PageFetchOutcome::Skipped(SkippedPage {
        index,
        url: url.to_string(),
        error,
        page_elapsed_ms: elapsed_ms(started_at),
    })
}

fn is_retriable_page_fetch_error(error: &anyhow::Error) -> bool {
    let message = format!("{error:#}");

    message.contains("Request failed:")
        || message.contains("invalid image bytes")
        || message.contains("image format could not be determined")
        || [
            "status 408",
            "status 429",
            "status 500",
            "status 502",
            "status 503",
            "status 504",
            "status 522",
            "status 523",
            "status 524",
        ]
        .iter()
        .any(|status| message.contains(status))
}

pub(super) async fn get_page_refs_with_retry(
    state: &Arc<AppState>,
    source: &str,
    chapter_source_id: &str,
    cancel_flag: &Arc<DownloadCancellation>,
) -> Result<Vec<SourceChapterPageReference>> {
    let mut attempt = 0usize;
    let (page_refs, attempt) = (|| {
        attempt += 1;
        let attempt = attempt;
        let cancel_flag = Arc::clone(cancel_flag);
        let state = Arc::clone(state);
        let source = source.to_string();
        let chapter_source_id = chapter_source_id.to_string();
        async move {
            resolve_page_refs_attempt(state, source, chapter_source_id, cancel_flag, attempt).await
        }
    })
    .retry(page_refs_backoff())
    .sleep({
        let cancel_flag = Arc::clone(cancel_flag);
        move |retry_delay| {
            let cancel_flag = Arc::clone(&cancel_flag);
            async move {
                tokio::select! {
                    () = cancel_flag.cancelled() => {}
                    () = tokio::time::sleep(retry_delay) => {}
                }
            }
        }
    })
    .when(|error| !is_cancelled_error(&error.error))
    .notify(|error, retry_delay| {
        tracing::debug!(
            source,
            chapter_source_id,
            attempt = error.attempt,
            retry_in_ms = u64::try_from(retry_delay.as_millis()).unwrap_or(u64::MAX),
            "Chapter Pages Request Retrying",
        );
    })
    .await
    .map_err(|error| error.error)?;

    tracing::trace!(
        source,
        chapter_source_id,
        attempt,
        page_count = page_refs.len(),
        outcome = "success",
        "Chapter Pages Resolved",
    );
    Ok(page_refs)
}

fn page_refs_backoff() -> ExponentialBuilder {
    ExponentialBuilder::default()
        .with_min_delay(PAGE_REFS_RETRY_BASE_DELAY)
        .with_factor(2.0)
        .with_max_times(PAGE_REFS_MAX_ATTEMPTS.saturating_sub(1))
}

async fn resolve_page_refs_attempt(
    state: Arc<AppState>,
    source: String,
    chapter_source_id: String,
    cancel_flag: Arc<DownloadCancellation>,
    attempt: usize,
) -> std::result::Result<(Vec<SourceChapterPageReference>, usize), PageRefsAttemptError> {
    ensure_not_cancelled(&cancel_flag).map_err(|error| PageRefsAttemptError { attempt, error })?;
    let result = chapter_pages::source_chapter_page_references(
        &state.source_registry,
        &state.cache,
        source.clone(),
        chapter_source_id.clone(),
    )
    .await
    .map(|pages| pages.items);

    match result {
        Ok(page_refs) if !page_refs.is_empty() => Ok((page_refs, attempt)),
        Ok(_) => {
            tracing::debug!(
                source,
                chapter_source_id,
                attempt,
                reason = "empty_page_list",
                outcome = "error",
                "Chapter Pages Request Failed",
            );
            Err(PageRefsAttemptError {
                attempt,
                error: anyhow::anyhow!("No pages found for chapter"),
            })
        }
        Err(error) => {
            tracing::debug!(
                source,
                chapter_source_id,
                attempt,
                error = %error,
                outcome = "error",
                "Chapter Pages Request Failed",
            );
            Err(PageRefsAttemptError {
                attempt,
                error: error.into(),
            })
        }
    }
}
