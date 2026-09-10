use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

use crate::AppState;

#[derive(Debug)]
struct DownloadCancelled;

impl std::fmt::Display for DownloadCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Download canceled")
    }
}

impl std::error::Error for DownloadCancelled {}

pub(super) fn cancelled_error() -> anyhow::Error {
    DownloadCancelled.into()
}

pub(super) fn is_cancelled_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<DownloadCancelled>().is_some()
}

pub struct DownloadCancellation {
    flag: AtomicBool,
    notify: Notify,
}

impl DownloadCancellation {
    fn new() -> Self {
        Self {
            flag: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    pub(super) async fn cancelled(&self) {
        while !self.is_cancelled() {
            self.notify.notified().await;
        }
    }
}

pub async fn request_download_cancel(state: &Arc<AppState>, id: &str) -> bool {
    let mut active = state.active_download_cancellations.lock().await;
    let flag = active
        .entry(id.to_string())
        .or_insert_with(|| Arc::new(DownloadCancellation::new()));
    flag.cancel();
    true
}

pub(super) async fn register_active_download(
    state: &Arc<AppState>,
    id: &str,
) -> Arc<DownloadCancellation> {
    let mut active = state.active_download_cancellations.lock().await;
    Arc::clone(
        active
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(DownloadCancellation::new())),
    )
}

pub(super) async fn unregister_active_download(state: &Arc<AppState>, id: &str) {
    let mut active = state.active_download_cancellations.lock().await;
    active.remove(id);
}
