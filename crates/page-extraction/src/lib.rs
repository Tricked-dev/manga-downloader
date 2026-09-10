use anyhow::{Context, Result};
use backend_telemetry::Metrics;
use std::collections::HashMap;
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, mpsc, oneshot};

pub const ARCHIVE_INDEX_SCHEMA_VERSION: i64 = 2;

const DEFAULT_MAX_CONCURRENT_DOWNLOADED_PAGE_EXTRACTS: usize = 8;
const MAX_MAX_CONCURRENT_DOWNLOADED_PAGE_EXTRACTS: usize = 16;
const MIN_MAX_CONCURRENT_DOWNLOADED_PAGE_EXTRACTS: usize = 4;
const ARCHIVE_EXTRACT_QUEUE_LIMIT: usize = 2048;
const ARCHIVE_EXTRACT_QUEUE_PRESSURE_DEPTH: usize = 128;
const ARCHIVE_EXTRACT_DIRECT_QUEUE_BYPASS_DEPTH: usize = 4;
const ARCHIVE_EXTRACT_BATCH_LIMIT: usize = 512;
const ARCHIVE_EXTRACT_BATCH_DELAY: Duration = Duration::from_micros(75);
const ARCHIVE_EXTRACT_PRESSURE_BATCH_DELAY: Duration = Duration::from_micros(250);

pub struct DownloadedPageExtractionConfig {
    pub max_concurrent_extracts: usize,
    pub queue_limit: usize,
    pub queue_pressure_depth: usize,
    pub direct_queue_bypass_depth: usize,
    pub batch_limit: usize,
    pub batch_delay: Duration,
    pub pressure_batch_delay: Duration,
}

impl Default for DownloadedPageExtractionConfig {
    fn default() -> Self {
        Self {
            max_concurrent_extracts: downloaded_page_extract_concurrency(),
            queue_limit: ARCHIVE_EXTRACT_QUEUE_LIMIT,
            queue_pressure_depth: ARCHIVE_EXTRACT_QUEUE_PRESSURE_DEPTH,
            direct_queue_bypass_depth: ARCHIVE_EXTRACT_DIRECT_QUEUE_BYPASS_DEPTH,
            batch_limit: ARCHIVE_EXTRACT_BATCH_LIMIT,
            batch_delay: ARCHIVE_EXTRACT_BATCH_DELAY,
            pressure_batch_delay: ARCHIVE_EXTRACT_PRESSURE_BATCH_DELAY,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PositionedImageTarget {
    pub file_position: usize,
    pub content_type: &'static str,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExtractionRequestMode {
    Foreground,
    ReadAhead,
}

pub enum PositionedExtractionResult {
    Completed(Vec<backend_image::EntryBytes>),
    DroppedQueuePressure,
    StaleArchive,
}

#[derive(Default)]
struct DownloadedPageAccessState {
    last_page: Option<usize>,
    sequential_hits: usize,
}

pub struct DownloadedPageExtractionScheduler {
    config: DownloadedPageExtractionConfig,
    extraction_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    extraction_queue: mpsc::Sender<ArchiveExtractRequest>,
    extraction_limiter: Arc<Semaphore>,
    page_access: Mutex<HashMap<String, DownloadedPageAccessState>>,
}

impl Default for DownloadedPageExtractionScheduler {
    fn default() -> Self {
        Self::new(DownloadedPageExtractionConfig::default())
    }
}

struct ArchiveExtractRequest {
    archive_key: String,
    archive_path: PathBuf,
    targets: Vec<PositionedImageTarget>,
    mode: ExtractionRequestMode,
    queued_at: Instant,
    metrics: Metrics,
    reply: oneshot::Sender<Result<PositionedExtractionResult>>,
}

impl DownloadedPageExtractionScheduler {
    /// Creates a scheduler and starts its background extraction coordinator.
    pub fn new(config: DownloadedPageExtractionConfig) -> Self {
        let (extraction_queue, extraction_receiver) = mpsc::channel(config.queue_limit);
        let extraction_limiter = Arc::new(Semaphore::new(config.max_concurrent_extracts));
        tokio::spawn(archive_extract_coordinator(
            Arc::clone(&extraction_limiter),
            extraction_receiver,
            config.queue_pressure_depth,
            config.batch_limit,
            config.batch_delay,
            config.pressure_batch_delay,
        ));

        Self {
            config,
            extraction_locks: Mutex::new(HashMap::new()),
            extraction_queue,
            extraction_limiter,
            page_access: Mutex::new(HashMap::new()),
        }
    }

    /// Returns a shared lock for a page extraction window.
    ///
    /// Callers can use the returned key with
    /// [`release_extraction_window_lock`](Self::release_extraction_window_lock).
    pub async fn extraction_window_lock(
        &self,
        archive_key: &str,
        page: usize,
        window_size: usize,
    ) -> (String, Arc<Mutex<()>>) {
        let key = downloaded_page_window_key(archive_key, page, window_size);
        let mut locks = self.extraction_locks.lock().await;
        let lock = Arc::clone(
            locks
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        );
        (key, lock)
    }

    /// Releases and removes a previously acquired extraction window lock when it is no longer shared.
    pub async fn release_extraction_window_lock(&self, key: &str, lock: &Arc<Mutex<()>>) {
        let mut locks = self.extraction_locks.lock().await;
        let should_remove = locks
            .get(key)
            .is_some_and(|current| Arc::ptr_eq(current, lock) && Arc::strong_count(lock) == 2);
        if should_remove {
            locks.remove(key);
        }
    }

    /// Extracts archive entries by file position through the scheduler.
    ///
    /// Foreground single-page reads may bypass the queue when capacity is
    /// available. Read-ahead requests can be dropped under queue pressure.
    pub async fn extract_positioned_images(
        &self,
        metrics: &Metrics,
        archive_key: String,
        archive_path: PathBuf,
        targets: Vec<PositionedImageTarget>,
        mode: ExtractionRequestMode,
    ) -> Result<PositionedExtractionResult> {
        let queue_depth = self
            .config
            .queue_limit
            .saturating_sub(self.extraction_queue.capacity());
        metrics.set_downloaded_page_extract_queue_depth(queue_depth);
        if mode == ExtractionRequestMode::ReadAhead
            && queue_depth >= self.config.queue_pressure_depth
        {
            metrics.record_downloaded_page_extract_backpressure("queue_pressure");
            return Ok(PositionedExtractionResult::DroppedQueuePressure);
        }

        if mode == ExtractionRequestMode::Foreground
            && targets.len() == 1
            && queue_depth <= self.config.direct_queue_bypass_depth
            && let Some(permit) = self.try_extraction_permit()
        {
            return self
                .extract_positioned_images_with_permit(
                    metrics,
                    &archive_key,
                    archive_path,
                    targets,
                    Instant::now(),
                    permit,
                )
                .await;
        }

        if queue_depth >= self.config.queue_pressure_depth {
            metrics.record_downloaded_page_extract_backpressure("queue_pressure");
        }
        if self.extraction_limiter.available_permits() == 0 {
            metrics.record_downloaded_page_extract_backpressure("extract_concurrency");
        }
        let (reply, response) = oneshot::channel();
        let request = ArchiveExtractRequest {
            archive_key,
            archive_path,
            targets,
            mode,
            queued_at: Instant::now(),
            metrics: *metrics,
            reply,
        };

        if let Err(error) = self.extraction_queue.send(request).await {
            let request = error.0;
            return self
                .extract_positioned_images_direct(
                    &request.metrics,
                    &request.archive_key,
                    request.archive_path,
                    request.targets,
                )
                .await;
        }

        response
            .await
            .context("archive extraction worker stopped before replying")?
    }

    /// Removes access-state and window locks associated with an archive key.
    pub async fn invalidate_archive(&self, archive_key: &str) {
        let archive_prefix = format!("{archive_key}:");
        self.page_access.lock().await.retain(|state_key, _| {
            state_key != archive_key && !state_key.starts_with(&archive_prefix)
        });
        self.extraction_locks.lock().await.retain(|lock_key, _| {
            lock_key != archive_key && !lock_key.starts_with(&archive_prefix)
        });
    }

    /// Clears all scheduler access-state and window-lock bookkeeping.
    pub async fn clear(&self) {
        self.extraction_locks.lock().await.clear();
        self.page_access.lock().await.clear();
    }

    /// Chooses the foreground read window for a page access.
    ///
    /// Sequential reads can expand to `max_window_size` while the queue is below
    /// pressure depth; random reads fall back to a single page.
    pub async fn foreground_read_window_size(
        &self,
        archive_key: &str,
        page: usize,
        max_window_size: usize,
    ) -> usize {
        let mut states = self.page_access.lock().await;
        let state = states.entry(archive_key.to_string()).or_default();

        let is_next_page = state
            .last_page
            .is_some_and(|last_page| page == last_page.saturating_add(1));
        state.sequential_hits = if is_next_page {
            state.sequential_hits.saturating_add(1)
        } else {
            0
        };
        state.last_page = Some(page);

        let queue_depth = self
            .config
            .queue_limit
            .saturating_sub(self.extraction_queue.capacity());
        if state.sequential_hits > 0 && queue_depth < self.config.queue_pressure_depth {
            max_window_size
        } else {
            1
        }
    }

    async fn extract_positioned_images_direct(
        &self,
        metrics: &Metrics,
        archive_key: &str,
        archive_path: PathBuf,
        targets: Vec<PositionedImageTarget>,
    ) -> Result<PositionedExtractionResult> {
        let permit_started = Instant::now();
        let permit = self.extraction_permit().await?;
        self.extract_positioned_images_with_permit(
            metrics,
            archive_key,
            archive_path,
            targets,
            permit_started,
            permit,
        )
        .await
    }

    async fn extract_positioned_images_with_permit(
        &self,
        metrics: &Metrics,
        archive_key: &str,
        archive_path: PathBuf,
        targets: Vec<PositionedImageTarget>,
        queued_at: Instant,
        _permit: OwnedSemaphorePermit,
    ) -> Result<PositionedExtractionResult> {
        metrics.record_downloaded_page_extract_queue_wait("direct", queued_at.elapsed());
        if !archive_key_is_current(&archive_path, archive_key) {
            return Ok(PositionedExtractionResult::StaleArchive);
        }
        let worker_started = Instant::now();
        let pages = targets.len();
        let result = run_positioned_extract(archive_path, targets).await;
        metrics.record_downloaded_page_extract_worker(
            if result.is_ok() { "success" } else { "error" },
            worker_started.elapsed(),
            1,
            pages,
        );
        result.map(PositionedExtractionResult::Completed)
    }

    async fn extraction_permit(&self) -> Result<OwnedSemaphorePermit> {
        Arc::clone(&self.extraction_limiter)
            .acquire_owned()
            .await
            .context("downloaded page extraction limiter closed")
    }

    fn try_extraction_permit(&self) -> Option<OwnedSemaphorePermit> {
        Arc::clone(&self.extraction_limiter)
            .try_acquire_owned()
            .ok()
    }
}

async fn archive_extract_coordinator(
    limiter: Arc<Semaphore>,
    mut receiver: mpsc::Receiver<ArchiveExtractRequest>,
    queue_pressure_depth: usize,
    batch_limit: usize,
    batch_delay: Duration,
    pressure_batch_delay: Duration,
) {
    while let Some(first) = receiver.recv().await {
        let batch = collect_archive_extract_batch(
            first,
            &mut receiver,
            queue_pressure_depth,
            batch_limit,
            batch_delay,
            pressure_batch_delay,
        )
        .await;
        let metrics = batch.first().map(|request| request.metrics);
        let mut grouped = group_archive_extract_requests(batch);
        if let Some(metrics) = metrics {
            metrics.record_downloaded_page_extract_batch(
                grouped.iter().map(Vec::len).sum(),
                grouped.len(),
            );
            metrics.set_downloaded_page_extract_queue_depth(receiver.len());
        }

        sort_archive_extract_groups(&mut grouped);

        for group in grouped {
            let limiter = Arc::clone(&limiter);
            if limiter.available_permits() == 0
                && let Some(first) = group.first()
            {
                first
                    .metrics
                    .record_downloaded_page_extract_backpressure("extract_concurrency");
            }
            match limiter.acquire_owned().await {
                Ok(permit) => {
                    tokio::spawn(run_archive_extract_group(permit, group));
                }
                Err(error) => reply_archive_extract_error(
                    group,
                    &format!("downloaded page extraction limiter closed: {error}"),
                ),
            }
        }
    }
}

async fn collect_archive_extract_batch(
    first: ArchiveExtractRequest,
    receiver: &mut mpsc::Receiver<ArchiveExtractRequest>,
    queue_pressure_depth: usize,
    batch_limit: usize,
    batch_delay: Duration,
    pressure_batch_delay: Duration,
) -> Vec<ArchiveExtractRequest> {
    let mut batch = vec![first];
    let queued = receiver.len();
    let delay = if queued >= queue_pressure_depth {
        Some(pressure_batch_delay)
    } else if queued > 0 {
        Some(batch_delay)
    } else {
        None
    };

    if let Some(delay) = delay {
        tokio::time::sleep(delay).await;
    }

    while batch.len() < batch_limit {
        match receiver.try_recv() {
            Ok(request) => batch.push(request),
            Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected) => {
                break;
            }
        }
    }

    batch
}

fn group_archive_extract_requests(
    batch: Vec<ArchiveExtractRequest>,
) -> Vec<Vec<ArchiveExtractRequest>> {
    let mut grouped = HashMap::<String, Vec<ArchiveExtractRequest>>::new();
    for request in batch {
        grouped
            .entry(request.archive_key.clone())
            .or_default()
            .push(request);
    }

    grouped.into_values().collect()
}

fn sort_archive_extract_groups(grouped: &mut [Vec<ArchiveExtractRequest>]) {
    grouped.sort_unstable_by(|left, right| {
        group_has_foreground(right)
            .cmp(&group_has_foreground(left))
            .then_with(|| right.len().cmp(&left.len()))
            .then_with(|| lowest_file_position(left).cmp(&lowest_file_position(right)))
    });
}

fn group_has_foreground(group: &[ArchiveExtractRequest]) -> bool {
    group
        .iter()
        .any(|request| request.mode == ExtractionRequestMode::Foreground)
}

fn lowest_file_position(group: &[ArchiveExtractRequest]) -> usize {
    group
        .iter()
        .map(request_lowest_file_position)
        .min()
        .unwrap_or(usize::MAX)
}

fn request_lowest_file_position(request: &ArchiveExtractRequest) -> usize {
    request
        .targets
        .iter()
        .map(|target| target.file_position)
        .min()
        .unwrap_or(usize::MAX)
}

async fn run_archive_extract_group(
    _permit: OwnedSemaphorePermit,
    mut group: Vec<ArchiveExtractRequest>,
) {
    group.sort_unstable_by_key(request_lowest_file_position);
    let Some(first) = group.first() else {
        return;
    };
    let metrics = first.metrics;
    let now = Instant::now();
    for request in &group {
        request.metrics.record_downloaded_page_extract_queue_wait(
            "started",
            now.saturating_duration_since(request.queued_at),
        );
    }
    let archive_path = first.archive_path.clone();
    let archive_key = first.archive_key.clone();
    if !archive_key_is_current(&archive_path, &archive_key) {
        reply_archive_extract_stale(group);
        return;
    }
    let targets = group
        .iter()
        .flat_map(|request| request.targets.iter().copied())
        .collect::<Vec<_>>();
    let expected_entries = targets.len();
    let request_count = group.len();

    let started = Instant::now();
    match run_positioned_extract(archive_path, targets).await {
        Ok(entries) if entries.len() == expected_entries => {
            metrics.record_downloaded_page_extract_worker(
                "success",
                started.elapsed(),
                request_count,
                expected_entries,
            );
            reply_archive_extract_success(group, entries);
        }
        Ok(entries) => {
            metrics.record_downloaded_page_extract_worker(
                "error",
                started.elapsed(),
                request_count,
                expected_entries,
            );
            reply_archive_extract_error(
                group,
                &format!(
                    "positioned extraction returned {} entries for {expected_entries} positions",
                    entries.len()
                ),
            );
        }
        Err(error) => {
            metrics.record_downloaded_page_extract_worker(
                "error",
                started.elapsed(),
                request_count,
                expected_entries,
            );
            reply_archive_extract_error(group, &error.to_string());
        }
    }
}

fn reply_archive_extract_success(
    group: Vec<ArchiveExtractRequest>,
    mut entries: Vec<backend_image::EntryBytes>,
) {
    for request in group {
        let request_entries = entries.drain(..request.targets.len()).collect::<Vec<_>>();
        let _ = request
            .reply
            .send(Ok(PositionedExtractionResult::Completed(request_entries)));
    }
}

fn reply_archive_extract_error(group: Vec<ArchiveExtractRequest>, message: &str) {
    for request in group {
        let _ = request.reply.send(Err(anyhow::anyhow!("{message}")));
    }
}

fn reply_archive_extract_stale(group: Vec<ArchiveExtractRequest>) {
    for request in group {
        let _ = request
            .reply
            .send(Ok(PositionedExtractionResult::StaleArchive));
    }
}

async fn run_positioned_extract(
    archive_path: PathBuf,
    targets: Vec<PositionedImageTarget>,
) -> Result<Vec<backend_image::EntryBytes>> {
    let (sender, receiver) = oneshot::channel();
    rayon::spawn(move || {
        let targets = targets
            .into_iter()
            .map(|target| (target.file_position, target.content_type))
            .collect::<Vec<_>>();
        let result =
            backend_image::extract_positioned_images_with_content_types(&archive_path, &targets);
        let _ = sender.send(result);
    });
    receiver
        .await
        .context("positioned extraction worker stopped before replying")?
}

fn downloaded_page_window_key(archive_key: &str, page: usize, window_size: usize) -> String {
    let window_start = page / window_size.max(1);
    let mut key = String::with_capacity(archive_key.len() + 50);
    key.push_str("library:chapter-page-window:");
    key.push_str(archive_key);
    key.push(':');
    let _ = write!(key, "{window_start}");
    key
}

fn archive_key_is_current(archive_path: &Path, archive_key: &str) -> bool {
    current_archive_key(archive_path).is_ok_and(|current| current == archive_key)
}

fn current_archive_key(archive_path: &Path) -> Result<String> {
    let metadata = backend_fs::metadata_sync(archive_path)
        .with_context(|| format!("failed to read metadata for {}", archive_path.display()))?;
    let archive_mtime_ms = metadata
        .modified()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or_default();
    Ok(format!(
        "{}:{}:{}:{}",
        archive_path.to_string_lossy(),
        ARCHIVE_INDEX_SCHEMA_VERSION,
        archive_mtime_ms,
        i64::try_from(metadata.len()).unwrap_or(i64::MAX),
    ))
}

fn downloaded_page_extract_concurrency() -> usize {
    std::env::var("MANGA_DOWNLOADED_PAGE_EXTRACT_CONCURRENCY")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(default_downloaded_page_extract_concurrency)
        .clamp(
            MIN_MAX_CONCURRENT_DOWNLOADED_PAGE_EXTRACTS,
            MAX_MAX_CONCURRENT_DOWNLOADED_PAGE_EXTRACTS,
        )
}

fn default_downloaded_page_extract_concurrency() -> usize {
    let available = std::thread::available_parallelism().map_or(
        DEFAULT_MAX_CONCURRENT_DOWNLOADED_PAGE_EXTRACTS,
        std::num::NonZeroUsize::get,
    );
    ((available * 3) / 4).clamp(
        MIN_MAX_CONCURRENT_DOWNLOADED_PAGE_EXTRACTS,
        DEFAULT_MAX_CONCURRENT_DOWNLOADED_PAGE_EXTRACTS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sequential_foreground_reads_expand_the_read_window() {
        let scheduler = DownloadedPageExtractionScheduler::new(test_config());

        assert_eq!(
            scheduler
                .foreground_read_window_size("archive.cbz", 0, 8)
                .await,
            1
        );
        assert_eq!(
            scheduler
                .foreground_read_window_size("archive.cbz", 1, 8)
                .await,
            8
        );
    }

    #[tokio::test]
    async fn queued_positioned_extraction_preserves_request_order() {
        let archive_path = test_archive(&[("001.jpg", b"first"), ("002.jpg", b"second")]);
        let scheduler = DownloadedPageExtractionScheduler::new(DownloadedPageExtractionConfig {
            direct_queue_bypass_depth: 0,
            ..test_config()
        });
        let metrics = Metrics::default();

        let entries = scheduler
            .extract_positioned_images(
                &metrics,
                current_archive_key(&archive_path).expect("archive key should resolve"),
                archive_path,
                vec![
                    PositionedImageTarget {
                        file_position: 1,
                        content_type: "image/jpeg",
                    },
                    PositionedImageTarget {
                        file_position: 0,
                        content_type: "image/jpeg",
                    },
                ],
                ExtractionRequestMode::ReadAhead,
            )
            .await
            .expect("extraction should succeed");

        let PositionedExtractionResult::Completed(entries) = entries else {
            panic!("extraction should complete");
        };
        assert_eq!(entries[0].bytes, b"second");
        assert_eq!(entries[1].bytes, b"first");
    }

    #[tokio::test]
    async fn queue_pressure_caps_foreground_read_window() {
        let scheduler = DownloadedPageExtractionScheduler::new(DownloadedPageExtractionConfig {
            queue_pressure_depth: 0,
            ..test_config()
        });

        assert_eq!(
            scheduler
                .foreground_read_window_size("archive.cbz", 0, 8)
                .await,
            1
        );
        assert_eq!(
            scheduler
                .foreground_read_window_size("archive.cbz", 1, 8)
                .await,
            1
        );
    }

    #[tokio::test]
    async fn invalidating_archive_path_clears_versioned_access_state() {
        let scheduler = DownloadedPageExtractionScheduler::new(test_config());
        let archive_path = "/tmp/archive.cbz";
        let archive_key = format!("{archive_path}:{ARCHIVE_INDEX_SCHEMA_VERSION}:10:20");

        assert_eq!(
            scheduler
                .foreground_read_window_size(&archive_key, 0, 8)
                .await,
            1
        );
        assert_eq!(
            scheduler
                .foreground_read_window_size(&archive_key, 1, 8)
                .await,
            8
        );

        scheduler.invalidate_archive(archive_path).await;

        assert_eq!(
            scheduler
                .foreground_read_window_size(&archive_key, 2, 8)
                .await,
            1
        );
    }

    #[tokio::test]
    async fn read_ahead_drops_before_queueing_under_pressure() {
        let scheduler = DownloadedPageExtractionScheduler::new(DownloadedPageExtractionConfig {
            queue_pressure_depth: 0,
            ..test_config()
        });
        let metrics = Metrics::default();

        let result = scheduler
            .extract_positioned_images(
                &metrics,
                "unused:stale".to_string(),
                PathBuf::from("unused.cbz"),
                vec![PositionedImageTarget {
                    file_position: 0,
                    content_type: "image/jpeg",
                }],
                ExtractionRequestMode::ReadAhead,
            )
            .await
            .expect("pressure drop should not error");

        assert!(matches!(
            result,
            PositionedExtractionResult::DroppedQueuePressure
        ));
    }

    #[tokio::test]
    async fn queued_extraction_rejects_stale_archive_key_before_reading() {
        let archive_path = test_archive(&[("001.jpg", b"first")]);
        let archive_key = current_archive_key(&archive_path).expect("archive key should resolve");
        let scheduler = DownloadedPageExtractionScheduler::new(DownloadedPageExtractionConfig {
            max_concurrent_extracts: 1,
            direct_queue_bypass_depth: 0,
            batch_delay: Duration::from_millis(25),
            ..test_config()
        });
        let permit = Arc::clone(&scheduler.extraction_limiter)
            .acquire_owned()
            .await
            .expect("permit should be acquired");
        let metrics = Metrics::default();

        let extraction = scheduler.extract_positioned_images(
            &metrics,
            archive_key,
            archive_path.clone(),
            vec![PositionedImageTarget {
                file_position: 0,
                content_type: "image/jpeg",
            }],
            ExtractionRequestMode::ReadAhead,
        );
        tokio::pin!(extraction);
        tokio::time::sleep(Duration::from_millis(10)).await;
        std::fs::write(&archive_path, b"archive changed").expect("archive should be mutated");
        drop(permit);

        let result = extraction.await.expect("stale extraction should not error");
        assert!(matches!(result, PositionedExtractionResult::StaleArchive));
    }

    #[test]
    fn extract_groups_prioritize_foreground_before_read_ahead() {
        let mut groups = vec![
            vec![test_request(
                "read-ahead",
                5,
                ExtractionRequestMode::ReadAhead,
            )],
            vec![test_request(
                "foreground",
                9,
                ExtractionRequestMode::Foreground,
            )],
        ];

        sort_archive_extract_groups(&mut groups);

        assert_eq!(groups[0][0].archive_key, "foreground");
        assert_eq!(groups[1][0].archive_key, "read-ahead");
    }

    #[test]
    fn extract_grouping_batches_requests_by_archive_key() {
        let grouped = group_archive_extract_requests(vec![
            test_request("archive-a", 1, ExtractionRequestMode::ReadAhead),
            test_request("archive-b", 2, ExtractionRequestMode::Foreground),
            test_request("archive-a", 3, ExtractionRequestMode::Foreground),
        ]);

        let archive_a = grouped
            .iter()
            .find(|group| group[0].archive_key == "archive-a")
            .expect("archive-a group should exist");
        assert_eq!(archive_a.len(), 2);
    }

    fn test_config() -> DownloadedPageExtractionConfig {
        DownloadedPageExtractionConfig {
            max_concurrent_extracts: 4,
            queue_limit: 8,
            queue_pressure_depth: 4,
            direct_queue_bypass_depth: 1,
            batch_limit: 8,
            batch_delay: Duration::ZERO,
            pressure_batch_delay: Duration::ZERO,
        }
    }

    fn test_archive(entries: &[(&str, &[u8])]) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "manga-server-extraction-test-{}",
            uuid::Uuid::new_v4()
        ));
        let page_dir = root.join("pages");
        std::fs::create_dir_all(&page_dir).expect("test page dir should be created");
        let archive_path = root.join("chapter.cbz");
        let pages = entries
            .iter()
            .map(|(name, bytes)| {
                let path = page_dir.join(name);
                std::fs::write(&path, bytes).expect("test page should be written");
                ((*name).to_string(), path)
            })
            .collect::<Vec<_>>();

        backend_image::build_zstd_folder_from_paths(&pages, &archive_path, None, None)
            .expect("test archive should be built");
        archive_path
    }

    fn test_request(
        archive_key: &str,
        file_position: usize,
        mode: ExtractionRequestMode,
    ) -> ArchiveExtractRequest {
        let (reply, _response) = oneshot::channel();
        ArchiveExtractRequest {
            archive_key: archive_key.to_string(),
            archive_path: PathBuf::from(format!("{archive_key}.cbz")),
            targets: vec![PositionedImageTarget {
                file_position,
                content_type: "image/jpeg",
            }],
            mode,
            queued_at: Instant::now(),
            metrics: Metrics::default(),
            reply,
        }
    }
}
