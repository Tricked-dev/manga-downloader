use std::sync::Arc;

use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use backend_plugin_host::SourceInfo;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    api::{
        dto::{
            ApiListResponse, ChapterResponse, ErrorEnvelopeResponse, MangaResponse,
            OperationStatusResponse, PluginArtifactResponse, SearchResponse,
            SetSourceEnabledResponse, SourceSettingsResponse, UploadPluginResponse,
        },
        error::AppError,
        response_cache, validation,
    },
    app::{catalog, chapter_pages, source_catalog_changes},
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(list_sources))
        .routes(routes!(reload_sources))
        .routes(routes!(upload_plugin))
        .routes(routes!(delete_source))
        .routes(routes!(set_source_enabled))
        .routes(routes!(list_source_artifacts))
        .routes(routes!(get_source_settings, update_source_settings))
        .routes(routes!(search_source))
        .routes(routes!(get_manga_details))
        .routes(routes!(get_chapter_list))
        .routes(routes!(get_page_list))
}

#[utoipa::path(
    get,
    path = "/v1/sources",
    tag = "sources",
    responses(
        (status = OK, body = ApiListResponse<SourceInfo>),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.sources.list", skip_all)]
async fn list_sources(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    response_cache::cached_json(&state.cache, "sources:list", || async {
        catalog::list_sources(&state.db, &state.plugin_manager).await
    })
    .await
}

#[derive(Deserialize, ToSchema)]
struct SetSourceEnabledRequest {
    enabled: bool,
}

#[utoipa::path(
    put,
    path = "/v1/sources/{name}/enabled",
    tag = "sources",
    params(("name" = String, Path, description = "Source plugin name")),
    request_body = SetSourceEnabledRequest,
    responses(
        (status = OK, body = SetSourceEnabledResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.sources.plugin.set_enabled", skip_all, fields(source = %name, enabled = req.enabled))]
async fn set_source_enabled(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<SetSourceEnabledRequest>,
) -> Result<impl IntoResponse, AppError> {
    let response = source_catalog_changes::set_source_enabled(&state, &name, req.enabled).await?;
    tracing::info!(
        plugin = %response.name,
        enabled = response.enabled,
        "Source Enabled Updated",
    );
    Ok(Json(response))
}

#[derive(ToSchema)]
#[allow(dead_code)]
struct UploadPluginRequest {
    #[schema(value_type = String, format = Binary)]
    file: String,
}

#[utoipa::path(
    post,
    path = "/v1/sources/upload",
    tag = "sources",
    request_body(content = UploadPluginRequest, content_type = "multipart/form-data"),
    responses(
        (status = CREATED, body = UploadPluginResponse),
        (status = BAD_REQUEST, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.sources.plugin.upload", skip_all)]
async fn upload_plugin(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        AppError::bad_request(anyhow::anyhow!("Failed to read multipart field: {e}"))
    })? {
        let file_name = field.file_name().unwrap_or("unknown").to_string();

        if std::path::Path::new(&file_name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("wasm"))
        {
            let data = field.bytes().await.map_err(|e| {
                AppError::bad_request(anyhow::anyhow!("Failed to read multipart data: {e}"))
            })?;

            let response = source_catalog_changes::upload_plugin(&state, &file_name, &data).await?;

            tracing::info!(
                file_name = %response.filename,
                source = %response.source,
                replaced_existing = response.replaced_existing,
                "Source Plugin Uploaded",
            );

            return Ok((StatusCode::CREATED, Json(response)));
        }
    }
    Err(AppError::bad_request(anyhow::anyhow!(
        "No .wasm file found in payload"
    )))
}

#[utoipa::path(
    post,
    path = "/v1/sources/reload",
    tag = "sources",
    responses(
        (status = OK, body = OperationStatusResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.sources.plugin.reload", skip_all)]
async fn reload_sources(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    let response = source_catalog_changes::reload_sources(&state).await?;
    tracing::info!("Source Plugins Reloaded");
    Ok(Json(response))
}

#[derive(Deserialize, ToSchema, garde::Validate)]
#[garde(allow_unvalidated)]
struct UpdateSourceSettingsRequest {
    hide_nsfw: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/v1/sources/{name}/settings",
    tag = "sources",
    params(("name" = String, Path, description = "Source plugin name")),
    responses(
        (status = OK, body = SourceSettingsResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.sources.settings.get", skip_all, fields(source = %name))]
async fn get_source_settings(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(
        catalog::get_source_settings(&state.db, &state.plugin_manager, &name).await?,
    ))
}

#[utoipa::path(
    get,
    path = "/v1/sources/{name}/artifacts",
    tag = "sources",
    params(("name" = String, Path, description = "Source plugin name")),
    responses(
        (status = OK, body = ApiListResponse<PluginArtifactResponse>),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.sources.artifacts.list", skip_all, fields(source = %name))]
async fn list_source_artifacts(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(
        catalog::list_source_artifacts(&state.db, &name).await?,
    ))
}

#[utoipa::path(
    put,
    path = "/v1/sources/{name}/settings",
    tag = "sources",
    params(("name" = String, Path, description = "Source plugin name")),
    request_body = UpdateSourceSettingsRequest,
    responses(
        (status = OK, body = SourceSettingsResponse),
        (status = BAD_REQUEST, body = ErrorEnvelopeResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.sources.settings.update", skip_all, fields(source = %name))]
async fn update_source_settings(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<UpdateSourceSettingsRequest>,
) -> Result<impl IntoResponse, AppError> {
    let UpdateSourceSettingsRequest { hide_nsfw } = validation::validate(req)?;
    let response = source_catalog_changes::update_source_settings(&state, &name, hide_nsfw).await?;
    tracing::info!(
        source = %response.name,
        hide_nsfw = response.hide_nsfw,
        "Source Settings Updated",
    );
    Ok(Json(response))
}

#[utoipa::path(
    delete,
    path = "/v1/sources/{name}",
    tag = "sources",
    params(("name" = String, Path, description = "Source plugin name")),
    responses(
        (status = NO_CONTENT),
        (status = CONFLICT, body = ErrorEnvelopeResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.sources.plugin.delete", skip_all, fields(source = %name))]
async fn delete_source(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    source_catalog_changes::delete_source(&state, &name).await?;
    tracing::info!(source = %name, "Source Deleted");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, IntoParams, garde::Validate)]
#[into_params(parameter_in = Query)]
#[garde(allow_unvalidated)]
struct SearchQuery {
    #[serde(default)]
    q: String,
    #[garde(range(min = 1))]
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    popular: bool,
}

fn default_page() -> u32 {
    1
}

#[utoipa::path(
    get,
    path = "/v1/sources/{name}/search",
    tag = "sources",
    params(
        ("name" = String, Path, description = "Source plugin name"),
        SearchQuery
    ),
    responses(
        (status = OK, body = SearchResponse),
        (status = BAD_REQUEST, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(
    name = "api.sources.search",
    skip_all,
    fields(source = %name, page = query.page, popular = query.popular, category = %query.category.as_deref().unwrap_or(""))
)]
async fn search_source(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let query = validation::validate(query)?;
    Ok(Json(
        catalog::search_source(
            &state.db,
            &state.plugin_manager,
            &state.cache,
            &state.telemetry.metrics,
            catalog::SearchSourceInput {
                name,
                query: query.q,
                page: query.page,
                category: query.category,
                popular: query.popular,
            },
        )
        .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/v1/sources/{name}/manga/{id}",
    tag = "sources",
    params(
        ("name" = String, Path, description = "Source plugin name"),
        ("id" = String, Path, description = "Remote manga id")
    ),
    responses(
        (status = OK, body = MangaResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.sources.manga_details", skip_all, fields(source = %name, remote_id = %id))]
async fn get_manga_details(
    State(state): State<Arc<AppState>>,
    Path((name, id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(
        catalog::get_manga_details(
            &state.plugin_manager,
            &state.cache,
            &state.telemetry.metrics,
            name,
            id,
        )
        .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/v1/sources/{name}/manga/{id}/chapters",
    tag = "sources",
    params(
        ("name" = String, Path, description = "Source plugin name"),
        ("id" = String, Path, description = "Remote manga id")
    ),
    responses(
        (status = OK, body = ApiListResponse<ChapterResponse>),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.sources.chapter_list", skip_all, fields(source = %name, remote_id = %id))]
async fn get_chapter_list(
    State(state): State<Arc<AppState>>,
    Path((name, id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(
        catalog::get_chapter_list(
            &state.plugin_manager,
            &state.cache,
            &state.telemetry.metrics,
            name,
            id,
        )
        .await?,
    ))
}

#[derive(Deserialize, IntoParams, garde::Validate)]
#[into_params(parameter_in = Query)]
#[garde(allow_unvalidated)]
struct PageListQuery {
    #[serde(default)]
    proxy: bool,
}

#[utoipa::path(
    get,
    path = "/v1/sources/{name}/chapters/{id}/pages",
    tag = "sources",
    params(
        ("name" = String, Path, description = "Source plugin name"),
        ("id" = String, Path, description = "Remote chapter id"),
        PageListQuery
    ),
    responses(
        (status = OK, body = ApiListResponse<String>),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.sources.page_list", skip_all, fields(source = %name, remote_id = %id, proxy = query.proxy))]
async fn get_page_list(
    State(state): State<Arc<AppState>>,
    Path((name, id)): Path<(String, String)>,
    Query(query): Query<PageListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let query = validation::validate(query)?;
    let pages = chapter_pages::source_chapter_page_references_for_reader(
        &state,
        name.clone(),
        id.clone(),
        chapter_pages::SourceChapterPageReferenceOptions {
            warm_current_chapter: query.proxy,
            warm_next_chapter: true,
        },
    )
    .await?;
    let source_base_url = if query.proxy {
        let pm = state.plugin_manager.read().await;
        Some(pm.source_base_url(&name)?)
    } else {
        None
    };
    let response = ApiListResponse::new(
        pages
            .items
            .iter()
            .map(|page| page.route_url(source_base_url.as_deref(), query.proxy))
            .collect::<Result<Vec<_>, _>>()?,
    );
    Ok(Json(response))
}
