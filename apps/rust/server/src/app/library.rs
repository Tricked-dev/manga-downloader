use crate::{
    AppState,
    api::{
        dto::{ApiListResponse, ComicInfoResponse, LibraryMangaResponse, OperationStatusResponse},
        error::AppError,
    },
    app::{
        downloaded_archive_lifecycle::{self, DownloadedArchiveMetadataChanged},
        downloaded_archive_resolution, route_snapshot_invalidation, source_chapter_sync,
    },
};
use autometrics::autometrics;
use backend_persistence::{Database, MangaInsert, MangaRow};
use backend_plugin_host::PluginManager;
use backend_plugin_host::media::{encode_media_spec, runtime_media_ref_to_spec};
use std::{collections::HashMap, sync::Arc};
use tokio::{sync::RwLock, task::JoinSet};

const MAX_CONCURRENT_COMICINFO_REWRITES: usize = 4;

pub struct AddToLibraryInput {
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub cover_url: String,
    pub cover_fetch_spec: Option<String>,
    pub description: String,
    pub author: String,
    pub genres: Vec<String>,
    pub status: String,
    pub category: String,
    pub is_nsfw: bool,
    pub language: Option<String>,
}

#[autometrics(track_concurrency)]
pub async fn add_to_local_library(
    state: &Arc<AppState>,
    input: AddToLibraryInput,
) -> Result<String, AppError> {
    let id = add(&state.db, &input).await?;
    route_snapshot_invalidation::local_library_changed(state);

    tracing::info!(
        library_id = %id,
        source = %input.source,
        source_id = %input.source_id,
        title = %input.title,
        category = %input.category,
        chapter_count = 0,
        chapter_refresh = "skipped",
        "Library Manga Added",
    );
    Ok(id)
}

#[autometrics]
pub async fn remove_from_local_library(state: &Arc<AppState>, id: &str) -> Result<(), AppError> {
    remove(&state.db, id).await?;
    route_snapshot_invalidation::local_library_changed(state);

    tracing::info!(library_id = %id, "Library Manga Removed");
    Ok(())
}

#[autometrics(track_concurrency)]
pub async fn refresh_local_library_chapters(
    state: &Arc<AppState>,
    id: &str,
) -> Result<ApiListResponse<backend_persistence::ChapterRow>, AppError> {
    let manga = require_library_manga(&state.db, id).await?;
    let chapters = refresh_chapters(&state.db, &state.plugin_manager, id).await?;
    route_snapshot_invalidation::local_library_changed(state);

    tracing::info!(
        library_id = %id,
        source = %manga.source,
        source_id = %manga.source_id,
        chapter_count = chapters.items.len(),
        "Library Manga Chapters Refreshed",
    );
    Ok(chapters)
}

#[autometrics(track_concurrency)]
pub async fn refresh_local_library_metadata(
    state: &Arc<AppState>,
    id: &str,
) -> Result<usize, AppError> {
    let manga = require_library_manga(&state.db, id).await?;
    let files_rewritten = refresh_downloaded_comicinfo(state, id).await?;
    route_snapshot_invalidation::local_library_changed(state);

    tracing::info!(
        library_id = %id,
        source = %manga.source,
        source_id = %manga.source_id,
        files_rewritten,
        "Library ComicInfo Refreshed",
    );
    Ok(files_rewritten)
}

#[autometrics]
pub async fn update_local_library_category(
    state: &Arc<AppState>,
    id: &str,
    category: &str,
) -> Result<OperationStatusResponse, AppError> {
    let response = update_category(&state.db, id, category).await?;
    route_snapshot_invalidation::local_library_changed(state);

    tracing::info!(
        library_id = %id,
        category = %category,
        "Library Category Updated",
    );
    Ok(response)
}

#[autometrics]
pub async fn list(db: &Database) -> Result<ApiListResponse<LibraryMangaResponse>, AppError> {
    let mangas = db.get_library_manga().await?;
    Ok(ApiListResponse::new(
        mangas.into_iter().map(map_library_manga).collect(),
    ))
}

#[autometrics]
pub async fn add(db: &Database, input: &AddToLibraryInput) -> Result<String, AppError> {
    let genres = join_non_empty(&input.genres);
    let language = input
        .language
        .as_deref()
        .map(str::trim)
        .filter(|language| !language.is_empty())
        .unwrap_or(backend_core::DEFAULT_LANGUAGE_ISO);

    db.add_manga_to_library(&MangaInsert {
        source: &input.source,
        source_id: &input.source_id,
        title: &input.title,
        cover_url: &input.cover_url,
        cover_fetch_spec: input.cover_fetch_spec.as_deref(),
        description: &input.description,
        author: &input.author,
        genres: &genres,
        status: &input.status,
        category: &input.category,
        is_nsfw: input.is_nsfw,
        language: Some(language),
    })
    .await
    .map_err(AppError::from)
}

#[autometrics]
pub async fn remove(db: &Database, id: &str) -> Result<(), AppError> {
    db.remove_from_library(id).await.map_err(AppError::from)
}

#[autometrics]
pub async fn get(db: &Database, id: &str) -> Result<LibraryMangaResponse, AppError> {
    Ok(map_library_manga(require_library_manga(db, id).await?))
}

#[autometrics]
pub async fn chapters(
    db: &Database,
    id: &str,
) -> Result<ApiListResponse<backend_persistence::ChapterRow>, AppError> {
    let manga = require_library_manga(db, id).await?;
    let chapters = db.get_chapters(&manga.id).await?;
    Ok(ApiListResponse::new(chapters))
}

#[autometrics]
pub async fn all_chapters(
    db: &Database,
) -> Result<ApiListResponse<backend_persistence::ChapterRow>, AppError> {
    let chapters = db.get_all_library_chapters().await?;
    Ok(ApiListResponse::new(chapters))
}

#[autometrics(track_concurrency)]
pub async fn refresh_chapters(
    db: &Database,
    plugin_manager: &RwLock<PluginManager>,
    id: &str,
) -> Result<ApiListResponse<backend_persistence::ChapterRow>, AppError> {
    let manga = refresh_manga_metadata(db, plugin_manager, id).await?;

    source_chapter_sync::refresh_local_library_chapters(db, plugin_manager, &manga).await
}

#[autometrics]
pub async fn update_category(
    db: &Database,
    id: &str,
    category: &str,
) -> Result<OperationStatusResponse, AppError> {
    db.update_manga_category(id, category).await?;
    Ok(OperationStatusResponse::ok())
}

#[autometrics]
pub async fn updates(
    db: &Database,
) -> Result<ApiListResponse<backend_persistence::ChapterRow>, AppError> {
    let chapters = db.get_library_updates().await?;
    Ok(ApiListResponse::new(chapters))
}

#[autometrics(track_concurrency)]
pub async fn refresh_downloaded_comicinfo(
    state: &Arc<AppState>,
    id: &str,
) -> Result<usize, AppError> {
    let manga = refresh_manga_metadata_strict(&state.db, &state.plugin_manager, id).await?;
    let chapter_by_id = state
        .db
        .get_chapters(&manga.id)
        .await?
        .into_iter()
        .map(|chapter| (chapter.id.clone(), chapter))
        .collect::<HashMap<_, _>>();
    let completed_archives =
        downloaded_archive_resolution::existing_completed_archives_for_manga(&state.db, &manga.id)
            .await?;
    let comicinfo_credits = crate::comicinfo::resolve_comicinfo_credits(&manga).await;
    let comicinfo_count =
        crate::downloader::comicinfo_count_for_series(&manga.status, manga.total_chapters);
    let age_rating = Some(crate::downloader::comicinfo_age_rating(manga.is_nsfw));
    let language = manga
        .language
        .as_deref()
        .map(str::trim)
        .filter(|language| !language.is_empty())
        .unwrap_or(backend_core::DEFAULT_LANGUAGE_ISO)
        .to_string();

    let mut files_rewritten = 0usize;
    let mut rewrite_tasks = JoinSet::new();
    for archive in completed_archives {
        let Some(chapter) = chapter_by_id.get(&archive.download.chapter_id) else {
            continue;
        };

        while rewrite_tasks.len() >= MAX_CONCURRENT_COMICINFO_REWRITES {
            files_rewritten += finish_comicinfo_rewrite_task(state, &mut rewrite_tasks).await?;
        }

        let manga_title = manga.title.clone();
        let manga_description = manga.description.clone();
        let manga_author = manga.author.clone();
        let manga_genres = manga.genres.clone();
        let chapter_title = chapter.title.clone();
        let chapter_number = chapter.chapter_number;
        let date_uploaded = chapter.date_uploaded.clone();
        let comicinfo_credits = comicinfo_credits.clone();
        let language = language.clone();
        let download_id = archive.download.id.clone();
        let chapter_id = archive.download.chapter_id.clone();
        let download_path = archive.download_path.clone();
        let archive_path = archive.archive_path.clone();

        rewrite_tasks.spawn(async move {
            let changed_archive_path = archive_path.clone();
            tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                let page_count = backend_image::count_zstd_folder_images(&archive_path)?;
                let comicinfo_xml = backend_core::build_comicinfo_xml(
                    &manga_title,
                    &manga_description,
                    &manga_author,
                    &manga_genres,
                    &backend_core::ComicInfoChapterMetadata {
                        title: &chapter_title,
                        number: chapter_number,
                        count: comicinfo_count,
                        date_uploaded: &date_uploaded,
                        page_count,
                        age_rating,
                        source_url: None,
                        language: Some(&language),
                        credits: comicinfo_credits.as_borrowed(),
                    },
                );

                backend_image::rewrite_zstd_folder_comicinfo_xml(&archive_path, &comicinfo_xml)?;
                Ok(())
            })
            .await
            .map_err(|error| anyhow::anyhow!("Failed to rewrite ComicInfo.xml: {error}"))??;
            Ok(DownloadedArchiveMetadataChanged {
                download_id,
                chapter_id,
                download_path,
                archive_path: changed_archive_path,
            })
        });
    }

    while !rewrite_tasks.is_empty() {
        files_rewritten += finish_comicinfo_rewrite_task(state, &mut rewrite_tasks).await?;
    }

    Ok(files_rewritten)
}

#[autometrics(track_concurrency)]
pub async fn refresh_manga_metadata(
    db: &Database,
    plugin_manager: &RwLock<PluginManager>,
    id: &str,
) -> Result<MangaRow, AppError> {
    let manga = require_library_manga(db, id).await?;

    let source_manga = {
        let pm = plugin_manager.read().await;
        match pm.get_manga_details(&manga.source, &manga.source_id) {
            Ok(source_manga) => source_manga,
            Err(error) => {
                tracing::warn!(
                    library_id = %manga.id,
                    source = %manga.source,
                    source_id = %manga.source_id,
                    error = %error,
                    "Library Metadata Refresh Fell Back To Stored Metadata",
                );
                return Ok(manga);
            }
        }
    };
    update_manga_metadata_from_source(db, id, &manga, source_manga).await
}

async fn refresh_manga_metadata_strict(
    db: &Database,
    plugin_manager: &RwLock<PluginManager>,
    id: &str,
) -> Result<MangaRow, AppError> {
    let manga = require_library_manga(db, id).await?;

    let source_manga = {
        let pm = plugin_manager.read().await;
        pm.get_manga_details(&manga.source, &manga.source_id)
            .map_err(AppError::internal)?
    };

    update_manga_metadata_from_source(db, id, &manga, source_manga).await
}

async fn update_manga_metadata_from_source(
    db: &Database,
    id: &str,
    manga: &MangaRow,
    source_manga: backend_plugin_host::runtime::manga::source::types::Manga,
) -> Result<MangaRow, AppError> {
    let (cover_url, cover_fetch_spec) = if source_manga.cover.request.is_some() {
        let spec = runtime_media_ref_to_spec(&source_manga.cover)?;
        (
            source_manga.cover.url.clone(),
            Some(encode_media_spec(&spec)?),
        )
    } else {
        (source_manga.cover.url.clone(), None)
    };
    let source_genres = join_non_empty(&source_manga.genres);
    let genres = if source_genres.is_empty() {
        manga.genres.as_str()
    } else {
        source_genres.as_str()
    };

    db.update_manga_metadata(
        &manga.id,
        &source_manga.title,
        &cover_url,
        cover_fetch_spec.as_deref(),
        &source_manga.description,
        &source_manga.author,
        genres,
        &source_manga.status,
        source_manga.is_nsfw,
        backend_core::DEFAULT_LANGUAGE_ISO,
    )
    .await?;

    require_library_manga(db, id).await
}

#[autometrics]
pub async fn refresh_manga_metadata_if_incomplete(
    db: &Database,
    plugin_manager: &RwLock<PluginManager>,
    id: &str,
) -> Result<MangaRow, AppError> {
    let manga = require_library_manga(db, id).await?;

    if manga_metadata_is_complete(&manga) {
        return Ok(manga);
    }

    refresh_manga_metadata(db, plugin_manager, id).await
}

async fn require_library_manga(db: &Database, id: &str) -> Result<MangaRow, AppError> {
    db.get_library_manga_by_id(id)
        .await?
        .ok_or_else(|| AppError::not_found(anyhow::anyhow!("Library manga not found")))
}

fn manga_metadata_is_complete(manga: &MangaRow) -> bool {
    !manga.description.trim().is_empty()
        && !manga.author.trim().is_empty()
        && !manga.genres.trim().is_empty()
        && !manga.status.trim().is_empty()
        && manga
            .language
            .as_deref()
            .is_some_and(|language| !language.trim().is_empty())
}

fn join_non_empty(values: &[String]) -> String {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn map_library_manga(manga: MangaRow) -> LibraryMangaResponse {
    let language = manga
        .language
        .as_deref()
        .map(str::trim)
        .filter(|language| !language.is_empty())
        .map(ToString::to_string);
    let genres = manga
        .genres
        .split(',')
        .map(str::trim)
        .filter(|genre| !genre.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let genre = if genres.is_empty() {
        None
    } else {
        Some(genres.join(", "))
    };
    let source_base_url = Some(manga.source_base_url.clone());
    let comic_info = ComicInfoResponse {
        title: Some(manga.title.clone()),
        series: Some(manga.title.clone()),
        summary: non_empty_string(&manga.description),
        writer: non_empty_string(&manga.author),
        genre,
        age_rating: Some(crate::downloader::comicinfo_age_rating(manga.is_nsfw).to_string()),
        language_iso: language.clone(),
        web: source_base_url.clone(),
    };

    LibraryMangaResponse {
        id: manga.id,
        source: manga.source.clone(),
        source_base_url,
        source_id: manga.source_id,
        title: manga.title,
        cover_url: manga.cover_url,
        cover_proxy_url: manga
            .cover_fetch_spec
            .as_deref()
            .map(|spec| crate::app::media::media_proxy_url(&manga.source, spec)),
        description: manga.description,
        author: manga.author,
        genres,
        status: manga.status,
        is_nsfw: manga.is_nsfw,
        language,
        comic_info,
        category: manga.category,
        auto_download: manga.auto_download,
        total_chapters: manga.total_chapters,
        downloaded_chapters: manga.downloaded_chapters,
        last_updated: manga.last_updated,
    }
}

fn non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

async fn finish_comicinfo_rewrite_task(
    state: &Arc<AppState>,
    rewrite_tasks: &mut JoinSet<anyhow::Result<DownloadedArchiveMetadataChanged>>,
) -> Result<usize, AppError> {
    let Some(task_result) = rewrite_tasks.join_next().await else {
        return Ok(0);
    };

    let changed = task_result.map_err(|error| {
        AppError::internal(anyhow::anyhow!("ComicInfo rewrite task failed: {error}"))
    })??;
    downloaded_archive_lifecycle::downloaded_archive_metadata_changed(state, &changed).await;
    Ok(1)
}
