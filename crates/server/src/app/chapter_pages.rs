use crate::{
    AppState,
    api::{dto::ApiListResponse, error::AppError},
    app::downloaded_archive_derived_state,
};
use anyhow::Context;
use autometrics::autometrics;
use axum::body::Bytes;
use backend_cache::{CachedImage, MangaCache};
use backend_page_extraction::{
    DownloadedPageExtractionScheduler, ExtractionRequestMode, PositionedExtractionResult,
    PositionedImageTarget,
};
use backend_persistence::Database;
use backend_sources::{
    SourceRegistry, SourceMediaClient,
    fetch::RequestProfile,
    media::{MediaRefSpec, MediaTransformSpec, encode_media_spec, media_ref_to_spec},
};
use dashmap::DashMap;
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

mod downloaded_reader;

const DOWNLOADED_PAGE_READ_AHEAD_WINDOW: usize = 4;
const NEXT_DOWNLOADED_CHAPTER_READ_AHEAD_PAGES: usize = DOWNLOADED_PAGE_READ_AHEAD_WINDOW;
const NEXT_CHAPTER_READ_AHEAD_TTL: Duration = Duration::from_mins(15);
const CURRENT_CHAPTER_READ_AHEAD_TTL: Duration = Duration::from_mins(5);
const MAX_CONCURRENT_IMAGE_READ_AHEAD: usize = 4;
const DEFAULT_DOWNLOAD_TRANSFORM_AVIF_QUALITY: u8 = 80;
const SOURCE_PAGE_REFS_CACHE_SCHEMA_VERSION: u8 = 9;

struct DownloadedPageExtractTarget {
    page: usize,
    cache_key: String,
    indexed_content_type: &'static str,
    file_position: usize,
}

pub(crate) struct DownloadedPageTransformCache {
    metadata: DashMap<String, Arc<DownloadedPageTransformMetadata>>,
}

impl DownloadedPageTransformCache {
    pub(crate) fn new() -> Self {
        Self {
            metadata: DashMap::new(),
        }
    }

    fn get(&self, chapter_id: &str) -> Option<Arc<DownloadedPageTransformMetadata>> {
        self.metadata
            .get(chapter_id)
            .map(|entry| Arc::clone(entry.value()))
    }

    fn insert(
        &self,
        chapter_id: &str,
        metadata: DownloadedPageTransformMetadata,
    ) -> Arc<DownloadedPageTransformMetadata> {
        let metadata = Arc::new(metadata);
        self.metadata
            .insert(chapter_id.to_string(), Arc::clone(&metadata));
        metadata
    }

    pub(crate) fn clear(&self) {
        self.metadata.clear();
    }
}

impl Default for DownloadedPageTransformCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct DownloadedPageTransformMetadata {
    transforms: Vec<Option<MediaTransformSpec>>,
    avif_quality: u8,
}

struct ResolvedDownloadedPageTransformMetadata {
    metadata: DownloadedPageTransformMetadata,
    cacheable: bool,
}

impl DownloadedPageTransformMetadata {
    fn empty() -> Self {
        Self {
            transforms: Vec::new(),
            avif_quality: DEFAULT_DOWNLOAD_TRANSFORM_AVIF_QUALITY,
        }
    }

    fn transform_for(&self, page: usize) -> Option<MediaTransformSpec> {
        self.transforms.get(page).cloned().flatten()
    }
}

enum DownloadedPageExtractResult {
    Completed {
        requested: DownloadedChapterPage,
        extracted_pages: usize,
    },
    Dropped,
}

impl DownloadedPageExtractResult {
    fn extracted_pages(&self) -> usize {
        match self {
            Self::Completed {
                extracted_pages, ..
            } => *extracted_pages,
            Self::Dropped => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DownloadedPageReadOptions {
    pub skip_page_cache: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SourceChapterPageReferenceOptions {
    pub warm_current_chapter: bool,
    pub warm_next_chapter: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DownloadedChapterPageReferenceOptions {
    pub skip_page_cache: bool,
}

pub struct DownloadedChapterPage {
    pub body: axum::body::Bytes,
    pub content_type: &'static str,
}

pub struct SourceChapterPage {
    pub reference: SourceChapterPageReference,
    pub body: Bytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadedChapterPageReference {
    pub index: usize,
    pub chapter_id: String,
}

#[derive(
    Clone,
    Debug,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    PartialEq,
    Eq,
)]
pub struct SourceChapterPageReference {
    pub index: usize,
    pub source: String,
    media: SourceChapterPageMedia,
}

#[derive(
    Clone,
    Debug,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    PartialEq,
    Eq,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SourceChapterPageMedia {
    Direct { url: String },
    Request { spec: MediaRefSpec },
}

impl SourceChapterPageReference {
    #[must_use]
    pub fn fallback_url(&self) -> &str {
        match &self.media {
            SourceChapterPageMedia::Direct { url } => url,
            SourceChapterPageMedia::Request { spec } => &spec.url,
        }
    }

    pub fn route_url(
        &self,
        source_base_url: Option<&str>,
        proxied_images: bool,
    ) -> Result<String, AppError> {
        match &self.media {
            SourceChapterPageMedia::Direct { url } => {
                if proxied_images {
                    let url = crate::app::media::normalize_page_url(url, source_base_url)?;
                    return Ok(crate::app::media::direct_media_proxy_url(
                        &self.source,
                        &url,
                    ));
                }
                Ok(url.clone())
            }
            SourceChapterPageMedia::Request { spec } => {
                let encoded = encode_media_spec(spec)?;
                Ok(crate::app::media::media_proxy_url(&self.source, &encoded))
            }
        }
    }

    fn media_spec(&self) -> MediaRefSpec {
        match &self.media {
            SourceChapterPageMedia::Direct { url } => MediaRefSpec {
                url: url.clone(),
                request: None,
                transform: None,
            },
            SourceChapterPageMedia::Request { spec } => spec.clone(),
        }
    }

    fn primary_cache_key(&self) -> anyhow::Result<String> {
        match &self.media {
            SourceChapterPageMedia::Direct { url } => Ok(url.clone()),
            SourceChapterPageMedia::Request { spec } => encode_media_spec(spec),
        }
    }

    fn cache_lookup_keys(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![self.primary_cache_key()?])
    }
}

fn validate_source_page_image(
    reference: &SourceChapterPageReference,
    body: &[u8],
    content_type: Option<&str>,
    cache_key: Option<&str>,
) -> anyhow::Result<&'static str> {
    let content_type = content_type.unwrap_or("unknown");
    let cache_key = cache_key.unwrap_or("uncached");
    backend_image::detect_supported_image_format(body).with_context(|| {
        format!(
            "invalid image bytes for source page {} from {} (content_type={content_type}, cache_key={cache_key}, bytes={})",
            reference.index + 1,
            reference.fallback_url(),
            body.len()
        )
    })
}

#[autometrics]
pub async fn source_chapter_page_references(
    source_registry: &RwLock<SourceRegistry>,
    cache: &MangaCache,
    source: String,
    chapter_id: String,
) -> Result<ApiListResponse<SourceChapterPageReference>, AppError> {
    source_chapter_page_references_with_ttl(source_registry, cache, source, chapter_id, None).await
}

#[autometrics]
pub async fn source_chapter_page_references_with_ttl(
    source_registry: &RwLock<SourceRegistry>,
    cache: &MangaCache,
    source: String,
    chapter_id: String,
    ttl: Option<Duration>,
) -> Result<ApiListResponse<SourceChapterPageReference>, AppError> {
    let cache_key = backend_cache::CacheKey::PageRefs {
        plugin: source.clone(),
        chapter_id: chapter_id.clone(),
        schema_version: SOURCE_PAGE_REFS_CACHE_SCHEMA_VERSION,
    };
    if let Some(cached) = cache
        .get_typed::<ApiListResponse<SourceChapterPageReference>>(&cache_key)
        .await
    {
        return Ok(cached);
    }

    let pages = {
        let pm = source_registry.read().await;
        pm.get_page_list(&source, &chapter_id).await?
    };
    let response = ApiListResponse::new(
        pages
            .into_iter()
            .enumerate()
            .map(|(index, page)| {
                let media = if page.image.request.is_some() {
                    SourceChapterPageMedia::Request {
                        spec: media_ref_to_spec(&page.image)?,
                    }
                } else {
                    SourceChapterPageMedia::Direct {
                        url: page.image.url,
                    }
                };
                Ok(SourceChapterPageReference {
                    index,
                    source: source.clone(),
                    media,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?,
    );
    if let Some(ttl) = ttl {
        cache.insert_typed_ttl(cache_key, &response, ttl);
    } else {
        cache.insert_typed(cache_key, &response);
    }
    Ok(response)
}

#[autometrics]
pub async fn source_chapter_page_references_for_reader(
    state: &Arc<AppState>,
    source: String,
    chapter_id: String,
    options: SourceChapterPageReferenceOptions,
) -> Result<ApiListResponse<SourceChapterPageReference>, AppError> {
    let pages = source_chapter_page_references(
        &state.source_registry,
        &state.cache,
        source.clone(),
        chapter_id.clone(),
    )
    .await?;

    if options.warm_current_chapter {
        warm_current_source_chapter_pages(
            Arc::clone(state),
            source.clone(),
            chapter_id.clone(),
            pages.items.clone(),
        );
    }
    if options.warm_next_chapter {
        warm_next_source_chapter(Arc::clone(state), source, chapter_id);
    }

    Ok(pages)
}

#[autometrics]
pub async fn warm_source_chapter_page(
    cache: &MangaCache,
    metrics: &backend_telemetry::Metrics,
    source_registry: &RwLock<SourceRegistry>,
    reference: &SourceChapterPageReference,
    source_base_url: Option<&str>,
    ttl: Duration,
) -> Result<(), AppError> {
    match &reference.media {
        SourceChapterPageMedia::Direct { url } => {
            crate::app::media::precache_direct_image_url(
                cache,
                metrics,
                source_registry,
                url,
                &reference.source,
                source_base_url,
                ttl,
            )
            .await
        }
        SourceChapterPageMedia::Request { spec } => {
            crate::app::media::precache_media_spec(
                cache,
                metrics,
                source_registry,
                spec,
                &reference.source,
                ttl,
            )
            .await
        }
    }
}

#[autometrics]
pub async fn read_source_chapter_page(
    cache: &MangaCache,
    metrics: &backend_telemetry::Metrics,
    media_client: &SourceMediaClient,
    reference: SourceChapterPageReference,
) -> anyhow::Result<SourceChapterPage> {
    for key in reference.cache_lookup_keys()? {
        if let Some(cached) = cache.get_image(&key).await
            && !cached.body.is_empty()
        {
            let detected_format = match validate_source_page_image(
                &reference,
                &cached.body,
                Some(&cached.content_type),
                Some(&key),
            ) {
                Ok(format) => format,
                Err(error) => {
                    let error_chain = format!("{error:#}");
                    cache.remove_image(&key);
                    metrics.record_page_fetch_cache(&reference.source, "invalid");
                    tracing::warn!(
                        source = %reference.source,
                        page = reference.index + 1,
                        url = %reference.fallback_url(),
                        cache_key = %key,
                        content_type = %cached.content_type,
                        bytes = cached.body.len(),
                        error = %error_chain,
                        "Source Chapter Page Cache Entry Invalid; Refetching",
                    );
                    continue;
                }
            };
            metrics.record_page_fetch_cache(&reference.source, "hit");
            tracing::trace!(
                source = %reference.source,
                page = reference.index + 1,
                url = %reference.fallback_url(),
                cache_key = %key,
                content_type = %cached.content_type,
                detected_format,
                bytes = cached.body.len(),
                "Source Chapter Page Cache Hit",
            );
            return Ok(SourceChapterPage {
                reference,
                body: cached.body,
            });
        }
    }

    metrics.record_page_fetch_cache(&reference.source, "miss");
    let media = reference.media_spec();
    let (body, content_type) = media_client
        .fetch_media(&media, RequestProfile::ImageHotlink)
        .await?;
    let detected_format = validate_source_page_image(&reference, &body, Some(&content_type), None)?;
    let body = Bytes::from(body);
    cache.insert_image(
        reference.primary_cache_key()?,
        CachedImage {
            content_type: content_type.clone(),
            body: body.clone(),
        },
    );
    tracing::trace!(
        source = %reference.source,
        page = reference.index + 1,
        url = %reference.fallback_url(),
        content_type = %content_type,
        detected_format,
        bytes = body.len(),
        "Source Chapter Page Fetched",
    );

    Ok(SourceChapterPage { reference, body })
}

fn warm_current_source_chapter_pages(
    state: Arc<AppState>,
    source: String,
    chapter_source_id: String,
    pages: Vec<SourceChapterPageReference>,
) {
    tokio::spawn(async move {
        let cached_images = warm_source_chapter_pages(
            &state,
            &source,
            current_source_chapter_warming_references(pages),
            CURRENT_CHAPTER_READ_AHEAD_TTL,
        )
        .await;
        tracing::debug!(
            source = %source,
            chapter_source_id = %chapter_source_id,
            cached_images,
            "Current Source Chapter Page Warming Completed",
        );
    });
}

fn warm_next_source_chapter(state: Arc<AppState>, source: String, chapter_source_id: String) {
    tokio::spawn(async move {
        if let Err(error) =
            warm_next_source_chapter_pages(&state, &source, &chapter_source_id).await
        {
            tracing::debug!(
                source = %source,
                chapter_source_id = %chapter_source_id,
                error = %error,
                "Next Source Chapter Page Warming Failed",
            );
        }
    });
}

#[autometrics(track_concurrency)]
async fn warm_next_source_chapter_pages(
    state: &Arc<AppState>,
    source: &str,
    chapter_source_id: &str,
) -> Result<(), AppError> {
    let Some(next) = state
        .db
        .get_next_chapter_by_source_chapter(source, chapter_source_id)
        .await?
    else {
        return Ok(());
    };

    let source_base_url = {
        let pm = state.source_registry.read().await;
        pm.source_base_url(source).ok().map(Arc::<str>::from)
    };
    let pages = source_chapter_page_references_with_ttl(
        &state.source_registry,
        &state.cache,
        source.to_string(),
        next.source_id.clone(),
        Some(NEXT_CHAPTER_READ_AHEAD_TTL),
    )
    .await?;

    let cached_images = warm_source_chapter_pages_with_base_url(
        state,
        source_base_url,
        pages.items,
        NEXT_CHAPTER_READ_AHEAD_TTL,
    )
    .await;

    tracing::debug!(
        source,
        current_chapter_source_id = %chapter_source_id,
        next_chapter_id = %next.id,
        next_chapter_source_id = %next.source_id,
        cached_images,
        "Next Source Chapter Page Warming Completed",
    );
    Ok(())
}

#[autometrics(track_concurrency)]
async fn warm_source_chapter_pages(
    state: &Arc<AppState>,
    source: &str,
    pages: Vec<SourceChapterPageReference>,
    ttl: Duration,
) -> usize {
    let source_base_url = {
        let pm = state.source_registry.read().await;
        pm.source_base_url(source).ok().map(Arc::<str>::from)
    };
    warm_source_chapter_pages_with_base_url(state, source_base_url, pages, ttl).await
}

#[autometrics(track_concurrency)]
async fn warm_source_chapter_pages_with_base_url(
    state: &Arc<AppState>,
    source_base_url: Option<Arc<str>>,
    pages: Vec<SourceChapterPageReference>,
    ttl: Duration,
) -> usize {
    stream::iter(pages.into_iter().map(|page| {
        let state = Arc::clone(state);
        let source_base_url = source_base_url.as_ref().map(Arc::clone);
        async move {
            warm_source_chapter_page(
                &state.cache,
                &state.telemetry.metrics,
                &state.source_registry,
                &page,
                source_base_url.as_deref(),
                ttl,
            )
            .await
        }
    }))
    .buffer_unordered(MAX_CONCURRENT_IMAGE_READ_AHEAD)
    .fold(0usize, |cached, result| async move {
        cached + usize::from(result.is_ok())
    })
    .await
}

#[autometrics(track_concurrency)]
async fn warm_next_downloaded_chapter_pages(
    state: &Arc<AppState>,
    chapter_id: &str,
) -> Result<(), AppError> {
    let Some(next) = state.db.get_next_chapter_by_id(chapter_id).await? else {
        return Ok(());
    };
    if !next.downloaded {
        return Ok(());
    }

    let cached_pages = downloaded_reader::warm_downloaded_chapter_start(
        state,
        &next.id,
        NEXT_CHAPTER_READ_AHEAD_TTL,
    )
    .await?;

    tracing::debug!(
        current_chapter_id = %chapter_id,
        next_chapter_id = %next.id,
        cached_pages,
        "Next Downloaded Chapter Page Warming Completed",
    );
    Ok(())
}

fn current_source_chapter_warming_references(
    pages: Vec<SourceChapterPageReference>,
) -> Vec<SourceChapterPageReference> {
    pages.into_iter().skip(1).collect()
}

#[autometrics]
pub async fn downloaded_chapter_page_references(
    db: &Database,
    archive_index: &crate::archive_index::ArchiveIndexService,
    metrics: &backend_telemetry::Metrics,
    chapter_id: &str,
) -> Result<ApiListResponse<DownloadedChapterPageReference>, AppError> {
    let archive = archive_index
        .get_for_chapter(db, metrics, chapter_id, "lazy")
        .await?;

    Ok(ApiListResponse::new(
        (0..archive.page_count())
            .map(|index| DownloadedChapterPageReference {
                index,
                chapter_id: chapter_id.to_string(),
            })
            .collect(),
    ))
}

#[autometrics]
pub async fn downloaded_chapter_page_references_for_reader(
    state: &Arc<AppState>,
    chapter_id: &str,
    options: DownloadedChapterPageReferenceOptions,
) -> Result<ApiListResponse<DownloadedChapterPageReference>, AppError> {
    downloaded_reader::downloaded_chapter_page_references_for_reader(state, chapter_id, options)
        .await
}

#[autometrics]
pub async fn read_downloaded_chapter_page(
    state: &AppState,
    chapter_id: &str,
    page: usize,
    options: DownloadedPageReadOptions,
) -> Result<DownloadedChapterPage, AppError> {
    downloaded_reader::read_downloaded_chapter_page(state, chapter_id, page, options).await
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn extract_downloaded_page_window(
    state: &AppState,
    cache: &MangaCache,
    extraction_scheduler: &DownloadedPageExtractionScheduler,
    metrics: &backend_telemetry::Metrics,
    archive: Arc<crate::archive_index::ResolvedArchiveIndex>,
    chapter_id: Option<&str>,
    requested_page: usize,
    window_size: usize,
    probe_cached_neighbors: bool,
    ttl: Option<Duration>,
    mode: &'static str,
    skip_page_cache: bool,
) -> Result<DownloadedPageExtractResult, AppError> {
    if requested_page >= archive.page_count() {
        return Err(AppError::not_found(anyhow::anyhow!(
            "page {requested_page} out of bounds"
        )));
    }

    let archive_key = archive.extraction_key();
    let (window_key, window_lock) = extraction_scheduler
        .extraction_window_lock(&archive_key, requested_page, window_size)
        .await;
    let (window_guard, lock_waited) = match window_lock.try_lock() {
        Ok(guard) => (guard, false),
        Err(_) => (window_lock.lock().await, true),
    };

    let result = async {
        if !skip_page_cache && lock_waited {
            let requested_entry = archive.page(requested_page).ok_or_else(|| {
                AppError::not_found(anyhow::anyhow!("page {requested_page} out of bounds"))
            })?;
            if let Some(cached) = cache.get_image(&requested_entry.cache_key).await {
                let requested = DownloadedChapterPage {
                    body: cached.body,
                    content_type: requested_entry.content_type,
                };
                let requested =
                    apply_downloaded_source_transform(state, chapter_id, requested_page, requested)
                        .await?;
                return Ok(DownloadedPageExtractResult::Completed {
                    requested,
                    extracted_pages: 0,
                });
            }
        }

        let targets = missing_downloaded_page_window(
            cache,
            &archive,
            requested_page,
            window_size,
            !skip_page_cache && probe_cached_neighbors,
        )
        .await?;
        if targets.is_empty() {
            return Err(AppError::internal(anyhow::anyhow!(
                "downloaded page extraction window had no targets"
            )));
        }

        let archive_key = archive.extraction_key();
        let archive_path = archive.archive_path.clone();
        let started = Instant::now();

        let target_positions = targets
            .iter()
            .map(|target| PositionedImageTarget {
                file_position: target.file_position,
                content_type: target.indexed_content_type,
            })
            .collect::<Vec<_>>();
        let extraction_mode = if mode == "read_ahead" {
            ExtractionRequestMode::ReadAhead
        } else {
            ExtractionRequestMode::Foreground
        };
        let extracted_result = extraction_scheduler
            .extract_positioned_images(
                metrics,
                archive_key,
                archive_path,
                target_positions,
                extraction_mode,
            )
            .await;

        let extracted_entries = match extracted_result {
            Ok(PositionedExtractionResult::Completed(entries)) => entries,
            Ok(PositionedExtractionResult::DroppedQueuePressure) => {
                metrics.record_downloaded_page_extract(
                    mode,
                    "dropped_queue_pressure",
                    started.elapsed(),
                    targets.len(),
                );
                return Ok(DownloadedPageExtractResult::Dropped);
            }
            Ok(PositionedExtractionResult::StaleArchive) => {
                downloaded_archive_derived_state::stale_downloaded_archive_identity_detected(
                    state,
                    &archive.archive_path,
                    chapter_id,
                    mode,
                )
                .await;

                metrics.record_downloaded_page_extract(
                    mode,
                    "stale_identity",
                    started.elapsed(),
                    targets.len(),
                );
                if mode == "read_ahead" {
                    return Ok(DownloadedPageExtractResult::Dropped);
                }
                return Err(AppError::downloaded_page_conflict(anyhow::anyhow!(
                    "downloaded archive changed while reading page"
                )));
            }
            Err(error) => {
                metrics.record_downloaded_page_extract(
                    mode,
                    "error",
                    started.elapsed(),
                    targets.len(),
                );
                return Err(AppError::internal(error));
            }
        };

        if !archive.identity_is_current() {
            downloaded_archive_derived_state::stale_downloaded_archive_identity_detected(
                state,
                &archive.archive_path,
                chapter_id,
                mode,
            )
            .await;

            if mode == "read_ahead" {
                metrics.record_downloaded_page_extract(
                    mode,
                    "stale_identity",
                    started.elapsed(),
                    targets.len(),
                );
                return Ok(DownloadedPageExtractResult::Dropped);
            }

            metrics.record_downloaded_page_extract(
                mode,
                "stale_identity",
                started.elapsed(),
                targets.len(),
            );
            return Err(AppError::downloaded_page_conflict(anyhow::anyhow!(
                "downloaded archive changed while reading page"
            )));
        }

        metrics.record_downloaded_page_extract(
            mode,
            "success",
            started.elapsed(),
            extracted_entries.len(),
        );

        let response_build_started = Instant::now();
        let mut requested = None;
        let mut extracted_pages = 0usize;
        for (target, entry) in targets.into_iter().zip(extracted_entries) {
            let (image, response_content_type) =
                cached_image_from_entry(entry, target.indexed_content_type);
            if target.page == requested_page {
                requested = Some(DownloadedChapterPage {
                    body: image.body.clone(),
                    content_type: response_content_type,
                });
            }
            if !skip_page_cache {
                if let Some(ttl) = ttl {
                    cache.insert_image_ttl(target.cache_key, image, ttl);
                } else {
                    cache.insert_image(target.cache_key, image);
                }
            }
            extracted_pages += 1;
        }
        metrics.record_downloaded_page_response_build(
            mode,
            response_build_started.elapsed(),
            extracted_pages,
        );

        let requested = requested.ok_or_else(|| {
            AppError::internal(anyhow::anyhow!(
                "downloaded page extraction did not include requested page"
            ))
        })?;
        let requested =
            apply_downloaded_source_transform(state, chapter_id, requested_page, requested).await?;

        Ok(DownloadedPageExtractResult::Completed {
            requested,
            extracted_pages,
        })
    }
    .await;

    drop(window_guard);
    extraction_scheduler
        .release_extraction_window_lock(&window_key, &window_lock)
        .await;

    result
}

async fn missing_downloaded_page_window(
    cache: &MangaCache,
    archive: &crate::archive_index::ResolvedArchiveIndex,
    requested_page: usize,
    window_size: usize,
    probe_cached_neighbors: bool,
) -> Result<Vec<DownloadedPageExtractTarget>, AppError> {
    let window_start = downloaded_page_window_start(requested_page, window_size);
    let window_end = downloaded_page_window_end(requested_page, archive.page_count(), window_size);
    let mut targets = Vec::with_capacity(window_end.saturating_sub(window_start));

    for page in window_start..window_end {
        let page_entry = archive
            .page(page)
            .ok_or_else(|| AppError::not_found(anyhow::anyhow!("page {page} out of bounds")))?;
        if probe_cached_neighbors
            && page != requested_page
            && cache.get_image(&page_entry.cache_key).await.is_some()
        {
            continue;
        }

        targets.push(DownloadedPageExtractTarget {
            page,
            cache_key: page_entry.cache_key.clone(),
            indexed_content_type: page_entry.content_type,
            file_position: page_entry.file_position,
        });
    }

    Ok(targets)
}

fn cached_image_from_entry(
    entry: backend_image::EntryBytes,
    indexed_content_type: &'static str,
) -> (CachedImage, &'static str) {
    let content_type = if entry.content_type == "application/octet-stream" {
        indexed_content_type
    } else {
        entry.content_type
    };
    (
        CachedImage {
            content_type: content_type.to_string(),
            body: entry.bytes.into(),
        },
        content_type,
    )
}

async fn apply_downloaded_source_transform(
    state: &AppState,
    chapter_id: Option<&str>,
    page: usize,
    response: DownloadedChapterPage,
) -> Result<DownloadedChapterPage, AppError> {
    let Some(chapter_id) = chapter_id else {
        return Ok(response);
    };
    let metadata = downloaded_page_transform_metadata(state, chapter_id).await?;
    let Some(transform) = metadata.transform_for(page) else {
        return Ok(response);
    };
    if !should_apply_downloaded_source_transform(&transform, response.content_type) {
        return Ok(response);
    }

    match transform {
        MediaTransformSpec::ComixDescramble5x5 | MediaTransformSpec::ComixDescramble5x5Map(_) => {
            let quality = metadata.avif_quality;
            let input = response.body;
            let body = tokio::task::spawn_blocking(move || {
                let png = match transform {
                    MediaTransformSpec::ComixDescramble5x5 => {
                        backend_image::descramble_comix_5x5_to_png(&input)?
                    }
                    MediaTransformSpec::ComixDescramble5x5Map(map) => {
                        let map: [usize; 25] = map.as_slice().try_into().map_err(|_| {
                            anyhow::anyhow!("Comix descramble map must contain 25 tiles")
                        })?;
                        backend_image::descramble_comix_5x5_with_map_to_png(&input, &map)?
                    }
                };
                backend_image::convert_to_avif(&png, quality)
            })
            .await
            .map_err(|error| {
                AppError::internal(anyhow::anyhow!(
                    "downloaded page transform task failed: {error}"
                ))
            })?
            .map_err(AppError::internal)?;

            Ok(DownloadedChapterPage {
                body: body.into(),
                content_type: "image/avif",
            })
        }
    }
}

fn should_apply_downloaded_source_transform(
    transform: &MediaTransformSpec,
    content_type: &str,
) -> bool {
    match transform {
        MediaTransformSpec::ComixDescramble5x5 | MediaTransformSpec::ComixDescramble5x5Map(_) => {
            !content_type_is_avif(content_type)
        }
    }
}

fn content_type_is_avif(content_type: &str) -> bool {
    backend_core::is_avif_content_type(content_type)
}

async fn downloaded_page_transform_metadata(
    state: &AppState,
    chapter_id: &str,
) -> Result<Arc<DownloadedPageTransformMetadata>, AppError> {
    if let Some(metadata) = state.downloaded_page_transform_cache.get(chapter_id) {
        return Ok(metadata);
    }

    let resolved = resolve_downloaded_page_transform_metadata(state, chapter_id).await?;
    if !resolved.cacheable {
        return Ok(Arc::new(resolved.metadata));
    }

    Ok(state
        .downloaded_page_transform_cache
        .insert(chapter_id, resolved.metadata))
}

async fn resolve_downloaded_page_transform_metadata(
    state: &AppState,
    chapter_id: &str,
) -> Result<ResolvedDownloadedPageTransformMetadata, AppError> {
    let Some(chapter) = state.db.get_chapter_by_id(chapter_id).await? else {
        return Ok(ResolvedDownloadedPageTransformMetadata {
            metadata: DownloadedPageTransformMetadata::empty(),
            cacheable: true,
        });
    };
    let Some(manga) = state.db.get_manga_by_id(&chapter.manga_id).await? else {
        return Ok(ResolvedDownloadedPageTransformMetadata {
            metadata: DownloadedPageTransformMetadata::empty(),
            cacheable: true,
        });
    };
    let refs = match source_chapter_page_references(
        &state.source_registry,
        &state.cache,
        manga.source.clone(),
        chapter.source_id,
    )
    .await
    {
        Ok(refs) => refs,
        Err(error) => {
            tracing::debug!(
                chapter_id,
                source = %manga.source,
                error = %error,
                "Downloaded Page Transform Metadata Unavailable",
            );
            return Ok(ResolvedDownloadedPageTransformMetadata {
                metadata: DownloadedPageTransformMetadata::empty(),
                cacheable: false,
            });
        }
    };

    let transforms = refs
        .items
        .iter()
        .map(|reference| reference.media_spec().transform)
        .collect::<Vec<_>>();
    let avif_quality = if transforms.iter().any(Option::is_some) {
        crate::app::settings::source_avif_quality(&state.db, &manga.source)
            .await
            .map_err(AppError::from)?
    } else {
        DEFAULT_DOWNLOAD_TRANSFORM_AVIF_QUALITY
    };

    Ok(ResolvedDownloadedPageTransformMetadata {
        metadata: DownloadedPageTransformMetadata {
            transforms,
            avif_quality,
        },
        cacheable: true,
    })
}

fn downloaded_page_bucket(page: usize) -> &'static str {
    match page {
        0 => "0",
        1..=9 => "1-9",
        10..=49 => "10-49",
        50..=99 => "50-99",
        100..=199 => "100-199",
        _ => "200+",
    }
}

fn downloaded_page_window_start(page: usize, window_size: usize) -> usize {
    let window_size = window_size.max(1);
    (page / window_size) * window_size
}

fn downloaded_page_window_end(page: usize, page_count: usize, window_size: usize) -> usize {
    let window_start = downloaded_page_window_start(page, window_size);
    window_start
        .saturating_add(window_size.max(1))
        .min(page_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct_reference(index: usize) -> SourceChapterPageReference {
        SourceChapterPageReference {
            index,
            source: "demo".to_string(),
            media: SourceChapterPageMedia::Direct {
                url: format!("https://img.example/page-{}.jpg", index + 1),
            },
        }
    }

    fn request_spec() -> MediaRefSpec {
        MediaRefSpec {
            url: "https://img.example/page-1.jpg".to_string(),
            request: Some(backend_sources::media::FetchRequestSpec {
                url: "https://img.example/page-1.jpg".to_string(),
                method: "GET".to_string(),
                headers: Vec::new(),
                body: None,
                purpose: backend_sources::media::RequestPurposeSpec::Image,
            }),
            transform: None,
        }
    }

    #[test]
    fn direct_source_chapter_page_reference_renders_proxy_url() {
        let reference = direct_reference(0);

        let route_url = reference.route_url(None, true).unwrap();
        let query = route_url
            .strip_prefix("/v1/media/image?")
            .expect("route URL should point at media proxy")
            .trim_start_matches('&');
        let pairs = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(pairs.get("source").map(String::as_str), Some("demo"));
        assert_eq!(
            pairs.get("url").map(String::as_str),
            Some("https://img.example/page-1.jpg")
        );
    }

    #[test]
    fn request_source_chapter_page_reference_renders_spec_proxy_url() {
        let spec = request_spec();
        let reference = SourceChapterPageReference {
            index: 0,
            source: "demo".to_string(),
            media: SourceChapterPageMedia::Request { spec: spec.clone() },
        };

        let route_url = reference.route_url(None, true).unwrap();
        let query = route_url
            .strip_prefix("/v1/media/image?")
            .expect("route URL should point at media proxy");
        let pairs = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect::<std::collections::HashMap<_, _>>();
        let decoded =
            backend_sources::media::decode_media_spec(pairs.get("spec").unwrap()).unwrap();

        assert_eq!(pairs.get("source").map(String::as_str), Some("demo"));
        assert_eq!(decoded, spec);
    }

    #[test]
    fn request_source_chapter_page_reference_uses_only_spec_cache_key() {
        let spec = request_spec();
        let reference = SourceChapterPageReference {
            index: 0,
            source: "demo".to_string(),
            media: SourceChapterPageMedia::Request { spec: spec.clone() },
        };

        let keys = reference.cache_lookup_keys().unwrap();

        assert_eq!(keys[0], encode_media_spec(&spec).unwrap());
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn direct_source_chapter_page_reference_uses_url_cache_key() {
        let reference = direct_reference(0);

        let keys = reference.cache_lookup_keys().unwrap();

        assert_eq!(keys, vec!["https://img.example/page-1.jpg".to_string()]);
    }

    #[test]
    fn current_source_chapter_warming_skips_first_reference() {
        let pages = vec![
            direct_reference(0),
            direct_reference(1),
            direct_reference(2),
        ];

        let warming = current_source_chapter_warming_references(pages);

        assert_eq!(
            warming
                .iter()
                .map(|reference| reference.index)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn downloaded_source_transform_skips_already_converted_avif_pages() {
        assert!(!should_apply_downloaded_source_transform(
            &MediaTransformSpec::ComixDescramble5x5,
            "image/avif"
        ));
        assert!(!should_apply_downloaded_source_transform(
            &MediaTransformSpec::ComixDescramble5x5Map((0..25).collect()),
            "image/avif; charset=binary"
        ));
    }

    #[test]
    fn downloaded_source_transform_applies_to_raw_source_images() {
        assert!(should_apply_downloaded_source_transform(
            &MediaTransformSpec::ComixDescramble5x5,
            "image/webp"
        ));
    }
}
