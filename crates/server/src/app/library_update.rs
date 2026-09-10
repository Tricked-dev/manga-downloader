use std::{sync::Arc, time::Instant};

use anyhow::Result;
use autometrics::autometrics;
use backend_persistence::MangaRow;
use tokio_graceful::WeakShutdownGuard;

use crate::{
    AppState,
    app::{
        downloads::{self, DownloadEnqueueOrigin},
        route_snapshot_invalidation, settings, source_chapter_sync,
    },
};

const MAX_CONCURRENT_UPDATE_CHECKS: usize = 4;

pub(crate) struct LibraryUpdateRun {
    pub(crate) new_chapters: usize,
}

/// Check all Local Library manga for new chapters.
#[autometrics(track_concurrency)]
pub(crate) async fn run(state: Arc<AppState>, trigger: &'static str) -> Result<LibraryUpdateRun> {
    run_with_shutdown(state, trigger, None).await
}

/// Check all Local Library manga for new chapters, stopping early during shutdown.
#[allow(clippy::too_many_lines)]
// This function owns the full Library Update Run policy: fan-out, aggregation, auto-download,
#[autometrics(track_concurrency)]
pub(crate) async fn run_with_shutdown(
    state: Arc<AppState>,
    trigger: &'static str,
    shutdown: Option<&WeakShutdownGuard>,
) -> Result<LibraryUpdateRun> {
    let started_at = Instant::now();
    let library = state.db.get_library_manga().await?;
    let mut total_new = 0;
    let mut changed_manga = 0usize;
    let mut failed_manga = 0usize;
    let mut enqueued_downloads = 0usize;

    let auto_download_policy = AutoDownloadPolicy::load(&state).await?;
    let mut pending_manga = library.iter().cloned();
    let mut workers = tokio::task::JoinSet::new();

    for _ in 0..MAX_CONCURRENT_UPDATE_CHECKS {
        if !spawn_next_update_check(
            &mut workers,
            &mut pending_manga,
            &state,
            auto_download_policy.clone(),
            shutdown,
        )
        .await
        {
            break;
        }
    }

    while let Some(result) = workers.join_next().await {
        match result {
            Ok((manga, Ok(outcome))) => {
                total_new += outcome.new_chapters;
                changed_manga += usize::from(outcome.local_library_changed);
                enqueued_downloads += outcome.enqueued_downloads;
                state.telemetry.metrics.record_library_update_manga_checked(
                    trigger,
                    &manga.source,
                    "success",
                );
                spawn_next_update_check(
                    &mut workers,
                    &mut pending_manga,
                    &state,
                    auto_download_policy.clone(),
                    shutdown,
                )
                .await;
            }
            Ok((manga, Err(error))) => {
                failed_manga += 1;
                state.telemetry.metrics.record_library_update_manga_checked(
                    trigger,
                    &manga.source,
                    "error",
                );
                tracing::warn!(
                    trigger,
                    manga_id = %manga.id,
                    manga_title = %manga.title,
                    source = %manga.source,
                    error = %error,
                    outcome = "error",
                    "Library Update Manga Failed",
                );
                spawn_next_update_check(
                    &mut workers,
                    &mut pending_manga,
                    &state,
                    auto_download_policy.clone(),
                    shutdown,
                )
                .await;
            }
            Err(error) => {
                failed_manga += 1;
                tracing::warn!(
                    trigger,
                    error = %error,
                    outcome = "error",
                    "Library Update Worker Failed",
                );
                spawn_next_update_check(
                    &mut workers,
                    &mut pending_manga,
                    &state,
                    auto_download_policy.clone(),
                    shutdown,
                )
                .await;
            }
        }
    }

    let outcome = if failed_manga == 0 {
        "success"
    } else {
        "partial"
    };
    if changed_manga > 0 {
        route_snapshot_invalidation::local_library_changed(&state);
    }
    tracing::info!(
        trigger,
        library_size = library.len(),
        checked_manga = library.len().saturating_sub(failed_manga),
        changed_manga,
        failed_manga,
        new_chapters = total_new,
        enqueued_downloads,
        auto_download_default = auto_download_policy.global_enabled,
        auto_download_category = auto_download_policy.category.as_deref().unwrap_or(""),
        duration_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
        outcome,
        "Library Update Run",
    );
    state.telemetry.metrics.record_library_update_run(
        trigger,
        outcome,
        started_at.elapsed(),
        total_new,
        enqueued_downloads,
    );

    Ok(LibraryUpdateRun {
        new_chapters: total_new,
    })
}

async fn spawn_next_update_check(
    workers: &mut tokio::task::JoinSet<(MangaRow, Result<MangaUpdateOutcome>)>,
    pending_manga: &mut impl Iterator<Item = MangaRow>,
    state: &Arc<AppState>,
    auto_download_policy: AutoDownloadPolicy,
    shutdown: Option<&WeakShutdownGuard>,
) -> bool {
    if shutdown_requested(shutdown).await {
        return false;
    }

    let Some(manga) = pending_manga.next() else {
        return false;
    };

    workers.spawn(check_manga_update_task(
        Arc::clone(state),
        manga,
        auto_download_policy,
    ));
    true
}

async fn shutdown_requested(shutdown: Option<&WeakShutdownGuard>) -> bool {
    let Some(shutdown) = shutdown else {
        return false;
    };

    tokio::select! {
        () = shutdown.cancelled() => true,
        else => false,
    }
}

#[derive(Clone)]
struct AutoDownloadPolicy {
    global_enabled: bool,
    category: Option<String>,
}

impl AutoDownloadPolicy {
    async fn load(state: &AppState) -> Result<Self> {
        let settings = settings::auto_download(&state.db).await?;

        Ok(Self {
            global_enabled: settings.enabled,
            category: settings.category,
        })
    }

    fn should_auto_download(&self, manga: &backend_persistence::MangaRow) -> bool {
        let matches_global_category = self
            .category
            .as_deref()
            .is_none_or(|category| manga.category == category);
        manga
            .auto_download
            .unwrap_or(self.global_enabled && matches_global_category)
    }
}

#[autometrics(track_concurrency)]
async fn check_manga_updates(
    state: &Arc<AppState>,
    manga: &backend_persistence::MangaRow,
    auto_download_policy: &AutoDownloadPolicy,
) -> Result<MangaUpdateOutcome> {
    let chapter_sync =
        source_chapter_sync::sync_local_library_chapters(&state.db, &state.plugin_manager, manga)
            .await?;
    let new_chapters = chapter_sync.new_chapter_count();
    let is_library_update = chapter_sync.is_library_update();
    let local_library_changed = chapter_sync.local_library_changed();
    let mut enqueued_downloads = 0usize;

    if auto_download_policy.should_auto_download(manga) && is_library_update {
        enqueued_downloads = downloads::enqueue_local_library_chapters(
            state,
            manga,
            &chapter_sync.new_ids,
            DownloadEnqueueOrigin::LibraryUpdate,
        )
        .await?
        .enqueued;
    }

    Ok(MangaUpdateOutcome {
        new_chapters,
        local_library_changed,
        enqueued_downloads,
    })
}

async fn check_manga_update_task(
    state: Arc<AppState>,
    manga: backend_persistence::MangaRow,
    auto_download_policy: AutoDownloadPolicy,
) -> (backend_persistence::MangaRow, Result<MangaUpdateOutcome>) {
    let result = check_manga_updates(&state, &manga, &auto_download_policy).await;
    (manga, result)
}

struct MangaUpdateOutcome {
    new_chapters: usize,
    local_library_changed: bool,
    enqueued_downloads: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manga(category: &str, auto_download: Option<bool>) -> backend_persistence::MangaRow {
        backend_persistence::MangaRow {
            id: "manga".to_string(),
            source: "source".to_string(),
            source_base_url: "https://example.com".to_string(),
            source_id: "source-manga".to_string(),
            title: "Title".to_string(),
            cover_url: String::new(),
            cover_fetch_spec: None,
            description: String::new(),
            author: String::new(),
            genres: String::new(),
            status: String::new(),
            category: category.to_string(),
            is_nsfw: false,
            auto_download,
            total_chapters: 0,
            downloaded_chapters: 0,
            last_updated: "0".to_string(),
            chapters_initialized: true,
            language: None,
        }
    }

    #[test]
    fn auto_download_policy_honors_overrides_before_global_category() {
        let policy = AutoDownloadPolicy {
            global_enabled: true,
            category: Some("tracked".to_string()),
        };

        assert!(policy.should_auto_download(&manga("tracked", None)));
        assert!(!policy.should_auto_download(&manga("other", None)));
        assert!(policy.should_auto_download(&manga("other", Some(true))));
        assert!(!policy.should_auto_download(&manga("tracked", Some(false))));
    }
}
