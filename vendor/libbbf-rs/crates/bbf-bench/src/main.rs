use std::{env, fmt, fs, hint::black_box, io::Write, process::ExitCode, time::Instant};

use bbf_format::{MediaType, PAGE_SIZE, Page};
use bbf_mmap::MappedFile;
use bbf_mux::{AssetLookup, Builder, FileAppender, FileBuilder};

const HELP: &str = "\
USAGE:
  bbf-bench <OPERATION> [--input=FILE] [--iterations=N]

OPERATIONS:
  writer-constructor    Create and close one file-backed builder per iteration
  writer-add-4kb       Build one 4 KiB asset per iteration
  writer-add-4mb       Build one 4 MiB asset per iteration
  writer-add-40mb      Build one 40 MiB asset per iteration
  writer-write-4kb     Write one 4 KiB asset to a BBF file per iteration
  writer-write-4mb     Write one 4 MiB asset to a BBF file per iteration
  writer-write-40mb    Write one 40 MiB asset to a BBF file per iteration
  writer-file-4kb      Read and write one 4 KiB page per iteration
  writer-file-4mb      Read and write one 4 MiB page per iteration
  writer-file-40mb     Read and write one 40 MiB page per iteration
  writer-add-file-4kb  File-backed addPage of one 4 KiB asset per iteration
  writer-add-file-4mb  File-backed addPage of one 4 MiB asset per iteration
  writer-add-file-40mb File-backed addPage of one 40 MiB asset per iteration
  writer-append-file-4mb
                       Open, append, and finalize one 4 MiB asset per iteration
  writer-petrify-40mb  Stream and publish one 40 MiB archive per iteration
  component-hash-4mb   XXH3-128 over one 4 MiB payload per iteration
  component-write-4mb  Write one 4 MiB payload per iteration
  component-dedup-lookup-4mb
                       Probe one successful asset-table lookup per iteration
  component-page-index-4mb
                       Encode and hash a 4 MiB page index per iteration
  component-page-index-hash-4mb
                       Hash a pre-encoded 4 MiB page index per iteration
  writer-dedup-4kb     Add a duplicate 4 KiB asset per iteration
  writer-dedup-4mb     Add a duplicate 4 MiB asset per iteration
  writer-dedup-40mb    Add a duplicate 40 MiB asset per iteration
  writer-dedup-file-4mb
                       File-backed duplicate 4 MiB asset per iteration
  writer-dedup-file-40mb
                       File-backed duplicate 40 MiB asset per iteration
  writer-dedup-file-4kb
                       File-backed duplicate 4 KiB asset per iteration
  writer-add-meta      Add metadata without a parent per iteration
  writer-add-meta-parent
                       Add metadata with a parent per iteration
  writer-add-section   Add a section without a parent per iteration
  writer-add-section-parent
                       Add a section with a parent per iteration
  reader-open          Open a mapped BBF per iteration
  reader-header        Read the mapped BBF header
  reader-footer        Read the mapped BBF footer
  reader-asset-lookup  Read asset table entry 0
  reader-string        Read string-pool entry 0
  reader-asset-hash    Hash asset 0 from the mapped BBF
  reader-verify        Verify the mapped BBF footer and page asset hashes

OPTIONS:
  --input=FILE         BBF input for reader operations
  --iterations=N       Number of measured iterations (default: 10)
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    WriterConstructor,
    WriterAdd(usize),
    WriterAddFile(usize),
    WriterAppendFile(usize),
    WriterPetrify(usize),
    ComponentHash(usize),
    ComponentWrite(usize),
    ComponentDedupLookup(usize),
    ComponentPageIndex(usize),
    ComponentPageIndexHash(usize),
    WriterWrite(usize),
    WriterFile(usize),
    WriterDedup(usize),
    WriterDedupFile(usize),
    WriterAddMeta,
    WriterAddMetaParent,
    WriterAddSection,
    WriterAddSectionParent,
    ReaderOpen,
    ReaderHeader,
    ReaderFooter,
    ReaderAssetLookup,
    ReaderString,
    ReaderAssetHash,
    ReaderVerify,
}

impl Operation {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "writer-constructor" => Self::WriterConstructor,
            "writer-add-4kb" => Self::WriterAdd(4 * 1024),
            "writer-add-4mb" => Self::WriterAdd(4 * 1024 * 1024),
            "writer-add-40mb" => Self::WriterAdd(40 * 1024 * 1024),
            "writer-add-file-4kb" => Self::WriterAddFile(4 * 1024),
            "writer-add-file-4mb" => Self::WriterAddFile(4 * 1024 * 1024),
            "writer-add-file-40mb" => Self::WriterAddFile(40 * 1024 * 1024),
            "writer-append-file-4mb" => Self::WriterAppendFile(4 * 1024 * 1024),
            "writer-petrify-40mb" => Self::WriterPetrify(40 * 1024 * 1024),
            "component-hash-4mb" => Self::ComponentHash(4 * 1024 * 1024),
            "component-write-4mb" => Self::ComponentWrite(4 * 1024 * 1024),
            "component-dedup-lookup-4mb" => Self::ComponentDedupLookup(4 * 1024 * 1024),
            "component-page-index-4mb" => Self::ComponentPageIndex(4 * 1024 * 1024),
            "component-page-index-hash-4mb" => Self::ComponentPageIndexHash(4 * 1024 * 1024),
            "writer-write-4kb" => Self::WriterWrite(4 * 1024),
            "writer-write-4mb" => Self::WriterWrite(4 * 1024 * 1024),
            "writer-write-40mb" => Self::WriterWrite(40 * 1024 * 1024),
            "writer-file-4kb" => Self::WriterFile(4 * 1024),
            "writer-file-4mb" => Self::WriterFile(4 * 1024 * 1024),
            "writer-file-40mb" => Self::WriterFile(40 * 1024 * 1024),
            "writer-dedup-4kb" => Self::WriterDedup(4 * 1024),
            "writer-dedup-4mb" => Self::WriterDedup(4 * 1024 * 1024),
            "writer-dedup-40mb" => Self::WriterDedup(40 * 1024 * 1024),
            "writer-dedup-file-4mb" => Self::WriterDedupFile(4 * 1024 * 1024),
            "writer-dedup-file-4kb" => Self::WriterDedupFile(4 * 1024),
            "writer-dedup-file-40mb" => Self::WriterDedupFile(40 * 1024 * 1024),
            "writer-add-meta" => Self::WriterAddMeta,
            "writer-add-meta-parent" => Self::WriterAddMetaParent,
            "writer-add-section" => Self::WriterAddSection,
            "writer-add-section-parent" => Self::WriterAddSectionParent,
            "reader-open" => Self::ReaderOpen,
            "reader-header" => Self::ReaderHeader,
            "reader-footer" => Self::ReaderFooter,
            "reader-asset-lookup" => Self::ReaderAssetLookup,
            "reader-string" => Self::ReaderString,
            "reader-asset-hash" => Self::ReaderAssetHash,
            "reader-verify" => Self::ReaderVerify,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::WriterConstructor => "writer-constructor",
            Self::WriterAdd(size) if size == 4 * 1024 => "writer-add-4kb",
            Self::WriterAdd(size) if size == 4 * 1024 * 1024 => "writer-add-4mb",
            Self::WriterAdd(size) if size == 40 * 1024 * 1024 => "writer-add-40mb",
            Self::WriterAddFile(size) if size == 4 * 1024 => "writer-add-file-4kb",
            Self::WriterAddFile(size) if size == 4 * 1024 * 1024 => "writer-add-file-4mb",
            Self::WriterAddFile(size) if size == 40 * 1024 * 1024 => "writer-add-file-40mb",
            Self::WriterAppendFile(size) if size == 4 * 1024 * 1024 => "writer-append-file-4mb",
            Self::WriterPetrify(size) if size == 40 * 1024 * 1024 => "writer-petrify-40mb",
            Self::ComponentHash(size) if size == 4 * 1024 * 1024 => "component-hash-4mb",
            Self::ComponentWrite(size) if size == 4 * 1024 * 1024 => "component-write-4mb",
            Self::ComponentDedupLookup(size) if size == 4 * 1024 * 1024 => {
                "component-dedup-lookup-4mb"
            }
            Self::ComponentPageIndex(size) if size == 4 * 1024 * 1024 => "component-page-index-4mb",
            Self::ComponentPageIndexHash(size) if size == 4 * 1024 * 1024 => {
                "component-page-index-hash-4mb"
            }
            Self::WriterWrite(size) if size == 4 * 1024 => "writer-write-4kb",
            Self::WriterWrite(size) if size == 4 * 1024 * 1024 => "writer-write-4mb",
            Self::WriterWrite(size) if size == 40 * 1024 * 1024 => "writer-write-40mb",
            Self::WriterFile(size) if size == 4 * 1024 => "writer-file-4kb",
            Self::WriterFile(size) if size == 4 * 1024 * 1024 => "writer-file-4mb",
            Self::WriterFile(size) if size == 40 * 1024 * 1024 => "writer-file-40mb",
            Self::WriterDedup(size) if size == 4 * 1024 => "writer-dedup-4kb",
            Self::WriterDedup(size) if size == 4 * 1024 * 1024 => "writer-dedup-4mb",
            Self::WriterDedup(size) if size == 40 * 1024 * 1024 => "writer-dedup-40mb",
            Self::WriterDedupFile(size) if size == 4 * 1024 * 1024 => "writer-dedup-file-4mb",
            Self::WriterDedupFile(size) if size == 4 * 1024 => "writer-dedup-file-4kb",
            Self::WriterDedupFile(size) if size == 40 * 1024 * 1024 => "writer-dedup-file-40mb",
            Self::WriterAddMeta => "writer-add-meta",
            Self::WriterAddMetaParent => "writer-add-meta-parent",
            Self::WriterAddSection => "writer-add-section",
            Self::WriterAddSectionParent => "writer-add-section-parent",
            Self::WriterAdd(_)
            | Self::WriterAddFile(_)
            | Self::WriterAppendFile(_)
            | Self::WriterPetrify(_)
            | Self::ComponentHash(_)
            | Self::ComponentWrite(_)
            | Self::ComponentDedupLookup(_)
            | Self::ComponentPageIndex(_)
            | Self::ComponentPageIndexHash(_)
            | Self::WriterWrite(_)
            | Self::WriterFile(_)
            | Self::WriterDedup(_) => "writer-unknown",
            Self::WriterDedupFile(_) => "writer-unknown",
            Self::ReaderOpen => "reader-open",
            Self::ReaderHeader => "reader-header",
            Self::ReaderFooter => "reader-footer",
            Self::ReaderAssetLookup => "reader-asset-lookup",
            Self::ReaderString => "reader-string",
            Self::ReaderAssetHash => "reader-asset-hash",
            Self::ReaderVerify => "reader-verify",
        }
    }
}

#[derive(Debug)]
enum BenchError {
    Usage(String),
    Io(std::io::Error),
    Mapping(bbf_mmap::MappedFileError),
    Reader(bbf_io::ReaderError),
    Build(bbf_mux::BuilderError),
    Append(bbf_mux::AppendError),
}

impl fmt::Display for BenchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(formatter, "{message}\n\n{HELP}"),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Mapping(error) => write!(formatter, "mapping error: {error}"),
            Self::Reader(error) => write!(formatter, "reader error: {error}"),
            Self::Build(error) => write!(formatter, "build error: {error}"),
            Self::Append(error) => write!(formatter, "append error: {error}"),
        }
    }
}

impl std::error::Error for BenchError {}

impl From<std::io::Error> for BenchError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<bbf_mmap::MappedFileError> for BenchError {
    fn from(error: bbf_mmap::MappedFileError) -> Self {
        Self::Mapping(error)
    }
}

impl From<bbf_io::ReaderError> for BenchError {
    fn from(error: bbf_io::ReaderError) -> Self {
        Self::Reader(error)
    }
}

impl From<bbf_mux::BuilderError> for BenchError {
    fn from(error: bbf_mux::BuilderError) -> Self {
        Self::Build(error)
    }
}

impl From<bbf_mux::AppendError> for BenchError {
    fn from(error: bbf_mux::AppendError) -> Self {
        Self::Append(error)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(BenchError::Usage(message)) if message == HELP => {
            println!("{HELP}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), BenchError> {
    let mut arguments = env::args().skip(1);
    let Some(operation) = arguments.next() else {
        return Err(BenchError::Usage(HELP.to_owned()));
    };
    if operation == "--help" || operation == "-h" {
        return Err(BenchError::Usage(HELP.to_owned()));
    }
    let operation = Operation::parse(&operation)
        .ok_or_else(|| BenchError::Usage(format!("unknown operation: {operation}")))?;
    let mut input = None;
    let mut iterations = 10usize;
    for argument in arguments {
        if let Some(value) = argument.strip_prefix("--input=") {
            input = Some(value.to_owned());
        } else if let Some(value) = argument.strip_prefix("--iterations=") {
            iterations = value
                .parse()
                .map_err(|_| BenchError::Usage(format!("invalid iteration count: {value}")))?;
        } else {
            return Err(BenchError::Usage(format!("unknown option: {argument}")));
        }
    }
    if iterations == 0 {
        return Err(BenchError::Usage(
            "--iterations must be greater than zero".to_owned(),
        ));
    }

    let result = match operation {
        Operation::WriterAdd(size) => measure_writer_add(size, iterations)?,
        Operation::WriterConstructor => measure_writer_constructor(iterations)?,
        Operation::WriterAddFile(size) => measure_writer_add_file(size, iterations)?,
        Operation::WriterAppendFile(size) => measure_writer_append_file(size, iterations)?,
        Operation::WriterPetrify(size) => measure_writer_petrify(size, iterations)?,
        Operation::ComponentHash(size) => measure_component_hash(size, iterations)?,
        Operation::ComponentWrite(size) => measure_component_write(size, iterations)?,
        Operation::ComponentDedupLookup(size) => measure_component_dedup_lookup(size, iterations)?,
        Operation::ComponentPageIndex(size) => measure_component_page_index(size, iterations)?,
        Operation::ComponentPageIndexHash(size) => {
            measure_component_page_index_hash(size, iterations)?
        }
        Operation::WriterWrite(size) => measure_writer_write(size, iterations)?,
        Operation::WriterFile(size) => measure_writer_file(size, iterations)?,
        Operation::WriterDedup(size) => measure_writer_dedup(size, iterations)?,
        Operation::WriterDedupFile(size) => measure_writer_dedup_file(size, iterations)?,
        Operation::WriterAddMeta => measure_writer_meta(false, iterations)?,
        Operation::WriterAddMetaParent => measure_writer_meta(true, iterations)?,
        Operation::WriterAddSection => measure_writer_section(false, iterations)?,
        Operation::WriterAddSectionParent => measure_writer_section(true, iterations)?,
        Operation::ReaderOpen
        | Operation::ReaderHeader
        | Operation::ReaderFooter
        | Operation::ReaderAssetLookup
        | Operation::ReaderString
        | Operation::ReaderAssetHash
        | Operation::ReaderVerify => {
            let input = input.ok_or_else(|| {
                BenchError::Usage("reader operations require --input=FILE".to_owned())
            })?;
            measure_reader(operation, &input, iterations)?
        }
    };
    println!(
        "operation={} iterations={} total_ns={} per_iteration_ns={:.2} checksum={}",
        operation.name(),
        result.iterations,
        result.total_ns,
        result.per_iteration_ns,
        result.checksum
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct Measurement {
    iterations: usize,
    total_ns: u128,
    per_iteration_ns: f64,
    checksum: u64,
}

fn measure_writer_add(size: usize, iterations: usize) -> Result<Measurement, BenchError> {
    let payload = payload(size, b'a');
    measure(iterations, || {
        let mut builder = Builder::new();
        builder.add_page_bytes(&payload, 0, 0, 0);
        Ok(builder.build_bytes()?.len() as u64)
    })
}

fn measure_writer_constructor(iterations: usize) -> Result<Measurement, BenchError> {
    let path = env::temp_dir().join(format!(
        "libbbf-rs-bench-constructor-{}.bbf",
        std::process::id()
    ));
    let measurement = measure(iterations, || {
        let builder = FileBuilder::new(&path)?;
        drop(builder);
        Ok(fs::metadata(&path)?.len())
    });
    let cleanup = fs::remove_file(&path);
    match measurement {
        Ok(measurement) => {
            cleanup?;
            Ok(measurement)
        }
        Err(error) => {
            let _ = cleanup;
            Err(error)
        }
    }
}

fn measure_writer_add_file(size: usize, iterations: usize) -> Result<Measurement, BenchError> {
    let input_path = env::temp_dir().join(format!(
        "libbbf-rs-bench-add-file-input-{}-{size}.png",
        std::process::id()
    ));
    let output_path = env::temp_dir().join(format!(
        "libbbf-rs-bench-add-file-output-{}-{size}.bbf",
        std::process::id()
    ));
    fs::write(&input_path, payload(size, b's'))?;

    let measurement = measure(iterations, || {
        let mut builder = FileBuilder::new(&output_path)?;
        builder.add_page(&input_path, 0, 0)?;
        drop(builder);
        Ok(1)
    });
    let input_cleanup = fs::remove_file(&input_path);
    let output_cleanup = fs::remove_file(&output_path);
    match measurement {
        Ok(measurement) => {
            input_cleanup?;
            output_cleanup?;
            Ok(measurement)
        }
        Err(error) => {
            let _ = (input_cleanup, output_cleanup);
            Err(error)
        }
    }
}

fn measure_writer_append_file(size: usize, iterations: usize) -> Result<Measurement, BenchError> {
    let base_path = env::temp_dir().join(format!(
        "libbbf-rs-bench-append-base-{}-{size}.bbf",
        std::process::id()
    ));
    let source_path = env::temp_dir().join(format!(
        "libbbf-rs-bench-append-source-{}-{size}.png",
        std::process::id()
    ));
    let base_payload = constant_payload(40 * 1024 * 1024, b'b');
    let append_payload = constant_payload(size, b'a');
    let mut base = Builder::new();
    base.add_page_bytes(&base_payload, 0, 0, 0);
    base.write_to(&base_path)?;
    fs::write(&source_path, append_payload)?;

    let mut destinations = Vec::with_capacity(iterations);
    for iteration in 0..iterations {
        let destination = env::temp_dir().join(format!(
            "libbbf-rs-bench-append-destination-{}-{iteration}.bbf",
            std::process::id()
        ));
        fs::copy(&base_path, &destination)?;
        destinations.push(destination);
    }

    let mut next_destination = 0usize;
    let measurement = measure(iterations, || {
        let destination = &destinations[next_destination];
        next_destination += 1;
        let mut appender = FileAppender::open(destination).map_err(BenchError::Append)?;
        let asset = appender
            .add_asset_file(&source_path, MediaType::Unknown.as_u8(), 0)
            .map_err(BenchError::Append)?;
        let page = appender
            .add_page_asset(asset, 0)
            .map_err(BenchError::Append)?;
        appender.finalize().map_err(BenchError::Append)?;
        Ok(page)
    });

    let base_cleanup = fs::remove_file(&base_path);
    let source_cleanup = fs::remove_file(&source_path);
    for destination in destinations {
        let _ = fs::remove_file(destination);
    }
    base_cleanup?;
    source_cleanup?;
    measurement
}

fn measure_writer_petrify(size: usize, iterations: usize) -> Result<Measurement, BenchError> {
    let input_path = env::temp_dir().join(format!(
        "libbbf-rs-bench-petrify-input-{}-{size}.bbf",
        std::process::id()
    ));
    let output_path = env::temp_dir().join(format!(
        "libbbf-rs-bench-petrify-output-{}-{size}.bbf",
        std::process::id()
    ));
    let payload = constant_payload(size, b'p');
    let mut builder = Builder::new();
    builder.add_page_bytes(&payload, 0, 0, 0);
    builder.write_to(&input_path)?;

    let measurement = measure(iterations, || {
        Builder::petrify_file(&input_path, &output_path)?;
        Ok(fs::metadata(&output_path)?.len())
    });
    let input_cleanup = fs::remove_file(&input_path);
    let output_cleanup = fs::remove_file(&output_path);
    match measurement {
        Ok(measurement) => {
            input_cleanup?;
            output_cleanup?;
            Ok(measurement)
        }
        Err(error) => {
            let _ = (input_cleanup, output_cleanup);
            Err(error)
        }
    }
}

fn measure_component_hash(size: usize, iterations: usize) -> Result<Measurement, BenchError> {
    let payload = constant_payload(size, b'm');
    measure(iterations, || {
        let hash = xxhash_rust::xxh3::xxh3_128(&payload);
        Ok((hash as u64) ^ ((hash >> 64) as u64))
    })
}

fn measure_component_write(size: usize, iterations: usize) -> Result<Measurement, BenchError> {
    let payload = constant_payload(size, b'm');
    let path = env::current_dir()?.join("component-write.bbf");
    let measurement = measure(iterations, || {
        let file = fs::File::create(&path)?;
        let mut writer = std::io::BufWriter::with_capacity(64 * 1024, file);
        writer.write_all(&payload)?;
        writer.flush()?;
        Ok(payload.len() as u64)
    });
    let cleanup = fs::remove_file(&path);
    match measurement {
        Ok(measurement) => {
            cleanup?;
            Ok(measurement)
        }
        Err(error) => {
            let _ = cleanup;
            Err(error)
        }
    }
}

fn measure_component_dedup_lookup(
    size: usize,
    iterations: usize,
) -> Result<Measurement, BenchError> {
    let payload = constant_payload(size, b'm');
    let hash = xxhash_rust::xxh3::xxh3_128(&payload);
    let low = hash as u64;
    let high = (hash >> 64) as u64;
    let mut lookup = AssetLookup::new();
    lookup.insert(low, high, 7);
    measure(iterations, || {
        let lookup = black_box(&lookup);
        Ok(lookup.find(low, high).unwrap_or_default())
    })
}

fn measure_component_page_index(size: usize, iterations: usize) -> Result<Measurement, BenchError> {
    let pages = page_index_pages(size);
    measure(iterations, || {
        let mut encoded = vec![0u8; size];
        Page::encode_many_zeroed(&pages, &mut encoded);
        Ok(xxhash_rust::xxh3::xxh3_64(&encoded))
    })
}

fn measure_component_page_index_hash(
    size: usize,
    iterations: usize,
) -> Result<Measurement, BenchError> {
    let pages = page_index_pages(size);
    let mut encoded = vec![0u8; size];
    Page::encode_many_zeroed(&pages, &mut encoded);
    measure(iterations, || Ok(xxhash_rust::xxh3::xxh3_64(&encoded)))
}

fn page_index_pages(size: usize) -> Vec<Page> {
    let page_count = size / PAGE_SIZE;
    (0..page_count)
        .map(|index| Page {
            asset_index: (index % 17) as u64,
            flags: (index as u32) & 3,
        })
        .collect()
}

fn measure_writer_write(size: usize, iterations: usize) -> Result<Measurement, BenchError> {
    let payload = payload(size, b'w');
    let path = env::temp_dir().join(format!(
        "libbbf-rs-bench-write-{}-{size}.bbf",
        std::process::id()
    ));
    let measurement = measure(iterations, || {
        let mut builder = Builder::new();
        builder.add_page_bytes(&payload, 0, 0, 0);
        builder.write_to(&path)?;
        Ok(fs::metadata(&path)?.len())
    });
    let cleanup = fs::remove_file(&path);
    match measurement {
        Ok(measurement) => {
            cleanup?;
            Ok(measurement)
        }
        Err(error) => {
            let _ = cleanup;
            Err(error)
        }
    }
}

fn measure_writer_file(size: usize, iterations: usize) -> Result<Measurement, BenchError> {
    let input_path = env::temp_dir().join(format!(
        "libbbf-rs-bench-input-{}-{size}.png",
        std::process::id()
    ));
    let output_path = env::temp_dir().join(format!(
        "libbbf-rs-bench-file-{}-{size}.bbf",
        std::process::id()
    ));
    fs::write(&input_path, payload(size, b'f'))?;

    let measurement = measure(iterations, || {
        let mut builder = Builder::new();
        builder.add_page(&input_path, 0, 0)?;
        builder.write_to(&output_path)?;
        Ok(fs::metadata(&output_path)?.len())
    });
    let input_cleanup = fs::remove_file(&input_path);
    let output_cleanup = fs::remove_file(&output_path);
    match measurement {
        Ok(measurement) => {
            input_cleanup?;
            output_cleanup?;
            Ok(measurement)
        }
        Err(error) => {
            let _ = (input_cleanup, output_cleanup);
            Err(error)
        }
    }
}

fn measure_writer_dedup(size: usize, iterations: usize) -> Result<Measurement, BenchError> {
    let payload = payload(size, b'd');
    let mut builder = Builder::new();
    builder.add_page_bytes(&payload, 0, 0, 0);
    measure(iterations, || {
        builder.add_page_bytes(&payload, 0, 0, 0);
        Ok(builder.page_count() as u64)
    })
}

fn measure_writer_dedup_file(size: usize, iterations: usize) -> Result<Measurement, BenchError> {
    let input_path = env::temp_dir().join(format!(
        "libbbf-rs-bench-dedup-input-{}-{size}.png",
        std::process::id()
    ));
    let output_path = env::temp_dir().join(format!(
        "libbbf-rs-bench-dedup-output-{}-{size}.bbf",
        std::process::id()
    ));
    fs::write(&input_path, constant_payload(size, b'm'))?;

    let measurement = (|| {
        let mut builder = FileBuilder::new(&output_path)?;
        builder.add_page(&input_path, 0, 0)?;
        measure(iterations, || {
            builder.add_page(&input_path, 0, 0)?;
            Ok(1)
        })
    })();
    let input_cleanup = fs::remove_file(&input_path);
    let output_cleanup = fs::remove_file(&output_path);
    match measurement {
        Ok(measurement) => {
            input_cleanup?;
            output_cleanup?;
            Ok(measurement)
        }
        Err(error) => {
            let _ = (input_cleanup, output_cleanup);
            Err(error)
        }
    }
}

fn measure_writer_meta(with_parent: bool, iterations: usize) -> Result<Measurement, BenchError> {
    let mut builder = Builder::new();
    measure(iterations, || {
        let added = if with_parent {
            builder.add_meta("BenchKey", "BenchVal", Some("BenchParentLabel"))
        } else {
            builder.add_meta("BenchKey", "BenchVal", None)
        };
        Ok(u64::from(added))
    })
}

fn measure_writer_section(with_parent: bool, iterations: usize) -> Result<Measurement, BenchError> {
    let payload = payload(4 * 1024 * 1024, b'm');
    let mut builder = Builder::new();
    builder.add_page_bytes(&payload, 0, 0, 0);
    measure(iterations, || {
        let added = if with_parent {
            builder.add_section("BenchKey", 1, Some("BenchParent"))
        } else {
            builder.add_section("BenchKey", 1, None)
        };
        Ok(u64::from(added))
    })
}

fn measure_reader(
    operation: Operation,
    input: &str,
    iterations: usize,
) -> Result<Measurement, BenchError> {
    if operation == Operation::ReaderOpen {
        return measure(iterations, || {
            // SAFETY: the benchmark input is treated as immutable during the
            // measured operation.
            let mapped = unsafe { MappedFile::open(input)? };
            Ok(mapped.len() as u64)
        });
    }

    // SAFETY: the benchmark input is treated as immutable during the
    // measured operation.
    let mapped = unsafe { MappedFile::open(input)? };
    let reader = mapped.reader();
    let indexed = reader.indexed()?;
    measure(iterations, || match operation {
        Operation::ReaderHeader => Ok(black_box(reader.header()?.footer_offset)),
        Operation::ReaderFooter => Ok(black_box(reader.footer()?.page_count)),
        Operation::ReaderAssetLookup => Ok(black_box(
            indexed
                .asset(0)?
                .map(|asset| asset.file_offset)
                .unwrap_or_default(),
        )),
        Operation::ReaderString => Ok(black_box(
            indexed
                .string(0)?
                .map(|value| value.len() as u64)
                .unwrap_or_default(),
        )),
        Operation::ReaderAssetHash => Ok(black_box(
            indexed
                .compute_asset_hash(0)?
                .map(|(low, high)| low ^ high)
                .unwrap_or_default(),
        )),
        Operation::ReaderVerify => {
            let footer_ok = indexed.verify_footer_hash()?;
            let footer = indexed.footer();
            let mut checksum = u64::from(footer_ok);
            for page_index in 0..footer.page_count {
                let page =
                    indexed
                        .page(page_index)?
                        .ok_or_else(|| bbf_io::ReaderError::OutOfBounds {
                            offset: page_index,
                            size: 1,
                            file_size: reader.len() as u64,
                        })?;
                checksum ^= u64::from(
                    indexed
                        .verify_asset_hash(page.asset_index)?
                        .unwrap_or(false),
                );
            }
            Ok(black_box(checksum))
        }
        Operation::ReaderOpen
        | Operation::WriterConstructor
        | Operation::WriterAddFile(_)
        | Operation::WriterAppendFile(_)
        | Operation::WriterPetrify(_)
        | Operation::ComponentHash(_)
        | Operation::ComponentWrite(_)
        | Operation::ComponentDedupLookup(_)
        | Operation::ComponentPageIndex(_)
        | Operation::ComponentPageIndexHash(_)
        | Operation::WriterAdd(_)
        | Operation::WriterWrite(_)
        | Operation::WriterFile(_)
        | Operation::WriterDedup(_)
        | Operation::WriterDedupFile(_)
        | Operation::WriterAddMeta
        | Operation::WriterAddMetaParent
        | Operation::WriterAddSection
        | Operation::WriterAddSectionParent => unreachable!(),
    })
}

fn measure<F>(iterations: usize, mut operation: F) -> Result<Measurement, BenchError>
where
    F: FnMut() -> Result<u64, BenchError>,
{
    let start = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum ^= black_box(operation()?);
    }
    let total_ns = start.elapsed().as_nanos();
    Ok(Measurement {
        iterations,
        total_ns,
        per_iteration_ns: total_ns as f64 / iterations as f64,
        checksum,
    })
}

fn payload(size: usize, fill: u8) -> Vec<u8> {
    (0..size)
        .map(|index| fill.wrapping_add(index as u8))
        .collect()
}

fn constant_payload(size: usize, fill: u8) -> Vec<u8> {
    vec![fill; size]
}

#[cfg(test)]
mod tests {
    use super::{Operation, constant_payload, payload};

    #[test]
    fn parses_reference_aligned_operation_names() {
        assert_eq!(
            Operation::parse("writer-add-4kb"),
            Some(Operation::WriterAdd(4096))
        );
        assert_eq!(
            Operation::parse("writer-constructor"),
            Some(Operation::WriterConstructor)
        );
        assert_eq!(
            Operation::parse("writer-write-4mb"),
            Some(Operation::WriterWrite(4 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-add-40mb"),
            Some(Operation::WriterAdd(40 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-add-file-4kb"),
            Some(Operation::WriterAddFile(4 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-add-file-4mb"),
            Some(Operation::WriterAddFile(4 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-append-file-4mb"),
            Some(Operation::WriterAppendFile(4 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-petrify-40mb"),
            Some(Operation::WriterPetrify(40 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("component-hash-4mb"),
            Some(Operation::ComponentHash(4 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("component-write-4mb"),
            Some(Operation::ComponentWrite(4 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("component-dedup-lookup-4mb"),
            Some(Operation::ComponentDedupLookup(4 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("component-page-index-4mb"),
            Some(Operation::ComponentPageIndex(4 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("component-page-index-hash-4mb"),
            Some(Operation::ComponentPageIndexHash(4 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-dedup-40mb"),
            Some(Operation::WriterDedup(40 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-dedup-file-4mb"),
            Some(Operation::WriterDedupFile(4 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-dedup-file-4kb"),
            Some(Operation::WriterDedupFile(4 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-add-file-40mb"),
            Some(Operation::WriterAddFile(40 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-write-40mb"),
            Some(Operation::WriterWrite(40 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-file-40mb"),
            Some(Operation::WriterFile(40 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-dedup-file-40mb"),
            Some(Operation::WriterDedupFile(40 * 1024 * 1024))
        );
        assert_eq!(
            Operation::parse("writer-file-4kb"),
            Some(Operation::WriterFile(4 * 1024))
        );
        assert_eq!(
            Operation::parse("reader-verify"),
            Some(Operation::ReaderVerify)
        );
        assert_eq!(
            Operation::parse("writer-add-section-parent"),
            Some(Operation::WriterAddSectionParent)
        );
        assert_eq!(
            Operation::parse("reader-asset-lookup"),
            Some(Operation::ReaderAssetLookup)
        );
        assert_eq!(Operation::parse("missing"), None);
        assert_eq!(Operation::WriterConstructor.name(), "writer-constructor");
        assert_eq!(
            Operation::WriterAddFile(4 * 1024).name(),
            "writer-add-file-4kb"
        );
        assert_eq!(
            Operation::WriterAddFile(4 * 1024 * 1024).name(),
            "writer-add-file-4mb"
        );
        assert_eq!(
            Operation::WriterAppendFile(4 * 1024 * 1024).name(),
            "writer-append-file-4mb"
        );
        assert_eq!(
            Operation::WriterPetrify(40 * 1024 * 1024).name(),
            "writer-petrify-40mb"
        );
        assert_eq!(
            Operation::ComponentHash(4 * 1024 * 1024).name(),
            "component-hash-4mb"
        );
        assert_eq!(
            Operation::ComponentWrite(4 * 1024 * 1024).name(),
            "component-write-4mb"
        );
        assert_eq!(
            Operation::ComponentDedupLookup(4 * 1024 * 1024).name(),
            "component-dedup-lookup-4mb"
        );
        assert_eq!(
            Operation::ComponentPageIndex(4 * 1024 * 1024).name(),
            "component-page-index-4mb"
        );
        assert_eq!(
            Operation::ComponentPageIndexHash(4 * 1024 * 1024).name(),
            "component-page-index-hash-4mb"
        );
    }

    #[test]
    fn payload_is_deterministic() {
        assert_eq!(payload(4, b'a'), [b'a', b'b', b'c', b'd']);
        assert_eq!(constant_payload(4, b'm'), [b'm'; 4]);
    }
}
