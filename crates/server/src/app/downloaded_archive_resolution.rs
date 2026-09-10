use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::{
    api::error::AppError,
    app::{download_work_state, settings},
};
use autometrics::autometrics;
use backend_persistence::{Database, DownloadRow};

#[derive(Clone, Debug)]
pub(crate) struct DownloadedArchivePathResolver {
    download_path: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedDownloadedArchive {
    pub(crate) download: DownloadRow,
    pub(crate) download_path: String,
    pub(crate) archive_path: PathBuf,
}

#[autometrics]
pub(crate) async fn path_resolver(db: &Database) -> anyhow::Result<DownloadedArchivePathResolver> {
    Ok(DownloadedArchivePathResolver {
        download_path: settings::download_path(db).await?,
    })
}

impl DownloadedArchivePathResolver {
    pub(crate) fn download_path(&self) -> &str {
        &self.download_path
    }

    pub(crate) fn archive_path_for_download(&self, download: &DownloadRow) -> PathBuf {
        archive_path_for_download(self.download_path(), download)
    }

    pub(crate) fn existing_archive_path_for_download(
        &self,
        download: &DownloadRow,
    ) -> Option<PathBuf> {
        let archive_path = self.archive_path_for_download(download);
        backend_fs::path_exists(&archive_path).then_some(archive_path)
    }

    pub(crate) fn resolved_archive(&self, download: DownloadRow) -> ResolvedDownloadedArchive {
        ResolvedDownloadedArchive {
            archive_path: self.archive_path_for_download(&download),
            download_path: self.download_path.clone(),
            download,
        }
    }

    pub(crate) fn existing_resolved_archive(
        &self,
        download: DownloadRow,
    ) -> Option<ResolvedDownloadedArchive> {
        let archive_path = self.existing_archive_path_for_download(&download)?;
        Some(ResolvedDownloadedArchive {
            download_path: self.download_path.clone(),
            download,
            archive_path,
        })
    }
}

pub(crate) fn archive_path_for_download(download_path: &str, download: &DownloadRow) -> PathBuf {
    backend_core::download_archive_path(
        download_path,
        &download.manga_source,
        &download.manga_title,
        download.chapter_number,
    )
}

#[autometrics]
pub(crate) async fn require_download(
    db: &Database,
    download_id: &str,
) -> Result<DownloadRow, AppError> {
    db.get_download_by_id(download_id)
        .await?
        .ok_or_else(|| AppError::not_found(anyhow::anyhow!("Download not found")))
}

#[autometrics]
pub(crate) async fn require_completed_archive(
    db: &Database,
    download_id: &str,
) -> Result<ResolvedDownloadedArchive, AppError> {
    let resolver = path_resolver(db).await?;
    let download = require_completed_download(db, download_id).await?;
    Ok(resolver.resolved_archive(download))
}

#[autometrics]
pub(crate) async fn require_existing_completed_archive(
    db: &Database,
    download_id: &str,
) -> Result<ResolvedDownloadedArchive, AppError> {
    let archive = require_completed_archive(db, download_id).await?;
    require_existing_archive_path(&archive.archive_path)?;
    Ok(archive)
}

#[autometrics]
pub(crate) async fn existing_completed_archive_for_chapter(
    db: &Database,
    chapter_id: &str,
) -> Result<ResolvedDownloadedArchive, AppError> {
    let resolver = path_resolver(db).await?;
    let download = db
        .get_completed_download_by_chapter(chapter_id)
        .await?
        .ok_or_else(|| AppError::not_found(anyhow::anyhow!("downloaded chapter not found")))?;
    resolver
        .existing_resolved_archive(download)
        .ok_or_else(download_archive_not_found)
}

#[autometrics]
pub(crate) async fn existing_completed_archives(
    db: &Database,
) -> Result<Vec<ResolvedDownloadedArchive>, AppError> {
    let resolver = path_resolver(db).await?;
    let downloads = db.get_downloads().await?;
    Ok(existing_completed_archives_from_downloads(
        &resolver, downloads,
    ))
}

#[autometrics]
pub(crate) async fn existing_completed_archives_for_manga(
    db: &Database,
    manga_id: &str,
) -> Result<Vec<ResolvedDownloadedArchive>, AppError> {
    let resolver = path_resolver(db).await?;
    let downloads = db.get_downloads_for_manga(manga_id).await?;
    Ok(existing_completed_archives_from_downloads(
        &resolver, downloads,
    ))
}

fn existing_completed_archives_from_downloads(
    resolver: &DownloadedArchivePathResolver,
    downloads: impl IntoIterator<Item = DownloadRow>,
) -> Vec<ResolvedDownloadedArchive> {
    let mut seen_paths = HashSet::new();
    let mut archives = Vec::new();

    for download in downloads {
        if !download_work_state::external_status_is_completed(&download.status) {
            continue;
        }
        let Some(archive) = resolver.existing_resolved_archive(download) else {
            continue;
        };
        if seen_paths.insert(archive.archive_path.clone()) {
            archives.push(archive);
        }
    }

    archives
}

async fn require_completed_download(
    db: &Database,
    download_id: &str,
) -> Result<DownloadRow, AppError> {
    let download = require_download(db, download_id).await?;
    if download_work_state::external_status_is_completed(&download.status) {
        Ok(download)
    } else {
        Err(AppError::bad_request(anyhow::anyhow!(
            "Download is not completed"
        )))
    }
}

pub(crate) fn require_existing_archive_path(archive_path: &Path) -> Result<(), AppError> {
    if backend_fs::path_exists(archive_path) {
        Ok(())
    } else {
        Err(download_archive_not_found())
    }
}

fn download_archive_not_found() -> AppError {
    AppError::not_found(anyhow::anyhow!("download archive not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn download_row(id: &str, status: &str, chapter_number: f64) -> DownloadRow {
        DownloadRow {
            id: id.to_string(),
            chapter_id: format!("chapter-{id}"),
            manga_id: "manga".to_string(),
            status: status.to_string(),
            progress: 0.0,
            error: None,
            chapter_title: "Chapter".to_string(),
            chapter_number,
            manga_title: "Demo".to_string(),
            chapter_source_id: format!("remote-chapter-{id}"),
            manga_source: "source".to_string(),
            manga_source_id: "remote-manga".to_string(),
        }
    }

    #[test]
    fn completed_archive_resolution_dedupes_existing_paths() {
        let temp = tempfile::tempdir().expect("temp dir should create");
        let resolver = DownloadedArchivePathResolver {
            download_path: temp.path().to_string_lossy().to_string(),
        };
        let first = download_row("one", "completed", 1.0);
        let duplicate = download_row("two", "completed", 1.0);
        let queued = download_row("queued", "queued", 2.0);
        let archive_path = resolver.archive_path_for_download(&first);
        std::fs::create_dir_all(
            archive_path
                .parent()
                .expect("archive path should have parent"),
        )
        .expect("archive dir should create");
        std::fs::write(&archive_path, b"archive").expect("archive should write");

        let archives =
            existing_completed_archives_from_downloads(&resolver, [first, duplicate, queued]);

        assert_eq!(archives.len(), 1);
        assert_eq!(archives[0].archive_path, archive_path);
        assert_eq!(archives[0].download.id, "one");
    }
}
