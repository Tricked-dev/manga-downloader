use std::{path::PathBuf, sync::Arc};

use crate::{
    AppState,
    api::error::AppError,
    app::{downloaded_archive_derived_state, downloaded_archive_resolution, settings},
};
use autometrics::autometrics;
use backend_persistence::DownloadRow;
use rayon::{ThreadPoolBuilder, prelude::*};

#[derive(Clone)]
struct ReencodeArchiveJob {
    download_id: String,
    chapter_id: String,
    archive_path: PathBuf,
    avif_quality: u8,
}

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

#[allow(dead_code)]
pub(crate) struct DownloadedArchiveReencoded {
    pub(crate) download_id: String,
    pub(crate) chapter_id: String,
    pub(crate) download_path: String,
    pub(crate) archive_path: PathBuf,
    pub(crate) avif_quality: u8,
    pub(crate) images_reencoded: usize,
}

pub(crate) struct DownloadedArchiveMetadataChanged {
    pub(crate) download_id: String,
    pub(crate) chapter_id: String,
    pub(crate) download_path: String,
    pub(crate) archive_path: PathBuf,
}

pub(crate) struct DownloadedArchiveReencodedItem {
    pub(crate) download_id: String,
    pub(crate) chapter_id: String,
    pub(crate) archive_path: PathBuf,
}

pub(crate) struct DownloadedArchivesReencoded {
    pub(crate) files_processed: usize,
    pub(crate) images_reencoded: usize,
    pub(crate) archives: Vec<DownloadedArchiveReencodedItem>,
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
    downloaded_archive_derived_state::downloaded_archive_created(
        state,
        &created,
        archive_resolver.download_path(),
    )
    .await;

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
    let removed_size_bytes = backend_fs::remove_file_if_present(&archive.archive_path).await?;

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
    downloaded_archive_derived_state::downloaded_archive_deleted(state, &deleted).await;

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
pub(crate) async fn downloaded_archive_reencoded(
    state: &Arc<AppState>,
    download_id: &str,
) -> Result<DownloadedArchiveReencoded, AppError> {
    let archive =
        downloaded_archive_resolution::require_existing_completed_archive(&state.db, download_id)
            .await?;
    let avif_quality = resolve_avif_quality(state, &archive.download).await?;
    let avif_conversion_workers = resolved_avif_conversion_workers(state).await?;
    let images_reencoded = reencode_archive(
        archive.archive_path.clone(),
        avif_quality,
        avif_conversion_workers,
    )
    .await?;

    let reencoded = DownloadedArchiveReencoded {
        download_id: archive.download.id,
        chapter_id: archive.download.chapter_id,
        download_path: archive.download_path,
        archive_path: archive.archive_path,
        avif_quality,
        images_reencoded,
    };
    downloaded_archive_derived_state::downloaded_archive_reencoded(state, &reencoded).await;

    tracing::info!(
        download_id = %reencoded.download_id,
        chapter_id = %reencoded.chapter_id,
        archive_path = %reencoded.archive_path.display(),
        images_reencoded,
        avif_quality,
        "Downloaded Archive Reencoded",
    );

    Ok(reencoded)
}

#[autometrics(track_concurrency)]
pub(crate) async fn downloaded_archive_metadata_changed(
    state: &Arc<AppState>,
    changed: &DownloadedArchiveMetadataChanged,
) {
    downloaded_archive_derived_state::downloaded_archive_metadata_changed(state, changed).await;

    tracing::info!(
        download_id = %changed.download_id,
        chapter_id = %changed.chapter_id,
        archive_path = %changed.archive_path.display(),
        "Downloaded Archive Metadata Changed",
    );
}

#[autometrics(track_concurrency)]
pub(crate) async fn downloaded_archives_reencoded(
    state: &Arc<AppState>,
) -> Result<DownloadedArchivesReencoded, AppError> {
    let archive_resolver = downloaded_archive_resolution::path_resolver(&state.db).await?;
    let download_path = archive_resolver.download_path().to_string();
    let archives = downloaded_archive_resolution::existing_completed_archives(&state.db).await?;
    let mut archive_jobs = Vec::new();

    for archive in archives {
        let avif_quality = resolve_avif_quality(state, &archive.download).await?;
        archive_jobs.push(ReencodeArchiveJob {
            download_id: archive.download.id,
            chapter_id: archive.download.chapter_id,
            archive_path: archive.archive_path,
            avif_quality,
        });
    }

    let avif_conversion_workers = resolved_avif_conversion_workers(state).await?;
    let files_processed = archive_jobs.len();
    let images_reencoded = reencode_archives(archive_jobs.clone(), avif_conversion_workers).await?;

    let result = DownloadedArchivesReencoded {
        files_processed,
        images_reencoded,
        archives: archive_jobs
            .into_iter()
            .map(|job| DownloadedArchiveReencodedItem {
                download_id: job.download_id,
                chapter_id: job.chapter_id,
                archive_path: job.archive_path,
            })
            .collect(),
    };
    downloaded_archive_derived_state::downloaded_archives_bulk_reencoded(
        state,
        &result,
        &download_path,
    )
    .await;

    tracing::info!(
        files_processed,
        images_reencoded,
        "Downloaded Archives Reencoded",
    );

    Ok(result)
}

async fn resolve_avif_quality(
    state: &Arc<AppState>,
    download: &DownloadRow,
) -> Result<u8, AppError> {
    Ok(settings::source_avif_quality(&state.db, &download.manga_source).await?)
}

async fn resolved_avif_conversion_workers(state: &Arc<AppState>) -> Result<usize, AppError> {
    Ok(settings::avif_conversion_workers(&state.db)
        .await?
        .min(backend_image::cpu_worker_budget()))
}

async fn reencode_archive(
    archive_path: PathBuf,
    avif_quality: u8,
    avif_conversion_workers: usize,
) -> Result<usize, AppError> {
    tokio::task::spawn_blocking(move || {
        backend_image::reencode_zstd_folder_to_avif_with_workers(
            &archive_path,
            avif_quality,
            avif_conversion_workers,
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("Re-encode task failed: {error}"))?
    .map_err(AppError::from)
}

async fn reencode_archives(
    archive_jobs: Vec<ReencodeArchiveJob>,
    avif_conversion_workers: usize,
) -> Result<usize, AppError> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<usize> {
        let available_workers = backend_image::cpu_worker_budget();
        if archive_jobs.is_empty() {
            return Ok(0);
        }

        let file_parallelism =
            backend_core::reencode_file_parallelism(archive_jobs.len(), available_workers)
                .min(avif_conversion_workers);
        let per_file_workers = backend_core::reencode_per_file_workers(
            file_parallelism,
            available_workers.min(avif_conversion_workers),
        )
        .min((avif_conversion_workers / file_parallelism.max(1)).max(1));
        let pool = ThreadPoolBuilder::new()
            .num_threads(file_parallelism)
            .build()?;
        let images_reencoded = pool.install(|| -> anyhow::Result<usize> {
            archive_jobs
                .par_iter()
                .map(|job| {
                    let converted = backend_image::reencode_zstd_folder_to_avif_with_workers(
                        &job.archive_path,
                        job.avif_quality,
                        per_file_workers,
                    )?;
                    tracing::info!(
                        download_id = %job.download_id,
                        chapter_id = %job.chapter_id,
                        archive_path = %job.archive_path.display(),
                        images_reencoded = converted,
                        avif_quality = job.avif_quality,
                        "Downloaded Archive Reencoded",
                    );
                    Ok(converted)
                })
                .try_reduce(|| 0usize, |acc, converted| Ok(acc + converted))
        })?;

        Ok(images_reencoded)
    })
    .await
    .map_err(|error| AppError::from(anyhow::anyhow!("Failed to run re-encode task: {error}")))?
    .map_err(AppError::from)
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
            tokio::fs::write(&archive_path, b"not-a-valid-archive")
                .await
                .expect("archive fixture should be written");

            let created =
                downloaded_archive_created(&state, &download_id, 3, archive_path.clone(), 19)
                    .await
                    .expect("creation should tolerate repairable index rebuild failure");
            assert_eq!(created.download_id, download_id);
            assert_eq!(created.chapter_id, chapter_id);
            assert_eq!(created.archive_path, archive_path);
            assert_eq!(created.page_count, 3);
            assert_eq!(created.archive_size_bytes, 19);

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
            assert_eq!(deleted.removed_size_bytes, 19);
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
    fn reencoding_does_not_record_another_downloaded_chapter_fact() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("test runtime should initialize");

        runtime.block_on(async {
            let state = test_state().await;
            let (download_id, chapter_id, archive_path) = seed_queued_download(&state).await;
            write_test_archive(&archive_path, &[("001.png", valid_png_page_bytes())]);

            downloaded_archive_created(
                &state,
                &download_id,
                1,
                archive_path.clone(),
                file_size(&archive_path),
            )
            .await
            .expect("downloaded archive should be created");
            let before = state
                .db
                .get_stats_overview()
                .await
                .expect("stats should load");

            let reencoded = downloaded_archive_reencoded(&state, &download_id)
                .await
                .expect("downloaded archive should reencode");
            assert_eq!(reencoded.download_id, download_id);
            assert_eq!(reencoded.chapter_id, chapter_id);
            assert_eq!(reencoded.images_reencoded, 1);

            let after = state
                .db
                .get_stats_overview()
                .await
                .expect("stats should load");
            assert_eq!(
                after.totals.chapters_downloaded,
                before.totals.chapters_downloaded
            );
            assert_eq!(
                after.totals.pages_downloaded,
                before.totals.pages_downloaded
            );
        });
    }

    #[test]
    fn metadata_change_rebuilds_archive_state_without_new_downloaded_fact() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("test runtime should initialize");

        runtime.block_on(async {
            let state = test_state().await;
            let (download_id, chapter_id, archive_path) = seed_queued_download(&state).await;
            write_test_archive(&archive_path, &[("001.png", valid_png_page_bytes())]);

            downloaded_archive_created(
                &state,
                &download_id,
                1,
                archive_path.clone(),
                file_size(&archive_path),
            )
            .await
            .expect("downloaded archive should be created");
            let before = state
                .db
                .get_stats_overview()
                .await
                .expect("stats should load");

            state.archive_index.invalidate_chapter(&chapter_id);
            assert!(
                state
                    .archive_index
                    .cached_chapter_pages(&chapter_id)
                    .is_none()
            );

            let download_path = settings::download_path(&state.db)
                .await
                .expect("download path should resolve");
            downloaded_archive_metadata_changed(
                &state,
                &DownloadedArchiveMetadataChanged {
                    download_id: download_id.clone(),
                    chapter_id: chapter_id.clone(),
                    download_path,
                    archive_path,
                },
            )
            .await;

            assert!(
                state
                    .archive_index
                    .cached_chapter_pages(&chapter_id)
                    .is_some()
            );
            let after = state
                .db
                .get_stats_overview()
                .await
                .expect("stats should load");
            assert_eq!(
                after.totals.chapters_downloaded,
                before.totals.chapters_downloaded
            );
            assert_eq!(
                after.totals.pages_downloaded,
                before.totals.pages_downloaded
            );
        });
    }

    #[test]
    fn bulk_reencoding_refreshes_archive_index_state_per_archive() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("test runtime should initialize");

        runtime.block_on(async {
            let state = test_state().await;
            let (download_id, chapter_id, archive_path) = seed_queued_download(&state).await;
            write_test_archive(&archive_path, &[("001.png", valid_png_page_bytes())]);

            downloaded_archive_created(
                &state,
                &download_id,
                1,
                archive_path.clone(),
                file_size(&archive_path),
            )
            .await
            .expect("downloaded archive should be created");
            let before = state
                .archive_index
                .cached_chapter_pages(&chapter_id)
                .expect("created archive should cache page facts");
            assert_eq!(before[0].content_type, "image/png");

            let result = downloaded_archives_reencoded(&state)
                .await
                .expect("bulk reencode should succeed");
            assert_eq!(result.files_processed, 1);
            assert_eq!(result.images_reencoded, 1);

            let after = state
                .archive_index
                .cached_chapter_pages(&chapter_id)
                .expect("bulk reencode should rebuild page facts");
            assert_eq!(after[0].content_type, "image/avif");
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

    fn write_test_archive(archive_path: &Path, entries: &[(&str, &[u8])]) {
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
        backend_image::build_zstd_folder_from_paths(&pages, archive_path, None, None)
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
