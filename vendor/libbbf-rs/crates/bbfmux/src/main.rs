use std::{
    env, fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use bbf_format::VARIABLE_REAM_SIZE_FLAG;
use bbf_io::IndexedReader;
use bbf_mmap::{MappedFileError, OwnedFile};
use bbf_mux::{Builder, BuilderConfig, BuilderError, FileBuilder};

const HELP: &str = "========[ BBFMUX v3.0 ]====================================================
| Bound Book Format Muxer                             Developed by EF1500 |
===========================================================================

USAGE: bbfmux <INPUT_DIR|BBF_FILE> [MODE] [OPTIONS]...

MODES (Mutually Exclusive):
  (Default)    Mux folder contents into a BBF container
  --info       Display headers, metadata, and statistics
  --verify     Validate XXH3-128/64 hashes
  --extract    Unpack contents to disk
  --petrify    Linearize BBF file for faster reading

MUXER OPTIONS:
  --meta=K:V[:P]         Add metadata (Key:Value[:Parent])
  --metafile=<FILE>      Read K:V:P entries from file
  --section=N:T[:P]      Add section (Name:Target[:Parent])
  --sections=<FILE>      Read section entries from file
  --ream-size=<N>        Ream size exponent override (2^N)
  --alignment=<N>        Byte alignment exponent override (2^N)
  --variable-ream-size   Enable variable ream sizing (reccomended)

VERIFY / EXTRACT OPTIONS:
  --section=\"NAME\"    Target specific section
  --rangekey=\"KEY\"    Stop extraction on key substring match
  --asset=<ID>        Target specific asset ID
  --outdir=[PATH]     Extract asset(s) to directory
  --write-meta[=F]    Dump metadata to file [default: path.txt]
  --write-hashes[=F]  Dump hashes to file [default: hashes.txt]

INFO FLAGS:
  --hashes, --footer, --sections, --counts, --header, --metadata, --offsets

NOTE: Use ':' as delimiter on this system.";

const MAX_ENTRIES: usize = 256;

fn help_text() -> String {
    if cfg!(windows) {
        HELP.replace("Use ':'", "Use ';'")
    } else {
        HELP.to_owned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Mux,
    Info,
    Verify,
    Extract,
    Petrify,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MetadataSpec {
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    parent: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SectionSpec {
    name: Vec<u8>,
    target: Option<Vec<u8>>,
    parent: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct InfoOptions {
    show_hashes: bool,
    show_sections: bool,
    show_counts: bool,
    show_meta: bool,
    show_header: bool,
    show_footer: bool,
    show_offsets: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Arguments {
    mode: Mode,
    input: PathBuf,
    output: Option<PathBuf>,
    config: BuilderConfig,
    metadata: Vec<MetadataSpec>,
    sections: Vec<SectionSpec>,
    metadata_file: Option<PathBuf>,
    section_file: Option<PathBuf>,
    info: InfoOptions,
    asset: Option<u64>,
    section: Option<String>,
    range_key: Option<String>,
    verify_asset_index_alias: Option<u64>,
    outdir: Option<PathBuf>,
    write_meta: Option<PathBuf>,
    write_hashes: Option<PathBuf>,
}

#[derive(Debug)]
enum CliError {
    Help,
    Usage(String),
    Io(std::io::Error),
    Reader(bbf_io::ReaderError),
    Mapped(MappedFileError),
    Builder(BuilderError),
    Verification(String),
    NoPetrifyOutput,
    MissingInput(Mode),
    InfoMissingFile(PathBuf),
    VerifySectionNotFound(String),
    VerifySectionHasNoPages(String),
    VerifyAssetIndexOutOfBounds { index: u64, max: u64 },
    ExtractRangeKeyRequired,
    ExtractAssetIndexOutOfBounds,
    ExtractReportOpen(PathBuf),
    InfoFooterUnavailable,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Help => formatter.write_str(&help_text()),
            Self::Usage(message) => write!(formatter, "{message}\n\n{}", help_text()),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Reader(error) => write!(formatter, "BBF read error: {error}"),
            Self::Mapped(error) => write!(formatter, "BBF mapping error: {error}"),
            Self::Builder(error) => write!(formatter, "BBF build error: {error}"),
            Self::Verification(message) => write!(formatter, "verification failed: {message}"),
            Self::NoPetrifyOutput => formatter.write_str("no file selected for petrification"),
            Self::MissingInput(_) => formatter.write_str("missing input path"),
            Self::InfoMissingFile(path) => {
                write!(formatter, "unable to open file {}", path.display())
            }
            Self::VerifySectionNotFound(name) => {
                write!(formatter, "unable to find section with title: {name}")
            }
            Self::VerifySectionHasNoPages(name) => {
                write!(formatter, "no pages to verify in section {name}")
            }
            Self::VerifyAssetIndexOutOfBounds { index, max } => {
                write!(formatter, "invalid asset index {index} (max {max})")
            }
            Self::ExtractRangeKeyRequired => {
                formatter.write_str("section extraction requires --rangekey")
            }
            Self::ExtractAssetIndexOutOfBounds => formatter.write_str("asset index out of bounds"),
            Self::ExtractReportOpen(path) => {
                write!(
                    formatter,
                    "unable to open extraction report {}",
                    path.display()
                )
            }
            Self::InfoFooterUnavailable => formatter.write_str("unable to retrieve footer"),
        }
    }
}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<bbf_io::ReaderError> for CliError {
    fn from(error: bbf_io::ReaderError) -> Self {
        Self::Reader(error)
    }
}

impl From<MappedFileError> for CliError {
    fn from(error: MappedFileError) -> Self {
        Self::Mapped(error)
    }
}

impl From<BuilderError> for CliError {
    fn from(error: BuilderError) -> Self {
        Self::Builder(error)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Help) => {
            println!("{}", help_text());
            ExitCode::SUCCESS
        }
        Err(CliError::NoPetrifyOutput) => {
            println!("[BBFMUX] No file selected for petrification.");
            ExitCode::from(1)
        }
        Err(CliError::MissingInput(mode)) => {
            match mode {
                Mode::Mux => {
                    print!("Invalid Syntax. Run bbfmux --help to display avaliable options.")
                }
                Mode::Info => println!("[BBFMUX] Argument syntax error: missing input file."),
                _ => unreachable!("missing-input compatibility is limited to handled modes"),
            }
            ExitCode::from(1)
        }
        Err(CliError::InfoMissingFile(path)) => {
            eprintln!("[BBFCODEC] Unable to open file {}", path.display());
            println!("[BBFMUX] Unable to read header.");
            ExitCode::from(1)
        }
        Err(CliError::VerifySectionNotFound(name)) => {
            println!("[BBFMUX] Unable to find section with title: {name}");
            ExitCode::from(1)
        }
        Err(CliError::VerifySectionHasNoPages(name)) => {
            println!("[BBFMUX] No pages to verify. Unable to verify section {name}");
            ExitCode::from(1)
        }
        Err(CliError::VerifyAssetIndexOutOfBounds { index, max }) => {
            println!("[BBFMUX] Invalid Asset Index: {index} (Max: {max})");
            ExitCode::from(1)
        }
        Err(CliError::ExtractRangeKeyRequired) => {
            println!("[BBFMUX] Section Extraction Requires a rangekey.");
            ExitCode::from(1)
        }
        Err(CliError::ExtractAssetIndexOutOfBounds) => {
            println!("[BBFMUX] Asset index out of bounds.");
            ExitCode::from(1)
        }
        Err(CliError::ExtractReportOpen(path)) => {
            print!("[BBFMUX] Unable to open file: {}", path.display());
            ExitCode::from(1)
        }
        Err(CliError::InfoFooterUnavailable) => {
            println!("Unable to retrieve footer.");
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("{error}");
            if matches!(error, CliError::Usage(_)) {
                ExitCode::from(2)
            } else {
                ExitCode::from(1)
            }
        }
    }
}

fn run() -> Result<(), CliError> {
    let arguments = parse_args(env::args().skip(1))?;
    match arguments.mode {
        Mode::Mux => mux(&arguments),
        Mode::Info => info(&arguments),
        Mode::Verify => verify(&arguments),
        Mode::Extract => extract(&arguments),
        Mode::Petrify => petrify(&arguments),
    }
}

fn parse_args<I>(arguments: I) -> Result<Arguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut positional = Vec::new();
    let mut mode = Mode::Mux;
    let mut config = BuilderConfig {
        header_flags: 0,
        ..BuilderConfig::default()
    };
    let mut metadata = Vec::new();
    let mut sections = Vec::new();
    let mut metadata_file = None;
    let mut section_file = None;
    let mut info = InfoOptions::default();
    let mut asset = None;
    let mut petrify_output = None;
    let mut petrify_union_section = None;
    let mut petrify_output_sets_info_meta = false;
    let mut section = None;
    let mut range_key = None;
    let mut verify_asset_index_alias = None;
    let mut outdir = None;
    let mut write_meta = None;
    let mut write_hashes = None;

    for argument in arguments {
        if argument == "--help" || argument == "-h" {
            return Err(CliError::Help);
        }
        if let Some(value) = argument.strip_prefix("--meta=") {
            if metadata.len() < MAX_ENTRIES {
                metadata.push(parse_metadata(value)?);
            }
            continue;
        }
        if let Some(value) = argument.strip_prefix("--metafile=") {
            metadata_file = Some(PathBuf::from(value));
            continue;
        }
        if let Some(value) = argument.strip_prefix("--section=") {
            if matches!(mode, Mode::Mux) {
                if sections.len() < MAX_ENTRIES {
                    sections.push(parse_section(value)?);
                }
            } else if matches!(mode, Mode::Extract | Mode::Verify) {
                section = Some(value.to_owned());
            }
            continue;
        }
        if let Some(value) = argument.strip_prefix("--sections=") {
            if matches!(mode, Mode::Info) {
                info.show_sections = true;
            } else {
                section_file = Some(PathBuf::from(value));
            }
            continue;
        }
        if let Some(value) = argument.strip_prefix("--alignment=") {
            config.alignment = parse_number("alignment", value)?;
            continue;
        }
        if let Some(value) = argument.strip_prefix("--ream-size=") {
            config.ream_size = parse_number("ream-size", value)?;
            continue;
        }
        if let Some(value) = argument.strip_prefix("--asset=") {
            if matches!(mode, Mode::Extract | Mode::Verify) {
                // The reference stores this in the extraction union member
                // even during verify mode; verify consequently ignores it.
                asset = Some(parse_number("asset", value)? as u64);
            }
            continue;
        }
        if let Some(value) = argument.strip_prefix("--petrify=") {
            mode = Mode::Petrify;
            petrify_output = Some(PathBuf::from(value));
            petrify_union_section = Some(value.to_owned());
            // The reference stores the output pointer in the mode union. If
            // --info follows, its showMeta byte overlaps that pointer and is
            // observed as enabled on the supported platform.
            petrify_output_sets_info_meta = true;
            continue;
        }
        if let Some(value) = argument.strip_prefix("--rangekey=") {
            let value = value.to_owned();
            // The reference writes --rangekey into the extraction union. Its
            // pointer-sized slot overlaps verify.assetIndex, so the option
            // remains observable as an invalid asset selection in verify mode.
            verify_asset_index_alias = Some(if value.is_empty() {
                u64::MAX
            } else {
                value.as_ptr() as usize as u64
            });
            range_key = Some(value);
            continue;
        }
        if let Some(value) = argument.strip_prefix("--outdir=") {
            outdir = Some(PathBuf::from(value));
            continue;
        }
        if let Some(value) = argument.strip_prefix("--write-meta=") {
            write_meta = Some(PathBuf::from(if value.is_empty() {
                "path.txt"
            } else {
                value
            }));
            continue;
        }
        if let Some(value) = argument.strip_prefix("--write-hashes=") {
            write_hashes = Some(PathBuf::from(if value.is_empty() {
                "hashes.txt"
            } else {
                value
            }));
            continue;
        }
        match argument.as_str() {
            "--info" => {
                mode = Mode::Info;
                if petrify_output_sets_info_meta {
                    info.show_meta = true;
                }
            }
            "--verify" => {
                mode = Mode::Verify;
                if let Some(alias) = &petrify_union_section {
                    section = Some(alias.clone());
                }
            }
            "--extract" => {
                mode = Mode::Extract;
                if let Some(alias) = &petrify_union_section {
                    section = Some(alias.clone());
                }
            }
            "--petrify" => mode = Mode::Petrify,
            "--meta" if metadata.len() < MAX_ENTRIES => metadata.push(parse_metadata("")?),
            "--metafile" => metadata_file = Some(PathBuf::new()),
            "--section" if matches!(mode, Mode::Mux) && sections.len() < MAX_ENTRIES => {
                sections.push(parse_section("")?)
            }
            "--section" if matches!(mode, Mode::Extract | Mode::Verify) => {
                section = Some(String::new())
            }
            "--section" => {}
            "--sections" if matches!(mode, Mode::Info) => info.show_sections = true,
            "--sections" if matches!(mode, Mode::Mux) => section_file = Some(PathBuf::new()),
            "--sections" => {}
            "--alignment" => config.alignment = parse_number("alignment", "")?,
            "--ream-size" => config.ream_size = parse_number("ream-size", "")?,
            "--asset" if matches!(mode, Mode::Extract | Mode::Verify) => {
                asset = Some(parse_number("asset", "")? as u64)
            }
            "--asset" => {}
            "--rangekey" => {
                let value = String::new();
                // The C++ pointer still points into argv for a bare option;
                // use a nonzero out-of-range surrogate for Rust's empty
                // String representation, whose dangling pointer is 1.
                verify_asset_index_alias = Some(u64::MAX);
                range_key = Some(value);
            }
            "--outdir" => outdir = Some(PathBuf::new()),
            "--hashes" => info.show_hashes = true,
            "--footer" => info.show_footer = true,
            "--counts" => info.show_counts = true,
            "--header" => info.show_header = true,
            "--metadata" => info.show_meta = true,
            "--offsets" => info.show_offsets = true,
            "--pages" | "--strings" => {}
            "--write-meta" => write_meta = Some(PathBuf::from("path.txt")),
            "--write-hashes" => write_hashes = Some(PathBuf::from("hashes.txt")),
            "--variable-ream-size" => config.header_flags |= VARIABLE_REAM_SIZE_FLAG,
            option if option.starts_with('-') => {}
            value => positional.push(PathBuf::from(value)),
        }
    }

    let input = positional.first().cloned().ok_or_else(|| match mode {
        Mode::Mux | Mode::Info => CliError::MissingInput(mode),
        _ => CliError::Usage("missing input path".to_owned()),
    })?;
    let output = if matches!(mode, Mode::Petrify) {
        petrify_output
    } else {
        positional.get(1).cloned()
    };
    if matches!(mode, Mode::Mux) && output.is_none() {
        return Err(CliError::Usage(
            "mux mode requires an output file".to_owned(),
        ));
    }

    Ok(Arguments {
        mode,
        input,
        output,
        config,
        metadata,
        sections,
        metadata_file,
        section_file,
        info,
        asset,
        section,
        range_key,
        verify_asset_index_alias,
        outdir,
        write_meta,
        write_hashes,
    })
}

fn parse_number(name: &str, value: &str) -> Result<u32, CliError> {
    // The reference uses atoi(), which consumes leading ASCII whitespace and
    // the longest decimal prefix, then returns zero when no digits exist.
    // Keep that useful compatibility for defined non-negative values while
    // rejecting negative and overflowing inputs instead of reproducing the
    // reference's unsigned-conversion/shift hazards.
    let value = value.trim_start_matches(|character: char| character.is_ascii_whitespace());
    let negative = value.starts_with('-');
    let digits = value
        .strip_prefix(['+', '-'])
        .unwrap_or(value)
        .bytes()
        .take_while(u8::is_ascii_digit)
        .collect::<Vec<_>>();

    if digits.is_empty() {
        return Ok(0);
    }

    let parsed = digits.into_iter().try_fold(0u32, |number, digit| {
        number
            .checked_mul(10)
            .and_then(|number| number.checked_add(u32::from(digit - b'0')))
    });
    let Some(parsed) = parsed else {
        return Err(CliError::Usage(format!("invalid {name}: {value}")));
    };
    if negative && parsed != 0 {
        return Err(CliError::Usage(format!("invalid {name}: {value}")));
    }
    Ok(parsed)
}

fn split_escaped_bytes(value: &[u8]) -> Vec<Vec<u8>> {
    let mut fields = vec![Vec::new()];
    let mut previous_was_backslash = false;
    for &byte in value {
        if byte == b':' && !previous_was_backslash && fields.len() < 3 {
            fields.push(Vec::new());
        } else {
            fields.last_mut().expect("field exists").push(byte);
        }
        previous_was_backslash = byte == b'\\';
    }
    fields
}

fn parse_metadata(value: &str) -> Result<MetadataSpec, CliError> {
    Ok(parse_metadata_bytes(value.as_bytes()))
}

fn parse_metadata_bytes(value: &[u8]) -> MetadataSpec {
    let fields = split_escaped_bytes(value);
    MetadataSpec {
        key: fields[0].clone(),
        value: fields.get(1).cloned(),
        parent: fields.get(2).cloned(),
    }
}

fn parse_section(value: &str) -> Result<SectionSpec, CliError> {
    Ok(parse_section_bytes(value.as_bytes()))
}

fn parse_section_bytes(value: &[u8]) -> SectionSpec {
    let fields = split_escaped_bytes(value);
    SectionSpec {
        name: fields[0].clone(),
        target: fields.get(1).cloned(),
        parent: fields.get(2).cloned(),
    }
}

fn read_metadata_file(path: &std::path::Path) -> Result<Vec<MetadataSpec>, CliError> {
    Ok(reference_config_records(&fs::read(path)?)
        .into_iter()
        .map(|line| parse_file_metadata_bytes(&line))
        .collect())
}

fn read_section_file(path: &std::path::Path) -> Result<Vec<SectionSpec>, CliError> {
    Ok(reference_config_records(&fs::read(path)?)
        .into_iter()
        .map(|line| parse_file_section_bytes(&line))
        .collect())
}

fn parse_file_metadata_bytes(value: &[u8]) -> MetadataSpec {
    let mut metadata = parse_metadata_bytes(value);
    truncate_file_parent(metadata.parent.as_mut());
    metadata
}

fn parse_file_section_bytes(value: &[u8]) -> SectionSpec {
    let mut section = parse_section_bytes(value);
    truncate_file_parent(section.parent.as_mut());
    section
}

fn truncate_file_parent(parent: Option<&mut Vec<u8>>) {
    let Some(parent) = parent else {
        return;
    };
    let mut previous_was_backslash = false;
    for index in 0..parent.len() {
        if parent[index] == b':' && !previous_was_backslash {
            parent.truncate(index);
            break;
        }
        previous_was_backslash = parent[index] == b'\\';
    }
}

fn reference_config_records(contents: &[u8]) -> Vec<Vec<u8>> {
    let mut records = Vec::new();
    let mut cursor = 0;
    while cursor < contents.len() && contents[cursor] != 0 {
        // The reference compares its signed `char` to 32, so bytes with the
        // high bit set are also skipped at the beginning of each record on
        // this platform. Preserve that observable file-parser behavior.
        while cursor < contents.len() && (contents[cursor] <= b' ' || contents[cursor] >= 0x80) {
            cursor += 1;
        }
        if cursor >= contents.len() || contents[cursor] == 0 {
            break;
        }
        let start = cursor;
        while cursor < contents.len()
            && contents[cursor] != 0
            && contents[cursor] != b'\n'
            && contents[cursor] != b'\r'
        {
            cursor += 1;
        }
        records.push(contents[start..cursor].to_vec());
        if cursor < contents.len() && contents[cursor] != 0 {
            cursor += 1;
        }
    }
    records
}

fn mux(arguments: &Arguments) -> Result<(), CliError> {
    let output = arguments
        .output
        .as_ref()
        .expect("argument parser requires mux output");
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(&arguments.input) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|file_type| file_type.is_file()) {
                files.push(entry.path());
            }
        }
    }
    files.sort();

    let mut builder = FileBuilder::with_config(output, arguments.config)?;
    for file in &files {
        if let Err(error) = builder.add_page(file, 0, 0) {
            if matches!(error, BuilderError::Io(_)) {
                eprintln!("[BBFCODEC] Unable to open {} for reading.", file.display());
                continue;
            }
            return Err(error.into());
        }
    }
    let mut metadata = arguments.metadata.clone();
    if let Some(path) = &arguments.metadata_file {
        match read_metadata_file(path) {
            Ok(entries) => metadata.extend(
                entries
                    .into_iter()
                    .take(MAX_ENTRIES.saturating_sub(metadata.len())),
            ),
            Err(CliError::Io(_)) => {
                println!("[BBFMUX] Unable to read text file: {}", path.display());
            }
            Err(error) => return Err(error),
        }
    }
    for metadata in &metadata {
        if let Some(value) = metadata.value.as_deref() {
            builder.add_meta_bytes(&metadata.key, value, metadata.parent.as_deref());
        }
    }
    let mut sections = arguments.sections.clone();
    if let Some(path) = &arguments.section_file {
        match read_section_file(path) {
            Ok(entries) => sections.extend(
                entries
                    .into_iter()
                    .take(MAX_ENTRIES.saturating_sub(sections.len())),
            ),
            Err(CliError::Io(_)) => {
                println!("[BBFMUX] Unable to read text file: {}", path.display());
            }
            Err(error) => return Err(error),
        }
    }
    let reference_target_count = sections.len();
    for section in &sections {
        let target = resolve_target(section.target.as_deref(), &files, reference_target_count)?;
        if !builder.add_section_bytes(&section.name, target, section.parent.as_deref()) {
            write_stdout_line(&[
                b"[BBFCODEC] Cannot add section ",
                &section.name,
                b": Index out of bounds.",
            ])?;
        }
    }
    match builder.finalize() {
        Ok(()) => {}
        Err(BuilderError::NoAssets) => print!("[BBFCODEC] No assets to finalize."),
        Err(error) => return Err(error.into()),
    }
    println!("Muxed {} files to '{}'...", files.len(), output.display());
    Ok(())
}

fn resolve_target(
    target: Option<&[u8]>,
    files: &[PathBuf],
    reference_count: usize,
) -> Result<u64, CliError> {
    let Some(target) = target else {
        return Ok(0);
    };
    if target.is_empty() {
        return Ok(0);
    }
    if target.iter().all(u8::is_ascii_digit) {
        return Ok(parse_decimal_bytes(target));
    }
    if let Some(index) = files
        .iter()
        .take(reference_count)
        .position(|file| file_name_bytes(file).is_some_and(|name| name == target))
    {
        return Ok(index as u64);
    }
    write_stdout_line(&[b"Warning: Could not resolve target '", target, b"'"])?;
    Ok(0)
}

fn parse_decimal_bytes(value: &[u8]) -> u64 {
    value.iter().fold(0u64, |number, digit| {
        number
            .checked_mul(10)
            .and_then(|number| number.checked_add(u64::from(digit - b'0')))
            .unwrap_or(u64::MAX)
    })
}

#[cfg(unix)]
fn file_name_bytes(path: &Path) -> Option<&[u8]> {
    use std::os::unix::ffi::OsStrExt;

    path.file_name().map(OsStrExt::as_bytes)
}

#[cfg(not(unix))]
fn file_name_bytes(path: &Path) -> Option<&[u8]> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::as_bytes)
}

fn info(arguments: &Arguments) -> Result<(), CliError> {
    let mapping = match OwnedFile::open(&arguments.input) {
        Ok(mapping) => mapping,
        Err(MappedFileError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(CliError::InfoMissingFile(arguments.input.clone()));
        }
        Err(error) => return Err(error.into()),
    };
    let reader = mapping.reader();
    let header = reader.header()?;
    let indexed = reader
        .indexed()
        .map_err(|_| CliError::InfoFooterUnavailable)?;
    let footer = indexed.footer();

    if arguments.info.show_header {
        println!("\n=== HEADER ===");
        println!("Signature:    BBF3");
        println!("Version:      {}", header.version);
        println!("Flags:        0x{:08X}", header.flags);
        println!(
            "  [{}] Petrified (Linearized)",
            if header.flags & bbf_format::PETRIFICATION_FLAG != 0 {
                'x'
            } else {
                ' '
            }
        );
        println!(
            "  [{}] Variable Alignment (Reams)",
            if header.flags & VARIABLE_REAM_SIZE_FLAG != 0 {
                'x'
            } else {
                ' '
            }
        );
        println!(
            "Alignment:    {} (Pow2) -> {} bytes",
            header.alignment,
            1u64 << header.alignment
        );
        println!(
            "Ream Size:    {} (Pow2) -> {} bytes",
            header.ream_size,
            1u64 << header.ream_size
        );
        println!("Footer Offset:   {}", header.footer_offset);
    }

    if arguments.info.show_footer {
        println!("\n=== FOOTER ===");
        print_footer_offsets(footer, true);
        print_footer_counts(footer, true);
        println!();
        println!("  Footer Hash (Index Hash): 0x{:016x}", footer.footer_hash);
    }

    if arguments.info.show_counts {
        println!("\n=== Counts ===");
        print_footer_counts(footer, false);
    }

    if arguments.info.show_offsets {
        println!("\n=== Offsets ===");
        print_footer_offsets(footer, false);
    }

    if arguments.info.show_meta {
        println!("\n=== Metadata ===");
        for index in 0..footer.metadata_count {
            let entry = indexed
                .metadata(index)?
                .ok_or_else(|| CliError::Verification("metadata table ended early".to_owned()))?;
            let key = indexed
                .string_bytes(entry.key_offset)
                .unwrap_or(None)
                .unwrap_or(b"<CORRUPT KEY>");
            let value = indexed
                .string_bytes(entry.value_offset)
                .unwrap_or(None)
                .unwrap_or(b"<CORRUPT VALUE>");
            write_stdout_line(&[key, b" : ", value])?;
            if entry.parent_offset != bbf_format::NO_PARENT_OFFSET {
                let parent = indexed
                    .string_bytes(entry.parent_offset)
                    .unwrap_or(None)
                    .unwrap_or(b"<INVALID>");
                write_stdout_line(&[b"     (Parent Key: ", parent, b")"])?;
            }
        }
    }

    if arguments.info.show_sections {
        println!("\n=== Sections ===");
        for index in 0..footer.section_count {
            let section = indexed
                .section(index)?
                .ok_or_else(|| CliError::Verification("section table ended early".to_owned()))?;
            let name = indexed
                .string_bytes(section.title_offset)
                .unwrap_or(None)
                .unwrap_or(b"<CORRUPT KEY>");
            let start_index = section.start_index.to_string();
            write_stdout_line(&[name, b" : ", start_index.as_bytes()])?;
            if section.parent_offset != bbf_format::NO_PARENT_OFFSET {
                let parent = indexed
                    .string_bytes(section.parent_offset)
                    .unwrap_or(None)
                    .unwrap_or(b"<INVALID>");
                write_stdout_line(&[b"(Parent Section: ", parent, b")"])?;
            }
        }
    }

    if arguments.info.show_hashes {
        println!("\n=== ASSET TABLE ({} entries) ===", footer.asset_count);
        println!("ID  | Hash (XXH3-128)                  | Offset      | Size     | Type");
        println!("----|----------------------------------|-------------|----------|-----");
        for index in 0..footer.asset_count {
            let asset = indexed
                .asset(index)?
                .ok_or_else(|| CliError::Verification("asset table ended early".to_owned()))?;
            println!(
                "{index:3} | {:016x}{:016x} | {:11} | {:8} | 0x{:02X}",
                asset.hash_high,
                asset.hash_low,
                asset.file_offset,
                asset.file_size,
                asset.media_type
            );
        }
    }
    Ok(())
}

fn print_footer_offsets(footer: &bbf_format::Footer, include_label: bool) {
    if include_label {
        println!("Offsets:");
    }
    println!("  Assets:   0x{:016x}", footer.asset_offset);
    println!("  Pages:    0x{:016x}", footer.page_offset);
    println!("  Sections: 0x{:016x}", footer.section_offset);
    println!("  Meta:     0x{:016x}", footer.metadata_offset);
    println!("  Expansion:0x{:016x}", footer.expansion_offset);
    println!("  Strings:  0x{:016x}", footer.string_pool_offset);
}

fn print_footer_counts(footer: &bbf_format::Footer, include_label: bool) {
    if include_label {
        println!("Counts:");
    }
    println!("  Assets:   {}", footer.asset_count);
    println!("  Pages:    {}", footer.page_count);
    println!("  Metadata: {}", footer.metadata_count);
    println!("  Sections: {}", footer.section_count);
}

fn write_stdout_line(parts: &[&[u8]]) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    for part in parts {
        stdout.write_all(part)?;
    }
    stdout.write_all(b"\n")?;
    Ok(())
}

fn verify(arguments: &Arguments) -> Result<(), CliError> {
    let mapping = OwnedFile::open(&arguments.input)?;
    let reader = mapping.reader();
    let indexed = reader.indexed()?;

    let footer = indexed.footer();
    if let Some(index) = arguments.verify_asset_index_alias
        && index > footer.asset_count
    {
        return Err(CliError::VerifyAssetIndexOutOfBounds {
            index,
            max: footer.asset_count,
        });
    }
    let (start_page, end_page) = if let Some(section_name) = &arguments.section {
        find_verify_section_range(&indexed, section_name, footer.page_count)?
    } else {
        (0, footer.page_count)
    };
    for page_index in start_page..end_page {
        let page = indexed
            .page(page_index)?
            .ok_or_else(|| CliError::Verification(format!("page {page_index} is missing")))?;
        let asset_index = page.asset_index;
        let asset = indexed
            .asset(asset_index)?
            .ok_or_else(|| CliError::Verification(format!("asset {asset_index} is missing")))?;
        let (hash_low, hash_high) = indexed
            .compute_asset_hash(asset_index)?
            .ok_or_else(|| CliError::Verification(format!("asset {asset_index} is missing")))?;
        if hash_low != asset.hash_low || hash_high != asset.hash_high {
            println!("[BBFMUX] [{page_index} | FAIL] Hash Mismatch (Asset: {asset_index}).");
            println!("Computed Hash: {hash_high:x}{hash_low:x}");
            println!("Asset Hash: {:x}{:x}", asset.hash_high, asset.hash_low);
        } else {
            println!("[BBFMUX] [{page_index} | OK] Hashes Match ({hash_high:x}{hash_low:x})");
        }
    }
    println!("[BBFMUX] Finished Verifying Hashes");
    Ok(())
}

fn find_verify_section_range<'a, D: AsRef<[u8]>>(
    reader: &IndexedReader<'a, D>,
    section_name: &str,
    page_count: u64,
) -> Result<(u64, u64), CliError> {
    let footer = reader.footer();
    let mut start = None;
    let mut end = page_count;
    for index in 0..footer.section_count {
        let section = reader
            .section(index)?
            .ok_or_else(|| CliError::Verification("section table ended early".to_owned()))?;
        let name = reader.string(section.title_offset)?.unwrap_or("");
        if start.is_none() && name == section_name {
            start = Some(section.start_index);
            continue;
        }
        if start.is_some() {
            let is_child = section.parent_offset != bbf_format::NO_PARENT_OFFSET
                && reader
                    .string(section.parent_offset)?
                    .is_some_and(|parent| parent == section_name);
            if !is_child {
                end = section.start_index;
                break;
            }
        }
    }
    let start = start.ok_or_else(|| CliError::VerifySectionNotFound(section_name.to_owned()))?;
    if end <= start || end > page_count {
        return Err(CliError::VerifySectionHasNoPages(section_name.to_owned()));
    }
    Ok((start, end))
}

fn extract(arguments: &Arguments) -> Result<(), CliError> {
    let mapping = OwnedFile::open(&arguments.input)?;
    let reader = mapping.reader();
    let indexed = reader.indexed()?;
    let footer = indexed.footer();
    if let Some(hashes_path) = &arguments.write_hashes {
        write_hash_report(&indexed, hashes_path, footer.asset_count)?;
    }
    if let Some(metadata_path) = &arguments.write_meta {
        write_metadata_report(&indexed, metadata_path, footer.metadata_count)?;
    }

    let section_range = if let Some(section_name) = &arguments.section {
        let Some(range_key) = arguments.range_key.as_deref() else {
            return Err(CliError::ExtractRangeKeyRequired);
        };
        Some(find_section_range(
            &indexed,
            section_name,
            range_key,
            footer.page_count,
        )?)
    } else {
        None
    };

    if let Some(asset_index) = arguments.asset
        && section_range.is_none()
    {
        extract_asset(
            &indexed,
            arguments.outdir.as_deref(),
            asset_index,
            asset_index,
            true,
            false,
        )?;
        return Ok(());
    }

    if let Some((start_page, end_page)) = section_range {
        for page_index in start_page..end_page {
            let page = indexed
                .page(page_index)?
                .ok_or_else(|| CliError::Verification(format!("page {page_index} is missing")))?;
            extract_section_asset(
                &indexed,
                arguments.outdir.as_deref(),
                page.asset_index,
                page_index,
            )?;
        }
    } else {
        // The reference extraction union is zero-initialized, so bare
        // --extract follows the same path as --asset=0. This is observable
        // even when the input has multiple distinct assets.
        extract_asset(&indexed, arguments.outdir.as_deref(), 0, 0, true, false)?;
    }
    Ok(())
}

fn extract_asset<'a, D: AsRef<[u8]>>(
    reader: &IndexedReader<'a, D>,
    output_dir: Option<&std::path::Path>,
    asset_index: u64,
    output_index: u64,
    announce: bool,
    section_style: bool,
) -> Result<(), CliError> {
    let asset = reader
        .asset(asset_index)?
        .ok_or(CliError::ExtractAssetIndexOutOfBounds)?;
    let data = reader
        .asset_data(&asset)
        .ok_or_else(|| CliError::Verification(format!("asset {asset_index} is out of bounds")))?;
    let filename = format!("page_{output_index}{}", media_extension(asset.media_type));
    let path = output_dir
        .map(|directory| directory.join(&filename))
        .unwrap_or_else(|| PathBuf::from(&filename));
    if announce {
        println!(
            "[BBFMUX] Extracting asset {asset_index} to {}",
            path.display()
        );
    }
    match fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
    {
        Ok(mut file) => {
            // The reference checks fopen but does not check fwrite. Keep a
            // successful open a successful extraction boundary.
            let _ = file.write_all(data);
        }
        Err(_) => {
            if section_style {
                println!("Failed to write {}", path.display());
            } else {
                println!("[BBFMUX] Failed to write file: {}", path.display());
            }
        }
    }
    Ok(())
}

fn extract_section_asset<'a, D: AsRef<[u8]>>(
    reader: &IndexedReader<'a, D>,
    output_dir: Option<&std::path::Path>,
    asset_index: u64,
    output_index: u64,
) -> Result<(), CliError> {
    if reader.asset(asset_index)?.is_none() {
        return Ok(());
    }
    extract_asset(reader, output_dir, asset_index, output_index, false, true)
}

fn find_section_range<'a, D: AsRef<[u8]>>(
    reader: &IndexedReader<'a, D>,
    section_name: &str,
    _range_key: &str,
    page_count: u64,
) -> Result<(u64, u64), CliError> {
    let footer = reader.footer();
    let mut start = None;
    let mut end = page_count;
    for index in 0..footer.section_count {
        let section = reader
            .section(index)?
            .ok_or_else(|| CliError::Verification("section table ended early".to_owned()))?;
        let name = reader.string(section.title_offset)?.unwrap_or("<INVALID>");
        if start.is_none() && name.contains(section_name) {
            start = Some(section.start_index);
            continue;
        }
        if start.is_some() {
            let is_child = section.parent_offset != bbf_format::NO_PARENT_OFFSET
                && reader
                    .string(section.parent_offset)?
                    .is_some_and(|parent| parent == section_name);
            if !is_child {
                end = section.start_index;
                break;
            }
        }
    }
    // The reference leaves the initial range unchanged when no section matches:
    // extraction therefore falls back to every page and still reports success.
    let Some(start) = start else {
        println!(
            "[BBFMUX] Extracting Section '{section_name}' (Pages 0 - {})",
            page_count.saturating_sub(1)
        );
        return Ok((0, page_count));
    };
    if end < start || end > page_count {
        return Err(CliError::Verification(
            "invalid section page range".to_owned(),
        ));
    }
    println!(
        "[BBFMUX] Extracting Section '{section_name}' (Pages {start} - {})",
        end.saturating_sub(1)
    );
    Ok((start, end))
}

fn write_metadata_report<'a, D: AsRef<[u8]>>(
    reader: &IndexedReader<'a, D>,
    path: &std::path::Path,
    count: u64,
) -> Result<(), CliError> {
    let mut report = b"=== Metadata ===\n".to_vec();
    for index in 0..count {
        let metadata = reader
            .metadata(index)?
            .ok_or_else(|| CliError::Verification("metadata table ended early".to_owned()))?;
        let key = reader
            .string_bytes(metadata.key_offset)?
            .unwrap_or(b"<CORRUPT KEY>");
        let value = reader
            .string_bytes(metadata.value_offset)?
            .unwrap_or(b"<CORRUPT VALUE>");
        report.extend_from_slice(key);
        report.extend_from_slice(b" : ");
        report.extend_from_slice(value);
        report.push(b'\n');
        if metadata.parent_offset != bbf_format::NO_PARENT_OFFSET {
            let parent = reader
                .string_bytes(metadata.parent_offset)?
                .unwrap_or(b"<INVALID>");
            report.extend_from_slice(b"     (Parent Key: ");
            report.extend_from_slice(parent);
            report.extend_from_slice(b")\n");
        }
    }
    write_report(path, &report)?;
    Ok(())
}

fn write_hash_report<'a, D: AsRef<[u8]>>(
    reader: &IndexedReader<'a, D>,
    path: &std::path::Path,
    count: u64,
) -> Result<(), CliError> {
    let mut report = format!(
        "=== ASSET TABLE ({count} entries) ===\nID  | Hash (XXH3-128)                  | Offset      | Size     | Type\n----|----------------------------------|-------------|----------|-----\n"
    );
    for index in 0..count {
        let asset = reader
            .asset(index)?
            .ok_or_else(|| CliError::Verification("asset table ended early".to_owned()))?;
        report.push_str(&format!(
            "{index:3} | {:016x}{:016x} | {:11} | {:8} | 0x{:02X}\n",
            asset.hash_high, asset.hash_low, asset.file_offset, asset.file_size, asset.media_type
        ));
    }
    write_report(path, report.as_bytes())?;
    Ok(())
}

fn write_report(path: &Path, report: &[u8]) -> Result<(), CliError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|_| CliError::ExtractReportOpen(path.to_owned()))?;
    // The reference checks the report open but does not check each fwrite.
    // Preserve that observable success boundary after a successful open.
    let _ = file.write_all(report);
    Ok(())
}

fn media_extension(media_type: u8) -> &'static str {
    match media_type {
        0x01 => ".avif",
        0x02 => ".png",
        0x03 => ".webp",
        0x04 => ".jxl",
        0x05 => ".bmp",
        0x07 => ".gif",
        0x08 => ".tiff",
        0x09 => ".jpg",
        _ => ".dat",
    }
}

fn petrify(arguments: &Arguments) -> Result<(), CliError> {
    let Some(output) = arguments.output.as_ref() else {
        return Err(CliError::NoPetrifyOutput);
    };
    println!(
        "[BBFMUX] Petrifying {} to {}...",
        arguments.input.display(),
        output.display()
    );
    match Builder::petrify_file_compat(&arguments.input, output) {
        Ok(()) => {
            println!("[BBFMUX] Success.");
            Ok(())
        }
        Err(error) => {
            println!("[BBFMUX] Failed to petrify {}.", arguments.input.display());
            Err(error.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, process};

    use bbf_format::MediaType;
    use bbf_io::Reader;
    use bbf_mux::Builder;

    use super::{
        Mode, parse_args, parse_metadata, parse_section, read_metadata_file, read_section_file,
    };

    #[test]
    fn parses_reference_style_mux_options() {
        let args = parse_args([
            "pages".to_owned(),
            "book.bbf".to_owned(),
            "--variable-ream-size".to_owned(),
            "--meta=Title:Example:Book".to_owned(),
            "--section=Chapter\\:1:001.png:Book".to_owned(),
        ])
        .unwrap();

        assert_eq!(args.mode, Mode::Mux);
        assert_eq!(args.output.unwrap().to_str(), Some("book.bbf"));
        assert_eq!(args.metadata[0].parent.as_deref(), Some(b"Book".as_slice()));
        assert_eq!(args.sections[0].name, b"Chapter\\:1");
        assert_eq!(args.metadata_file, None);
        assert_eq!(args.section_file, None);
        assert_ne!(
            args.config.header_flags & bbf_format::VARIABLE_REAM_SIZE_FLAG,
            0
        );
    }

    #[test]
    fn caps_inline_metadata_and_sections_at_reference_entry_limit() {
        let mut arguments = vec!["pages".to_owned(), "book.bbf".to_owned()];
        for index in 0..300 {
            arguments.push(format!("--meta=Key{index}:Value{index}"));
            arguments.push(format!("--section=Section{index}:0"));
        }

        let args = parse_args(arguments).expect("reference entry limit parses");
        assert_eq!(args.metadata.len(), 256);
        assert_eq!(args.sections.len(), 256);
    }

    #[test]
    fn accepts_reference_unknown_and_bare_mux_options() {
        let args = parse_args([
            "pages".to_owned(),
            "book.bbf".to_owned(),
            "--unknown-option=ignored".to_owned(),
            "--meta".to_owned(),
            "--metafile".to_owned(),
            "--section".to_owned(),
            "--sections".to_owned(),
            "--alignment".to_owned(),
            "--ream-size".to_owned(),
            "--asset".to_owned(),
            "--rangekey".to_owned(),
            "--outdir".to_owned(),
        ])
        .expect("reference-compatible options parse");

        assert_eq!(args.metadata[0].value, None);
        assert_eq!(args.metadata_file, Some(PathBuf::new()));
        assert_eq!(args.sections[0].target, None);
        assert_eq!(args.section_file, Some(PathBuf::new()));
        assert_eq!(args.config.alignment, 0);
        assert_eq!(args.config.ream_size, 0);
        assert_eq!(args.asset, None);
        assert_eq!(args.range_key.as_deref(), Some(""));
        assert_eq!(args.outdir, Some(PathBuf::new()));
    }

    #[test]
    fn parses_file_backed_metadata_and_sections() {
        let args = parse_args([
            "pages".to_owned(),
            "book.bbf".to_owned(),
            "--metafile=metadata.txt".to_owned(),
            "--sections=sections.txt".to_owned(),
        ])
        .unwrap();

        assert_eq!(args.metadata_file.unwrap().to_str(), Some("metadata.txt"));
        assert_eq!(args.section_file.unwrap().to_str(), Some("sections.txt"));
    }

    #[test]
    fn parses_petrify_output_and_asset_selection() {
        let args = parse_args([
            "book.bbf".to_owned(),
            "--petrify=linear.bbf".to_owned(),
            "--asset=3".to_owned(),
        ])
        .unwrap();

        assert_eq!(args.mode, Mode::Petrify);
        assert_eq!(args.output.unwrap().to_str(), Some("linear.bbf"));
        assert_eq!(args.asset, None);
    }

    #[test]
    fn keeps_bare_petrify_without_a_reference_output_path() {
        let args = parse_args(["book.bbf".to_owned(), "--petrify".to_owned()]).unwrap();
        assert_eq!(args.mode, Mode::Petrify);
        assert_eq!(args.output, None);

        let args = parse_args([
            "book.bbf".to_owned(),
            "positional-output.bbf".to_owned(),
            "--petrify".to_owned(),
        ])
        .unwrap();
        assert_eq!(args.output, None);
    }

    #[test]
    fn parses_extraction_reports_and_section_range() {
        let args = parse_args([
            "book.bbf".to_owned(),
            "--extract".to_owned(),
            "--section=Chapter 1".to_owned(),
            "--rangekey=Chapter 4".to_owned(),
            "--outdir=pages".to_owned(),
            "--write-meta=metadata.txt".to_owned(),
            "--write-hashes=hashes.txt".to_owned(),
        ])
        .unwrap();

        assert_eq!(args.mode, Mode::Extract);
        assert_eq!(args.section.as_deref(), Some("Chapter 1"));
        assert_eq!(args.range_key.as_deref(), Some("Chapter 4"));
        assert_eq!(args.outdir.unwrap().to_str(), Some("pages"));
    }

    #[test]
    fn ignores_sections_parsed_while_reference_mode_cannot_consume_them() {
        let args = parse_args([
            "book.bbf".to_owned(),
            "--info".to_owned(),
            "--section=Ignored".to_owned(),
            "--extract".to_owned(),
        ])
        .unwrap();
        assert_eq!(args.mode, Mode::Extract);
        assert_eq!(args.section, None);

        let args = parse_args([
            "book.bbf".to_owned(),
            "--petrify=linear.bbf".to_owned(),
            "--section=Ignored".to_owned(),
        ])
        .unwrap();
        assert_eq!(args.mode, Mode::Petrify);
        assert_eq!(args.section, None);
    }

    #[test]
    fn preserves_reference_missing_metadata_and_section_fields() {
        assert_eq!(
            parse_metadata("only-key").unwrap().value,
            None,
            "reference drops metadata without a value during muxing"
        );
        assert_eq!(
            parse_section("missing-target").unwrap().target,
            None,
            "reference resolves a section without a target to page zero"
        );
    }

    #[test]
    fn parses_the_reference_decimal_prefix_without_unsigned_hazards() {
        assert_eq!(
            super::parse_number("alignment", " 12 trailing").unwrap(),
            12
        );
        assert_eq!(super::parse_number("alignment", "+12").unwrap(), 12);
        assert_eq!(super::parse_number("alignment", "not-a-number").unwrap(), 0);
        assert_eq!(super::parse_number("alignment", "-0").unwrap(), 0);
        assert!(super::parse_number("alignment", "-1").is_err());
    }

    #[test]
    fn preserves_reference_backslashes_while_shielding_colons() {
        let metadata = parse_metadata(r"Key\q:Value\:part").expect("valid metadata");
        assert_eq!(metadata.key, br"Key\q");
        assert_eq!(metadata.value.as_deref(), Some(br"Value\:part".as_slice()));

        let section = parse_section(r"Name\:1:target").expect("valid section");
        assert_eq!(section.name, br"Name\:1");
        assert_eq!(section.target.as_deref(), Some(b"target".as_slice()));
    }

    #[test]
    fn keeps_extra_colons_in_the_reference_parent_field() {
        let metadata = parse_metadata("Key:Value:Parent:Extra").expect("valid metadata");
        assert_eq!(metadata.parent.as_deref(), Some(b"Parent:Extra".as_slice()));

        let section = parse_section("Name:target:Parent:Extra").expect("valid section");
        assert_eq!(section.parent.as_deref(), Some(b"Parent:Extra".as_slice()));
    }

    #[test]
    fn preserves_an_explicitly_empty_reference_parent_field() {
        let metadata = parse_metadata("Key:Value:").expect("valid metadata");
        assert_eq!(metadata.parent.as_deref(), Some(b"".as_slice()));

        let section = parse_section("Name:target:").expect("valid section");
        assert_eq!(section.parent.as_deref(), Some(b"".as_slice()));
    }

    #[test]
    fn file_config_truncates_unescaped_colons_after_parent() {
        let metadata = super::parse_file_metadata_bytes(b"Key:Value:Parent:Extra");
        assert_eq!(metadata.parent.as_deref(), Some(b"Parent".as_slice()));

        let section = super::parse_file_section_bytes(b"Name:target:Parent:Extra");
        assert_eq!(section.parent.as_deref(), Some(b"Parent".as_slice()));

        let escaped = super::parse_file_metadata_bytes(b"Key:Value:Parent\\:Extra");
        assert_eq!(
            escaped.parent.as_deref(),
            Some(b"Parent\\:Extra".as_slice())
        );
    }

    #[test]
    fn file_config_parsing_preserves_trailing_reference_whitespace() {
        let stem = format!("bbfmux-config-whitespace-{}", process::id());
        let metadata_path = std::env::temp_dir().join(format!("{stem}-metadata.txt"));
        let section_path = std::env::temp_dir().join(format!("{stem}-sections.txt"));
        fs::write(&metadata_path, b"\t Title:Value with space  \nOnlyKey\n")
            .expect("write metadata fixture");
        fs::write(
            &section_path,
            b"  Chapter: target  :Parent  \nMissingTarget\n",
        )
        .expect("write section fixture");

        let metadata = read_metadata_file(&metadata_path).expect("read metadata fixture");
        let sections = read_section_file(&section_path).expect("read section fixture");
        let _ = fs::remove_file(metadata_path);
        let _ = fs::remove_file(section_path);

        assert_eq!(metadata[0].key, b"Title");
        assert_eq!(
            metadata[0].value.as_deref(),
            Some(b"Value with space  ".as_slice())
        );
        assert_eq!(metadata[1].key, b"OnlyKey");
        assert_eq!(metadata[1].value, None);
        assert_eq!(sections[0].name, b"Chapter");
        assert_eq!(sections[0].target.as_deref(), Some(b" target  ".as_slice()));
        assert_eq!(sections[0].parent.as_deref(), Some(b"Parent  ".as_slice()));
        assert_eq!(sections[1].name, b"MissingTarget");
        assert_eq!(sections[1].target, None);
    }

    #[test]
    fn section_extraction_ends_at_the_next_non_child_section() {
        let mut builder = Builder::new();
        for payload in [b"page-0".as_slice(), b"page-1", b"page-2"] {
            builder.add_page_bytes(payload, MediaType::Unknown.as_u8(), 0, 0);
        }
        assert!(builder.add_section("Start", 0, None));
        assert!(builder.add_section("Child", 1, Some("Start")));
        assert!(builder.add_section("Other", 2, None));
        let reader = Reader::from_bytes(builder.build_bytes().expect("build section fixture"));
        let indexed = reader.indexed().expect("index section fixture");

        assert_eq!(
            super::find_section_range(&indexed, "Start", "ignored", 3).unwrap(),
            (0, 2)
        );
    }

    #[test]
    fn missing_extraction_section_falls_back_to_all_pages_like_reference() {
        let mut builder = Builder::new();
        for payload in [b"page-0".as_slice(), b"page-1"] {
            builder.add_page_bytes(payload, MediaType::Unknown.as_u8(), 0, 0);
        }
        let reader = Reader::from_bytes(builder.build_bytes().expect("build fixture"));
        let indexed = reader.indexed().expect("index fixture");

        assert_eq!(
            super::find_section_range(&indexed, "Missing", "ignored", 2).unwrap(),
            (0, 2)
        );
    }
}
