//! Chapter jobs seal their originals first, then append a GPU-generated section.
use crate::AppState;
use anyhow::{Context, Result, ensure};
use backend_storage::{PageVariant, UpscaledChapter};
use backend_upscale::UpscaleOutcome;
use std::sync::Arc;

pub(crate) async fn automatic_enabled(state: &AppState, source: &str) -> Result<bool> {
    let global = state
        .db
        .get_setting("auto_upscale")
        .await?
        .is_none_or(|value| value != "false");
    let per_source = super::settings::source_auto_upscale(&state.db, source).await?;
    Ok(global && per_source)
}

pub(crate) async fn run(state: Arc<AppState>, download_id: &str, scale: u32) -> Result<()> {
    ensure!(matches!(scale, 2 | 4), "upscale scale must be 2 or 4");
    let Some(download) = state.db.get_download_by_id(download_id).await? else {
        return Ok(());
    };
    if download.status != "completed" {
        return Ok(());
    }
    let archive = super::downloaded_archive_resolution::require_existing_completed_archive(
        &state.db,
        download_id,
    )
    .await?;
    let before = backend_storage::inspect(archive.archive_path.clone()).await?;
    let staging = tempfile::tempdir().context("create upscale staging directory")?;
    let mut pages = Vec::with_capacity(before.page_count);
    let mut models = std::collections::BTreeSet::new();
    let mut tile_size = 0;
    for index in 0..before.page_count {
        if state.shutdown_drain.is_draining() {
            anyhow::bail!("shutdown interrupted chapter upscaling");
        }
        if state.db.get_download_by_id(download_id).await?.is_none() {
            return Ok(());
        }
        let original = backend_storage::read_page(
            archive.archive_path.clone(),
            index,
            Some(PageVariant::Original),
        )
        .await?;
        // Release the mapping before GPU work: one page is staged at a time.
        let bytes = original.bytes.to_vec();
        drop(original);
        let page = match state.upscaler.upscale(bytes, scale).await? {
            UpscaleOutcome::Complete(page) => page,
            UpscaleOutcome::MissingModels { reason } => {
                tracing::warn!(download_id, reason, "Upscale skipped: no usable models");
                return Ok(());
            }
        };
        models.insert(page.model);
        tile_size = page.tile_size;
        let path = staging.path().join(format!("{index:06}.avif"));
        tokio::fs::write(&path, &page.bytes).await?;
        pages.push(path);
    }
    let model = models.into_iter().collect::<Vec<_>>().join(",");
    let cancel_state = Arc::clone(&state);
    backend_storage::append_upscaled(
        archive.archive_path.clone(),
        UpscaledChapter {
            pages,
            model: model.clone(),
            scale,
            tile_size,
            expected_footer_hash: before.footer_hash,
        },
        move || cancel_state.shutdown_drain.is_draining(),
    )
    .await?;
    let after = backend_storage::inspect(archive.archive_path).await?;
    state
        .db
        .mark_upscaled(download_id, &model, scale, after.file_size)
        .await?;
    crate::downloader::update_download_storage_usage_delta(
        &state,
        &archive.download_path,
        i64::try_from(after.file_size.saturating_sub(before.file_size)).unwrap_or(i64::MAX),
    )
    .await;
    super::route_snapshot_invalidation::downloaded_archive_metadata_changed(
        &state,
        download_id,
        &archive.download.chapter_id,
    );
    tracing::info!(
        download_id,
        model,
        scale,
        pages = before.page_count,
        "Upscaled chapter section published"
    );
    Ok(())
}
