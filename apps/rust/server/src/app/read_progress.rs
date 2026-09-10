use std::sync::Arc;

use autometrics::autometrics;
use backend_persistence::ChapterRow;

use crate::{AppState, api::error::AppError, app::route_snapshot_invalidation};

#[autometrics]
pub async fn update(
    state: &Arc<AppState>,
    chapter_id: &str,
    page: usize,
    completed: bool,
) -> Result<ChapterRow, AppError> {
    let previous = require_chapter(&state.db, chapter_id).await?;
    let chapter = state
        .db
        .update_chapter_read_progress(chapter_id, page, completed)
        .await?
        .ok_or_else(|| AppError::not_found(anyhow::anyhow!("Library chapter not found")))?;
    let activity = read_progress_activity(&previous, &chapter);
    state
        .db
        .record_read_progress_activity(
            &chapter,
            activity.pages_read_delta,
            activity.chapter_completed,
        )
        .await?;
    route_snapshot_invalidation::read_progress_changed(state, chapter_id);
    tracing::debug!(
        chapter_id = %chapter.id,
        manga_id = %chapter.manga_id,
        pages_read = chapter.pages_read,
        read_completed = chapter.read_completed,
        pages_read_delta = activity.pages_read_delta,
        chapter_completed = activity.chapter_completed,
        "Library Chapter Read Progress Updated",
    );
    Ok(chapter)
}

#[autometrics]
pub async fn clear(state: &Arc<AppState>, chapter_id: &str) -> Result<ChapterRow, AppError> {
    let chapter = state
        .db
        .clear_chapter_read_progress(chapter_id)
        .await?
        .ok_or_else(|| AppError::not_found(anyhow::anyhow!("Library chapter not found")))?;
    route_snapshot_invalidation::read_progress_changed(state, chapter_id);
    tracing::debug!(
        chapter_id = %chapter.id,
        manga_id = %chapter.manga_id,
        "Library Chapter Read Progress Cleared",
    );
    Ok(chapter)
}

async fn require_chapter(
    db: &backend_persistence::Database,
    chapter_id: &str,
) -> Result<ChapterRow, AppError> {
    db.get_chapter_by_id(chapter_id)
        .await?
        .ok_or_else(|| AppError::not_found(anyhow::anyhow!("Library chapter not found")))
}

#[derive(Debug, PartialEq, Eq)]
struct ReadProgressActivity {
    pages_read_delta: usize,
    chapter_completed: bool,
}

fn read_progress_activity(previous: &ChapterRow, updated: &ChapterRow) -> ReadProgressActivity {
    ReadProgressActivity {
        pages_read_delta: updated.pages_read.saturating_sub(previous.pages_read),
        chapter_completed: !previous.read_completed && updated.read_completed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(pages_read: usize, read_completed: bool) -> ChapterRow {
        ChapterRow {
            id: "chapter".to_string(),
            manga_id: "manga".to_string(),
            source_id: "source-chapter".to_string(),
            title: "Chapter".to_string(),
            chapter_number: 1.0,
            date_uploaded: "0".to_string(),
            fetched_at: "0".to_string(),
            downloaded: false,
            is_new: false,
            pages_read,
            read_completed,
            last_read_at: None,
        }
    }

    #[test]
    fn read_progress_activity_records_only_forward_progress_and_first_completion() {
        assert_eq!(
            read_progress_activity(&chapter(2, false), &chapter(5, true)),
            ReadProgressActivity {
                pages_read_delta: 3,
                chapter_completed: true,
            }
        );
        assert_eq!(
            read_progress_activity(&chapter(5, true), &chapter(5, true)),
            ReadProgressActivity {
                pages_read_delta: 0,
                chapter_completed: false,
            }
        );
        assert_eq!(
            read_progress_activity(&chapter(5, true), &chapter(0, false)),
            ReadProgressActivity {
                pages_read_delta: 0,
                chapter_completed: false,
            }
        );
    }
}
