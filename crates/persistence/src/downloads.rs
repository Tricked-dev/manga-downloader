use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};

use crate::schema::{Chapter, Download, DownloadEvent, LibrarySeries, Source, StatsEvent};
use crate::{
    Database, DownloadMetricRow, DownloadRow, DownloadWorkStatus, DownloadWorkTransition,
    decode_chapter_number, decode_progress, encode_progress, now_timestamp,
};

impl Database {
    /// Record a published upscale without counting a second chapter download.
    pub async fn mark_upscaled(
        &self,
        download_id: &str,
        model: &str,
        scale: u32,
        file_size: u64,
    ) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        Download::filter(
            Download::fields()
                .id()
                .eq(download_id)
                .and(Download::fields().status().eq("completed")),
        )
        .update()
        .upscaled_at(Some(now_timestamp()))
        .upscale_model(Some(model.to_string()))
        .upscale_scale(Some(i64::from(scale)))
        .file_size_bytes(Some(i64::try_from(file_size).unwrap_or(i64::MAX)))
        .exec(&mut db)
        .await?;
        Ok(())
    }

    /// Enqueues a single chapter download and returns its download id.
    ///
    /// If an active download already exists for the chapter, that id is returned.
    pub async fn enqueue_download(&self, chapter_id: &str, manga_id: &str) -> Result<String> {
        let mut ids = self
            .enqueue_downloads(manga_id, &[chapter_id.to_string()])
            .await?;
        ids.pop()
            .ok_or_else(|| anyhow!("chapter {chapter_id} was not enqueued"))
    }

    /// Enqueues chapter downloads for a manga and returns download ids in input order.
    ///
    /// Active downloads are reused instead of duplicated.
    pub async fn enqueue_downloads(
        &self,
        manga_id: &str,
        chapter_ids: &[String],
    ) -> Result<Vec<String>> {
        if chapter_ids.is_empty() {
            return Ok(Vec::new());
        }

        let _write = self.write_guard().await;
        let mut db = self.executor();
        let mut tx = db.transaction().await?;

        let chapters = Chapter::filter(
            Chapter::fields()
                .series_id()
                .eq(manga_id)
                .and(Chapter::fields().id().in_list(chapter_ids.to_vec())),
        )
        .exec(&mut tx)
        .await?;
        let chapters_by_id = chapters
            .into_iter()
            .map(|chapter| (chapter.id.clone(), chapter))
            .collect::<HashMap<_, _>>();

        let existing_active = Download::filter(
            Download::fields()
                .chapter_id()
                .in_list(chapter_ids.to_vec())
                .and(Download::fields().series_id().eq(manga_id))
                .and(
                    Download::fields()
                        .status()
                        .in_list(DownloadWorkStatus::ACTIVE_PERSISTED_VALUES),
                ),
        )
        .order_by(Download::fields().queued_at().desc())
        .exec(&mut tx)
        .await?;
        let mut active_by_chapter = HashMap::<String, String>::new();
        for download in existing_active {
            active_by_chapter
                .entry(download.chapter_id)
                .or_insert(download.id);
        }

        let mut ids = Vec::with_capacity(chapter_ids.len());
        for chapter_id in chapter_ids {
            let chapter = chapters_by_id
                .get(chapter_id)
                .ok_or_else(|| anyhow!("chapter {chapter_id} not found for manga {manga_id}"))?;

            if let Some(existing_id) = active_by_chapter.get(chapter_id) {
                ids.push(existing_id.clone());
                continue;
            }

            let download = Download::create()
                .series_id(chapter.series_id.clone())
                .chapter_id(chapter_id.clone())
                .status(DownloadWorkStatus::Queued.persisted_value().to_string())
                .progress_percent(encode_progress(0.0))
                .stage(DownloadWorkTransition::Queued.event_stage().to_string())
                .error_code(None::<String>)
                .error_message(None::<String>)
                .file_path(None::<String>)
                .file_size_bytes(None::<i64>)
                .page_count(0)
                .attempt_count(0)
                .started_at(None::<String>)
                .finished_at(None::<String>)
                .exec(&mut tx)
                .await?;

            self.insert_download_event(
                &mut tx,
                &download.id,
                DownloadWorkTransition::Queued.event_stage(),
                "Download queued",
                None,
            )
            .await?;
            active_by_chapter.insert(chapter_id.clone(), download.id.clone());
            ids.push(download.id);
        }

        tx.commit().await?;
        Ok(ids)
    }

    /// Lists downloads with newest queued items first.
    pub async fn get_downloads(&self) -> Result<Vec<DownloadRow>> {
        let mut db = self.executor();
        let downloads = Download::all()
            .order_by(Download::fields().queued_at().desc())
            .exec(&mut db)
            .await?;
        build_download_rows(&mut db, downloads).await
    }

    /// Lists downloads for one manga with newest queued items first.
    pub async fn get_downloads_for_manga(&self, manga_id: &str) -> Result<Vec<DownloadRow>> {
        let mut db = self.executor();
        let downloads = Download::filter(Download::fields().series_id().eq(manga_id))
            .order_by(Download::fields().queued_at().desc())
            .exec(&mut db)
            .await?;
        build_download_rows(&mut db, downloads).await
    }

    /// Reads compact source/status rows for download metrics.
    pub async fn get_download_metric_rows(&self) -> Result<Vec<DownloadMetricRow>> {
        let mut db = self.executor();
        let downloads = Download::all().exec(&mut db).await?;
        if downloads.is_empty() {
            return Ok(Vec::new());
        }

        let series_ids = downloads
            .iter()
            .map(|download| download.series_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let series = LibrarySeries::filter(LibrarySeries::fields().id().in_list(series_ids))
            .exec(&mut db)
            .await?;
        let series_sources = series
            .into_iter()
            .map(|series| (series.id, series.source_key))
            .collect::<HashMap<_, _>>();

        downloads
            .into_iter()
            .map(|download| {
                let source = series_sources.get(&download.series_id).ok_or_else(|| {
                    anyhow!(
                        "series {} missing for download {}",
                        download.series_id,
                        download.id
                    )
                })?;

                Ok(DownloadMetricRow {
                    status: external_download_status(&download.status),
                    source: source.clone(),
                })
            })
            .collect()
    }

    /// Looks up a download by id.
    pub async fn get_download_by_id(&self, id: &str) -> Result<Option<DownloadRow>> {
        let mut db = self.executor();
        let download = Download::filter(Download::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?;
        let Some(download) = download else {
            return Ok(None);
        };
        let mut rows = build_download_rows(&mut db, vec![download]).await?;
        Ok(rows.pop())
    }

    /// Returns the newest completed download for a chapter.
    pub async fn get_completed_download_by_chapter(
        &self,
        chapter_id: &str,
    ) -> Result<Option<DownloadRow>> {
        let mut db = self.executor();
        let download = Download::filter(
            Download::fields().chapter_id().eq(chapter_id).and(
                Download::fields()
                    .status()
                    .eq(DownloadWorkStatus::Completed.persisted_value()),
            ),
        )
        .order_by(Download::fields().queued_at().desc())
        .first()
        .exec(&mut db)
        .await?;
        let Some(download) = download else {
            return Ok(None);
        };
        let mut rows = build_download_rows(&mut db, vec![download]).await?;
        Ok(rows.pop())
    }

    /// Claims the oldest queued download and moves it into the fetch stage.
    pub async fn get_next_queued_download(&self) -> Result<Option<DownloadRow>> {
        if self.backend() == crate::DatabaseBackend::Postgres {
            return self.claim_postgres_download().await;
        }
        let write_guard = self.write_guard().await;
        let mut db = self.executor();
        let mut tx = db.transaction().await?;

        let Some(mut next) = Download::filter(
            Download::fields()
                .status()
                .eq(DownloadWorkStatus::Queued.persisted_value()),
        )
        .order_by(Download::fields().queued_at().asc())
        .exec(&mut tx)
        .await?
        .into_iter()
        .next() else {
            tx.commit().await?;
            return Ok(None);
        };

        let started_at = next.started_at.clone().unwrap_or_else(now_timestamp);
        let attempt_count = next.attempt_count + 1;

        next.update()
            .status(
                DownloadWorkTransition::FetchingPages
                    .persisted_status()
                    .persisted_value()
                    .to_string(),
            )
            .stage(
                DownloadWorkTransition::FetchingPages
                    .event_stage()
                    .to_string(),
            )
            .progress_percent(encode_progress(0.0))
            .error_code(None::<String>)
            .error_message(None::<String>)
            .started_at(Some(started_at))
            .attempt_count(attempt_count)
            .exec(&mut tx)
            .await?;

        let updated = Download::filter(Download::fields().id().eq(next.id.as_str()))
            .first()
            .exec(&mut tx)
            .await?;

        tx.commit().await?;

        let Some(updated) = updated else {
            return Ok(None);
        };

        drop(db);
        drop(write_guard);
        let mut db = self.executor();
        let mut rows = build_download_rows(&mut db, vec![updated]).await?;
        Ok(rows.pop())
    }

    /// PostgreSQL workers claim one row atomically across connections/processes.
    /// Toasty 0.7 does not expose SKIP LOCKED in model queries.
    async fn claim_postgres_download(&self) -> Result<Option<DownloadRow>> {
        let mut db = self.executor();
        let rows = toasty::sql::query(
            "UPDATE downloads SET status = 'fetching_pages', stage = 'fetching_pages',              progress_percent = 0, error_code = NULL, error_message = NULL,              started_at = COALESCE(started_at, $1), attempt_count = attempt_count + 1              WHERE id = (SELECT id FROM downloads WHERE status = 'queued'              ORDER BY queued_at, id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING id"
        ).bind(now_timestamp()).exec(&mut db).await?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let id = row
            .as_record()
            .and_then(|record| record.first())
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow!("download claim returned an invalid id"))?;
        self.get_download_by_id(id).await
    }

    /// Requeues interrupted downloads left in non-terminal startup states.
    ///
    /// Pending cancellations are completed as cancelled.
    pub async fn recover_interrupted_downloads(&self) -> Result<usize> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let interrupted = Download::filter(
            Download::fields()
                .status()
                .in_list(DownloadWorkStatus::INTERRUPTED_PERSISTED_VALUES),
        )
        .exec(&mut db)
        .await?;
        let recovered = interrupted.len();

        for mut download in interrupted {
            let id = download.id.clone();
            let previous_status = download.status.clone();
            if previous_status == DownloadWorkStatus::CancelRequested.persisted_value() {
                download
                    .update()
                    .status(DownloadWorkStatus::Cancelled.persisted_value().to_string())
                    .stage(DownloadWorkTransition::Cancelled.event_stage().to_string())
                    .error_code(None::<String>)
                    .error_message(None::<String>)
                    .finished_at(Some(now_timestamp()))
                    .exec(&mut db)
                    .await?;
                self.insert_download_event(
                    &mut db,
                    &id,
                    DownloadWorkTransition::Cancelled.event_stage(),
                    "Interrupted cancellation completed on startup",
                    None,
                )
                .await?;
            } else {
                download
                    .update()
                    .status(DownloadWorkStatus::Queued.persisted_value().to_string())
                    .progress_percent(encode_progress(0.0))
                    .stage(DownloadWorkTransition::Queued.event_stage().to_string())
                    .error_code(None::<String>)
                    .error_message(None::<String>)
                    .queued_at(now_timestamp())
                    .started_at(None::<String>)
                    .finished_at(None::<String>)
                    .exec(&mut db)
                    .await?;
                self.insert_download_event(
                    &mut db,
                    &id,
                    "recovered",
                    &format!("Interrupted {previous_status} download requeued on startup"),
                    None,
                )
                .await?;
            }
        }

        Ok(recovered)
    }

    /// Applies a status transition, progress value, and optional error message.
    pub async fn update_download_status(
        &self,
        id: &str,
        transition: DownloadWorkTransition,
        progress: f64,
        error: Option<&str>,
    ) -> Result<()> {
        let status = transition.persisted_status();
        let error_code = error.or_else(|| transition.default_error_code());

        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut download) = Download::filter(Download::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?
        else {
            return Err(anyhow!("download {id} not found"));
        };

        let finished_at = if transition.is_terminal() {
            Some(now_timestamp())
        } else {
            None
        };

        download
            .update()
            .status(status.persisted_value().to_string())
            .progress_percent(encode_progress(progress))
            .stage(transition.event_stage().to_string())
            .error_code(error_code.map(ToOwned::to_owned))
            .error_message(error.map(ToOwned::to_owned))
            .finished_at(finished_at)
            .exec(&mut db)
            .await?;

        self.insert_download_event(
            &mut db,
            id,
            transition.event_stage(),
            error.unwrap_or_else(|| transition.event_stage()),
            None,
        )
        .await?;
        Ok(())
    }

    /// Marks a download completed and records archive metadata and stats events.
    pub async fn complete_download(
        &self,
        id: &str,
        total_pages: usize,
        archive_path: &str,
        archive_size_bytes: u64,
    ) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let mut tx = db.transaction().await?;
        let Some(mut download) = Download::filter(Download::fields().id().eq(id))
            .first()
            .exec(&mut tx)
            .await?
        else {
            return Err(anyhow!("download {id} not found"));
        };

        let was_completed = download.status == DownloadWorkStatus::Completed.persisted_value();
        let total_pages = i64::try_from(total_pages)?;
        let archive_size_bytes = i64::try_from(archive_size_bytes)?;
        let series =
            LibrarySeries::filter(LibrarySeries::fields().id().eq(download.series_id.as_str()))
                .first()
                .exec(&mut tx)
                .await?;
        let source = series.map(|series| series.source_key);

        download
            .update()
            .status(DownloadWorkStatus::Completed.persisted_value().to_string())
            .progress_percent(encode_progress(100.0))
            .stage(DownloadWorkTransition::Completed.event_stage().to_string())
            .error_code(None::<String>)
            .error_message(None::<String>)
            .file_path(Some(archive_path.to_string()))
            .file_size_bytes(Some(archive_size_bytes))
            .page_count(total_pages)
            .finished_at(Some(now_timestamp()))
            .exec(&mut tx)
            .await?;

        if let Some(mut chapter) =
            Chapter::filter(Chapter::fields().id().eq(download.chapter_id.as_str()))
                .first()
                .exec(&mut tx)
                .await?
        {
            chapter.update().is_new(false).exec(&mut tx).await?;
        }

        self.insert_download_event(
            &mut tx,
            id,
            DownloadWorkTransition::Completed.event_stage(),
            "Download completed",
            None,
        )
        .await?;

        if !was_completed {
            insert_stats_event(
                &mut tx,
                "chapters_downloaded",
                source.as_deref(),
                Some(&download.series_id),
                Some(&download.chapter_id),
                1,
            )
            .await?;
            insert_stats_event(
                &mut tx,
                "pages_downloaded",
                source.as_deref(),
                Some(&download.series_id),
                Some(&download.chapter_id),
                total_pages,
            )
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Updates only the progress percentage for a download.
    pub async fn update_download_progress(&self, id: &str, progress: f64) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut download) = Download::filter(Download::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?
        else {
            return Err(anyhow!("download {id} not found"));
        };

        download
            .update()
            .progress_percent(encode_progress(progress))
            .exec(&mut db)
            .await?;
        Ok(())
    }

    /// Deletes a download and its event history.
    ///
    /// The `chapter_id` argument is currently retained for the caller contract.
    pub async fn delete_download_and_reset_chapter(
        &self,
        id: &str,
        _chapter_id: &str,
    ) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let mut tx = db.transaction().await?;
        self.delete_download_and_events(&mut tx, id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Resets a download to the queued state for another attempt.
    pub async fn retry_download(&self, id: &str) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut download) = Download::filter(Download::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?
        else {
            return Err(anyhow!("download {id} not found"));
        };

        download
            .update()
            .status(DownloadWorkStatus::Queued.persisted_value().to_string())
            .progress_percent(encode_progress(0.0))
            .stage(DownloadWorkTransition::Queued.event_stage().to_string())
            .error_code(None::<String>)
            .error_message(None::<String>)
            .queued_at(now_timestamp())
            .started_at(None::<String>)
            .finished_at(None::<String>)
            .exec(&mut db)
            .await?;

        self.insert_download_event(&mut db, id, "retry", "Download retried", None)
            .await?;
        Ok(())
    }

    pub(crate) async fn delete_downloads_for_chapter(
        &self,
        db: &mut dyn toasty::Executor,
        chapter_id: &str,
    ) -> Result<()> {
        let downloads = Download::filter(Download::fields().chapter_id().eq(chapter_id))
            .exec(db)
            .await?;
        for download in downloads {
            self.delete_download_and_events(db, &download.id).await?;
        }
        Ok(())
    }

    async fn insert_download_event(
        &self,
        db: &mut dyn toasty::Executor,
        download_id: &str,
        event_type: &str,
        message: &str,
        payload_json: Option<&str>,
    ) -> Result<()> {
        DownloadEvent::create()
            .download_id(download_id.to_string())
            .event_type(event_type.to_string())
            .message(message.to_string())
            .payload_json(payload_json.map(ToOwned::to_owned))
            .exec(db)
            .await?;
        Ok(())
    }

    async fn delete_download_and_events(
        &self,
        db: &mut dyn toasty::Executor,
        id: &str,
    ) -> Result<()> {
        crate::schema::UpscaleProgress::filter(
            crate::schema::UpscaleProgress::fields()
                .download_id()
                .eq(id),
        )
        .delete()
        .exec(db)
        .await?;
        DownloadEvent::filter(DownloadEvent::fields().download_id().eq(id))
            .delete()
            .exec(db)
            .await?;

        Download::filter(Download::fields().id().eq(id))
            .delete()
            .exec(db)
            .await?;
        Ok(())
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

async fn build_download_rows(
    db: &mut dyn toasty::Executor,
    downloads: Vec<Download>,
) -> Result<Vec<DownloadRow>> {
    if downloads.is_empty() {
        return Ok(Vec::new());
    }

    let chapter_ids = downloads
        .iter()
        .map(|download| download.chapter_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let series_ids = downloads
        .iter()
        .map(|download| download.series_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let chapters = Chapter::filter(Chapter::fields().id().in_list(chapter_ids))
        .exec(db)
        .await?;
    let series = LibrarySeries::filter(LibrarySeries::fields().id().in_list(series_ids))
        .exec(db)
        .await?;
    let source_keys = series
        .iter()
        .map(|row| row.source_key.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let sources = Source::filter(Source::fields().key().in_list(source_keys))
        .exec(db)
        .await?;

    let chapter_map = chapters
        .into_iter()
        .map(|chapter| (chapter.id.clone(), chapter))
        .collect::<HashMap<_, _>>();
    let series_map = series
        .into_iter()
        .map(|series| (series.id.clone(), series))
        .collect::<HashMap<_, _>>();
    let source_map = sources
        .into_iter()
        .map(|source| (source.key.clone(), source))
        .collect::<HashMap<_, _>>();

    downloads
        .into_iter()
        .map(|download| {
            let chapter = chapter_map.get(&download.chapter_id).ok_or_else(|| {
                anyhow!(
                    "chapter {} missing for download {}",
                    download.chapter_id,
                    download.id
                )
            })?;
            let series = series_map.get(&download.series_id).ok_or_else(|| {
                anyhow!(
                    "series {} missing for download {}",
                    download.series_id,
                    download.id
                )
            })?;
            let source = source_map.get(&series.source_key).ok_or_else(|| {
                anyhow!(
                    "source {} missing for series {}",
                    series.source_key,
                    series.id
                )
            })?;

            Ok(DownloadRow {
                upscaled_at: download.upscaled_at,
                upscale_model: download.upscale_model,
                upscale_scale: download
                    .upscale_scale
                    .and_then(|scale| u32::try_from(scale).ok()),
                id: download.id,
                chapter_id: chapter.id.clone(),
                manga_id: series.id.clone(),
                status: external_download_status(&download.status),
                progress: decode_progress(download.progress_percent),
                error: download.error_message,
                chapter_title: chapter.title.clone(),
                chapter_number: decode_chapter_number(chapter.number),
                manga_title: series.title.clone(),
                chapter_source_id: chapter.remote_chapter_id.clone(),
                manga_source: source.key.clone(),
                manga_source_id: series.remote_series_id.clone(),
            })
        })
        .collect()
}

pub(crate) fn external_download_status(status: &str) -> String {
    DownloadWorkStatus::from_persisted_value(status).map_or_else(
        || status.to_string(),
        |work_status| work_status.external_label().to_string(),
    )
}
