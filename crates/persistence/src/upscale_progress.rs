use crate::{Database, now_timestamp, schema::UpscaleProgress};
use anyhow::Result;

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct UpscaleProgressRow {
    pub status: String,
    pub completed_pages: i64,
    pub total_pages: i64,
    pub message: String,
    pub updated_at: String,
}

/// One queue entry: the progress row plus the download it belongs to, so a queue view
/// does not have to fetch every download to show a title.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct UpscaleQueueEntryRow {
    pub download_id: String,
    pub manga_id: String,
    pub manga_title: String,
    pub chapter_number: f64,
    pub chapter_title: String,
    pub status: String,
    pub completed_pages: i64,
    pub total_pages: i64,
    pub message: String,
    pub updated_at: String,
}

impl Database {
    /// Progress rows outlive their job, so the caller decides which statuses still count
    /// as queue work. Rows whose download has been deleted are dropped.
    pub async fn list_upscale_queue(&self) -> Result<Vec<UpscaleQueueEntryRow>> {
        let mut db = self.executor();
        let progress = UpscaleProgress::all().exec(&mut db).await?;
        let downloads = self.get_downloads().await?;
        let mut rows = Vec::new();
        for row in progress {
            let Some(download) = downloads
                .iter()
                .find(|download| download.id == row.download_id)
            else {
                continue;
            };
            rows.push(UpscaleQueueEntryRow {
                download_id: row.download_id,
                manga_id: download.manga_id.clone(),
                manga_title: download.manga_title.clone(),
                chapter_number: download.chapter_number,
                chapter_title: download.chapter_title.clone(),
                status: row.status,
                completed_pages: row.completed_pages,
                total_pages: row.total_pages,
                message: row.message,
                updated_at: row.updated_at,
            });
        }
        rows.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(rows)
    }

    pub async fn get_upscale_progress(&self, id: &str) -> Result<Option<UpscaleProgressRow>> {
        let mut db = self.executor();
        Ok(
            UpscaleProgress::filter(UpscaleProgress::fields().download_id().eq(id))
                .first()
                .exec(&mut db)
                .await?
                .map(|row| UpscaleProgressRow {
                    status: row.status,
                    completed_pages: row.completed_pages,
                    total_pages: row.total_pages,
                    message: row.message,
                    updated_at: row.updated_at,
                }),
        )
    }

    /// A chapter whose task is queued again should read as waiting, not as a failure an
    /// operator has to retry by hand.
    pub async fn queue_requeued_upscale_progress(&self, download_ids: &[String]) -> Result<()> {
        if download_ids.is_empty() {
            return Ok(());
        }

        let _write = self.write_guard().await;
        let mut db = self.executor();
        let sql = match self.backend() {
            crate::DatabaseBackend::Sqlite => {
                "UPDATE upscale_progress SET status = 'queued', message = ?1, updated_at = ?2 WHERE download_id = ?3"
            }
            crate::DatabaseBackend::Postgres => {
                "UPDATE upscale_progress SET status = 'queued', message = $1, updated_at = $2 WHERE download_id = $3"
            }
        };
        for download_id in download_ids {
            toasty::sql::statement(sql)
                .bind("Requeued after a server restart")
                .bind(now_timestamp())
                .bind(download_id.as_str())
                .exec(&mut db)
                .await?;
        }
        Ok(())
    }

    /// Nothing can be mid-upscale while the server is starting, so a row left claiming it is
    /// a job the previous process took down with it. Saying so lets the queue view offer a
    /// retry instead of showing work that will never move; a job that is re-queued overwrites
    /// this as soon as the worker picks it up again.
    pub async fn fail_interrupted_upscale_progress(&self) -> Result<u64> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let sql = match self.backend() {
            crate::DatabaseBackend::Sqlite => {
                "UPDATE upscale_progress SET status = 'failed', message = ?1, updated_at = ?2 WHERE status IN ('running', 'paused')"
            }
            crate::DatabaseBackend::Postgres => {
                "UPDATE upscale_progress SET status = 'failed', message = $1, updated_at = $2 WHERE status IN ('running', 'paused')"
            }
        };
        let rows = toasty::sql::statement(sql)
            .bind("Interrupted by a server restart")
            .bind(now_timestamp())
            .exec(&mut db)
            .await?;
        Ok(rows)
    }

    pub async fn set_upscale_progress(
        &self,
        id: &str,
        status: &str,
        completed: usize,
        total: usize,
        message: &str,
    ) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let sql = match self.backend() {
            crate::DatabaseBackend::Sqlite => {
                "INSERT INTO upscale_progress VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (download_id) DO UPDATE SET status=excluded.status, completed_pages=excluded.completed_pages, total_pages=excluded.total_pages, message=excluded.message, updated_at=excluded.updated_at"
            }
            crate::DatabaseBackend::Postgres => {
                "INSERT INTO upscale_progress VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (download_id) DO UPDATE SET status=excluded.status, completed_pages=excluded.completed_pages, total_pages=excluded.total_pages, message=excluded.message, updated_at=excluded.updated_at"
            }
        };
        toasty::sql::statement(sql)
            .bind(id)
            .bind(status)
            .bind(completed as i64)
            .bind(total as i64)
            .bind(message)
            .bind(now_timestamp())
            .exec(&mut db)
            .await?;
        Ok(())
    }
}
