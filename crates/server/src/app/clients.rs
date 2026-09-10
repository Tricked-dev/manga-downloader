use std::{
    io::{Cursor, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use anyhow::{Context, anyhow};
use autometrics::autometrics;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::api::error::AppError;

const AIDOKU_CLIENT_ID: &str = "aidoku";
const TACHIYOMI_CLIENT_ID: &str = "tachiyomi";
const AIDOKU_PACKAGE_FILE_NAME: &str = "package.aix";
const TACHIYOMI_PACKAGE_FILE_NAME: &str = "manga-downloader-tachiyomi.apk";
const AIDOKU_WASM_FILE_NAME: &str = "manga_downloader.wasm";
const AIDOKU_WASM_TARGET: &str = "wasm32-unknown-unknown";
const AIDOKU_PACKAGE_PATH_ENV: &str = "AIDOKU_PACKAGE_PATH";
const TACHIYOMI_PACKAGE_PATH_ENV: &str = "TACHIYOMI_PACKAGE_PATH";
const AIDOKU_RUSTFLAGS: &str = "-C link-arg=--allow-undefined";
const CLIENT_PACKAGE_CONTENT_TYPE: &str = "application/octet-stream";

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/client_packages.rs"));
}

pub struct ClientPackage {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
    pub filename: &'static str,
}

#[autometrics(track_concurrency)]
pub async fn build_package(client: &str) -> Result<ClientPackage, AppError> {
    match client {
        AIDOKU_CLIENT_ID => build_aidoku_client_package().await,
        TACHIYOMI_CLIENT_ID => build_tachiyomi_client_package().await,
        _ => Err(AppError::not_found(anyhow!(
            "client package not found: {client}"
        ))),
    }
}

#[autometrics(track_concurrency)]
async fn build_aidoku_client_package() -> Result<ClientPackage, AppError> {
    if let Some(package_path) = std::env::var_os(AIDOKU_PACKAGE_PATH_ENV) {
        return read_prebuilt_package(
            PathBuf::from(package_path),
            AIDOKU_PACKAGE_FILE_NAME,
            "Aidoku",
        )
        .await;
    }

    if let Some(bytes) = embedded::AIDOKU {
        return Ok(embedded_package(bytes, AIDOKU_PACKAGE_FILE_NAME));
    }

    static BUILD_LOCK: std::sync::LazyLock<Arc<tokio::sync::Mutex<()>>> =
        std::sync::LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(())));
    let guard = Arc::clone(&BUILD_LOCK).lock_owned().await;
    tokio::task::spawn_blocking(move || {
        let _guard = guard;
        build_aidoku_package()
    })
    .await
    .map_err(|error| AppError::internal(anyhow!("client package build task failed: {error}")))?
}

#[autometrics]
async fn read_prebuilt_package(
    path: PathBuf,
    filename: &'static str,
    client_name: &'static str,
) -> Result<ClientPackage, AppError> {
    let bytes = tokio::fs::read(&path)
        .await
        .with_context(|| format!("failed to read {}", path.display()))
        .map_err(AppError::internal)?;

    if bytes.is_empty() {
        return Err(AppError::internal(anyhow!(
            "{client_name} package file is empty: {}",
            path.display(),
        )));
    }

    Ok(ClientPackage {
        bytes,
        content_type: CLIENT_PACKAGE_CONTENT_TYPE,
        filename,
    })
}

#[autometrics(track_concurrency)]
async fn build_tachiyomi_client_package() -> Result<ClientPackage, AppError> {
    if let Some(package_path) = std::env::var_os(TACHIYOMI_PACKAGE_PATH_ENV) {
        return read_prebuilt_package(
            PathBuf::from(package_path),
            TACHIYOMI_PACKAGE_FILE_NAME,
            "Tachiyomi",
        )
        .await;
    }

    if let Some(bytes) = embedded::TACHIYOMI {
        return Ok(embedded_package(bytes, TACHIYOMI_PACKAGE_FILE_NAME));
    }

    static BUILD_LOCK: std::sync::LazyLock<Arc<tokio::sync::Mutex<()>>> =
        std::sync::LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(())));
    let guard = Arc::clone(&BUILD_LOCK).lock_owned().await;
    tokio::task::spawn_blocking(move || {
        let _guard = guard;
        build_tachiyomi_package()
    })
    .await
    .map_err(|error| AppError::internal(anyhow!("client package build task failed: {error}")))?
}

fn embedded_package(bytes: &[u8], filename: &'static str) -> ClientPackage {
    ClientPackage {
        bytes: bytes.to_vec(),
        content_type: CLIENT_PACKAGE_CONTENT_TYPE,
        filename,
    }
}

#[autometrics(track_concurrency)]
fn build_tachiyomi_package() -> Result<ClientPackage, AppError> {
    let source_dir = tachiyomi_source_dir();
    ensure_tachiyomi_release_build(&source_dir)?;
    let package_path = source_dir.join("build").join("package.apk");
    let bytes = std::fs::read(&package_path)
        .with_context(|| format!("failed to read {}", package_path.display()))
        .map_err(AppError::internal)?;

    if bytes.is_empty() {
        return Err(AppError::internal(anyhow!(
            "Tachiyomi package file is empty: {}",
            package_path.display()
        )));
    }

    Ok(ClientPackage {
        bytes,
        content_type: CLIENT_PACKAGE_CONTENT_TYPE,
        filename: TACHIYOMI_PACKAGE_FILE_NAME,
    })
}

#[autometrics(track_concurrency)]
fn build_aidoku_package() -> Result<ClientPackage, AppError> {
    let source_dir = aidoku_source_dir();
    ensure_aidoku_release_build(&source_dir)?;
    let bytes = package_aidoku_source(&source_dir)?;

    Ok(ClientPackage {
        bytes,
        content_type: CLIENT_PACKAGE_CONTENT_TYPE,
        filename: AIDOKU_PACKAGE_FILE_NAME,
    })
}

fn aidoku_source_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../clients/aidoku")
}

fn tachiyomi_source_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../clients/tachiyomi")
}

#[autometrics]
fn ensure_aidoku_release_build(source_dir: &Path) -> Result<(), AppError> {
    let output = Command::new("cargo")
        .args([
            "build",
            "--locked",
            "--target",
            AIDOKU_WASM_TARGET,
            "--release",
        ])
        .current_dir(source_dir)
        .env("CARGO_TARGET_DIR", source_dir.join("target"))
        .env("RUSTFLAGS", AIDOKU_RUSTFLAGS)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTUP_TOOLCHAIN")
        .output()
        .with_context(|| format!("failed to run Aidoku build in {}", source_dir.display()))
        .map_err(AppError::internal)?;

    if output.status.success() {
        return Ok(());
    }

    Err(AppError::internal(anyhow!(
        "Aidoku build failed: {}",
        command_output_summary(&output)
    )))
}

#[autometrics]
fn ensure_tachiyomi_release_build(source_dir: &Path) -> Result<(), AppError> {
    let output = Command::new(source_dir.join("scripts").join("build-apk.sh"))
        .current_dir(source_dir)
        .output()
        .with_context(|| format!("failed to run Tachiyomi build in {}", source_dir.display()))
        .map_err(AppError::internal)?;

    if output.status.success() {
        return Ok(());
    }

    Err(AppError::internal(anyhow!(
        "Tachiyomi build failed: {}",
        command_output_summary(&output)
    )))
}

#[autometrics]
fn package_aidoku_source(source_dir: &Path) -> Result<Vec<u8>, AppError> {
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    archive
        .add_directory("Payload/", options)
        .context("failed to add Aidoku package payload directory")
        .map_err(AppError::internal)?;

    add_package_file(
        &mut archive,
        options,
        &source_dir
            .join("target")
            .join(AIDOKU_WASM_TARGET)
            .join("release")
            .join(AIDOKU_WASM_FILE_NAME),
        "Payload/main.wasm",
    )?;

    for file_name in ["settings.json", "source.json", "icon.png", "filters.json"] {
        add_package_file(
            &mut archive,
            options,
            &source_dir.join("res").join(file_name),
            &format!("Payload/{file_name}"),
        )?;
    }

    archive
        .finish()
        .map(Cursor::into_inner)
        .context("failed to finish Aidoku package")
        .map_err(AppError::internal)
}

fn add_package_file(
    archive: &mut ZipWriter<Cursor<Vec<u8>>>,
    options: SimpleFileOptions,
    source_path: &Path,
    package_path: &str,
) -> Result<(), AppError> {
    let bytes = std::fs::read(source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))
        .map_err(AppError::internal)?;

    if bytes.is_empty() {
        return Err(AppError::internal(anyhow!(
            "Aidoku package file is empty: {}",
            source_path.display()
        )));
    }

    archive
        .start_file(package_path, options)
        .with_context(|| format!("failed to add {package_path} to Aidoku package"))
        .map_err(AppError::internal)?;
    archive
        .write_all(&bytes)
        .with_context(|| format!("failed to write {package_path} to Aidoku package"))
        .map_err(AppError::internal)
}

fn command_output_summary(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let details = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };

    if details.is_empty() {
        return format!("cargo exited with {}", output.status);
    }

    format!("cargo exited with {}; {details}", output.status)
}
