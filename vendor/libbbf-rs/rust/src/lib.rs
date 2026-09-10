//! Rust implementation of the Bound Book Format.
//!
//! The C++ implementation remains in this repository as a compatibility
//! reference while the format reader and writer are ported to Rust.

pub use bbf_format::{
    Asset, DecodeError, Expansion, FORMAT_VERSION, Footer, HEADER_SIZE, Header, MAGIC, MediaType,
    Metadata, NO_PARENT_OFFSET, Page, Section,
};
pub use bbf_io::{IndexedReader, Reader, ReaderError};
pub use bbf_mmap::{MappedFile, MappedFileError, OwnedFile};
pub use bbf_mux::{
    AppendError, ArchiveEditor, Builder, BuilderConfig, BuilderError, EditError, FileAppender,
    FileBuilder, PendingAsset, StringPool,
};

/// Tokio-backed archive access, available with the crate's `tokio` feature.
#[cfg(feature = "tokio")]
pub use bbf_tokio;

/// Computes the non-cryptographic XXH3-64 checksum used by BBF index regions.
pub fn index_checksum(bytes: &[u8]) -> u64 {
    xxhash_rust::xxh3::xxh3_64(bytes)
}

#[cfg(test)]
mod tests {
    use super::{Builder, FORMAT_VERSION, FileBuilder, index_checksum};

    #[test]
    fn format_version_matches_the_specification() {
        assert_eq!(FORMAT_VERSION, 3);
    }

    #[test]
    fn index_checksum_is_deterministic() {
        assert_eq!(index_checksum(b"BBF"), index_checksum(b"BBF"));
    }

    #[test]
    fn facade_exposes_in_memory_and_file_backed_builders() {
        fn assert_builder_types<T, U>() {}

        assert_builder_types::<Builder, FileBuilder>();
    }
}
