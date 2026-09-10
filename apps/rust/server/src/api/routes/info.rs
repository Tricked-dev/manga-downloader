use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    api::{
        dto::{ErrorEnvelopeResponse, HealthResponse, ServerBuildInfoResponse},
        response_cache,
    },
    build_info,
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(get_info))
        .routes(routes!(get_health))
        .routes(routes!(get_health_diagnostics))
}

#[utoipa::path(
    get,
    path = "/v1/info",
    tag = "info",
    responses(
        (status = OK, body = ServerBuildInfoResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn get_info(
    State(state): State<Arc<AppState>>,
) -> Result<axum::response::Response, crate::api::error::AppError> {
    response_cache::cached_json(&state.cache, "info:build", || async {
        Ok(build_info::server_build_info())
    })
    .await
}

#[utoipa::path(
    get,
    path = "/v1/health",
    tag = "info",
    responses(
        (status = OK, body = HealthResponse),
        (status = SERVICE_UNAVAILABLE, body = HealthResponse)
    )
)]
async fn get_health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let response: HealthResponse = crate::app::health::readiness(&state).await;
    let status = if response.ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(response))
}

#[utoipa::path(
    get,
    path = "/v1/health/diagnostics",
    tag = "info",
    responses(
        (status = OK, body = HealthResponse),
        (status = SERVICE_UNAVAILABLE, body = HealthResponse)
    )
)]
async fn get_health_diagnostics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let response: HealthResponse = crate::app::health::diagnostics(&state).await;
    let status = if response.ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(response))
}
