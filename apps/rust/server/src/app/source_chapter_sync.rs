use crate::api::{dto::ApiListResponse, error::AppError};
use autometrics::autometrics;
use backend_persistence::{ChapterInsert, ChapterRow, Database, MangaRow};
use backend_plugin_host::PluginManager;
use tokio::sync::RwLock;

#[derive(Debug)]
pub(crate) struct SourceChapterSyncOutcome {
    pub(crate) new_ids: Vec<String>,
    pub(crate) is_initial_sync: bool,
}

impl SourceChapterSyncOutcome {
    pub(crate) fn new_chapter_count(&self) -> usize {
        if self.is_initial_sync {
            0
        } else {
            self.new_ids.len()
        }
    }

    pub(crate) fn is_library_update(&self) -> bool {
        !self.is_initial_sync && !self.new_ids.is_empty()
    }

    pub(crate) fn local_library_changed(&self) -> bool {
        self.is_initial_sync || !self.new_ids.is_empty()
    }
}

#[autometrics(track_concurrency)]
pub(crate) async fn sync_local_library_chapters(
    db: &Database,
    plugin_manager: &RwLock<PluginManager>,
    manga: &MangaRow,
) -> Result<SourceChapterSyncOutcome, AppError> {
    let chapter_inserts = source_chapter_inserts(plugin_manager, manga).await?;
    let new_ids = db.sync_chapters(&manga.id, chapter_inserts).await?;
    let is_initial_sync = !manga.chapters_initialized;

    if is_initial_sync {
        db.baseline_chapter_updates(&manga.id).await?;
        db.mark_chapters_initialized(&manga.id).await?;
    }

    Ok(SourceChapterSyncOutcome {
        new_ids,
        is_initial_sync,
    })
}

#[autometrics(track_concurrency)]
pub(crate) async fn refresh_local_library_chapters(
    db: &Database,
    plugin_manager: &RwLock<PluginManager>,
    manga: &MangaRow,
) -> Result<ApiListResponse<ChapterRow>, AppError> {
    sync_local_library_chapters(db, plugin_manager, manga).await?;
    let chapters = db.get_chapters(&manga.id).await?;
    Ok(ApiListResponse::new(chapters))
}

#[autometrics(track_concurrency)]
pub(crate) async fn ensure_local_library_chapter(
    db: &Database,
    plugin_manager: &RwLock<PluginManager>,
    manga: &MangaRow,
    chapter_ref: &str,
) -> Result<ChapterRow, AppError> {
    if let Some(chapter) = db
        .get_chapter_by_manga_and_source_id(&manga.id, chapter_ref)
        .await?
    {
        return Ok(chapter);
    }

    if let Some(chapter) = db.get_chapter_by_id(chapter_ref).await? {
        if chapter.manga_id == manga.id {
            return Ok(chapter);
        }

        return Err(AppError::bad_request(anyhow::anyhow!(
            "Chapter does not belong to the requested manga"
        )));
    }

    sync_local_library_chapters(db, plugin_manager, manga).await?;

    db.get_chapter_by_manga_and_source_id(&manga.id, chapter_ref)
        .await?
        .ok_or_else(|| AppError::not_found(anyhow::anyhow!("Chapter not found for manga")))
}

#[autometrics(track_concurrency)]
pub(crate) async fn refresh_stale_chapter_source_id(
    db: &Database,
    plugin_manager: &RwLock<PluginManager>,
    manga: &MangaRow,
    chapter: &ChapterRow,
) -> Result<Option<ChapterRow>, AppError> {
    sync_local_library_chapters(db, plugin_manager, manga).await?;
    let refreshed = db
        .get_chapter_by_id(&chapter.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Chapter not found after refreshing chapter list"))?;

    if refreshed.source_id == chapter.source_id {
        return Ok(None);
    }

    Ok(Some(refreshed))
}

async fn source_chapter_inserts(
    plugin_manager: &RwLock<PluginManager>,
    manga: &MangaRow,
) -> Result<Vec<ChapterInsert>, AppError> {
    let source_chapters = {
        let plugins = plugin_manager.read().await;
        plugins.get_chapter_list(&manga.source, &manga.source_id)?
    };

    Ok(source_chapters
        .into_iter()
        .map(|chapter| ChapterInsert {
            source_id: chapter.id,
            title: chapter.title,
            chapter_number: chapter.number,
            date_uploaded: chapter.published_at,
        })
        .collect())
}
