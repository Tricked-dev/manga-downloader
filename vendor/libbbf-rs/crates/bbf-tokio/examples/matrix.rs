//! Deterministic async benchmark matrix.
//!
//! This driver measures operation boundaries that can be compared with the
//! synchronous benchmark drivers. It creates its fixtures outside measured
//! regions and removes them when the process exits.
//!
//! ```text
//! cargo run -p bbf-tokio --release --example matrix -- 3
//! ```

use std::{
    env,
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use bbf_format::MediaType;
use bbf_tokio::{AsyncArchive, AsyncArchiveWriter, ConcurrencyLimiter};
use tokio::{
    io::AsyncReadExt,
    runtime::Builder,
    task::JoinSet,
    time::{self, MissedTickBehavior},
};

const SMALL_ASSET: usize = 4 * 1024;
const MEDIUM_ASSET: usize = 4 * 1024 * 1024;
const LARGE_ASSET: usize = 40 * 1024 * 1024;
const LARGE_INDEX_ASSETS: usize = 64;
const RANGE_SIZE: u64 = 64 * 1024;
const CONCURRENCY: usize = 8;

fn main() -> Result<(), Box<dyn Error>> {
    let iterations = env::args()
        .nth(1)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(3);
    if iterations == 0 {
        return Err("iterations must be greater than zero".into());
    }

    // Keep one worker available for the timer while the streaming adapter
    // performs its intentionally bounded direct positional reads.
    Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(run(iterations))
}

struct Fixtures {
    root: PathBuf,
}

impl Fixtures {
    fn create() -> Result<Self, Box<dyn Error>> {
        let root = env::temp_dir().join(format!("libbbf-rs-async-matrix-{}", std::process::id()));
        fs::create_dir(&root)?;
        Ok(Self { root })
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn write_payload(&self, name: &str, size: usize, seed: u64) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.path(name);
        let mut file = fs::File::create(&path)?;
        let mut buffer = vec![0u8; 256 * 1024];
        let mut written = 0usize;
        while written < size {
            let amount = (size - written).min(buffer.len());
            for (index, byte) in buffer[..amount].iter_mut().enumerate() {
                let position = written + index;
                let value = seed
                    .wrapping_add(position as u64)
                    .wrapping_mul(0x9e37_79b9_7f4a_7c15);
                *byte = (value ^ (value >> 23) ^ (value >> 41)) as u8;
            }
            file.write_all(&buffer[..amount])?;
            written += amount;
        }
        file.flush()?;
        Ok(path)
    }
}

impl Drop for Fixtures {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

async fn run(iterations: usize) -> Result<(), Box<dyn Error>> {
    let fixtures = Fixtures::create()?;
    let small_unique = make_inputs(&fixtures, "small-unique", SMALL_ASSET, 4, false)?;
    let medium_unique = make_inputs(&fixtures, "medium-unique", MEDIUM_ASSET, 4, false)?;
    let large_unique = make_inputs(&fixtures, "large-unique", LARGE_ASSET, 2, false)?;
    let small_duplicate = make_inputs(&fixtures, "small-duplicate", SMALL_ASSET, 4, true)?;
    let medium_duplicate = make_inputs(&fixtures, "medium-duplicate", MEDIUM_ASSET, 4, true)?;
    let large_duplicate = make_inputs(&fixtures, "large-duplicate", LARGE_ASSET, 2, true)?;
    let large_index = make_inputs(
        &fixtures,
        "large-index",
        SMALL_ASSET,
        LARGE_INDEX_ASSETS,
        false,
    )?;
    let large_index_path = fixtures.path("large-index.bbf");
    build_archive(&large_index, &large_index_path).await?;

    for (label, inputs, bytes) in [
        ("small-unique", &small_unique, SMALL_ASSET),
        ("small-duplicate", &small_duplicate, SMALL_ASSET),
        ("medium-unique", &medium_unique, MEDIUM_ASSET),
        ("medium-duplicate", &medium_duplicate, MEDIUM_ASSET),
        ("large-unique", &large_unique, LARGE_ASSET),
        ("large-duplicate", &large_duplicate, LARGE_ASSET),
    ] {
        let output = fixtures.path(&format!("{label}.bbf"));
        let measurement = measure_build(label, inputs, bytes, &output, iterations).await?;
        print_measurement(measurement);
    }

    let small_archive_path = fixtures.path("small-unique.bbf");
    let large_archive_path = fixtures.path("large-unique.bbf");
    let large_archive = AsyncArchive::open(&large_archive_path).await?;

    print_measurement(measure_open("open-small-index", &small_archive_path, iterations).await?);
    print_measurement(measure_open("open-large-index", &large_index_path, iterations).await?);
    print_measurement(measure_verify("verify-large", &large_archive, iterations).await?);
    print_measurement(measure_read("read-large", &large_archive, iterations).await?);
    print_measurement(measure_stream("stream-large", &large_archive, iterations).await?);
    print_measurement(measure_range("range-large", &large_archive, iterations).await?);
    print_measurement(
        measure_concurrent("concurrent-unlimited", &large_archive, iterations, None).await?,
    );
    print_measurement(
        measure_concurrent(
            "concurrent-limited-4",
            &large_archive,
            iterations,
            Some(ConcurrencyLimiter::new(4)?),
        )
        .await?,
    );

    Ok(())
}

fn make_inputs(
    fixtures: &Fixtures,
    prefix: &str,
    size: usize,
    count: usize,
    duplicate: bool,
) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let first = fixtures.write_payload(&format!("{prefix}-0.bin"), size, 0x1234_5678)?;
    if duplicate {
        return Ok((0..count).map(|_| first.clone()).collect());
    }

    let mut paths = vec![first];
    for index in 1..count {
        paths.push(fixtures.write_payload(
            &format!("{prefix}-{index}.bin"),
            size,
            0x1234_5678 + index as u64,
        )?);
    }
    Ok(paths)
}

async fn build_archive(
    inputs: &[PathBuf],
    destination: &Path,
) -> Result<AsyncArchive, Box<dyn Error>> {
    let mut writer = AsyncArchiveWriter::create(destination).await?;
    let assets = writer
        .add_files(
            inputs
                .iter()
                .cloned()
                .map(|path| (path, MediaType::Unknown.as_u8(), 0)),
        )
        .await?;
    for asset in assets {
        writer.add_page(asset, 0)?;
    }
    Ok(writer.finish().await?)
}

async fn measure_build(
    label: &'static str,
    inputs: &[PathBuf],
    bytes_per_input: usize,
    destination: &Path,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        let archive = build_archive(inputs, destination).await?;
        checksum ^= archive.asset_count() as u64;
        checksum ^= archive.page_count() as u64;
    }
    Ok(Measurement::new(
        label,
        iterations,
        started.elapsed(),
        checksum ^ (bytes_per_input as u64),
        0,
    ))
}

async fn measure_open(
    label: &'static str,
    path: &Path,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        let archive = AsyncArchive::open(path).await?;
        checksum ^= archive.asset_count() as u64;
        checksum ^= archive.page_count() as u64;
    }
    Ok(Measurement::new(
        label,
        iterations,
        started.elapsed(),
        checksum,
        0,
    ))
}

async fn measure_verify(
    label: &'static str,
    archive: &AsyncArchive,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum ^= archive.verify().await?.bytes_verified;
    }
    Ok(Measurement::new(
        label,
        iterations,
        started.elapsed(),
        checksum,
        0,
    ))
}

async fn measure_read(
    label: &'static str,
    archive: &AsyncArchive,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum ^= archive.read_asset(0).await?.len() as u64;
    }
    Ok(Measurement::new(
        label,
        iterations,
        started.elapsed(),
        checksum,
        0,
    ))
}

async fn measure_stream(
    label: &'static str,
    archive: &AsyncArchive,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let mut buffer = vec![0u8; 256 * 1024];
    let running = Arc::new(AtomicBool::new(true));
    let max_lag = Arc::new(AtomicU64::new(0));
    let timer = spawn_timer(Arc::clone(&running), Arc::clone(&max_lag));
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        let mut reader = archive.asset_reader(0).await?;
        let mut total = 0u64;
        loop {
            let read = reader.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            total += read as u64;
        }
        checksum ^= total;
    }
    running.store(false, Ordering::Release);
    timer.await?;
    Ok(Measurement::new(
        label,
        iterations,
        started.elapsed(),
        checksum,
        max_lag.load(Ordering::Acquire),
    ))
}

async fn measure_range(
    label: &'static str,
    archive: &AsyncArchive,
    iterations: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let length = archive.asset(0)?.file_size.min(RANGE_SIZE);
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum ^= archive.read_asset_range(0, 0, length).await?.len() as u64;
    }
    Ok(Measurement::new(
        label,
        iterations,
        started.elapsed(),
        checksum,
        0,
    ))
}

async fn measure_concurrent(
    label: &'static str,
    archive: &AsyncArchive,
    iterations: usize,
    limiter: Option<ConcurrencyLimiter>,
) -> Result<Measurement, Box<dyn Error>> {
    let length = archive.asset(0)?.file_size.min(RANGE_SIZE);
    let archive = limiter.map_or_else(
        || archive.clone(),
        |limiter| archive.clone().with_limiter(limiter),
    );
    let running = Arc::new(AtomicBool::new(true));
    let max_lag = Arc::new(AtomicU64::new(0));
    let timer = spawn_timer(Arc::clone(&running), Arc::clone(&max_lag));
    let started = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        let mut tasks = JoinSet::new();
        for _ in 0..CONCURRENCY {
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
    Ok(Measurement::new(
        label,
        iterations,
        started.elapsed(),
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

#[derive(Debug, Clone, Copy)]
struct Measurement {
    label: &'static str,
    iterations: usize,
    elapsed: Duration,
    checksum: u64,
    timer_max_lag_ns: u64,
}

impl Measurement {
    fn new(
        label: &'static str,
        iterations: usize,
        elapsed: Duration,
        checksum: u64,
        timer_max_lag_ns: u64,
    ) -> Self {
        Self {
            label,
            iterations,
            elapsed,
            checksum,
            timer_max_lag_ns,
        }
    }
}

fn print_measurement(measurement: Measurement) {
    let per_iteration_ns = measurement.elapsed.as_nanos() as f64 / measurement.iterations as f64;
    println!(
        "operation={} iterations={} total_ns={} per_iteration_ns={per_iteration_ns:.2} checksum={} timer_max_lag_ns={}",
        measurement.label,
        measurement.iterations,
        measurement.elapsed.as_nanos(),
        measurement.checksum,
        measurement.timer_max_lag_ns,
    );
}
