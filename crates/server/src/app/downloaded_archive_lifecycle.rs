use std::{path::PathBuf, sync::Arc};

use crate::{
    AppState,
    api::error::AppError,
    app::{downloaded_archive_resolution, route_snapshot_invalidation},
};
use autometrics::autometrics;

#[allow(dead_code)]
pub(crate) struct DownloadedArchiveCreated {
    pub(crate) download_id: String,
    pub(crate) chapter_id: String,
    pub(crate) archive_path: PathBuf,
    pub(crate) page_count: usize,
    pub(crate) archive_size_bytes: u64,
}

#[allow(dead_code)]
pub(crate) struct DownloadedArchiveDeleted {
    pub(crate) download_id: String,
    pub(crate) chapter_id: String,
    pub(crate) download_path: String,
    pub(crate) archive_path: PathBuf,
    pub(crate) removed_size_bytes: u64,
}

pub(crate) struct DownloadedArchiveMetadataChanged {
    pub(crate) download_id: String,
    pub(crate) chapter_id: String,
    pub(crate) download_path: String,
    pub(crate) archive_path: PathBuf,
}

#[autometrics(track_concurrency)]
pub(crate) async fn downloaded_archive_created(
    state: &Arc<AppState>,
    download_id: &str,
    page_count: usize,
    archive_path: PathBuf,
    archive_size_bytes: u64,
) -> Result<DownloadedArchiveCreated, AppError> {
    let download = downloaded_archive_resolution::require_download(&state.db, download_id).await?;
    let archive_resolver = downloaded_archive_resolution::path_resolver(&state.db).await?;

    state
        .db
        .complete_download(
            download_id,
            page_count,
            &archive_path.to_string_lossy(),
            archive_size_bytes,
        )
        .await?;

    let created = DownloadedArchiveCreated {
        download_id: download_id.to_string(),
        chapter_id: download.chapter_id,
        archive_path,
        page_count,
        archive_size_bytes,
    };
    route_snapshot_invalidation::downloaded_archive_created(
        state,
        &created.download_id,
        &created.chapter_id,
    );
    refresh_storage(state, archive_resolver.download_path()).await;
    match super::upscaling::automatic_enabled(state, &download.manga_source).await {
        Ok(true) => {
            if let Err(error) = crate::jobs::enqueue_upscale(state, download_id, 2).await {
                tracing::error!(download_id, error = %error, "Failed to queue automatic upscale");
            }
        }
        Ok(false) => {}
        Err(error) => {
            tracing::error!(download_id, error = %error, "Failed to read automatic upscale settings")
        }
    }

    tracing::info!(
        download_id = %created.download_id,
        chapter_id = %created.chapter_id,
        archive_path = %created.archive_path.display(),
        page_count,
        archive_bytes = archive_size_bytes,
        "Downloaded Archive Created",
    );

    Ok(created)
}

#[autometrics(track_concurrency)]
pub(crate) async fn downloaded_archive_deleted(
    state: &Arc<AppState>,
    download_id: &str,
) -> Result<DownloadedArchiveDeleted, AppError> {
    let archive =
        downloaded_archive_resolution::require_completed_archive(&state.db, download_id).await?;
    let removed_size_bytes = backend_storage::remove(archive.archive_path.clone()).await?;

    state
        .db
        .delete_download_and_reset_chapter(&archive.download.id, &archive.download.chapter_id)
        .await?;

    let deleted = DownloadedArchiveDeleted {
        download_id: archive.download.id,
        chapter_id: archive.download.chapter_id,
        download_path: archive.download_path,
        archive_path: archive.archive_path,
        removed_size_bytes,
    };
    route_snapshot_invalidation::downloaded_archive_deleted(
        state,
        &deleted.download_id,
        &deleted.chapter_id,
    );
    crate::downloader::update_download_storage_usage_delta(
        state,
        &deleted.download_path,
        crate::downloader::negative_storage_delta(deleted.removed_size_bytes),
    )
    .await;

    tracing::info!(
        download_id = %deleted.download_id,
        chapter_id = %deleted.chapter_id,
        archive_path = %deleted.archive_path.display(),
        archive_bytes = removed_size_bytes,
        "Downloaded Archive Deleted",
    );

    Ok(deleted)
}

#[autometrics(track_concurrency)]
pub(crate) async fn downloaded_archive_metadata_changed(
    state: &Arc<AppState>,
    changed: &DownloadedArchiveMetadataChanged,
) {
    route_snapshot_invalidation::downloaded_archive_metadata_changed(
        state,
        &changed.download_id,
        &changed.chapter_id,
    );
    refresh_storage(state, &changed.download_path).await;

    tracing::info!(
        download_id = %changed.download_id,
        chapter_id = %changed.chapter_id,
        archive_path = %changed.archive_path.display(),
        "Downloaded Archive Metadata Changed",
    );
}

async fn refresh_storage(state: &Arc<AppState>, download_path: &str) {
    crate::downloader::invalidate_download_storage_usage(state).await;
    if let Err(error) =
        crate::downloader::get_download_storage_usage_bytes(state, download_path).await
    {
        tracing::warn!(error = %error, "Failed to refresh chapter storage usage");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::build_test_app_state;
    use backend_persistence::{ChapterInsert, DownloadWorkTransition, MangaInsert};
    use std::{fs, path::Path};

    #[test]
    fn creation_and_deletion_coordinate_downloaded_archive_state() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("test runtime should initialize");

        runtime.block_on(async {
            let state = test_state().await;
            let (download_id, chapter_id, archive_path) = seed_queued_download(&state).await;
            backend_fs::create_dir_all(
                archive_path
                    .parent()
                    .expect("archive path should have a parent"),
            )
            .await
            .expect("archive directory should be created");
            write_test_archive(
                &archive_path,
                &[
                    ("001.png", valid_png_page_bytes()),
                    ("002.png", valid_png_page_bytes()),
                    ("003.png", valid_png_page_bytes()),
                ],
            )
            .await;
            let size = file_size(&archive_path);

            let created =
                downloaded_archive_created(&state, &download_id, 3, archive_path.clone(), size)
                    .await
                    .expect("creation should record the sealed BBF");
            assert_eq!(created.download_id, download_id);
            assert_eq!(created.chapter_id, chapter_id);
            assert_eq!(created.archive_path, archive_path);
            assert_eq!(created.page_count, 3);
            assert_eq!(created.archive_size_bytes, size);

            let chapter = state
                .db
                .get_chapter_by_id(&chapter_id)
                .await
                .expect("chapter lookup should succeed")
                .expect("chapter should exist");
            assert!(chapter.downloaded);
            assert!(!chapter.is_new);

            let overview = state
                .db
                .get_stats_overview()
                .await
                .expect("stats should load");
            assert_eq!(overview.totals.chapters_downloaded, 1);
            assert_eq!(overview.totals.pages_downloaded, 3);
            assert!(
                overview
                    .activity
                    .iter()
                    .any(|point| { point.chapters_downloaded == 1 && point.pages_downloaded == 3 }),
                "downloaded archive creation should record activity facts"
            );

            let deleted = downloaded_archive_deleted(&state, &download_id)
                .await
                .expect("completed archive deletion should succeed");
            assert_eq!(deleted.download_id, download_id);
            assert_eq!(deleted.chapter_id, chapter_id);
            assert_eq!(deleted.archive_path, archive_path);
            assert_eq!(deleted.removed_size_bytes, size);
            assert!(!backend_fs::path_exists(&deleted.archive_path));
            assert!(
                state
                    .db
                    .get_download_by_id(&download_id)
                    .await
                    .expect("download lookup should succeed")
                    .is_none()
            );

            let chapter = state
                .db
                .get_chapter_by_id(&chapter_id)
                .await
                .expect("chapter lookup should succeed")
                .expect("chapter should still exist");
            assert!(!chapter.downloaded);
        });
    }

    #[test]
    fn failed_download_cleanup_stays_outside_downloaded_archive_lifecycle() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("test runtime should initialize");

        runtime.block_on(async {
            let state = test_state().await;
            let (download_id, chapter_id, archive_path) = seed_queued_download(&state).await;
            backend_fs::create_dir_all(
                archive_path
                    .parent()
                    .expect("archive path should have a parent"),
            )
            .await
            .expect("archive directory should be created");
            tokio::fs::write(&archive_path, b"partial archive")
                .await
                .expect("partial archive should be written");
            state
                .db
                .update_download_status(
                    &download_id,
                    DownloadWorkTransition::Failed,
                    42.0,
                    Some("fetch failed"),
                )
                .await
                .expect("download should be marked failed");

            let cleared = crate::app::downloads::clear_failed(&state.db)
                .await
                .expect("failed downloads should clear");

            assert_eq!(cleared.cleared, 1);
            assert!(!backend_fs::path_exists(&archive_path));
            assert!(
                state
                    .db
                    .get_download_by_id(&download_id)
                    .await
                    .expect("download lookup should succeed")
                    .is_none()
            );
            let chapter = state
                .db
                .get_chapter_by_id(&chapter_id)
                .await
                .expect("chapter lookup should succeed")
                .expect("chapter should still exist");
            assert!(!chapter.downloaded);
            let overview = state
                .db
                .get_stats_overview()
                .await
                .expect("stats should load");
            assert_eq!(overview.totals.chapters_downloaded, 0);
            assert_eq!(overview.totals.pages_downloaded, 0);
        });
    }

    async fn test_state() -> Arc<AppState> {
        build_test_app_state("lifecycle", "manga_server_lifecycle_test").await
    }

    async fn seed_queued_download(state: &Arc<AppState>) -> (String, String, PathBuf) {
        let manga_id = state
            .db
            .add_manga_to_library(&MangaInsert {
                source: "test-source",
                source_id: "series-1",
                title: "Test Manga",
                cover_url: "",
                cover_fetch_spec: None,
                description: "",
                author: "",
                genres: "",
                status: "ongoing",
                category: "",
                is_nsfw: false,
                language: None,
            })
            .await
            .expect("manga should be inserted");
        let chapter_id = state
            .db
            .upsert_chapters(
                &manga_id,
                vec![ChapterInsert {
                    source_id: "chapter-1".to_string(),
                    title: "Chapter 1".to_string(),
                    chapter_number: 1.0,
                    date_uploaded: "2026-05-25".to_string(),
                }],
            )
            .await
            .expect("chapter should be inserted")
            .pop()
            .expect("chapter id should be returned");
        let download_id = state
            .db
            .enqueue_download(&chapter_id, &manga_id)
            .await
            .expect("download should be enqueued");
        let download = state
            .db
            .get_download_by_id(&download_id)
            .await
            .expect("download lookup should succeed")
            .expect("download should exist");
        let archive_resolver = downloaded_archive_resolution::path_resolver(&state.db)
            .await
            .expect("archive resolver should resolve");
        let archive_path = archive_resolver.archive_path_for_download(&download);

        (download_id, chapter_id, archive_path)
    }

    async fn write_test_archive(archive_path: &Path, entries: &[(&str, &[u8])]) {
        let page_dir = archive_path
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
        backend_storage::write_originals(
            archive_path.to_path_buf(),
            backend_storage::OriginalChapter {
                pages: pages.into_iter().map(|(_, path)| path).collect(),
                comicinfo_xml: "<ComicInfo/>".into(),
                cover: None,
            },
            || false,
        )
        .await
        .expect("archive should be built");
    }

    fn file_size(path: &PathBuf) -> u64 {
        fs::metadata(path).expect("file metadata should load").len()
    }

    fn valid_png_page_bytes() -> &'static [u8] {
        &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 10, 73, 68, 65, 84, 120, 156, 99, 0, 1, 0, 0,
            5, 0, 1, 13, 10, 45, 180, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ]
    }
}
