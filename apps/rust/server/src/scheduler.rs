use std::sync::Arc;
use tokio_graceful::ShutdownGuard;

use crate::{
    AppState,
    app::{library_update, settings},
};

const SCHEDULER_TICK: tokio::time::Duration = tokio::time::Duration::from_mins(1);

/// Background scheduler that periodically checks for new chapters.
pub async fn run_scheduler(state: Arc<AppState>, shutdown: ShutdownGuard) {
    tracing::info!("Library Update Scheduler Started");
    let mut ticker = tokio::time::interval(SCHEDULER_TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_run = Some(tokio::time::Instant::now());
    let shutdown = shutdown.clone_weak();

    loop {
        tokio::select! {
            () = shutdown.cancelled() => {
                tracing::info!("Library Update Scheduler Stopping");
                break;
            }
            _tick = ticker.tick() => {}
        }

        let interval = settings::library_update_interval(&state.db).await;

        if interval.is_zero() {
            continue;
        }

        if let Some(last_run_at) = last_run
            && last_run_at.elapsed() < interval
        {
            continue;
        }

        match library_update::run_with_shutdown(Arc::clone(&state), "scheduled", Some(&shutdown))
            .await
        {
            Ok(run) => {
                last_run = Some(tokio::time::Instant::now());
                if run.new_chapters > 0 {
                    tracing::trace!(
                        new_chapters = run.new_chapters,
                        "Scheduled Library Update Detected Changes",
                    );
                }
            }
            Err(error) => tracing::error!(
                trigger = "scheduled",
                error = %error,
                outcome = "error",
                "Library Update Run Failed",
            ),
        }
    }

    tracing::info!("Library Update Scheduler Stopped");
}
