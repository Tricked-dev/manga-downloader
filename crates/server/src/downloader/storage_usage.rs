use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use byte_unit::{Byte, UnitType};

use crate::{AppState, app::settings};

const DOWNLOAD_STORAGE_USAGE_REFRESH: Duration = Duration::from_secs(30);

pub(crate) fn positive_storage_delta(bytes: u64) -> i64 {
    i64::try_from(bytes).unwrap_or(i64::MAX)
}

pub(crate) fn negative_storage_delta(bytes: u64) -> i64 {
    -positive_storage_delta(bytes)
}

#[derive(Clone, Copy)]
pub(super) struct DownloadStorageReservation {
    pub(super) archive_size_bytes: u64,
    pub(super) replaced_size_bytes: u64,
}

pub(super) async fn enforce_download_storage_limit(
    state: &Arc<AppState>,
    download: &backend_persistence::DownloadRow,
    download_path: &str,
) -> Result<()> {
    let Some(limit_bytes) = settings::max_download_storage_bytes(&state.db).await? else {
        return Ok(());
    };

    let usage_bytes = get_download_storage_usage_bytes(state, download_path).await?;
    if usage_bytes < limit_bytes {
        return Ok(());
    }

    let limit = format_bytes(limit_bytes);
    let current = format_bytes(usage_bytes);
    Err(anyhow::anyhow!(
        "Download storage limit reached: using {current} of {limit}. Increase the storage cap or remove downloads before retrying \"{}\".",
        download.chapter_title
    ))
}

pub(super) async fn move_archive_into_library(
    staged_path: &Path,
    archive_path: &Path,
) -> Result<()> {
    backend_storage::publish(staged_path.to_path_buf(), archive_path.to_path_buf()).await
}

pub(super) async fn reserve_download_storage_for_archive(
    state: &Arc<AppState>,
    download: &backend_persistence::DownloadRow,
    download_path: &str,
    archive_path: &Path,
    archive_size_bytes: u64,
) -> Result<DownloadStorageReservation> {
    let limit_bytes = settings::max_download_storage_bytes(&state.db).await?;
    let requested_path = PathBuf::from(download_path);
    let mut usage = state.download_storage_usage.lock().await;
    refresh_download_storage_usage_locked(&mut usage, &requested_path)?;

    let replaced_size_bytes = file_size_if_present(archive_path)?;
    let next_usage_bytes = usage
        .bytes
        .saturating_sub(replaced_size_bytes)
        .saturating_add(archive_size_bytes);

    if let Some(limit_bytes) = limit_bytes
        && next_usage_bytes > limit_bytes
    {
        let limit = format_bytes(limit_bytes);
        let current = format_bytes(usage.bytes);
        let archive_size = format_bytes(archive_size_bytes);
        return Err(anyhow::anyhow!(
            "Download storage limit reached: using {current} of {limit}; \"{}\" needs {archive_size}. Increase the storage cap or remove downloads before retrying.",
            download.chapter_title
        ));
    }

    usage.bytes = next_usage_bytes;
    usage.refreshed_at = Some(Instant::now());

    Ok(DownloadStorageReservation {
        archive_size_bytes,
        replaced_size_bytes,
    })
}

pub(super) async fn rollback_download_storage_reservation(
    state: &Arc<AppState>,
    download_path: &str,
    reservation: DownloadStorageReservation,
) {
    update_download_storage_usage_delta(
        state,
        download_path,
        storage_delta(
            reservation.replaced_size_bytes,
            reservation.archive_size_bytes,
        ),
    )
    .await;
}

pub(crate) async fn get_download_storage_usage_bytes(
    state: &Arc<AppState>,
    download_path: &str,
) -> Result<u64> {
    let requested_path = PathBuf::from(download_path);
    let mut usage = state.download_storage_usage.lock().await;
    refresh_download_storage_usage_locked(&mut usage, &requested_path)?;
    Ok(usage.bytes)
}

pub(crate) async fn cached_download_storage_usage_bytes(
    state: &Arc<AppState>,
    download_path: &str,
) -> Option<u64> {
    let requested_path = std::path::Path::new(download_path);
    let usage = state.download_storage_usage.lock().await;
    if usage.path.as_deref() == Some(requested_path) {
        Some(usage.bytes)
    } else {
        None
    }
}

pub(crate) async fn refresh_download_storage_usage_if_stale(
    state: Arc<AppState>,
    download_path: String,
) {
    let requested_path = PathBuf::from(download_path);
    let should_refresh = {
        let mut usage = state.download_storage_usage.lock().await;
        if usage.refresh_in_progress
            || !should_refresh_download_storage_usage(&usage, &requested_path)
        {
            false
        } else {
            usage.refresh_in_progress = true;
            true
        }
    };

    if !should_refresh {
        return;
    }

    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking({
            let requested_path = requested_path.clone();
            move || directory_size_bytes(&requested_path)
        })
        .await
        .context("download storage usage scan task failed")
        .and_then(std::convert::identity);

        let mut usage = state.download_storage_usage.lock().await;
        usage.refresh_in_progress = false;
        match result {
            Ok(bytes) => {
                usage.path = Some(requested_path);
                usage.bytes = bytes;
                usage.refreshed_at = Some(Instant::now());
            }
            Err(error) => {
                tracing::warn!(error = %error, "Download storage usage background refresh failed");
            }
        }
    });
}

pub(crate) async fn update_download_storage_usage_delta(
    state: &Arc<AppState>,
    download_path: &str,
    delta_bytes: i64,
) {
    let mut usage = state.download_storage_usage.lock().await;
    if usage.path.as_deref() != Some(std::path::Path::new(download_path)) {
        return;
    }

    if delta_bytes >= 0 {
        usage.bytes = usage.bytes.saturating_add(delta_bytes.unsigned_abs());
    } else {
        usage.bytes = usage.bytes.saturating_sub(delta_bytes.unsigned_abs());
    }
    usage.refreshed_at = Some(Instant::now());
}

pub(crate) async fn invalidate_download_storage_usage(state: &Arc<AppState>) {
    let mut usage = state.download_storage_usage.lock().await;
    *usage = crate::DownloadStorageUsage::default();
}

fn directory_size_bytes(root: &std::path::Path) -> Result<u64> {
    backend_fs::directory_size_bytes(root)
}

fn refresh_download_storage_usage_locked(
    usage: &mut crate::DownloadStorageUsage,
    requested_path: &Path,
) -> Result<()> {
    if !should_refresh_download_storage_usage(usage, requested_path) {
        return Ok(());
    }

    let usage_bytes = directory_size_bytes(requested_path).with_context(|| {
        format!(
            "Failed to scan download storage usage in {}",
            requested_path.display()
        )
    })?;
    usage.path = Some(requested_path.to_path_buf());
    usage.bytes = usage_bytes;
    usage.refreshed_at = Some(Instant::now());
    usage.refresh_in_progress = false;
    Ok(())
}

fn should_refresh_download_storage_usage(
    usage: &crate::DownloadStorageUsage,
    requested_path: &Path,
) -> bool {
    usage.path.as_deref() != Some(requested_path)
        || usage
            .refreshed_at
            .is_none_or(|refreshed_at| refreshed_at.elapsed() >= DOWNLOAD_STORAGE_USAGE_REFRESH)
}

fn file_size_if_present(path: &Path) -> Result<u64> {
    backend_fs::file_size_if_present_sync(path)
}

fn storage_delta(add_bytes: u64, subtract_bytes: u64) -> i64 {
    let delta = i128::from(add_bytes) - i128::from(subtract_bytes);
    if delta > i128::from(i64::MAX) {
        i64::MAX
    } else if delta < i128::from(i64::MIN) {
        i64::MIN
    } else {
        i64::try_from(delta).expect("delta is clamped to i64 range")
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let adjusted = Byte::from_u64(bytes).get_appropriate_unit(UnitType::Binary);
    format!("{adjusted:.1}")
}
