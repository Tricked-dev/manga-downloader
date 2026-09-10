use std::{sync::Arc, time::Instant};

use autometrics::autometrics;
use tokio_graceful::ShutdownGuard;

use crate::{
    AppState,
    api::{dto::OperationStatusResponse, error::AppError},
};

const DATABASE_CLEANUP_INTERVAL: tokio::time::Duration = tokio::time::Duration::from_hours(24);
const MANUAL_CLEANUP_TRIGGER: &str = "manual";
const SCHEDULED_CLEANUP_TRIGGER: &str = "scheduled";

pub async fn run_database_cleanup_task(state: Arc<AppState>, shutdown: ShutdownGuard) {
    tracing::info!(
        interval_hours = DATABASE_CLEANUP_INTERVAL.as_secs() / 60 / 60,
        "Database Cleanup Task Started",
    );

    let shutdown = shutdown.clone_weak();

    loop {
        let sleep = tokio::time::sleep(DATABASE_CLEANUP_INTERVAL);
        tokio::pin!(sleep);

        tokio::select! {
            () = shutdown.cancelled() => {
                tracing::info!("Database Cleanup Task Stopping");
                break;
            }
            () = &mut sleep => {}
        }

        let _ = cleanup_database(&state, SCHEDULED_CLEANUP_TRIGGER).await;
    }

    tracing::info!("Database Cleanup Task Stopped");
}

#[autometrics(track_concurrency)]
pub async fn cleanup_database_manually(
    state: &Arc<AppState>,
) -> Result<OperationStatusResponse, AppError> {
    cleanup_database(state, MANUAL_CLEANUP_TRIGGER).await?;
    Ok(OperationStatusResponse::ok())
}

#[autometrics(track_concurrency)]
async fn cleanup_database(state: &AppState, trigger: &'static str) -> Result<(), AppError> {
    let started_at = Instant::now();

    match state.db.cleanup_database().await {
        Ok(result) => {
            crate::app::route_snapshot_invalidation::database_maintenance_completed(state);
            tracing::info!(
                orphaned_download_events = result.orphaned_download_events,
                duration_ms = started_at.elapsed().as_millis(),
                trigger,
                outcome = "success",
                "Database Cleanup Completed",
            );
            Ok(())
        }
        Err(error) => {
            tracing::warn!(
                error = %error,
                duration_ms = started_at.elapsed().as_millis(),
                trigger,
                outcome = "error",
                "Database Cleanup Failed",
            );
            Err(error.into())
        }
    }
}
