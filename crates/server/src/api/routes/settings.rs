use std::{collections::HashMap, sync::Arc};

use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::api::validation;
use crate::{
    AppState,
    api::{
        dto::{
            ApiTokenListResponse, ApiTokenSummary, BackendApiKeyResponse, CreatedApiTokenResponse,
            ErrorEnvelopeResponse, OperationStatusResponse, SettingsResponse,
        },
        error::AppError,
        response_cache,
    },
    app::settings,
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(get_settings, update_settings))
        .routes(routes!(regenerate_backend_api_key))
        .routes(routes!(pause_upscaling))
        .routes(routes!(resume_upscaling))
        .routes(routes!(list_api_tokens, create_api_token))
        .routes(routes!(delete_api_token))
}

#[utoipa::path(
    get,
    path = "/v1/tokens",
    tag = "settings",
    responses(
        (status = OK, body = ApiTokenListResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn list_api_tokens(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    let items = state
        .db
        .list_api_tokens()
        .await?
        .into_iter()
        .map(|row| ApiTokenSummary {
            id: row.id,
            name: row.name,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        })
        .collect();
    Ok(Json(ApiTokenListResponse { items }))
}

#[derive(Deserialize, ToSchema, garde::Validate)]
struct CreateApiTokenRequest {
    #[garde(length(min = 1, max = 100))]
    name: String,
}

/// Tokens do not expire, so the list and its delete control are the only revocation
/// there is. The plaintext is returned once and never stored.
#[utoipa::path(
    post,
    path = "/v1/tokens",
    tag = "settings",
    request_body = CreateApiTokenRequest,
    responses(
        (status = OK, body = CreatedApiTokenResponse),
        (status = BAD_REQUEST, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn create_api_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateApiTokenRequest>,
) -> Result<impl IntoResponse, AppError> {
    let req = validation::validate(req)?;
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let row = state
        .db
        .create_api_token(&req.name, &crate::api::auth::token_hash(&token))
        .await?;
    tracing::info!(token_id = %row.id, name = %row.name, "API Token Created");
    Ok(Json(CreatedApiTokenResponse {
        id: row.id,
        name: row.name,
        token,
        created_at: row.created_at,
    }))
}

#[utoipa::path(
    delete,
    path = "/v1/tokens/{id}",
    tag = "settings",
    params(("id" = String, Path, description = "API token id")),
    responses(
        (status = OK, body = OperationStatusResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn delete_api_token(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    if !state.db.delete_api_token(&id).await? {
        return Err(AppError::new(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "No such API token",
            None,
        ));
    }
    tracing::info!(token_id = %id, "API Token Deleted");
    Ok(Json(OperationStatusResponse::ok()))
}

/// Paused work keeps its place in the queue rather than failing. A restart always
/// comes back paused, and enqueuing any new upscale resumes everything.
#[utoipa::path(
    post,
    path = "/v1/upscale/pause",
    tag = "settings",
    responses(
        (status = OK, body = OperationStatusResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn pause_upscaling(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    settings::set_upscale_paused(&state.db, true).await?;
    tracing::info!("Upscaling Paused");
    Ok(Json(OperationStatusResponse::ok()))
}

#[utoipa::path(
    post,
    path = "/v1/upscale/resume",
    tag = "settings",
    responses(
        (status = OK, body = OperationStatusResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn resume_upscaling(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    settings::set_upscale_paused(&state.db, false).await?;
    tracing::info!("Upscaling Resumed");
    Ok(Json(OperationStatusResponse::ok()))
}

#[utoipa::path(
    get,
    path = "/v1/settings",
    tag = "settings",
    responses(
        (status = OK, body = SettingsResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn get_settings(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    response_cache::cached_json(&state.cache, "settings:get", || async {
        settings::get(&state).await
    })
    .await
}

/// The bearer key is never written through `PUT /v1/settings`, so rotation needs its own
/// route. The surrounding authorization layer already demands a matching origin for unsafe
/// methods, which keeps a session cookie alone from rotating it cross-site.
#[utoipa::path(
    post,
    path = "/v1/settings/api-key",
    tag = "settings",
    responses(
        (status = OK, body = BackendApiKeyResponse),
        (status = UNAUTHORIZED, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn regenerate_backend_api_key(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let backend_api_key = settings::regenerate_backend_api_key(&state).await?;
    Ok(Json(BackendApiKeyResponse { backend_api_key }))
}

#[derive(Deserialize, ToSchema, garde::Validate)]
struct UpdateSettingsRequest {
    #[garde(length(min = 1))]
    #[serde(flatten)]
    settings: HashMap<String, String>,
}

#[utoipa::path(
    put,
    path = "/v1/settings",
    tag = "settings",
    request_body = UpdateSettingsRequest,
    responses(
        (status = OK, body = SettingsResponse),
        (status = BAD_REQUEST, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<impl IntoResponse, AppError> {
    let req = validation::validate(req)?;
    let update = settings::update(&state, &req.settings).await?;

    tracing::info!(
        keys = ?update.changes.updated_keys,
        updated_count = update.changes.updated_count(),
        download_path_changed = update.changes.download_path_changed(),
        cache_runtime_changed = update
            .changes
            .contains_family(settings::SettingsChangeFamily::CacheRuntime),
        download_storage_policy_changed = update
            .changes
            .contains_family(settings::SettingsChangeFamily::DownloadStoragePolicy),
        library_update_policy_changed = update
            .changes
            .contains_family(settings::SettingsChangeFamily::LibraryUpdatePolicy),
        "Settings Updated",
    );
    Ok(Json(update.response))
}
