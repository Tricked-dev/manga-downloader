//! Read-only memory-mapped BBF input.
//!
//! This crate's only unsafe operation is the platform mapping call in
//! [`MappedFile::open`]. `memmap2` keeps the mapping alive and the public API
//! exposes it only as an immutable byte-backed [`bbf_io::Reader`]. Use
//! [`OwnedFile`] when the input may be modified by another process.

use std::{fs::File, path::Path};

use bbf_io::Reader;
use memmap2::{Mmap, MmapOptions};

#[derive(Debug)]
pub enum MappedFileError {
    Io(std::io::Error),
}

impl std::fmt::Display for MappedFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "could not memory-map BBF file: {error}"),
        }
    }
}

impl std::error::Error for MappedFileError {}

impl From<std::io::Error> for MappedFileError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// A read-only memory mapping whose lifetime owns the mapped file contents.
#[derive(Debug)]
pub struct MappedFile {
    mapping: Mmap,
}

impl MappedFile {
    /// Maps the file read-only. The file descriptor may be closed after this
    /// call; the mapping owns the readable region for the remainder of `self`.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that the file's contents and length remain
    /// unchanged for the entire lifetime of the returned mapping. This
    /// includes preventing writes, truncation, and concurrent mutation by
    /// other processes. A read-only mapping does not provide that guarantee.
    pub unsafe fn open(path: impl AsRef<Path>) -> Result<Self, MappedFileError> {
        let file = File::open(path)?;
        // SAFETY: the caller provides the immutability guarantee documented
        // above; the file handle remains valid for this mapping call.
        let mapping = unsafe { MmapOptions::new().map(&file)? };
        Ok(Self { mapping })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.mapping
    }

    pub fn reader(&self) -> Reader<&[u8]> {
        Reader::from_data(self.as_bytes())
    }

    pub fn len(&self) -> usize {
        self.mapping.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mapping.is_empty()
    }
}

/// An owned BBF file image for callers that cannot guarantee source-file
/// immutability while reading.
#[derive(Debug)]
pub struct OwnedFile {
    bytes: Vec<u8>,
}

impl OwnedFile {
    /// Reads the complete file into owned memory.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MappedFileError> {
        Ok(Self {
            bytes: std::fs::read(path)?,
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn reader(&self) -> Reader<&[u8]> {
        Reader::from_data(self.as_bytes())
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{MappedFile, OwnedFile};

    #[test]
    fn reader_views_the_mapping_without_copying() {
        let path = std::env::temp_dir().join(format!("bbf-mmap-test-{}", std::process::id()));
        fs::write(&path, b"mapped bytes").expect("write mapping fixture");

        let mapped = unsafe { MappedFile::open(&path) }.expect("open mapping");
        let reader = mapped.reader();
        assert_eq!(mapped.as_bytes(), b"mapped bytes");
        assert_eq!(reader.bytes(), mapped.as_bytes());
        assert_eq!(reader.bytes().as_ptr(), mapped.as_bytes().as_ptr());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn owned_file_is_stable_when_the_source_changes_after_open() {
        let path = std::env::temp_dir().join(format!("bbf-owned-file-test-{}", std::process::id()));
        fs::write(&path, b"original bytes").expect("write owned-file fixture");

        let owned = OwnedFile::open(&path).expect("open owned file");
        fs::write(&path, b"changed").expect("change source file");
        assert_eq!(owned.as_bytes(), b"original bytes");
        assert_eq!(owned.reader().bytes(), b"original bytes");

        let _ = fs::remove_file(path);
    }
}
