use crate::api::error::AppError;
use anyhow::Context;
use autometrics::autometrics;
use axum::body::Bytes;
use backend_cache::{CachedImage, MangaCache};
use backend_sources::{
    SourceRegistry,
    fetch::RequestProfile,
    media::{MediaRefSpec, decode_media_spec, encode_media_spec},
};
use backend_telemetry::{Metrics, trace};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::Instrument as _;

pub enum MediaProxyResult {
    Redirect(String),
    Bytes {
        content_type: String,
        body: axum::body::Bytes,
        cache_hit: bool,
    },
}

pub use backend_image::OutputFormat as MediaProxyFormat;

const MEDIA_PROXY_FETCH_TIMEOUT: Duration = Duration::from_secs(8);

pub(crate) fn media_proxy_url(source: &str, spec: &str) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new("/v1/media/image?".to_string());
    serializer.append_pair("source", source);
    serializer.append_pair("spec", spec);
    serializer.finish()
}

pub(crate) fn direct_media_proxy_url(source: &str, url: &str) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new("/v1/media/image?".to_string());
    serializer.append_pair("source", source);
    serializer.append_pair("url", url);
    serializer.finish()
}

#[allow(clippy::too_many_arguments)]
#[autometrics(track_concurrency)]
#[tracing::instrument(name = "app.media.proxy_image", skip_all, fields(source = %source.as_deref().unwrap_or("unscoped"), format = %media_proxy_format_label(format), width = tracing::field::Empty, skip_cache, outcome = tracing::field::Empty))]
pub async fn proxy_image(
    cache: &MangaCache,
    metrics: &Metrics,
    source_registry: &RwLock<SourceRegistry>,
    url: Option<String>,
    spec: Option<String>,
    source: Option<String>,
    format: Option<MediaProxyFormat>,
    width: Option<u32>,
    skip_cache: bool,
) -> Result<MediaProxyResult, AppError> {
    if let Some(width) = width {
        tracing::Span::current().record("width", width);
    }
    let request = ImageProxyRequest {
        url,
        spec,
        source,
        format,
        width,
        skip_cache,
    };

    proxy_image_with_ttl(cache, metrics, source_registry, request, None).await
}

#[autometrics]
#[tracing::instrument(name = "background.media.precache_direct_url", skip_all, fields(source = %source, outcome = tracing::field::Empty))]
pub async fn precache_direct_image_url(
    cache: &MangaCache,
    metrics: &Metrics,
    source_registry: &RwLock<SourceRegistry>,
    page_url: &str,
    source: &str,
    source_base_url: Option<&str>,
    ttl: Duration,
) -> Result<(), AppError> {
    let request = ImageProxyRequest {
        url: Some(normalize_page_url(page_url, source_base_url)?),
        spec: None,
        source: Some(source.to_string()),
        format: None,
        width: None,
        skip_cache: false,
    };
    let _ = proxy_image_with_ttl(cache, metrics, source_registry, request, Some(ttl)).await?;
    trace::record_outcome(&tracing::Span::current(), "success");
    Ok(())
}

#[autometrics]
#[tracing::instrument(name = "background.media.precache_spec", skip_all, fields(source = %source, outcome = tracing::field::Empty))]
pub async fn precache_media_spec(
    cache: &MangaCache,
    metrics: &Metrics,
    source_registry: &RwLock<SourceRegistry>,
    spec: &MediaRefSpec,
    source: &str,
    ttl: Duration,
) -> Result<(), AppError> {
    let request = ImageProxyRequest {
        url: None,
        spec: Some(encode_media_spec(spec)?),
        source: Some(source.to_string()),
        format: None,
        width: None,
        skip_cache: false,
    };
    let _ = proxy_image_with_ttl(cache, metrics, source_registry, request, Some(ttl)).await?;
    trace::record_outcome(&tracing::Span::current(), "success");
    Ok(())
}

#[autometrics(track_concurrency)]
#[tracing::instrument(name = "app.media.proxy_pipeline", skip_all, fields(source = tracing::field::Empty, format = tracing::field::Empty, skip_cache = tracing::field::Empty, ttl_seconds = tracing::field::Empty, outcome = tracing::field::Empty, duration_ms = tracing::field::Empty))]
async fn proxy_image_with_ttl(
    cache: &MangaCache,
    metrics: &Metrics,
    source_registry: &RwLock<SourceRegistry>,
    request: ImageProxyRequest,
    ttl: Option<Duration>,
) -> Result<MediaProxyResult, AppError> {
    let total_started = Instant::now();
    let span = tracing::Span::current();
    let ImageProxyRequest {
        url,
        spec,
        source,
        format,
        width,
        skip_cache,
    } = request;
    let source_label = source.as_deref().unwrap_or("unscoped").to_string();
    let format_label = media_proxy_format_label(format);
    span.record("source", tracing::field::display(&source_label));
    span.record("format", format_label);
    span.record("skip_cache", skip_cache);
    if let Some(ttl) = ttl {
        span.record("ttl_seconds", ttl.as_secs());
    }
    let source_cache_key = spec.as_ref().or(url.as_ref()).cloned().ok_or_else(|| {
        AppError::bad_request(anyhow::anyhow!("Missing url or spec query parameter"))
    })?;
    let cache_key = media_cache_key(&source_cache_key, format, width);
    let cache_lookup_started = Instant::now();
    let cache_span = trace::cache_lookup_span("media_image", "image_proxy");

    if skip_cache {
        metrics.record_media_proxy_stage(
            &source_label,
            format_label,
            "cache_lookup",
            "skipped",
            cache_lookup_started.elapsed(),
        );
        trace::record_result(&cache_span, "skipped");
        trace::record_duration(&cache_span, cache_lookup_started.elapsed());
    } else {
        if let Some(cached) = async { cache.get_image(&cache_key).await }
            .instrument(cache_span.clone())
            .await
        {
            metrics.record_media_proxy_stage(
                &source_label,
                format_label,
                "cache_lookup",
                "hit",
                cache_lookup_started.elapsed(),
            );
            metrics.record_media_proxy_stage(
                &source_label,
                format_label,
                "total",
                "cache_hit",
                total_started.elapsed(),
            );
            trace::record_result(&cache_span, "hit");
            trace::record_duration(&cache_span, cache_lookup_started.elapsed());
            trace::record_outcome(&span, "cache_hit");
            trace::record_duration(&span, total_started.elapsed());
            return Ok(MediaProxyResult::Bytes {
                content_type: cached.content_type,
                body: cached.body,
                cache_hit: true,
            });
        }
        metrics.record_media_proxy_stage(
            &source_label,
            format_label,
            "cache_lookup",
            "miss",
            cache_lookup_started.elapsed(),
        );
        trace::record_result(&cache_span, "miss");
        trace::record_duration(&cache_span, cache_lookup_started.elapsed());
    }

    let media = media_ref_from_query(url.as_deref(), spec.as_deref())?;
    let fallback_url = media.url.clone();
    let fetcher = {
        let pm = source_registry.read().await;
        if let Some(source) = source.as_deref() {
            pm.media_client(source)?
        } else {
            pm.unscoped_media_client()
        }
    };
    let fetch_span = trace::plugin_operation_span(&source_label, "media_fetch");
    let fetch_started = Instant::now();
    let image_result = async {
        timeout(
            MEDIA_PROXY_FETCH_TIMEOUT,
            fetcher.fetch_media(&media, RequestProfile::ImageHotlink),
        )
        .await
    }
    .instrument(fetch_span.clone())
    .await;
    trace::record_duration(&fetch_span, fetch_started.elapsed());

    let (body, content_type) = match image_result {
        Ok(Ok(result)) => {
            trace::record_outcome(&fetch_span, "success");
            metrics.record_media_proxy_stage(
                &source_label,
                format_label,
                "fetch",
                "success",
                fetch_started.elapsed(),
            );
            result
        }
        Ok(Err(error)) => {
            trace::record_error(&fetch_span, &error);
            trace::record_outcome(&span, "redirect");
            trace::record_duration(&span, total_started.elapsed());
            let correlation = trace::current_trace_context();
            metrics.record_media_proxy_stage(
                &source_label,
                format_label,
                "fetch",
                "error",
                fetch_started.elapsed(),
            );
            metrics.record_media_proxy_stage(
                &source_label,
                format_label,
                "total",
                "redirect",
                total_started.elapsed(),
            );
            tracing::warn!(
                trace_id = correlation.trace_id(),
                span_id = correlation.span_id(),
                source = %source_label,
                format = %format_label,
                url = %fallback_url,
                error = %error,
                "Media Proxy Fetch Failed; Redirecting To Source URL",
            );
            return Ok(MediaProxyResult::Redirect(fallback_url));
        }
        Err(_) => {
            trace::record_outcome(&fetch_span, "timeout");
            trace::record_outcome(&span, "redirect");
            trace::record_duration(&span, total_started.elapsed());
            let correlation = trace::current_trace_context();
            metrics.record_media_proxy_stage(
                &source_label,
                format_label,
                "fetch",
                "timeout",
                fetch_started.elapsed(),
            );
            metrics.record_media_proxy_stage(
                &source_label,
                format_label,
                "total",
                "redirect",
                total_started.elapsed(),
            );
            tracing::warn!(
                trace_id = correlation.trace_id(),
                span_id = correlation.span_id(),
                source = %source_label,
                format = %format_label,
                url = %fallback_url,
                timeout_ms = MEDIA_PROXY_FETCH_TIMEOUT.as_millis(),
                "Media Proxy Fetch Timed Out; Redirecting To Source URL",
            );
            return Ok(MediaProxyResult::Redirect(fallback_url));
        }
    };

    if let Err(error) = backend_image::detect_supported_image_format(&body).with_context(|| {
        format!(
            "invalid image bytes fetched from media proxy url {fallback_url} (content_type={content_type}, bytes={})",
            body.len()
        )
    }) {
        trace::record_error(&span, &error);
        trace::record_outcome(&span, "redirect");
        trace::record_duration(&span, total_started.elapsed());
        let correlation = trace::current_trace_context();
        metrics.record_media_proxy_stage(
            &source_label,
            format_label,
            "total",
            "redirect",
            total_started.elapsed(),
        );
        tracing::warn!(
            trace_id = correlation.trace_id(),
            span_id = correlation.span_id(),
            source = %source_label,
            format = %format_label,
            url = %fallback_url,
            content_type = %content_type,
            bytes = body.len(),
            error = %error,
            "Media Proxy Fetched Invalid Image; Redirecting To Source URL",
        );
        return Ok(MediaProxyResult::Redirect(fallback_url));
    }

    let (body, content_type) = match apply_proxy_format(
        metrics,
        &source_label,
        format_label,
        body,
        content_type,
        format,
        width,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            trace::record_error(&span, &error);
            trace::record_duration(&span, total_started.elapsed());
            let correlation = trace::current_trace_context();
            metrics.record_media_proxy_stage(
                &source_label,
                format_label,
                "total",
                "redirect",
                total_started.elapsed(),
            );
            tracing::warn!(
                trace_id = correlation.trace_id(),
                span_id = correlation.span_id(),
                source = %source_label,
                format = %format_label,
                url = %fallback_url,
                error = %error,
                "Media Proxy Format Failed; Redirecting To Source URL",
            );
            return Ok(MediaProxyResult::Redirect(fallback_url));
        }
    };

    if body.is_empty() {
        trace::record_outcome(&span, "empty_redirect");
        trace::record_duration(&span, total_started.elapsed());
        metrics.record_media_proxy_stage(
            &source_label,
            format_label,
            "total",
            "empty_redirect",
            total_started.elapsed(),
        );
        return Ok(MediaProxyResult::Redirect(fallback_url));
    }

    let body = Bytes::from(body);

    if skip_cache {
        metrics.record_media_proxy_stage(
            &source_label,
            format_label,
            "cache_write",
            "skipped",
            Duration::ZERO,
        );
    } else {
        let cache_write_span = trace::cache_write_span("media_image", "image_proxy");
        let cache_write_started = Instant::now();
        let cached = CachedImage {
            content_type: content_type.clone(),
            body: body.clone(),
        };
        {
            let _entered = cache_write_span.enter();
            if let Some(ttl) = ttl {
                cache.insert_image_ttl(cache_key, cached, ttl);
            } else {
                cache.insert_image(cache_key, cached);
            }
        }
        trace::record_duration(&cache_write_span, cache_write_started.elapsed());
        metrics.record_media_proxy_stage(
            &source_label,
            format_label,
            "cache_write",
            "success",
            cache_write_started.elapsed(),
        );
    }

    metrics.record_media_proxy_stage(
        &source_label,
        format_label,
        "total",
        "fetched",
        total_started.elapsed(),
    );
    trace::record_outcome(&span, "fetched");
    trace::record_duration(&span, total_started.elapsed());

    Ok(MediaProxyResult::Bytes {
        content_type,
        body,
        cache_hit: false,
    })
}

struct ImageProxyRequest {
    url: Option<String>,
    spec: Option<String>,
    source: Option<String>,
    format: Option<MediaProxyFormat>,
    width: Option<u32>,
    skip_cache: bool,
}

pub(crate) fn normalize_page_url(
    page_url: &str,
    source_base_url: Option<&str>,
) -> Result<String, AppError> {
    if page_url.starts_with("http://") || page_url.starts_with("https://") {
        return Ok(page_url.to_string());
    }

    let base_url = source_base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request(anyhow::anyhow!("source base URL is required")))?;
    let base = url::Url::parse(base_url).map_err(|error| {
        AppError::bad_request(anyhow::anyhow!("invalid source base URL: {error}"))
    })?;
    base.join(page_url)
        .map(|url| url.to_string())
        .map_err(|error| AppError::bad_request(anyhow::anyhow!("invalid page URL: {error}")))
}

pub fn media_proxy_format_from_query(
    format: Option<&str>,
) -> Result<Option<MediaProxyFormat>, AppError> {
    match format.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some("avif") => Ok(Some(MediaProxyFormat::Avif)),
        Some("webp") => Ok(Some(MediaProxyFormat::Webp)),
        Some("jpeg" | "jpg") => Ok(Some(MediaProxyFormat::Jpeg)),
        Some(value) => Err(AppError::bad_request(anyhow::anyhow!(
            "Unsupported media image format `{value}`"
        ))),
    }
}

fn media_cache_key(
    source_cache_key: &str,
    format: Option<MediaProxyFormat>,
    width: Option<u32>,
) -> String {
    match (format, width) {
        (None, None) => source_cache_key.to_string(),
        _ => format!(
            "format={}:width={}:v2:{source_cache_key}",
            media_proxy_format_label(format.or(Some(MediaProxyFormat::Avif))),
            width.map_or_else(|| "native".to_string(), |value| value.to_string())
        ),
    }
}

fn media_proxy_format_label(format: Option<MediaProxyFormat>) -> &'static str {
    format.map_or("original", MediaProxyFormat::as_str)
}

#[autometrics]
#[tracing::instrument(name = "app.media.format", skip_all, fields(source = %source, format = %format_label, width = tracing::field::Empty, outcome = tracing::field::Empty, duration_ms = tracing::field::Empty))]
async fn apply_proxy_format(
    metrics: &Metrics,
    source: &str,
    format_label: &str,
    body: Vec<u8>,
    content_type: String,
    format: Option<MediaProxyFormat>,
    width: Option<u32>,
) -> Result<(Vec<u8>, String), AppError> {
    let span = tracing::Span::current();
    if let Some(width) = width {
        span.record("width", width);
    }
    if format.is_none() && width.is_none() {
        trace::record_outcome(&span, "passthrough");
        return Ok((body, content_type));
    }
    if width == Some(0) {
        return Err(AppError::bad_request(anyhow::anyhow!(
            "width must be greater than zero"
        )));
    }
    let format = format.unwrap_or(MediaProxyFormat::Avif);
    let started = Instant::now();
    let result =
        tokio::task::spawn_blocking(move || backend_image::transform(&body, format, width))
            .await
            .map_err(|error| anyhow::anyhow!("image conversion task failed: {error}"))?;
    metrics.record_media_proxy_stage(
        source,
        format_label,
        "convert",
        if result.is_ok() { "success" } else { "error" },
        started.elapsed(),
    );
    trace::record_duration(&span, started.elapsed());
    Ok((result?, format.content_type().to_string()))
}

fn media_ref_from_query(url: Option<&str>, spec: Option<&str>) -> Result<MediaRefSpec, AppError> {
    if let Some(spec) = spec {
        return decode_media_spec(spec).map_err(AppError::from);
    }

    let url = url
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| {
            AppError::bad_request(anyhow::anyhow!("Missing url or spec query parameter"))
        })?;

    Ok(MediaRefSpec {
        url: url.to_string(),
        request: None,
        transform: None,
    })
}
