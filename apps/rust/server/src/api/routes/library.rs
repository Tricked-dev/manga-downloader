use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use backend_persistence::ChapterRow;
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    api::{
        dto::{
            ApiListResponse, CreatedResourceResponse, ErrorEnvelopeResponse, LibraryMangaResponse,
            LibraryUpdateResponse, OperationStatusResponse, RefreshLibraryMetadataResponse,
        },
        error::AppError,
        response_cache, validation,
    },
    app::{
        chapter_pages::{self, DownloadedPageReadOptions},
        external_reader,
        library::{self, AddToLibraryInput},
        library_update, read_progress,
    },
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(get_library, add_to_library))
        .routes(routes!(get_all_library_chapters))
        .routes(routes!(get_downloaded_chapter_pages))
        .routes(routes!(get_downloaded_chapter_page))
        .routes(routes!(
            update_chapter_read_progress,
            clear_chapter_read_progress
        ))
        .routes(routes!(get_library_manga, remove_from_library))
        .routes(routes!(get_library_manga_chapters))
        .routes(routes!(refresh_library_manga_chapters))
        .routes(routes!(refresh_library_manga_metadata))
        .routes(routes!(update_library_manga_category))
        .routes(routes!(trigger_library_update))
        .routes(routes!(get_library_updates))
}

#[utoipa::path(
    get,
    path = "/v1/library",
    tag = "library",
    responses(
        (status = OK, body = ApiListResponse<LibraryMangaResponse>),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.list", skip_all)]
async fn get_library(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    response_cache::cached_json(&state.cache, "library:list", || async {
        library::list(&state.db).await
    })
    .await
}

#[derive(Deserialize, ToSchema, garde::Validate)]
#[garde(allow_unvalidated)]
struct AddToLibraryRequest {
    #[garde(custom(crate::api::validation::trimmed_non_empty))]
    source: String,
    #[garde(custom(crate::api::validation::trimmed_non_empty))]
    source_id: String,
    #[garde(custom(crate::api::validation::trimmed_non_empty))]
    title: String,
    #[garde(custom(crate::api::validation::trimmed_non_empty))]
    cover_url: String,
    #[serde(default)]
    cover_fetch_spec: Option<String>,
    description: String,
    author: String,
    #[serde(default)]
    genres: Vec<String>,
    status: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    is_nsfw: bool,
    #[serde(default)]
    language: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
struct DownloadedChapterPageQuery {
    #[serde(default, alias = "skip_cache")]
    skip_page_cache: bool,
}

#[utoipa::path(
    post,
    path = "/v1/library",
    tag = "library",
    request_body = AddToLibraryRequest,
    responses(
        (status = CREATED, body = CreatedResourceResponse),
        (status = BAD_REQUEST, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.add", skip_all, fields(source = %req.source, source_id = %req.source_id, category = %req.category))]
async fn add_to_library(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddToLibraryRequest>,
) -> Result<impl IntoResponse, AppError> {
    let AddToLibraryRequest {
        source,
        source_id,
        title,
        cover_url,
        cover_fetch_spec,
        description,
        author,
        genres,
        status,
        category,
        is_nsfw,
        language,
    } = validation::validate(req)?;

    let input = AddToLibraryInput {
        source,
        source_id,
        title,
        cover_url,
        cover_fetch_spec,
        description,
        author,
        genres,
        status,
        category,
        is_nsfw,
        language,
    };
    let id = library::add_to_local_library(&state, input).await?;
    Ok((StatusCode::CREATED, Json(CreatedResourceResponse::new(id))))
}

#[utoipa::path(
    delete,
    path = "/v1/library/{id}",
    tag = "library",
    params(("id" = String, Path, description = "Library manga id")),
    responses(
        (status = NO_CONTENT),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.remove", skip_all, fields(library_id = %id))]
async fn remove_from_library(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    library::remove_from_local_library(&state, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/v1/library/{id}",
    tag = "library",
    params(("id" = String, Path, description = "Library manga id")),
    responses(
        (status = OK, body = LibraryMangaResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.get", skip_all, fields(library_id = %id))]
async fn get_library_manga(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(library::get(&state.db, &id).await?))
}

#[utoipa::path(
    get,
    path = "/v1/library/{id}/chapters",
    tag = "library",
    params(("id" = String, Path, description = "Library manga id")),
    responses(
        (status = OK, body = ApiListResponse<ChapterRow>),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.chapters", skip_all, fields(library_id = %id))]
async fn get_library_manga_chapters(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(library::chapters(&state.db, &id).await?))
}

#[utoipa::path(
    get,
    path = "/v1/library/chapters",
    tag = "library",
    responses(
        (status = OK, body = ApiListResponse<ChapterRow>),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.all_chapters", skip_all)]
async fn get_all_library_chapters(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    response_cache::cached_json(&state.cache, "library:all-chapters", || async {
        library::all_chapters(&state.db).await
    })
    .await
}

#[utoipa::path(
    get,
    path = "/v1/library/chapters/{chapter_id}/pages",
    tag = "library",
    params(
        ("chapter_id" = String, Path, description = "Library chapter id"),
        ("skip_page_cache" = Option<bool>, Query, description = "Skip downloaded page cache side effects for this request")
    ),
    responses(
        (status = OK, body = ApiListResponse<String>),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.downloaded_pages.list", skip_all, fields(chapter_id = %chapter_id, skip_page_cache = query.skip_page_cache))]
async fn get_downloaded_chapter_pages(
    State(state): State<Arc<AppState>>,
    Path(chapter_id): Path<String>,
    Query(query): Query<DownloadedChapterPageQuery>,
) -> Result<impl IntoResponse, AppError> {
    let references = chapter_pages::downloaded_chapter_page_references_for_reader(
        &state,
        &chapter_id,
        chapter_pages::DownloadedChapterPageReferenceOptions {
            skip_page_cache: query.skip_page_cache,
        },
    )
    .await?;
    Ok(Json(external_reader::downloaded_chapter_page_urls(
        &references,
    )))
}

#[utoipa::path(
    get,
    path = "/v1/library/chapters/{chapter_id}/pages/{page}",
    tag = "library",
    params(
        ("chapter_id" = String, Path, description = "Library chapter id"),
        ("page" = usize, Path, description = "Zero-based page index"),
        ("skip_page_cache" = Option<bool>, Query, description = "Skip downloaded page image cache reads and writes for this request")
    ),
    responses(
        (status = OK, description = "Downloaded chapter page image"),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = CONFLICT, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.downloaded_pages.get", skip_all, fields(chapter_id = %chapter_id, page, skip_page_cache = query.skip_page_cache))]
async fn get_downloaded_chapter_page(
    State(state): State<Arc<AppState>>,
    Path((chapter_id, page)): Path<(String, usize)>,
    Query(query): Query<DownloadedChapterPageQuery>,
) -> Result<Response, AppError> {
    let page = chapter_pages::read_downloaded_chapter_page(
        &state,
        &chapter_id,
        page,
        DownloadedPageReadOptions {
            skip_page_cache: query.skip_page_cache,
        },
    )
    .await?;
    Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, page.content_type)
        .body(axum::body::Body::from(page.body))
        .map_err(AppError::from)
}

#[derive(Deserialize, ToSchema, garde::Validate)]
#[garde(allow_unvalidated)]
struct UpdateChapterReadProgressRequest {
    #[serde(default)]
    page: usize,
    #[serde(default)]
    completed: bool,
}

#[utoipa::path(
    put,
    path = "/v1/library/chapters/{chapter_id}/read-progress",
    tag = "library",
    params(("chapter_id" = String, Path, description = "Library chapter id")),
    request_body = UpdateChapterReadProgressRequest,
    responses(
        (status = OK, body = ChapterRow),
        (status = BAD_REQUEST, body = ErrorEnvelopeResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.read_progress.update", skip_all, fields(chapter_id = %chapter_id, page = req.page, completed = req.completed))]
async fn update_chapter_read_progress(
    State(state): State<Arc<AppState>>,
    Path(chapter_id): Path<String>,
    Json(req): Json<UpdateChapterReadProgressRequest>,
) -> Result<impl IntoResponse, AppError> {
    let req = validation::validate(req)?;
    let chapter = read_progress::update(&state, &chapter_id, req.page, req.completed).await?;
    Ok(Json(chapter))
}

#[utoipa::path(
    delete,
    path = "/v1/library/chapters/{chapter_id}/read-progress",
    tag = "library",
    params(("chapter_id" = String, Path, description = "Library chapter id")),
    responses(
        (status = OK, body = ChapterRow),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.read_progress.clear", skip_all, fields(chapter_id = %chapter_id))]
async fn clear_chapter_read_progress(
    State(state): State<Arc<AppState>>,
    Path(chapter_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let chapter = read_progress::clear(&state, &chapter_id).await?;
    Ok(Json(chapter))
}

#[utoipa::path(
    post,
    path = "/v1/library/{id}/chapters/refresh",
    tag = "library",
    params(("id" = String, Path, description = "Library manga id")),
    responses(
        (status = OK, body = ApiListResponse<ChapterRow>),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.chapters.refresh", skip_all, fields(library_id = %id))]
async fn refresh_library_manga_chapters(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let chapters = library::refresh_local_library_chapters(&state, &id).await?;
    Ok(Json(chapters))
}

#[utoipa::path(
    post,
    path = "/v1/library/{id}/metadata/refresh",
    tag = "library",
    params(("id" = String, Path, description = "Library manga id")),
    responses(
        (status = OK, body = RefreshLibraryMetadataResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.metadata.refresh", skip_all, fields(library_id = %id))]
async fn refresh_library_manga_metadata(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let files_rewritten = library::refresh_local_library_metadata(&state, &id).await?;
    Ok(Json(RefreshLibraryMetadataResponse::ok(files_rewritten)))
}

#[derive(Deserialize, ToSchema, garde::Validate)]
#[garde(allow_unvalidated)]
struct UpdateLibraryCategoryRequest {
    category: String,
}

#[utoipa::path(
    put,
    path = "/v1/library/{id}/category",
    tag = "library",
    params(("id" = String, Path, description = "Library manga id")),
    request_body = UpdateLibraryCategoryRequest,
    responses(
        (status = OK, body = OperationStatusResponse),
        (status = BAD_REQUEST, body = ErrorEnvelopeResponse),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.category.update", skip_all, fields(library_id = %id))]
async fn update_library_manga_category(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateLibraryCategoryRequest>,
) -> Result<impl IntoResponse, AppError> {
    let req = validation::validate(req)?;
    let response = library::update_local_library_category(&state, &id, &req.category).await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/v1/library/update",
    tag = "library",
    responses(
        (status = OK, body = LibraryUpdateResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.update.trigger", skip_all)]
async fn trigger_library_update(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let run = library_update::run(Arc::clone(&state), "manual").await?;
    Ok(Json(LibraryUpdateResponse {
        new_chapters: run.new_chapters,
    }))
}

#[utoipa::path(
    get,
    path = "/v1/library/updates",
    tag = "library",
    responses(
        (status = OK, body = ApiListResponse<ChapterRow>),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
#[tracing::instrument(name = "api.library.updates.list", skip_all)]
async fn get_library_updates(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    response_cache::cached_json(&state.cache, "library:updates", || async {
        library::updates(&state.db).await
    })
    .await
}
