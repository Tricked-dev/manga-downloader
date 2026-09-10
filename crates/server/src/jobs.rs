use std::{fmt, marker::PhantomData, sync::Arc, time::Duration};

use apalis::prelude::{
    Acknowledge, AcknowledgementExt, Attempt, Backend, BoxDynError, Parts, Task, TaskId, TaskSink,
    TaskSinkError, TaskStream, WorkerBuilder, WorkerContext,
};
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
