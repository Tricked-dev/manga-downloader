use std::sync::Arc;

use axum::{Json, extract::State, response::IntoResponse};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    api::{
        dto::{ErrorEnvelopeResponse, OperationStatusResponse},
        error::AppError,
    },
    app::settings,
    database_maintenance,
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(clear_cache))
        .routes(routes!(cleanup_database))
}

#[utoipa::path(
    post,
    path = "/v1/settings/clear-cache",
    tag = "settings",
    responses(
        (status = OK, body = OperationStatusResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn clear_cache(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    let response = settings::clear_cache(&state.cache).await?;
    tracing::info!("Cache Cleared");
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/v1/settings/cleanup-database",
    tag = "settings",
    responses(
        (status = OK, body = OperationStatusResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn cleanup_database(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let response = database_maintenance::cleanup_database_manually(&state).await?;
    Ok(Json(response))
}
