use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_PATH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn temp_path(label: &str) -> PathBuf {
    let process_id = std::process::id();
    let sequence = TEMP_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("manga-server-{label}-{process_id}-{sequence}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_path_returns_distinct_paths_for_reused_labels() {
        let first = temp_path("state");
        let second = temp_path("state");

        assert_ne!(first, second);
    }
}
