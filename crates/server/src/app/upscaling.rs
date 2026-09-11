//! Chapter jobs seal their originals first, then append a model-generated section.
use crate::AppState;
use anyhow::{Context, Result, ensure};
use backend_storage::{PageVariant, UpscaledChapter};
use backend_upscale::UpscaleOutcome;
use std::sync::Arc;

pub(crate) async fn automatic_enabled(
    state: &AppState,
    source: &str,
    manga_id: &str,
) -> Result<bool> {
    if let Some(value) = state
        .db
        .get_setting(&format!("series.{manga_id}.auto_upscale"))
        .await?
        && value != "inherit"
    {
        return Ok(value == "true");
    }
    let global = state
        .db
        .get_setting("auto_upscale")
        .await?
        .is_none_or(|value| value != "false");
    let per_source = super::settings::source_auto_upscale(&state.db, source).await?;
    Ok(global && per_source)
}

/// Waiting here rather than declining the job keeps its place in the queue. The worker
/// stays alive and heartbeating throughout, so nothing reclaims the lease.
async fn wait_while_paused(state: &Arc<AppState>, download_id: &str) -> Result<()> {
    let mut announced = false;
    while super::settings::upscale_paused(&state.db).await {
        if !announced {
            let previous = state.db.get_upscale_progress(download_id).await?;
            state
                .db
                .set_upscale_progress(
                    download_id,
                    "paused",
                    previous.as_ref().map_or(0, |p| p.completed_pages as usize),
                    previous.as_ref().map_or(0, |p| p.total_pages as usize),
                    "Upscaling is paused",
                )
                .await?;
            announced = true;
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    Ok(())
}

pub(crate) async fn run(state: Arc<AppState>, download_id: &str, scale: u32) -> Result<()> {
    wait_while_paused(&state, download_id).await?;
    state
        .db
        .set_upscale_progress(download_id, "running", 0, 0, "Inspecting originals")
        .await?;
    let result = run_inner(Arc::clone(&state), download_id, scale).await;
    if let Err(error) = &result {
        let previous = state.db.get_upscale_progress(download_id).await?;
        state
            .db
            .set_upscale_progress(
                download_id,
                "failed",
                previous.as_ref().map_or(0, |p| p.completed_pages as usize),
                previous.as_ref().map_or(0, |p| p.total_pages as usize),
                &error.to_string(),
            )
            .await?;
    }
    result
}

async fn run_inner(state: Arc<AppState>, download_id: &str, scale: u32) -> Result<()> {
    ensure!(matches!(scale, 2 | 4), "upscale scale must be 2 or 4");
    let Some(download) = state.db.get_download_by_id(download_id).await? else {
        state
            .db
            .set_upscale_progress(download_id, "skipped", 0, 0, "Download no longer exists")
            .await?;
        return Ok(());
    };
    if download.status != "completed" {
        state
            .db
            .set_upscale_progress(download_id, "skipped", 0, 0, "Download is not completed")
            .await?;
        return Ok(());
    }
    if download.upscaled_at.is_some() {
        state
            .db
            .set_upscale_progress(
                download_id,
                "completed",
                0,
                0,
                "Upscaled pages already available",
            )
            .await?;
        return Ok(());
    }
    let archive = super::downloaded_archive_resolution::require_existing_completed_archive(
        &state.db,
        download_id,
    )
    .await?;
    let before = backend_storage::inspect(archive.archive_path.clone()).await?;
    let source_width = median_original_width(&archive.archive_path, before.page_count).await?;
    if source_width >= 2000 {
        state
            .db
            .set_upscale_progress(
                download_id,
                "skipped",
                0,
                before.page_count,
                "Originals are already at least 2000 pixels wide",
            )
            .await?;
        tracing::info!(
            download_id,
            source_width,
            "Upscale skipped: chapter originals are already at least 2000 pixels wide"
        );
        return Ok(());
    }
    let staging = tempfile::tempdir().context("create upscale staging directory")?;
    let mut pages = Vec::with_capacity(before.page_count);
    let mut models = std::collections::BTreeSet::new();
    let mut tile_size = 0;
    state
        .db
        .set_upscale_progress(
            download_id,
            "running",
            0,
            before.page_count,
            "Upscaling pages",
        )
        .await?;
    for index in 0..before.page_count {
        if state.shutdown_drain.is_draining() {
            anyhow::bail!("shutdown interrupted chapter upscaling");
        }
        if state.db.get_download_by_id(download_id).await?.is_none() {
            state
                .db
                .set_upscale_progress(
                    download_id,
                    "skipped",
                    index,
                    before.page_count,
                    "Download removed",
                )
                .await?;
            return Ok(());
        }
        let original = backend_storage::read_page(
            archive.archive_path.clone(),
            index,
            Some(PageVariant::Original),
        )
        .await?;
        // Release the mapping before inference: one page is staged at a time.
        let bytes = original.bytes.to_vec();
        drop(original);
        let page = match state.upscaler.upscale(bytes, scale).await? {
            UpscaleOutcome::Complete(page) => page,
            UpscaleOutcome::MissingModels { reason } => {
                tracing::warn!(download_id, reason, "Upscale skipped: no usable models");
                state
                    .db
                    .set_upscale_progress(download_id, "skipped", index, before.page_count, &reason)
                    .await?;
                return Ok(());
            }
        };
        models.insert(page.model);
        tile_size = page.tile_size;
        let path = staging.path().join(format!("{index:06}.avif"));
        tokio::fs::write(&path, &page.bytes).await?;
        pages.push(path);
        state
            .db
            .set_upscale_progress(
                download_id,
                "running",
                index + 1,
                before.page_count,
                "Upscaling pages",
            )
            .await?;
    }
    let model = models.into_iter().collect::<Vec<_>>().join(",");
    let cancel_state = Arc::clone(&state);
    state
        .db
        .set_upscale_progress(
            download_id,
            "running",
            before.page_count,
            before.page_count,
            "Saving upscaled pages",
        )
        .await?;
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
    state
        .db
        .set_upscale_progress(
            download_id,
            "completed",
            before.page_count,
            before.page_count,
            "Upscaled pages ready",
        )
        .await?;
    tracing::info!(
        download_id,
        model,
        scale,
        pages = before.page_count,
        "Upscaled chapter section published"
    );
    Ok(())
}

// The median describes chapter quality without treating a wide spread or a narrow
// credits page as the resolution of the entire chapter. This never resizes originals.
async fn median_original_width(path: &std::path::Path, page_count: usize) -> Result<u32> {
    let mut widths = Vec::with_capacity(page_count);
    for index in 0..page_count {
        let page =
            backend_storage::read_page(path.to_path_buf(), index, Some(PageVariant::Original))
                .await?;
        widths.push(
            tokio::task::spawn_blocking(move || -> Result<u32> {
                Ok(backend_image::image_dimensions(&page.bytes)?.0)
            })
            .await??,
        );
    }
    ensure!(!widths.is_empty(), "cannot upscale an empty chapter");
    widths.sort_unstable();
    Ok(widths[widths.len() / 2])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn resolution_policy_uses_chapter_pages_and_preserves_originals() {
        for (widths, expected) in [
            (vec![1400, 1400, 2800], 1400),
            (vec![700, 2000, 2200], 2000),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let mut pages = Vec::new();
            for (index, width) in widths.iter().enumerate() {
                let path = directory.path().join(format!("{index}.png"));
                image::RgbImage::from_pixel(*width, 8, image::Rgb([12, 34, 56]))
                    .save(&path)
                    .unwrap();
                pages.push(path);
            }
            let archive = directory.path().join("chapter.bbf");
            backend_storage::write_originals(
                archive.clone(),
                backend_storage::OriginalChapter {
                    pages,
                    comicinfo_xml: "<ComicInfo/>".into(),
                    cover: None,
                },
                || false,
            )
            .await
            .unwrap();
            let before = tokio::fs::read(&archive).await.unwrap();
            assert_eq!(
                median_original_width(&archive, widths.len()).await.unwrap(),
                expected
            );
            assert_eq!(tokio::fs::read(archive).await.unwrap(), before);
        }
    }
}

#[cfg(test)]
mod series_tests {
    use super::*;

    #[tokio::test]
    async fn series_override_takes_precedence_and_progress_survives_reads() {
        let state = crate::server::build_test_app_state("series-upscale", "series-upscale").await;
        state.db.set_setting("auto_upscale", "false").await.unwrap();
        state
            .db
            .set_setting("source.rawkuma.auto_upscale", "false")
            .await
            .unwrap();
        assert!(
            !automatic_enabled(&state, "rawkuma", "chosen")
                .await
                .unwrap()
        );
        state
            .db
            .set_setting("series.chosen.auto_upscale", "true")
            .await
            .unwrap();
        assert!(
            automatic_enabled(&state, "rawkuma", "chosen")
                .await
                .unwrap()
        );
        assert!(!automatic_enabled(&state, "rawkuma", "other").await.unwrap());
        state.db.set_setting("auto_upscale", "true").await.unwrap();
        state
            .db
            .set_setting("source.rawkuma.auto_upscale", "true")
            .await
            .unwrap();
        state
            .db
            .set_setting("series.chosen.auto_upscale", "false")
            .await
            .unwrap();
        assert!(
            !automatic_enabled(&state, "rawkuma", "chosen")
                .await
                .unwrap()
        );
        state
            .db
            .set_setting("series.chosen.auto_upscale", "inherit")
            .await
            .unwrap();
        assert!(
            automatic_enabled(&state, "rawkuma", "chosen")
                .await
                .unwrap()
        );
        state
            .db
            .set_upscale_progress("download", "running", 3, 12, "Upscaling pages")
            .await
            .unwrap();
        let progress = state
            .db
            .get_upscale_progress("download")
            .await
            .unwrap()
            .unwrap();
        assert_eq!((progress.completed_pages, progress.total_pages), (3, 12));
        state
            .db
            .set_upscale_progress("download", "failed", 3, 12, "Inference failed")
            .await
            .unwrap();
        let progress = state
            .db
            .get_upscale_progress("download")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(progress.status, "failed");
        assert_eq!(progress.message, "Inference failed");
        state.upscaler.shutdown().await.unwrap();
    }
}
