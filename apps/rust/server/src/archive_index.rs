use crate::{
    AppState,
    api::{
        dto::{ArchiveIndexCleanupResponse, ArchiveIndexJobResponse, ArchiveIndexStatusResponse},
        error::AppError,
    },
    app::downloaded_archive_resolution,
};
use anyhow::{Context, Result};
use backend_page_extraction::ARCHIVE_INDEX_SCHEMA_VERSION;
use backend_persistence::{ArchiveIndexIdentity, ArchiveIndexRecord, Database};
use backend_telemetry::Metrics;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const ARCHIVE_INDEX_STATUS_TTL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DownloadedArchiveIndex {
    pages: Vec<DownloadedArchivePage>,
    cover_entry: Option<String>,
    comic_info: backend_image::ComicInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadedArchivePage {
    name: String,
    content_type: DownloadedArchiveContentType,
    file_position: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum DownloadedArchiveContentType {
    Avif,
    Gif,
    Jpeg,
    Png,
    Webp,
    Other,
}

impl DownloadedArchiveContentType {
    fn from_entry_name(name: &str) -> Self {
        match backend_image::content_type_for(name) {
            "image/avif" => Self::Avif,
            "image/gif" => Self::Gif,
            "image/jpeg" => Self::Jpeg,
            "image/png" => Self::Png,
            "image/webp" => Self::Webp,
            _ => Self::Other,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Avif => "image/avif",
            Self::Gif => "image/gif",
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Other => "application/octet-stream",
        }
    }
}

#[derive(Debug)]
pub(crate) struct ResolvedArchiveIndex {
    pub(crate) archive_path: PathBuf,
    pub(crate) identity: ArchiveIndexIdentity,
    index: DownloadedArchiveIndex,
    pages: Vec<ResolvedArchivePage>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedArchivePage {
    pub(crate) content_type: &'static str,
    pub(crate) cache_key: String,
    pub(crate) file_position: usize,
}

impl ResolvedArchiveIndex {
    fn new(
        archive_path: PathBuf,
        identity: ArchiveIndexIdentity,
        index: DownloadedArchiveIndex,
    ) -> Self {
        let identity_prefix = format!(
            "{}:{}:{}:{}:",
            identity.archive_path,
            identity.schema_version,
            identity.archive_mtime_ms,
            identity.archive_size,
        );

        let cache_key_prefix = format!("library:chapter-page:{identity_prefix}");
        let pages = index
            .pages
            .iter()
            .enumerate()
            .map(|(page, entry)| {
                let mut cache_key = String::with_capacity(cache_key_prefix.len() + 20);
                cache_key.push_str(&cache_key_prefix);
                let _ = write!(cache_key, "{page}");
                ResolvedArchivePage {
                    content_type: entry.content_type.as_str(),
                    cache_key,
                    file_position: entry.file_position,
                }
            })
            .collect();

        Self {
            archive_path,
            identity,
            index,
            pages,
        }
    }

    pub(crate) fn page_count(&self) -> usize {
        self.index.pages.len()
    }

    pub(crate) fn page(&self, page: usize) -> Option<&ResolvedArchivePage> {
        self.pages.get(page)
    }

    pub(crate) fn extraction_key(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.identity.archive_path,
            self.identity.schema_version,
            self.identity.archive_mtime_ms,
            self.identity.archive_size,
        )
    }

    pub(crate) fn identity_is_current(&self) -> bool {
        identity_for_path(&self.archive_path)
            .is_ok_and(|identity| identity_matches(&identity, &self.identity))
    }
}

pub(crate) struct ArchiveIndexService {
    memory: DashMap<String, Arc<ResolvedArchiveIndex>>,
    chapter_memory: DashMap<String, Arc<ResolvedArchiveIndex>>,
    chapter_pages: DashMap<String, Arc<Vec<ResolvedArchivePage>>>,
    build_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    job: Mutex<ArchiveIndexJobResponse>,
    status_cache: Mutex<Option<CachedArchiveIndexStatus>>,
}

pub(crate) struct ArchiveIndexRebuildStart {
    pub(crate) response: ArchiveIndexJobResponse,
    pub(crate) accepted: bool,
}

pub(crate) struct ArchiveIndexStaleCleanupResult {
    pub(crate) response: ArchiveIndexCleanupResponse,
    pub(crate) affected_archive_paths: Vec<String>,
}

struct CachedArchiveIndexStatus {
    refreshed_at: Instant,
    status: ArchiveIndexStatusResponse,
}

impl ArchiveIndexService {
    pub(crate) fn new() -> Self {
        Self {
            memory: DashMap::new(),
            chapter_memory: DashMap::new(),
            chapter_pages: DashMap::new(),
            build_locks: Mutex::new(HashMap::new()),
            job: Mutex::new(ArchiveIndexJobResponse::default()),
            status_cache: Mutex::new(None),
        }
    }

    pub(crate) async fn get_for_chapter(
        &self,
        db: &Database,
        metrics: &Metrics,
        chapter_id: &str,
        trigger: &str,
    ) -> Result<Arc<ResolvedArchiveIndex>, AppError> {
        if let Some(index) = self
            .chapter_memory
            .get(chapter_id)
            .map(|entry| Arc::clone(&entry))
        {
            metrics.record_archive_index_request("memory");
            return Ok(index);
        }

        let archive_path = archive_path_for_chapter(db, chapter_id).await?;
        let index = self
            .get_or_build(db, metrics, archive_path, trigger)
            .await?;
        self.remember_chapter_index(chapter_id, &index);
        Ok(index)
    }

    pub(crate) fn cached_chapter_pages(
        &self,
        chapter_id: &str,
    ) -> Option<Arc<Vec<ResolvedArchivePage>>> {
        self.chapter_pages
            .get(chapter_id)
            .map(|pages| Arc::clone(&pages))
    }

    pub(crate) async fn get_or_build(
        &self,
        db: &Database,
        metrics: &Metrics,
        archive_path: PathBuf,
        trigger: &str,
    ) -> Result<Arc<ResolvedArchiveIndex>> {
        let identity = identity_for_path(&archive_path)?;
        if let Some(index) = self.memory_hit(&identity) {
            metrics.record_archive_index_request("memory");
            return Ok(index);
        }

        let build_lock = self.build_lock(identity.archive_path.as_str()).await;
        let _guard = build_lock.lock().await;

        if let Some(index) = self.memory_hit(&identity) {
            metrics.record_archive_index_request("memory");
            return Ok(index);
        }

        match db.get_fresh_archive_index(&identity).await? {
            Some(record) => match decode_record(&archive_path, record) {
                Ok(index) => {
                    let index = Arc::new(index);
                    self.memory
                        .insert(identity.archive_path.clone(), Arc::clone(&index));
                    metrics.record_archive_index_request("sqlite");
                    return Ok(index);
                }
                Err(error) => {
                    metrics.record_archive_index_request("error");
                    tracing::warn!(
                        archive_path = %archive_path.display(),
                        error = %error,
                        "Downloaded Archive Index Decode Failed",
                    );
                }
            },
            None => {
                metrics.record_archive_index_request("stale");
            }
        }

        let index = self
            .build_persist_and_store(db, metrics, archive_path, identity, trigger)
            .await?;
        metrics.record_archive_index_request("rebuilt");
        Ok(index)
    }

    pub(crate) async fn index_archive_after_download(
        &self,
        db: &Database,
        metrics: &Metrics,
        chapter_id: &str,
        archive_path: PathBuf,
    ) -> Result<()> {
        let index = self
            .rebuild_archive(db, metrics, archive_path, "download")
            .await?;
        self.remember_chapter_index(chapter_id, &index);
        Ok(())
    }

    async fn rebuild_archive(
        &self,
        db: &Database,
        metrics: &Metrics,
        archive_path: PathBuf,
        trigger: &str,
    ) -> Result<Arc<ResolvedArchiveIndex>> {
        let identity = identity_for_path(&archive_path)?;
        let build_lock = self.build_lock(identity.archive_path.as_str()).await;
        let _guard = build_lock.lock().await;
        self.build_persist_and_store(db, metrics, archive_path, identity, trigger)
            .await
    }

    pub(crate) async fn status(
        &self,
        db: &Database,
        metrics: &Metrics,
    ) -> Result<ArchiveIndexStatusResponse> {
        let memory_entries = self.memory.len();
        let job = self.job.lock().await.clone();
        let mut cache = self.status_cache.lock().await;
        if let Some(cached) = cache.as_ref()
            && cached.refreshed_at.elapsed() <= ARCHIVE_INDEX_STATUS_TTL
        {
            let mut status = cached.status.clone();
            status.memory_entries = memory_entries;
            status.job = job;
            return Ok(status);
        }

        let stats = db.archive_index_stats().await?;
        let stale_rows = stale_archive_rows(db).await?.len();
        metrics.set_archive_index_stats(
            stats.indexed_archives,
            stats.indexed_pages,
            stats.index_blob_bytes,
        );

        let response = ArchiveIndexStatusResponse {
            indexed_archives: stats.indexed_archives,
            indexed_pages: stats.indexed_pages,
            index_blob_bytes: stats.index_blob_bytes,
            stale_rows,
            memory_entries,
            job,
        };
        *cache = Some(CachedArchiveIndexStatus {
            refreshed_at: Instant::now(),
            status: response.clone(),
        });
        Ok(response)
    }

    pub(crate) async fn begin_rebuild(&self, trigger: &str) -> ArchiveIndexRebuildStart {
        let mut job = self.job.lock().await;
        if job.running {
            return ArchiveIndexRebuildStart {
                response: job.clone(),
                accepted: false,
            };
        }
        *job = ArchiveIndexJobResponse {
            running: true,
            trigger: trigger.to_string(),
            indexed_archives: 0,
            skipped_archives: 0,
            failed_archives: 0,
            removed_rows: 0,
            message: "Archive index rebuild running".to_string(),
        };

        ArchiveIndexRebuildStart {
            response: job.clone(),
            accepted: true,
        }
    }

    pub(crate) async fn mark_rebuild_running_if_idle(&self, trigger: &str) {
        let mut job = self.job.lock().await;
        if job.running {
            return;
        }

        *job = ArchiveIndexJobResponse {
            running: true,
            trigger: trigger.to_string(),
            indexed_archives: 0,
            skipped_archives: 0,
            failed_archives: 0,
            removed_rows: 0,
            message: "Archive index rebuild running".to_string(),
        };
    }

    pub(crate) async fn finish_rebuild(
        &self,
        result: Result<ArchiveIndexJobResponse>,
    ) -> ArchiveIndexJobResponse {
        let mut job = self.job.lock().await;
        match result {
            Ok(summary) => *job = summary,
            Err(error) => {
                tracing::warn!(error = %error, "Archive Index Rebuild Failed");
                job.running = false;
                job.failed_archives += 1;
                job.message = format!("Archive index rebuild failed: {error}");
            }
        }
        job.clone()
    }

    pub(crate) async fn rebuild_archive_for_maintenance(
        &self,
        db: &Database,
        metrics: &Metrics,
        archive_path: PathBuf,
        trigger: &str,
    ) -> Result<()> {
        if trigger == "manual" {
            self.rebuild_archive(db, metrics, archive_path, trigger)
                .await
                .map(|_| ())
        } else {
            self.get_or_build(db, metrics, archive_path, trigger)
                .await
                .map(|_| ())
        }
    }

    pub(crate) async fn cleanup_stale_with_paths(
        &self,
        db: &Database,
        metrics: &Metrics,
    ) -> Result<ArchiveIndexStaleCleanupResult> {
        let stale_paths = self.stale_archive_paths(db).await?;
        let removed = db.delete_archive_indexes_by_path(&stale_paths).await?;
        if removed.removed_rows > 0 {
            let stale = stale_paths.iter().cloned().collect::<HashSet<_>>();
            self.memory
                .retain(|archive_path, _| !stale.contains(archive_path));
            self.chapter_memory
                .retain(|_, index| !stale.contains(index.identity.archive_path.as_str()));
            self.chapter_pages
                .retain(|chapter_id, _| self.chapter_memory.contains_key(chapter_id.as_str()));
        }
        self.status_cache.lock().await.take();
        metrics.record_archive_index_cleanup_rows("stale", removed.removed_rows);
        publish_stats(db, metrics).await;
        Ok(ArchiveIndexStaleCleanupResult {
            response: ArchiveIndexCleanupResponse {
                ok: true,
                removed_rows: removed.removed_rows,
            },
            affected_archive_paths: stale_paths,
        })
    }

    pub(crate) async fn clear(
        &self,
        db: &Database,
        metrics: &Metrics,
    ) -> Result<ArchiveIndexCleanupResponse> {
        let removed = db.clear_archive_indexes().await?;
        self.memory.clear();
        self.chapter_memory.clear();
        self.chapter_pages.clear();
        self.status_cache.lock().await.take();
        metrics.record_archive_index_cleanup_rows("manual_clear", removed.removed_rows);
        publish_stats(db, metrics).await;
        Ok(ArchiveIndexCleanupResponse {
            ok: true,
            removed_rows: removed.removed_rows,
        })
    }

    fn memory_hit(&self, identity: &ArchiveIndexIdentity) -> Option<Arc<ResolvedArchiveIndex>> {
        self.memory
            .get(identity.archive_path.as_str())
            .filter(|index| identity_matches(&index.identity, identity))
            .map(|index| Arc::clone(&index))
    }

    fn remember_chapter_index(&self, chapter_id: &str, index: &Arc<ResolvedArchiveIndex>) {
        self.chapter_memory
            .insert(chapter_id.to_string(), Arc::clone(index));
        self.chapter_pages
            .insert(chapter_id.to_string(), Arc::new(index.pages.clone()));
    }

    async fn build_lock(&self, archive_path: &str) -> Arc<Mutex<()>> {
        let mut locks = self.build_locks.lock().await;
        Arc::clone(
            locks
                .entry(archive_path.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    pub(crate) fn invalidate_chapter(&self, chapter_id: &str) {
        self.chapter_memory.remove(chapter_id);
        self.chapter_pages.remove(chapter_id);
    }

    pub(crate) fn invalidate_archive_path(&self, archive_path: &Path) {
        let archive_path = archive_path.to_string_lossy().to_string();
        self.memory.remove(&archive_path);
        self.chapter_memory
            .retain(|_, index| index.identity.archive_path != archive_path);
        self.chapter_pages
            .retain(|chapter_id, _| self.chapter_memory.contains_key(chapter_id.as_str()));
    }

    pub(crate) async fn stale_archive_paths(&self, db: &Database) -> Result<Vec<String>> {
        stale_archive_rows(db).await
    }

    async fn build_persist_and_store(
        &self,
        db: &Database,
        metrics: &Metrics,
        archive_path: PathBuf,
        identity: ArchiveIndexIdentity,
        trigger: &str,
    ) -> Result<Arc<ResolvedArchiveIndex>> {
        let started = Instant::now();
        match build_index_from_path(archive_path.clone(), identity.clone()).await {
            Ok((index, encoded)) => {
                db.upsert_archive_index(&identity, index.page_count(), &encoded)
                    .await?;
                let index = Arc::new(index);
                self.memory
                    .insert(identity.archive_path.clone(), Arc::clone(&index));
                metrics.record_archive_index_build(trigger, "success", started.elapsed());
                self.status_cache.lock().await.take();
                publish_stats(db, metrics).await;
                Ok(index)
            }
            Err(error) => {
                metrics.record_archive_index_build(trigger, "error", started.elapsed());
                Err(error)
            }
        }
    }
}

pub(crate) fn spawn_startup_warm(state: Arc<AppState>) {
    tokio::spawn(async move {
        if let Err(error) =
            crate::app::archive_index_maintenance::start_startup_rebuild(&state).await
        {
            tracing::warn!(error = %error, "Archive Index Startup Warm Failed");
        }
    });
}

pub(crate) async fn archive_path_for_chapter(
    db: &Database,
    chapter_id: &str,
) -> Result<PathBuf, AppError> {
    Ok(
        downloaded_archive_resolution::existing_completed_archive_for_chapter(db, chapter_id)
            .await?
            .archive_path,
    )
}

async fn build_index_from_path(
    archive_path: PathBuf,
    identity: ArchiveIndexIdentity,
) -> Result<(ResolvedArchiveIndex, Vec<u8>)> {
    let index = tokio::task::spawn_blocking({
        let archive_path = archive_path.clone();
        move || -> Result<DownloadedArchiveIndex> {
            let info = backend_image::inspect_archive(&archive_path)?;
            Ok(DownloadedArchiveIndex {
                pages: info
                    .pages
                    .into_iter()
                    .map(|entry| DownloadedArchivePage {
                        content_type: DownloadedArchiveContentType::from_entry_name(&entry.name),
                        name: entry.name,
                        file_position: entry.file_position,
                    })
                    .collect(),
                cover_entry: info.cover_entry,
                comic_info: info.comic_info,
            })
        }
    })
    .await
    .context("archive index build task failed")??;
    let encoded = postcard::to_allocvec(&index).context("failed to serialize archive index")?;

    Ok((
        ResolvedArchiveIndex::new(archive_path, identity, index),
        encoded,
    ))
}

fn decode_record(archive_path: &Path, record: ArchiveIndexRecord) -> Result<ResolvedArchiveIndex> {
    let index: DownloadedArchiveIndex =
        postcard::from_bytes(&record.index_postcard).context("invalid postcard archive index")?;
    Ok(ResolvedArchiveIndex::new(
        archive_path.to_path_buf(),
        record.identity,
        index,
    ))
}

fn identity_for_path(archive_path: &Path) -> Result<ArchiveIndexIdentity> {
    let metadata = backend_fs::metadata_sync(archive_path)
        .with_context(|| format!("failed to read metadata for {}", archive_path.display()))?;
    let archive_mtime_ms = metadata
        .modified()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or_default();

    Ok(ArchiveIndexIdentity {
        archive_path: archive_path.to_string_lossy().to_string(),
        archive_size: i64::try_from(metadata.len()).unwrap_or(i64::MAX),
        archive_mtime_ms,
        schema_version: ARCHIVE_INDEX_SCHEMA_VERSION,
    })
}

fn identity_matches(left: &ArchiveIndexIdentity, right: &ArchiveIndexIdentity) -> bool {
    left.archive_path == right.archive_path
        && left.archive_size == right.archive_size
        && left.archive_mtime_ms == right.archive_mtime_ms
        && left.schema_version == right.schema_version
}

async fn stale_archive_rows(db: &Database) -> Result<Vec<String>> {
    let records = db.list_archive_index_records().await?;
    let mut stale_paths = Vec::new();
    for record in records {
        if is_stale_record(&record) {
            stale_paths.push(record.identity.archive_path);
        }
    }
    Ok(stale_paths)
}

fn is_stale_record(record: &ArchiveIndexRecord) -> bool {
    if record.identity.schema_version != ARCHIVE_INDEX_SCHEMA_VERSION {
        return true;
    }
    let archive_path = Path::new(&record.identity.archive_path);
    let Ok(identity) = identity_for_path(archive_path) else {
        return true;
    };
    !identity_matches(&record.identity, &identity)
}

pub(crate) async fn publish_stats(db: &Database, metrics: &Metrics) {
    match db.archive_index_stats().await {
        Ok(stats) => metrics.set_archive_index_stats(
            stats.indexed_archives,
            stats.indexed_pages,
            stats.index_blob_bytes,
        ),
        Err(error) => {
            tracing::warn!(error = %error, "Archive Index Stats Metric Update Failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_archive_identity_becomes_stale_when_archive_file_changes() {
        let root = std::env::temp_dir().join(format!(
            "manga-server-archive-index-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("test dir should be created");
        let archive_path = root.join("chapter.cbz");
        std::fs::write(&archive_path, b"before").expect("test archive should be written");

        let identity = identity_for_path(&archive_path).expect("identity should resolve");
        let index = ResolvedArchiveIndex::new(
            archive_path.clone(),
            identity,
            DownloadedArchiveIndex {
                pages: Vec::new(),
                cover_entry: None,
                comic_info: backend_image::ComicInfo::default(),
            },
        );
        assert!(index.identity_is_current());

        std::fs::write(&archive_path, b"after-size-change")
            .expect("test archive should be changed");

        assert!(!index.identity_is_current());
    }
}
