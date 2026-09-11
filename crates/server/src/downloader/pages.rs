use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use autometrics::autometrics;
use axum::body::Bytes;
use backend_persistence::{ChapterRow, MangaRow};
use backend_sources::{SourceMediaClient, fetch::RequestProfile, media::MediaRefSpec};
use backon::{ExponentialBuilder, Retryable};
use futures_util::{StreamExt, stream};
use std::io::{Cursor, Read};
use zip::ZipArchive;

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

struct ExternalPage {
    source_name: String,
    extension: &'static str,
    data: Vec<u8>,
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

/// Tries Rawkuma's optional Google Drive download before resolving reader pages.
///
/// Rawkuma's link is intentionally treated as an optimization, not as the only
/// source of truth. Missing links, unavailable Drive files, non-archives, and
/// archives without supported images all return `Ok(None)` so the caller can use
/// the normal reader-page downloader.
pub(super) async fn try_fetch_google_drive_pages(
    state: &Arc<crate::AppState>,
    manga: &MangaRow,
    chapter: &ChapterRow,
    media_client: &SourceMediaClient,
    cancel_flag: &super::DownloadCancellation,
    staging_dir: &Path,
) -> Result<Option<Vec<(String, PathBuf)>>> {
    if manga.source != "rawkuma" {
        return Ok(None);
    }

    let drive_url = {
        let registry = state.source_registry.read().await;
        match registry
            .get_chapter_list(&manga.source, &manga.source_id)
            .await
        {
            Ok(chapters) => chapters
                .into_iter()
                .find(|candidate| candidate.id == chapter.source_id)
                .and_then(|candidate| candidate.download_url),
            Err(error) => {
                tracing::debug!(
                    source = %manga.source,
                    chapter_id = %chapter.id,
                    error = %error,
                    "Google Drive Chapter Lookup Failed; Falling Back To Reader Pages",
                );
                None
            }
        }
    };
    let Some(drive_url) = drive_url.filter(|url| !url.trim().is_empty()) else {
        return Ok(None);
    };

    ensure_not_cancelled(cancel_flag)?;
    let request_url = google_drive_download_url(&drive_url);
    let media = MediaRefSpec {
        url: request_url,
        request: None,
        transform: None,
    };
    let (body, content_type) = match tokio::select! {
        () = cancel_flag.cancelled() => return Err(super::cancelled_error()),
        result = media_client.fetch_media(&media, RequestProfile::BinaryAsset) => result,
    } {
        Ok(response) => response,
        Err(error) => {
            tracing::debug!(
                source = %manga.source,
                chapter_id = %chapter.id,
                url = %drive_url,
                error = %error,
                "Google Drive Chapter Fetch Failed; Falling Back To Reader Pages",
            );
            return Ok(None);
        }
    };

    let external_pages = match extract_external_pages(&body, &drive_url) {
        Ok(pages) => pages,
        Err(error) => {
            tracing::debug!(
                source = %manga.source,
                chapter_id = %chapter.id,
                url = %drive_url,
                error = %error,
                "Google Drive Chapter Archive Invalid; Falling Back To Reader Pages",
            );
            None
        }
    };
    let Some(external_pages) = external_pages else {
        tracing::debug!(
            source = %manga.source,
            chapter_id = %chapter.id,
            url = %drive_url,
            content_type = %content_type,
            bytes = body.len(),
            "Google Drive Chapter Payload Unusable; Falling Back To Reader Pages",
        );
        return Ok(None);
    };

    let mut pages = Vec::with_capacity(external_pages.len());
    for (index, page) in external_pages.into_iter().enumerate() {
        ensure_not_cancelled(cancel_flag)?;
        let filename = format!("{:04}.{}", index + 1, page.extension);
        let path = super::stage_page_bytes(staging_dir, &filename, &page.data).await?;
        pages.push((filename, path));
    }
    Ok(Some(pages))
}

fn extract_external_pages(
    body: &[u8],
    source_url: &str,
) -> Result<Option<Vec<ExternalPage>>> {
    if let Ok(extension) = backend_image::detect_supported_image_format(body) {
        return Ok(Some(vec![ExternalPage {
            source_name: source_url.to_string(),
            extension,
            data: body.to_vec(),
        }]));
    }

    let mut archive = match ZipArchive::new(Cursor::new(body)) {
        Ok(archive) => archive,
        Err(_) => return Ok(None),
    };
    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("read Google Drive archive entry {index}"))?;
        if entry.is_dir() {
            continue;
        }
        let source_name = entry.name().to_string();
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        let Ok(extension) = backend_image::detect_supported_image_format(&data) else {
            continue;
        };
        pages.push(ExternalPage {
            source_name,
            extension,
            data,
        });
    }
    pages.sort_by(|left, right| natord::compare(&left.source_name, &right.source_name));
    if pages.is_empty() {
        return Ok(None);
    }

    Ok(Some(pages))
}

fn google_drive_download_url(value: &str) -> String {
    let Ok(url) = url::Url::parse(value) else {
        return value.to_owned();
    };
    if !url.host_str().is_some_and(|host| {
        host.trim_end_matches('.')
            .eq_ignore_ascii_case("drive.google.com")
    }) {
        return value.to_owned();
    }

    let segments = url.path_segments().into_iter().flatten().collect::<Vec<_>>();
    let file_id = segments
        .windows(2)
        .find(|pair| pair[0] == "d")
        .map(|pair| pair[1])
        .or_else(|| {
            url.query_pairs()
                .find_map(|(key, value)| (key == "id").then_some(value))
        })
        .filter(|id| !id.is_empty());
    let Some(file_id) = file_id else {
        return value.to_owned();
    };

    let mut direct = url::Url::parse("https://drive.google.com/uc").expect("static Drive URL");
    direct
        .query_pairs_mut()
        .append_pair("export", "download")
        .append_pair("id", &file_id);
    direct.into()
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn image_bytes() -> Vec<u8> {
        let image = RgbImage::new(1, 1);
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn google_drive_archive_extracts_supported_pages_in_natural_order() {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        for name in ["10.png", "cover.txt", "2.png"] {
            archive
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            if name.ends_with(".png") {
                archive.write_all(&image_bytes()).unwrap();
            } else {
                archive.write_all(b"metadata").unwrap();
            }
        }
        let bytes = archive.finish().unwrap().into_inner();

        let pages =
            extract_external_pages(&bytes, "application/zip", "https://drive.google.com/file")
                .unwrap()
                .expect("archive should contain supported pages");
        assert_eq!(
            pages
                .iter()
                .map(|page| page.source_name.as_str())
                .collect::<Vec<_>>(),
            ["2.png", "10.png"]
        );
    }

    #[test]
    fn unusable_google_drive_payload_requests_reader_fallback() {
        assert!(
            extract_external_pages(
                b"Google Drive file is unavailable",
                "text/html",
                "https://drive.google.com/file"
            )
            .unwrap()
            .is_none()
        );
    }
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
