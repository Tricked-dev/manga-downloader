use std::sync::Arc;

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Redirect},
};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    api::dto::ErrorEnvelopeResponse,
    api::error::AppError,
    api::validation,
    app::media::{self, MediaProxyResult},
};

const IMAGE_CACHE_CONTROL: &str =
    "public, max-age=86400, s-maxage=86400, stale-while-revalidate=86400";
const IMAGE_CDN_CACHE_CONTROL: &str = "public, s-maxage=86400, stale-while-revalidate=86400";

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(proxy_image))
}

#[derive(Deserialize, IntoParams, garde::Validate)]
#[into_params(parameter_in = Query)]
#[garde(allow_unvalidated)]
struct ProxyQuery {
    #[garde(url)]
    url: Option<String>,
    #[garde(custom(crate::api::validation::optional_trimmed_non_empty))]
    spec: Option<String>,
    #[garde(custom(crate::api::validation::optional_trimmed_non_empty))]
    source: Option<String>,
    #[garde(custom(crate::api::validation::optional_trimmed_non_empty))]
    format: Option<String>,
    #[garde(range(min = 1, max = 2400))]
    width: Option<u32>,
    #[serde(default)]
    skip_cache: bool,
}

#[utoipa::path(
    get,
    path = "/v1/media/image",
    tag = "media",
    params(ProxyQuery),
    responses(
        (status = OK, description = "Proxied image bytes"),
        (status = TEMPORARY_REDIRECT, description = "Redirect to the original image"),
        (status = BAD_REQUEST, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(
    name = "api.media.image_proxy",
    skip_all,
    fields(
        source = %query.source.as_deref().unwrap_or("unscoped"),
        format = %query.format.as_deref().unwrap_or("original"),
        width = tracing::field::Empty,
        skip_cache = query.skip_cache,
        has_spec = query.spec.is_some(),
        has_url = query.url.is_some(),
    )
)]
async fn proxy_image(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ProxyQuery>,
) -> Result<impl IntoResponse, AppError> {
    let query = validation::validate(query)?;
    if let Some(width) = query.width {
        tracing::Span::current().record("width", width);
    }
    if query.url.is_none() && query.spec.is_none() {
        return Err(AppError::bad_request(anyhow::anyhow!(
            "either `url` or `spec` is required"
        )));
    }
    let source_label = query.source.as_deref().unwrap_or("unscoped").to_string();
    let had_source = query.source.is_some();
    let format_label = query
        .format
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("original")
        .to_string();
    let format = media::media_proxy_format_from_query(query.format.as_deref())?;
    let width = query.width;

    match media::proxy_image(
        &state.cache,
        &state.telemetry.metrics,
        &state.plugin_manager,
        query.url,
        query.spec,
        query.source,
        format,
        width,
        query.skip_cache,
    )
    .await
    {
        Ok(MediaProxyResult::Redirect(url)) => {
            state.telemetry.metrics.record_media_proxy_request(
                &source_label,
                &format_label,
                "redirect",
                None,
            );
            Ok(Redirect::temporary(&url).into_response())
        }
        Ok(MediaProxyResult::Bytes {
            content_type,
            body,
            cache_hit,
        }) => {
            let bytes = u64::try_from(body.len()).unwrap_or(u64::MAX);
            state.telemetry.metrics.record_media_proxy_request(
                &source_label,
                &format_label,
                if cache_hit { "cache_hit" } else { "fetched" },
                Some(bytes),
            );
            let proxied = axum::response::Response::builder()
                .header(axum::http::header::CONTENT_TYPE, content_type)
                .header(axum::http::header::CACHE_CONTROL, IMAGE_CACHE_CONTROL)
                .header("CDN-Cache-Control", IMAGE_CDN_CACHE_CONTROL)
                .header("Cloudflare-CDN-Cache-Control", IMAGE_CDN_CACHE_CONTROL)
                .header("X-Image-Cache", if cache_hit { "HIT" } else { "MISS" })
                .body(axum::body::Body::from(body))
                .map_err(AppError::from)?;
            Ok(proxied)
        }
        Err(error) => {
            let error_source_label = if had_source { "unknown" } else { "unscoped" };
            state.telemetry.metrics.record_media_proxy_request(
                error_source_label,
                &format_label,
                "error",
                None,
            );
            Err(error)
        }
    }
}
