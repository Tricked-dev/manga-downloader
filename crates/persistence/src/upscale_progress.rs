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

impl Database {
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
