use anyhow::{Context, Result};

use crate::{Database, SqliteDatabaseOptions, now_timestamp, open_configured_sqlite_connection};

#[derive(Debug, Clone)]
pub struct ArchiveIndexIdentity {
    pub archive_path: String,
    pub archive_size: i64,
    pub archive_mtime_ms: i64,
    pub schema_version: i64,
}

#[derive(Debug, Clone)]
pub struct ArchiveIndexRecord {
    pub identity: ArchiveIndexIdentity,
    pub page_count: i64,
    pub index_postcard: Vec<u8>,
    pub indexed_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, utoipa::ToSchema)]
pub struct ArchiveIndexStats {
    pub indexed_archives: usize,
    pub indexed_pages: usize,
    pub index_blob_bytes: usize,
}

#[derive(Debug, Clone, Default, serde::Serialize, utoipa::ToSchema)]
pub struct ArchiveIndexCleanupResult {
    pub removed_rows: usize,
}

impl Database {
    /// Looks up an archive index record that exactly matches the supplied identity.
    ///
    /// A successful hit updates `last_used_at`.
    pub async fn get_fresh_archive_index(
        &self,
        identity: &ArchiveIndexIdentity,
    ) -> Result<Option<ArchiveIndexRecord>> {
        let path = self.path().to_string();
        let identity = identity.clone();
        tokio::task::spawn_blocking(move || -> Result<Option<ArchiveIndexRecord>> {
            let connection = open_configured_sqlite_connection(
                &path,
                SqliteDatabaseOptions::default(),
            )?;
            let mut statement = connection.prepare(
                "SELECT archive_path, archive_size, archive_mtime_ms, schema_version, page_count, index_postcard, indexed_at, last_used_at
                 FROM downloaded_archive_index
                 WHERE archive_path = ?1
                   AND archive_size = ?2
                   AND archive_mtime_ms = ?3
                   AND schema_version = ?4",
            )?;
            let mut rows = statement.query((
                identity.archive_path.as_str(),
                identity.archive_size,
                identity.archive_mtime_ms,
                identity.schema_version,
            ))?;
            let Some(row) = rows.next()? else {
                return Ok(None);
            };
            let record = archive_index_record_from_row(row)?;
            drop(rows);
            drop(statement);
            connection.execute(
                "UPDATE downloaded_archive_index SET last_used_at = ?1 WHERE archive_path = ?2",
                (now_timestamp(), identity.archive_path.as_str()),
            )?;
            Ok(Some(record))
        })
        .await
        .context("archive index lookup task failed")?
    }

    /// Inserts or replaces the persisted page index for an archive identity.
    pub async fn upsert_archive_index(
        &self,
        identity: &ArchiveIndexIdentity,
        page_count: usize,
        index_postcard: &[u8],
    ) -> Result<()> {
        let _write = self.write_guard().await;
        let path = self.path().to_string();
        let identity = identity.clone();
        let index_postcard = index_postcard.to_vec();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let connection =
                open_configured_sqlite_connection(&path, SqliteDatabaseOptions::default())?;
            connection.execute(
                "INSERT INTO downloaded_archive_index (
                    archive_path,
                    archive_size,
                    archive_mtime_ms,
                    schema_version,
                    page_count,
                    index_postcard,
                    indexed_at,
                    last_used_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                ON CONFLICT(archive_path) DO UPDATE SET
                    archive_size = excluded.archive_size,
                    archive_mtime_ms = excluded.archive_mtime_ms,
                    schema_version = excluded.schema_version,
                    page_count = excluded.page_count,
                    index_postcard = excluded.index_postcard,
                    indexed_at = excluded.indexed_at,
                    last_used_at = excluded.last_used_at",
                (
                    identity.archive_path.as_str(),
                    identity.archive_size,
                    identity.archive_mtime_ms,
                    identity.schema_version,
                    i64::try_from(page_count).unwrap_or(i64::MAX),
                    index_postcard,
                    now_timestamp(),
                ),
            )?;
            Ok(())
        })
        .await
        .context("archive index upsert task failed")?
    }

    /// Returns aggregate archive-index row, page, and blob-size statistics.
    pub async fn archive_index_stats(&self) -> Result<ArchiveIndexStats> {
        let path = self.path().to_string();
        tokio::task::spawn_blocking(move || -> Result<ArchiveIndexStats> {
            let connection = open_configured_sqlite_connection(
                &path,
                SqliteDatabaseOptions::default(),
            )?;
            let (indexed_archives, indexed_pages, index_blob_bytes) = connection.query_row(
                "SELECT COUNT(*), COALESCE(SUM(page_count), 0), COALESCE(SUM(length(index_postcard)), 0)
                 FROM downloaded_archive_index",
                (),
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )?;
            Ok(ArchiveIndexStats {
                indexed_archives: usize::try_from(indexed_archives).unwrap_or(usize::MAX),
                indexed_pages: usize::try_from(indexed_pages).unwrap_or(usize::MAX),
                index_blob_bytes: usize::try_from(index_blob_bytes).unwrap_or(usize::MAX),
            })
        })
        .await
        .context("archive index stats task failed")?
    }

    /// Lists all persisted archive index records.
    pub async fn list_archive_index_records(&self) -> Result<Vec<ArchiveIndexRecord>> {
        let path = self.path().to_string();
        tokio::task::spawn_blocking(move || -> Result<Vec<ArchiveIndexRecord>> {
            let connection = open_configured_sqlite_connection(
                &path,
                SqliteDatabaseOptions::default(),
            )?;
            let mut statement = connection.prepare(
                "SELECT archive_path, archive_size, archive_mtime_ms, schema_version, page_count, index_postcard, indexed_at, last_used_at
                 FROM downloaded_archive_index",
            )?;
            let rows = statement.query_map((), archive_index_record_from_row)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
        .await
        .context("archive index list task failed")?
    }

    /// Deletes every persisted archive index record.
    pub async fn clear_archive_indexes(&self) -> Result<ArchiveIndexCleanupResult> {
        let _write = self.write_guard().await;
        let path = self.path().to_string();
        tokio::task::spawn_blocking(move || -> Result<ArchiveIndexCleanupResult> {
            let connection =
                open_configured_sqlite_connection(&path, SqliteDatabaseOptions::default())?;
            let removed_rows = connection.execute("DELETE FROM downloaded_archive_index", ())?;
            Ok(ArchiveIndexCleanupResult { removed_rows })
        })
        .await
        .context("archive index clear task failed")?
    }

    /// Deletes archive index records whose archive paths match `archive_paths`.
    pub async fn delete_archive_indexes_by_path(
        &self,
        archive_paths: &[String],
    ) -> Result<ArchiveIndexCleanupResult> {
        if archive_paths.is_empty() {
            return Ok(ArchiveIndexCleanupResult::default());
        }

        let _write = self.write_guard().await;
        let path = self.path().to_string();
        let archive_paths = archive_paths.to_vec();
        tokio::task::spawn_blocking(move || -> Result<ArchiveIndexCleanupResult> {
            let mut connection =
                open_configured_sqlite_connection(&path, SqliteDatabaseOptions::default())?;
            let tx = connection.transaction()?;
            let mut removed_rows = 0usize;
            {
                let mut statement =
                    tx.prepare("DELETE FROM downloaded_archive_index WHERE archive_path = ?1")?;
                for archive_path in archive_paths {
                    removed_rows += statement.execute((archive_path,))?;
                }
            }
            tx.commit()?;
            Ok(ArchiveIndexCleanupResult { removed_rows })
        })
        .await
        .context("archive index delete task failed")?
    }
}

fn archive_index_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArchiveIndexRecord> {
    Ok(ArchiveIndexRecord {
        identity: ArchiveIndexIdentity {
            archive_path: row.get(0)?,
            archive_size: row.get(1)?,
            archive_mtime_ms: row.get(2)?,
            schema_version: row.get(3)?,
        },
        page_count: row.get(4)?,
        index_postcard: row.get(5)?,
        indexed_at: row.get(6)?,
        last_used_at: row.get(7)?,
    })
}
