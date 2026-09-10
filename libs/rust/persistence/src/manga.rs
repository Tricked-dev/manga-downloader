use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};

use crate::schema::{Chapter, Download, LibrarySeries, Source};
use crate::{Database, MangaInsert, MangaRow, SourceCountRow, split_csv};

impl Database {
    /// Inserts a manga into the library or updates the existing source/id row.
    ///
    /// Returns the library series id.
    pub async fn add_manga_to_library(&self, manga: &MangaInsert<'_>) -> Result<String> {
        let source_key = self.ensure_source_stub(manga.source, None).await?;
        let genres = split_csv(manga.genres);

        let _write = self.write_guard().await;
        let mut db = self.executor();
        let existing = LibrarySeries::filter(
            LibrarySeries::fields()
                .source_key()
                .eq(source_key.as_str())
                .and(
                    LibrarySeries::fields()
                        .remote_series_id()
                        .eq(manga.source_id),
                ),
        )
        .first()
        .exec(&mut db)
        .await?;

        if let Some(mut existing) = existing {
            let id = existing.id.clone();
            existing
                .update()
                .title(manga.title.to_string())
                .description(manga.description.to_string())
                .author(manga.author.to_string())
                .status(manga.status.to_string())
                .cover_url(manga.cover_url.to_string())
                .cover_fetch_spec(manga.cover_fetch_spec.map(ToOwned::to_owned))
                .category(manga.category.to_string())
                .is_nsfw(manga.is_nsfw)
                .language(manga.language.map(ToOwned::to_owned))
                .genres(genres)
                .exec(&mut db)
                .await?;
            Ok(id)
        } else {
            let series = LibrarySeries::create()
                .source_key(source_key)
                .remote_series_id(manga.source_id.to_string())
                .title(manga.title.to_string())
                .cover_url(manga.cover_url.to_string())
                .cover_fetch_spec(manga.cover_fetch_spec.map(ToOwned::to_owned))
                .description(manga.description.to_string())
                .author(manga.author.to_string())
                .genres(genres)
                .status(manga.status.to_string())
                .category(manga.category.to_string())
                .is_nsfw(manga.is_nsfw)
                .auto_download_new(None::<bool>)
                .language(manga.language.map(ToOwned::to_owned))
                .chapters_initialized(false)
                .exec(&mut db)
                .await?;
            Ok(series.id)
        }
    }

    /// Lists library manga sorted by title.
    pub async fn get_library_manga(&self) -> Result<Vec<MangaRow>> {
        let mut db = self.executor();
        let series = LibrarySeries::all()
            .order_by(LibrarySeries::fields().title().asc())
            .exec(&mut db)
            .await?;
        build_manga_rows(&mut db, series).await
    }

    /// Counts library manga by source key.
    pub async fn get_library_source_counts(&self) -> Result<Vec<SourceCountRow>> {
        let mut db = self.executor();
        let series = LibrarySeries::all().exec(&mut db).await?;
        let mut counts = HashMap::<String, u64>::new();

        for row in series {
            *counts.entry(row.source_key).or_default() += 1;
        }

        Ok(counts
            .into_iter()
            .map(|(source, count)| SourceCountRow { source, count })
            .collect())
    }

    /// Removes a manga and its chapters/download records from the library.
    pub async fn remove_from_library(&self, id: &str) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let mut tx = db.transaction().await?;

        let chapter_ids = Chapter::filter(Chapter::fields().series_id().eq(id))
            .exec(&mut tx)
            .await?
            .into_iter()
            .map(|chapter| chapter.id)
            .collect::<Vec<_>>();

        for chapter_id in &chapter_ids {
            self.delete_downloads_for_chapter(&mut tx, chapter_id)
                .await?;
        }

        Chapter::filter(Chapter::fields().series_id().eq(id))
            .delete()
            .exec(&mut tx)
            .await?;

        LibrarySeries::filter(LibrarySeries::fields().id().eq(id))
            .delete()
            .exec(&mut tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Updates the category assigned to one library manga.
    pub async fn update_manga_category(&self, id: &str, category: &str) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut manga) = find_series_by_id(&mut db, id).await? else {
            return Err(anyhow!("manga {id} not found"));
        };

        manga
            .update()
            .category(category.to_string())
            .exec(&mut db)
            .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    /// Updates source metadata for one library manga row.
    pub async fn update_manga_metadata(
        &self,
        id: &str,
        title: &str,
        cover_url: &str,
        cover_fetch_spec: Option<&str>,
        description: &str,
        author: &str,
        genres: &str,
        status: &str,
        is_nsfw: bool,
        language: &str,
    ) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut manga) = find_series_by_id(&mut db, id).await? else {
            return Err(anyhow!("manga {id} not found"));
        };

        manga
            .update()
            .title(title.to_string())
            .cover_url(cover_url.to_string())
            .cover_fetch_spec(cover_fetch_spec.map(ToOwned::to_owned))
            .description(description.to_string())
            .author(author.to_string())
            .genres(split_csv(genres))
            .status(status.to_string())
            .is_nsfw(is_nsfw)
            .language(Some(language.to_string()))
            .exec(&mut db)
            .await?;

        Ok(())
    }

    /// Marks a library manga as having completed its initial chapter load.
    pub async fn mark_chapters_initialized(&self, id: &str) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut manga) = find_series_by_id(&mut db, id).await? else {
            return Err(anyhow!("manga {id} not found"));
        };

        manga
            .update()
            .chapters_initialized(true)
            .exec(&mut db)
            .await?;
        Ok(())
    }

    /// Looks up library manga by local series id.
    pub async fn get_library_manga_by_id(&self, id: &str) -> Result<Option<MangaRow>> {
        let mut db = self.executor();
        let Some(series) = find_series_by_id(&mut db, id).await? else {
            return Ok(None);
        };

        let mut rows = build_manga_rows(&mut db, vec![series]).await?;
        Ok(rows.pop())
    }

    /// Looks up library manga by source key and remote source id.
    pub async fn get_manga_by_source(
        &self,
        source: &str,
        source_id: &str,
    ) -> Result<Option<MangaRow>> {
        let mut db = self.executor();
        let row = LibrarySeries::filter(
            LibrarySeries::fields()
                .source_key()
                .eq(source)
                .and(LibrarySeries::fields().remote_series_id().eq(source_id)),
        )
        .first()
        .exec(&mut db)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let mut rows = build_manga_rows(&mut db, vec![row]).await?;
        Ok(rows.pop())
    }

    /// Looks up library manga by local series id.
    pub async fn get_manga_by_id(&self, id: &str) -> Result<Option<MangaRow>> {
        self.get_library_manga_by_id(id).await
    }

    /// Looks up the first library manga row with a matching remote source id.
    pub async fn get_manga_by_source_id(&self, source_id: &str) -> Result<Option<MangaRow>> {
        let mut db = self.executor();
        let row = LibrarySeries::filter(LibrarySeries::fields().remote_series_id().eq(source_id))
            .first()
            .exec(&mut db)
            .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let mut rows = build_manga_rows(&mut db, vec![row]).await?;
        Ok(rows.pop())
    }
}

async fn find_series_by_id(db: &mut toasty::Db, id: &str) -> Result<Option<LibrarySeries>> {
    LibrarySeries::filter(LibrarySeries::fields().id().eq(id))
        .first()
        .exec(db)
        .await
        .map_err(Into::into)
}

async fn build_manga_rows(
    db: &mut toasty::Db,
    series: Vec<LibrarySeries>,
) -> Result<Vec<MangaRow>> {
    if series.is_empty() {
        return Ok(Vec::new());
    }

    let series_ids = series
        .iter()
        .map(|row| row.id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let source_keys = series
        .iter()
        .map(|row| row.source_key.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let sources = Source::filter(Source::fields().key().in_list(source_keys))
        .exec(db)
        .await?;
    let chapters = Chapter::filter(Chapter::fields().series_id().in_list(series_ids))
        .exec(db)
        .await?;

    let source_map = sources
        .into_iter()
        .map(|source| (source.key.clone(), source))
        .collect::<HashMap<_, _>>();

    let mut chapter_counts = HashMap::<String, usize>::new();
    let mut chapter_series = HashMap::<String, String>::new();
    for chapter in chapters {
        *chapter_counts.entry(chapter.series_id.clone()).or_default() += 1;
        chapter_series.insert(chapter.id, chapter.series_id);
    }

    let chapter_ids = chapter_series.keys().cloned().collect::<Vec<_>>();
    let downloads = if chapter_ids.is_empty() {
        Vec::new()
    } else {
        Download::filter(
            Download::fields()
                .status()
                .eq("completed")
                .and(Download::fields().chapter_id().in_list(chapter_ids)),
        )
        .exec(db)
        .await?
    };

    let mut downloaded_chapters = HashMap::<String, HashSet<String>>::new();
    for download in downloads {
        if let Some(series_id) = chapter_series.get(&download.chapter_id) {
            downloaded_chapters
                .entry(series_id.clone())
                .or_default()
                .insert(download.chapter_id);
        }
    }

    series
        .into_iter()
        .map(|series| {
            let source = source_map.get(&series.source_key).ok_or_else(|| {
                anyhow!(
                    "source {} missing for series {}",
                    series.source_key,
                    series.id
                )
            })?;

            Ok(MangaRow {
                id: series.id.clone(),
                source: source.key.clone(),
                source_base_url: source.base_url.clone(),
                source_id: series.remote_series_id,
                title: series.title,
                cover_url: series.cover_url,
                cover_fetch_spec: series.cover_fetch_spec,
                description: series.description,
                author: series.author,
                genres: series.genres.join(", "),
                status: series.status,
                category: series.category,
                is_nsfw: series.is_nsfw,
                auto_download: series.auto_download_new,
                total_chapters: chapter_counts.get(&series.id).copied().unwrap_or_default(),
                downloaded_chapters: downloaded_chapters
                    .get(&series.id)
                    .map(HashSet::len)
                    .unwrap_or_default(),
                last_updated: series.updated_at,
                chapters_initialized: series.chapters_initialized,
                language: series.language,
            })
        })
        .collect()
}
