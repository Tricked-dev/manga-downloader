use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::collections::HashSet;

use crate::downloads::external_download_status;
use crate::{
    ChapterRow, DownloadRow, MangaRow, decode_chapter_number, decode_progress,
    sqlite_read_only_immutable_uri,
};

pub struct ReadOnlyDatabase {
    uri: String,
}

impl ReadOnlyDatabase {
    /// Opens a read-only immutable view of a backend SQLite database.
    pub fn open(path: &str) -> Result<Self> {
        let uri = sqlite_read_only_immutable_uri(path);
        let _ = Connection::open_with_flags(
            &uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .with_context(|| format!("failed to open backend database read-only at {path}"))?;
        Ok(Self { uri })
    }

    /// Reads one application setting from the read-only database.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    /// Lists library manga from the read-only database.
    pub fn get_library_manga(&self) -> Result<Vec<MangaRow>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT
                ls.id, COALESCE(s.key, ls.source_key), COALESCE(s.base_url, ''),
                ls.remote_series_id, ls.title,
                ls.cover_url, ls.cover_fetch_spec, ls.description, ls.author, ls.genres,
                ls.status, ls.category, ls.is_nsfw, ls.auto_download_new,
                ls.updated_at, ls.chapters_initialized, ls.language,
                COUNT(DISTINCT c.id) AS total_chapters,
                COUNT(DISTINCT cd.chapter_id) AS downloaded_chapters
             FROM library_series ls
             LEFT JOIN sources s ON s.key = ls.source_key
             LEFT JOIN chapters c ON c.series_id = ls.id
             LEFT JOIN downloads cd ON cd.chapter_id = c.id AND cd.status = 'completed'
             GROUP BY ls.id
             ORDER BY ls.title COLLATE NOCASE ASC",
        )?;

        stmt.query_map([], |row| {
            let genres_json: String = row.get(9)?;
            Ok(MangaRow {
                id: row.get(0)?,
                source: row.get(1)?,
                source_base_url: row.get(2)?,
                source_id: row.get(3)?,
                title: row.get(4)?,
                cover_url: row.get(5)?,
                cover_fetch_spec: row.get(6)?,
                description: row.get(7)?,
                author: row.get(8)?,
                genres: parse_json_string_list(&genres_json).join(", "),
                status: row.get(10)?,
                category: row.get(11)?,
                is_nsfw: row.get::<_, i64>(12)? != 0,
                auto_download: row.get(13)?,
                total_chapters: i64_to_usize(row.get(17)?),
                downloaded_chapters: i64_to_usize(row.get(18)?),
                last_updated: row.get(14)?,
                chapters_initialized: row.get::<_, i64>(15)? != 0,
                language: row.get(16)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
    }

    /// Lists chapters for one manga from the read-only database.
    pub fn get_chapters(&self, manga_id: &str) -> Result<Vec<ChapterRow>> {
        let conn = self.connection()?;
        let completed = completed_chapter_ids(&conn)?;
        let mut stmt = conn.prepare(
            "SELECT
                id, series_id, remote_chapter_id, title, number, published_at, fetched_at, is_new,
                pages_read, read_completed, last_read_at
             FROM chapters
             WHERE series_id = ?1
             ORDER BY number ASC, published_at ASC, id ASC",
        )?;
        stmt.query_map(params![manga_id], |row| {
            let id: String = row.get(0)?;
            Ok(ChapterRow {
                id: id.clone(),
                manga_id: row.get(1)?,
                source_id: row.get(2)?,
                title: row.get(3)?,
                chapter_number: decode_chapter_number(row.get(4)?),
                date_uploaded: row.get(5)?,
                fetched_at: row.get(6)?,
                downloaded: completed.contains(&id),
                is_new: row.get::<_, i64>(7)? != 0,
                pages_read: i64_to_usize(row.get(8)?),
                read_completed: row.get::<_, i64>(9)? != 0,
                last_read_at: row.get(10)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
    }

    /// Lists downloads from the read-only database.
    pub fn get_downloads(&self) -> Result<Vec<DownloadRow>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT
                d.id, d.chapter_id, d.series_id, d.status, d.progress_percent, d.error_message,
                c.title, c.number, ls.title, c.remote_chapter_id,
                COALESCE(s.key, ls.source_key), ls.remote_series_id
             FROM downloads d
             JOIN chapters c ON c.id = d.chapter_id
             JOIN library_series ls ON ls.id = d.series_id
             LEFT JOIN sources s ON s.key = ls.source_key
             ORDER BY d.queued_at DESC, d.id DESC",
        )?;
        stmt.query_map([], |row| {
            let status: String = row.get(3)?;
            Ok(DownloadRow {
                id: row.get(0)?,
                chapter_id: row.get(1)?,
                manga_id: row.get(2)?,
                status: external_download_status(&status),
                progress: decode_progress(row.get(4)?),
                error: row.get(5)?,
                chapter_title: row.get(6)?,
                chapter_number: decode_chapter_number(row.get(7)?),
                manga_title: row.get(8)?,
                chapter_source_id: row.get(9)?,
                manga_source: row.get(10)?,
                manga_source_id: row.get(11)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
    }

    fn connection(&self) -> Result<Connection> {
        Connection::open_with_flags(
            &self.uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .with_context(|| format!("failed to open backend database read-only at {}", self.uri))
    }
}

fn completed_chapter_ids(conn: &Connection) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT chapter_id FROM downloads WHERE status = 'completed'")?;
    stmt.query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<HashSet<_>, _>>()
        .map_err(Into::into)
}

fn parse_json_string_list(value: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(value).unwrap_or_default()
}

fn i64_to_usize(value: i64) -> usize {
    usize::try_from(value).unwrap_or_default()
}
