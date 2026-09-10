use std::{collections::HashMap, sync::Arc};

use axum::{Json, extract::State, response::IntoResponse};
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::api::validation;
use crate::{
    AppState,
    api::{
        dto::{ErrorEnvelopeResponse, SettingsResponse},
        error::AppError,
        response_cache,
    },
    app::settings,
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(get_settings, update_settings))
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
