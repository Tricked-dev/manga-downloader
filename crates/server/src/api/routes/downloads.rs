use std::sync::Arc;

use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use backend_persistence::DownloadRow;
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    api::{
        dto::{
            ApiListResponse, DownloadBulkEnqueueResponse, DownloadEnqueueResponse,
            ErrorEnvelopeResponse, OperationStatusResponse,
        },
        error::AppError,
        response_cache, validation,
    },
    app::downloads,
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(enqueue_download, get_downloads))
        .routes(routes!(enqueue_downloads))
        .routes(routes!(clear_failed_downloads))
        .routes(routes!(delete_download))
        .routes(routes!(retry_download))
        .routes(routes!(upscale_download))
        .routes(routes!(serve_archive))
}

#[derive(Deserialize, ToSchema, garde::Validate)]
struct DownloadRequest {
    #[garde(custom(crate::api::validation::trimmed_non_empty))]
    chapter_id: String,
    #[garde(custom(crate::api::validation::trimmed_non_empty))]
    manga_id: String,
}

#[derive(Deserialize, ToSchema, garde::Validate)]
struct DownloadBulkRequest {
    #[garde(custom(crate::api::validation::trimmed_non_empty))]
    manga_id: String,
    #[garde(length(min = 1))]
    chapter_ids: Vec<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
struct DownloadsQuery {
    #[serde(default)]
    manga_id: Option<String>,
}

#[utoipa::path(
    post,
    path = "/v1/downloads",
    tag = "downloads",
    request_body = DownloadRequest,
    responses(
        (status = CREATED, body = DownloadEnqueueResponse),
        (status = BAD_REQUEST, body = ErrorEnvelopeResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.downloads.enqueue", skip_all, fields(chapter_id = %req.chapter_id, manga_id = %req.manga_id))]
async fn enqueue_download(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DownloadRequest>,
) -> Result<impl IntoResponse, AppError> {
    let req = validation::validate(req)?;
    let result = downloads::enqueue_manual(&state, &req.chapter_id, &req.manga_id).await?;
    tracing::info!(
        download_id = %result.response.id,
        manga_id = %result.response.manga_id,
        chapter_id = %result.response.chapter_id,
        source = %result.manga_source,
        manga_title = %result.manga_title,
        "Download Enqueued",
    );
    Ok((axum::http::StatusCode::CREATED, Json(result.response)))
}

#[utoipa::path(
    post,
    path = "/v1/downloads/bulk",
    tag = "downloads",
    request_body = DownloadBulkRequest,
    responses(
        (status = OK, body = DownloadBulkEnqueueResponse),
        (status = BAD_REQUEST, body = ErrorEnvelopeResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.downloads.bulk_enqueue", skip_all, fields(manga_id = %req.manga_id, chapter_count = req.chapter_ids.len()))]
async fn enqueue_downloads(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DownloadBulkRequest>,
) -> Result<impl IntoResponse, AppError> {
    let req = validation::validate(req)?;
    let result =
        downloads::enqueue_library_chapters(&state, &req.manga_id, &req.chapter_ids).await?;
    tracing::info!(
        manga_id = %req.manga_id,
        chapter_count = req.chapter_ids.len(),
        enqueued = result.enqueued,
        "Downloads Bulk Enqueued",
    );
    Ok(Json(DownloadBulkEnqueueResponse {
        enqueued: result.enqueued,
    }))
}

#[utoipa::path(
    get,
    path = "/v1/downloads",
    tag = "downloads",
    params(("manga_id" = Option<String>, Query, description = "Filter downloads to one library manga id")),
    responses(
        (status = OK, body = ApiListResponse<DownloadRow>),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.downloads.list", skip_all, fields(filtered = query.manga_id.is_some()))]
async fn get_downloads(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DownloadsQuery>,
) -> Result<Response, AppError> {
    let manga_id = query
        .manga_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if manga_id.is_some() {
        return Ok(Json(downloads::list(&state.db, manga_id).await?).into_response());
    }

    response_cache::cached_json(&state.cache, "downloads:list", || async {
        downloads::list(&state.db, None).await
    })
    .await
}

#[utoipa::path(
    delete,
    path = "/v1/downloads/{id}",
    tag = "downloads",
    params(("id" = String, Path, description = "Download id")),
    responses(
        (status = NO_CONTENT),
        (status = ACCEPTED),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.downloads.delete", skip_all, fields(download_id = %id))]
async fn delete_download(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<impl IntoResponse, AppError> {
    let outcome = downloads::remove_or_cancel(&state, &id).await?;
    Ok(if outcome.is_async_completion() {
        StatusCode::ACCEPTED
    } else {
        StatusCode::NO_CONTENT
    })
}

#[utoipa::path(
    delete,
    path = "/v1/downloads/failed",
    tag = "downloads",
    responses(
        (status = OK, body = OperationStatusResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.downloads.clear_failed", skip_all)]
async fn clear_failed_downloads(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let result = downloads::clear_failed_work_records(&state).await?;

    tracing::info!(
        cleared = result.cleared,
        archive_bytes = result.removed_size_bytes,
        "Failed Downloads Cleared",
    );
    Ok(Json(OperationStatusResponse::ok()))
}

#[utoipa::path(
    post,
    path = "/v1/downloads/{id}/retry",
    tag = "downloads",
    params(("id" = String, Path, description = "Download id")),
    responses(
        (status = OK, body = OperationStatusResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.downloads.retry", skip_all, fields(download_id = %id))]
async fn retry_download(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<impl IntoResponse, AppError> {
    let response = downloads::retry_work_record(&state, &id).await?;
    tracing::info!(download_id = %id, "Download Retried");
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/v1/downloads/{id}/upscale",
    tag = "downloads",
    params(("id" = String, Path, description = "Download id")),
    responses(
        (status = ACCEPTED, body = serde_json::Value),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.downloads.upscale", skip_all, fields(download_id = %id))]
async fn upscale_download(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<impl IntoResponse, AppError> {
    crate::app::downloaded_archive_resolution::require_existing_completed_archive(&state.db, &id)
        .await?;
    let job_id = crate::jobs::enqueue_upscale(&state, &id, 2).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "ok": true, "job_id": job_id })),
    ))
}

#[utoipa::path(
    get,
    path = "/v1/downloads/{id}/archive",
    tag = "downloads",
    params(("id" = String, Path, description = "Download id")),
    responses(
        (status = OK, description = "Download archive", content_type = "application/vnd.bbf"),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.downloads.archive", skip_all, fields(download_id = %id))]
async fn serve_archive(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<impl IntoResponse, AppError> {
    let archive = downloads::open_archive(&state.db, &id).await?;

    let response = axum::response::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "application/vnd.bbf")
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", archive.filename),
        )
        .body(axum::body::Body::from(archive.body))
        .map_err(AppError::from)?;

    Ok(response)
}
