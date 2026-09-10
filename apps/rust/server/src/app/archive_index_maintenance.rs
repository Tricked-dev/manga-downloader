use std::{path::PathBuf, sync::Arc};

use anyhow::Result as AnyResult;
use autometrics::autometrics;
use futures_util::{StreamExt, stream};

use crate::{
    AppState,
    api::{
        dto::{ArchiveIndexCleanupResponse, ArchiveIndexJobResponse, ArchiveIndexStatusResponse},
        error::AppError,
    },
    app::{downloaded_archive_derived_state, downloaded_archive_resolution},
    archive_index, jobs,
};

const MANUAL_REBUILD_TRIGGER: &str = "manual";
const STARTUP_REBUILD_TRIGGER: &str = "startup";
const STARTUP_WARM_CONCURRENCY: usize = 1;
const MANUAL_REBUILD_CONCURRENCY: usize = 2;

/// Read current Downloaded Archive Index maintenance state.
#[autometrics]
pub(crate) async fn status(state: &Arc<AppState>) -> Result<ArchiveIndexStatusResponse, AppError> {
    Ok(state
        .archive_index
        .status(&state.db, &state.telemetry.metrics)
        .await?)
}

/// Start a user-requested Downloaded Archive Index rebuild.
#[autometrics(track_concurrency)]
pub(crate) async fn start_manual_rebuild(
    state: &Arc<AppState>,
) -> Result<ArchiveIndexJobResponse, AppError> {
    start_rebuild(state, MANUAL_REBUILD_TRIGGER).await
}

/// Start the background startup warm rebuild.
#[autometrics(track_concurrency)]
pub(crate) async fn start_startup_rebuild(
    state: &Arc<AppState>,
) -> Result<ArchiveIndexJobResponse, AppError> {
    start_rebuild(state, STARTUP_REBUILD_TRIGGER).await
}

#[autometrics(track_concurrency)]
pub(crate) async fn cleanup_stale(
    state: &Arc<AppState>,
) -> Result<ArchiveIndexCleanupResponse, AppError> {
    let cleaned = state
        .archive_index
        .cleanup_stale_with_paths(&state.db, &state.telemetry.metrics)
        .await?;
    downloaded_archive_derived_state::stale_downloaded_archive_index_rows_cleaned(state, &cleaned)
        .await;
    Ok(cleaned.response)
}

#[autometrics(track_concurrency)]
pub(crate) async fn clear(state: &Arc<AppState>) -> Result<ArchiveIndexCleanupResponse, AppError> {
    let response = state
        .archive_index
        .clear(&state.db, &state.telemetry.metrics)
        .await?;
    downloaded_archive_derived_state::downloaded_archive_index_cleared(state, &response).await;
    Ok(response)
}

async fn start_rebuild(
    state: &Arc<AppState>,
    trigger: &str,
) -> Result<ArchiveIndexJobResponse, AppError> {
    let start = state.archive_index.begin_rebuild(trigger).await;

    if start.accepted {
        downloaded_archive_derived_state::downloaded_archive_index_rebuild_accepted(
            state,
            &start.response,
        )
        .await;
        match jobs::enqueue_archive_index_rebuild(state, trigger).await {
            Ok(job_id) => {
                tracing::info!(
                    job_id = %job_id,
                    trigger,
                    "Archive Index Rebuild Enqueued",
                );
            }
            Err(error) => {
                let response = state.archive_index.finish_rebuild(Err(error)).await;
                downloaded_archive_derived_state::downloaded_archive_index_rebuild_finished(
                    state, &response,
                )
                .await;
                return Ok(response);
            }
        }
    }

    tracing::info!(
        running = start.response.running,
        accepted = start.accepted,
        trigger,
        "Archive Index Rebuild Requested"
    );

    Ok(start.response)
}

pub(crate) async fn run_queued_rebuild(state: Arc<AppState>, trigger: String) -> AnyResult<()> {
    state
        .archive_index
        .mark_rebuild_running_if_idle(&trigger)
        .await;
    let result = run_rebuild(&state, &trigger).await;
    let error = result.as_ref().err().map(ToString::to_string);
    let response = state.archive_index.finish_rebuild(result).await;
    downloaded_archive_derived_state::downloaded_archive_index_rebuild_finished(&state, &response)
        .await;

    if let Some(error) = error {
        return Err(anyhow::anyhow!(error));
    }

    Ok(())
}

async fn run_rebuild(state: &Arc<AppState>, trigger: &str) -> AnyResult<ArchiveIndexJobResponse> {
    let archives = completed_archive_paths(state).await?;
    let mut rebuilds = stream::iter(archives.into_iter().map(|archive_path| {
        let state = Arc::clone(state);
        async move {
            state
                .archive_index
                .rebuild_archive_for_maintenance(
                    &state.db,
                    &state.telemetry.metrics,
                    archive_path,
                    trigger,
                )
                .await
        }
    }))
    .buffer_unordered(rebuild_concurrency(trigger));

    let mut indexed_archives = 0usize;
    let mut failed_archives = 0usize;
    while let Some(result) = rebuilds.next().await {
        match result {
            Ok(()) => indexed_archives += 1,
            Err(error) => {
                failed_archives += 1;
                tracing::warn!(error = %error, trigger, "Archive Index Rebuild Item Failed");
            }
        }
    }

    archive_index::publish_stats(&state.db, &state.telemetry.metrics).await;

    Ok(ArchiveIndexJobResponse {
        running: false,
        trigger: trigger.to_string(),
        indexed_archives,
        skipped_archives: 0,
        failed_archives,
        removed_rows: 0,
        message: format!(
            "Archive index rebuild finished: {indexed_archives} indexed, {failed_archives} failed"
        ),
    })
}

fn rebuild_concurrency(trigger: &str) -> usize {
    match trigger {
        STARTUP_REBUILD_TRIGGER => STARTUP_WARM_CONCURRENCY,
        _ => MANUAL_REBUILD_CONCURRENCY,
    }
}

async fn completed_archive_paths(state: &Arc<AppState>) -> AnyResult<Vec<PathBuf>> {
    Ok(
        downloaded_archive_resolution::existing_completed_archives(&state.db)
            .await?
            .into_iter()
            .map(|archive| archive.archive_path)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::response_cache, server::build_test_app_state};
    use backend_page_extraction::ARCHIVE_INDEX_SCHEMA_VERSION;
    use backend_persistence::ArchiveIndexIdentity;

    #[tokio::test(flavor = "multi_thread")]
    async fn cleanup_stale_coordinates_index_and_derived_state() {
        let state = test_state("archive-index-maintenance-cleanup").await;
        let archive_path = "/tmp/archive-index-maintenance-stale.cbz";
        let archive_key = format!("{archive_path}:{ARCHIVE_INDEX_SCHEMA_VERSION}:10:20");
        seed_expanded_read_window(&state, &archive_key).await;
        seed_archive_index_route_snapshot(&state);
        seed_stale_index_row(&state, archive_path).await;

        let response = cleanup_stale(&state).await.expect("cleanup should succeed");

        assert!(response.ok);
        assert_eq!(response.removed_rows, 1);
        assert_eq!(
            state
                .db
                .list_archive_index_records()
                .await
                .expect("records should list")
                .len(),
            0
        );
        assert_route_snapshot_invalidated(&state).await;
        assert_eq!(
            state
                .extraction_scheduler
                .foreground_read_window_size(&archive_key, 2, 8)
                .await,
            1
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn clear_coordinates_index_and_derived_state() {
        let state = test_state("archive-index-maintenance-clear").await;
        let archive_key = "/tmp/archive-index-maintenance-clear.cbz:2:10:20";
        seed_expanded_read_window(&state, archive_key).await;
        seed_archive_index_route_snapshot(&state);
        seed_stale_index_row(&state, "/tmp/archive-index-maintenance-clear.cbz").await;

        let response = clear(&state).await.expect("clear should succeed");

        assert!(response.ok);
        assert_eq!(response.removed_rows, 1);
        assert_route_snapshot_invalidated(&state).await;
        assert_eq!(
            state
                .extraction_scheduler
                .foreground_read_window_size(archive_key, 2, 8)
                .await,
            1
        );
    }

    async fn seed_stale_index_row(state: &Arc<AppState>, archive_path: &str) {
        state
            .db
            .upsert_archive_index(
                &ArchiveIndexIdentity {
                    archive_path: archive_path.to_string(),
                    archive_size: 10,
                    archive_mtime_ms: 20,
                    schema_version: ARCHIVE_INDEX_SCHEMA_VERSION,
                },
                1,
                b"stale",
            )
            .await
            .expect("stale archive index row should insert");
    }

    async fn seed_expanded_read_window(state: &Arc<AppState>, archive_key: &str) {
        assert_eq!(
            state
                .extraction_scheduler
                .foreground_read_window_size(archive_key, 0, 8)
                .await,
            1
        );
        assert_eq!(
            state
                .extraction_scheduler
                .foreground_read_window_size(archive_key, 1, 8)
                .await,
            8
        );
    }

    fn seed_archive_index_route_snapshot(state: &Arc<AppState>) {
        state.cache.insert_api_bytes(
            response_cache::route_snapshot_key("settings:archive-index"),
            b"cached".to_vec(),
        );
    }

    async fn assert_route_snapshot_invalidated(state: &Arc<AppState>) {
        assert!(
            state
                .cache
                .get_api_bytes(&response_cache::route_snapshot_key(
                    "settings:archive-index"
                ))
                .await
                .is_none()
        );
    }

    async fn test_state(name: &str) -> Arc<AppState> {
        build_test_app_state(name, "manga_server_archive_index_maintenance_test").await
    }
}
