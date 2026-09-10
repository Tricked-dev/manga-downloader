//! Minimal Tokio-backed access to BBF archives.
//!
//! The async API reuses the BBF codecs and synchronous streaming builder. It
//! performs file work in coarse `spawn_blocking` operations and does not
//! create an executor, worker pool, or task per record.

use std::{
    collections::HashSet,
    error::Error as StdError,
    fmt, fs,
    io::{self, BufReader, ErrorKind, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
};

use bbf_format::{
    ASSET_SIZE, Asset, DecodeError, EXPANSION_SIZE, Expansion, FOOTER_SIZE, Footer, HEADER_SIZE,
    Header, METADATA_SIZE, Metadata, PAGE_SIZE, PETRIFICATION_FLAG, Page, SECTION_SIZE, Section,
};
use bbf_mux::{
    AppendError as SyncAppendError, Builder, BuilderConfig, BuilderError, FileAppender, FileBuilder,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf},
    sync::{OwnedSemaphorePermit, Semaphore},
};
use xxhash_rust::xxh3::Xxh3;

const DEFAULT_MAX_INDEX_BYTES: u64 = 256 * 1024 * 1024;
const DEFAULT_MAX_ASSET_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const READ_BUFFER_SIZE: usize = 256 * 1024;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Errors returned by the Tokio API.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Format(DecodeError),
    Builder(BuilderError),
    Append(SyncAppendError),
    InvalidArchive(&'static str),
    MissingAsset(u64),
    MissingPage(u64),
    MissingSection(u64),
    MissingMetadata(u64),
    LimitExceeded {
        what: &'static str,
        requested: u64,
        limit: u64,
    },
    HashMismatch {
        asset: u64,
    },
    WriterClosed,
    InvalidConcurrencyLimit,
    LimiterClosed,
    Task(tokio::task::JoinError),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "BBF asynchronous I/O failed: {error}"),
            Self::Format(error) => write!(formatter, "invalid BBF structure: {error}"),
            Self::Builder(error) => write!(formatter, "BBF asynchronous writer failed: {error}"),
            Self::Append(error) => write!(formatter, "BBF asynchronous append failed: {error}"),
            Self::InvalidArchive(reason) => write!(formatter, "invalid BBF archive: {reason}"),
            Self::MissingAsset(index) => write!(formatter, "asset index {index} does not exist"),
            Self::MissingPage(index) => write!(formatter, "page index {index} does not exist"),
            Self::MissingSection(index) => {
                write!(formatter, "section index {index} does not exist")
            }
            Self::MissingMetadata(index) => {
                write!(formatter, "metadata index {index} does not exist")
            }
            Self::LimitExceeded {
                what,
                requested,
                limit,
            } => write!(
                formatter,
                "BBF {what} size {requested} exceeds configured limit {limit}"
            ),
            Self::HashMismatch { asset } => write!(formatter, "hash mismatch for asset {asset}"),
            Self::WriterClosed => write!(formatter, "BBF asynchronous writer is already closed"),
            Self::InvalidConcurrencyLimit => {
                write!(formatter, "BBF concurrency limit must be greater than zero")
            }
            Self::LimiterClosed => write!(formatter, "BBF concurrency limiter is closed"),
            Self::Task(error) => {
                write!(formatter, "BBF asynchronous blocking task failed: {error}")
            }
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Format(error) => Some(error),
            Self::Builder(error) => Some(error),
            Self::Append(error) => Some(error),
            Self::Task(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<DecodeError> for Error {
    fn from(error: DecodeError) -> Self {
        Self::Format(error)
    }
}

impl From<BuilderError> for Error {
    fn from(error: BuilderError) -> Self {
        Self::Builder(error)
    }
}

impl From<SyncAppendError> for Error {
    fn from(error: SyncAppendError) -> Self {
        Self::Append(error)
    }
}

/// Limits applied before async archive index or payload allocations.
#[derive(Debug, Clone, Copy)]
pub struct OpenOptions {
    pub max_index_bytes: u64,
    pub max_asset_bytes: u64,
}

/// A caller-owned limit for concurrent blocking file operations.
///
/// The limiter can be cloned and shared by multiple archive handles and
/// writers. It does not create workers or tasks; a permit is held only while
/// one coarse `spawn_blocking` operation is running.
#[derive(Clone, Debug)]
pub struct ConcurrencyLimiter {
    semaphore: Arc<Semaphore>,
}

impl ConcurrencyLimiter {
    pub fn new(max_concurrent_operations: usize) -> Result<Self, Error> {
        if max_concurrent_operations == 0 {
            return Err(Error::InvalidConcurrencyLimit);
        }
        Ok(Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent_operations)),
        })
    }

    async fn acquire(&self) -> Result<OwnedSemaphorePermit, Error> {
        self.semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| Error::LimiterClosed)
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            max_index_bytes: DEFAULT_MAX_INDEX_BYTES,
            max_asset_bytes: DEFAULT_MAX_ASSET_BYTES,
        }
    }
}

#[derive(Debug)]
struct ArchiveIndex {
    header: Header,
    footer: Footer,
    footer_verified: bool,
    assets: Vec<Asset>,
    pages: Vec<Page>,
    sections: Vec<Section>,
    metadata: Vec<Metadata>,
    expansions: Vec<Expansion>,
    string_pool: Vec<u8>,
}

/// A cheaply cloneable immutable view of an opened archive.
#[derive(Clone, Debug)]
pub struct AsyncArchive {
    path: Arc<PathBuf>,
    file: Arc<fs::File>,
    index: Arc<ArchiveIndex>,
    limiter: Option<ConcurrencyLimiter>,
}

/// A bounded asynchronous view over one asset or asset range.
///
/// The adapter uses the archive's retained file handle with positional reads
/// and never retains the payload in memory. Each adapter has an independent
/// logical offset and can therefore be read concurrently. A source file must
/// not be truncated while the archive is in use. Each poll performs one
/// bounded regular-file read directly; this intentionally avoids reopening
/// the path and avoids creating a task for every small read. Callers that
/// require all file work to leave the executor should use
/// [`AsyncArchive::read_asset_range`] at a coarse operation boundary.
pub struct AsyncAssetReader {
    file: Arc<fs::File>,
    offset: u64,
    remaining: u64,
}

impl AsyncAssetReader {
    /// Returns the number of payload bytes that have not been read yet.
    pub fn remaining(&self) -> u64 {
        self.remaining
    }
}

impl AsyncRead for AsyncAssetReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.remaining == 0 {
            return Poll::Ready(Ok(()));
        }

        let maximum = usize::try_from(self.remaining)
            .unwrap_or(usize::MAX)
            .min(buffer.remaining());
        if maximum == 0 {
            return Poll::Ready(Ok(()));
        }
        let read = match read_at(
            &self.file,
            &mut buffer.initialize_unfilled()[..maximum],
            self.offset,
        ) {
            Ok(read) => read,
            Err(error) => return Poll::Ready(Err(error)),
        };
        if read == 0 {
            return Poll::Ready(Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                "short BBF asset read",
            )));
        }
        buffer.advance(read);
        self.offset += read as u64;
        self.remaining -= read as u64;
        Poll::Ready(Ok(()))
    }
}

/// Summary returned after successful full verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationReport {
    pub footer_verified: bool,
    pub assets_verified: usize,
    pub bytes_verified: u64,
}

/// A staged, deduplicating asynchronous archive writer.
pub struct AsyncArchiveWriter {
    destination: PathBuf,
    staging: Option<PathBuf>,
    builder: Option<FileBuilder>,
    limiter: Option<ConcurrencyLimiter>,
}

/// Async access to the append-only editor for an already sealed BBF file.
///
/// Existing payload bytes are retained in place. Appended payloads and the
/// replacement index/footer are written after the old end of file, and the
/// original archive remains authoritative until [`Self::finalize`] completes.
pub struct AsyncFileAppender {
    inner: Option<FileAppender>,
    limiter: Option<ConcurrencyLimiter>,
}

impl AsyncArchive {
    /// Opens and structurally validates an archive without loading payloads.
    ///
    /// ```no_run
    /// # use std::error::Error;
    /// # fn main() -> Result<(), Box<dyn Error>> {
    /// tokio::runtime::Builder::new_current_thread()
    ///     .enable_all()
    ///     .build()?
    ///     .block_on(async {
    ///         let archive = bbf_tokio::AsyncArchive::open("archive.bbf").await?;
    ///         let page = archive.page(0)?;
    ///         let bytes = archive.read_asset(page.asset_index).await?;
    ///         assert_eq!(bytes.len() as u64, archive.asset(page.asset_index)?.file_size);
    ///         archive.verify().await?;
    ///         Ok::<(), Box<dyn Error>>(())
    ///     })?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_with_options(path, OpenOptions::default()).await
    }

    /// Opens an archive using a caller-owned shared blocking-operation limit.
    pub async fn open_with_limiter(
        path: impl AsRef<Path>,
        limiter: ConcurrencyLimiter,
    ) -> Result<Self, Error> {
        Self::open_with_options_and_limiter(path, OpenOptions::default(), limiter).await
    }

    /// Opens an archive with explicit pre-allocation limits.
    pub async fn open_with_options(
        path: impl AsRef<Path>,
        options: OpenOptions,
    ) -> Result<Self, Error> {
        Self::open_with_options_and_limiter(path, options, None).await
    }

    pub async fn open_with_options_and_limiter(
        path: impl AsRef<Path>,
        options: OpenOptions,
        limiter: impl Into<Option<ConcurrencyLimiter>>,
    ) -> Result<Self, Error> {
        let limiter = limiter.into();
        let permit = acquire_limiter(limiter.as_ref()).await?;
        let path = path.as_ref().to_owned();
        let path_for_open = path.clone();
        let opened = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            open_blocking(&path_for_open, options)
        })
        .await
        .map_err(Error::Task)??;
        Ok(Self {
            path: Arc::new(path),
            file: Arc::new(opened.file),
            index: Arc::new(opened.index),
            limiter,
        })
    }

    pub fn with_limiter(mut self, limiter: ConcurrencyLimiter) -> Self {
        self.limiter = Some(limiter);
        self
    }

    pub fn header(&self) -> Header {
        self.index.header
    }
    pub fn footer(&self) -> Footer {
        self.index.footer
    }
    pub fn asset_count(&self) -> usize {
        self.index.assets.len()
    }
    pub fn page_count(&self) -> usize {
        self.index.pages.len()
    }
    pub fn section_count(&self) -> usize {
        self.index.sections.len()
    }
    pub fn metadata_count(&self) -> usize {
        self.index.metadata.len()
    }
    pub fn expansion_count(&self) -> usize {
        self.index.expansions.len()
    }

    pub fn asset(&self, index: u64) -> Result<Asset, Error> {
        let requested = index;
        let index = usize::try_from(index).map_err(|_| Error::MissingAsset(requested))?;
        self.index
            .assets
            .get(index)
            .copied()
            .ok_or(Error::MissingAsset(requested))
    }

    pub fn page(&self, index: u64) -> Result<Page, Error> {
        let requested = index;
        let index = usize::try_from(index).map_err(|_| Error::MissingPage(requested))?;
        self.index
            .pages
            .get(index)
            .copied()
            .ok_or(Error::MissingPage(requested))
    }

    pub fn section(&self, index: u64) -> Result<Section, Error> {
        let requested = index;
        let index = usize::try_from(index).map_err(|_| Error::MissingSection(requested))?;
        self.index
            .sections
            .get(index)
            .copied()
            .ok_or(Error::MissingSection(requested))
    }

    pub fn metadata(&self, index: u64) -> Result<Metadata, Error> {
        let requested = index;
        let index = usize::try_from(index).map_err(|_| Error::MissingMetadata(requested))?;
        self.index
            .metadata
            .get(index)
            .copied()
            .ok_or(Error::MissingMetadata(requested))
    }

    pub fn expansion(&self, index: u64) -> Result<Expansion, Error> {
        let index = usize::try_from(index)
            .map_err(|_| Error::InvalidArchive("expansion index does not exist"))?;
        self.index
            .expansions
            .get(index)
            .copied()
            .ok_or(Error::InvalidArchive("expansion index does not exist"))
    }

    pub fn string_bytes(&self, offset: u64) -> Result<Option<&[u8]>, Error> {
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        let Some(tail) = self.index.string_pool.get(offset..) else {
            return Ok(None);
        };
        let end = tail.iter().position(|byte| *byte == 0);
        Ok(end.map(|end| &tail[..end]))
    }

    pub fn string(&self, offset: u64) -> Result<Option<&str>, Error> {
        Ok(self
            .string_bytes(offset)?
            .and_then(|bytes| std::str::from_utf8(bytes).ok()))
    }

    /// Reads a complete asset into memory. Use `read_asset_range` for bounded access.
    pub async fn read_asset(&self, asset_index: u64) -> Result<Vec<u8>, Error> {
        let asset = self.asset(asset_index)?;
        self.read_asset_range(asset_index, 0, asset.file_size).await
    }

    /// Reads a bounded asset range using positional file I/O.
    pub async fn read_asset_range(
        &self,
        asset_index: u64,
        offset: u64,
        length: u64,
    ) -> Result<Vec<u8>, Error> {
        let asset = self.asset(asset_index)?;
        let end = offset
            .checked_add(length)
            .ok_or(Error::InvalidArchive("asset range overflows"))?;
        if end > asset.file_size {
            return Err(Error::InvalidArchive("asset range exceeds asset size"));
        }
        let length_usize = usize::try_from(length).map_err(|_| Error::LimitExceeded {
            what: "asset allocation",
            requested: length,
            limit: usize::MAX as u64,
        })?;
        let file = Arc::clone(&self.file);
        let file_offset = asset
            .file_offset
            .checked_add(offset)
            .ok_or(Error::InvalidArchive("asset offset overflows"))?;
        let permit = acquire_limiter(self.limiter.as_ref()).await?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            read_range_blocking(&file, file_offset, length_usize)
        })
        .await
        .map_err(Error::Task)?
    }

    /// Opens a bounded asynchronous reader for a complete asset.
    ///
    /// This is the non-allocating alternative to [`Self::read_asset`]. The
    /// returned reader owns one private file cursor and limits reads to the
    /// asset's stored length.
    pub async fn asset_reader(&self, asset_index: u64) -> Result<AsyncAssetReader, Error> {
        let asset = self.asset(asset_index)?;
        self.asset_reader_range(asset_index, 0, asset.file_size)
            .await
    }

    /// Opens a bounded asynchronous reader for an asset range without
    /// allocating the range or retaining it in memory.
    pub async fn asset_reader_range(
        &self,
        asset_index: u64,
        offset: u64,
        length: u64,
    ) -> Result<AsyncAssetReader, Error> {
        let asset = self.asset(asset_index)?;
        let end = offset
            .checked_add(length)
            .ok_or(Error::InvalidArchive("asset range overflows"))?;
        if end > asset.file_size {
            return Err(Error::InvalidArchive("asset range exceeds asset size"));
        }
        let file_offset = asset
            .file_offset
            .checked_add(offset)
            .ok_or(Error::InvalidArchive("asset offset overflows"))?;
        Ok(AsyncAssetReader {
            file: Arc::clone(&self.file),
            offset: file_offset,
            remaining: length,
        })
    }

    /// Verifies the index and hashes each distinct payload once.
    pub async fn verify(&self) -> Result<VerificationReport, Error> {
        let file = Arc::clone(&self.file);
        let index = Arc::clone(&self.index);
        let permit = acquire_limiter(self.limiter.as_ref()).await?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            verify_blocking(&file, &index)
        })
        .await
        .map_err(Error::Task)?
    }

    /// Streams a normal-layout archive into a petrified destination.
    ///
    /// The transform runs as one coarse blocking operation, uses bounded
    /// buffers, and publishes through the synchronous library's unique
    /// destination-local staging file. Existing payloads are not retained in
    /// memory or copied into an in-memory editor.
    pub async fn petrify_to(&self, destination: impl AsRef<Path>) -> Result<Self, Error> {
        if self.index.header.flags & PETRIFICATION_FLAG != 0 {
            return Err(Error::InvalidArchive("file is already petrified"));
        }
        let input = Arc::clone(&self.path);
        let destination = destination.as_ref().to_owned();
        let destination_for_transform = destination.clone();
        let limiter = self.limiter.clone();
        let permit = acquire_limiter(limiter.as_ref()).await?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            Builder::petrify_file(input.as_ref(), &destination_for_transform)
                .map_err(Error::Builder)
        })
        .await
        .map_err(Error::Task)??;
        Self::open_with_options_and_limiter(destination, OpenOptions::default(), limiter).await
    }
}

impl AsyncFileAppender {
    /// Opens an existing archive without loading any asset payloads.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_with_options_and_limiter(path, OpenOptions::default(), None).await
    }

    pub async fn open_with_limiter(
        path: impl AsRef<Path>,
        limiter: impl Into<Option<ConcurrencyLimiter>>,
    ) -> Result<Self, Error> {
        Self::open_with_options_and_limiter(path, OpenOptions::default(), limiter).await
    }

    /// Opens an existing archive with the same pre-allocation limits as
    /// [`AsyncArchive::open_with_options`]. Existing payloads are never loaded;
    /// the limit applies to the replacement index and string-pool snapshot.
    pub async fn open_with_options(
        path: impl AsRef<Path>,
        options: OpenOptions,
    ) -> Result<Self, Error> {
        Self::open_with_options_and_limiter(path, options, None).await
    }

    pub async fn open_with_options_and_limiter(
        path: impl AsRef<Path>,
        options: OpenOptions,
        limiter: impl Into<Option<ConcurrencyLimiter>>,
    ) -> Result<Self, Error> {
        let limiter = limiter.into();
        let permit = acquire_limiter(limiter.as_ref()).await?;
        let path = path.as_ref().to_owned();
        let inner = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            FileAppender::open_with_index_limit(path, options.max_index_bytes).map_err(|error| {
                match error {
                    SyncAppendError::IndexTooLarge { requested, limit } => Error::LimitExceeded {
                        what: "append index",
                        requested,
                        limit,
                    },
                    error => Error::Append(error),
                }
            })
        })
        .await
        .map_err(Error::Task)??;
        Ok(Self {
            inner: Some(inner),
            limiter,
        })
    }

    pub fn with_limiter(mut self, limiter: ConcurrencyLimiter) -> Self {
        self.limiter = Some(limiter);
        self
    }

    pub fn page_count(&self) -> Result<usize, Error> {
        Ok(self.inner.as_ref().ok_or(Error::WriterClosed)?.page_count())
    }

    pub fn page(&self, index: u64) -> Result<Option<Page>, Error> {
        Ok(self.inner.as_ref().ok_or(Error::WriterClosed)?.page(index))
    }

    pub fn section_count(&self) -> Result<usize, Error> {
        Ok(self
            .inner
            .as_ref()
            .ok_or(Error::WriterClosed)?
            .section_count())
    }

    pub fn section(&self, index: u64) -> Result<Option<Section>, Error> {
        Ok(self
            .inner
            .as_ref()
            .ok_or(Error::WriterClosed)?
            .section(index))
    }

    pub fn string(&self, offset: u64) -> Result<Option<&str>, Error> {
        Ok(self
            .inner
            .as_ref()
            .ok_or(Error::WriterClosed)?
            .string(offset))
    }

    /// Returns a raw NUL-terminated string-pool entry by relative offset.
    ///
    /// This is the byte-oriented counterpart to [`Self::string`] and keeps
    /// non-UTF-8 metadata inspectable during async sealed-file edits.
    pub fn string_bytes(&self, offset: u64) -> Result<Option<&[u8]>, Error> {
        Ok(self
            .inner
            .as_ref()
            .ok_or(Error::WriterClosed)?
            .string_bytes(offset))
    }

    pub async fn add_asset_file(
        &mut self,
        path: impl AsRef<Path>,
        media_type: u8,
        asset_flags: u32,
    ) -> Result<u64, Error> {
        let path = path.as_ref().to_owned();
        self.run_appender(move |appender| appender.add_asset_file(path, media_type, asset_flags))
            .await
    }

    pub fn add_page_asset(&mut self, asset_index: u64, page_flags: u32) -> Result<u64, Error> {
        self.inner
            .as_mut()
            .ok_or(Error::WriterClosed)?
            .add_page_asset(asset_index, page_flags)
            .map_err(Error::Append)
    }

    pub fn replace_page(
        &mut self,
        page_index: u64,
        asset_index: u64,
        page_flags: u32,
    ) -> Result<(), Error> {
        self.inner
            .as_mut()
            .ok_or(Error::WriterClosed)?
            .replace_page(page_index, asset_index, page_flags)
            .map_err(Error::Append)
    }

    /// Removes a page from the replacement index and rebases section starts.
    /// Existing asset payloads remain in place until they are no longer
    /// referenced by the published index.
    pub fn remove_page(&mut self, page_index: u64) -> Result<Page, Error> {
        self.inner
            .as_mut()
            .ok_or(Error::WriterClosed)?
            .remove_page(page_index)
            .map_err(Error::Append)
    }

    pub fn add_section(
        &mut self,
        name: &str,
        start_index: u64,
        parent: Option<&str>,
    ) -> Result<(), Error> {
        self.add_section_bytes(name.as_bytes(), start_index, parent.map(str::as_bytes))
    }

    /// Adds a section while preserving arbitrary non-NUL bytes in the string pool.
    pub fn add_section_bytes(
        &mut self,
        name: &[u8],
        start_index: u64,
        parent: Option<&[u8]>,
    ) -> Result<(), Error> {
        self.inner
            .as_mut()
            .ok_or(Error::WriterClosed)?
            .add_section_bytes(name, start_index, parent)
            .map_err(Error::Append)
    }

    pub fn set_meta(&mut self, key: &str, value: &str, parent: Option<&str>) -> Result<(), Error> {
        self.set_meta_bytes(key.as_bytes(), value.as_bytes(), parent.map(str::as_bytes))
    }

    /// Inserts or replaces metadata while preserving arbitrary non-NUL bytes.
    pub fn set_meta_bytes(
        &mut self,
        key: &[u8],
        value: &[u8],
        parent: Option<&[u8]>,
    ) -> Result<(), Error> {
        self.inner
            .as_mut()
            .ok_or(Error::WriterClosed)?
            .set_meta_bytes(key, value, parent)
            .map_err(Error::Append)
    }

    /// Removes a metadata record from the replacement index.
    pub fn remove_metadata(&mut self, index: u64) -> Result<Metadata, Error> {
        self.inner
            .as_mut()
            .ok_or(Error::WriterClosed)?
            .remove_metadata(index)
            .map_err(Error::Append)
    }

    /// Removes a section from the replacement index.
    pub fn remove_section(&mut self, index: u64) -> Result<Section, Error> {
        self.inner
            .as_mut()
            .ok_or(Error::WriterClosed)?
            .remove_section(index)
            .map_err(Error::Append)
    }

    /// Publishes the appended index/footer by patching the existing header.
    pub async fn finalize(mut self) -> Result<(), Error> {
        let inner = self.inner.take().ok_or(Error::WriterClosed)?;
        let permit = acquire_limiter(self.limiter.as_ref()).await?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            inner.finalize()
        })
        .await
        .map_err(Error::Task)??;
        Ok(())
    }

    /// Drops the unfinalized append and leaves the previous archive view intact.
    pub fn abort(mut self) {
        let _ = self.inner.take();
    }

    async fn run_appender<T, F>(&mut self, operation: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&mut FileAppender) -> Result<T, SyncAppendError> + Send + 'static,
    {
        let mut appender = self.inner.take().ok_or(Error::WriterClosed)?;
        let permit = acquire_limiter(self.limiter.as_ref()).await?;
        let (appender, result) = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let result = operation(&mut appender);
            (appender, result)
        })
        .await
        .map_err(Error::Task)?;
        self.inner = Some(appender);
        result.map_err(Error::Append)
    }
}

impl AsyncArchiveWriter {
    /// Creates a staged archive writer that publishes only after `finish`.
    ///
    /// ```no_run
    /// # use std::error::Error;
    /// # use bbf_format::MediaType;
    /// # fn main() -> Result<(), Box<dyn Error>> {
    /// tokio::runtime::Builder::new_current_thread()
    ///     .enable_all()
    ///     .build()?
    ///     .block_on(async {
    ///         let mut writer = bbf_tokio::AsyncArchiveWriter::create("archive.bbf").await?;
    ///         let asset = writer
    ///             .add_asset_reader(&b"payload"[..], MediaType::Unknown.as_u8(), 0)
    ///             .await?;
    ///         writer.add_page(asset, 0)?;
    ///         let archive = writer.finish().await?;
    ///         archive.verify().await?;
    ///         Ok::<(), Box<dyn Error>>(())
    ///     })?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::create_with_config_and_limiter(path, BuilderConfig::default(), None).await
    }

    pub async fn create_with_limiter(
        path: impl AsRef<Path>,
        limiter: ConcurrencyLimiter,
    ) -> Result<Self, Error> {
        Self::create_with_config_and_limiter(path, BuilderConfig::default(), limiter).await
    }

    pub async fn create_with_config(
        path: impl AsRef<Path>,
        config: BuilderConfig,
    ) -> Result<Self, Error> {
        Self::create_with_config_and_limiter(path, config, None).await
    }

    pub async fn create_with_config_and_limiter(
        path: impl AsRef<Path>,
        config: BuilderConfig,
        limiter: impl Into<Option<ConcurrencyLimiter>>,
    ) -> Result<Self, Error> {
        let limiter = limiter.into();
        let permit = acquire_limiter(limiter.as_ref()).await?;
        let destination = path.as_ref().to_owned();
        let destination_for_task = destination.clone();
        let (staging, builder) = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let staging = create_unique_staging(&destination_for_task)?;
            match FileBuilder::with_config(&staging, config) {
                Ok(builder) => Ok((staging, builder)),
                Err(error) => {
                    let _ = fs::remove_file(&staging);
                    Err(Error::Builder(error))
                }
            }
        })
        .await
        .map_err(Error::Task)??;
        Ok(Self {
            destination,
            staging: Some(staging),
            builder: Some(builder),
            limiter,
        })
    }

    pub fn with_limiter(mut self, limiter: ConcurrencyLimiter) -> Self {
        self.limiter = Some(limiter);
        self
    }

    pub fn asset_count(&self) -> Result<usize, Error> {
        Ok(self
            .builder
            .as_ref()
            .ok_or(Error::WriterClosed)?
            .asset_count())
    }

    pub fn page_count(&self) -> Result<usize, Error> {
        Ok(self
            .builder
            .as_ref()
            .ok_or(Error::WriterClosed)?
            .page_count())
    }

    pub async fn add_asset_file(
        &mut self,
        source: impl AsRef<Path>,
        media_type: u8,
        asset_flags: u32,
    ) -> Result<u64, Error> {
        let source = source.as_ref().to_owned();
        self.run_builder(move |builder| builder.add_asset_file(source, media_type, asset_flags))
            .await
    }

    /// Appends a deduplicated file-backed asset without adding a page.
    pub async fn append_asset_file(
        &mut self,
        source: impl AsRef<Path>,
        media_type: u8,
        asset_flags: u32,
    ) -> Result<u64, Error> {
        self.add_asset_file(source, media_type, asset_flags).await
    }

    pub async fn add_files(
        &mut self,
        files: impl IntoIterator<Item = (PathBuf, u8, u32)>,
    ) -> Result<Vec<u64>, Error> {
        let files: Vec<_> = files.into_iter().collect();
        self.run_builder(move |builder| {
            files
                .into_iter()
                .map(|(path, media_type, flags)| builder.add_asset_file(path, media_type, flags))
                .collect()
        })
        .await
    }

    /// Appends multiple deduplicated file-backed assets without adding pages.
    pub async fn append_files(
        &mut self,
        files: impl IntoIterator<Item = (PathBuf, u8, u32)>,
    ) -> Result<Vec<u64>, Error> {
        self.add_files(files).await
    }

    pub fn add_page(&mut self, asset_index: u64, page_flags: u32) -> Result<(), Error> {
        self.builder
            .as_mut()
            .ok_or(Error::WriterClosed)?
            .add_page_asset(asset_index, page_flags)
            .map_err(Error::Builder)
    }

    /// Appends a page referencing an existing asset.
    pub fn append_page(&mut self, asset_index: u64, page_flags: u32) -> Result<(), Error> {
        self.add_page(asset_index, page_flags)
    }

    pub fn add_meta(&mut self, key: &str, value: &str, parent: Option<&str>) -> Result<(), Error> {
        self.add_meta_bytes(key.as_bytes(), value.as_bytes(), parent.map(str::as_bytes))
    }

    /// Adds metadata while preserving arbitrary non-NUL bytes in the string pool.
    pub fn add_meta_bytes(
        &mut self,
        key: &[u8],
        value: &[u8],
        parent: Option<&[u8]>,
    ) -> Result<(), Error> {
        if self
            .builder
            .as_mut()
            .ok_or(Error::WriterClosed)?
            .add_meta_bytes(key, value, parent)
        {
            Ok(())
        } else {
            Err(Error::Builder(BuilderError::FileBuilderFailed))
        }
    }

    pub fn add_section(
        &mut self,
        name: &str,
        start_index: u64,
        parent: Option<&str>,
    ) -> Result<(), Error> {
        self.add_section_bytes(name.as_bytes(), start_index, parent.map(str::as_bytes))
    }

    /// Adds a section while preserving arbitrary non-NUL bytes in the string pool.
    pub fn add_section_bytes(
        &mut self,
        name: &[u8],
        start_index: u64,
        parent: Option<&[u8]>,
    ) -> Result<(), Error> {
        if self
            .builder
            .as_mut()
            .ok_or(Error::WriterClosed)?
            .add_section_bytes(name, start_index, parent)
        {
            Ok(())
        } else {
            Err(Error::Builder(BuilderError::FileBuilderFailed))
        }
    }

    pub fn add_expansion(&mut self, expansion: Expansion) -> Result<(), Error> {
        let builder = self.builder.as_mut().ok_or(Error::WriterClosed)?;
        if builder.is_failed() {
            return Err(Error::Builder(BuilderError::FileBuilderFailed));
        }
        builder.add_expansion(expansion);
        Ok(())
    }

    /// Ingests a non-seekable Tokio reader through bounded private staging.
    pub async fn add_asset_reader<R: AsyncRead + Unpin>(
        &mut self,
        mut reader: R,
        media_type: u8,
        asset_flags: u32,
    ) -> Result<u64, Error> {
        let destination = self.destination.clone();
        let permit = acquire_limiter(self.limiter.as_ref()).await?;
        let temporary = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            create_unique_staging(&destination)
        })
        .await
        .map_err(Error::Task)??;
        let mut temporary_guard = TemporaryPathGuard::new(temporary.clone());
        let temporary_for_ingest = temporary.clone();
        let result = async {
            let mut output = tokio::fs::File::create(&temporary).await?;
            let mut buffer = vec![0u8; READ_BUFFER_SIZE];
            let mut hash = Xxh3::new();
            loop {
                let read = reader.read(&mut buffer).await?;
                if read == 0 {
                    break;
                }
                hash.update(&buffer[..read]);
                output.write_all(&buffer[..read]).await?;
            }
            output.flush().await?;
            drop(output);
            let digest = hash.digest128();
            self.run_builder(move |builder| {
                builder.add_asset_file_with_hash(
                    temporary_for_ingest,
                    media_type,
                    asset_flags,
                    digest as u64,
                    (digest >> 64) as u64,
                )
            })
            .await
        }
        .await;
        match result {
            Err(error) => Err(error),
            Ok(asset) => match tokio::fs::remove_file(&temporary).await {
                Ok(()) => {
                    temporary_guard.disarm();
                    Ok(asset)
                }
                Err(error) => Err(Error::Io(error)),
            },
        }
    }

    /// Appends a deduplicated asset from a non-seekable Tokio reader.
    pub async fn append_asset_reader<R: AsyncRead + Unpin>(
        &mut self,
        reader: R,
        media_type: u8,
        asset_flags: u32,
    ) -> Result<u64, Error> {
        self.add_asset_reader(reader, media_type, asset_flags).await
    }

    /// Flushes, atomically publishes, and reopens the completed archive.
    pub async fn finish(mut self) -> Result<AsyncArchive, Error> {
        let staging = self.staging.clone().ok_or(Error::WriterClosed)?;
        let destination = self.destination.clone();
        let staging_for_publish = staging.clone();
        let destination_for_publish = destination.clone();
        let builder = self.builder.take().ok_or(Error::WriterClosed)?;
        let limiter = self.limiter.clone();
        let permit = acquire_limiter(limiter.as_ref()).await?;
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            builder.finalize().map_err(Error::Builder)?;
            fs::rename(&staging_for_publish, &destination_for_publish).map_err(Error::Io)
        })
        .await
        .map_err(Error::Task)?;
        if let Err(error) = result {
            let _ = fs::remove_file(&staging);
            self.staging = None;
            return Err(error);
        }
        self.staging = None;
        AsyncArchive::open_with_options_and_limiter(destination, OpenOptions::default(), limiter)
            .await
    }

    /// Discards the unpublished staging file without touching the destination.
    ///
    /// This is the explicit cleanup path for callers that do not want to rely
    /// on `Drop`. A blocking cleanup operation is kept coarse and owns no
    /// archive publication state after this future completes.
    pub async fn abort(mut self) -> Result<(), Error> {
        let Some(staging) = self.staging.take() else {
            return Ok(());
        };
        self.builder = None;
        tokio::task::spawn_blocking(move || fs::remove_file(staging))
            .await
            .map_err(Error::Task)?
            .map_err(Error::Io)
    }

    async fn run_builder<T, F>(&mut self, operation: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&mut FileBuilder) -> Result<T, BuilderError> + Send + 'static,
    {
        let mut builder = self.builder.take().ok_or(Error::WriterClosed)?;
        let permit = acquire_limiter(self.limiter.as_ref()).await?;
        let (builder, result) = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let result = operation(&mut builder);
            (builder, result)
        })
        .await
        .map_err(Error::Task)?;
        self.builder = Some(builder);
        result.map_err(Error::Builder)
    }
}

impl Drop for AsyncArchiveWriter {
    fn drop(&mut self) {
        if let Some(staging) = self.staging.take() {
            let _ = fs::remove_file(staging);
        }
    }
}

struct OpenedArchive {
    file: fs::File,
    index: ArchiveIndex,
}

struct TemporaryPathGuard(Option<PathBuf>);

impl TemporaryPathGuard {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for TemporaryPathGuard {
    fn drop(&mut self) {
        let Some(path) = self.0.take() else {
            return;
        };
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = tokio::fs::remove_file(path).await;
            });
        } else {
            let _ = fs::remove_file(path);
        }
    }
}

async fn acquire_limiter(
    limiter: Option<&ConcurrencyLimiter>,
) -> Result<Option<OwnedSemaphorePermit>, Error> {
    match limiter {
        Some(limiter) => Ok(Some(limiter.acquire().await?)),
        None => Ok(None),
    }
}

fn open_blocking(path: &Path, options: OpenOptions) -> Result<OpenedArchive, Error> {
    let file = fs::File::open(path)?;
    let file_size = file.metadata()?.len();
    let mut header_bytes = [0u8; HEADER_SIZE];
    read_exact_at(&file, 0, &mut header_bytes)?;
    let header = Header::decode(&header_bytes)?;
    let petrified = header.flags & PETRIFICATION_FLAG != 0;
    let footer_end = header
        .footer_offset
        .checked_add(FOOTER_SIZE as u64)
        .ok_or(Error::InvalidArchive("footer range overflows"))?;
    if footer_end > file_size {
        return Err(Error::InvalidArchive("footer exceeds file size"));
    }
    let mut footer_bytes = [0u8; FOOTER_SIZE];
    read_exact_at(&file, header.footer_offset, &mut footer_bytes)?;
    let footer = Footer::decode(&footer_bytes)?;
    let index_end = footer
        .string_pool_offset
        .checked_add(footer.string_pool_size)
        .ok_or(Error::InvalidArchive("string-pool range overflows"))?;
    let index_limit = if petrified {
        file_size
    } else {
        header.footer_offset
    };
    if (petrified && footer.asset_offset < footer_end)
        || footer.asset_offset > index_end
        || index_end > index_limit
    {
        return Err(Error::InvalidArchive("index range is not ordered"));
    }
    let index_size = index_end - footer.asset_offset;
    if index_size > options.max_index_bytes {
        return Err(Error::LimitExceeded {
            what: "index",
            requested: index_size,
            limit: options.max_index_bytes,
        });
    }
    let index_len = usize::try_from(index_size).map_err(|_| Error::LimitExceeded {
        what: "index allocation",
        requested: index_size,
        limit: usize::MAX as u64,
    })?;
    let mut index_bytes = vec![0u8; index_len];
    read_exact_at(&file, footer.asset_offset, &mut index_bytes)?;
    let footer_verified = xxhash_rust::xxh3::xxh3_64(&index_bytes) == footer.footer_hash;
    if !petrified && !footer_verified {
        return Err(Error::InvalidArchive(
            "footer hash does not match the index",
        ));
    }

    let assets = decode_table(
        &index_bytes,
        footer.asset_offset,
        footer.asset_offset,
        footer.asset_count,
        ASSET_SIZE,
        Asset::decode,
        "asset table",
    )?;
    let pages = decode_table(
        &index_bytes,
        footer.asset_offset,
        footer.page_offset,
        footer.page_count,
        PAGE_SIZE,
        Page::decode,
        "page table",
    )?;
    let sections = decode_table(
        &index_bytes,
        footer.asset_offset,
        footer.section_offset,
        footer.section_count,
        SECTION_SIZE,
        Section::decode,
        "section table",
    )?;
    let metadata = decode_table(
        &index_bytes,
        footer.asset_offset,
        footer.metadata_offset,
        footer.metadata_count,
        METADATA_SIZE,
        Metadata::decode,
        "metadata table",
    )?;
    let expansions = if footer.expansion_count == 0 {
        Vec::new()
    } else {
        decode_table(
            &index_bytes,
            footer.asset_offset,
            footer.expansion_offset,
            footer.expansion_count,
            EXPANSION_SIZE,
            Expansion::decode,
            "expansion table",
        )?
    };
    let string_pool_start = table_range(
        footer.asset_offset,
        footer.string_pool_offset,
        footer.string_pool_size,
        index_bytes.len(),
        "string pool",
    )?;
    let string_pool_len = usize::try_from(footer.string_pool_size)
        .map_err(|_| Error::InvalidArchive("string pool size overflows"))?;
    let string_pool = index_bytes[string_pool_start..string_pool_start + string_pool_len].to_vec();

    for section in &sections {
        validate_string_pool_entry(&string_pool, section.title_offset)?;
        if section.parent_offset != bbf_format::NO_PARENT_OFFSET {
            validate_string_pool_entry(&string_pool, section.parent_offset)?;
        }
        if section.start_index > pages.len() as u64 {
            return Err(Error::InvalidArchive("section starts after the page table"));
        }
    }
    for metadata in &metadata {
        validate_string_pool_entry(&string_pool, metadata.key_offset)?;
        validate_string_pool_entry(&string_pool, metadata.value_offset)?;
        if metadata.parent_offset != bbf_format::NO_PARENT_OFFSET {
            validate_string_pool_entry(&string_pool, metadata.parent_offset)?;
        }
    }

    let payload_start = if petrified {
        index_end
    } else {
        HEADER_SIZE as u64
    };
    let payload_end = if petrified {
        file_size
    } else {
        footer.asset_offset
    };
    for asset in &assets {
        if asset.file_size > options.max_asset_bytes {
            return Err(Error::LimitExceeded {
                what: "asset",
                requested: asset.file_size,
                limit: options.max_asset_bytes,
            });
        }
        let end = asset
            .file_offset
            .checked_add(asset.file_size)
            .ok_or(Error::InvalidArchive("asset range overflows"))?;
        if asset.file_offset < payload_start || end > payload_end {
            return Err(Error::InvalidArchive(
                "asset payload is outside the payload region",
            ));
        }
    }
    for page in &pages {
        if page.asset_index >= assets.len() as u64 {
            return Err(Error::InvalidArchive("page references an unknown asset"));
        }
    }

    Ok(OpenedArchive {
        file,
        index: ArchiveIndex {
            header,
            footer,
            footer_verified,
            assets,
            pages,
            sections,
            metadata,
            expansions,
            string_pool,
        },
    })
}

fn decode_table<T>(
    bytes: &[u8],
    index_start: u64,
    offset: u64,
    count: u64,
    record_size: usize,
    decode: impl Fn(&[u8]) -> Result<T, DecodeError>,
    name: &'static str,
) -> Result<Vec<T>, Error> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let range_size = count
        .checked_mul(record_size as u64)
        .ok_or(Error::InvalidArchive("table range overflows"))?;
    let start = table_range(index_start, offset, range_size, bytes.len(), name)?;
    let count_usize =
        usize::try_from(count).map_err(|_| Error::InvalidArchive("table count overflows"))?;
    let range_size_usize =
        usize::try_from(range_size).map_err(|_| Error::InvalidArchive("table size overflows"))?;
    let mut records = Vec::with_capacity(count_usize);
    for chunk in bytes[start..start + range_size_usize].chunks_exact(record_size) {
        records.push(decode(chunk)?);
    }
    Ok(records)
}

fn table_range(
    index_start: u64,
    offset: u64,
    size: u64,
    index_len: usize,
    name: &'static str,
) -> Result<usize, Error> {
    let relative = offset
        .checked_sub(index_start)
        .ok_or(Error::InvalidArchive("table precedes index"))?;
    let end = relative
        .checked_add(size)
        .ok_or(Error::InvalidArchive("table range overflows"))?;
    if end > index_len as u64 {
        return Err(Error::InvalidArchive(name));
    }
    usize::try_from(relative).map_err(|_| Error::InvalidArchive("table offset overflows"))
}

fn validate_string_pool_entry(pool: &[u8], offset: u64) -> Result<(), Error> {
    let start = usize::try_from(offset)
        .map_err(|_| Error::InvalidArchive("string-pool offset overflows"))?;
    let tail = pool
        .get(start..)
        .ok_or(Error::InvalidArchive("string-pool offset is out of range"))?;
    if tail.contains(&0) {
        Ok(())
    } else {
        Err(Error::InvalidArchive(
            "string-pool entry is not NUL terminated",
        ))
    }
}

fn read_range_blocking(file: &fs::File, offset: u64, length: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = vec![0u8; length];
    read_exact_at(file, offset, &mut bytes)?;
    Ok(bytes)
}

fn verify_blocking(file: &fs::File, index: &ArchiveIndex) -> Result<VerificationReport, Error> {
    let mut verified_ranges = HashSet::with_capacity(index.assets.len());
    let mut buffer = vec![0u8; READ_BUFFER_SIZE];
    let mut input = BufReader::with_capacity(READ_BUFFER_SIZE, file.try_clone()?);
    let mut bytes_verified = 0u64;
    let mut assets_verified = 0usize;
    for (asset_index, asset) in index.assets.iter().enumerate() {
        let key = (asset.file_offset, asset.file_size);
        if !verified_ranges.insert(key) {
            continue;
        }
        let mut hasher = Xxh3::new();
        input.seek(SeekFrom::Start(asset.file_offset))?;
        let mut offset = 0u64;
        while offset < asset.file_size {
            let requested = usize::try_from((asset.file_size - offset).min(buffer.len() as u64))
                .expect("bounded verification read fits usize");
            input.read_exact(&mut buffer[..requested])?;
            hasher.update(&buffer[..requested]);
            offset += requested as u64;
        }
        let hash = hasher.digest128();
        if hash as u64 != asset.hash_low || (hash >> 64) as u64 != asset.hash_high {
            return Err(Error::HashMismatch {
                asset: asset_index as u64,
            });
        }
        bytes_verified = bytes_verified
            .checked_add(asset.file_size)
            .ok_or(Error::InvalidArchive("verification byte count overflows"))?;
        assets_verified += 1;
    }
    Ok(VerificationReport {
        footer_verified: index.footer_verified,
        assets_verified,
        bytes_verified,
    })
}

fn read_exact_at(file: &fs::File, mut offset: u64, mut buffer: &mut [u8]) -> io::Result<()> {
    while !buffer.is_empty() {
        let read = read_at(file, buffer, offset)?;
        if read == 0 {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "short BBF read"));
        }
        offset += read as u64;
        buffer = &mut buffer[read..];
    }
    Ok(())
}

#[cfg(unix)]
fn read_at(file: &fs::File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buffer, offset)
}

#[cfg(windows)]
fn read_at(file: &fs::File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buffer, offset)
}

#[cfg(not(any(unix, windows)))]
fn read_at(_file: &fs::File, _buffer: &mut [u8], _offset: u64) -> io::Result<usize> {
    Err(io::Error::new(
        ErrorKind::Unsupported,
        "positional BBF reads are unsupported on this platform",
    ))
}

fn create_unique_staging(destination: &Path) -> Result<PathBuf, Error> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let file_name = destination
        .file_name()
        .ok_or(Error::InvalidArchive("destination has no filename"))?;
    for _ in 0..100 {
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            ".{}.bbf-tokio-{}-{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            sequence
        );
        let path = parent.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                drop(file);
                return Ok(path);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(Error::Io(error)),
        }
    }
    Err(Error::Io(io::Error::new(
        ErrorKind::AlreadyExists,
        "could not create unique BBF staging",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bbf_format::MediaType;
    use bbf_io::Reader;
    use bbf_mux::{Builder, BuilderError};
    use std::io;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncRead, ReadBuf};

    fn test_path(suffix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("libbbf-rs-tokio-{stamp}-{suffix}"))
    }

    struct ChunkedReader {
        bytes: Vec<u8>,
        position: usize,
        chunk_size: usize,
        fail_after: Option<usize>,
    }

    impl AsyncRead for ChunkedReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            buffer: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.fail_after.is_some_and(|limit| self.position >= limit) {
                return Poll::Ready(Err(io::Error::other("injected reader failure")));
            }
            if self.position >= self.bytes.len() {
                return Poll::Ready(Ok(()));
            }
            let limit = self.fail_after.unwrap_or(self.bytes.len());
            let end = (self.position + self.chunk_size.max(1))
                .min(limit)
                .min(self.bytes.len());
            buffer.put_slice(&self.bytes[self.position..end]);
            self.position = end;
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn temporary_path_guard_removes_unpublished_staging() {
        let path = test_path("temporary-guard");
        std::fs::write(&path, b"staging").expect("staging");
        drop(TemporaryPathGuard::new(path.clone()));
        assert!(!path.exists());
    }

    #[test]
    fn async_error_preserves_format_error_source() {
        let error = Error::Format(DecodeError::TooShort {
            expected: 64,
            actual: 0,
        });
        assert!(StdError::source(&error).is_some());
    }

    #[tokio::test]
    async fn builds_reads_verifies_and_matches_sync_reader() {
        let source = test_path("source.png");
        let output = test_path("archive.bbf");
        let payload = b"async payload".to_vec();
        tokio::fs::write(&source, &payload).await.expect("source");

        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        let asset = writer
            .append_asset_file(&source, MediaType::Png.as_u8(), 7)
            .await
            .expect("asset");
        writer.append_page(asset, 9).expect("page");
        let archive = writer.finish().await.expect("finish");

        assert_eq!(archive.page(0).expect("page").asset_index, asset);
        assert_eq!(archive.read_asset(asset).await.expect("read"), payload);
        assert_eq!(archive.verify().await.expect("verify").assets_verified, 1);

        let mut expected = Builder::new();
        let expected_asset = expected
            .append_asset_file(&source, MediaType::Png.as_u8(), 7)
            .expect("sync asset");
        expected.append_page(expected_asset, 9).expect("sync page");
        assert_eq!(
            tokio::fs::read(&output).await.expect("async output bytes"),
            expected.build_bytes().expect("sync output bytes")
        );

        let sync = Reader::open(&output).expect("sync reader");
        let sync_asset = sync.asset(0).expect("asset").expect("present");
        assert_eq!(sync.asset_data(&sync_asset), Some(payload.as_slice()));

        let _ = tokio::fs::remove_file(source).await;
        let _ = tokio::fs::remove_file(output).await;
    }

    #[tokio::test]
    async fn streams_asset_ranges_without_retaining_the_payload() {
        use tokio::io::AsyncReadExt;

        let output = test_path("asset-reader.bbf");
        let payload = b"0123456789abcdef";
        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        let asset = writer
            .append_asset_reader(&payload[..], MediaType::Unknown.as_u8(), 0)
            .await
            .expect("asset");
        writer.append_page(asset, 0).expect("page");
        let archive = writer.finish().await.expect("finish");

        let mut reader = archive
            .asset_reader_range(asset, 3, 7)
            .await
            .expect("asset reader");
        assert_eq!(reader.remaining(), 7);
        let mut first = [0u8; 2];
        reader.read_exact(&mut first).await.expect("first read");
        assert_eq!(&first, b"34");
        assert_eq!(reader.remaining(), 5);

        let mut rest = Vec::new();
        reader.read_to_end(&mut rest).await.expect("remaining read");
        assert_eq!(rest, b"56789");
        assert_eq!(reader.remaining(), 0);

        let left = archive
            .asset_reader_range(asset, 0, 4)
            .await
            .expect("left reader");
        let right = archive
            .asset_reader_range(asset, 12, 4)
            .await
            .expect("right reader");
        let (left, right) = tokio::join!(
            async move {
                let mut left = left;
                let mut bytes = Vec::new();
                left.read_to_end(&mut bytes).await.expect("left read");
                bytes
            },
            async move {
                let mut right = right;
                let mut bytes = Vec::new();
                right.read_to_end(&mut bytes).await.expect("right read");
                bytes
            },
        );
        assert_eq!(left, b"0123");
        assert_eq!(right, b"cdef");

        let mut full_reader = archive.asset_reader(asset).await.expect("full reader");
        let mut full = Vec::new();
        full_reader.read_to_end(&mut full).await.expect("full read");
        assert_eq!(full, payload);

        let _ = tokio::fs::remove_file(output).await;
    }

    #[tokio::test]
    async fn asset_reader_uses_the_open_archive_handle() {
        use tokio::io::AsyncReadExt;

        let output = test_path("asset-reader-retained.bbf");
        let replacement = test_path("asset-reader-replacement.bbf");
        let payload = b"retained archive payload";
        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        let asset = writer
            .append_asset_reader(&payload[..], MediaType::Unknown.as_u8(), 0)
            .await
            .expect("asset");
        writer.append_page(asset, 0).expect("page");
        let archive = writer.finish().await.expect("finish");

        tokio::fs::rename(&output, &replacement)
            .await
            .expect("move original archive");
        tokio::fs::write(&output, b"replacement archive")
            .await
            .expect("write replacement archive");

        let mut reader = archive.asset_reader(asset).await.expect("asset reader");
        let mut actual = Vec::new();
        reader.read_to_end(&mut actual).await.expect("read asset");
        assert_eq!(actual, payload);

        let _ = tokio::fs::remove_file(output).await;
        let _ = tokio::fs::remove_file(replacement).await;
    }

    #[tokio::test]
    async fn opens_verifies_and_reads_petrified_archives() {
        let output = test_path("petrify-input.bbf");
        let petrified = test_path("petrify-output.bbf");
        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        let asset = writer
            .append_asset_reader(&b"petrified payload"[..], MediaType::Png.as_u8(), 0)
            .await
            .expect("asset");
        writer.append_page(asset, 0).expect("page");
        let archive = writer.finish().await.expect("finish");

        let transformed = archive.petrify_to(&petrified).await.expect("petrify");
        assert_ne!(transformed.header().flags & PETRIFICATION_FLAG, 0);
        assert_eq!(
            transformed.read_asset(asset).await.expect("read"),
            b"petrified payload"
        );
        let report = transformed.verify().await.expect("verify");
        assert!(!report.footer_verified);
        assert_eq!(report.assets_verified, 1);

        let _ = tokio::fs::remove_file(output).await;
        let _ = tokio::fs::remove_file(petrified).await;
    }

    #[tokio::test]
    async fn reads_ranges_concurrently_and_ingests_non_seekable_input() {
        let output = test_path("concurrent.bbf");
        let first_payload = b"0123456789abcdef".to_vec();
        let second_payload = b"fedcba9876543210".to_vec();
        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        let first_asset = writer
            .append_asset_reader(&first_payload[..], MediaType::Unknown.as_u8(), 0)
            .await
            .expect("first asset");
        let second_asset = writer
            .append_asset_reader(&second_payload[..], MediaType::Unknown.as_u8(), 0)
            .await
            .expect("second asset");
        writer.append_page(first_asset, 0).expect("first page");
        writer.append_page(second_asset, 0).expect("second page");
        let archive = writer.finish().await.expect("finish");

        let first = archive.clone();
        let second = archive;
        let (left, right) = tokio::join!(
            first.read_asset_range(first_asset, 0, 4),
            second.read_asset_range(second_asset, 12, 4),
        );
        assert_eq!(left.expect("left"), b"0123");
        assert_eq!(right.expect("right"), b"3210");
        let _ = tokio::fs::remove_file(output).await;
    }

    #[tokio::test]
    async fn ingests_short_async_reads_and_recovers_after_reader_failure() {
        let output = test_path("reader-failure-recovery.bbf");
        let payload = b"delivered in deliberately short reads".to_vec();
        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");

        let asset = writer
            .add_asset_reader(
                ChunkedReader {
                    bytes: payload.clone(),
                    position: 0,
                    chunk_size: 2,
                    fail_after: None,
                },
                MediaType::Unknown.as_u8(),
                0,
            )
            .await
            .expect("short-read asset");
        writer.add_page(asset, 0).expect("short-read page");

        let failed = writer
            .add_asset_reader(
                ChunkedReader {
                    bytes: b"discarded before failure".to_vec(),
                    position: 0,
                    chunk_size: 3,
                    fail_after: Some(8),
                },
                MediaType::Unknown.as_u8(),
                0,
            )
            .await;
        assert!(matches!(
            failed,
            Err(Error::Io(error)) if error.kind() == io::ErrorKind::Other
        ));
        assert_eq!(writer.asset_count().expect("asset count"), 1);

        let archive = writer.finish().await.expect("finish after failed input");
        assert_eq!(
            archive.read_asset(asset).await.expect("read asset"),
            payload
        );

        let _ = tokio::fs::remove_file(output).await;
    }

    #[tokio::test]
    async fn shares_a_caller_owned_limiter_across_archive_operations() {
        assert!(matches!(
            ConcurrencyLimiter::new(0),
            Err(Error::InvalidConcurrencyLimit)
        ));

        let output = test_path("limited-operations.bbf");
        let limiter = ConcurrencyLimiter::new(1).expect("limiter");
        let mut writer = AsyncArchiveWriter::create_with_limiter(&output, limiter.clone())
            .await
            .expect("writer");
        let asset = writer
            .append_asset_reader(&b"limited operations"[..], MediaType::Unknown.as_u8(), 0)
            .await
            .expect("asset");
        writer.append_page(asset, 0).expect("page");
        let archive = writer.finish().await.expect("finish");

        let first = archive.clone().with_limiter(limiter.clone());
        let second = archive.with_limiter(limiter);
        let (left, right) = tokio::join!(first.read_asset_range(asset, 0, 7), second.verify(),);
        assert_eq!(left.expect("range"), b"limited");
        assert_eq!(right.expect("verify").assets_verified, 1);

        let _ = tokio::fs::remove_file(output).await;
    }

    #[tokio::test]
    async fn async_writer_preserves_non_utf8_metadata_and_sections() {
        let output = test_path("non-utf8-records.bbf");
        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        let asset = writer
            .append_asset_reader(&b"payload"[..], MediaType::Unknown.as_u8(), 0)
            .await
            .expect("asset");
        writer.append_page(asset, 0).expect("page");
        writer
            .add_meta_bytes(b"\xffkey", b"\xffvalue", Some(b"\xffparent"))
            .expect("metadata");
        writer
            .add_section_bytes(b"\xffsection", 0, Some(b"\xffparent"))
            .expect("section");
        let archive = writer.finish().await.expect("finish");

        let metadata = archive.metadata(0).expect("metadata record");
        assert_eq!(
            archive.string_bytes(metadata.key_offset).expect("key"),
            Some(&b"\xffkey"[..])
        );
        assert_eq!(
            archive.string_bytes(metadata.value_offset).expect("value"),
            Some(&b"\xffvalue"[..])
        );
        assert_eq!(
            archive
                .string_bytes(metadata.parent_offset)
                .expect("parent"),
            Some(&b"\xffparent"[..])
        );
        let section = archive.section(0).expect("section record");
        assert_eq!(
            archive.string_bytes(section.title_offset).expect("section"),
            Some(&b"\xffsection"[..])
        );

        let _ = tokio::fs::remove_file(output).await;
    }

    #[tokio::test]
    async fn failed_finish_preserves_existing_destination() {
        let output = test_path("preserve.bbf");
        let original = b"existing archive placeholder";
        tokio::fs::write(&output, original)
            .await
            .expect("destination");

        let writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        assert!(matches!(
            writer.finish().await,
            Err(Error::Builder(BuilderError::NoAssets))
        ));
        assert_eq!(
            tokio::fs::read(&output).await.expect("read destination"),
            original
        );
        let _ = tokio::fs::remove_file(output).await;
    }

    #[tokio::test]
    async fn aborting_async_writer_discards_unpublished_staging() {
        let output = test_path("writer-abort.bbf");
        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        writer
            .append_asset_reader(&b"unpublished payload"[..], MediaType::Unknown.as_u8(), 0)
            .await
            .expect("asset");

        writer.abort().await.expect("abort");
        assert!(!output.exists());
    }

    #[tokio::test]
    async fn rejects_oversized_index_before_allocating_it() {
        let output = test_path("index-limit.bbf");
        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        let asset = writer
            .append_asset_reader(&b"limited"[..], MediaType::Unknown.as_u8(), 0)
            .await
            .expect("asset");
        writer.append_page(asset, 0).expect("page");
        let _ = writer.finish().await.expect("finish");

        let result = AsyncArchive::open_with_options(
            &output,
            OpenOptions {
                max_index_bytes: 1,
                ..OpenOptions::default()
            },
        )
        .await;
        assert!(matches!(
            result,
            Err(Error::LimitExceeded { what: "index", .. })
        ));

        let result = AsyncFileAppender::open_with_options(
            &output,
            OpenOptions {
                max_index_bytes: 1,
                ..OpenOptions::default()
            },
        )
        .await;
        assert!(matches!(
            result,
            Err(Error::LimitExceeded {
                what: "append index",
                ..
            })
        ));
        let _ = tokio::fs::remove_file(output).await;
    }

    #[tokio::test]
    async fn rejects_corrupted_index_and_reports_payload_mutation() {
        let output = test_path("corruption.bbf");
        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        let asset = writer
            .append_asset_reader(&b"payload"[..], MediaType::Unknown.as_u8(), 0)
            .await
            .expect("asset");
        writer.append_page(asset, 0).expect("page");
        let archive = writer.finish().await.expect("finish");
        let asset_record = archive.asset(asset).expect("asset record");

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&output)
            .expect("open output");
        use std::io::{Seek, SeekFrom, Write};
        file.seek(SeekFrom::Start(asset_record.file_offset))
            .expect("seek payload");
        file.write_all(b"X").expect("mutate payload");
        drop(file);
        assert!(matches!(
            archive.verify().await,
            Err(Error::HashMismatch { asset: 0 })
        ));

        let mut bytes = tokio::fs::read(&output).await.expect("read output");
        let footer = bbf_io::Reader::from_bytes(bytes.clone())
            .footer()
            .expect("footer");
        bytes[footer.asset_offset as usize] ^= 1;
        tokio::fs::write(&output, bytes)
            .await
            .expect("mutate index");
        assert!(matches!(
            AsyncArchive::open(&output).await,
            Err(Error::InvalidArchive(
                "footer hash does not match the index"
            ))
        ));
        let _ = tokio::fs::remove_file(output).await;
    }

    #[tokio::test]
    async fn rejects_index_string_references_before_exposing_archive() {
        let output = test_path("bad-string-reference.bbf");
        let mut builder = Builder::new();
        builder.add_page_bytes(b"payload", MediaType::Unknown.as_u8(), 0, 0);
        builder.add_meta("key", "value", None);
        let mut bytes = builder.build_bytes().expect("archive");
        let reader = Reader::from_bytes(bytes.clone());
        let header = reader.header().expect("header");
        let mut footer = reader.footer().expect("footer");
        let metadata_offset = footer.metadata_offset as usize;
        bytes[metadata_offset + 8..metadata_offset + 16].copy_from_slice(&u64::MAX.to_le_bytes());
        let index_end = footer.string_pool_offset + footer.string_pool_size;
        footer.footer_hash =
            xxhash_rust::xxh3::xxh3_64(&bytes[footer.asset_offset as usize..index_end as usize]);
        let footer_start = header.footer_offset as usize;
        bytes[footer_start..footer_start + bbf_format::FOOTER_SIZE]
            .copy_from_slice(&footer.encode());
        tokio::fs::write(&output, bytes)
            .await
            .expect("write malformed archive");

        assert!(matches!(
            AsyncArchive::open(&output).await,
            Err(Error::InvalidArchive("string-pool offset is out of range"))
        ));
        let _ = tokio::fs::remove_file(output).await;
    }

    #[tokio::test]
    async fn malformed_async_open_and_append_inputs_do_not_panic() {
        let mut source = Builder::new();
        source.add_page_bytes(b"payload", MediaType::Unknown.as_u8(), 0, 0);
        let base = source.build_bytes().expect("source archive");
        let reader = Reader::from_bytes(base.clone());
        let header = reader.header().expect("header");
        let footer = reader.footer().expect("footer");
        let footer_start = header.footer_offset as usize;

        let mut cases = vec![
            Vec::new(),
            vec![0],
            vec![0xff; bbf_format::HEADER_SIZE - 1],
            vec![0xff; bbf_format::HEADER_SIZE],
        ];

        let mut bad_footer_offset = base.clone();
        let mut bad_header = header;
        bad_header.footer_offset = u64::MAX;
        bad_footer_offset[..bbf_format::HEADER_SIZE].copy_from_slice(&bad_header.encode());
        cases.push(bad_footer_offset);

        let mut overflowing_string_pool = base.clone();
        let mut overflowing_footer = footer;
        overflowing_footer.string_pool_size = u64::MAX;
        overflowing_string_pool[footer_start..footer_start + bbf_format::FOOTER_SIZE]
            .copy_from_slice(&overflowing_footer.encode());
        cases.push(overflowing_string_pool);

        let mut reversed_index = base;
        let mut reversed_footer = footer;
        reversed_footer.asset_offset = u64::MAX;
        reversed_index[footer_start..footer_start + bbf_format::FOOTER_SIZE]
            .copy_from_slice(&reversed_footer.encode());
        cases.push(reversed_index);

        for (index, bytes) in cases.into_iter().enumerate() {
            let output = test_path(&format!("malformed-open-{index}.bbf"));
            tokio::fs::write(&output, bytes)
                .await
                .expect("write malformed input");

            let archive_result = tokio::spawn(AsyncArchive::open(output.clone()))
                .await
                .expect("archive open must not panic");
            assert!(archive_result.is_err(), "malformed archive was accepted");

            let appender_result = tokio::spawn(AsyncFileAppender::open(output.clone()))
                .await
                .expect("appender open must not panic");
            assert!(
                appender_result.is_err(),
                "malformed appender input was accepted"
            );
            let _ = tokio::fs::remove_file(output).await;
        }
    }

    #[tokio::test]
    async fn appends_to_a_sealed_archive_without_copying_old_payloads() {
        let output = test_path("append.bbf");
        let added = test_path("append.bin");
        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        let original_asset = writer
            .append_asset_reader(&b"original"[..], MediaType::Png.as_u8(), 0)
            .await
            .expect("original asset");
        writer
            .append_page(original_asset, 1)
            .expect("original page");
        writer.add_meta("title", "before", None).expect("metadata");
        writer.add_section("chapter", 0, None).expect("section");
        writer.finish().await.expect("finish");

        tokio::fs::write(&added, b"appended")
            .await
            .expect("added asset");
        let mut appender = AsyncFileAppender::open(&output).await.expect("appender");
        assert_eq!(appender.page_count().expect("page count"), 1);
        assert_eq!(appender.section_count().expect("section count"), 1);
        assert_eq!(appender.string(0).expect("string"), Some("title"));
        assert_eq!(
            appender.string_bytes(0).expect("raw string"),
            Some(&b"title"[..])
        );
        let added_asset = appender
            .add_asset_file(&added, MediaType::Jpg.as_u8(), 3)
            .await
            .expect("added asset");
        assert_eq!(
            appender.add_page_asset(added_asset, 2).expect("added page"),
            1
        );
        appender
            .replace_page(0, added_asset, 4)
            .expect("replaced page");
        appender
            .set_meta_bytes(b"title", b"\xffafter", None)
            .expect("replaced metadata");
        appender
            .add_section_bytes(b"\xffappendix", 1, None)
            .expect("added section");
        appender.finalize().await.expect("finalize");

        let archive = AsyncArchive::open(&output).await.expect("reopen");
        assert_eq!(archive.page_count(), 2);
        assert_eq!(
            archive.read_asset(added_asset).await.expect("read"),
            b"appended"
        );
        assert_eq!(
            archive
                .string_bytes(archive.metadata(0).unwrap().value_offset)
                .unwrap(),
            Some(&b"\xffafter"[..])
        );
        assert_eq!(
            archive
                .string_bytes(archive.section(1).unwrap().title_offset)
                .unwrap(),
            Some(&b"\xffappendix"[..])
        );

        let _ = tokio::fs::remove_file(output).await;
        let _ = tokio::fs::remove_file(added).await;
    }

    #[tokio::test]
    async fn aborting_async_append_keeps_the_previous_archive_view() {
        let output = test_path("append-abort.bbf");
        let added = test_path("append-abort.bin");
        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        let original_asset = writer
            .append_asset_reader(&b"original"[..], MediaType::Png.as_u8(), 0)
            .await
            .expect("original asset");
        writer
            .append_page(original_asset, 7)
            .expect("original page");
        writer.finish().await.expect("finish");
        tokio::fs::write(&added, b"unpublished")
            .await
            .expect("added asset");

        let mut appender = AsyncFileAppender::open(&output).await.expect("appender");
        let unpublished = appender
            .add_asset_file(&added, MediaType::Jpg.as_u8(), 0)
            .await
            .expect("unpublished asset");
        appender
            .replace_page(0, unpublished, 99)
            .expect("unpublished page edit");
        appender.abort();

        let archive = AsyncArchive::open(&output).await.expect("reopen");
        assert_eq!(archive.page_count(), 1);
        assert_eq!(archive.page(0).expect("page").asset_index, original_asset);
        assert_eq!(archive.page(0).expect("page").flags, 7);
        assert_eq!(
            archive
                .read_asset(original_asset)
                .await
                .expect("original read"),
            b"original"
        );

        let _ = tokio::fs::remove_file(output).await;
        let _ = tokio::fs::remove_file(added).await;
    }

    #[tokio::test]
    async fn async_appender_removes_records_without_copying_payloads() {
        let output = test_path("append-remove.bbf");
        let mut writer = AsyncArchiveWriter::create(&output).await.expect("writer");
        let first = writer
            .append_asset_reader(&b"first"[..], MediaType::Png.as_u8(), 0)
            .await
            .expect("first asset");
        let second = writer
            .append_asset_reader(&b"second"[..], MediaType::Jpg.as_u8(), 0)
            .await
            .expect("second asset");
        writer.append_page(first, 0).expect("first page");
        writer.append_page(second, 0).expect("second page");
        writer
            .add_meta("keep", "value", None)
            .expect("keep metadata");
        writer
            .add_meta("remove", "value", None)
            .expect("remove metadata");
        writer.add_section("first", 0, None).expect("first section");
        writer
            .add_section("second", 1, None)
            .expect("second section");
        writer.finish().await.expect("finish");

        let mut appender = AsyncFileAppender::open(&output).await.expect("appender");
        assert_eq!(
            appender.remove_page(0).expect("remove page").asset_index,
            first
        );
        appender.remove_metadata(1).expect("remove metadata");
        appender.remove_section(1).expect("remove section");
        appender.finalize().await.expect("finalize");

        let archive = AsyncArchive::open(&output).await.expect("reopen");
        assert_eq!(archive.page_count(), 1);
        assert_eq!(archive.metadata_count(), 1);
        assert_eq!(archive.section_count(), 1);
        assert_eq!(archive.page(0).expect("page").asset_index, second);
        assert_eq!(archive.section(0).expect("section").start_index, 0);
        assert_eq!(archive.read_asset(second).await.expect("asset"), b"second");

        let _ = tokio::fs::remove_file(output).await;
    }

    #[tokio::test]
    async fn scans_async_strings_to_the_string_pool_boundary() {
        let output = test_path("long-string.bbf");
        let mut builder = Builder::new();
        builder.add_page_bytes(b"payload", MediaType::Unknown.as_u8(), 0, 0);
        let long_value = "x".repeat(4096);
        builder.add_meta("comicinfo", &long_value, None);
        tokio::fs::write(&output, builder.build_bytes().expect("archive"))
            .await
            .expect("write archive");

        let archive = AsyncArchive::open(&output).await.expect("open archive");
        let metadata = archive.metadata(0).expect("metadata");
        assert_eq!(
            archive.string(metadata.value_offset).expect("string"),
            Some(long_value.as_str())
        );
        let _ = tokio::fs::remove_file(output).await;
    }
}
