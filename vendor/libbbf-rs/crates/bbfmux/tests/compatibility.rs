use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use bbf_format::{HEADER_SIZE, NO_PARENT_OFFSET, PETRIFICATION_FLAG};
use bbf_io::Reader;
use bbf_mux::Builder;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn decode_hex(hex: &str) -> Vec<u8> {
    let hex = hex.trim();
    assert_eq!(hex.len() % 2, 0, "fixture hex must contain complete bytes");
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("valid fixture hex"))
        .collect()
}

fn temp_fixture_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "bbfmux-compat-{}-{suffix}-{counter}",
        std::process::id()
    ))
}

fn raw_pool_entry(reader: &Reader, offset: u64) -> &[u8] {
    let footer = reader.footer().expect("read footer");
    let start = usize::try_from(footer.string_pool_offset + offset).expect("pool offset fits");
    let pool = &reader.bytes()[start..];
    let end = pool
        .iter()
        .position(|byte| *byte == 0)
        .expect("NUL-terminated pool entry");
    &pool[..end]
}

#[test]
fn mux_matches_pinned_cpp_fixture_byte_for_byte() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create temporary fixture directory");
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(
        pages.join("alpha.txt"),
        include_bytes!("fixtures/alpha.txt"),
    )
    .expect("write alpha fixture");
    fs::write(pages.join("beta.dat"), include_bytes!("fixtures/beta.dat"))
        .expect("write beta fixture");
    fs::write(directory.join("metadata.txt"), b"Source:fixture\n")
        .expect("write metadata configuration");
    fs::write(directory.join("sections.txt"), b"Start:0\n").expect("write section configuration");
    let output = directory.join("fixture.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&pages)
        .arg(&output)
        .args([
            "--variable-ream-size",
            "--metafile=metadata.txt",
            "--sections=sections.txt",
        ])
        .output()
        .expect("run Rust bbfmux");
    assert!(
        command.status.success(),
        "bbfmux failed: {}",
        String::from_utf8_lossy(&command.stderr)
    );

    let actual = fs::read(&output).expect("read generated fixture");
    let expected = decode_hex(include_str!("fixtures/cpp_basic.bbf.hex"));
    assert_eq!(
        actual, expected,
        "Rust output diverged from the C++ fixture"
    );

    let reader = Reader::from_bytes(actual);
    assert!(reader.verify_footer_hash().expect("read footer hash"));
    assert_eq!(reader.footer().expect("read footer").asset_count, 2);
    assert_eq!(
        reader
            .asset(0)
            .expect("read first asset")
            .unwrap()
            .file_size,
        22
    );
    assert_eq!(
        reader
            .asset(1)
            .expect("read second asset")
            .unwrap()
            .file_size,
        23
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn mux_matches_reference_section_filename_resolution_limit() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("alpha.txt"), b"alpha page").expect("write first page");
    fs::write(pages.join("beta.dat"), b"beta page").expect("write second page");
    let output = directory.join("section-target.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&pages)
        .arg(&output)
        .arg("--section=Second:beta.dat")
        .output()
        .expect("run Rust bbfmux");
    assert!(command.status.success());
    assert_eq!(
        String::from_utf8(command.stdout).expect("mux stdout is UTF-8"),
        format!(
            "Warning: Could not resolve target 'beta.dat'\nMuxed 2 files to '{}'...\n",
            output.display()
        )
    );

    let reader = Reader::open(&output).expect("read generated BBF");
    assert_eq!(reader.footer().expect("read footer").section_count, 1);
    assert_eq!(reader.section(0).unwrap().unwrap().start_index, 0);

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn missing_input_diagnostics_match_reference_modes() {
    let mux = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .output()
        .expect("run Rust mux without input");
    assert_eq!(mux.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(mux.stdout).expect("mux stdout is UTF-8"),
        "Invalid Syntax. Run bbfmux --help to display avaliable options."
    );
    assert!(mux.stderr.is_empty());

    let info = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .arg("--info")
        .output()
        .expect("run Rust info mode without input");
    assert_eq!(info.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(info.stdout).expect("info stdout is UTF-8"),
        "[BBFMUX] Argument syntax error: missing input file.\n"
    );
    assert!(info.stderr.is_empty());
}

#[test]
fn missing_mode_inputs_remain_bounded_rust_usage_errors() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create missing-input directory");

    for arguments in [
        vec!["--verify".to_owned()],
        vec!["--extract".to_owned()],
        vec!["--petrify=missing-output.bbf".to_owned()],
    ] {
        let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
            .current_dir(&directory)
            .args(arguments)
            .output()
            .expect("run Rust mode without input");
        assert_eq!(command.status.code(), Some(2));
        assert!(command.stdout.is_empty());
        assert!(String::from_utf8_lossy(&command.stderr).starts_with("missing input path\n\n"));
    }

    assert!(!directory.join("missing-output.bbf").exists());
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn help_output_matches_pinned_cpp_contract() {
    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .arg("--help")
        .output()
        .expect("run Rust help mode");
    assert_eq!(command.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(command.stdout).expect("help stdout is UTF-8"),
        include_str!("fixtures/cpp_help.txt")
    );
    assert!(command.stderr.is_empty());
}

#[test]
fn info_missing_file_diagnostics_match_reference() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create info directory");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["missing.bbf", "--info"])
        .output()
        .expect("run Rust info mode");
    assert_eq!(command.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] Unable to read header.\n"
    );
    assert_eq!(
        String::from_utf8(command.stderr).expect("stderr is UTF-8"),
        "[BBFCODEC] Unable to open file missing.bbf\n"
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn info_truncated_footer_diagnostics_match_reference() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create info directory");
    let fixture = decode_hex(include_str!("fixtures/cpp_basic.bbf.hex"));
    fs::write(directory.join("truncated.bbf"), &fixture[..64]).expect("write truncated fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["truncated.bbf", "--info", "--header"])
        .output()
        .expect("run Rust info mode");
    assert_eq!(command.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "Unable to retrieve footer.\n"
    );
    assert!(command.stderr.is_empty());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn info_reserved_footer_bytes_remain_rejected_safely() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create info directory");
    let mut fixture = decode_hex(include_str!("fixtures/cpp_basic.bbf.hex"));
    let footer_offset = Reader::from_bytes(fixture.clone())
        .header()
        .expect("read fixture header")
        .footer_offset as usize;
    fixture[footer_offset + 112] = 1;
    fs::write(directory.join("malformed.bbf"), fixture).expect("write malformed fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["malformed.bbf", "--info", "--counts"])
        .output()
        .expect("run Rust info mode");
    assert_eq!(command.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "Unable to retrieve footer.\n"
    );
    assert!(command.stderr.is_empty());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn verify_truncated_footer_returns_a_bounded_reader_error() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create verify directory");
    let fixture = decode_hex(include_str!("fixtures/cpp_basic.bbf.hex"));
    fs::write(directory.join("truncated.bbf"), &fixture[..64]).expect("write truncated fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["truncated.bbf", "--verify"])
        .output()
        .expect("run Rust verify mode");
    assert_eq!(command.status.code(), Some(1));
    assert!(command.stdout.is_empty());
    assert_eq!(
        String::from_utf8(command.stderr).expect("stderr is UTF-8"),
        "BBF read error: BBF range 324..580 exceeds file size 64\n"
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn verify_asset_option_matches_reference_union_behavior() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create verify directory");
    fs::write(
        directory.join("fixture.bbf"),
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write verify fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["fixture.bbf", "--verify", "--asset=1"])
        .output()
        .expect("run Rust verify mode");
    assert!(command.status.success());
    let stdout = String::from_utf8(command.stdout).expect("verify stdout is UTF-8");
    assert!(stdout.contains("[0 | OK] Hashes Match ("));
    assert!(stdout.contains("[1 | OK] Hashes Match ("));
    assert!(stdout.ends_with("[BBFMUX] Finished Verifying Hashes\n"));
    assert!(command.stderr.is_empty());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn verify_hash_mismatch_reports_failure_and_continues_like_reference() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create verify directory");

    let mut builder = Builder::new();
    builder.add_page_bytes(b"first payload", 0, 0, 0);
    builder.add_page_bytes(b"second payload", 0, 0, 0);
    let mut fixture = builder.build_bytes().expect("build verify fixture");
    let first_asset = Reader::from_bytes(fixture.clone())
        .asset(0)
        .expect("read first asset")
        .expect("first asset exists");
    let first_offset = usize::try_from(first_asset.file_offset).expect("asset offset fits");
    fixture[first_offset] ^= 1;
    fs::write(directory.join("corrupt.bbf"), fixture).expect("write corrupt fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["corrupt.bbf", "--verify"])
        .output()
        .expect("run Rust verify mode");
    assert!(command.status.success());
    let stdout = String::from_utf8(command.stdout).expect("verify stdout is UTF-8");
    assert!(stdout.contains("[BBFMUX] [0 | FAIL] Hash Mismatch (Asset: 0)."));
    assert!(stdout.contains("[BBFMUX] [1 | OK] Hashes Match ("));
    assert!(stdout.ends_with("[BBFMUX] Finished Verifying Hashes\n"));
    assert!(command.stderr.is_empty());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn verify_missing_section_diagnostics_match_reference() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create verify directory");
    fs::write(
        directory.join("fixture.bbf"),
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write verify fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["fixture.bbf", "--verify", "--section=Nope"])
        .output()
        .expect("run Rust section verification");
    assert_eq!(command.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] Unable to find section with title: Nope\n"
    );
    assert!(command.stderr.is_empty());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn verify_empty_section_diagnostics_match_reference() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create verify directory");

    let mut builder = Builder::new();
    builder.add_page_bytes(b"first", 0, 0, 0);
    builder.add_page_bytes(b"second", 0, 0, 0);
    assert!(builder.add_section("End", 2, None));
    fs::write(
        directory.join("fixture.bbf"),
        builder.build_bytes().expect("build verify fixture"),
    )
    .expect("write verify fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["fixture.bbf", "--verify", "--section=End"])
        .output()
        .expect("run Rust section verification");
    assert_eq!(command.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] No pages to verify. Unable to verify section End\n"
    );
    assert!(command.stderr.is_empty());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn extract_section_without_rangekey_matches_reference_diagnostics() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create extract directory");
    fs::write(
        directory.join("fixture.bbf"),
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write extract fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["fixture.bbf", "--extract", "--section=Start"])
        .output()
        .expect("run Rust section extraction");
    assert_eq!(command.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] Section Extraction Requires a rangekey.\n"
    );
    assert!(command.stderr.is_empty());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn extract_missing_section_falls_back_to_all_pages_like_reference() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create extract directory");
    fs::write(
        directory.join("fixture.bbf"),
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write extract fixture");
    let output_dir = directory.join("pages");
    fs::create_dir_all(&output_dir).expect("create extraction output directory");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args([
            "fixture.bbf",
            "--extract",
            "--section=Nope",
            "--rangekey=Stop",
            "--outdir=pages",
        ])
        .output()
        .expect("run Rust section extraction");
    assert_eq!(command.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] Extracting Section 'Nope' (Pages 0 - 1)\n"
    );
    assert!(command.stderr.is_empty());
    assert_eq!(
        fs::read_dir(&output_dir)
            .expect("read extracted pages")
            .count(),
        2
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn missing_external_config_files_warn_and_do_not_abort_muxing() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("page.dat"), b"page payload").expect("write page fixture");
    let output = directory.join("missing-config.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&pages)
        .arg(&output)
        .args([
            "--metafile=missing-metadata.txt",
            "--sections=missing-sections.txt",
        ])
        .output()
        .expect("run Rust bbfmux");
    assert!(
        command.status.success(),
        "missing config files aborted muxing: {}",
        String::from_utf8_lossy(&command.stderr)
    );

    let reader = Reader::open(&output).expect("read generated BBF");
    assert_eq!(reader.footer().expect("read footer").page_count, 1);
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn unreadable_config_paths_warn_and_do_not_abort_muxing() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("page.dat"), b"page payload").expect("write page fixture");
    let metadata_path = directory.join("metadata-dir");
    let sections_path = directory.join("sections-dir");
    fs::create_dir(&metadata_path).expect("create metadata directory");
    fs::create_dir(&sections_path).expect("create sections directory");
    let output = directory.join("unreadable-config.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&pages)
        .arg("unreadable-config.bbf")
        .args(["--metafile=metadata-dir", "--sections=sections-dir"])
        .output()
        .expect("run Rust bbfmux");
    assert_eq!(command.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] Unable to read text file: metadata-dir\n[BBFMUX] Unable to read text file: sections-dir\nMuxed 1 files to 'unreadable-config.bbf'...\n"
    );
    assert!(command.stderr.is_empty());
    assert!(output.is_file());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn mux_preserves_non_utf8_config_bytes() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("page.dat"), b"page payload").expect("write page fixture");
    fs::write(
        directory.join("metadata.bin"),
        [0xff, b'K', b':', 0xfe, b'V', b':', 0xfd, b'P', b'\n'],
    )
    .expect("write raw metadata fixture");
    fs::write(
        directory.join("sections.bin"),
        [0xfc, b':', b'0', b':', 0xfb, b'\n'],
    )
    .expect("write raw section fixture");
    let output = directory.join("raw-config.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&pages)
        .arg(&output)
        .args(["--metafile=metadata.bin", "--sections=sections.bin"])
        .output()
        .expect("run Rust bbfmux");
    assert!(
        command.status.success(),
        "raw config mux failed: {}",
        String::from_utf8_lossy(&command.stderr)
    );

    let reader = Reader::open(&output).expect("read generated BBF");
    let metadata = reader.metadata(0).expect("read metadata").unwrap();
    let section = reader.section(0).expect("read section").unwrap();
    assert_eq!(raw_pool_entry(&reader, metadata.key_offset), b"K");
    assert_eq!(
        raw_pool_entry(&reader, metadata.value_offset),
        &[0xfe, b'V']
    );
    assert_eq!(
        raw_pool_entry(&reader, metadata.parent_offset),
        &[0xfd, b'P']
    );
    assert_eq!(raw_pool_entry(&reader, section.title_offset), &[]);
    assert_eq!(raw_pool_entry(&reader, section.parent_offset), &[0xfb]);

    let info = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args([
            &output.to_string_lossy(),
            "--info",
            "--metadata",
            "--sections",
        ])
        .output()
        .expect("run Rust info on raw string fixture");
    assert!(info.status.success());
    assert_eq!(
        info.stdout,
        b"\n=== Metadata ===\nK : \xfeV\n     (Parent Key: \xfdP)\n\n=== Sections ===\n : 0\n(Parent Section: \xfb)\n"
    );
    assert!(info.stderr.is_empty());

    let extract = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args([
            &output.to_string_lossy(),
            "--extract",
            "--write-meta=raw-meta.txt",
        ])
        .output()
        .expect("extract raw metadata report");
    assert!(extract.status.success());
    assert_eq!(
        fs::read(directory.join("raw-meta.txt")).expect("read raw metadata report"),
        b"=== Metadata ===\nK : \xfeV\n     (Parent Key: \xfdP)\n"
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn empty_directory_mux_matches_reference_success_semantics() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create empty page directory");
    let output = directory.join("empty.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&pages)
        .arg("empty.bbf")
        .output()
        .expect("run Rust bbfmux");
    assert_eq!(command.status.code(), Some(0));
    assert_eq!(
        command.stdout,
        b"[BBFCODEC] No assets to finalize.Muxed 0 files to 'empty.bbf'...\n"
    );
    assert!(command.stderr.is_empty());

    let bytes = fs::read(&output).expect("read blank output");
    assert_eq!(bytes.len(), HEADER_SIZE);
    assert!(bytes.iter().all(|byte| *byte == 0));

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn missing_input_directory_matches_reference_empty_mux_semantics() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create missing-input directory");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["missing-pages", "missing-pages.bbf"])
        .output()
        .expect("run Rust bbfmux with missing input directory");
    assert_eq!(command.status.code(), Some(0));
    assert_eq!(
        command.stdout,
        b"[BBFCODEC] No assets to finalize.Muxed 0 files to 'missing-pages.bbf'...\n"
    );
    assert!(command.stderr.is_empty());

    let output = fs::read(directory.join("missing-pages.bbf")).expect("read blank output");
    assert_eq!(output.len(), HEADER_SIZE);
    assert!(output.iter().all(|byte| *byte == 0));

    let _ = fs::remove_dir_all(directory);
}

#[cfg(unix)]
#[test]
fn unreadable_page_matches_reference_continue_semantics() {
    use std::os::unix::fs::PermissionsExt;

    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create unreadable-page directory");
    let page = pages.join("page.dat");
    fs::write(&page, b"page payload").expect("write unreadable page");
    let mut permissions = fs::metadata(&page)
        .expect("stat unreadable page")
        .permissions();
    permissions.set_mode(0o0);
    fs::set_permissions(&page, permissions).expect("make page unreadable");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["pages", "unreadable-page.bbf"])
        .output()
        .expect("run Rust bbfmux with unreadable page");
    assert_eq!(command.status.code(), Some(0));
    assert_eq!(
        command.stdout,
        b"[BBFCODEC] No assets to finalize.Muxed 1 files to 'unreadable-page.bbf'...\n"
    );
    assert_eq!(
        command.stderr,
        b"[BBFCODEC] Unable to open pages/page.dat for reading.\n"
    );

    let output = fs::read(directory.join("unreadable-page.bbf")).expect("read blank output");
    assert_eq!(output.len(), HEADER_SIZE);
    assert!(output.iter().all(|byte| *byte == 0));

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn mux_preserves_explicitly_empty_parent_fields() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("page.dat"), b"page payload").expect("write page fixture");
    let output = directory.join("empty-parent.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&pages)
        .arg(&output)
        .args(["--meta=Key:Value:", "--section=Start:0:"])
        .output()
        .expect("run Rust bbfmux");
    assert!(
        command.status.success(),
        "empty-parent mux failed: {}",
        String::from_utf8_lossy(&command.stderr)
    );

    let reader = Reader::open(&output).expect("read generated BBF");
    let metadata = reader.metadata(0).expect("read metadata").unwrap();
    let section = reader.section(0).expect("read section").unwrap();
    assert_ne!(metadata.parent_offset, NO_PARENT_OFFSET);
    assert_ne!(section.parent_offset, NO_PARENT_OFFSET);
    assert_eq!(reader.string(metadata.parent_offset).unwrap(), Some(""));
    assert_eq!(reader.string(section.parent_offset).unwrap(), Some(""));

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn mux_matches_reference_malformed_config_fallbacks() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("page.dat"), b"page payload").expect("write page fixture");
    let output = directory.join("malformed-config.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&pages)
        .arg(&output)
        .args(["--meta=missing-value", "--section=Name:unknown-target"])
        .output()
        .expect("run Rust bbfmux");
    assert!(
        command.status.success(),
        "malformed-config mux failed: {}",
        String::from_utf8_lossy(&command.stderr)
    );
    assert_eq!(command.status.code(), Some(0));
    assert_eq!(
        command.stdout,
        format!(
            "Warning: Could not resolve target 'unknown-target'\nMuxed 1 files to '{}'...\n",
            output.display()
        )
        .as_bytes()
    );
    assert!(command.stderr.is_empty());

    let reader = Reader::open(&output).expect("read generated BBF");
    let footer = reader.footer().expect("read footer");
    assert_eq!(footer.metadata_count, 0);
    assert_eq!(footer.section_count, 1);
    assert_eq!(reader.section(0).unwrap().unwrap().start_index, 0);

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn file_config_malformed_records_match_reference_success_boundary() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("page.dat"), b"page payload").expect("write page fixture");
    fs::write(directory.join("metadata.txt"), b"MissingValue\n")
        .expect("write malformed metadata configuration");
    fs::write(directory.join("sections.txt"), b"DefaultPage\n")
        .expect("write malformed section configuration");
    let output = directory.join("malformed-files.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args([
            "pages",
            "malformed-files.bbf",
            "--metafile=metadata.txt",
            "--sections=sections.txt",
        ])
        .output()
        .expect("run malformed file-config mux");

    assert_eq!(command.status.code(), Some(0));
    assert_eq!(
        command.stdout,
        b"Muxed 1 files to 'malformed-files.bbf'...\n"
    );
    assert!(command.stderr.is_empty());

    let reader = Reader::open(&output).expect("read malformed file-config output");
    let footer = reader.footer().expect("read malformed file-config footer");
    assert_eq!(footer.metadata_count, 0);
    assert_eq!(footer.section_count, 1);
    assert_eq!(reader.section(0).unwrap().unwrap().start_index, 0);

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn file_config_truncates_extra_unescaped_parent_fields_like_reference() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("page.dat"), b"page payload").expect("write page fixture");
    fs::write(directory.join("metadata.txt"), b"Key:Value:Parent:Extra\n")
        .expect("write metadata configuration");
    fs::write(directory.join("sections.txt"), b"Name:0:Parent:Extra\n")
        .expect("write section configuration");
    let output = directory.join("file-config.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args([
            "pages",
            "file-config.bbf",
            "--metafile=metadata.txt",
            "--sections=sections.txt",
        ])
        .output()
        .expect("run file-config mux");
    assert!(command.status.success());

    let reader = Reader::open(&output).expect("read file-config BBF");
    let metadata = reader.metadata(0).unwrap().unwrap();
    assert_eq!(
        reader.string(metadata.parent_offset).unwrap(),
        Some("Parent")
    );
    let section = reader.section(0).unwrap().unwrap();
    assert_eq!(
        reader.string(section.parent_offset).unwrap(),
        Some("Parent")
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn file_config_unresolved_target_warning_preserves_raw_bytes() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("page.dat"), b"page payload").expect("write page fixture");
    fs::write(directory.join("sections.txt"), b"Name:\x80\n")
        .expect("write raw section configuration");
    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["pages", "raw-warning.bbf", "--sections=sections.txt"])
        .output()
        .expect("run raw-warning mux");
    assert!(command.status.success());
    assert_eq!(
        command.stdout,
        b"Warning: Could not resolve target '\x80'\nMuxed 1 files to 'raw-warning.bbf'...\n"
    );
    assert!(command.stderr.is_empty());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn file_config_section_count_overflow_stays_bounded_like_rust_contract() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("page.dat"), b"page payload").expect("write page fixture");
    fs::write(
        directory.join("sections.txt"),
        b"MissingTarget\nName:unknown-target\n",
    )
    .expect("write section configuration");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["pages", "bounded.bbf", "--sections=sections.txt"])
        .output()
        .expect("run Rust bbfmux");
    assert_eq!(command.status.code(), Some(0));
    assert_eq!(
        command.stdout,
        b"Warning: Could not resolve target 'unknown-target'\nMuxed 1 files to 'bounded.bbf'...\n"
    );
    assert!(command.stderr.is_empty());

    let reader = Reader::open(directory.join("bounded.bbf")).expect("read bounded fixture");
    let footer = reader.footer().expect("read bounded footer");
    assert_eq!(footer.section_count, 2);
    assert_eq!(reader.section(0).unwrap().unwrap().start_index, 0);
    assert_eq!(reader.section(1).unwrap().unwrap().start_index, 0);

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn file_config_entry_capacity_matches_reference_limit() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("page.dat"), b"page payload").expect("write page fixture");

    let mut metadata = String::new();
    let mut sections = String::new();
    for index in 0..300 {
        metadata.push_str(&format!("Key{index}:Value{index}\n"));
        sections.push_str(&format!("Section{index}:0\n"));
    }
    fs::write(directory.join("metadata.txt"), metadata).expect("write metadata configuration");
    fs::write(directory.join("sections.txt"), sections).expect("write section configuration");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args([
            "pages",
            "capacity.bbf",
            "--metafile=metadata.txt",
            "--sections=sections.txt",
        ])
        .output()
        .expect("run capacity-limited file config mux");
    assert!(command.status.success());

    let reader = Reader::open(directory.join("capacity.bbf")).expect("read capacity fixture");
    let footer = reader.footer().expect("read capacity footer");
    assert_eq!(footer.metadata_count, 256);
    assert_eq!(footer.section_count, 256);

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn mux_ignores_out_of_range_reference_sections() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("page.dat"), b"page payload").expect("write page fixture");
    let output = directory.join("out-of-range-section.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&pages)
        .arg(&output)
        .arg("--section=Name:99")
        .output()
        .expect("run Rust bbfmux");
    assert!(
        command.status.success(),
        "out-of-range section aborted muxing: {}",
        String::from_utf8_lossy(&command.stderr)
    );

    let reader = Reader::open(&output).expect("read generated BBF");
    assert_eq!(reader.footer().expect("read footer").section_count, 0);

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn mux_ignores_unknown_options_and_accepts_bare_reference_options() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create page directory");
    fs::write(pages.join("page.dat"), b"page payload").expect("write page fixture");
    fs::write(directory.join("order.txt"), b"page.dat:0\n").expect("write order fixture");
    let output = directory.join("bare-options.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&pages)
        .arg(&output)
        .args([
            "--unknown-option",
            "--order=order.txt",
            "--meta",
            "--section",
        ])
        .output()
        .expect("run Rust bbfmux");
    assert!(
        command.status.success(),
        "bare/unknown options aborted muxing: {}",
        String::from_utf8_lossy(&command.stderr)
    );

    let reader = Reader::open(&output).expect("read generated BBF");
    let footer = reader.footer().expect("read footer");
    assert_eq!(footer.metadata_count, 0);
    assert_eq!(footer.section_count, 1);
    assert_eq!(reader.section(0).unwrap().unwrap().start_index, 0);

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn deduplicated_pages_match_pinned_cpp_fixture_byte_for_byte() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    fs::create_dir_all(&pages).expect("create duplicate page directory");
    let payload = include_bytes!("fixtures/alpha.txt");
    fs::write(pages.join("alpha.txt"), payload).expect("write first duplicate page");
    fs::write(pages.join("duplicate.dat"), payload).expect("write second duplicate page");
    let output = directory.join("duplicate.bbf");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&pages)
        .arg(&output)
        .arg("--variable-ream-size")
        .output()
        .expect("run Rust deduplication mux");
    assert!(command.status.success());

    let actual = fs::read(&output).expect("read generated duplicate fixture");
    let expected = decode_hex(include_str!("fixtures/cpp_duplicate.bbf.hex"));
    assert_eq!(actual, expected, "Rust deduplication diverged from C++");
    let reader = Reader::from_bytes(actual);
    assert_eq!(
        reader.footer().expect("read duplicate footer").asset_count,
        1
    );
    assert_eq!(
        reader
            .page(0)
            .expect("read first page")
            .unwrap()
            .asset_index,
        0
    );
    assert_eq!(
        reader
            .page(1)
            .expect("read second page")
            .unwrap()
            .asset_index,
        0
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn default_deduplicated_extraction_matches_reference_bug() {
    let directory = temp_fixture_dir();
    let pages = directory.join("pages");
    let output_dir = directory.join("extracted");
    fs::create_dir_all(&pages).expect("create duplicate page directory");
    fs::create_dir_all(&output_dir).expect("create extraction directory");
    let payload = include_bytes!("fixtures/alpha.txt");
    fs::write(pages.join("alpha.txt"), payload).expect("write first duplicate page");
    fs::write(pages.join("duplicate.dat"), payload).expect("write second duplicate page");
    let input = directory.join("duplicate.bbf");

    let mux = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&pages)
        .arg(&input)
        .arg("--variable-ream-size")
        .output()
        .expect("run Rust deduplication mux");
    assert!(mux.status.success());

    let extract = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["duplicate.bbf", "--extract", "--outdir=extracted"])
        .output()
        .expect("run Rust deduplication extraction");
    assert!(extract.status.success());
    assert_eq!(
        String::from_utf8(extract.stdout).expect("extract stdout is UTF-8"),
        "[BBFMUX] Extracting asset 0 to extracted/page_0.dat\n"
    );
    assert_eq!(fs::read(output_dir.join("page_0.dat")).unwrap(), payload);
    assert!(!output_dir.join("page_1.dat").exists());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn petrify_matches_pinned_cpp_fixture_byte_for_byte() {
    let default_layout = decode_hex(include_str!("fixtures/cpp_basic.bbf.hex"));
    let expected = decode_hex(include_str!("fixtures/cpp_basic_petrified.bbf.hex"));
    let actual = Builder::petrify_bytes(&default_layout).expect("petrify fixture");
    assert_eq!(actual, expected, "Rust petrification diverged from C++");

    let reader = Reader::from_bytes(actual);
    let header = reader.header().expect("read petrified header");
    assert_ne!(header.flags & PETRIFICATION_FLAG, 0);
    assert_eq!(header.footer_offset, bbf_format::HEADER_SIZE as u64);
    assert_eq!(
        reader
            .asset(0)
            .expect("read petrified asset")
            .unwrap()
            .file_size,
        22
    );
    assert_eq!(
        reader
            .asset_data(&reader.asset(0).expect("read petrified asset").unwrap())
            .expect("read petrified payload"),
        b"alpha fixture payload\n"
    );
    // The reference transform preserves its pre-rebase footer hash.
    assert!(
        !reader
            .verify_footer_hash()
            .expect("read petrified footer hash")
    );
}

#[test]
fn info_output_matches_pinned_cpp_contract() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create temporary info directory");
    let input = directory.join("fixture.bbf");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write info fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&input)
        .args([
            "--info",
            "--header",
            "--footer",
            "--counts",
            "--offsets",
            "--metadata",
            "--sections",
            "--hashes",
            "--pages",
            "--strings",
        ])
        .output()
        .expect("run Rust info mode");
    assert!(command.status.success());
    assert_eq!(
        String::from_utf8(command.stdout).expect("info output is UTF-8"),
        include_str!("fixtures/cpp_basic_info.txt")
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn bare_info_flags_remain_bounded_rust_usage_errors() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create bare-info directory");
    let input = directory.join("fixture.bbf");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write bare-info fixture");

    for option in [
        "--hashes",
        "--footer",
        "--sections",
        "--counts",
        "--header",
        "--metadata",
        "--offsets",
        "--pages",
        "--strings",
    ] {
        let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
            .current_dir(&directory)
            .arg(&input)
            .arg(option)
            .output()
            .expect("run bounded bare-info option");
        assert_eq!(
            command.status.code(),
            Some(2),
            "unexpected status for {option}"
        );
        assert!(command.stdout.is_empty(), "unexpected stdout for {option}");
        assert!(
            String::from_utf8_lossy(&command.stderr)
                .starts_with("mux mode requires an output file"),
            "unexpected stderr for {option}: {}",
            String::from_utf8_lossy(&command.stderr)
        );
    }

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn info_after_petrify_reproduces_reference_union_metadata() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create temporary mode-order directory");
    let input = directory.join("fixture.bbf");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write mode-order fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&input)
        .args(["--petrify=linear.bbf", "--info"])
        .output()
        .expect("run mode-order info");
    assert!(command.status.success());
    assert_eq!(command.stdout, b"\n=== Metadata ===\nSource : fixture\n");
    assert!(command.stderr.is_empty());
    assert!(!directory.join("linear.bbf").exists());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn petrify_output_aliases_verify_section_name_like_reference() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create temporary mode-order directory");
    fs::write(
        directory.join("fixture.bbf"),
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write mode-order fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["fixture.bbf", "--petrify=linear.bbf", "--verify"])
        .output()
        .expect("run petrify-to-verify mode order");
    assert_eq!(command.status.code(), Some(1));
    assert_eq!(
        command.stdout,
        b"[BBFMUX] Unable to find section with title: linear.bbf\n"
    );
    assert!(command.stderr.is_empty());
    assert!(!directory.join("linear.bbf").exists());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn petrify_output_aliases_extract_section_name_like_reference() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create temporary mode-order directory");
    fs::write(
        directory.join("fixture.bbf"),
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write mode-order fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["fixture.bbf", "--petrify=linear.bbf", "--extract"])
        .output()
        .expect("run petrify-to-extract mode order");
    assert_eq!(command.status.code(), Some(1));
    assert_eq!(
        command.stdout,
        b"[BBFMUX] Section Extraction Requires a rangekey.\n"
    );
    assert!(command.stderr.is_empty());
    assert!(!directory.join("linear.bbf").exists());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn info_counts_do_not_implicitly_dump_metadata() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create info counts directory");
    let input = directory.join("fixture.bbf");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write info counts fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args([&input.to_string_lossy(), "--info", "--counts"])
        .output()
        .expect("run Rust info counts mode");
    assert!(command.status.success());
    assert_eq!(
        String::from_utf8(command.stdout).expect("info counts output is UTF-8"),
        "\n=== Counts ===\n  Assets:   2\n  Pages:    2\n  Metadata: 1\n  Sections: 1\n"
    );
    assert!(command.stderr.is_empty());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn bare_petrify_reports_the_reference_missing_output_error() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create petrify directory");
    let input = directory.join("fixture.bbf");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write petrify fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&input)
        .arg("--petrify")
        .output()
        .expect("run Rust bbfmux");
    assert_eq!(command.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] No file selected for petrification.\n"
    );
    assert!(command.stderr.is_empty());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn petrify_success_output_matches_reference_status_contract() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create petrify directory");
    fs::write(
        directory.join("fixture.bbf"),
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write petrify fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["fixture.bbf", "--petrify=linear.bbf"])
        .output()
        .expect("run Rust petrification");
    assert!(command.status.success());
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] Petrifying fixture.bbf to linear.bbf...\n[BBFMUX] Success.\n"
    );
    assert!(command.stderr.is_empty());
    assert_eq!(
        fs::read(directory.join("linear.bbf")).expect("read petrified output"),
        decode_hex(include_str!("fixtures/cpp_basic_petrified.bbf.hex"))
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn petrify_rename_failure_leaves_reference_temp_output() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create petrify directory");
    let input = directory.join("fixture.bbf");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write petrify fixture");
    let occupied_output = directory.join("occupied");
    fs::create_dir(&occupied_output).expect("create occupied output directory");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg("fixture.bbf")
        .arg("--petrify=occupied")
        .output()
        .expect("run Rust petrification");
    assert_eq!(command.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] Petrifying fixture.bbf to occupied...\n[BBFMUX] Failed to petrify fixture.bbf.\n"
    );
    assert!(occupied_output.is_dir());

    let temp_output = directory.join("petrified.bbf.tmp");
    assert!(temp_output.is_file(), "reference temp output was removed");
    assert!(fs::metadata(temp_output).expect("stat temp output").len() > 0);

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn malformed_petrification_rejects_and_cleans_staging_output() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create petrify directory");
    let input = directory.join("malformed.bbf");
    let mut fixture = decode_hex(include_str!("fixtures/cpp_basic.bbf.hex"));
    let footer_offset = Reader::from_bytes(fixture.clone())
        .header()
        .expect("read fixture header")
        .footer_offset as usize;

    // Point the footer's asset table past the footer. The reference attempts
    // the transform and leaves its staged file after copyRange fails; Rust
    // rejects the impossible default layout before committing any output.
    fixture[footer_offset..footer_offset + 8]
        .copy_from_slice(&(footer_offset as u64 + 1).to_le_bytes());
    fs::write(&input, fixture).expect("write malformed fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args(["malformed.bbf", "--petrify=linear.bbf"])
        .output()
        .expect("run Rust petrification");
    assert_eq!(command.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] Petrifying malformed.bbf to linear.bbf...\n[BBFMUX] Failed to petrify malformed.bbf.\n"
    );
    assert_eq!(
        String::from_utf8(command.stderr).expect("stderr is UTF-8"),
        "BBF build error: invalid BBF input: invalid default-layout ranges\n"
    );
    assert!(!directory.join("linear.bbf").exists());
    assert!(!directory.join("petrified.bbf.tmp").exists());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn section_verify_output_matches_pinned_cpp_contract() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create temporary verify directory");
    let input = directory.join("fixture.bbf");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write verify fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&input)
        .args(["--verify", "--section=Start"])
        .output()
        .expect("run Rust section verification");
    assert!(command.status.success());
    assert_eq!(
        String::from_utf8(command.stdout).expect("verify output is UTF-8"),
        include_str!("fixtures/cpp_basic_verify_section.txt")
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn verify_rangekey_reproduces_reference_union_alias() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create temporary verify directory");
    let input = directory.join("fixture.bbf");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write verify fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&input)
        .args(["--verify", "--section=Start", "--rangekey=Start"])
        .output()
        .expect("run Rust cross-mode verification");
    assert_eq!(command.status.code(), Some(1));
    let stdout = String::from_utf8(command.stdout).expect("verify output is UTF-8");
    assert!(stdout.starts_with("[BBFMUX] Invalid Asset Index: "));
    assert!(stdout.ends_with(" (Max: 2)\n"));
    assert!(command.stderr.is_empty());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn extraction_reports_and_default_asset_selection_match_fixture_contract() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create temporary extraction directory");
    let input = directory.join("fixture.bbf");
    let output_dir = directory.join("pages");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write extraction fixture");
    fs::create_dir_all(&output_dir).expect("create selected extraction output directory");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&input)
        .args([
            "--extract",
            "--outdir=pages",
            "--write-meta=meta.txt",
            "--write-hashes=hashes.txt",
        ])
        .output()
        .expect("run Rust extraction");
    assert!(command.status.success());
    assert_eq!(
        fs::read_to_string(directory.join("meta.txt")).expect("read metadata report"),
        include_str!("fixtures/cpp_basic_meta.txt")
    );
    assert_eq!(
        fs::read_to_string(directory.join("hashes.txt")).expect("read hash report"),
        include_str!("fixtures/cpp_basic_hashes.txt")
    );
    assert_eq!(
        fs::read(output_dir.join("page_0.dat")).expect("read first extracted page"),
        include_bytes!("fixtures/alpha.txt")
    );
    assert!(!output_dir.join("page_1.dat").exists());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn extraction_reports_are_selective_and_metadata_defaults_to_path_txt() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create report defaults directory");
    let input = directory.join("fixture.bbf");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write report defaults fixture");

    let no_reports = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args([&input.to_string_lossy(), "--extract"])
        .output()
        .expect("run Rust extraction without reports");
    assert!(no_reports.status.success());
    assert!(!directory.join("path.txt").exists());
    assert!(!directory.join("meta.txt").exists());
    assert!(!directory.join("hashes.txt").exists());

    let with_metadata = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args([&input.to_string_lossy(), "--extract", "--write-meta"])
        .output()
        .expect("run Rust extraction with metadata report");
    assert!(with_metadata.status.success());
    assert_eq!(
        fs::read_to_string(directory.join("path.txt")).expect("read default metadata report"),
        include_str!("fixtures/cpp_basic_meta.txt")
    );
    assert!(!directory.join("meta.txt").exists());
    assert!(!directory.join("hashes.txt").exists());

    let default_output = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .args([&input.to_string_lossy(), "--extract", "--asset=0"])
        .output()
        .expect("run Rust extraction with the default output directory");
    assert!(default_output.status.success());
    assert_eq!(
        String::from_utf8(default_output.stdout).expect("default extraction output is UTF-8"),
        "[BBFMUX] Extracting asset 0 to page_0.dat\n"
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn selected_asset_extraction_matches_fixture_payload() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create selected extraction directory");
    let input = directory.join("fixture.bbf");
    let output_dir = directory.join("pages");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write extraction fixture");
    fs::create_dir_all(&output_dir).expect("create selected extraction output directory");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&input)
        .args([
            "--extract",
            "--asset=1",
            "--outdir=pages",
            "--write-meta=meta.txt",
            "--write-hashes=hashes.txt",
        ])
        .output()
        .expect("run Rust selected-asset extraction");
    assert!(
        command.status.success(),
        "selected extraction failed: {}",
        String::from_utf8_lossy(&command.stderr)
    );
    assert_eq!(
        fs::read(output_dir.join("page_1.dat")).expect("read selected asset"),
        include_bytes!("fixtures/beta.dat")
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn selected_asset_write_failure_matches_reference_status_and_diagnostic() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create selected extraction directory");
    let input = directory.join("fixture.bbf");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write extraction fixture");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&input)
        .args([
            "--extract",
            "--asset=0",
            "--outdir=missing-pages",
            "--write-meta=meta.txt",
            "--write-hashes=hashes.txt",
        ])
        .output()
        .expect("run Rust selected-asset extraction");
    assert_eq!(command.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] Extracting asset 0 to missing-pages/page_0.dat\n[BBFMUX] Failed to write file: missing-pages/page_0.dat\n"
    );
    assert!(command.stderr.is_empty());
    assert!(!directory.join("missing-pages").exists());

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn empty_outdir_forms_stay_inside_the_working_directory() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create empty-outdir directory");
    let input = directory.join("fixture.bbf");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write empty-outdir fixture");

    for option in ["--outdir", "--outdir="] {
        let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
            .current_dir(&directory)
            .args([&input.to_string_lossy(), "--extract", "--asset=0", option])
            .output()
            .expect("run Rust extraction with empty outdir");
        assert!(command.status.success());
        assert_eq!(
            command.stdout,
            b"[BBFMUX] Extracting asset 0 to page_0.dat\n"
        );
        assert_eq!(
            fs::read(directory.join("page_0.dat")).expect("read empty-outdir extraction"),
            include_bytes!("fixtures/alpha.txt")
        );
        let _ = fs::remove_file(directory.join("page_0.dat"));
    }

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn extraction_report_open_failure_matches_reference_status_and_diagnostic() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create extraction directory");
    let input = directory.join("fixture.bbf");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write extraction fixture");
    fs::create_dir(directory.join("blocked")).expect("create blocked report path");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&input)
        .args([
            "--extract",
            "--asset=0",
            "--write-hashes=blocked",
            "--write-meta=meta.txt",
        ])
        .output()
        .expect("run Rust extraction");
    assert_eq!(command.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] Unable to open file: blocked"
    );
    assert!(command.stderr.is_empty());
    assert!(!directory.join("meta.txt").exists());

    let _ = fs::remove_dir_all(directory);
}

#[cfg(unix)]
#[test]
fn extraction_report_write_failure_matches_reference_success_boundary() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create extraction directory");
    let input = directory.join("fixture.bbf");
    let report = directory.join("report.txt");

    let mut builder = Builder::new();
    for index in 0..25 {
        let payload = format!("unique page {index} payload");
        builder.add_page_bytes(payload.as_bytes(), 0, 0, 0);
    }
    builder.write_to(&input).expect("write extraction fixture");

    // With SIGXFSZ ignored, the reference's unchecked fwrite stops at the
    // file-size limit and the extraction command still returns success.
    let command = Command::new("sh")
        .current_dir(&directory)
        .args([
            "-c",
            "trap '' SIGXFSZ; ulimit -f 1; exec \"$0\" \"$@\"",
            env!("CARGO_BIN_EXE_bbfmux"),
            "fixture.bbf",
            "--extract",
            "--asset=0",
            "--write-hashes=report.txt",
        ])
        .output()
        .expect("run extraction with a constrained report");
    assert_eq!(command.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] Extracting asset 0 to page_0.dat\n"
    );
    assert!(command.stderr.is_empty());
    assert_eq!(fs::metadata(&report).unwrap().len(), 512);
    assert_eq!(
        fs::read(directory.join("page_0.dat")).unwrap(),
        b"unique page 0 payload"
    );

    let _ = fs::remove_dir_all(directory);
}

#[cfg(unix)]
#[test]
fn extraction_asset_write_failure_matches_reference_success_boundary() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create extraction directory");
    let input = directory.join("fixture.bbf");
    let payload = vec![b'x'; 4096];
    let mut builder = Builder::new();
    builder.add_page_bytes(&payload, 0, 0, 0);
    builder.write_to(&input).expect("write extraction fixture");

    let command = Command::new("sh")
        .current_dir(&directory)
        .args([
            "-c",
            "trap '' SIGXFSZ; ulimit -f 1; exec \"$0\" \"$@\"",
            env!("CARGO_BIN_EXE_bbfmux"),
            "fixture.bbf",
            "--extract",
            "--asset=0",
        ])
        .output()
        .expect("run extraction with a constrained output");
    assert_eq!(command.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(command.stdout).expect("stdout is UTF-8"),
        "[BBFMUX] Extracting asset 0 to page_0.dat\n"
    );
    assert!(command.stderr.is_empty());
    assert_eq!(
        fs::metadata(directory.join("page_0.dat")).unwrap().len(),
        512
    );

    let _ = fs::remove_dir_all(directory);
}

#[test]
fn section_extraction_uses_page_asset_indexes() {
    let directory = temp_fixture_dir();
    fs::create_dir_all(&directory).expect("create section extraction directory");
    let input = directory.join("fixture.bbf");
    let output_dir = directory.join("pages");
    fs::write(
        &input,
        decode_hex(include_str!("fixtures/cpp_basic.bbf.hex")),
    )
    .expect("write section fixture");
    fs::create_dir_all(&output_dir).expect("create section extraction output directory");

    let command = Command::new(env!("CARGO_BIN_EXE_bbfmux"))
        .current_dir(&directory)
        .arg(&input)
        .args([
            "--extract",
            "--section=Start",
            "--rangekey=unused",
            "--asset=1",
            "--outdir=pages",
            "--write-meta=meta.txt",
            "--write-hashes=hashes.txt",
        ])
        .output()
        .expect("run Rust section extraction");
    assert!(
        command.status.success(),
        "section extraction failed: {}",
        String::from_utf8_lossy(&command.stderr)
    );
    assert_eq!(
        String::from_utf8(command.stdout).expect("section extraction output is UTF-8"),
        "[BBFMUX] Extracting Section 'Start' (Pages 0 - 1)\n"
    );
    assert_eq!(
        fs::read(output_dir.join("page_0.dat")).expect("read section page 0"),
        include_bytes!("fixtures/alpha.txt")
    );
    assert_eq!(
        fs::read(output_dir.join("page_1.dat")).expect("read section page 1"),
        include_bytes!("fixtures/beta.dat")
    );

    let _ = fs::remove_dir_all(directory);
}
