use super::state::{AppState, ShutdownDrain};
use crate::{database_maintenance, downloader, jobs, scheduler, search_cache};
use axum::Router;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_graceful::Shutdown;

const SEARCH_CACHE_WARM_ENABLED_ENV: &str = "MANGA_SERVER_SEARCH_CACHE_WARM_ENABLED";

pub(super) async fn run_until_shutdown(
    listener: TcpListener,
    app: Router,
    state: &Arc<AppState>,
) -> anyhow::Result<()> {
    let (server_failed_tx, server_failed_rx) = oneshot::channel();
    let (http_shutdown_tx, http_shutdown_rx) = oneshot::channel();
    let shutdown = build_shutdown(server_failed_rx, Arc::clone(state));
    let shutdown_signal = shutdown.guard_weak();
    let tasks = spawn_server_tasks(
        &shutdown,
        listener,
        app,
        state,
        server_failed_tx,
        http_shutdown_rx,
    );

    shutdown_signal.cancelled().await;
    tracing::info!("Shutdown Signal Observed; Waiting For Background Tasks");

    let ServerTasks { background, server } = tasks;
    let background_result = background.join().await;

    let _ = http_shutdown_tx.send(());
    tracing::info!("HTTP Server Shutdown Released After Background Tasks Completed");

    let shutdown_wait = shutdown.shutdown().await;
    tracing::info!(
        shutdown_wait_secs = shutdown_wait.as_secs_f64(),
        "Graceful Shutdown Tasks Completed",
    );

    background_result?;
    await_task("http server", server).await?;
    wait_for_plugin_runtime_idle(state).await;
    state.cache.close().await?;

    tracing::info!("Server Shutdown Finalized");
    state.telemetry.shutdown()?;

    Ok(())
}

fn build_shutdown(server_failed: oneshot::Receiver<()>, state: Arc<AppState>) -> Shutdown {
    Shutdown::builder()
        .with_signal(async move {
            tokio::select! {
                () = wait_for_shutdown_signal() => {
                    begin_shutdown_drain(&state.shutdown_drain);
                    tracing::warn!("Shutdown signal received; marking server unready and starting graceful shutdown");
                }
                _ = server_failed => {}
            }
        })
        .build()
}

fn begin_shutdown_drain(shutdown_drain: &ShutdownDrain) {
    shutdown_drain.begin();
    tracing::warn!("Server marked unready for graceful shutdown");
}

fn spawn_server_tasks(
    shutdown: &Shutdown,
    listener: TcpListener,
    app: Router,
    state: &Arc<AppState>,
    server_failed: oneshot::Sender<()>,
    http_shutdown: oneshot::Receiver<()>,
) -> ServerTasks {
    let downloads = shutdown.spawn_task_fn({
        let state = Arc::clone(state);
        move |guard| async move {
            downloader::process_downloads(state, guard).await;
            Ok::<(), anyhow::Error>(())
        }
    });

    let scheduler = shutdown.spawn_task_fn({
        let state = Arc::clone(state);
        move |guard| async move {
            scheduler::run_scheduler(state, guard).await;
            Ok::<(), anyhow::Error>(())
        }
    });

    let search_cache = search_cache_warm_enabled().then(|| {
        shutdown.spawn_task_fn({
            let state = Arc::clone(state);
            move |guard| async move {
                search_cache::run_search_cache_refresher(state, guard).await;
                Ok::<(), anyhow::Error>(())
            }
        })
    });

    let database_cleanup = shutdown.spawn_task_fn({
        let state = Arc::clone(state);
        move |guard| async move {
            database_maintenance::run_database_cleanup_task(state, guard).await;
            Ok::<(), anyhow::Error>(())
        }
    });

    let archive_index_rebuild = shutdown.spawn_task_fn({
        let state = Arc::clone(state);
        move |guard| async move { jobs::run_archive_index_rebuild_worker(state, guard).await }
    });

    let tokio_runtime_metrics = shutdown.spawn_task_fn(|guard| async move {
        let shutdown = guard.clone_weak();
        tokio::select! {
            () = backend_telemetry::run_tokio_runtime_metrics_reporter() => {}
            () = shutdown.cancelled() => {}
        }
        Ok::<(), anyhow::Error>(())
    });

    let discord = state.discord.as_ref().map(|_| {
        shutdown.spawn_task_fn({
            let state = Arc::clone(state);
            move |guard| Box::pin(backend_discord::run_server_gateway(state, guard))
        })
    });

    let server = shutdown.spawn_task_fn({
        move |guard| async move {
            let shutdown_guard = guard;
            let result = axum::serve(listener, app)
                .with_graceful_shutdown(wait_for_http_shutdown_release(http_shutdown))
                .await;
            drop(shutdown_guard);

            if let Err(ref error) = result {
                tracing::error!(
                    error = %error,
                    outcome = "error",
                    "HTTP Server Exited Unexpectedly",
                );
                let _ = server_failed.send(());
            }

            result.map_err(Into::into)
        }
    });

    ServerTasks {
        background: BackgroundTasks {
            downloads,
            scheduler,
        search_cache,
            database_cleanup,
            archive_index_rebuild,
            tokio_runtime_metrics,
            discord,
        },
        server,
    }
}

struct ServerTasks {
    background: BackgroundTasks,
    server: JoinHandle<anyhow::Result<()>>,
}

struct BackgroundTasks {
    downloads: JoinHandle<anyhow::Result<()>>,
    scheduler: JoinHandle<anyhow::Result<()>>,
    search_cache: Option<JoinHandle<anyhow::Result<()>>>,
    database_cleanup: JoinHandle<anyhow::Result<()>>,
    archive_index_rebuild: JoinHandle<anyhow::Result<()>>,
    tokio_runtime_metrics: JoinHandle<anyhow::Result<()>>,
    discord: Option<JoinHandle<anyhow::Result<()>>>,
}

impl BackgroundTasks {
    async fn join(self) -> anyhow::Result<()> {
        await_task("download processor", self.downloads).await?;
        await_task("library update scheduler", self.scheduler).await?;
        if let Some(search_cache) = self.search_cache {
            await_task("search cache refresher", search_cache).await?;
        }
        await_task("database cleanup", self.database_cleanup).await?;
        await_task("archive index rebuild worker", self.archive_index_rebuild).await?;
        await_task("tokio runtime metrics reporter", self.tokio_runtime_metrics).await?;
        if let Some(discord) = self.discord {
            await_task("discord gateway", discord).await?;
        }
        Ok(())
    }
}

fn search_cache_warm_enabled() -> bool {
    std::env::var(SEARCH_CACHE_WARM_ENABLED_ENV)
        .ok()
        .is_some_and(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

async fn wait_for_shutdown_signal() {
    tokio_graceful::default_signal().await;
}

async fn wait_for_http_shutdown_release(http_shutdown: oneshot::Receiver<()>) {
    let _ = http_shutdown.await;
    tracing::info!("HTTP Server Shutdown Initiated");
}

async fn wait_for_plugin_runtime_idle(state: &AppState) {
    let runtime_activity = {
        let plugin_manager = state.plugin_manager.read().await;
        plugin_manager.runtime_activity()
    };
    runtime_activity.wait_for_idle().await;
    tracing::info!("Plugin Runtime Activity Drained");
}

async fn await_task(name: &str, handle: JoinHandle<anyhow::Result<()>>) -> anyhow::Result<()> {
    match handle.await {
        Ok(result) => result,
        Err(error) => Err(anyhow::anyhow!("{name} task failed to join: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn shutdown_drain_marks_shutdown_state_before_listener_shutdown() {
        let shutdown_drain = ShutdownDrain::default();

        begin_shutdown_drain(&shutdown_drain);

        assert!(shutdown_drain.is_draining());
    }

    #[tokio::test]
    async fn http_shutdown_waits_for_background_release() {
        let (release_http_shutdown, http_shutdown) = oneshot::channel();
        let pending_shutdown = wait_for_http_shutdown_release(http_shutdown);

        tokio::pin!(pending_shutdown);

        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut pending_shutdown)
                .await
                .is_err()
        );

        release_http_shutdown.send(()).unwrap();

        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut pending_shutdown)
                .await
                .is_ok()
        );
    }
}
