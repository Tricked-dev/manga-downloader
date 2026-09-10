#![allow(clippy::missing_errors_doc)]

mod background_jobs;
mod chapters;
mod download_work_state;
mod downloads;
mod maintenance;
mod manga;
mod migrations;
mod models;
mod readonly;
mod schema;
mod settings;
mod setup;
mod sources;
mod stats;

use anyhow::Result;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use toasty::Db;
use tokio::sync::{Mutex, MutexGuard};

use backend_core::settings::SETTING_DEFINITIONS;

pub use backend_core::settings::setting_default;
pub use background_jobs::{BackgroundJobFailureOutcome, BackgroundJobLease, BackgroundJobStatus};
pub use download_work_state::{DownloadWorkStatus, DownloadWorkTransition};
pub use maintenance::DatabaseCleanupResult;
pub use models::{
    ChapterInsert, ChapterRow, DownloadMetricRow, DownloadRow, MangaInsert, MangaRow,
    SourceCountRow, SourceRecordInput, StatsActivityPoint, StatsCacheSummary, StatsOverview,
    StatsRecentChapter, StatsSourceBreakdown, StatsStorageSummary, StatsTotals,
};
pub use readonly::ReadOnlyDatabase;
pub use setup::{
    DatabaseMigration, SqliteDatabaseOptions, apply_sqlite_migrations, configure_sqlite_connection,
    database_parent_dir, ensure_database_parent_dir, open_configured_sqlite_connection,
    open_sqlite_database, setup_sqlite_database, sqlite_read_only_immutable_uri,
};

/// Applies the bundled database migrations to `path`.
pub async fn migrate_database(path: &str) -> Result<()> {
    setup_sqlite_database(
        path,
        migrations::MIGRATIONS,
        SqliteDatabaseOptions::default(),
    )
    .await
}

#[derive(Clone)]
pub struct Database {
    db: Db,
    path: Arc<str>,
    write_lock: Arc<Mutex<()>>,
}

impl Database {
    /// Opens the application database, applies migrations, and seeds default settings.
    pub async fn new(path: &str) -> Result<Self> {
        let db = open_sqlite_database(
            path,
            toasty::models!(crate::*),
            migrations::MIGRATIONS,
            SqliteDatabaseOptions::default(),
        )
        .await?;

        let database = Self {
            db,
            path: Arc::from(path),
            write_lock: Arc::new(Mutex::new(())),
        };
        database.seed_default_settings().await?;
        Ok(database)
    }

    /// Applies non-empty environment variable overrides for known settings.
    pub async fn apply_env_overrides(&self) -> Result<()> {
        for definition in SETTING_DEFINITIONS {
            if let Ok(value) = std::env::var(definition.env_var) {
                if value.trim().is_empty() {
                    continue;
                }
                self.set_setting(definition.key, &value).await?;
            }
        }

        Ok(())
    }

    pub(crate) fn executor(&self) -> Db {
        self.db.clone()
    }

    pub(crate) async fn write_guard(&self) -> MutexGuard<'_, ()> {
        self.write_lock.lock().await
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }
}

pub(crate) fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

pub(crate) fn now_timestamp() -> String {
    format!(
        "{:020}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    )
}

pub(crate) fn split_csv(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

const DECIMAL_SCALE: f64 = 1_000_000.0;
const PROGRESS_SCALE: f64 = 1_000.0;

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn encode_chapter_number(value: f64) -> i64 {
    (value * DECIMAL_SCALE).round() as i64
}

#[allow(clippy::cast_precision_loss)]
pub(crate) fn decode_chapter_number(value: i64) -> f64 {
    value as f64 / DECIMAL_SCALE
}

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn encode_progress(value: f64) -> i64 {
    (value * PROGRESS_SCALE).round() as i64
}

#[allow(clippy::cast_precision_loss)]
pub(crate) fn decode_progress(value: i64) -> f64 {
    value as f64 / PROGRESS_SCALE
}
