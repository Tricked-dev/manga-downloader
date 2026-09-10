#![allow(clippy::missing_errors_doc)]

use std::borrow::Cow;
use std::hash::Hash;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use backend_core::parse_positive_byte_size;
use backend_runtime::truncate_for_log;
use bytes::Bytes;
use foyer::{
    BlockEngineConfig, DeviceBuilder, FsDeviceBuilder, HybridCache, HybridCacheBuilder,
    PsyncIoEngineConfig, S3FifoConfig,
};
use metrics::{
    Counter, Gauge, Histogram, Label, counter, describe_counter, describe_gauge,
    describe_histogram, gauge, histogram,
};
use mixtrics::metrics::{
    BoxedCounter, BoxedCounterVec, BoxedGauge, BoxedGaugeVec, BoxedHistogram, BoxedHistogramVec,
    CounterOps, CounterVecOps, GaugeOps, GaugeVecOps, HistogramOps, HistogramVecOps, RegistryOps,
};
use rkyv::{
    Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize,
    api::high::{HighDeserializer, HighSerializer, HighValidator},
    bytecheck::CheckBytes,
    rancor::Error as RkyvError,
    ser::allocator::ArenaHandle,
    util::AlignedVec,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const CACHE_SHARDS: usize = 8;
pub const DEFAULT_CACHE_MAX_MEMORY_BYTES: usize = 256 * 1024 * 1024;
const DISK_CAPACITY_BYTES: usize = 1536 * 1024 * 1024;
const BLOCK_SIZE_BYTES: usize = 16 * 1024 * 1024;
const BLOB_INDEX_SIZE_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CacheKey {
    Search {
        plugin: String,
        query: String,
        page: u32,
        category: Option<String>,
        popular: bool,
        hide_nsfw: bool,
    },
    Details {
        plugin: String,
        id: String,
    },
    Chapters {
        plugin: String,
        manga_id: String,
    },
    Pages {
        plugin: String,
        chapter_id: String,
        proxied_images: bool,
    },
    PageRefs {
        plugin: String,
        chapter_id: String,
        schema_version: u8,
    },
    RouteSnapshot {
        name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedImage {
    pub content_type: String,
    pub body: Bytes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedExpiringImage {
    image: CachedImage,
    expires_at_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedExpiringApi {
    value: Vec<u8>,
    expires_at_ms: u128,
}

pub struct MangaCache {
    backend: HybridCache<UnifiedCacheKey, UnifiedCacheValue>,
    stats: CacheStatsCounters,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub requests: u64,
    pub hit_rate: f64,
}

#[derive(Debug, Default)]
struct CacheStatsCounters {
    hits: AtomicU64,
    misses: AtomicU64,
}

impl MangaCache {
    /// Creates a hybrid memory and disk cache rooted at `disk_path`.
    ///
    /// `max_memory_bytes` is normalized to at least one byte before being
    /// passed to the cache backend.
    pub async fn new(disk_path: &str, max_memory_bytes: usize) -> anyhow::Result<Self> {
        let cache = init_hybrid_disk_cache(
            "manga-server-cache",
            disk_path,
            max_memory_bytes,
            cache_entry_weight,
        )
        .await?;
        tracing::info!(
            disk_path = %disk_path,
            max_memory_bytes = max_memory_bytes.max(1),
            "Cache Initialized",
        );
        Ok(Self {
            backend: cache,
            stats: CacheStatsCounters::default(),
        })
    }

    /// Reads and deserializes an archived API value for `key`.
    ///
    /// Returns `None` when the key is absent, expired, stored as another value
    /// kind, or cannot be deserialized as `T`.
    pub async fn get_typed<T>(&self, key: &CacheKey) -> Option<T>
    where
        T: Archive,
        T::Archived: for<'a> CheckBytes<HighValidator<'a, RkyvError>>
            + RkyvDeserialize<T, HighDeserializer<RkyvError>>,
    {
        let cache_key = UnifiedCacheKey::Api(key.clone());
        let bytes = self.archived_api_bytes(&cache_key).await?;

        match rkyv::from_bytes::<T, RkyvError>(&bytes) {
            Ok(parsed) => Some(parsed),
            Err(err) => {
                tracing::debug!(
                    error = %err,
                    outcome = "error",
                    "Cache Rkyv Deserialize Failed",
                );
                None
            }
        }
    }

    /// Reads raw API response bytes for `key`.
    ///
    /// Expired entries are removed and reported as missing.
    pub async fn get_api_bytes(&self, key: &CacheKey) -> Option<Vec<u8>> {
        let cache_key = UnifiedCacheKey::Api(key.clone());
        self.api_bytes(&cache_key).await
    }

    /// Stores raw API response bytes with a time-to-live.
    pub fn insert_api_bytes_ttl(&self, key: CacheKey, value: Vec<u8>, ttl: Duration) {
        self.insert_value(
            UnifiedCacheKey::Api(key),
            UnifiedCacheValue::ExpiringApi(CachedExpiringApi {
                value,
                expires_at_ms: expires_at_ms(ttl),
            }),
        );
    }

    /// Stores raw API response bytes without an explicit expiry.
    pub fn insert_api_bytes(&self, key: CacheKey, value: Vec<u8>) {
        self.insert_value(UnifiedCacheKey::Api(key), UnifiedCacheValue::Api(value));
    }

    /// Serializes and stores a typed API value without an explicit expiry.
    ///
    /// Serialization failures are logged and do not update the cache.
    pub fn insert_typed<T>(&self, key: CacheKey, value: &T)
    where
        T: for<'a> RkyvSerialize<HighSerializer<AlignedVec, ArenaHandle<'a>, RkyvError>>,
    {
        match rkyv::to_bytes::<RkyvError>(value) {
            Ok(serialized) => {
                self.insert_value(
                    UnifiedCacheKey::Api(key),
                    UnifiedCacheValue::ArchivedApi(serialized.into_vec()),
                );
            }
            Err(err) => tracing::debug!(
                error = %err,
                outcome = "error",
                "Cache Rkyv Serialize Failed",
            ),
        }
    }

    /// Serializes and stores a typed API value with a time-to-live.
    ///
    /// Serialization failures are logged and do not update the cache.
    pub fn insert_typed_ttl<T>(&self, key: CacheKey, value: &T, ttl: Duration)
    where
        T: for<'a> RkyvSerialize<HighSerializer<AlignedVec, ArenaHandle<'a>, RkyvError>>,
    {
        match rkyv::to_bytes::<RkyvError>(value) {
            Ok(serialized) => {
                self.insert_value(
                    UnifiedCacheKey::Api(key),
                    UnifiedCacheValue::ExpiringArchivedApi(CachedExpiringApi {
                        value: serialized.into_vec(),
                        expires_at_ms: expires_at_ms(ttl),
                    }),
                );
            }
            Err(err) => tracing::debug!(
                error = %err,
                outcome = "error",
                "Cache Rkyv Serialize Failed",
            ),
        }
    }

    /// Removes an API cache entry by key.
    pub fn remove(&self, key: &CacheKey) {
        let key = UnifiedCacheKey::Api(key.clone());
        tracing::trace!(
            key = %describe_unified_cache_key(&key),
            "Cache Remove",
        );
        self.backend.remove(&key);
    }

    /// Reads an image cache entry by key.
    ///
    /// Expired image entries are removed and reported as missing.
    pub async fn get_image(&self, key: &str) -> Option<CachedImage> {
        let cache_key = UnifiedCacheKey::Image(key.to_string());
        let Some(value) = self
            .get_value(&cache_key, "Image cache lookup failed")
            .await
        else {
            self.record_cache_miss();
            return None;
        };
        match value.into_image_value() {
            CacheValueState::Fresh(image) => {
                self.record_cache_hit();
                Some(image)
            }
            CacheValueState::Expired => {
                self.remove_value(&cache_key);
                self.record_cache_miss();
                None
            }
            CacheValueState::WrongKind => {
                self.record_cache_miss();
                None
            }
        }
    }

    /// Stores an image cache entry without an explicit expiry.
    pub fn insert_image(&self, key: String, value: CachedImage) {
        self.insert_value(UnifiedCacheKey::Image(key), UnifiedCacheValue::Image(value));
    }

    /// Stores an image cache entry with a time-to-live.
    pub fn insert_image_ttl(&self, key: String, value: CachedImage, ttl: Duration) {
        self.insert_value(
            UnifiedCacheKey::Image(key),
            UnifiedCacheValue::ExpiringImage(CachedExpiringImage {
                image: value,
                expires_at_ms: expires_at_ms(ttl),
            }),
        );
    }

    /// Removes an image cache entry by key.
    pub fn remove_image(&self, key: &str) {
        let cache_key = UnifiedCacheKey::Image(key.to_string());
        tracing::trace!(
            key = %describe_unified_cache_key(&cache_key),
            "Cache Remove",
        );
        self.backend.remove(&cache_key);
    }

    /// Removes all cache entries and resets in-memory hit statistics.
    pub async fn clear(&self) -> anyhow::Result<()> {
        tracing::info!("Cache Clear Started");
        self.backend.clear().await?;
        self.reset_stats();
        tracing::info!("Cache Clear Completed");
        Ok(())
    }

    /// Flushes and closes the cache backend.
    pub async fn close(&self) -> anyhow::Result<()> {
        tracing::info!("Cache Close Started");
        self.backend.close().await?;
        tracing::info!("Cache Close Completed");
        Ok(())
    }

    async fn get_value(
        &self,
        key: &UnifiedCacheKey,
        failure_message: &'static str,
    ) -> Option<UnifiedCacheValue> {
        match self.backend.get(key).await {
            Ok(Some(entry)) => {
                let value = entry.value().clone();
                tracing::trace!(
                    key = %describe_unified_cache_key(key),
                    estimated_size = estimate_unified_cache_key_size(key) + estimate_unified_cache_value_size(&value),
                    "Cache Read Hit",
                );
                Some(value)
            }
            Ok(None) => {
                tracing::trace!(
                    key = %describe_unified_cache_key(key),
                    "Cache Read Miss",
                );
                None
            }
            Err(err) => {
                tracing::debug!(
                    key = %describe_unified_cache_key(key),
                    error = %err,
                    failure_message,
                    outcome = "error",
                    "Cache Lookup Failed",
                );
                None
            }
        }
    }

    async fn api_bytes(&self, key: &UnifiedCacheKey) -> Option<Vec<u8>> {
        let Some(value) = self.get_value(key, "API cache lookup failed").await else {
            self.record_cache_miss();
            return None;
        };
        match value.into_api_value() {
            CacheValueState::Fresh(bytes) => {
                self.record_cache_hit();
                Some(bytes)
            }
            CacheValueState::Expired => {
                self.remove_value(key);
                self.record_cache_miss();
                None
            }
            CacheValueState::WrongKind => {
                self.record_cache_miss();
                None
            }
        }
    }

    async fn archived_api_bytes(&self, key: &UnifiedCacheKey) -> Option<Vec<u8>> {
        let Some(value) = self.get_value(key, "API cache lookup failed").await else {
            self.record_cache_miss();
            return None;
        };
        match value.into_archived_api_value() {
            CacheValueState::Fresh(bytes) => {
                self.record_cache_hit();
                Some(bytes)
            }
            CacheValueState::Expired => {
                self.remove_value(key);
                self.record_cache_miss();
                None
            }
            CacheValueState::WrongKind => {
                self.record_cache_miss();
                None
            }
        }
    }

    fn insert_value(&self, key: UnifiedCacheKey, value: UnifiedCacheValue) {
        tracing::trace!(
            key = %describe_unified_cache_key(&key),
            estimated_size = cache_entry_weight(&key, &value),
            "Cache Insert",
        );
        self.backend.insert(key, value);
    }

    fn remove_value(&self, key: &UnifiedCacheKey) {
        tracing::trace!(
            key = %describe_unified_cache_key(key),
            "Cache Remove",
        );
        self.backend.remove(key);
    }

    #[allow(clippy::cast_precision_loss)]
    /// Returns an in-memory snapshot of cache hit and miss counters.
    pub fn stats(&self) -> CacheStats {
        let hits = self.stats.hits.load(Ordering::Relaxed);
        let misses = self.stats.misses.load(Ordering::Relaxed);
        let requests = hits.saturating_add(misses);
        CacheStats {
            hits,
            misses,
            requests,
            hit_rate: if requests == 0 {
                0.0
            } else {
                hits as f64 / requests as f64
            },
        }
    }

    fn record_cache_hit(&self) {
        self.stats.hits.fetch_add(1, Ordering::Relaxed);
    }

    fn record_cache_miss(&self) {
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
    }

    fn reset_stats(&self) {
        self.stats.hits.store(0, Ordering::Relaxed);
        self.stats.misses.store(0, Ordering::Relaxed);
    }
}

/// Builds a named `foyer` hybrid cache with shared disk-cache defaults.
///
/// The caller supplies the key/value types and entry weight function so this
/// helper can be reused outside [`MangaCache`].
pub async fn init_hybrid_disk_cache<K, V, W>(
    name: &'static str,
    disk_path: &str,
    max_memory_bytes: usize,
    weighter: W,
) -> Result<HybridCache<K, V>>
where
    K: Clone + Eq + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    V: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    W: Fn(&K, &V) -> usize + Send + Sync + 'static,
{
    let path = Path::new(disk_path);
    backend_fs::create_dir_all(path).await?;

    let device = FsDeviceBuilder::new(path)
        .with_capacity(DISK_CAPACITY_BYTES)
        .build()?;

    let builder = HybridCacheBuilder::new()
        .with_name(name)
        .with_flush_on_close(true)
        .with_metrics_registry(Box::new(MetricsRegistry));

    builder
        .memory(normalize_cache_memory_limit(max_memory_bytes))
        .with_shards(CACHE_SHARDS)
        .with_eviction_config(S3FifoConfig::default())
        .with_weighter(weighter)
        .storage()
        .with_io_engine_config(PsyncIoEngineConfig::new())
        .with_engine_config(
            BlockEngineConfig::new(device)
                .with_block_size(BLOCK_SIZE_BYTES)
                .with_blob_index_size(BLOB_INDEX_SIZE_BYTES),
        )
        .build()
        .await
        .map_err(Into::into)
}

#[must_use]
/// Normalizes a cache memory limit to the minimum accepted backend value.
pub fn normalize_cache_memory_limit(max_memory_bytes: usize) -> usize {
    max_memory_bytes.max(1)
}

#[must_use]
/// Parses a human-readable memory limit, falling back to the default cache size.
///
/// Empty, invalid, or zero values use [`DEFAULT_CACHE_MAX_MEMORY_BYTES`].
pub fn parse_max_memory_bytes(value: Option<&str>) -> usize {
    value
        .and_then(parse_memory_limit)
        .unwrap_or(DEFAULT_CACHE_MAX_MEMORY_BYTES)
}

fn parse_memory_limit(raw: &str) -> Option<usize> {
    parse_positive_byte_size(raw)
        .ok()
        .flatten()
        .and_then(|bytes| usize::try_from(bytes).ok())
}

#[derive(Debug, Default)]
struct MetricsRegistry;

impl RegistryOps for MetricsRegistry {
    fn register_counter_vec(
        &self,
        name: Cow<'static, str>,
        desc: Cow<'static, str>,
        label_names: &'static [&'static str],
    ) -> BoxedCounterVec {
        describe_counter!(name.clone().into_owned(), desc.into_owned());
        Box::new(MetricsCounterVec {
            name: name.into_owned(),
            label_names,
        })
    }

    fn register_gauge_vec(
        &self,
        name: Cow<'static, str>,
        desc: Cow<'static, str>,
        label_names: &'static [&'static str],
    ) -> BoxedGaugeVec {
        describe_gauge!(name.clone().into_owned(), desc.into_owned());
        Box::new(MetricsGaugeVec {
            name: name.into_owned(),
            label_names,
        })
    }

    fn register_histogram_vec(
        &self,
        name: Cow<'static, str>,
        desc: Cow<'static, str>,
        label_names: &'static [&'static str],
    ) -> BoxedHistogramVec {
        describe_histogram!(name.clone().into_owned(), desc.into_owned());
        Box::new(MetricsHistogramVec {
            name: name.into_owned(),
            label_names,
        })
    }

    fn register_histogram_vec_with_buckets(
        &self,
        name: Cow<'static, str>,
        desc: Cow<'static, str>,
        label_names: &'static [&'static str],
        _buckets: Vec<f64>,
    ) -> BoxedHistogramVec {
        self.register_histogram_vec(name, desc, label_names)
    }
}

#[derive(Debug)]
struct MetricsCounterVec {
    name: String,
    label_names: &'static [&'static str],
}

impl CounterVecOps for MetricsCounterVec {
    fn counter(&self, labels: &[Cow<'static, str>]) -> BoxedCounter {
        Box::new(MetricsCounter {
            handle: counter!(self.name.clone(), metric_labels(self.label_names, labels)),
        })
    }
}

#[derive(Debug)]
struct MetricsGaugeVec {
    name: String,
    label_names: &'static [&'static str],
}

impl GaugeVecOps for MetricsGaugeVec {
    fn gauge(&self, labels: &[Cow<'static, str>]) -> BoxedGauge {
        Box::new(MetricsGauge {
            handle: gauge!(self.name.clone(), metric_labels(self.label_names, labels)),
        })
    }
}

#[derive(Debug)]
struct MetricsHistogramVec {
    name: String,
    label_names: &'static [&'static str],
}

impl HistogramVecOps for MetricsHistogramVec {
    fn histogram(&self, labels: &[Cow<'static, str>]) -> BoxedHistogram {
        Box::new(MetricsHistogram {
            handle: histogram!(self.name.clone(), metric_labels(self.label_names, labels)),
        })
    }
}

#[derive(Debug)]
struct MetricsCounter {
    handle: Counter,
}

impl CounterOps for MetricsCounter {
    fn increase(&self, val: u64) {
        self.handle.increment(val);
    }
}

#[derive(Debug)]
struct MetricsGauge {
    handle: Gauge,
}

impl GaugeOps for MetricsGauge {
    fn increase(&self, val: u64) {
        self.handle.increment(u64_to_f64(val));
    }

    fn decrease(&self, val: u64) {
        self.handle.decrement(u64_to_f64(val));
    }

    fn absolute(&self, val: u64) {
        self.handle.set(u64_to_f64(val));
    }
}

#[derive(Debug)]
struct MetricsHistogram {
    handle: Histogram,
}

impl HistogramOps for MetricsHistogram {
    fn record(&self, val: f64) {
        self.handle.record(val);
    }
}

fn metric_labels(label_names: &[&'static str], labels: &[Cow<'static, str>]) -> Vec<Label> {
    label_names
        .iter()
        .zip(labels)
        .map(|(name, value)| Label::new(*name, value.to_string()))
        .collect()
}

#[allow(clippy::cast_precision_loss)]
fn u64_to_f64(value: u64) -> f64 {
    value as f64
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
enum UnifiedCacheKey {
    Api(CacheKey),
    Image(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum UnifiedCacheValue {
    Api(Vec<u8>),
    ExpiringApi(CachedExpiringApi),
    ArchivedApi(Vec<u8>),
    ExpiringArchivedApi(CachedExpiringApi),
    Image(CachedImage),
    ExpiringImage(CachedExpiringImage),
}

impl UnifiedCacheValue {
    fn into_api_value(self) -> CacheValueState<Vec<u8>> {
        match self {
            Self::Api(value) => CacheValueState::Fresh(value),
            Self::ExpiringApi(value) if !is_expired(value.expires_at_ms) => {
                CacheValueState::Fresh(value.value)
            }
            Self::ExpiringApi(_) => CacheValueState::Expired,
            Self::ArchivedApi(_)
            | Self::ExpiringArchivedApi(_)
            | Self::Image(_)
            | Self::ExpiringImage(_) => CacheValueState::WrongKind,
        }
    }

    fn into_archived_api_value(self) -> CacheValueState<Vec<u8>> {
        match self {
            Self::ArchivedApi(value) => CacheValueState::Fresh(value),
            Self::ExpiringArchivedApi(value) if !is_expired(value.expires_at_ms) => {
                CacheValueState::Fresh(value.value)
            }
            Self::ExpiringArchivedApi(_) => CacheValueState::Expired,
            Self::Api(_) | Self::ExpiringApi(_) | Self::Image(_) | Self::ExpiringImage(_) => {
                CacheValueState::WrongKind
            }
        }
    }

    fn into_image_value(self) -> CacheValueState<CachedImage> {
        match self {
            Self::Image(value) => CacheValueState::Fresh(value),
            Self::ExpiringImage(value) if !is_expired(value.expires_at_ms) => {
                CacheValueState::Fresh(value.image)
            }
            Self::ExpiringImage(_) => CacheValueState::Expired,
            Self::Api(_)
            | Self::ExpiringApi(_)
            | Self::ArchivedApi(_)
            | Self::ExpiringArchivedApi(_) => CacheValueState::WrongKind,
        }
    }
}

enum CacheValueState<T> {
    Fresh(T),
    Expired,
    WrongKind,
}

fn cache_entry_weight(key: &UnifiedCacheKey, value: &UnifiedCacheValue) -> usize {
    estimate_unified_cache_key_size(key) + estimate_unified_cache_value_size(value)
}

fn estimate_unified_cache_key_size(key: &UnifiedCacheKey) -> usize {
    match key {
        UnifiedCacheKey::Api(key) => estimate_cache_key_size(key),
        UnifiedCacheKey::Image(url) => url.len(),
    }
}

fn estimate_unified_cache_value_size(value: &UnifiedCacheValue) -> usize {
    match value {
        UnifiedCacheValue::Api(value) => value.len(),
        UnifiedCacheValue::ExpiringApi(value) => value.value.len() + size_of::<u128>(),
        UnifiedCacheValue::ArchivedApi(value) => value.len(),
        UnifiedCacheValue::ExpiringArchivedApi(value) => value.value.len() + size_of::<u128>(),
        UnifiedCacheValue::Image(value) => value.content_type.len() + value.body.len(),
        UnifiedCacheValue::ExpiringImage(value) => {
            value.image.content_type.len() + value.image.body.len() + size_of::<u128>()
        }
    }
}

fn expires_at_ms(ttl: Duration) -> u128 {
    now_ms().saturating_add(ttl.as_millis())
}

fn is_expired(expires_at_ms: u128) -> bool {
    now_ms() >= expires_at_ms
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn estimate_cache_key_size(key: &CacheKey) -> usize {
    match key {
        CacheKey::Search {
            plugin,
            query,
            page: _,
            category,
            popular: _,
            hide_nsfw: _,
        } => plugin.len() + query.len() + category.as_ref().map_or(0, String::len) + 16,
        CacheKey::Details { plugin, id } => plugin.len() + id.len(),
        CacheKey::Chapters { plugin, manga_id } => plugin.len() + manga_id.len(),
        CacheKey::Pages {
            plugin,
            chapter_id,
            proxied_images: _,
        }
        | CacheKey::PageRefs {
            plugin,
            chapter_id,
            schema_version: _,
        } => plugin.len() + chapter_id.len() + 1,
        CacheKey::RouteSnapshot { name } => name.len(),
    }
}

fn describe_unified_cache_key(key: &UnifiedCacheKey) -> String {
    match key {
        UnifiedCacheKey::Api(key) => describe_cache_key(key),
        UnifiedCacheKey::Image(key) => {
            let preview = truncate_for_log(key, 96);
            format!("image:{preview}")
        }
    }
}

fn describe_cache_key(key: &CacheKey) -> String {
    match key {
        CacheKey::Search {
            plugin,
            query,
            page,
            category,
            popular,
            hide_nsfw,
        } => format!(
            "search plugin={plugin} query={} page={page} category={} popular={popular} hide_nsfw={hide_nsfw}",
            truncate_for_log(query, 48),
            category.as_deref().unwrap_or("-"),
        ),
        CacheKey::Details { plugin, id } => {
            format!("details plugin={plugin} id={}", truncate_for_log(id, 48))
        }
        CacheKey::Chapters { plugin, manga_id } => {
            format!(
                "chapters plugin={plugin} manga_id={}",
                truncate_for_log(manga_id, 48),
            )
        }
        CacheKey::Pages {
            plugin,
            chapter_id,
            proxied_images,
        } => {
            format!(
                "pages plugin={plugin} chapter_id={} proxied_images={proxied_images}",
                truncate_for_log(chapter_id, 48),
            )
        }
        CacheKey::PageRefs {
            plugin,
            chapter_id,
            schema_version,
        } => {
            format!(
                "page_refs plugin={plugin} chapter_id={} schema_version={schema_version}",
                truncate_for_log(chapter_id, 48),
            )
        }
        CacheKey::RouteSnapshot { name } => {
            format!("route_snapshot name={}", truncate_for_log(name, 64))
        }
    }
}
