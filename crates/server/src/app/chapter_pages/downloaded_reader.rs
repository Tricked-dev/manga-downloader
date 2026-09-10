use super::{
    DownloadedChapterPage, DownloadedChapterPageReference, DownloadedChapterPageReferenceOptions,
    DownloadedPageReadOptions,
};
use crate::{
    AppState,
    api::{dto::ApiListResponse, error::AppError},
};
use backend_cache::CachedImage;
use std::sync::Arc;

pub(super) async fn downloaded_chapter_page_references_for_reader(
    state: &Arc<AppState>,
    chapter_id: &str,
    options: DownloadedChapterPageReferenceOptions,
) -> Result<ApiListResponse<DownloadedChapterPageReference>, AppError> {
    // Legacy clients may send this; mapped pages need no extraction or warming.
    let _ = options.skip_page_cache;
    super::downloaded_chapter_page_references(&state.db, chapter_id).await
}

pub(super) async fn read_downloaded_chapter_page(
    state: &AppState,
    chapter_id: &str,
    page: usize,
    options: DownloadedPageReadOptions,
) -> Result<DownloadedChapterPage, AppError> {
    if options.width == Some(0) {
        return Err(AppError::bad_request(anyhow::anyhow!(
            "width must be greater than zero"
        )));
    }
    let archive =
        crate::app::downloaded_archive_resolution::existing_completed_archive_for_chapter(
            &state.db, chapter_id,
        )
        .await?;
    let stored = backend_storage::read_page(archive.archive_path, page, options.variant)
        .await
        .map_err(|error| {
            if error.downcast_ref::<backend_storage::ReadError>().is_some() {
                AppError::not_found(error)
            } else {
                AppError::from(error)
            }
        })?;
    let variant = stored.variant;
    if options.format.is_none() && options.width.is_none() {
        state
            .telemetry
            .metrics
            .record_downloaded_page_request(super::downloaded_page_bucket(page), "mapped");
        return Ok(DownloadedChapterPage {
            body: stored.bytes,
            content_type: stored.content_type,
            variant,
            cache_hit: false,
        });
    }
    let format = options.format.unwrap_or(backend_image::OutputFormat::Avif);
    let key = format!(
        "bbf:v1:{}:{}:{}:{}",
        stored.content_hash,
        variant.as_str(),
        format.as_str(),
        options
            .width
            .map_or_else(|| "native".to_string(), |width| width.to_string())
    );
    if !options.skip_page_cache
        && let Some(cached) = state.cache.get_image(&key).await
    {
        state
            .telemetry
            .metrics
            .record_downloaded_page_request(super::downloaded_page_bucket(page), "cache_hit");
        return Ok(DownloadedChapterPage {
            body: cached.body,
            content_type: format.content_type(),
            variant,
            cache_hit: true,
        });
    }
    let body = tokio::task::spawn_blocking(move || {
        backend_image::transform(&stored.bytes, format, options.width)
    })
    .await
    .map_err(|error| anyhow::anyhow!("image conversion task failed: {error}"))??;
    let body = axum::body::Bytes::from(body);
    if !options.skip_page_cache {
        state.cache.insert_image(
            key,
            CachedImage {
                body: body.clone(),
                content_type: format.content_type().to_string(),
            },
        );
    }
    state
        .telemetry
        .metrics
        .record_downloaded_page_request(super::downloaded_page_bucket(page), "converted");
    Ok(DownloadedChapterPage {
        body,
        content_type: format.content_type(),
        variant,
        cache_hit: false,
    })
}
