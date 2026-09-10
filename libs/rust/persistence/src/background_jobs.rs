use anyhow::{Result, anyhow};

use crate::{Database, now_timestamp, schema::BackgroundJob};

const MAX_ERROR_LENGTH: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl BackgroundJobStatus {
    #[must_use]
    pub const fn persisted_value(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundJobLease {
    pub id: String,
    pub payload_json: String,
    pub attempt_count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackgroundJobFailureOutcome {
    pub retrying: bool,
    pub attempt_count: i64,
    pub max_attempts: i64,
}

impl Database {
    pub async fn enqueue_background_job(
        &self,
        queue: &str,
        payload_json: String,
        max_attempts: i64,
    ) -> Result<String> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let now = now_timestamp();
        let job = BackgroundJob::create()
            .queue(queue.to_string())
            .payload_json(payload_json)
            .status(BackgroundJobStatus::Pending.persisted_value().to_string())
            .attempt_count(0)
            .max_attempts(max_attempts.max(1))
            .run_after(now)
            .locked_by(None::<String>)
            .locked_at(None::<String>)
            .last_error(None::<String>)
            .exec(&mut db)
            .await?;

        Ok(job.id)
    }

    pub async fn lease_next_background_job(
        &self,
        queue: &str,
        worker_id: &str,
    ) -> Result<Option<BackgroundJobLease>> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let mut tx = db.transaction().await?;
        let now = now_timestamp();

        let Some(mut job) = BackgroundJob::filter(
            BackgroundJob::fields().queue().eq(queue).and(
                BackgroundJob::fields()
                    .status()
                    .eq(BackgroundJobStatus::Pending.persisted_value()),
            ),
        )
        .order_by(BackgroundJob::fields().run_after().asc())
        .order_by(BackgroundJob::fields().created_at().asc())
        .exec(&mut tx)
        .await?
        .into_iter()
        .find(|job| job.run_after.as_str() <= now.as_str()) else {
            tx.commit().await?;
            return Ok(None);
        };

        let id = job.id.clone();
        let payload_json = job.payload_json.clone();
        let attempt_count = job.attempt_count + 1;
        job.update()
            .status(BackgroundJobStatus::Running.persisted_value().to_string())
            .attempt_count(attempt_count)
            .locked_by(Some(worker_id.to_string()))
            .locked_at(Some(now.clone()))
            .last_error(None::<String>)
            .updated_at(now)
            .exec(&mut tx)
            .await?;
        tx.commit().await?;

        Ok(Some(BackgroundJobLease {
            id,
            payload_json,
            attempt_count,
        }))
    }

    pub async fn complete_background_job(&self, id: &str) -> Result<()> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut job) = BackgroundJob::filter(BackgroundJob::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?
        else {
            return Err(anyhow!("background job {id} not found"));
        };

        let now = now_timestamp();
        job.update()
            .status(BackgroundJobStatus::Completed.persisted_value().to_string())
            .locked_by(None::<String>)
            .locked_at(None::<String>)
            .last_error(None::<String>)
            .updated_at(now)
            .exec(&mut db)
            .await?;

        Ok(())
    }

    pub async fn fail_background_job(
        &self,
        id: &str,
        error: &str,
    ) -> Result<BackgroundJobFailureOutcome> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let Some(mut job) = BackgroundJob::filter(BackgroundJob::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?
        else {
            return Err(anyhow!("background job {id} not found"));
        };

        let retrying = job.attempt_count < job.max_attempts;
        let status = if retrying {
            BackgroundJobStatus::Pending
        } else {
            BackgroundJobStatus::Failed
        };
        let now = now_timestamp();
        let attempt_count = job.attempt_count;
        let max_attempts = job.max_attempts;
        job.update()
            .status(status.persisted_value().to_string())
            .run_after(now.clone())
            .locked_by(None::<String>)
            .locked_at(None::<String>)
            .last_error(Some(truncate_error(error)))
            .updated_at(now)
            .exec(&mut db)
            .await?;

        Ok(BackgroundJobFailureOutcome {
            retrying,
            attempt_count,
            max_attempts,
        })
    }

    pub async fn recover_interrupted_background_jobs(&self, queue: &str) -> Result<usize> {
        let _write = self.write_guard().await;
        let mut db = self.executor();
        let running = BackgroundJob::filter(
            BackgroundJob::fields().queue().eq(queue).and(
                BackgroundJob::fields()
                    .status()
                    .eq(BackgroundJobStatus::Running.persisted_value()),
            ),
        )
        .exec(&mut db)
        .await?;
        let recovered = running.len();

        for mut job in running {
            let status = if job.attempt_count < job.max_attempts {
                BackgroundJobStatus::Pending
            } else {
                BackgroundJobStatus::Failed
            };
            let now = now_timestamp();
            job.update()
                .status(status.persisted_value().to_string())
                .run_after(now.clone())
                .locked_by(None::<String>)
                .locked_at(None::<String>)
                .updated_at(now)
                .exec(&mut db)
                .await?;
        }

        Ok(recovered)
    }
}

fn truncate_error(error: &str) -> String {
    if error.len() <= MAX_ERROR_LENGTH {
        return error.to_string();
    }

    error
        .char_indices()
        .take_while(|(index, _)| *index < MAX_ERROR_LENGTH)
        .map(|(_, value)| value)
        .collect()
}
