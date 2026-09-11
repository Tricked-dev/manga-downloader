use crate::api::error::AppError;
use crate::{AppState, app::settings, downloader};
use autometrics::autometrics;
use backend_persistence::{StatsCacheSummary, StatsOverview, StatsStorageSummary};
use std::sync::Arc;

#[autometrics]
pub async fn overview(state: &Arc<AppState>) -> Result<StatsOverview, AppError> {
    let overview = state.db.get_stats_overview().await?;
    let cache = cache_summary(state);
    let storage = storage_summary(state).await;

    Ok(StatsProjectionAssembly::from_persistence(overview)
        .with_cache(cache)
        .with_storage(storage)
        .finish())
}

struct StatsProjectionAssembly {
    overview: StatsOverview,
}

impl StatsProjectionAssembly {
    fn from_persistence(overview: StatsOverview) -> Self {
        Self { overview }
    }

    fn with_cache(mut self, cache: StatsCacheSummary) -> Self {
        self.overview.cache = cache;
        self
    }

    fn with_storage(mut self, storage: StatsStorageSummary) -> Self {
        self.overview.storage = storage;
        self
    }

    fn finish(self) -> StatsOverview {
        self.overview
    }
}

fn cache_summary(state: &Arc<AppState>) -> StatsCacheSummary {
    let cache_stats = state.cache.stats();
    StatsCacheSummary {
        hits: cache_stats.hits,
        misses: cache_stats.misses,
        requests: cache_stats.requests,
        hit_rate: cache_stats.hit_rate,
    }
}

async fn storage_summary(state: &Arc<AppState>) -> StatsStorageSummary {
    let settings = settings::interface(&state.db);
    let limit_bytes = match settings.max_download_storage_bytes().await {
        Ok(limit_bytes) => limit_bytes,
        Err(error) => {
            tracing::warn!(error = %error, "Stats storage limit query failed");
            None
        }
    };

    let download_path = match settings.download_path().await {
        Ok(download_path) => download_path,
        Err(error) => {
            tracing::warn!(error = %error, "Stats download path query failed");
            return StatsStorageSummary {
                limit_bytes,
                ..StatsStorageSummary::default()
            };
        }
    };

    let used_bytes = downloader::cached_download_storage_usage_bytes(state, &download_path).await;
    downloader::refresh_download_storage_usage_if_stale(Arc::clone(state), download_path.clone())
        .await;

    match used_bytes {
        Some(used_bytes) => StatsStorageSummary {
            available: true,
            used_bytes,
            limit_bytes,
            usage_rate: storage_usage_rate(used_bytes, limit_bytes),
        },
        None => StatsStorageSummary {
            limit_bytes,
            ..StatsStorageSummary::default()
        },
    }
}

fn storage_usage_rate(used_bytes: u64, limit_bytes: Option<u64>) -> f64 {
    let Some(limit_bytes) = limit_bytes.filter(|limit_bytes| *limit_bytes > 0) else {
        return 0.0;
    };

    (used_bytes as f64 / limit_bytes as f64).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use backend_persistence::{
        StatsActivityPoint, StatsRecentChapter, StatsSeriesStorage, StatsSourceBreakdown,
        StatsTotals,
    };

    #[test]
    fn stats_projection_assembly_attaches_runtime_summaries() {
        let overview = StatsOverview {
            totals: StatsTotals {
                pages_read: 10,
                pages_downloaded: 20,
                chapters_read: 2,
                chapters_downloaded: 3,
                library_series: 4,
                library_chapters: 5,
                active_downloads: 1,
                failed_downloads: 0,
                plugin_sources: 6,
                new_chapters: 7,
            },
            cache: StatsCacheSummary::default(),
            storage: StatsStorageSummary::default(),
            series_storage: vec![StatsSeriesStorage {
                series_id: "series-1".into(),
                title: "Series One".into(),
                source: "comix".into(),
                bytes: 2048,
                chapters: 2,
                upscaled_chapters: 1,
            }],
            recorded_bytes: 2048,
            activity: vec![StatsActivityPoint {
                day: 42,
                chapters_read: 1,
                pages_downloaded: 9,
                chapters_downloaded: 1,
            }],
            source_breakdown: vec![StatsSourceBreakdown {
                source: "demo".to_string(),
                series: 1,
                chapters: 2,
                chapters_read: 1,
                chapters_downloaded: 1,
                pages_read: 10,
                pages_downloaded: 20,
            }],
            recent_reads: vec![StatsRecentChapter {
                chapter_id: "chapter-1".to_string(),
                manga_id: "manga-1".to_string(),
                manga_title: "Demo".to_string(),
                chapter_title: "One".to_string(),
                source: "demo".to_string(),
                pages_read: 8,
                read_completed: true,
                last_read_at: "00000000000000000042".to_string(),
            }],
        };
        let cache = StatsCacheSummary {
            hits: 8,
            misses: 2,
            requests: 10,
            hit_rate: 0.8,
        };
        let storage = StatsStorageSummary {
            available: true,
            used_bytes: 50,
            limit_bytes: Some(100),
            usage_rate: 0.5,
        };

        let assembled = StatsProjectionAssembly::from_persistence(overview)
            .with_cache(cache)
            .with_storage(storage)
            .finish();

        assert_eq!(assembled.cache.hits, 8);
        assert_eq!(assembled.storage.used_bytes, 50);
        assert_eq!(assembled.recorded_bytes, 2048);
        assert_eq!(assembled.series_storage.len(), 1);
        assert_eq!(assembled.totals.pages_read, 10);
        assert_eq!(assembled.activity.len(), 1);
        assert_eq!(assembled.source_breakdown.len(), 1);
        assert_eq!(assembled.recent_reads.len(), 1);
    }

    #[test]
    fn storage_usage_rate_clamps_to_limit() {
        assert_eq!(storage_usage_rate(25, Some(100)), 0.25);
        assert_eq!(storage_usage_rate(150, Some(100)), 1.0);
    }

    #[test]
    fn storage_usage_rate_without_positive_limit_is_zero() {
        assert_eq!(storage_usage_rate(25, None), 0.0);
        assert_eq!(storage_usage_rate(25, Some(0)), 0.0);
    }
}
