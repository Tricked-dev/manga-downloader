use std::{fmt, marker::PhantomData, sync::Arc, time::Duration};

use apalis::prelude::{
    Acknowledge, AcknowledgementExt, Attempt, Backend, BoxDynError, Parts, Task, TaskId, TaskSink,
    TaskSinkError, TaskStream, WorkerBuilder, WorkerContext,
};
use apalis_postgres::{Config as PgConfig, PgContext, PgPool, PgTask, PgTaskId, PostgresStorage};
use backend_persistence::DatabaseBackend;
use futures_util::{
    FutureExt, Stream, StreamExt,
    future::BoxFuture,
    stream::{self, BoxStream},
};
use serde::{Serialize, de::DeserializeOwned};
use tokio_graceful::ShutdownGuard;

use crate::{AppState, app::upscaling};

const UPSCALE_QUEUE: &str = "upscale";
const UPSCALE_WORKER: &str = "upscale-worker";
const DEFAULT_MAX_ATTEMPTS: i64 = 3;
const JOB_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// The application and its durable jobs always use the same database backend.
#[derive(Clone)]
pub(crate) enum UpscaleQueue {
    Sqlite,
    Postgres(PgPool),
}

impl UpscaleQueue {
    pub(crate) async fn open(db: &backend_persistence::Database) -> anyhow::Result<Self> {
        match db.backend() {
            DatabaseBackend::Sqlite => Ok(Self::Sqlite),
            DatabaseBackend::Postgres => {
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(5)
                    .connect(db.connection_url())
                    .await?;
                PostgresStorage::setup(&pool).await?;
                Ok(Self::Postgres(pool))
            }
        }
    }
}

fn postgres_config() -> PgConfig {
    PgConfig::new(UPSCALE_QUEUE).set_buffer_size(1)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct UpscaleJob {
    download_id: String,
    scale: u32,
}

impl UpscaleJob {
    fn new(download_id: &str, scale: u32) -> Self {
        Self {
            download_id: download_id.to_string(),
            scale,
        }
    }
}

pub(crate) async fn enqueue_upscale(
    state: &Arc<AppState>,
    download_id: &str,
    scale: u32,
) -> anyhow::Result<String> {
    state
        .db
        .set_upscale_progress(download_id, "queued", 0, 0, "Waiting for upscale worker")
        .await?;
    let result = enqueue_upscale_job(state, download_id, scale).await;
    if let Err(error) = &result {
        state
            .db
            .set_upscale_progress(download_id, "failed", 0, 0, &error.to_string())
            .await?;
    }
    result
}

async fn enqueue_upscale_job(
    state: &Arc<AppState>,
    download_id: &str,
    scale: u32,
) -> anyhow::Result<String> {
    if let UpscaleQueue::Postgres(pool) = &state.upscale_queue {
        let mut storage = PostgresStorage::<UpscaleJob>::new_with_config(pool, &postgres_config());
        let id = PgTaskId::new(ulid::Ulid::new());
        let mut task: PgTask<UpscaleJob> = Task::new(UpscaleJob::new(download_id, scale));
        task.parts.task_id = Some(id);
        task.parts.ctx = PgContext::default().with_max_attempts(3);
        storage
            .push_task(task)
            .await
            .map_err(|error| anyhow::anyhow!("failed to enqueue upscale: {error}"))?;
        return Ok(id.to_string());
    }
    let mut storage = ToastyJobStorage::<UpscaleJob>::new(state.db.clone(), UPSCALE_QUEUE);
    storage
        .push(UpscaleJob::new(download_id, scale))
        .await
        .map_err(|error| anyhow::anyhow!("failed to enqueue upscale: {error}"))?;
    Ok(storage.last_enqueued_id.unwrap_or_default())
}

pub(crate) async fn run_upscale_worker(
    state: Arc<AppState>,
    shutdown: ShutdownGuard,
) -> anyhow::Result<()> {
    if let UpscaleQueue::Postgres(pool) = &state.upscale_queue {
        let storage = PostgresStorage::<UpscaleJob>::new_with_notify(pool, &postgres_config());
        let worker =
            WorkerBuilder::new(UPSCALE_WORKER)
                .backend(storage)
                .build(move |job: UpscaleJob| {
                    let state = Arc::clone(&state);
                    async move { upscaling::run(state, &job.download_id, job.scale).await }
                });
        let shutdown = shutdown.clone_weak();
        tracing::info!(
            queue = UPSCALE_QUEUE,
            backend = "postgres",
            "Upscale Worker Started"
        );
        worker
            .run_until(async move {
                shutdown.cancelled().await;
                Ok::<(), apalis::prelude::WorkerError>(())
            })
            .await
            .map_err(|error| anyhow::anyhow!("upscale worker failed: {error}"))?;
        return Ok(());
    }
    let recovered = state
        .db
        .recover_interrupted_background_jobs(UPSCALE_QUEUE)
        .await?;
    if recovered > 0 {
        tracing::warn!(
            recovered,
            queue = UPSCALE_QUEUE,
            "Interrupted Background Jobs Recovered On Startup",
        );
    }

    let storage = ToastyJobStorage::<UpscaleJob>::new(state.db.clone(), UPSCALE_QUEUE);
    let ack = ToastyJobAck::new(state.db.clone());
    let worker = WorkerBuilder::new(UPSCALE_WORKER)
        .backend(storage)
        .ack_with(ack)
        .build(move |job: UpscaleJob| {
            let state = Arc::clone(&state);
            async move { upscaling::run(state, &job.download_id, job.scale).await }
        });

    let shutdown = shutdown.clone_weak();
    tracing::info!(queue = UPSCALE_QUEUE, "Upscale Worker Started",);
    worker
        .run_until(async move {
            shutdown.cancelled().await;
            Ok::<(), apalis::prelude::WorkerError>(())
        })
        .await
        .map_err(|error| anyhow::anyhow!("upscale worker failed: {error}"))?;
    tracing::info!(queue = UPSCALE_QUEUE, "Upscale Worker Stopped",);

    Ok(())
}

struct ToastyJobStorage<Job> {
    db: backend_persistence::Database,
    queue: &'static str,
    poll_interval: Duration,
    last_enqueued_id: Option<String>,
    _job: PhantomData<fn() -> Job>,
}

impl<Job> ToastyJobStorage<Job> {
    fn new(db: backend_persistence::Database, queue: &'static str) -> Self {
        Self {
            db,
            queue,
            poll_interval: JOB_POLL_INTERVAL,
            last_enqueued_id: None,
            _job: PhantomData,
        }
    }
}

impl<Job> Backend for ToastyJobStorage<Job>
where
    Job: DeserializeOwned + Send + 'static,
{
    type Args = Job;
    type IdType = String;
    type Context = ();
    type Error = BoxDynError;
    type Stream = TaskStream<Task<Job, (), String>, BoxDynError>;
    type Beat = BoxStream<'static, Result<(), Self::Error>>;
    type Layer = tower::layer::util::Identity;

    fn heartbeat(&self, _worker: &WorkerContext) -> Self::Beat {
        stream::pending::<Result<(), Self::Error>>().boxed()
    }

    fn middleware(&self) -> Self::Layer {
        tower::layer::util::Identity::new()
    }

    fn poll(self, worker: &WorkerContext) -> Self::Stream {
        let worker_id = worker.name().clone();
        stream::unfold((self, worker_id), |(storage, worker_id)| async move {
            let item = match storage
                .db
                .lease_next_background_job(storage.queue, &worker_id)
                .await
            {
                Ok(Some(lease)) => match serde_json::from_str::<Job>(&lease.payload_json) {
                    Ok(job) => Ok(Some(task_from_lease(job, lease.id, lease.attempt_count))),
                    Err(error) => fail_decoded_job::<Job>(&storage.db, &lease.id, error).await,
                },
                Ok(None) => {
                    tokio::time::sleep(storage.poll_interval).await;
                    Ok(None)
                }
                Err(error) => Err(boxed_job_error(format_args!(
                    "failed to lease background job: {error}"
                ))),
            };

            Some((item, (storage, worker_id)))
        })
        .boxed()
    }
}

impl<Job> TaskSink<Job> for ToastyJobStorage<Job>
where
    Job: Serialize + DeserializeOwned + Send + 'static,
{
    async fn push(&mut self, task: Job) -> Result<(), TaskSinkError<Self::Error>> {
        let payload_json = serde_json::to_string(&task)
            .map_err(|error| TaskSinkError::PushError(boxed_job_error(error)))?;
        let id = self
            .db
            .enqueue_background_job(self.queue, payload_json, DEFAULT_MAX_ATTEMPTS)
            .await
            .map_err(|error| TaskSinkError::PushError(boxed_job_error(error)))?;
        self.last_enqueued_id = Some(id);
        Ok(())
    }

    async fn push_bulk(&mut self, tasks: Vec<Job>) -> Result<(), TaskSinkError<Self::Error>> {
        for task in tasks {
            self.push(task).await?;
        }
        Ok(())
    }

    async fn push_stream(
        &mut self,
        mut tasks: impl Stream<Item = Job> + Unpin + Send,
    ) -> Result<(), TaskSinkError<Self::Error>> {
        while let Some(task) = tasks.next().await {
            self.push(task).await?;
        }
        Ok(())
    }

    async fn push_task(
        &mut self,
        task: Task<Job, Self::Context, Self::IdType>,
    ) -> Result<(), TaskSinkError<Self::Error>> {
        self.push(task.args).await
    }

    async fn push_all(
        &mut self,
        mut tasks: impl Stream<Item = Task<Job, Self::Context, Self::IdType>> + Unpin + Send,
    ) -> Result<(), TaskSinkError<Self::Error>> {
        while let Some(task) = tasks.next().await {
            self.push_task(task).await?;
        }
        Ok(())
    }
}

async fn fail_decoded_job<Job>(
    db: &backend_persistence::Database,
    id: &str,
    error: serde_json::Error,
) -> Result<Option<Task<Job, (), String>>, BoxDynError> {
    let message = format!("failed to decode background job payload: {error}");
    let outcome = db
        .fail_background_job(id, &message)
        .await
        .map_err(|error| {
            boxed_job_error(format_args!(
                "failed to mark undecodable background job failed: {error}"
            ))
        })?;
    tracing::warn!(
        job_id = %id,
        retrying = outcome.retrying,
        attempt_count = outcome.attempt_count,
        max_attempts = outcome.max_attempts,
        error = %message,
        "Background Job Payload Decode Failed",
    );

    Ok(None)
}

#[derive(Clone)]
struct ToastyJobAck {
    db: backend_persistence::Database,
}

impl ToastyJobAck {
    fn new(db: backend_persistence::Database) -> Self {
        Self { db }
    }
}

impl Acknowledge<(), (), String> for ToastyJobAck {
    type Error = JobStorageError;
    type Future = BoxFuture<'static, Result<(), Self::Error>>;

    fn ack(&mut self, res: &Result<(), BoxDynError>, parts: &Parts<(), String>) -> Self::Future {
        let db = self.db.clone();
        let task_id = parts.task_id.clone().map(|task_id| task_id.inner().clone());
        let error = res.as_ref().err().map(ToString::to_string);

        async move {
            let id =
                task_id.ok_or_else(|| JobStorageError::new("background job task id missing"))?;
            match error {
                Some(error) => {
                    let outcome = db
                        .fail_background_job(&id, &error)
                        .await
                        .map_err(JobStorageError::from_display)?;
                    tracing::warn!(
                        job_id = %id,
                        retrying = outcome.retrying,
                        attempt_count = outcome.attempt_count,
                        max_attempts = outcome.max_attempts,
                        error = %error,
                        "Background Job Failed",
                    );
                }
                None => {
                    db.complete_background_job(&id)
                        .await
                        .map_err(JobStorageError::from_display)?;
                }
            }

            Ok(())
        }
        .boxed()
    }
}

fn task_from_lease<Job>(job: Job, id: String, attempt_count: i64) -> Task<Job, (), String> {
    let mut task = Task::new(job);
    task.parts.task_id = Some(TaskId::new(id));
    task.parts.attempt =
        Attempt::new_with_value(usize::try_from((attempt_count - 1).max(0)).unwrap_or(usize::MAX));
    task
}

fn boxed_job_error(error: impl fmt::Display) -> BoxDynError {
    Box::new(JobStorageError::from_display(error))
}

#[derive(Debug)]
struct JobStorageError {
    message: String,
}

impl JobStorageError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn from_display(error: impl fmt::Display) -> Self {
        Self::new(error.to_string())
    }
}

impl fmt::Display for JobStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JobStorageError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn durable_queue_drains_preexisting_and_live_jobs_then_stops() {
        let state = crate::server::build_test_app_state("durable-jobs", "durable-jobs").await;
        // A deleted download is a terminal no-op; it must not retry forever.
        let first = enqueue_upscale(&state, "already-deleted", 2).await.unwrap();
        let (stop, signal) = tokio::sync::oneshot::channel();
        let shutdown = tokio_graceful::Shutdown::builder()
            .with_signal(async move {
                let _ = signal.await;
            })
            .build();
        let worker_state = Arc::clone(&state);
        let worker = shutdown.spawn_task_fn(move |guard| run_upscale_worker(worker_state, guard));
        wait_for_done(&state, &first).await;
        let second = enqueue_upscale(&state, "deleted-after-start", 2)
            .await
            .unwrap();
        assert_ne!(first, second);
        wait_for_done(&state, &second).await;
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        shutdown.shutdown().await;
        state.upscaler.shutdown().await.unwrap();
        state.cache.close().await.unwrap();
    }

    async fn wait_for_done(state: &AppState, id: &str) {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let done = match &state.upscale_queue {
                    UpscaleQueue::Postgres(pool) => {
                        let (status, attempts): (String, i32) = sqlx::query_as(
                            "SELECT status, attempts FROM apalis.jobs WHERE id = $1",
                        )
                        .bind(id)
                        .fetch_one(pool)
                        .await
                        .unwrap();
                        if status == "Done" {
                            assert_eq!(attempts, 1);
                            true
                        } else {
                            false
                        }
                    }
                    UpscaleQueue::Sqlite => {
                        let connection = backend_persistence::open_configured_sqlite_connection(
                            state.db.connection_url(),
                            Default::default(),
                        )
                        .unwrap();
                        let (status, attempts): (String, i64) = connection
                            .query_row(
                                "SELECT status, attempt_count FROM background_jobs WHERE id = ?1",
                                [id],
                                |row| Ok((row.get(0)?, row.get(1)?)),
                            )
                            .unwrap();
                        if status == "completed" {
                            assert_eq!(attempts, 1);
                            true
                        } else {
                            false
                        }
                    }
                };
                if done {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect(
            "job should be acknowledged once, including jobs queued before the listener starts",
        );
    }
}
