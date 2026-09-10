use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};

use crate::schema::{Chapter, Download, LibrarySeries};
use crate::{
    ChapterInsert, ChapterRow, Database, decode_chapter_number, encode_chapter_number,
    now_timestamp,
};

impl Database {
    /// Inserts missing chapters and updates existing chapters by remote source id.
    ///
    /// Returns ids for newly created chapters.
    pub async fn upsert_chapters(
        &self,
        manga_id: &str,
        chapters: Vec<ChapterInsert>,
    ) -> Result<Vec<String>> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let mut tx = db.transaction().await?;
        let mut new_ids = Vec::new();

        for chapter in chapters {
            if let Some(mut existing) =
                find_chapter_by_series_and_remote_id(&mut tx, manga_id, &chapter.source_id).await?
            {
                existing
                    .update()
                    .title(chapter.title)
                    .number(encode_chapter_number(chapter.chapter_number))
                    .published_at(chapter.date_uploaded)
                    .exec(&mut tx)
                    .await?;
            } else {
                let created = Chapter::create()
                    .series_id(manga_id.to_string())
                    .remote_chapter_id(chapter.source_id)
                    .title(chapter.title)
                    .number(encode_chapter_number(chapter.chapter_number))
                    .published_at(chapter.date_uploaded)
                    .is_new(true)
                    .pages_read(0)
                    .read_completed(false)
                    .last_read_at(None::<String>)
                    .fetched_at(now_timestamp())
                    .exec(&mut tx)
                    .await?;
                new_ids.push(created.id);
            }
        }

        tx.commit().await?;
        Ok(new_ids)
    }

    /// Reconciles a source chapter list with persisted chapters.
    ///
    /// Existing chapters are matched by remote id first and chapter number
    /// second. Returns ids for newly created chapters.
    pub async fn sync_chapters(
        &self,
        manga_id: &str,
        chapters: Vec<ChapterInsert>,
    ) -> Result<Vec<String>> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let mut tx = db.transaction().await?;
        let mut stored = Chapter::filter(Chapter::fields().series_id().eq(manga_id))
            .exec(&mut tx)
            .await?;
        let mut new_ids = Vec::with_capacity(chapters.len());
        let mut remote_id_to_index = HashMap::<String, usize>::with_capacity(stored.len());
        let mut number_to_index = HashMap::<i64, usize>::with_capacity(stored.len());

        for (index, chapter) in stored.iter().enumerate() {
            remote_id_to_index.insert(chapter.remote_chapter_id.clone(), index);
            number_to_index.insert(chapter.number, index);
        }

        for incoming in chapters {
            let next_number = encode_chapter_number(incoming.chapter_number);

            if let Some(&index) = remote_id_to_index.get(incoming.source_id.as_str()) {
                let existing = &mut stored[index];
                let previous_number = existing.number;
                existing
                    .update()
                    .title(incoming.title.clone())
                    .number(next_number)
                    .published_at(incoming.date_uploaded.clone())
                    .exec(&mut tx)
                    .await?;
                existing.title = incoming.title;
                existing.number = next_number;
                existing.published_at = incoming.date_uploaded;
                if previous_number != next_number
                    && number_to_index
                        .get(&previous_number)
                        .is_some_and(|&value| value == index)
                {
                    number_to_index.remove(&previous_number);
                }
                number_to_index.insert(next_number, index);
                continue;
            }

            if let Some(&index) = number_to_index.get(&next_number) {
                let existing = &mut stored[index];
                let previous_remote_id = existing.remote_chapter_id.clone();
                existing
                    .update()
                    .remote_chapter_id(incoming.source_id.clone())
                    .title(incoming.title.clone())
                    .number(next_number)
                    .published_at(incoming.date_uploaded.clone())
                    .exec(&mut tx)
                    .await?;
                if previous_remote_id != incoming.source_id {
                    remote_id_to_index.remove(previous_remote_id.as_str());
                    remote_id_to_index.insert(incoming.source_id.clone(), index);
                }
                existing.remote_chapter_id = incoming.source_id;
                existing.title = incoming.title;
                existing.number = next_number;
                existing.published_at = incoming.date_uploaded;
                continue;
            }

            let created = Chapter::create()
                .series_id(manga_id.to_string())
                .remote_chapter_id(incoming.source_id)
                .title(incoming.title)
                .number(next_number)
                .published_at(incoming.date_uploaded)
                .is_new(true)
                .pages_read(0)
                .read_completed(false)
                .last_read_at(None::<String>)
                .fetched_at(now_timestamp())
                .exec(&mut tx)
                .await?;
            new_ids.push(created.id.clone());
            remote_id_to_index.insert(created.remote_chapter_id.clone(), stored.len());
            number_to_index.insert(created.number, stored.len());
            stored.push(created);
        }

        reconcile_duplicate_chapters(&mut tx, manga_id).await?;

        tx.commit().await?;
        Ok(new_ids)
    }

    /// Lists chapters for a manga in reading order.
    pub async fn get_chapters(&self, manga_id: &str) -> Result<Vec<ChapterRow>> {
        let mut db = self.executor();
        let mut chapters = Chapter::filter(Chapter::fields().series_id().eq(manga_id))
            .exec(&mut db)
            .await?;
        chapters.sort_by(compare_chapter_listing);
        let chapter_ids = chapters
            .iter()
            .map(|chapter| chapter.id.clone())
            .collect::<Vec<_>>();
        let completed = completed_download_chapter_ids_for(&mut db, &chapter_ids).await?;
        Ok(chapters
            .into_iter()
            .map(|chapter| map_chapter_row(chapter, &completed))
            .collect())
    }

    /// Lists all library chapters with newest uploads first.
    pub async fn get_all_library_chapters(&self) -> Result<Vec<ChapterRow>> {
        let mut db = self.executor();
        let mut chapters = Chapter::all().exec(&mut db).await?;
        chapters.sort_by(|left, right| {
            right
                .published_at
                .cmp(&left.published_at)
                .then_with(|| {
                    decode_chapter_number(right.number)
                        .total_cmp(&decode_chapter_number(left.number))
                })
                .then_with(|| right.fetched_at.cmp(&left.fetched_at))
        });
        let chapter_ids = chapters
            .iter()
            .map(|chapter| chapter.id.clone())
            .collect::<Vec<_>>();
        let completed = completed_download_chapter_ids_for(&mut db, &chapter_ids).await?;
        Ok(chapters
            .into_iter()
            .map(|chapter| map_chapter_row(chapter, &completed))
            .collect())
    }

    /// Counts chapters for a manga.
    pub async fn get_chapter_count(&self, manga_id: &str) -> Result<usize> {
        let mut db = self.executor();
        let count = Chapter::filter(Chapter::fields().series_id().eq(manga_id))
            .count()
            .exec(&mut db)
            .await?;
        usize::try_from(count).map_err(Into::into)
    }

    /// Looks up a chapter by manga id and remote source chapter id.
    pub async fn get_chapter_by_manga_and_source_id(
        &self,
        manga_id: &str,
        chapter_source_id: &str,
    ) -> Result<Option<ChapterRow>> {
        let mut db = self.executor();
        let chapter =
            find_chapter_by_series_and_remote_id(&mut db, manga_id, chapter_source_id).await?;
        let Some(chapter) = chapter else {
            return Ok(None);
        };
        let completed =
            completed_download_chapter_ids_for(&mut db, std::slice::from_ref(&chapter.id)).await?;
        Ok(Some(map_chapter_row(chapter, &completed)))
    }

    async fn get_chapter_by_manga_and_number(
        &self,
        manga_id: &str,
        chapter_number: f64,
    ) -> Result<Option<ChapterRow>> {
        let mut db = self.executor();
        let target_number = encode_chapter_number(chapter_number);
        let mut matches = Chapter::filter(
            Chapter::fields()
                .series_id()
                .eq(manga_id)
                .and(Chapter::fields().number().eq(target_number)),
        )
        .exec(&mut db)
        .await?;
        matches.sort_by(|left, right| {
            right
                .published_at
                .cmp(&left.published_at)
                .then_with(|| right.fetched_at.cmp(&left.fetched_at))
                .then_with(|| left.id.cmp(&right.id))
        });
        let Some(chapter) = matches.into_iter().next() else {
            return Ok(None);
        };
        let completed =
            completed_download_chapter_ids_for(&mut db, std::slice::from_ref(&chapter.id)).await?;
        Ok(Some(map_chapter_row(chapter, &completed)))
    }

    /// Looks up a chapter by local id.
    pub async fn get_chapter_by_id(&self, id: &str) -> Result<Option<ChapterRow>> {
        let mut db = self.executor();
        let chapter = Chapter::filter(Chapter::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?;
        let Some(chapter) = chapter else {
            return Ok(None);
        };
        let completed =
            completed_download_chapter_ids_for(&mut db, std::slice::from_ref(&chapter.id)).await?;
        Ok(Some(map_chapter_row(chapter, &completed)))
    }

    /// Returns the next chapter in the same series after `id`.
    pub async fn get_next_chapter_by_id(&self, id: &str) -> Result<Option<ChapterRow>> {
        let mut db = self.executor();
        let Some(current) = Chapter::filter(Chapter::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?
        else {
            return Ok(None);
        };

        next_chapter_in_series(&mut db, current).await
    }

    /// Returns the next chapter after a remote source chapter id.
    pub async fn get_next_chapter_by_source_chapter(
        &self,
        source: &str,
        chapter_source_id: &str,
    ) -> Result<Option<ChapterRow>> {
        let mut db = self.executor();
        let series = LibrarySeries::filter(LibrarySeries::fields().source_key().eq(source))
            .exec(&mut db)
            .await?;
        if series.is_empty() {
            return Ok(None);
        }

        let series_ids = series
            .into_iter()
            .map(|series| series.id)
            .collect::<Vec<_>>();
        let Some(current) = Chapter::filter(
            Chapter::fields()
                .series_id()
                .in_list(series_ids)
                .and(Chapter::fields().remote_chapter_id().eq(chapter_source_id)),
        )
        .first()
        .exec(&mut db)
        .await?
        else {
            return Ok(None);
        };

        next_chapter_in_series(&mut db, current).await
    }

    /// Finds or creates a chapter for a manga, reconciling by number when needed.
    pub async fn ensure_chapter_for_manga(
        &self,
        manga_id: &str,
        chapter_source_id: &str,
        title: &str,
        chapter_number: f64,
        date_uploaded: &str,
    ) -> Result<ChapterRow> {
        if let Some(existing) = self
            .get_chapter_by_manga_and_source_id(manga_id, chapter_source_id)
            .await?
        {
            return Ok(existing);
        }

        if let Some(existing) = self
            .get_chapter_by_manga_and_number(manga_id, chapter_number)
            .await?
        {
            let write_guard = self.write_guard().await;
            let mut db = self.executor();
            let Some(mut chapter) =
                Chapter::filter(Chapter::fields().id().eq(existing.id.as_str()))
                    .first()
                    .exec(&mut db)
                    .await?
            else {
                return Err(anyhow!(
                    "chapter {} disappeared during reconciliation",
                    existing.id
                ));
            };

            chapter
                .update()
                .remote_chapter_id(chapter_source_id.to_string())
                .title(title.to_string())
                .number(encode_chapter_number(chapter_number))
                .published_at(date_uploaded.to_string())
                .exec(&mut db)
                .await?;

            drop(db);
            drop(write_guard);
            return self
                .get_chapter_by_id(&existing.id)
                .await?
                .ok_or_else(|| anyhow!("chapter was reconciled but could not be reloaded"));
        }

        let write_guard = self.write_guard().await;
        let mut db = self.executor();
        let created = Chapter::create()
            .series_id(manga_id.to_string())
            .remote_chapter_id(chapter_source_id.to_string())
            .title(title.to_string())
            .number(encode_chapter_number(chapter_number))
            .published_at(date_uploaded.to_string())
            .is_new(true)
            .pages_read(0)
            .read_completed(false)
            .last_read_at(None::<String>)
            .fetched_at(now_timestamp())
            .exec(&mut db)
            .await?;

        drop(db);
        drop(write_guard);
        self.get_chapter_by_id(&created.id)
            .await?
            .ok_or_else(|| anyhow!("chapter was inserted but could not be reloaded"))
    }

    /// Marks a chapter as no longer new after a download completes.
    pub async fn mark_chapter_downloaded(&self, chapter_id: &str) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut chapter) = Chapter::filter(Chapter::fields().id().eq(chapter_id))
            .first()
            .exec(&mut db)
            .await?
        else {
            return Ok(());
        };

        chapter.update().is_new(false).exec(&mut db).await?;
        Ok(())
    }

    /// Updates reader progress for a chapter and returns the refreshed row.
    pub async fn update_chapter_read_progress(
        &self,
        chapter_id: &str,
        page: usize,
        completed: bool,
    ) -> Result<Option<ChapterRow>> {
        let write_guard = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut chapter) = Chapter::filter(Chapter::fields().id().eq(chapter_id))
            .first()
            .exec(&mut db)
            .await?
        else {
            return Ok(None);
        };

        let page = i64::try_from(page)?;
        let should_clear = !completed && page == 0;
        let next_pages_read = if should_clear {
            0
        } else {
            chapter.pages_read.max(page)
        };
        let next_completed = if should_clear {
            false
        } else {
            chapter.read_completed || completed
        };
        let last_read_at = if next_pages_read > 0 || next_completed {
            Some(now_timestamp())
        } else {
            None
        };

        chapter
            .update()
            .pages_read(next_pages_read)
            .read_completed(next_completed)
            .last_read_at(last_read_at)
            .is_new(false)
            .exec(&mut db)
            .await?;

        drop(db);
        drop(write_guard);
        self.get_chapter_by_id(chapter_id).await
    }

    /// Clears reader progress for a chapter and returns the refreshed row.
    pub async fn clear_chapter_read_progress(
        &self,
        chapter_id: &str,
    ) -> Result<Option<ChapterRow>> {
        let write_guard = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut chapter) = Chapter::filter(Chapter::fields().id().eq(chapter_id))
            .first()
            .exec(&mut db)
            .await?
        else {
            return Ok(None);
        };

        chapter
            .update()
            .pages_read(0)
            .read_completed(false)
            .last_read_at(None::<String>)
            .exec(&mut db)
            .await?;

        drop(db);
        drop(write_guard);
        self.get_chapter_by_id(chapter_id).await
    }

    /// Marks existing chapters as baseline content for update tracking.
    pub async fn baseline_chapter_updates(&self, manga_id: &str) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let created_at = LibrarySeries::filter(LibrarySeries::fields().id().eq(manga_id))
            .first()
            .exec(&mut db)
            .await?
            .map_or_else(now_timestamp, |series| series.created_at);

        let chapters = Chapter::filter(Chapter::fields().series_id().eq(manga_id))
            .exec(&mut db)
            .await?;

        for mut chapter in chapters {
            chapter
                .update()
                .is_new(false)
                .fetched_at(created_at.clone())
                .exec(&mut db)
                .await?;
        }

        Ok(())
    }

    /// Lists chapters considered new relative to each series baseline.
    pub async fn get_library_updates(&self) -> Result<Vec<ChapterRow>> {
        let mut db = self.executor();
        let chapters = Chapter::all().exec(&mut db).await?;
        let chapter_ids = chapters
            .iter()
            .map(|chapter| chapter.id.clone())
            .collect::<Vec<_>>();
        let completed = completed_download_chapter_ids_for(&mut db, &chapter_ids).await?;

        let mut minimum_fetched_at = HashMap::<String, String>::new();
        for chapter in &chapters {
            minimum_fetched_at
                .entry(chapter.series_id.clone())
                .and_modify(|current| {
                    if chapter.fetched_at < *current {
                        current.clone_from(&chapter.fetched_at);
                    }
                })
                .or_insert_with(|| chapter.fetched_at.clone());
        }

        let mut updates = chapters
            .into_iter()
            .filter(|chapter| {
                minimum_fetched_at
                    .get(&chapter.series_id)
                    .is_some_and(|min| chapter.fetched_at > *min)
            })
            .collect::<Vec<_>>();

        updates.sort_by(|left, right| {
            right
                .fetched_at
                .cmp(&left.fetched_at)
                .then_with(|| {
                    decode_chapter_number(right.number)
                        .total_cmp(&decode_chapter_number(left.number))
                })
                .then_with(|| right.published_at.cmp(&left.published_at))
        });

        Ok(updates
            .into_iter()
            .map(|chapter| map_chapter_row(chapter, &completed))
            .collect())
    }
}

async fn next_chapter_in_series(
    db: &mut dyn toasty::Executor,
    current: Chapter,
) -> Result<Option<ChapterRow>> {
    let mut chapters =
        Chapter::filter(Chapter::fields().series_id().eq(current.series_id.as_str()))
            .exec(db)
            .await?;
    chapters.sort_by(|left, right| {
        decode_chapter_number(left.number)
            .total_cmp(&decode_chapter_number(right.number))
            .then_with(|| left.published_at.cmp(&right.published_at))
    });

    let Some(index) = chapters.iter().position(|chapter| chapter.id == current.id) else {
        return Ok(None);
    };
    let Some(next) = chapters.into_iter().nth(index + 1) else {
        return Ok(None);
    };

    let completed = completed_download_chapter_ids_for(db, std::slice::from_ref(&next.id)).await?;
    Ok(Some(map_chapter_row(next, &completed)))
}

async fn find_chapter_by_series_and_remote_id(
    db: &mut dyn toasty::Executor,
    manga_id: &str,
    chapter_source_id: &str,
) -> Result<Option<Chapter>> {
    Chapter::filter(
        Chapter::fields()
            .series_id()
            .eq(manga_id)
            .and(Chapter::fields().remote_chapter_id().eq(chapter_source_id)),
    )
    .first()
    .exec(db)
    .await
    .map_err(Into::into)
}

async fn completed_download_chapter_ids_for(
    db: &mut dyn toasty::Executor,
    chapter_ids: &[String],
) -> Result<HashSet<String>> {
    if chapter_ids.is_empty() {
        return Ok(HashSet::new());
    }

    let downloads = Download::filter(
        Download::fields().status().eq("completed").and(
            Download::fields()
                .chapter_id()
                .in_list(chapter_ids.to_vec()),
        ),
    )
    .exec(db)
    .await?;
    Ok(downloads
        .into_iter()
        .map(|download| download.chapter_id)
        .collect())
}

async fn reconcile_duplicate_chapters(db: &mut dyn toasty::Executor, manga_id: &str) -> Result<()> {
    let chapters = Chapter::filter(Chapter::fields().series_id().eq(manga_id))
        .exec(db)
        .await?;
    let downloads = Download::all().exec(db).await?;
    let has_downloads = downloads
        .into_iter()
        .map(|download| download.chapter_id)
        .collect::<HashSet<_>>();

    let mut groups = HashMap::<u64, Vec<Chapter>>::new();
    for chapter in chapters {
        groups
            .entry(decode_chapter_number(chapter.number).to_bits())
            .or_default()
            .push(chapter);
    }

    for group in groups.values_mut() {
        if group.len() < 2 {
            continue;
        }

        group.sort_by(|left, right| compare_canonical_rows(left, right, &has_downloads));
        let canonical_id = group[0].id.clone();
        let freshest = group
            .iter()
            .max_by(|left, right| compare_freshness(left, right))
            .cloned()
            .expect("duplicate group should not be empty");

        let Some(mut canonical) = Chapter::filter(Chapter::fields().id().eq(canonical_id.as_str()))
            .first()
            .exec(db)
            .await?
        else {
            continue;
        };

        canonical
            .update()
            .remote_chapter_id(freshest.remote_chapter_id.clone())
            .title(freshest.title.clone())
            .number(freshest.number)
            .published_at(freshest.published_at.clone())
            .is_new(freshest.is_new)
            .fetched_at(freshest.fetched_at.clone())
            .exec(db)
            .await?;

        for duplicate in &group[1..] {
            let downloads =
                Download::filter(Download::fields().chapter_id().eq(duplicate.id.as_str()))
                    .exec(db)
                    .await?;
            for mut download in downloads {
                download
                    .update()
                    .chapter_id(canonical_id.clone())
                    .exec(db)
                    .await?;
            }

            Chapter::filter(Chapter::fields().id().eq(duplicate.id.as_str()))
                .delete()
                .exec(db)
                .await?;
        }
    }

    Ok(())
}

fn compare_canonical_rows(
    left: &Chapter,
    right: &Chapter,
    has_downloads: &HashSet<String>,
) -> Ordering {
    has_downloads
        .contains(&right.id)
        .cmp(&has_downloads.contains(&left.id))
        .then_with(|| compare_freshness(right, left))
}

fn compare_freshness(left: &Chapter, right: &Chapter) -> Ordering {
    left.published_at
        .cmp(&right.published_at)
        .then_with(|| left.fetched_at.cmp(&right.fetched_at))
        .then_with(|| right.id.cmp(&left.id))
}

fn compare_chapter_listing(left: &Chapter, right: &Chapter) -> Ordering {
    decode_chapter_number(right.number)
        .total_cmp(&decode_chapter_number(left.number))
        .then_with(|| right.published_at.cmp(&left.published_at))
}

fn map_chapter_row(chapter: Chapter, completed_downloads: &HashSet<String>) -> ChapterRow {
    ChapterRow {
        id: chapter.id.clone(),
        manga_id: chapter.series_id,
        source_id: chapter.remote_chapter_id,
        title: chapter.title,
        chapter_number: decode_chapter_number(chapter.number),
        date_uploaded: chapter.published_at,
        fetched_at: chapter.fetched_at,
        downloaded: completed_downloads.contains(&chapter.id),
        is_new: chapter.is_new,
        pages_read: usize::try_from(chapter.pages_read).unwrap_or_default(),
        read_completed: chapter.read_completed,
        last_read_at: chapter.last_read_at,
    }
}
