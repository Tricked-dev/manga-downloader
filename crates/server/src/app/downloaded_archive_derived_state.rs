use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    AppState,
    api::dto::{ArchiveIndexCleanupResponse, ArchiveIndexJobResponse},
    app::{
        downloaded_archive_lifecycle::{
            DownloadedArchiveCreated, DownloadedArchiveDeleted, DownloadedArchiveMetadataChanged,
            DownloadedArchiveReencoded, DownloadedArchivesReencoded,
        },
        route_snapshot_invalidation,
    },
    archive_index::ArchiveIndexStaleCleanupResult,
};

pub(crate) async fn downloaded_archive_created(
    state: &Arc<AppState>,
    created: &DownloadedArchiveCreated,
    download_path: &str,
) {
    route_snapshot_invalidation::downloaded_archive_created(
        state,
        &created.download_id,
        &created.chapter_id,
    );
    state
        .extraction_scheduler
        .invalidate_archive(&created.archive_path.to_string_lossy())
        .await;
    rebuild_archive_index(
        state,
        &created.download_id,
        &created.chapter_id,
        created.archive_path.clone(),
    )
    .await;
    refresh_download_storage_usage(state, &created.download_id, download_path).await;
}

pub(crate) async fn downloaded_archive_deleted(
    state: &Arc<AppState>,
    deleted: &DownloadedArchiveDeleted,
) {
    route_snapshot_invalidation::downloaded_archive_deleted(
        state,
        &deleted.download_id,
        &deleted.chapter_id,
    );
    state.archive_index.invalidate_chapter(&deleted.chapter_id);
    state
        .archive_index
        .invalidate_archive_path(&deleted.archive_path);
    state
        .extraction_scheduler
        .invalidate_archive(&deleted.archive_path.to_string_lossy())
        .await;
    crate::downloader::update_download_storage_usage_delta(
        state,
        &deleted.download_path,
        crate::downloader::negative_storage_delta(deleted.removed_size_bytes),
    )
    .await;
}

pub(crate) async fn downloaded_archive_reencoded(
    state: &Arc<AppState>,
    reencoded: &DownloadedArchiveReencoded,
) {
    route_snapshot_invalidation::downloaded_archive_reencoded(
        state,
        &reencoded.download_id,
        &reencoded.chapter_id,
    );
    downloaded_archive_representation_changed(
        state,
        &reencoded.download_id,
        &reencoded.chapter_id,
        &reencoded.download_path,
        &reencoded.archive_path,
    )
    .await;
}

pub(crate) async fn downloaded_archive_metadata_changed(
    state: &Arc<AppState>,
    changed: &DownloadedArchiveMetadataChanged,
) {
    route_snapshot_invalidation::downloaded_archive_metadata_changed(
        state,
        &changed.download_id,
        &changed.chapter_id,
    );
    downloaded_archive_representation_changed(
        state,
        &changed.download_id,
        &changed.chapter_id,
        &changed.download_path,
        &changed.archive_path,
    )
    .await;
}

pub(crate) async fn downloaded_archives_bulk_reencoded(
    state: &Arc<AppState>,
    reencoded: &DownloadedArchivesReencoded,
    download_path: &str,
) {
    route_snapshot_invalidation::downloaded_archives_bulk_reencoded(state);
    for archive in &reencoded.archives {
        state.archive_index.invalidate_chapter(&archive.chapter_id);
        state
            .archive_index
            .invalidate_archive_path(&archive.archive_path);
        state
            .extraction_scheduler
            .invalidate_archive(&archive.archive_path.to_string_lossy())
            .await;
        rebuild_archive_index(
            state,
            &archive.download_id,
            &archive.chapter_id,
            archive.archive_path.clone(),
        )
        .await;
    }
    crate::downloader::invalidate_download_storage_usage(state).await;
    refresh_download_storage_usage(state, "bulk", download_path).await;
}

#[allow(clippy::unused_async)]
pub(crate) async fn downloaded_archive_index_rebuild_accepted(
    state: &Arc<AppState>,
    response: &ArchiveIndexJobResponse,
) {
    route_snapshot_invalidation::archive_index_maintenance_completed(state);
    tracing::info!(
        running = response.running,
        trigger = %response.trigger,
        "Archive Index Rebuild Accepted",
    );
}

pub(crate) async fn downloaded_archive_index_rebuild_finished(
    state: &Arc<AppState>,
    response: &ArchiveIndexJobResponse,
) {
    state.extraction_scheduler.clear().await;
    route_snapshot_invalidation::archive_index_maintenance_completed(state);
    tracing::info!(
        indexed_archives = response.indexed_archives,
        failed_archives = response.failed_archives,
        skipped_archives = response.skipped_archives,
        removed_rows = response.removed_rows,
        trigger = %response.trigger,
        "Archive Index Rebuild Finished",
    );
}

pub(crate) async fn stale_downloaded_archive_index_rows_cleaned(
    state: &Arc<AppState>,
    cleaned: &ArchiveIndexStaleCleanupResult,
) {
    for archive_path in &cleaned.affected_archive_paths {
        state
            .extraction_scheduler
            .invalidate_archive(archive_path)
            .await;
    }
    route_snapshot_invalidation::archive_index_maintenance_completed(state);
    tracing::info!(
        removed_rows = cleaned.response.removed_rows,
        affected_archive_paths = cleaned.affected_archive_paths.len(),
        "Archive Index Stale Cleanup Completed",
    );
}

pub(crate) async fn downloaded_archive_index_cleared(
    state: &Arc<AppState>,
    response: &ArchiveIndexCleanupResponse,
) {
    state.extraction_scheduler.clear().await;
    route_snapshot_invalidation::archive_index_maintenance_completed(state);
    tracing::info!(
        removed_rows = response.removed_rows,
        "Archive Index Cleared",
    );
}

pub(crate) async fn stale_downloaded_archive_identity_detected(
    state: &AppState,
    archive_path: &std::path::Path,
    chapter_id: Option<&str>,
    mode: &'static str,
) {
    if let Some(chapter_id) = chapter_id {
        state.archive_index.invalidate_chapter(chapter_id);
    }
    state.archive_index.invalidate_archive_path(archive_path);
    state
        .extraction_scheduler
        .invalidate_archive(&archive_path.to_string_lossy())
        .await;
    tracing::debug!(
        chapter_id,
        archive_path = %archive_path.display(),
        mode,
        "Stale Downloaded Archive Identity Cleanup Completed",
    );
}

async fn downloaded_archive_representation_changed(
    state: &Arc<AppState>,
    download_id: &str,
    chapter_id: &str,
    download_path: &str,
    archive_path: &Path,
) {
    state.archive_index.invalidate_chapter(chapter_id);
    state.archive_index.invalidate_archive_path(archive_path);
    state
        .extraction_scheduler
        .invalidate_archive(&archive_path.to_string_lossy())
        .await;
    rebuild_archive_index(state, download_id, chapter_id, archive_path.to_path_buf()).await;
    refresh_download_storage_usage(state, download_id, download_path).await;
}

async fn rebuild_archive_index(
    state: &Arc<AppState>,
    download_id: &str,
    chapter_id: &str,
    archive_path: PathBuf,
) {
    if let Err(error) = state
        .archive_index
        .index_archive_after_download(
            &state.db,
            &state.telemetry.metrics,
            chapter_id,
            archive_path.clone(),
        )
        .await
    {
        tracing::warn!(
            download_id = %download_id,
            chapter_id = %chapter_id,
            archive_path = %archive_path.display(),
            error = %error,
            "Downloaded Archive Index Build Failed",
        );
    }
}

async fn refresh_download_storage_usage(
    state: &Arc<AppState>,
    download_id: &str,
    download_path: &str,
) {
    if let Err(error) =
        crate::downloader::get_download_storage_usage_bytes(state, download_path).await
    {
        tracing::warn!(
            download_id = %download_id,
            download_path,
            error = %error,
            "Downloaded Archive Storage Usage Refresh Failed",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::response_cache, server::build_test_app_state};
    use std::fs;

    #[tokio::test(flavor = "multi_thread")]
    async fn stale_index_cleanup_invalidates_only_affected_scheduler_state_and_route_snapshot() {
        let state = test_state("stale-index-cleanup").await;
        let archive_path = "/tmp/stale-index.cbz";
        let affected_archive_key = format!("{archive_path}:2:10:20");
        let unaffected_archive_key = "/tmp/current-index.cbz:2:10:20";
        seed_expanded_read_window(&state, &affected_archive_key).await;
        seed_expanded_read_window(&state, unaffected_archive_key).await;
        seed_archive_index_route_snapshot(&state);

        stale_downloaded_archive_index_rows_cleaned(
            &state,
            &ArchiveIndexStaleCleanupResult {
                response: ArchiveIndexCleanupResponse {
                    ok: true,
                    removed_rows: 3,
                },
                affected_archive_paths: vec![archive_path.to_string()],
            },
        )
        .await;

        assert_route_snapshot_invalidated(&state).await;
        assert_eq!(
            state
                .extraction_scheduler
                .foreground_read_window_size(&affected_archive_key, 2, 8)
                .await,
            1
        );
        assert_eq!(
            state
                .extraction_scheduler
                .foreground_read_window_size(unaffected_archive_key, 2, 8)
                .await,
            8
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn clearing_or_rebuilding_archive_index_clears_all_scheduler_state() {
        let state = test_state("archive-index-clear").await;
        let archive_key = "/tmp/cleared-index.cbz:2:10:20";
        seed_expanded_read_window(&state, archive_key).await;
        seed_archive_index_route_snapshot(&state);

        downloaded_archive_index_cleared(
            &state,
            &ArchiveIndexCleanupResponse {
                ok: true,
                removed_rows: 4,
            },
        )
        .await;

        assert_route_snapshot_invalidated(&state).await;
        assert_eq!(
            state
                .extraction_scheduler
                .foreground_read_window_size(archive_key, 2, 8)
                .await,
            1
        );

        seed_expanded_read_window(&state, archive_key).await;
        downloaded_archive_index_rebuild_finished(
            &state,
            &ArchiveIndexJobResponse {
                running: false,
                trigger: "manual".to_string(),
                indexed_archives: 1,
                skipped_archives: 0,
                failed_archives: 0,
                removed_rows: 0,
                message: "done".to_string(),
            },
        )
        .await;

        assert_eq!(
            state
                .extraction_scheduler
                .foreground_read_window_size(archive_key, 2, 8)
                .await,
            1
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stale_reader_identity_cleanup_invalidates_scheduler_without_route_snapshots() {
        let state = test_state("stale-reader-identity").await;
        let archive_path = std::path::Path::new("/tmp/stale-reader.cbz");
        let archive_key = "/tmp/stale-reader.cbz:2:10:20";
        seed_expanded_read_window(&state, archive_key).await;
        seed_archive_index_route_snapshot(&state);

        stale_downloaded_archive_identity_detected(
            &state,
            archive_path,
            Some("chapter-1"),
            "foreground",
        )
        .await;

        assert!(
            state
                .cache
                .get_api_bytes(&response_cache::route_snapshot_key(
                    "settings:archive-index"
                ))
                .await
                .is_some()
        );
        assert_eq!(
            state
                .extraction_scheduler
                .foreground_read_window_size(archive_key, 2, 8)
                .await,
            1
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bulk_reencoded_continues_index_repair_after_per_archive_failure() {
        let state = test_state("bulk-reencode-repair").await;
        let root = crate::test_support::temp_path("bulk-reencode-repair-archives");
        fs::create_dir_all(&root).expect("archive fixture root should be created");
        let missing_archive = root.join("missing.cbz");
        let valid_archive = root.join("valid.cbz");
        write_test_archive(&valid_archive, &[("001.png", valid_png_page_bytes())]);

        downloaded_archives_bulk_reencoded(
            &state,
            &DownloadedArchivesReencoded {
                files_processed: 2,
                images_reencoded: 1,
                archives: vec![
                    crate::app::downloaded_archive_lifecycle::DownloadedArchiveReencodedItem {
                        download_id: "download-missing".to_string(),
                        chapter_id: "chapter-missing".to_string(),
                        archive_path: missing_archive,
                    },
                    crate::app::downloaded_archive_lifecycle::DownloadedArchiveReencodedItem {
                        download_id: "download-valid".to_string(),
                        chapter_id: "chapter-valid".to_string(),
                        archive_path: valid_archive,
                    },
                ],
            },
            &root.to_string_lossy(),
        )
        .await;

        assert!(
            state
                .archive_index
                .cached_chapter_pages("chapter-valid")
                .is_some()
        );
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
        build_test_app_state(name, "manga_server_derived_state_test").await
    }

    fn write_test_archive(path: &std::path::Path, entries: &[(&str, &[u8])]) {
        let page_dir = path
            .parent()
            .expect("archive path should have a parent")
            .join("fixture-pages");
        fs::create_dir_all(&page_dir).expect("page fixture dir should be created");
        let pages = entries
            .iter()
            .map(|(name, bytes)| {
                let path = page_dir.join(name);
                fs::write(&path, bytes).expect("page fixture should be written");
                ((*name).to_string(), path)
            })
            .collect::<Vec<_>>();
        backend_image::build_zstd_folder_from_paths(&pages, path, None, None)
            .expect("archive should be built");
    }

    fn valid_png_page_bytes() -> &'static [u8] {
        &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 10, 73, 68, 65, 84, 120, 156, 99, 0, 1, 0, 0,
            5, 0, 1, 13, 10, 45, 180, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ]
    }
}
