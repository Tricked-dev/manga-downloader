use crate::{
    AppState,
    api::{dto::ApiListResponse, error::AppError},
};
use anyhow::Context;
use autometrics::autometrics;
use axum::body::Bytes;
use backend_cache::{CachedImage, MangaCache};
use backend_persistence::Database;
use backend_sources::{
    SourceMediaClient, SourceRegistry,
    fetch::RequestProfile,
    media::{MediaRefSpec, encode_media_spec, media_ref_to_spec},
};
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;

mod downloaded_reader;

const NEXT_CHAPTER_READ_AHEAD_TTL: Duration = Duration::from_mins(15);
const CURRENT_CHAPTER_READ_AHEAD_TTL: Duration = Duration::from_mins(5);
const MAX_CONCURRENT_IMAGE_READ_AHEAD: usize = 4;
const SOURCE_PAGE_REFS_CACHE_SCHEMA_VERSION: u8 = 9;

#[derive(Clone, Copy, Debug, Default)]
pub struct DownloadedPageReadOptions {
    pub skip_page_cache: bool,
    pub variant: Option<backend_storage::PageVariant>,
    pub format: Option<super::media::MediaProxyFormat>,
    pub width: Option<u32>,
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
    pub variant: backend_storage::PageVariant,
    pub cache_hit: bool,
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

fn current_source_chapter_warming_references(
    pages: Vec<SourceChapterPageReference>,
) -> Vec<SourceChapterPageReference> {
    pages.into_iter().skip(1).collect()
}

#[autometrics]
pub async fn downloaded_chapter_page_references(
    db: &Database,
    chapter_id: &str,
) -> Result<ApiListResponse<DownloadedChapterPageReference>, AppError> {
    let archive = super::downloaded_archive_resolution::existing_completed_archive_for_chapter(
        db, chapter_id,
    )
    .await?;
    let info = backend_storage::inspect(archive.archive_path).await?;

    Ok(ApiListResponse::new(
        (0..info.page_count)
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
}
