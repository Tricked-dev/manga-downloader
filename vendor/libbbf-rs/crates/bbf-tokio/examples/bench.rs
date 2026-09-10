//! Reproducible async operation-boundary benchmark driver.
//!
//! Examples:
//!
//! ```text
//! cargo run -p bbf-tokio --release --example bench -- build-file input.bin output.bbf 10
//! cargo run -p bbf-tokio --release --example bench -- dedup-file input.bin output.bbf 10
//! cargo run -p bbf-tokio --release --example bench -- append-file archive.bbf input.bin 10
//! cargo run -p bbf-tokio --release --example bench -- petrify archive.bbf output.bbf 10
//! cargo run -p bbf-tokio --release --example bench -- verify archive.bbf 10
//! cargo run -p bbf-tokio --release --example bench -- concurrent archive.bbf 10 8
//! ```

use std::{
    env,
    error::Error,
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use bbf_format::MediaType;
use bbf_tokio::{AsyncArchive, AsyncArchiveWriter, AsyncFileAppender};
use tokio::{
    runtime::Builder,
    task::JoinSet,
    time::{self, MissedTickBehavior},
};

fn main() -> Result<(), Box<dyn Error>> {
    Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run(env::args().skip(1).collect()))
}

async fn run(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let operation = args.first().ok_or("operation is required")?.as_str();
    let (measurement, cleanup) = match operation {
        "build-file" => {
            let source = path_arg(&args, 1, "source")?;
            let destination = path_arg(&args, 2, "destination")?;
            let iterations = number_arg(&args, 3, 10)?;
            (
                measure_build_file(source, destination.clone(), iterations).await?,
                Some(destination),
            )
        }
        "dedup-file" => {
            let source = path_arg(&args, 1, "source")?;
            let destination = path_arg(&args, 2, "destination")?;
            let iterations = number_arg(&args, 3, 10)?;
            (
                measure_dedup_file(source, destination, iterations).await?,
                None,
            )
        }
        "append-file" => {
            let archive = path_arg(&args, 1, "archive")?;
            let source = path_arg(&args, 2, "source")?;
            let iterations = number_arg(&args, 3, 10)?;
            (
                measure_append_file(archive, source, iterations).await?,
                None,
            )
        }
        "petrify" => {
            let archive = path_arg(&args, 1, "archive")?;
            let destination = path_arg(&args, 2, "destination")?;
            let iterations = number_arg(&args, 3, 10)?;
            (
                measure_petrify(archive, destination, iterations).await?,
                None,
            )
        }
        "stream" => {
            let destination = path_arg(&args, 1, "destination")?;
            let size = number_arg(&args, 2, 4 * 1024 * 1024)?;
            let iterations = number_arg(&args, 3, 10)?;
            (
                measure_stream(destination.clone(), size, iterations).await?,
                Some(destination),
            )
        }
        "open" | "verify" | "read" | "range" | "concurrent" => {
            let archive = AsyncArchive::open(path_arg(&args, 1, "archive")?).await?;
            let iterations = number_arg(&args, 2, 10)?;
            match operation {
                "open" => (
                    measure_open(path_arg(&args, 1, "archive")?, iterations).await?,
                    None,
                ),
                "verify" => (measure_verify(&archive, iterations).await?, None),
                "read" => (measure_read(&archive, iterations).await?, None),
                "range" => (measure_range(&archive, iterations).await?, None),
                "concurrent" => {
                    let concurrency = number_arg(&args, 3, 8)?;
                    (
                        measure_concurrent(&archive, iterations, concurrency).await?,
                        None,
                    )
                }
                _ => unreachable!(),
            }
        }
        _ => return Err("unknown operation".into()),
    };
    if let Some(path) = cleanup {
        let _ = fs::remove_file(path);
    }
    println!(
        "operation={} iterations={} total_ns={} per_iteration_ns={:.2} checksum={} timer_max_lag_ns={}",
        measurement.operation,
        measurement.iterations,
        measurement.total_ns,
        measurement.total_ns as f64 / measurement.iterations as f64,
        measurement.checksum,
        measurement.timer_max_lag_ns,
    );
    Ok(())
}

fn path_arg(args: &[String], index: usize, name: &str) -> Result<PathBuf, Box<dyn Error>> {
    args.get(index)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{name} is required").into())
}

fn number_arg(args: &[String], index: usize, default: usize) -> Result<usize, Box<dyn Error>> {
    let value = args
        .get(index)
        .map(|value| {
            value
                .parse()
                .map_err(|error| format!("invalid number: {error}"))
        })
        .transpose()?
        .unwrap_or(default);
    if value == 0 {
        return Err("iteration and size arguments must be greater than zero".into());
    }
    Ok(value)
}

#[derive(Debug)]
struct Measurement {
    operation: &'static str,
    iterations: usize,
    total_ns: u128,
    checksum: u64,
    timer_max_lag_ns: u64,
}

async fn measure_build_file(
    source: PathBuf,
    destination: PathBuf,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        let mut writer = AsyncArchiveWriter::create(&destination).await?;
        let asset = writer
            .append_asset_file(&source, MediaType::Unknown.as_u8(), 0)
            .await?;
        writer.append_page(asset, 0)?;
        checksum ^= writer.finish().await?.footer().asset_count;
    }
    Ok(measurement("build-file", iterations, started, checksum, 0))
}

async fn measure_stream(
    destination: PathBuf,
    size: usize,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let payload = vec![b's'; size];
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        let mut writer = AsyncArchiveWriter::create(&destination).await?;
        let asset = writer
            .append_asset_reader(&payload[..], MediaType::Unknown.as_u8(), 0)
            .await?;
        writer.append_page(asset, 0)?;
        checksum ^= writer.finish().await?.footer().string_pool_size;
    }
    Ok(measurement("stream", iterations, started, checksum, 0))
}

async fn measure_dedup_file(
    source: PathBuf,
    destination: PathBuf,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let mut writer = AsyncArchiveWriter::create(&destination).await?;
    let first = writer
        .append_asset_file(&source, MediaType::Unknown.as_u8(), 0)
        .await?;
    let started = Instant::now();
    let mut checksum = first;
    for _ in 0..iterations {
        checksum ^= writer
            .append_asset_file(&source, MediaType::Unknown.as_u8(), 0)
            .await?;
    }
    let measurement = measurement("dedup-file", iterations, started, checksum, 0);
    writer.abort().await?;
    Ok(measurement)
}

async fn measure_append_file(
    archive: PathBuf,
    source: PathBuf,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let mut destinations = Vec::with_capacity(iterations);
    for iteration in 0..iterations {
        let destination = env::temp_dir().join(format!(
            "libbbf-tokio-bench-append-{}-{iteration}.bbf",
            std::process::id()
        ));
        fs::copy(&archive, &destination)?;
        destinations.push(destination);
    }

    let started = Instant::now();
    let mut checksum = 0;
    for destination in &destinations {
        let mut appender = AsyncFileAppender::open(destination).await?;
        let asset = appender
            .add_asset_file(&source, MediaType::Unknown.as_u8(), 0)
            .await?;
        checksum ^= appender.add_page_asset(asset, 0)?;
        appender.finalize().await?;
    }
    let measurement = measurement("append-file", iterations, started, checksum, 0);
    for destination in destinations {
        let _ = fs::remove_file(destination);
    }
    Ok(measurement)
}

async fn measure_petrify(
    archive_path: PathBuf,
    destination: PathBuf,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let archive = AsyncArchive::open(&archive_path).await?;
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum ^= archive.petrify_to(&destination).await?.footer().page_count;
    }
    let measurement = measurement("petrify", iterations, started, checksum, 0);
    let _ = fs::remove_file(destination);
    Ok(measurement)
}

async fn measure_open(path: PathBuf, iterations: usize) -> Result<Measurement, Box<dyn Error>> {
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum ^= AsyncArchive::open(&path).await?.footer().page_count;
    }
    Ok(measurement("open", iterations, started, checksum, 0))
}

async fn measure_verify(
    archive: &AsyncArchive,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum ^= archive.verify().await?.bytes_verified;
    }
    Ok(measurement("verify", iterations, started, checksum, 0))
}

async fn measure_read(
    archive: &AsyncArchive,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum ^= archive.read_asset(0).await?.len() as u64;
    }
    Ok(measurement("read", iterations, started, checksum, 0))
}

async fn measure_range(
    archive: &AsyncArchive,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let length = archive.asset(0)?.file_size.min(64 * 1024);
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum ^= archive.read_asset_range(0, 0, length).await?.len() as u64;
    }
    Ok(measurement("range", iterations, started, checksum, 0))
}

async fn measure_concurrent(
    archive: &AsyncArchive,
    iterations: usize,
    concurrency: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let length = archive.asset(0)?.file_size.min(64 * 1024);
    let running = Arc::new(AtomicBool::new(true));
    let max_lag = Arc::new(AtomicU64::new(0));
    let timer = spawn_timer(Arc::clone(&running), Arc::clone(&max_lag));
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        let mut tasks = JoinSet::new();
        for _ in 0..concurrency {
            let archive = archive.clone();
            tasks.spawn(async move {
                archive
                    .read_asset_range(0, 0, length)
                    .await
                    .map(|data| data.len() as u64)
            });
        }
        while let Some(result) = tasks.join_next().await {
            checksum ^= result??;
        }
    }
    running.store(false, Ordering::Release);
    timer.await?;
    Ok(measurement(
        "concurrent",
        iterations,
        started,
        checksum,
        max_lag.load(Ordering::Acquire),
    ))
}

fn spawn_timer(running: Arc<AtomicBool>, max_lag: Arc<AtomicU64>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let period = Duration::from_millis(1);
        let mut interval = time::interval(period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        interval.tick().await;
        let mut expected = Instant::now() + period;
        while running.load(Ordering::Acquire) {
            interval.tick().await;
            let now = Instant::now();
            let lag = now.saturating_duration_since(expected).as_nanos() as u64;
            max_lag.fetch_max(lag, Ordering::Relaxed);
            expected += period;
        }
    })
}

fn measurement(
    operation: &'static str,
    iterations: usize,
    started: Instant,
    checksum: u64,
    timer_max_lag_ns: u64,
) -> Measurement {
    Measurement {
        operation,
        iterations,
        total_ns: started.elapsed().as_nanos(),
        checksum,
        timer_max_lag_ns,
    }
}
