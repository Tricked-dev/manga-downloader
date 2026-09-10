use std::collections::HashSet;

use anyhow::Result;

use crate::schema::{Download, DownloadEvent};
use crate::{Database, SqliteDatabaseOptions, open_configured_sqlite_connection};

#[derive(Debug, Default)]
pub struct DatabaseCleanupResult {
    pub orphaned_download_events: usize,
}

impl Database {
    /// Removes orphaned database rows and runs SQLite maintenance pragmas.
    pub async fn cleanup_database(&self) -> Result<DatabaseCleanupResult> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let mut result = DatabaseCleanupResult::default();

        let downloads = Download::all().exec(&mut db).await?;
        let download_ids = downloads
            .into_iter()
            .map(|download| download.id)
            .collect::<HashSet<_>>();

        let download_events = DownloadEvent::all().exec(&mut db).await?;
        let orphaned_event_ids = download_events
            .into_iter()
            .filter(|event| !download_ids.contains(&event.download_id))
            .map(|event| event.id)
            .collect::<Vec<_>>();

        result.orphaned_download_events = orphaned_event_ids.len();
        for event_id in orphaned_event_ids {
            DownloadEvent::filter(DownloadEvent::fields().id().eq(event_id.as_str()))
                .delete()
                .exec(&mut db)
                .await?;
        }

        run_sqlite_maintenance(self.path())?;
        Ok(result)
    }
}

fn run_sqlite_maintenance(path: &str) -> Result<()> {
    let connection = open_configured_sqlite_connection(path, SqliteDatabaseOptions::default())?;
    connection.execute_batch("PRAGMA optimize; PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
}
