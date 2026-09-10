#![allow(clippy::missing_errors_doc)]

use anyhow::{Context, Result};
use fusio::{
    Fs, OpenOptions, Read, Write,
    disk::LocalFs,
    path::{Path as FusioPath, path_to_local},
};
use futures_util::{StreamExt, pin_mut};
use std::{
    ffi::OsStr,
    fs::{File, OpenOptions as StdOpenOptions},
    path::{Component as PathComponent, Path, PathBuf},
    time::SystemTime,
};

pub type LocalFile = File;
pub type AsyncFile = tokio::fs::File;

#[derive(Debug, Clone)]
pub struct LocalMetadata {
    len: u64,
    is_file: bool,
    is_dir: bool,
    modified: Option<SystemTime>,
}

impl LocalMetadata {
    #[must_use]
    /// Returns the file length in bytes.
    pub const fn len(&self) -> u64 {
        self.len
    }

    #[must_use]
    /// Returns whether the metadata length is zero bytes.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    /// Returns whether the path metadata describes a regular file.
    pub const fn is_file(&self) -> bool {
        self.is_file
    }

    #[must_use]
    /// Returns whether the path metadata describes a directory.
    pub const fn is_dir(&self) -> bool {
        self.is_dir
    }

    #[must_use]
    /// Returns the last-modified time when the platform reports one.
    pub const fn modified(&self) -> Option<SystemTime> {
        self.modified
    }
}

/// Creates a directory and any missing parents asynchronously.
pub async fn create_dir_all(path: &Path) -> Result<()> {
    let fusio_path = to_fusio_path(path)?;
    <LocalFs as Fs>::create_dir_all(&fusio_path)
        .await
        .with_context(|| format!("Failed to create directory {}", path.display()))
}

/// Creates a directory and any missing parents synchronously.
pub fn create_dir_all_sync(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)
        .with_context(|| format!("Failed to create directory {}", path.display()))
}

/// Reads an entire file into memory asynchronously.
pub async fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    let fs = LocalFs::default();
    let fusio_path = to_fusio_path(path)?;
    let mut file = fs
        .open(&fusio_path)
        .await
        .with_context(|| format!("Failed to open {}", path.display()))?;
    let (result, bytes) = file.read_to_end_at(Vec::new(), 0).await;
    result.with_context(|| format!("Failed to read {}", path.display()))?;
    Ok(bytes)
}

/// Reads an entire file into memory synchronously.
pub fn read_bytes_sync(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("Failed to read {}", path.display()))
}

/// Reads a UTF-8 file into a string synchronously.
pub fn read_to_string_sync(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))
}

/// Writes bytes to a file asynchronously, creating or truncating it.
pub async fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let fs = LocalFs::default();
    let fusio_path = to_fusio_path(path)?;
    let mut file = fs
        .open_options(
            &fusio_path,
            OpenOptions::default()
                .create(true)
                .write(true)
                .truncate(true),
        )
        .await
        .with_context(|| format!("Failed to open {} for writing", path.display()))?;
    let (result, _) = file.write_all(bytes).await;
    result.with_context(|| format!("Failed to write {}", path.display()))?;
    file.close()
        .await
        .with_context(|| format!("Failed to close {}", path.display()))?;
    Ok(())
}

/// Writes bytes to a file synchronously, creating or truncating it.
pub fn write_bytes_sync(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).with_context(|| format!("Failed to write {}", path.display()))
}

/// Reads path metadata asynchronously.
pub async fn metadata(path: &Path) -> Result<LocalMetadata> {
    tokio::fs::metadata(path)
        .await
        .with_context(|| format!("Failed to read metadata for {}", path.display()))
        .map(|metadata| local_metadata(&metadata))
}

/// Reads path metadata synchronously.
pub fn metadata_sync(path: &Path) -> Result<LocalMetadata> {
    std::fs::metadata(path)
        .with_context(|| format!("Failed to read metadata for {}", path.display()))
        .map(|metadata| local_metadata(&metadata))
}

/// Returns the size of a file in bytes asynchronously.
pub async fn file_size(path: &Path) -> Result<u64> {
    let fs = LocalFs::default();
    let fusio_path = to_fusio_path(path)?;
    let file = fs
        .open(&fusio_path)
        .await
        .with_context(|| format!("Failed to open {}", path.display()))?;
    file.size()
        .await
        .with_context(|| format!("Failed to read file size for {}", path.display()))
}

/// Returns the size of a file in bytes synchronously.
pub fn file_size_sync(path: &Path) -> Result<u64> {
    metadata_sync(path).map(|metadata| metadata.len())
}

/// Returns a file size synchronously, treating missing paths and directories as zero bytes.
pub fn file_size_if_present_sync(path: &Path) -> Result<u64> {
    if !path_exists(path) {
        return Ok(0);
    }

    metadata_sync(path).map(|metadata| {
        if metadata.is_file() {
            metadata.len()
        } else {
            0
        }
    })
}

#[must_use]
/// Returns whether metadata can be read for `path`.
pub fn path_exists(path: &Path) -> bool {
    std::fs::metadata(path).is_ok()
}

/// Opens a file for synchronous reading.
pub fn open_file(path: &Path) -> Result<LocalFile> {
    File::open(path).with_context(|| format!("Failed to open {}", path.display()))
}

/// Opens a file for asynchronous reading.
pub async fn open_async_file(path: &Path) -> Result<AsyncFile> {
    tokio::fs::File::open(path)
        .await
        .with_context(|| format!("Failed to open {}", path.display()))
}

/// Creates a new file, failing if the target already exists.
pub fn create_new_file(path: &Path) -> std::io::Result<LocalFile> {
    StdOpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

/// Removes a file asynchronously.
pub async fn remove_file(path: &Path) -> Result<()> {
    let fs = LocalFs::default();
    let fusio_path = to_fusio_path(path)?;
    fs.remove(&fusio_path)
        .await
        .with_context(|| format!("Failed to remove {}", path.display()))
}

/// Removes a file synchronously.
pub fn remove_file_sync(path: &Path) -> Result<()> {
    std::fs::remove_file(path).with_context(|| format!("Failed to remove {}", path.display()))
}

/// Removes a directory tree asynchronously.
pub async fn remove_dir_all(path: &Path) -> Result<()> {
    tokio::fs::remove_dir_all(path)
        .await
        .with_context(|| format!("Failed to remove directory {}", path.display()))
}

/// Removes a file asynchronously if it exists and returns its previous size.
///
/// Missing paths and non-readable sizes are treated as zero bytes.
pub async fn remove_file_if_present(path: &Path) -> Result<u64> {
    let Ok(size) = file_size(path).await else {
        return Ok(0);
    };

    let _ = remove_file(path).await;
    Ok(size)
}

/// Removes a file synchronously if it exists and returns its previous size.
pub fn remove_file_if_present_sync(path: &Path) -> Result<u64> {
    if !path_exists(path) {
        return Ok(0);
    }

    let size = file_size_if_present_sync(path)?;
    let _ = remove_file_sync(path);
    Ok(size)
}

/// Renames or moves a path asynchronously.
pub async fn rename(from: &Path, to: &Path) -> Result<()> {
    tokio::fs::rename(from, to).await.with_context(|| {
        format!(
            "Failed to move file from {} to {}",
            from.display(),
            to.display()
        )
    })
}

/// Renames or moves a path synchronously.
pub fn rename_sync(from: &Path, to: &Path) -> Result<()> {
    std::fs::rename(from, to).with_context(|| {
        format!(
            "Failed to move file from {} to {}",
            from.display(),
            to.display()
        )
    })
}

/// Lists directory entries sorted by file name.
pub async fn list_paths_sorted(path: &Path) -> Result<Vec<PathBuf>> {
    let fs = LocalFs::default();
    let fusio_path = to_fusio_path(path)?;
    let stream = fs
        .list(&fusio_path)
        .await
        .with_context(|| format!("Failed to list {}", path.display()))?;
    pin_mut!(stream);

    let mut entries = Vec::new();
    while let Some(entry) = stream.next().await {
        let meta = entry.with_context(|| format!("Failed to read entry in {}", path.display()))?;
        let local_path = path_to_local(&meta.path)
            .with_context(|| format!("Failed to convert entry in {}", path.display()))?;
        entries.push(local_path);
    }

    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    Ok(entries)
}

/// Converts `path` into a normalized absolute path.
pub fn absolute_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    Ok(normalize_path(&absolute))
}

/// Recursively sums regular-file sizes under `root`.
///
/// Missing roots are treated as empty directories.
pub fn directory_size_bytes(root: &Path) -> Result<u64> {
    if !path_exists(root) {
        return Ok(0);
    }

    directory_size_bytes_inner(root)
}

/// Recursively collects files whose names end with `suffix`, case-insensitively.
///
/// Missing roots return an empty list.
pub fn collect_files_with_suffix(root: &Path, suffix: &str) -> Result<Vec<PathBuf>> {
    if !path_exists(root) {
        return Ok(Vec::new());
    }

    let suffix = suffix.to_ascii_lowercase();
    let mut files = Vec::new();
    collect_files_with_suffix_inner(root, &suffix, &mut files)?;
    files.sort();
    Ok(files)
}

fn local_metadata(metadata: &std::fs::Metadata) -> LocalMetadata {
    LocalMetadata {
        len: metadata.len(),
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        modified: metadata.modified().ok(),
    }
}

fn directory_size_bytes_inner(root: &Path) -> Result<u64> {
    let mut total = 0_u64;
    for entry in std::fs::read_dir(root)
        .with_context(|| format!("Failed to list directory {}", root.display()))?
    {
        let entry = entry.with_context(|| format!("Failed to read entry in {}", root.display()))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .with_context(|| format!("Failed to read metadata for {}", path.display()))?;
        if metadata.is_dir() {
            total = total.saturating_add(directory_size_bytes_inner(&path)?);
        } else if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

fn collect_files_with_suffix_inner(
    root: &Path,
    suffix: &str,
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in std::fs::read_dir(root)
        .with_context(|| format!("Failed to list directory {}", root.display()))?
    {
        let entry = entry.with_context(|| format!("Failed to read entry in {}", root.display()))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .with_context(|| format!("Failed to read metadata for {}", path.display()))?;
        if metadata.is_dir() {
            collect_files_with_suffix_inner(&path, suffix, files)?;
        } else if metadata.is_file() && path_has_suffix(&path, suffix) {
            files.push(path);
        }
    }
    Ok(())
}

fn path_has_suffix(path: &Path, suffix: &str) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.to_ascii_lowercase().ends_with(suffix))
}

fn to_fusio_path(path: &Path) -> Result<FusioPath> {
    let absolute = absolute_path(path)?;
    FusioPath::from_absolute_path(&absolute)
        .with_context(|| format!("Failed to convert {} to fusio path", absolute.display()))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            PathComponent::CurDir => {}
            PathComponent::ParentDir => {
                normalized.pop();
            }
            PathComponent::RootDir | PathComponent::Prefix(_) | PathComponent::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}
