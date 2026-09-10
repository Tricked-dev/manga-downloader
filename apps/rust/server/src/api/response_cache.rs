use std::{future::Future, time::Duration};

use axum::{
    body::Body,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use backend_cache::{CacheKey, MangaCache};
use serde::Serialize;

use crate::api::error::AppError;

pub(crate) const ROUTE_SNAPSHOT_TTL: Duration = Duration::from_secs(2);

pub(crate) async fn cached_json<T, F, Fut>(
    cache: &MangaCache,
    name: &'static str,
    compute: F,
) -> Result<Response, AppError>
where
    T: Serialize,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    let key = route_snapshot_key(name);
    if let Some(bytes) = cache.get_api_bytes(&key).await {
        return Ok(json_bytes_response(bytes));
    }

    let value = compute().await?;
    let bytes = serde_json::to_vec(&value).map_err(AppError::internal)?;
    cache.insert_api_bytes_ttl(key, bytes.clone(), ROUTE_SNAPSHOT_TTL);
    Ok(json_bytes_response(bytes))
}

pub(crate) fn route_snapshot_key(name: &str) -> CacheKey {
    CacheKey::RouteSnapshot {
        name: name.to_string(),
    }
}

fn json_bytes_response(bytes: Vec<u8>) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        Body::from(bytes),
    )
        .into_response()
}
