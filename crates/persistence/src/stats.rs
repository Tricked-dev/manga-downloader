use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::Result;

use crate::models::{
    ChapterRow, StatsActivityPoint, StatsCacheSummary, StatsOverview, StatsRecentChapter,
    StatsSeriesStorage, StatsSourceBreakdown, StatsStorageSummary, StatsTotals,
};
use crate::schema::{Chapter, Download, LibrarySeries, Source, StatsEvent};
use crate::{Database, decode_chapter_number, now_timestamp};

const ACTIVITY_DAYS: u64 = 30;
const MILLIS_PER_DAY: u64 = 86_400_000;
const RECENT_READ_LIMIT: usize = 6;

impl Database {
    /// Records read-progress stats events for a chapter update.
    pub async fn record_read_progress_activity(
        &self,
        chapter: &ChapterRow,
        pages_read_delta: usize,
        chapter_completed: bool,
    ) -> Result<()> {
        if pages_read_delta == 0 && !chapter_completed {
            return Ok(());
        }

        let _write = self.write_guard().await;
        let mut db = self.executor();
        let series =
            LibrarySeries::filter(LibrarySeries::fields().id().eq(chapter.manga_id.as_str()))
                .first()
                .exec(&mut db)
                .await?;
        let source = series.map(|series| series.source_key);

        if pages_read_delta > 0 {
            insert_stats_event(
                &mut db,
                "pages_read",
                source.as_deref(),
                Some(&chapter.manga_id),
                Some(&chapter.id),
                i64::try_from(pages_read_delta)?,
            )
            .await?;
        }
        if chapter_completed {
            insert_stats_event(
                &mut db,
                "chapters_read",
                source.as_deref(),
                Some(&chapter.manga_id),
                Some(&chapter.id),
                1,
            )
            .await?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    /// Builds the aggregate stats overview for the library dashboard.
    pub async fn get_stats_overview(&self) -> Result<StatsOverview> {
        let mut db = self.executor();
        let today = timestamp_day(&now_timestamp()).unwrap_or_default();
        let start_day = today.saturating_sub(ACTIVITY_DAYS - 1);
        let start_timestamp = format!("{:020}", start_day.saturating_mul(MILLIS_PER_DAY));

        let source_count = Source::all().count().exec(&mut db).await?;
        let series_count = LibrarySeries::all().count().exec(&mut db).await?;
        let chapter_count = Chapter::all().count().exec(&mut db).await?;

        let sources = Source::all().exec(&mut db).await?;
        let series = LibrarySeries::all().exec(&mut db).await?;
        let chapters = Chapter::all().exec(&mut db).await?;
        let completed_downloads = Download::filter(Download::fields().status().eq("completed"))
            .exec(&mut db)
            .await?;
        let active_download_count = Download::filter(Download::fields().status().in_list([
            "queued",
            "cancel_requested",
            "fetching_pages",
            "downloading_assets",
            "transforming_assets",
            "archiving",
        ]))
        .count()
        .exec(&mut db)
        .await?;
        let failed_download_count = Download::filter(Download::fields().status().eq("failed"))
            .count()
            .exec(&mut db)
            .await?;
        let events = StatsEvent::filter(StatsEvent::fields().created_at().ge(start_timestamp))
            .exec(&mut db)
            .await?;

        let series_by_id = series
            .iter()
            .map(|series| (series.id.as_str(), series))
            .collect::<HashMap<_, _>>();
        let mut source_breakdown = sources
            .iter()
            .map(|source| (source.key.as_str(), SourceStatsAccumulator::default()))
            .collect::<BTreeMap<_, _>>();

        for series in &series {
            source_breakdown
                .entry(series.source_key.as_str())
                .or_default()
                .series += 1;
        }

        let mut pages_read = 0usize;
        let mut chapters_read = 0usize;
        let mut new_chapters = 0usize;

        for chapter in &chapters {
            let chapter_pages_read = usize::try_from(chapter.pages_read).unwrap_or_default();
            pages_read += chapter_pages_read;
            chapters_read += usize::from(chapter.read_completed);
            new_chapters += usize::from(chapter.is_new);

            if let Some(series) = series_by_id.get(chapter.series_id.as_str()) {
                let stats = source_breakdown
                    .entry(series.source_key.as_str())
                    .or_default();
                stats.chapters += 1;
                stats.pages_read += chapter_pages_read;
                stats.chapters_read += usize::from(chapter.read_completed);
            }
        }

        let mut completed_chapter_ids = HashSet::new();
        let mut pages_downloaded = 0usize;
        let mut series_storage = HashMap::<&str, SeriesStorageAccumulator>::new();
        for download in &completed_downloads {
            completed_chapter_ids.insert(download.chapter_id.as_str());
            let downloaded_pages = usize::try_from(download.page_count).unwrap_or_default();
            pages_downloaded += downloaded_pages;

            let storage = series_storage
                .entry(download.series_id.as_str())
                .or_default();
            storage.bytes +=
                u64::try_from(download.file_size_bytes.unwrap_or_default()).unwrap_or_default();
            storage.chapters += 1;
            storage.upscaled_chapters += usize::from(download.upscaled_at.is_some());

            if let Some(series) = series_by_id.get(download.series_id.as_str()) {
                let stats = source_breakdown
                    .entry(series.source_key.as_str())
                    .or_default();
                stats.pages_downloaded += downloaded_pages;
                stats.chapters_downloaded += 1;
            }
        }

        let mut series_storage = series_storage
            .into_iter()
            .filter_map(|(series_id, storage)| {
                let series = series_by_id.get(series_id)?;
                Some(StatsSeriesStorage {
                    series_id: series_id.to_owned(),
                    title: series.title.clone(),
                    source: series.source_key.clone(),
                    bytes: storage.bytes,
                    chapters: storage.chapters,
                    upscaled_chapters: storage.upscaled_chapters,
                })
            })
            .collect::<Vec<_>>();
        series_storage.sort_by(|left, right| {
            right
                .bytes
                .cmp(&left.bytes)
                .then_with(|| left.title.cmp(&right.title))
        });
        let recorded_bytes = series_storage.iter().map(|series| series.bytes).sum();

        let activity = build_activity(today, start_day, &events, &chapters, &completed_downloads);
        let source_breakdown = source_breakdown
            .into_iter()
            .filter(|(_, stats)| stats.has_activity())
            .map(|(source, stats)| StatsSourceBreakdown {
                source: source.to_owned(),
                series: stats.series,
                chapters: stats.chapters,
                chapters_read: stats.chapters_read,
                chapters_downloaded: stats.chapters_downloaded,
                pages_read: stats.pages_read,
                pages_downloaded: stats.pages_downloaded,
            })
            .collect();

        Ok(StatsOverview {
            totals: StatsTotals {
                pages_read,
                pages_downloaded,
                chapters_read,
                chapters_downloaded: completed_chapter_ids.len(),
                library_series: usize::try_from(series_count).unwrap_or_default(),
                library_chapters: usize::try_from(chapter_count).unwrap_or_default(),
                active_downloads: usize::try_from(active_download_count).unwrap_or_default(),
                failed_downloads: usize::try_from(failed_download_count).unwrap_or_default(),
                plugin_sources: usize::try_from(source_count).unwrap_or_default(),
                new_chapters,
            },
            cache: StatsCacheSummary::default(),
            storage: StatsStorageSummary::default(),
            series_storage,
            recorded_bytes,
            activity,
            source_breakdown,
            recent_reads: recent_reads(chapters, &series_by_id),
        })
    }
}

async fn insert_stats_event(
    db: &mut dyn toasty::Executor,
    kind: &str,
    source: Option<&str>,
    series_id: Option<&str>,
    chapter_id: Option<&str>,
    amount: i64,
) -> Result<()> {
    if amount <= 0 {
        return Ok(());
    }

    StatsEvent::create()
        .kind(kind.to_string())
        .source(source.map(ToOwned::to_owned))
        .series_id(series_id.map(ToOwned::to_owned))
        .chapter_id(chapter_id.map(ToOwned::to_owned))
        .amount(amount)
        .exec(db)
        .await?;
    Ok(())
}

#[derive(Default)]
struct SeriesStorageAccumulator {
    bytes: u64,
    chapters: usize,
    upscaled_chapters: usize,
}

#[derive(Default)]
struct SourceStatsAccumulator {
    series: usize,
    chapters: usize,
    chapters_read: usize,
    chapters_downloaded: usize,
    pages_read: usize,
    pages_downloaded: usize,
}

impl SourceStatsAccumulator {
    const fn has_activity(&self) -> bool {
        self.series > 0
            || self.chapters > 0
            || self.chapters_read > 0
            || self.chapters_downloaded > 0
            || self.pages_read > 0
            || self.pages_downloaded > 0
    }
}

fn build_activity(
    today: u64,
    start_day: u64,
    events: &[StatsEvent],
    chapters: &[Chapter],
    downloads: &[Download],
) -> Vec<StatsActivityPoint> {
    let mut days = (start_day..=today)
        .map(|day| (day, StatsActivityAccumulator::default()))
        .collect::<BTreeMap<_, _>>();
    let mut event_read_chapter_ids = HashSet::<&str>::new();
    let mut event_download_pages_by_chapter = HashMap::<&str, usize>::new();
    let mut event_download_chapter_ids = HashSet::<&str>::new();

    for event in events {
        let amount = usize::try_from(event.amount).unwrap_or_default();
        if let Some(chapter_id) = event.chapter_id.as_deref() {
            match event.kind.as_str() {
                "chapters_read" => {
                    event_read_chapter_ids.insert(chapter_id);
                }
                "pages_downloaded" => {
                    *event_download_pages_by_chapter
                        .entry(chapter_id)
                        .or_default() += amount;
                }
                "chapters_downloaded" => {
                    event_download_chapter_ids.insert(chapter_id);
                }
                _ => {}
            }
        }

        let Some(day) = timestamp_day(&event.created_at) else {
            continue;
        };
        if day < start_day || day > today {
            continue;
        }

        let stats = days.entry(day).or_default();
        match event.kind.as_str() {
            "chapters_read" => stats.chapters_read += amount,
            "pages_downloaded" => stats.pages_downloaded += amount,
            "chapters_downloaded" => stats.chapters_downloaded += amount,
            _ => {}
        }
    }

    for chapter in chapters {
        if !chapter.read_completed || event_read_chapter_ids.contains(chapter.id.as_str()) {
            continue;
        }
        let Some(day) = chapter.last_read_at.as_deref().and_then(timestamp_day) else {
            continue;
        };
        if day >= start_day && day <= today {
            days.entry(day).or_default().chapters_read += 1;
        }
    }

    for download in downloads {
        let timestamp = download
            .finished_at
            .as_deref()
            .unwrap_or(&download.queued_at);
        let Some(day) = timestamp_day(timestamp) else {
            continue;
        };
        if day < start_day || day > today {
            continue;
        }

        let stats = days.entry(day).or_default();
        if !event_download_chapter_ids.contains(download.chapter_id.as_str()) {
            stats.chapters_downloaded += 1;
        }

        let downloaded_pages = usize::try_from(download.page_count).unwrap_or_default();
        let already_recorded = event_download_pages_by_chapter
            .get(download.chapter_id.as_str())
            .copied()
            .unwrap_or_default();
        stats.pages_downloaded += downloaded_pages.saturating_sub(already_recorded);
    }

    days.into_iter()
        .map(|(day, stats)| StatsActivityPoint {
            day,
            chapters_read: stats.chapters_read,
            pages_downloaded: stats.pages_downloaded,
            chapters_downloaded: stats.chapters_downloaded,
        })
        .collect()
}

#[derive(Default)]
struct StatsActivityAccumulator {
    chapters_read: usize,
    pages_downloaded: usize,
    chapters_downloaded: usize,
}

fn recent_reads(
    mut chapters: Vec<Chapter>,
    series_by_id: &HashMap<&str, &LibrarySeries>,
) -> Vec<StatsRecentChapter> {
    chapters.retain(|chapter| chapter.last_read_at.is_some());
    chapters.sort_by(|left, right| {
        right.last_read_at.cmp(&left.last_read_at).then_with(|| {
            decode_chapter_number(right.number).total_cmp(&decode_chapter_number(left.number))
        })
    });

    chapters
        .into_iter()
        .take(RECENT_READ_LIMIT)
        .filter_map(|chapter| {
            let series = series_by_id.get(chapter.series_id.as_str())?;
            Some(StatsRecentChapter {
                chapter_id: chapter.id,
                manga_id: series.id.clone(),
                manga_title: series.title.clone(),
                chapter_title: chapter.title,
                source: series.source_key.clone(),
                pages_read: usize::try_from(chapter.pages_read).unwrap_or_default(),
                read_completed: chapter.read_completed,
                last_read_at: chapter.last_read_at?,
            })
        })
        .collect()
}

fn timestamp_day(timestamp: &str) -> Option<u64> {
    timestamp
        .parse::<u64>()
        .ok()
        .map(|millis| millis / MILLIS_PER_DAY)
}
