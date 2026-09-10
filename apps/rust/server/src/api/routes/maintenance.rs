use std::sync::Arc;

use axum::{Json, extract::State, response::IntoResponse};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    api::{
        dto::{ErrorEnvelopeResponse, OperationStatusResponse},
        error::AppError,
        response_cache,
    },
    app::{archive_index_maintenance, downloaded_archive_lifecycle, settings},
    database_maintenance,
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(get_archive_index_status, clear_archive_index))
        .routes(routes!(rebuild_archive_index))
        .routes(routes!(cleanup_stale_archive_indexes))
        .routes(routes!(reencode_downloaded_avif))
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

#[utoipa::path(
    get,
    path = "/v1/settings/archive-index",
    tag = "settings",
    responses(
        (status = OK, body = crate::api::dto::ArchiveIndexStatusResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn get_archive_index_status(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    response_cache::cached_json(&state.cache, "settings:archive-index", || async {
        archive_index_maintenance::status(&state).await
    })
    .await
}

#[utoipa::path(
    post,
    path = "/v1/settings/archive-index/rebuild",
    tag = "settings",
    responses(
        (status = OK, body = crate::api::dto::ArchiveIndexJobResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn rebuild_archive_index(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(
        archive_index_maintenance::start_manual_rebuild(&state).await?,
    ))
}

#[utoipa::path(
    post,
    path = "/v1/settings/archive-index/cleanup-stale",
    tag = "settings",
    responses(
        (status = OK, body = crate::api::dto::ArchiveIndexCleanupResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn cleanup_stale_archive_indexes(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(
        archive_index_maintenance::cleanup_stale(&state).await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/v1/settings/archive-index",
    tag = "settings",
    responses(
        (status = OK, body = crate::api::dto::ArchiveIndexCleanupResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn clear_archive_index(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(archive_index_maintenance::clear(&state).await?))
}

#[utoipa::path(
    post,
    path = "/v1/settings/reencode-avif",
    tag = "settings",
    responses(
        (status = OK, body = serde_json::Value),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn reencode_downloaded_avif(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let result = downloaded_archive_lifecycle::downloaded_archives_reencoded(&state).await?;

    Ok(Json(serde_json::json!({
        "files_processed": result.files_processed,
        "images_reencoded": result.images_reencoded
    })))
}
