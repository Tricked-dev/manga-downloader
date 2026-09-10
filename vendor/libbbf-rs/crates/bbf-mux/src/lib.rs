//! BBF muxing primitives.

use std::{
    fmt, fs,
    io::{BufReader, BufWriter, Cursor, ErrorKind, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    str,
    sync::atomic::{AtomicU64, Ordering},
};

use bbf_format::{
    ASSET_SIZE, Asset, DecodeError, EXPANSION_SIZE, Expansion, FOOTER_SIZE, Footer, HEADER_SIZE,
    Header, METADATA_SIZE, MediaType, Metadata, NO_PARENT_OFFSET, PAGE_SIZE, PETRIFICATION_FLAG,
    Page, SECTION_SIZE, Section, VARIABLE_REAM_SIZE_FLAG,
};
use bbf_io::{Reader, ReaderError};
use xxhash_rust::xxh3::Xxh3;

const DEFAULT_POOL_CAPACITY: usize = 4096;
const DEFAULT_TABLE_CAPACITY: usize = 4096;
const DEFAULT_ALIGNMENT: u32 = 12;
const DEFAULT_REAM_SIZE: u32 = 16;
const PETRIFY_TEMP_PATH: &str = "petrified.bbf.tmp";
static WRITE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
enum PetrifyStaging {
    UniqueSibling,
    ReferenceCompatibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slot {
    hash: u64,
    offset: u64,
}

impl Slot {
    const EMPTY: Self = Self { hash: 0, offset: 0 };

    const fn is_empty(self) -> bool {
        self.hash == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AssetSlot {
    low: u64,
    high: u64,
    asset_index: u64,
}

impl AssetSlot {
    const EMPTY: Self = Self {
        low: 0,
        high: 0,
        asset_index: 0,
    };

    const fn is_empty(self) -> bool {
        self.low == 0 && self.high == 0
    }
}

#[cfg_attr(feature = "component-bench", derive(Debug, Clone))]
#[cfg(feature = "component-bench")]
pub struct AssetLookup {
    entries: Vec<AssetSlot>,
    asset_count: usize,
}

#[cfg(not(feature = "component-bench"))]
#[derive(Debug, Clone)]
struct AssetLookup {
    entries: Vec<AssetSlot>,
    asset_count: usize,
}

impl AssetLookup {
    pub fn new() -> Self {
        Self {
            entries: vec![AssetSlot::EMPTY; DEFAULT_TABLE_CAPACITY],
            asset_count: 0,
        }
    }

    pub fn find(&self, low: u64, high: u64) -> Option<u64> {
        let mut slot = self.slot_for(low);
        loop {
            let entry = self.entries[slot];
            if entry.is_empty() {
                return None;
            }
            if entry.low == low && entry.high == high {
                return Some(entry.asset_index);
            }
            slot = (slot + 1) & (self.entries.len() - 1);
        }
    }

    pub fn insert(&mut self, low: u64, high: u64, asset_index: u64) {
        if self.asset_count * 10 > self.entries.len() * 7 {
            self.grow();
        }

        let mut slot = self.slot_for(low);
        while !self.entries[slot].is_empty() {
            slot = (slot + 1) & (self.entries.len() - 1);
        }
        self.entries[slot] = AssetSlot {
            low,
            high,
            asset_index,
        };
        self.asset_count += 1;
    }

    fn slot_for(&self, low: u64) -> usize {
        low as usize & (self.entries.len() - 1)
    }

    fn grow(&mut self) {
        let new_capacity = self.entries.len() * 2;
        let old_entries =
            std::mem::replace(&mut self.entries, vec![AssetSlot::EMPTY; new_capacity]);
        for entry in old_entries.into_iter().filter(|entry| !entry.is_empty()) {
            let mut slot = self.slot_for(entry.low);
            while !self.entries[slot].is_empty() {
                slot = (slot + 1) & (self.entries.len() - 1);
            }
            self.entries[slot] = entry;
        }
    }
}

impl Default for AssetLookup {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum BuilderError {
    Io(std::io::Error),
    InvalidInput(&'static str),
    Format(DecodeError),
    NoAssets,
    FileBuilderFailed,
    InvalidAlignment(u32),
    InvalidReamSize(u32),
}

impl fmt::Display for BuilderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "could not read BBF page: {error}"),
            Self::InvalidInput(reason) => write!(formatter, "invalid BBF input: {reason}"),
            Self::Format(error) => write!(formatter, "invalid BBF structure: {error}"),
            Self::NoAssets => write!(formatter, "cannot finalize a BBF without assets"),
            Self::FileBuilderFailed => write!(
                formatter,
                "file builder is unusable after a failed streaming append"
            ),
            Self::InvalidAlignment(value) => {
                write!(formatter, "invalid BBF alignment exponent: {value}")
            }
            Self::InvalidReamSize(value) => write!(formatter, "invalid BBF ream exponent: {value}"),
        }
    }
}

impl std::error::Error for BuilderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Format(error) => Some(error),
            Self::InvalidInput(_)
            | Self::NoAssets
            | Self::FileBuilderFailed
            | Self::InvalidAlignment(_)
            | Self::InvalidReamSize(_) => None,
        }
    }
}

impl From<std::io::Error> for BuilderError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<DecodeError> for BuilderError {
    fn from(error: DecodeError) -> Self {
        Self::Format(error)
    }
}

/// Errors returned while appending to an existing sealed BBF file.
#[derive(Debug)]
pub enum AppendError {
    Io(std::io::Error),
    Format(DecodeError),
    Builder(BuilderError),
    IndexTooLarge { requested: u64, limit: u64 },
    InvalidInput(&'static str),
    InvalidAssetIndex { index: u64, count: usize },
    InvalidPageIndex { index: u64, count: usize },
    InvalidMetadataIndex { index: u64, count: usize },
    InvalidSectionIndex { index: u64, count: usize },
    MissingString { offset: u64 },
}

impl fmt::Display for AppendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "could not append BBF file: {error}"),
            Self::Format(error) => write!(formatter, "invalid BBF structure: {error}"),
            Self::Builder(error) => write!(formatter, "could not finalize appended BBF: {error}"),
            Self::IndexTooLarge { requested, limit } => write!(
                formatter,
                "BBF append index size {requested} exceeds configured limit {limit}"
            ),
            Self::InvalidInput(reason) => write!(formatter, "invalid BBF append input: {reason}"),
            Self::InvalidAssetIndex { index, count } => {
                write!(
                    formatter,
                    "asset index {index} is outside asset count {count}"
                )
            }
            Self::InvalidPageIndex { index, count } => {
                write!(
                    formatter,
                    "page index {index} is outside page count {count}"
                )
            }
            Self::InvalidMetadataIndex { index, count } => {
                write!(
                    formatter,
                    "metadata index {index} is outside metadata count {count}"
                )
            }
            Self::InvalidSectionIndex { index, count } => {
                write!(
                    formatter,
                    "section index {index} is outside section count {count}"
                )
            }
            Self::MissingString { offset } => {
                write!(
                    formatter,
                    "string-pool offset {offset} is not a valid string"
                )
            }
        }
    }
}

impl std::error::Error for AppendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Format(error) => Some(error),
            Self::Builder(error) => Some(error),
            Self::IndexTooLarge { .. }
            | Self::InvalidInput(_)
            | Self::InvalidAssetIndex { .. }
            | Self::InvalidPageIndex { .. }
            | Self::InvalidMetadataIndex { .. }
            | Self::InvalidSectionIndex { .. }
            | Self::MissingString { .. } => None,
        }
    }
}

impl From<std::io::Error> for AppendError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<DecodeError> for AppendError {
    fn from(error: DecodeError) -> Self {
        Self::Format(error)
    }
}

impl From<BuilderError> for AppendError {
    fn from(error: BuilderError) -> Self {
        Self::Builder(error)
    }
}

#[derive(Debug)]
pub enum EditError {
    Reader(ReaderError),
    Builder(BuilderError),
    InvalidInput(&'static str),
    InvalidAssetIndex {
        index: u64,
        count: usize,
    },
    InvalidPageIndex {
        index: u64,
        count: usize,
    },
    InvalidExpansionIndex {
        index: u64,
        count: usize,
    },
    InvalidSectionIndex {
        index: u64,
        count: usize,
    },
    InvalidMetadataIndex {
        index: u64,
        count: usize,
    },
    InvalidPageAssetReference {
        page: u64,
        asset: u64,
        count: usize,
    },
    MissingString {
        offset: u64,
    },
    MissingAssetData {
        index: u64,
    },
    AssetSizeMismatch {
        index: u64,
        expected: u64,
        actual: usize,
    },
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reader(error) => write!(formatter, "could not read BBF for editing: {error}"),
            Self::Builder(error) => write!(formatter, "could not encode edited BBF: {error}"),
            Self::InvalidInput(reason) => write!(formatter, "invalid BBF edit input: {reason}"),
            Self::InvalidAssetIndex { index, count } => {
                write!(
                    formatter,
                    "asset index {index} is outside asset count {count}"
                )
            }
            Self::InvalidPageIndex { index, count } => {
                write!(
                    formatter,
                    "page index {index} is outside page count {count}"
                )
            }
            Self::InvalidExpansionIndex { index, count } => write!(
                formatter,
                "expansion index {index} is outside expansion count {count}"
            ),
            Self::InvalidSectionIndex { index, count } => write!(
                formatter,
                "section index {index} is outside section count {count}"
            ),
            Self::InvalidMetadataIndex { index, count } => write!(
                formatter,
                "metadata index {index} is outside metadata count {count}"
            ),
            Self::InvalidPageAssetReference { page, asset, count } => write!(
                formatter,
                "page {page} references asset {asset}, outside asset count {count}"
            ),
            Self::MissingString { offset } => {
                write!(
                    formatter,
                    "string-pool offset {offset} is not a valid string"
                )
            }
            Self::MissingAssetData { index } => {
                write!(formatter, "asset {index} payload is outside the BBF")
            }
            Self::AssetSizeMismatch {
                index,
                expected,
                actual,
            } => write!(
                formatter,
                "asset {index} declares {expected} bytes but contains {actual} bytes"
            ),
        }
    }
}

impl std::error::Error for EditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Reader(error) => Some(error),
            Self::Builder(error) => Some(error),
            Self::InvalidInput(_)
            | Self::InvalidAssetIndex { .. }
            | Self::InvalidPageIndex { .. }
            | Self::InvalidExpansionIndex { .. }
            | Self::InvalidSectionIndex { .. }
            | Self::InvalidMetadataIndex { .. }
            | Self::InvalidPageAssetReference { .. }
            | Self::MissingString { .. }
            | Self::MissingAssetData { .. }
            | Self::AssetSizeMismatch { .. } => None,
        }
    }
}

impl From<ReaderError> for EditError {
    fn from(error: ReaderError) -> Self {
        Self::Reader(error)
    }
}

impl From<BuilderError> for EditError {
    fn from(error: BuilderError) -> Self {
        Self::Builder(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuilderConfig {
    pub alignment: u32,
    pub ream_size: u32,
    pub header_flags: u32,
}

impl Default for BuilderConfig {
    fn default() -> Self {
        Self {
            alignment: DEFAULT_ALIGNMENT,
            ream_size: DEFAULT_REAM_SIZE,
            header_flags: VARIABLE_REAM_SIZE_FLAG,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAsset {
    pub file_offset: u64,
    pub hash_low: u64,
    pub hash_high: u64,
    pub file_size: u64,
    pub flags: u32,
    pub media_type: u8,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StreamAsset {
    file_offset: u64,
    hash_low: u64,
    hash_high: u64,
    file_size: u64,
    flags: u32,
    media_type: u8,
}

/// In-memory builder state for BBF assets and logical pages.
#[derive(Debug, Clone)]
pub struct Builder {
    config: BuilderConfig,
    assets: Vec<PendingAsset>,
    pages: Vec<Page>,
    sections: Vec<Section>,
    metadata: Vec<Metadata>,
    expansions: Vec<Expansion>,
    asset_lookup: AssetLookup,
    string_pool: StringPool,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self::with_config(BuilderConfig::default())
    }

    pub fn with_config(config: BuilderConfig) -> Self {
        Self {
            config,
            assets: Vec::new(),
            pages: Vec::new(),
            sections: Vec::new(),
            metadata: Vec::new(),
            expansions: Vec::new(),
            asset_lookup: AssetLookup::new(),
            string_pool: StringPool::new(),
        }
    }

    pub fn config(&self) -> BuilderConfig {
        self.config
    }

    pub fn add_page(
        &mut self,
        path: impl AsRef<Path>,
        page_flags: u32,
        asset_flags: u32,
    ) -> Result<(), BuilderError> {
        let path = path.as_ref();
        let data = fs::read(path)?;
        self.add_page_owned(data, detect_media_type(path), page_flags, asset_flags);
        Ok(())
    }

    pub fn add_page_bytes(
        &mut self,
        data: &[u8],
        media_type: u8,
        page_flags: u32,
        asset_flags: u32,
    ) {
        self.add_page_owned(data.to_vec(), media_type, page_flags, asset_flags);
    }

    /// Appends a deduplicated asset without adding a page reference.
    pub fn append_asset_bytes(&mut self, data: &[u8], media_type: u8, asset_flags: u32) -> u64 {
        self.add_asset_owned(data.to_vec(), media_type, asset_flags)
    }

    /// Appends a file-backed asset without adding a page reference.
    pub fn append_asset_file(
        &mut self,
        path: impl AsRef<Path>,
        media_type: u8,
        asset_flags: u32,
    ) -> Result<u64, BuilderError> {
        let path = path.as_ref();
        Ok(self.add_asset_owned(fs::read(path)?, media_type, asset_flags))
    }

    /// Appends a page and its asset in one operation.
    pub fn append_page_bytes(
        &mut self,
        data: &[u8],
        media_type: u8,
        page_flags: u32,
        asset_flags: u32,
    ) -> u64 {
        let page_index = self.pages.len() as u64;
        self.add_page_bytes(data, media_type, page_flags, asset_flags);
        page_index
    }

    /// Appends a page referencing an already appended asset.
    pub fn append_page(&mut self, asset_index: u64, page_flags: u32) -> Result<u64, BuilderError> {
        if asset_index >= self.assets.len() as u64 {
            return Err(BuilderError::InvalidInput(
                "page references an unknown asset",
            ));
        }
        let page_index = self.pages.len() as u64;
        self.pages.push(Page {
            asset_index,
            flags: page_flags,
        });
        Ok(page_index)
    }

    fn add_page_owned(&mut self, data: Vec<u8>, media_type: u8, page_flags: u32, asset_flags: u32) {
        let asset_index = self.add_asset_owned(data, media_type, asset_flags);
        self.pages.push(Page {
            asset_index,
            flags: page_flags,
        });
    }

    fn add_asset_owned(&mut self, data: Vec<u8>, media_type: u8, asset_flags: u32) -> u64 {
        let hash = xxhash_rust::xxh3::xxh3_128(&data);
        let hash_low = hash as u64;
        let hash_high = (hash >> 64) as u64;
        if let Some(asset_index) = self.asset_lookup.find(hash_low, hash_high) {
            return asset_index;
        }

        let asset_index = self.assets.len() as u64;
        self.assets.push(PendingAsset {
            file_offset: 0,
            hash_low,
            hash_high,
            file_size: data.len() as u64,
            flags: asset_flags,
            media_type,
            data,
        });
        self.asset_lookup.insert(hash_low, hash_high, asset_index);
        asset_index
    }

    fn import_asset(&mut self, asset: Asset, data: Vec<u8>) -> u64 {
        let asset_index = self.assets.len() as u64;
        if self
            .asset_lookup
            .find(asset.hash_low, asset.hash_high)
            .is_none()
        {
            self.asset_lookup
                .insert(asset.hash_low, asset.hash_high, asset_index);
        }
        self.assets.push(PendingAsset {
            file_offset: 0,
            hash_low: asset.hash_low,
            hash_high: asset.hash_high,
            file_size: asset.file_size,
            flags: asset.flags,
            media_type: asset.media_type,
            data,
        });
        asset_index
    }

    fn rebuild_asset_lookup(&mut self) {
        let mut asset_lookup = AssetLookup::new();
        for (asset_index, asset) in self.assets.iter().enumerate() {
            if asset_lookup.find(asset.hash_low, asset.hash_high).is_none() {
                asset_lookup.insert(asset.hash_low, asset.hash_high, asset_index as u64);
            }
        }
        self.asset_lookup = asset_lookup;
    }

    pub fn assets(&self) -> &[PendingAsset] {
        &self.assets
    }

    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub fn asset_count(&self) -> usize {
        self.assets.len()
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn add_meta(&mut self, key: &str, value: &str, parent: Option<&str>) -> bool {
        self.add_meta_bytes(key.as_bytes(), value.as_bytes(), parent.map(str::as_bytes))
    }

    /// Adds metadata from raw bytes, preserving non-UTF-8 content.
    pub fn add_meta_bytes(&mut self, key: &[u8], value: &[u8], parent: Option<&[u8]>) -> bool {
        let key_offset = self.string_pool.add_bytes(key);
        let value_offset = self.string_pool.add_bytes(value);
        let parent_offset = parent
            .map(|parent| self.string_pool.add_bytes(parent))
            .unwrap_or(NO_PARENT_OFFSET);
        self.metadata.push(Metadata {
            key_offset,
            value_offset,
            parent_offset,
        });
        true
    }

    #[inline]
    pub fn add_section(&mut self, name: &str, start_index: u64, parent: Option<&str>) -> bool {
        self.add_section_bytes(name.as_bytes(), start_index, parent.map(str::as_bytes))
    }

    /// Adds a section from raw bytes, preserving non-UTF-8 content.
    #[inline]
    pub fn add_section_bytes(
        &mut self,
        name: &[u8],
        start_index: u64,
        parent: Option<&[u8]>,
    ) -> bool {
        if start_index > self.pages.len() as u64 {
            return false;
        }
        let parent_offset = parent
            .map(|parent| self.string_pool.add_bytes(parent))
            .unwrap_or(NO_PARENT_OFFSET);
        self.sections.push(Section {
            title_offset: self.string_pool.add_bytes(name),
            start_index,
            parent_offset,
        });
        true
    }

    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    pub fn metadata(&self) -> &[Metadata] {
        &self.metadata
    }

    pub fn expansions(&self) -> &[Expansion] {
        &self.expansions
    }

    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    pub fn metadata_count(&self) -> usize {
        self.metadata.len()
    }

    pub fn expansion_count(&self) -> usize {
        self.expansions.len()
    }

    pub fn add_expansion(&mut self, expansion: Expansion) {
        self.expansions.push(expansion);
    }

    /// Serializes the current builder state using the default BBF v3 layout.
    pub fn build_bytes(&mut self) -> Result<Vec<u8>, BuilderError> {
        let mut output = Cursor::new(Vec::with_capacity(self.output_capacity()?));
        self.serialize(&mut output)?;
        Ok(output.into_inner())
    }

    /// Writes the current builder state directly to a buffered file.
    pub fn write_to(&mut self, path: impl AsRef<Path>) -> Result<(), BuilderError> {
        self.output_capacity()?;
        let destination = path.as_ref();
        let (temporary_path, file) = create_unique_sibling(destination)?;
        let result = (|| {
            let mut output = BufWriter::with_capacity(64 * 1024, file);
            self.serialize(&mut output)?;
            output.flush().map_err(BuilderError::Io)?;
            drop(output);
            fs::rename(&temporary_path, destination).map_err(BuilderError::Io)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    fn output_capacity(&self) -> Result<usize, BuilderError> {
        if self.assets.is_empty() {
            return Err(BuilderError::NoAssets);
        }
        if self.config.alignment > 16 {
            return Err(BuilderError::InvalidAlignment(self.config.alignment));
        }
        if self.config.ream_size > 63 {
            return Err(BuilderError::InvalidReamSize(self.config.ream_size));
        }

        // Reserve the complete output envelope up front. The old zero-capacity
        // buffer repeatedly reallocated and copied the payload while assets
        // were appended, which made large-file muxing pay an avoidable copy
        // cost. The padding term is a safe upper bound for the alignment gap
        // before each asset; a smaller actual gap simply leaves capacity
        // unused.
        let asset_bytes = self.assets.iter().try_fold(0usize, |total, asset| {
            total
                .checked_add(asset.data.len())
                .ok_or(BuilderError::InvalidInput("BBF output size overflows"))
        })?;
        let index_bytes = self
            .assets
            .len()
            .checked_mul(ASSET_SIZE)
            .and_then(|size| size.checked_add(self.pages.len().checked_mul(PAGE_SIZE)?))
            .and_then(|size| size.checked_add(self.sections.len().checked_mul(SECTION_SIZE)?))
            .and_then(|size| size.checked_add(self.metadata.len().checked_mul(METADATA_SIZE)?))
            .and_then(|size| size.checked_add(self.expansions.len().checked_mul(EXPANSION_SIZE)?))
            .and_then(|size| size.checked_add(self.string_pool.used_size()))
            .ok_or(BuilderError::InvalidInput("BBF output size overflows"))?;
        let max_alignment_padding = self
            .assets
            .len()
            .checked_mul((1usize << self.config.alignment) - 1)
            .ok_or(BuilderError::InvalidInput("BBF output size overflows"))?;
        let output_capacity = HEADER_SIZE
            .checked_add(asset_bytes)
            .and_then(|size| size.checked_add(max_alignment_padding))
            .and_then(|size| size.checked_add(index_bytes))
            .and_then(|size| size.checked_add(FOOTER_SIZE))
            .ok_or(BuilderError::InvalidInput("BBF output size overflows"))?;

        Ok(output_capacity)
    }

    fn serialize<W: Write + Seek>(&mut self, output: &mut W) -> Result<(), BuilderError> {
        if self.assets.is_empty() {
            return Err(BuilderError::NoAssets);
        }
        if self.config.alignment > 16 {
            return Err(BuilderError::InvalidAlignment(self.config.alignment));
        }
        if self.config.ream_size > 63 {
            return Err(BuilderError::InvalidReamSize(self.config.ream_size));
        }

        output.write_all(&[0; HEADER_SIZE])?;
        let mut current_offset = HEADER_SIZE as u64;
        let variable_ream = self.config.header_flags & VARIABLE_REAM_SIZE_FLAG != 0;

        for asset in &mut self.assets {
            let mut alignment = 1u64 << self.config.alignment;
            if variable_ream && asset.file_size < (1u64 << self.config.ream_size) {
                alignment = 8;
            }
            let file_offset = align_up(current_offset, alignment)?;
            write_zeroes(output, file_offset - current_offset)?;
            current_offset = file_offset;
            output.write_all(&asset.data)?;
            current_offset = current_offset
                .checked_add(asset.file_size)
                .ok_or(BuilderError::InvalidAlignment(self.config.alignment))?;
            asset.file_offset = file_offset;
        }

        let asset_offset = current_offset;
        let mut index_hash = Xxh3::new();
        for asset in &self.assets {
            let wire_asset = Asset {
                file_offset: asset.file_offset,
                hash_low: asset.hash_low,
                hash_high: asset.hash_high,
                file_size: asset.file_size,
                flags: asset.flags,
                media_type: asset.media_type,
            };
            write_hashed(output, &mut index_hash, &wire_asset.encode())?;
        }
        current_offset = current_offset
            .checked_add((self.assets.len() * ASSET_SIZE) as u64)
            .ok_or(BuilderError::InvalidAlignment(self.config.alignment))?;

        let page_offset = current_offset;
        let page_bytes = encode_pages(&self.pages)?;
        write_hashed(output, &mut index_hash, &page_bytes)?;
        current_offset = current_offset
            .checked_add(page_bytes.len() as u64)
            .ok_or(BuilderError::InvalidAlignment(self.config.alignment))?;

        let section_offset = current_offset;
        for section in &self.sections {
            write_hashed(output, &mut index_hash, &section.encode())?;
        }
        current_offset = current_offset
            .checked_add((self.sections.len() * SECTION_SIZE) as u64)
            .ok_or(BuilderError::InvalidAlignment(self.config.alignment))?;

        let metadata_offset = current_offset;
        for metadata in &self.metadata {
            write_hashed(output, &mut index_hash, &metadata.encode())?;
        }
        current_offset = current_offset
            .checked_add((self.metadata.len() * METADATA_SIZE) as u64)
            .ok_or(BuilderError::InvalidAlignment(self.config.alignment))?;

        let expansion_offset = if self.expansions.is_empty() {
            0
        } else {
            current_offset
        };
        for expansion in &self.expansions {
            write_hashed(output, &mut index_hash, &expansion.encode())?;
        }
        current_offset = current_offset
            .checked_add((self.expansions.len() * EXPANSION_SIZE) as u64)
            .ok_or(BuilderError::InvalidAlignment(self.config.alignment))?;

        let string_pool_offset = current_offset;
        write_hashed(output, &mut index_hash, self.string_pool.raw_data())?;
        current_offset = current_offset
            .checked_add(self.string_pool.used_size() as u64)
            .ok_or(BuilderError::InvalidAlignment(self.config.alignment))?;

        let footer_offset = current_offset;
        let footer = Footer {
            asset_offset,
            page_offset,
            section_offset,
            metadata_offset,
            expansion_offset,
            string_pool_offset,
            string_pool_size: self.string_pool.used_size() as u64,
            asset_count: self.assets.len() as u64,
            page_count: self.pages.len() as u64,
            section_count: self.sections.len() as u64,
            metadata_count: self.metadata.len() as u64,
            expansion_count: self.expansions.len() as u64,
            footer_hash: index_hash.digest(),
        };
        output.write_all(&footer.encode())?;

        let header = Header::new(
            self.config.header_flags,
            self.config.alignment as u8,
            self.config.ream_size as u8,
            footer_offset,
        );
        output.seek(SeekFrom::Start(0))?;
        output.write_all(&header.encode())?;
        Ok(())
    }

    /// Reorders a default-layout BBF by streaming its index and asset ranges.
    pub fn petrify_file(
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
    ) -> Result<(), BuilderError> {
        Self::petrify_file_with_staging(
            input_path.as_ref(),
            output_path.as_ref(),
            PetrifyStaging::UniqueSibling,
        )
    }

    /// Petrifies a file using the reference implementation's shared staging
    /// path and rename-failure side effect. This is intended only for the
    /// compatibility CLI; library callers should use [`Self::petrify_file`].
    pub fn petrify_file_compat(
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
    ) -> Result<(), BuilderError> {
        Self::petrify_file_with_staging(
            input_path.as_ref(),
            output_path.as_ref(),
            PetrifyStaging::ReferenceCompatibility,
        )
    }

    fn petrify_file_with_staging(
        input_path: &Path,
        output_path: &Path,
        staging: PetrifyStaging,
    ) -> Result<(), BuilderError> {
        let input_file = fs::File::open(input_path)?;
        let input_size = input_file.metadata()?.len();
        let mut input = BufReader::new(input_file);

        let mut header_bytes = [0; HEADER_SIZE];
        input.read_exact(&mut header_bytes)?;
        let header = Header::decode(&header_bytes)?;
        if header.flags & PETRIFICATION_FLAG != 0 {
            return Err(BuilderError::InvalidInput("file is already petrified"));
        }

        let old_footer_offset = header.footer_offset;
        let footer_end = old_footer_offset
            .checked_add(FOOTER_SIZE as u64)
            .ok_or(BuilderError::InvalidInput("footer range overflows"))?;
        if footer_end > input_size {
            return Err(BuilderError::InvalidInput("missing footer"));
        }
        input.seek(SeekFrom::Start(old_footer_offset))?;
        let mut footer_bytes = [0; FOOTER_SIZE];
        input.read_exact(&mut footer_bytes)?;
        let footer = Footer::decode(&footer_bytes)?;

        let index_start = footer.asset_offset;
        if index_start < HEADER_SIZE as u64 || index_start > old_footer_offset {
            return Err(BuilderError::InvalidInput("invalid default-layout ranges"));
        }

        let index_size = old_footer_offset - index_start;
        let data_size = index_start - HEADER_SIZE as u64;
        let new_index_start = (HEADER_SIZE + FOOTER_SIZE) as u64;
        let new_data_start = new_index_start
            .checked_add(index_size)
            .ok_or(BuilderError::InvalidInput("petrified index overflows"))?;
        let shift_index = new_index_start as i128 - index_start as i128;
        let shift_data = new_data_start as i128 - HEADER_SIZE as i128;

        let mut new_footer = footer;
        new_footer.asset_offset = shift_offset(footer.asset_offset, shift_index)?;
        new_footer.page_offset = shift_offset(footer.page_offset, shift_index)?;
        new_footer.section_offset = shift_offset(footer.section_offset, shift_index)?;
        new_footer.metadata_offset = shift_offset(footer.metadata_offset, shift_index)?;
        new_footer.expansion_offset = if footer.expansion_offset == 0 {
            0
        } else {
            shift_offset(footer.expansion_offset, shift_index)?
        };
        new_footer.string_pool_offset = shift_offset(footer.string_pool_offset, shift_index)?;

        let asset_bytes = footer
            .asset_count
            .checked_mul(ASSET_SIZE as u64)
            .ok_or(BuilderError::InvalidInput("asset table is too large"))?;
        if asset_bytes > index_size {
            return Err(BuilderError::InvalidInput(
                "asset table exceeds index region",
            ));
        }

        let mut new_header = header;
        new_header.flags |= PETRIFICATION_FLAG;
        new_header.footer_offset = HEADER_SIZE as u64;

        let (temp_path, output_file) = match staging {
            PetrifyStaging::UniqueSibling => create_unique_sibling(output_path)?,
            PetrifyStaging::ReferenceCompatibility => {
                let temp_path = PathBuf::from(PETRIFY_TEMP_PATH);
                let output_file = fs::File::create(&temp_path)?;
                (temp_path, output_file)
            }
        };
        let mut output = BufWriter::with_capacity(64 * 1024, output_file);
        let remove_unique_staging = || {
            if matches!(staging, PetrifyStaging::UniqueSibling) {
                let _ = fs::remove_file(&temp_path);
            }
        };
        if let Err(error) = output.write_all(&new_header.encode()) {
            remove_unique_staging();
            return Err(error.into());
        }
        if let Err(error) = output.write_all(&new_footer.encode()) {
            remove_unique_staging();
            return Err(error.into());
        }

        if let Err(error) = input.seek(SeekFrom::Start(index_start)) {
            remove_unique_staging();
            return Err(error.into());
        }
        for _ in 0..footer.asset_count {
            let mut asset_bytes = [0; ASSET_SIZE];
            if let Err(error) = input.read_exact(&mut asset_bytes) {
                drop(output);
                let _ = fs::remove_file(&temp_path);
                return Err(error.into());
            }
            let mut asset = match Asset::decode(&asset_bytes) {
                Ok(asset) => asset,
                Err(error) => {
                    drop(output);
                    let _ = fs::remove_file(&temp_path);
                    return Err(error.into());
                }
            };
            asset.file_offset = match shift_offset(asset.file_offset, shift_data) {
                Ok(offset) => offset,
                Err(error) => {
                    drop(output);
                    let _ = fs::remove_file(&temp_path);
                    return Err(error);
                }
            };
            if let Err(error) = output.write_all(&asset.encode()) {
                drop(output);
                let _ = fs::remove_file(&temp_path);
                return Err(error.into());
            }
        }

        if let Err(error) = copy_range(&mut input, &mut output, index_size - asset_bytes) {
            remove_unique_staging();
            return Err(error);
        }
        if let Err(error) = input.seek(SeekFrom::Start(HEADER_SIZE as u64)) {
            remove_unique_staging();
            return Err(error.into());
        }
        if let Err(error) = copy_range(&mut input, &mut output, data_size) {
            remove_unique_staging();
            return Err(error);
        }
        if let Err(error) = output.flush() {
            remove_unique_staging();
            return Err(error.into());
        }

        drop(output);
        match fs::rename(&temp_path, output_path) {
            Ok(()) => Ok(()),
            Err(error) => {
                if matches!(staging, PetrifyStaging::UniqueSibling) {
                    let _ = fs::remove_file(&temp_path);
                }
                Err(error.into())
            }
        }
    }

    /// Reorders a default-layout BBF into the reference petrified layout.
    pub fn petrify_bytes(input: &[u8]) -> Result<Vec<u8>, BuilderError> {
        let header = Header::decode(
            input
                .get(..HEADER_SIZE)
                .ok_or(BuilderError::InvalidInput("missing header"))?,
        )?;
        if header.flags & PETRIFICATION_FLAG != 0 {
            return Err(BuilderError::InvalidInput("file is already petrified"));
        }

        let old_footer_offset = usize::try_from(header.footer_offset)
            .map_err(|_| BuilderError::InvalidInput("footer offset does not fit usize"))?;
        let footer_end = old_footer_offset
            .checked_add(FOOTER_SIZE)
            .ok_or(BuilderError::InvalidInput("footer range overflows"))?;
        let footer = Footer::decode(
            input
                .get(old_footer_offset..footer_end)
                .ok_or(BuilderError::InvalidInput("missing footer"))?,
        )?;

        let index_start = usize::try_from(footer.asset_offset)
            .map_err(|_| BuilderError::InvalidInput("index offset does not fit usize"))?;
        if index_start < HEADER_SIZE || old_footer_offset < index_start {
            return Err(BuilderError::InvalidInput("invalid default-layout ranges"));
        }

        let index_size = old_footer_offset - index_start;
        let data_size = index_start - HEADER_SIZE;
        let new_index_start = HEADER_SIZE + FOOTER_SIZE;
        let new_data_start = new_index_start
            .checked_add(index_size)
            .ok_or(BuilderError::InvalidInput("petrified index overflows"))?;
        let shift_index = new_index_start as i128 - index_start as i128;
        let shift_data = new_data_start as i128 - HEADER_SIZE as i128;

        let mut new_footer = footer;
        new_footer.asset_offset = shift_offset(footer.asset_offset, shift_index)?;
        new_footer.page_offset = shift_offset(footer.page_offset, shift_index)?;
        new_footer.section_offset = shift_offset(footer.section_offset, shift_index)?;
        new_footer.metadata_offset = shift_offset(footer.metadata_offset, shift_index)?;
        new_footer.expansion_offset = if footer.expansion_offset == 0 {
            0
        } else {
            shift_offset(footer.expansion_offset, shift_index)?
        };
        new_footer.string_pool_offset = shift_offset(footer.string_pool_offset, shift_index)?;

        let mut index = input[index_start..old_footer_offset].to_vec();
        let asset_bytes = usize::try_from(footer.asset_count)
            .ok()
            .and_then(|count| count.checked_mul(ASSET_SIZE))
            .ok_or(BuilderError::InvalidInput("asset table is too large"))?;
        if asset_bytes > index.len() {
            return Err(BuilderError::InvalidInput(
                "asset table exceeds index region",
            ));
        }
        for asset_index in 0..footer.asset_count {
            let start = usize::try_from(asset_index)
                .ok()
                .and_then(|index| index.checked_mul(ASSET_SIZE))
                .ok_or(BuilderError::InvalidInput("asset index does not fit usize"))?;
            let end = start + ASSET_SIZE;
            let mut asset = Asset::decode(&index[start..end])?;
            asset.file_offset = shift_offset(asset.file_offset, shift_data)?;
            index[start..end].copy_from_slice(&asset.encode());
        }

        let mut new_header = header;
        new_header.flags |= PETRIFICATION_FLAG;
        new_header.footer_offset = HEADER_SIZE as u64;

        let mut output = Vec::with_capacity(input.len());
        output.extend_from_slice(&new_header.encode());
        output.extend_from_slice(&new_footer.encode());
        output.extend_from_slice(&index);
        output.extend_from_slice(
            input
                .get(HEADER_SIZE..index_start)
                .ok_or(BuilderError::InvalidInput("missing asset data"))?,
        );
        debug_assert_eq!(data_size, index_start - HEADER_SIZE);
        Ok(output)
    }
}

/// In-memory editor for an existing BBF archive.
///
/// Opening an archive imports its assets, pages, metadata, and sections into
/// the normal BBF builder. Edits preserve table indices while the final
/// encoding may normalize physical offsets for petrified or nonstandard input.
#[derive(Debug, Clone)]
pub struct ArchiveEditor {
    builder: Builder,
}

impl ArchiveEditor {
    /// Opens and decodes an existing BBF archive from memory.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, EditError> {
        let reader = Reader::from_bytes(bytes);
        let header = reader.header()?;
        let indexed = reader.indexed()?;
        let footer = *indexed.footer();

        let mut builder = Builder::with_config(BuilderConfig {
            alignment: u32::from(header.alignment),
            ream_size: u32::from(header.ream_size),
            header_flags: header.flags & !PETRIFICATION_FLAG,
        });
        builder.string_pool = StringPool::from_raw(editor_string_pool(&reader, &footer)?);

        for index in 0..footer.asset_count {
            let asset = indexed
                .asset(index)?
                .ok_or(EditError::MissingAssetData { index })?;
            let data = indexed
                .asset_data(&asset)
                .ok_or(EditError::MissingAssetData { index })?;
            if data.len() as u64 != asset.file_size {
                return Err(EditError::AssetSizeMismatch {
                    index,
                    expected: asset.file_size,
                    actual: data.len(),
                });
            }
            builder.import_asset(asset, data.to_vec());
        }

        for index in 0..footer.page_count {
            let page = indexed.page(index)?.ok_or(EditError::InvalidPageIndex {
                index,
                count: usize::try_from(footer.page_count).unwrap_or(usize::MAX),
            })?;
            if page.asset_index >= builder.asset_count() as u64 {
                return Err(EditError::InvalidPageAssetReference {
                    page: index,
                    asset: page.asset_index,
                    count: builder.asset_count(),
                });
            }
            builder.pages.push(page);
        }

        for index in 0..footer.section_count {
            let section = indexed
                .section(index)?
                .ok_or(EditError::InvalidInput("missing section"))?;
            if section.start_index > builder.page_count() as u64 {
                return Err(EditError::InvalidInput(
                    "section starts after the page table",
                ));
            }
            editor_string(&indexed, section.title_offset)?;
            if section.parent_offset != NO_PARENT_OFFSET {
                editor_string(&indexed, section.parent_offset)?;
            }
            builder.sections.push(section);
        }

        for index in 0..footer.metadata_count {
            let metadata = indexed
                .metadata(index)?
                .ok_or(EditError::InvalidInput("missing metadata"))?;
            editor_string(&indexed, metadata.key_offset)?;
            editor_string(&indexed, metadata.value_offset)?;
            if metadata.parent_offset != NO_PARENT_OFFSET {
                editor_string(&indexed, metadata.parent_offset)?;
            }
            builder.metadata.push(metadata);
        }

        for index in 0..footer.expansion_count {
            let expansion = indexed
                .expansion(index)?
                .ok_or(EditError::InvalidInput("missing expansion"))?;
            builder.expansions.push(expansion);
        }

        Ok(Self { builder })
    }

    /// Opens an existing BBF archive from a file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EditError> {
        let bytes = fs::read(path).map_err(ReaderError::Io)?;
        Self::from_bytes(bytes)
    }

    pub fn config(&self) -> BuilderConfig {
        self.builder.config()
    }

    pub fn assets(&self) -> &[PendingAsset] {
        self.builder.assets()
    }

    pub fn pages(&self) -> &[Page] {
        self.builder.pages()
    }

    pub fn asset_count(&self) -> usize {
        self.builder.asset_count()
    }

    pub fn page_count(&self) -> usize {
        self.builder.page_count()
    }

    pub fn section_count(&self) -> usize {
        self.builder.section_count()
    }

    pub fn metadata_count(&self) -> usize {
        self.builder.metadata_count()
    }

    pub fn expansions(&self) -> &[Expansion] {
        self.builder.expansions()
    }

    pub fn expansion_count(&self) -> usize {
        self.builder.expansion_count()
    }

    pub fn sections(&self) -> &[Section] {
        self.builder.sections()
    }

    pub fn metadata(&self) -> &[Metadata] {
        self.builder.metadata()
    }

    /// Adds an asset from a file without adding a page that references it.
    pub fn add_asset(
        &mut self,
        path: impl AsRef<Path>,
        asset_flags: u32,
    ) -> Result<u64, EditError> {
        let path = path.as_ref();
        let data = fs::read(path).map_err(ReaderError::Io)?;
        Ok(self
            .builder
            .add_asset_owned(data, detect_media_type(path), asset_flags))
    }

    /// Appends a file-backed asset to this edited archive without adding a page.
    pub fn append_asset(
        &mut self,
        path: impl AsRef<Path>,
        asset_flags: u32,
    ) -> Result<u64, EditError> {
        self.add_asset(path, asset_flags)
    }

    /// Adds an asset without adding a page that references it.
    pub fn add_asset_bytes(&mut self, data: &[u8], media_type: u8, asset_flags: u32) -> u64 {
        self.builder
            .add_asset_owned(data.to_vec(), media_type, asset_flags)
    }

    /// Appends a deduplicated asset to this edited archive without adding a page.
    pub fn append_asset_bytes(&mut self, data: &[u8], media_type: u8, asset_flags: u32) -> u64 {
        self.add_asset_bytes(data, media_type, asset_flags)
    }

    /// Replaces an asset payload while keeping its asset index stable.
    pub fn replace_asset_bytes(
        &mut self,
        index: u64,
        data: &[u8],
        media_type: u8,
        asset_flags: u32,
    ) -> Result<(), EditError> {
        let asset = self.asset_mut(index)?;
        let hash = xxhash_rust::xxh3::xxh3_128(data);
        asset.file_offset = 0;
        asset.hash_low = hash as u64;
        asset.hash_high = (hash >> 64) as u64;
        asset.file_size = data.len() as u64;
        asset.flags = asset_flags;
        asset.media_type = media_type;
        asset.data.clear();
        asset.data.extend_from_slice(data);
        self.builder.rebuild_asset_lookup();
        Ok(())
    }

    pub fn set_asset_flags(&mut self, index: u64, flags: u32) -> Result<(), EditError> {
        self.asset_mut(index)?.flags = flags;
        Ok(())
    }

    pub fn set_asset_media_type(&mut self, index: u64, media_type: u8) -> Result<(), EditError> {
        self.asset_mut(index)?.media_type = media_type;
        Ok(())
    }

    /// Appends a page referencing an existing asset and returns its index.
    pub fn add_page(&mut self, asset_index: u64, page_flags: u32) -> Result<u64, EditError> {
        self.ensure_asset(asset_index)?;
        let page_index = self.page_count() as u64;
        self.builder.pages.push(Page {
            asset_index,
            flags: page_flags,
        });
        Ok(page_index)
    }

    /// Appends a page referencing an existing asset.
    pub fn append_page(&mut self, asset_index: u64, page_flags: u32) -> Result<u64, EditError> {
        self.add_page(asset_index, page_flags)
    }

    pub fn replace_page_asset(
        &mut self,
        page_index: u64,
        asset_index: u64,
    ) -> Result<(), EditError> {
        self.ensure_asset(asset_index)?;
        self.page_mut(page_index)?.asset_index = asset_index;
        Ok(())
    }

    pub fn set_page_flags(&mut self, page_index: u64, flags: u32) -> Result<(), EditError> {
        self.page_mut(page_index)?.flags = flags;
        Ok(())
    }

    /// Removes a page and keeps section boundaries attached to their logical
    /// position in the remaining page table.
    pub fn remove_page(&mut self, page_index: u64) -> Result<Page, EditError> {
        let count = self.page_count();
        let page_index_usize = usize::try_from(page_index).unwrap_or(usize::MAX);
        if page_index_usize >= count {
            return Err(EditError::InvalidPageIndex {
                index: page_index,
                count,
            });
        }
        let page = self.builder.pages.remove(page_index_usize);
        let new_page_count = self.page_count() as u64;
        for section in &mut self.builder.sections {
            if section.start_index > page_index {
                section.start_index -= 1;
            }
            section.start_index = section.start_index.min(new_page_count);
        }
        Ok(page)
    }

    pub fn add_meta(&mut self, key: &str, value: &str, parent: Option<&str>) -> bool {
        self.builder.add_meta(key, value, parent)
    }

    pub fn replace_metadata(
        &mut self,
        index: u64,
        key: &str,
        value: &str,
        parent: Option<&str>,
    ) -> Result<(), EditError> {
        self.replace_metadata_bytes(
            index,
            key.as_bytes(),
            value.as_bytes(),
            parent.map(str::as_bytes),
        )
    }

    pub fn replace_metadata_bytes(
        &mut self,
        index: u64,
        key: &[u8],
        value: &[u8],
        parent: Option<&[u8]>,
    ) -> Result<(), EditError> {
        let requested_index = index;
        let count = self.metadata_count();
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        if index >= count {
            return Err(EditError::InvalidMetadataIndex {
                index: requested_index,
                count,
            });
        }
        let key_offset = self.builder.string_pool.add_bytes(key);
        let value_offset = self.builder.string_pool.add_bytes(value);
        let parent_offset = parent
            .map(|parent| self.builder.string_pool.add_bytes(parent))
            .unwrap_or(NO_PARENT_OFFSET);
        let metadata = &mut self.builder.metadata[index];
        *metadata = Metadata {
            key_offset,
            value_offset,
            parent_offset,
        };
        Ok(())
    }

    pub fn remove_metadata(&mut self, index: u64) -> Result<Metadata, EditError> {
        let requested_index = index;
        let count = self.metadata_count();
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        if index >= count {
            return Err(EditError::InvalidMetadataIndex {
                index: requested_index,
                count,
            });
        }
        Ok(self.builder.metadata.remove(index))
    }

    pub fn add_meta_bytes(&mut self, key: &[u8], value: &[u8], parent: Option<&[u8]>) -> bool {
        self.builder.add_meta_bytes(key, value, parent)
    }

    pub fn add_section_bytes(
        &mut self,
        name: &[u8],
        start_index: u64,
        parent: Option<&[u8]>,
    ) -> bool {
        self.builder.add_section_bytes(name, start_index, parent)
    }

    pub fn add_section(&mut self, name: &str, start_index: u64, parent: Option<&str>) -> bool {
        self.builder.add_section(name, start_index, parent)
    }

    pub fn replace_section(
        &mut self,
        index: u64,
        name: &str,
        start_index: u64,
        parent: Option<&str>,
    ) -> Result<(), EditError> {
        self.replace_section_bytes(
            index,
            name.as_bytes(),
            start_index,
            parent.map(str::as_bytes),
        )
    }

    pub fn replace_section_bytes(
        &mut self,
        index: u64,
        name: &[u8],
        start_index: u64,
        parent: Option<&[u8]>,
    ) -> Result<(), EditError> {
        if start_index > self.page_count() as u64 {
            return Err(EditError::InvalidInput(
                "section starts after the page table",
            ));
        }
        let requested_index = index;
        let count = self.section_count();
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        if index >= count {
            return Err(EditError::InvalidSectionIndex {
                index: requested_index,
                count,
            });
        }
        let title_offset = self.builder.string_pool.add_bytes(name);
        let parent_offset = parent
            .map(|parent| self.builder.string_pool.add_bytes(parent))
            .unwrap_or(NO_PARENT_OFFSET);
        let section = &mut self.builder.sections[index];
        *section = Section {
            title_offset,
            start_index,
            parent_offset,
        };
        Ok(())
    }

    pub fn remove_section(&mut self, index: u64) -> Result<Section, EditError> {
        let requested_index = index;
        let count = self.section_count();
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        if index >= count {
            return Err(EditError::InvalidSectionIndex {
                index: requested_index,
                count,
            });
        }
        Ok(self.builder.sections.remove(index))
    }

    pub fn add_expansion(&mut self, expansion: Expansion) {
        self.builder.add_expansion(expansion);
    }

    pub fn replace_expansion(&mut self, index: u64, expansion: Expansion) -> Result<(), EditError> {
        let count = self.expansion_count();
        let target = self
            .builder
            .expansions
            .get_mut(usize::try_from(index).unwrap_or(usize::MAX))
            .ok_or(EditError::InvalidExpansionIndex { index, count })?;
        *target = expansion;
        Ok(())
    }

    pub fn build_bytes(&mut self) -> Result<Vec<u8>, EditError> {
        Ok(self.builder.build_bytes()?)
    }

    pub fn write_to(&mut self, path: impl AsRef<Path>) -> Result<(), EditError> {
        Ok(self.builder.write_to(path)?)
    }

    fn ensure_asset(&self, index: u64) -> Result<(), EditError> {
        if index >= self.asset_count() as u64 {
            return Err(EditError::InvalidAssetIndex {
                index,
                count: self.asset_count(),
            });
        }
        Ok(())
    }

    fn asset_mut(&mut self, index: u64) -> Result<&mut PendingAsset, EditError> {
        let count = self.asset_count();
        self.builder
            .assets
            .get_mut(usize::try_from(index).unwrap_or(usize::MAX))
            .ok_or(EditError::InvalidAssetIndex { index, count })
    }

    fn page_mut(&mut self, index: u64) -> Result<&mut Page, EditError> {
        let count = self.page_count();
        self.builder
            .pages
            .get_mut(usize::try_from(index).unwrap_or(usize::MAX))
            .ok_or(EditError::InvalidPageIndex { index, count })
    }
}

fn editor_string<D: AsRef<[u8]>>(
    reader: &bbf_io::IndexedReader<'_, D>,
    offset: u64,
) -> Result<Vec<u8>, EditError> {
    reader
        .string_bytes(offset)?
        .map(ToOwned::to_owned)
        .ok_or(EditError::MissingString { offset })
}

fn editor_string_pool(reader: &Reader<Vec<u8>>, footer: &Footer) -> Result<Vec<u8>, EditError> {
    let start = usize::try_from(footer.string_pool_offset)
        .map_err(|_| EditError::InvalidInput("string pool offset does not fit usize"))?;
    let size = usize::try_from(footer.string_pool_size)
        .map_err(|_| EditError::InvalidInput("string pool size does not fit usize"))?;
    let end = start
        .checked_add(size)
        .ok_or(EditError::InvalidInput("string pool range overflows"))?;
    reader
        .bytes()
        .get(start..end)
        .map(ToOwned::to_owned)
        .ok_or(EditError::Reader(ReaderError::OutOfBounds {
            offset: footer.string_pool_offset,
            size: footer.string_pool_size,
            file_size: reader.len() as u64,
        }))
}

/// File-backed BBF builder that streams new assets as they are added.
///
/// Unlike [`Builder`], this type does not retain asset payloads in memory. It
/// writes a blank header on creation, appends each unique asset during
/// [`Self::add_page`], and writes the tables, footer, and final header during
/// [`Self::finalize`]. A write or source-consistency failure after an append
/// marks the builder failed; its partial output must be discarded and later
/// append/finalize operations return [`BuilderError::FileBuilderFailed`].
pub struct FileBuilder {
    config: BuilderConfig,
    output: BufWriter<fs::File>,
    current_offset: u64,
    assets: Vec<StreamAsset>,
    pages: Vec<Page>,
    sections: Vec<Section>,
    metadata: Vec<Metadata>,
    expansions: Vec<Expansion>,
    asset_lookup: AssetLookup,
    string_pool: StringPool,
    failed: bool,
}

impl FileBuilder {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, BuilderError> {
        Self::with_config(path, BuilderConfig::default())
    }

    pub fn with_config(
        path: impl AsRef<Path>,
        config: BuilderConfig,
    ) -> Result<Self, BuilderError> {
        validate_config(config)?;
        let file = fs::File::create(path)?;
        let mut output = BufWriter::with_capacity(64 * 1024, file);
        output.write_all(&[0; HEADER_SIZE])?;
        Ok(Self {
            config,
            output,
            current_offset: HEADER_SIZE as u64,
            assets: Vec::new(),
            pages: Vec::new(),
            sections: Vec::new(),
            metadata: Vec::new(),
            expansions: Vec::new(),
            asset_lookup: AssetLookup::new(),
            string_pool: StringPool::new(),
            failed: false,
        })
    }

    pub fn config(&self) -> BuilderConfig {
        self.config
    }

    pub fn asset_count(&self) -> usize {
        self.assets.len()
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    pub fn metadata_count(&self) -> usize {
        self.metadata.len()
    }

    pub fn expansion_count(&self) -> usize {
        self.expansions.len()
    }

    /// Returns whether a failed append has made this builder unusable.
    pub fn is_failed(&self) -> bool {
        self.failed
    }

    /// Hashes a source file and streams it to the output when it is new.
    pub fn add_page(
        &mut self,
        path: impl AsRef<Path>,
        page_flags: u32,
        asset_flags: u32,
    ) -> Result<(), BuilderError> {
        self.add_page_with_media_type(
            &path,
            detect_media_type(path.as_ref()),
            page_flags,
            asset_flags,
        )
    }

    /// Adds a file with an explicit media type, preserving the streaming
    /// builder's deduplication and append behavior.
    pub fn add_page_with_media_type(
        &mut self,
        path: impl AsRef<Path>,
        media_type: u8,
        page_flags: u32,
        asset_flags: u32,
    ) -> Result<(), BuilderError> {
        if self.failed {
            return Err(BuilderError::FileBuilderFailed);
        }
        let path = path.as_ref();
        let mut input = BufReader::with_capacity(64 * 1024, fs::File::open(path)?);
        let file_size = input.get_ref().metadata()?.len();
        let asset_index =
            self.add_asset_from_reader(&mut input, file_size, media_type, asset_flags, None)?;
        self.pages.push(Page {
            asset_index,
            flags: page_flags,
        });
        Ok(())
    }

    /// Adds a file as a deduplicated asset without adding a page.
    pub fn add_asset_file(
        &mut self,
        path: impl AsRef<Path>,
        media_type: u8,
        asset_flags: u32,
    ) -> Result<u64, BuilderError> {
        if self.failed {
            return Err(BuilderError::FileBuilderFailed);
        }
        let mut input = BufReader::with_capacity(64 * 1024, fs::File::open(path)?);
        let file_size = input.get_ref().metadata()?.len();
        self.add_asset_from_reader(&mut input, file_size, media_type, asset_flags, None)
    }

    /// Appends a deduplicated file-backed asset without adding a page.
    pub fn append_asset_file(
        &mut self,
        path: impl AsRef<Path>,
        media_type: u8,
        asset_flags: u32,
    ) -> Result<u64, BuilderError> {
        self.add_asset_file(path, media_type, asset_flags)
    }

    /// Adds a previously ingested file as an asset without hashing it again.
    ///
    /// The copied bytes are still hashed while they are written and must match
    /// the supplied digest. This is used by async non-seekable ingestion after
    /// it has copied the input into bounded temporary staging.
    pub fn add_asset_file_with_hash(
        &mut self,
        path: impl AsRef<Path>,
        media_type: u8,
        asset_flags: u32,
        hash_low: u64,
        hash_high: u64,
    ) -> Result<u64, BuilderError> {
        if self.failed {
            return Err(BuilderError::FileBuilderFailed);
        }
        let mut input = BufReader::with_capacity(64 * 1024, fs::File::open(path)?);
        let file_size = input.get_ref().metadata()?.len();
        let hash = u128::from(hash_low) | (u128::from(hash_high) << 64);
        self.add_asset_from_reader(&mut input, file_size, media_type, asset_flags, Some(hash))
    }

    /// Adds a page referencing an existing streamed asset.
    pub fn add_page_asset(
        &mut self,
        asset_index: u64,
        page_flags: u32,
    ) -> Result<(), BuilderError> {
        if self.failed {
            return Err(BuilderError::FileBuilderFailed);
        }
        if asset_index >= self.assets.len() as u64 {
            return Err(BuilderError::InvalidInput(
                "page references an unknown asset",
            ));
        }
        self.pages.push(Page {
            asset_index,
            flags: page_flags,
        });
        Ok(())
    }

    /// Appends a page referencing an existing streamed asset.
    pub fn append_page(&mut self, asset_index: u64, page_flags: u32) -> Result<(), BuilderError> {
        self.add_page_asset(asset_index, page_flags)
    }

    fn add_asset_from_reader<R: Read + Seek>(
        &mut self,
        input: &mut R,
        file_size: u64,
        media_type: u8,
        asset_flags: u32,
        known_hash: Option<u128>,
    ) -> Result<u64, BuilderError> {
        let hash = match known_hash {
            Some(hash) => hash,
            None => hash_reader(input)?,
        };
        let hash_low = hash as u64;
        let hash_high = (hash >> 64) as u64;
        let asset_index = if let Some(asset_index) = self.asset_lookup.find(hash_low, hash_high) {
            asset_index
        } else {
            let mut alignment = 1u64 << self.config.alignment;
            if self.config.header_flags & VARIABLE_REAM_SIZE_FLAG != 0
                && file_size < (1u64 << self.config.ream_size)
            {
                alignment = 8;
            }
            let file_offset = align_up(self.current_offset, alignment)?;
            if let Err(error) = write_zeroes(&mut self.output, file_offset - self.current_offset) {
                self.failed = true;
                return Err(error);
            }
            let copied_hash = match input.seek(SeekFrom::Start(0)) {
                Ok(_) => match copy_reader(input, &mut self.output, file_size) {
                    Ok(hash) => hash,
                    Err(error) => {
                        self.failed = true;
                        return Err(error);
                    }
                },
                Err(error) => {
                    self.failed = true;
                    return Err(error.into());
                }
            };
            if copied_hash != hash {
                self.failed = true;
                return Err(BuilderError::InvalidInput("asset changed while being read"));
            }
            self.current_offset = file_offset.checked_add(file_size).ok_or_else(|| {
                self.failed = true;
                BuilderError::InvalidInput("BBF output size overflows")
            })?;

            let asset_index = self.assets.len() as u64;
            self.assets.push(StreamAsset {
                file_offset,
                hash_low,
                hash_high,
                file_size,
                flags: asset_flags,
                media_type,
            });
            self.asset_lookup.insert(hash_low, hash_high, asset_index);
            asset_index
        };

        Ok(asset_index)
    }

    #[cfg(test)]
    fn add_page_from_reader<R: Read + Seek>(
        &mut self,
        input: &mut R,
        file_size: u64,
        media_type: u8,
        page_flags: u32,
        asset_flags: u32,
    ) -> Result<(), BuilderError> {
        let asset_index =
            self.add_asset_from_reader(input, file_size, media_type, asset_flags, None)?;
        self.pages.push(Page {
            asset_index,
            flags: page_flags,
        });
        Ok(())
    }

    pub fn add_meta(&mut self, key: &str, value: &str, parent: Option<&str>) -> bool {
        if self.failed {
            return false;
        }
        self.add_meta_bytes(key.as_bytes(), value.as_bytes(), parent.map(str::as_bytes))
    }

    /// Adds metadata from raw bytes, preserving non-UTF-8 content.
    pub fn add_meta_bytes(&mut self, key: &[u8], value: &[u8], parent: Option<&[u8]>) -> bool {
        if self.failed {
            return false;
        }
        let key_offset = self.string_pool.add_bytes(key);
        let value_offset = self.string_pool.add_bytes(value);
        let parent_offset = parent
            .map(|parent| self.string_pool.add_bytes(parent))
            .unwrap_or(NO_PARENT_OFFSET);
        self.metadata.push(Metadata {
            key_offset,
            value_offset,
            parent_offset,
        });
        true
    }

    #[inline]
    pub fn add_section(&mut self, name: &str, start_index: u64, parent: Option<&str>) -> bool {
        self.add_section_bytes(name.as_bytes(), start_index, parent.map(str::as_bytes))
    }

    /// Adds a section from raw bytes, preserving non-UTF-8 content.
    #[inline]
    pub fn add_section_bytes(
        &mut self,
        name: &[u8],
        start_index: u64,
        parent: Option<&[u8]>,
    ) -> bool {
        if self.failed {
            return false;
        }
        if start_index > self.pages.len() as u64 {
            return false;
        }
        let parent_offset = parent
            .map(|parent| self.string_pool.add_bytes(parent))
            .unwrap_or(NO_PARENT_OFFSET);
        self.sections.push(Section {
            title_offset: self.string_pool.add_bytes(name),
            start_index,
            parent_offset,
        });
        true
    }

    pub fn add_expansion(&mut self, expansion: Expansion) {
        if self.failed {
            return;
        }
        self.expansions.push(expansion);
    }

    /// Completes the file and closes its buffered output.
    pub fn finalize(mut self) -> Result<(), BuilderError> {
        if self.failed {
            return Err(BuilderError::FileBuilderFailed);
        }
        if self.assets.is_empty() {
            return Err(BuilderError::NoAssets);
        }
        validate_config(self.config)?;

        let asset_offset = self.current_offset;
        let mut index_hash = Xxh3::new();
        for asset in &self.assets {
            let wire_asset = Asset {
                file_offset: asset.file_offset,
                hash_low: asset.hash_low,
                hash_high: asset.hash_high,
                file_size: asset.file_size,
                flags: asset.flags,
                media_type: asset.media_type,
            };
            write_hashed(&mut self.output, &mut index_hash, &wire_asset.encode())?;
        }
        self.current_offset = self
            .current_offset
            .checked_add((self.assets.len() * ASSET_SIZE) as u64)
            .ok_or(BuilderError::InvalidInput("BBF output size overflows"))?;

        let page_offset = self.current_offset;
        let page_bytes = encode_pages(&self.pages)?;
        write_hashed(&mut self.output, &mut index_hash, &page_bytes)?;
        self.current_offset = self
            .current_offset
            .checked_add(page_bytes.len() as u64)
            .ok_or(BuilderError::InvalidInput("BBF output size overflows"))?;

        let section_offset = self.current_offset;
        for section in &self.sections {
            write_hashed(&mut self.output, &mut index_hash, &section.encode())?;
        }
        self.current_offset = self
            .current_offset
            .checked_add((self.sections.len() * SECTION_SIZE) as u64)
            .ok_or(BuilderError::InvalidInput("BBF output size overflows"))?;

        let metadata_offset = self.current_offset;
        for metadata in &self.metadata {
            write_hashed(&mut self.output, &mut index_hash, &metadata.encode())?;
        }
        self.current_offset = self
            .current_offset
            .checked_add((self.metadata.len() * METADATA_SIZE) as u64)
            .ok_or(BuilderError::InvalidInput("BBF output size overflows"))?;

        let expansion_offset = if self.expansions.is_empty() {
            0
        } else {
            self.current_offset
        };
        for expansion in &self.expansions {
            write_hashed(&mut self.output, &mut index_hash, &expansion.encode())?;
        }
        self.current_offset = self
            .current_offset
            .checked_add((self.expansions.len() * EXPANSION_SIZE) as u64)
            .ok_or(BuilderError::InvalidInput("BBF output size overflows"))?;

        let string_pool_offset = self.current_offset;
        write_hashed(
            &mut self.output,
            &mut index_hash,
            self.string_pool.raw_data(),
        )?;
        self.current_offset = self
            .current_offset
            .checked_add(self.string_pool.used_size() as u64)
            .ok_or(BuilderError::InvalidInput("BBF output size overflows"))?;

        let footer = Footer {
            asset_offset,
            page_offset,
            section_offset,
            metadata_offset,
            expansion_offset,
            string_pool_offset,
            string_pool_size: self.string_pool.used_size() as u64,
            asset_count: self.assets.len() as u64,
            page_count: self.pages.len() as u64,
            section_count: self.sections.len() as u64,
            metadata_count: self.metadata.len() as u64,
            expansion_count: self.expansions.len() as u64,
            footer_hash: index_hash.digest(),
        };
        let footer_offset = self.current_offset;
        self.output.write_all(&footer.encode())?;
        let header = Header::new(
            self.config.header_flags,
            self.config.alignment as u8,
            self.config.ream_size as u8,
            footer_offset,
        );
        self.output.seek(SeekFrom::Start(0))?;
        self.output.write_all(&header.encode())?;
        self.output.flush().map_err(BuilderError::Io)
    }
}

/// Appends records and payloads to an existing sealed BBF file.
///
/// Opening reads the header, footer, tables, and string pool, but never loads
/// existing asset payloads. New payloads and a replacement index/footer are
/// appended after the current end of file. The original header continues to
/// point at the old footer until [`Self::finalize`] succeeds, so dropping an
/// unfinished appender leaves the previous archive view readable (with any
/// appended bytes becoming unreachable trailing data).
pub struct FileAppender {
    builder: FileBuilder,
}

impl FileAppender {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppendError> {
        Self::open_with_index_limit(path, u64::MAX)
    }

    /// Opens an existing archive while bounding the index allocation.
    ///
    /// The default [`Self::open`] behavior remains unbounded for synchronous
    /// compatibility. Async callers should pass their configured index limit
    /// through this entry point before any index buffer is allocated.
    pub fn open_with_index_limit(
        path: impl AsRef<Path>,
        max_index_bytes: u64,
    ) -> Result<Self, AppendError> {
        let path = path.as_ref();
        let mut input = fs::File::open(path)?;
        let file_size = input.metadata()?.len();
        let mut header_bytes = [0u8; HEADER_SIZE];
        append_read_at(&mut input, 0, &mut header_bytes)?;
        let header = Header::decode(&header_bytes)?;
        let petrified = header.flags & PETRIFICATION_FLAG != 0;
        let footer_end = header
            .footer_offset
            .checked_add(FOOTER_SIZE as u64)
            .ok_or(AppendError::InvalidInput("footer range overflows"))?;
        if footer_end > file_size {
            return Err(AppendError::InvalidInput("footer exceeds file size"));
        }
        let mut footer_bytes = [0u8; FOOTER_SIZE];
        append_read_at(&mut input, header.footer_offset, &mut footer_bytes)?;
        let footer = Footer::decode(&footer_bytes)?;
        let index_end = footer
            .string_pool_offset
            .checked_add(footer.string_pool_size)
            .ok_or(AppendError::InvalidInput("string-pool range overflows"))?;
        let index_limit = if petrified {
            file_size
        } else {
            header.footer_offset
        };
        if (petrified && footer.asset_offset < footer_end)
            || footer.asset_offset > index_end
            || index_end > index_limit
        {
            return Err(AppendError::InvalidInput("index range is not ordered"));
        }
        let index_size = index_end - footer.asset_offset;
        if index_size > max_index_bytes {
            return Err(AppendError::IndexTooLarge {
                requested: index_size,
                limit: max_index_bytes,
            });
        }
        let index_len = usize::try_from(index_size)
            .map_err(|_| AppendError::InvalidInput("index does not fit usize"))?;
        let mut index = vec![0u8; index_len];
        append_read_at(&mut input, footer.asset_offset, &mut index)?;
        if !petrified && xxhash_rust::xxh3::xxh3_64(&index) != footer.footer_hash {
            return Err(AppendError::InvalidInput(
                "footer hash does not match the index",
            ));
        }

        let assets = append_decode_table(
            &index,
            footer.asset_offset,
            footer.asset_offset,
            footer.asset_count,
            ASSET_SIZE,
            Asset::decode,
            "asset table",
        )?;
        let pages = append_decode_table(
            &index,
            footer.asset_offset,
            footer.page_offset,
            footer.page_count,
            PAGE_SIZE,
            Page::decode,
            "page table",
        )?;
        let sections = append_decode_table(
            &index,
            footer.asset_offset,
            footer.section_offset,
            footer.section_count,
            SECTION_SIZE,
            Section::decode,
            "section table",
        )?;
        let metadata = append_decode_table(
            &index,
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
            append_decode_table(
                &index,
                footer.asset_offset,
                footer.expansion_offset,
                footer.expansion_count,
                EXPANSION_SIZE,
                Expansion::decode,
                "expansion table",
            )?
        };
        let string_pool = append_table_slice(
            &index,
            footer.asset_offset,
            footer.string_pool_offset,
            footer.string_pool_size,
            "string pool",
        )?
        .to_vec();

        for section in &sections {
            append_validate_string(&string_pool, section.title_offset)?;
            if section.parent_offset != NO_PARENT_OFFSET {
                append_validate_string(&string_pool, section.parent_offset)?;
            }
            if section.start_index > pages.len() as u64 {
                return Err(AppendError::InvalidInput(
                    "section starts after the page table",
                ));
            }
        }
        for metadata in &metadata {
            append_validate_string(&string_pool, metadata.key_offset)?;
            append_validate_string(&string_pool, metadata.value_offset)?;
            if metadata.parent_offset != NO_PARENT_OFFSET {
                append_validate_string(&string_pool, metadata.parent_offset)?;
            }
        }

        for asset in &assets {
            let end = asset
                .file_offset
                .checked_add(asset.file_size)
                .ok_or(AppendError::InvalidInput("asset range overflows"))?;
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
            if asset.file_offset < payload_start || end > payload_end {
                return Err(AppendError::InvalidInput(
                    "asset payload is outside the payload region",
                ));
            }
        }
        for page in &pages {
            if page.asset_index >= assets.len() as u64 {
                return Err(AppendError::InvalidInput(
                    "page references an unknown asset",
                ));
            }
        }

        let config = BuilderConfig {
            alignment: u32::from(header.alignment),
            ream_size: u32::from(header.ream_size),
            header_flags: header.flags & !PETRIFICATION_FLAG,
        };
        validate_config(config).map_err(AppendError::Builder)?;
        let mut asset_lookup = AssetLookup::new();
        let stream_assets = assets
            .into_iter()
            .enumerate()
            .map(|(index, asset)| {
                if asset_lookup.find(asset.hash_low, asset.hash_high).is_none() {
                    asset_lookup.insert(asset.hash_low, asset.hash_high, index as u64);
                }
                StreamAsset {
                    file_offset: asset.file_offset,
                    hash_low: asset.hash_low,
                    hash_high: asset.hash_high,
                    file_size: asset.file_size,
                    flags: asset.flags,
                    media_type: asset.media_type,
                }
            })
            .collect();

        drop(input);
        let mut output_file = fs::OpenOptions::new().read(true).write(true).open(path)?;
        output_file.seek(SeekFrom::End(0))?;
        let builder = FileBuilder {
            config,
            output: BufWriter::with_capacity(64 * 1024, output_file),
            current_offset: file_size,
            assets: stream_assets,
            pages,
            sections,
            metadata,
            expansions,
            asset_lookup,
            string_pool: StringPool::from_raw(string_pool),
            failed: false,
        };
        Ok(Self { builder })
    }

    pub fn page_count(&self) -> usize {
        self.builder.pages.len()
    }

    pub fn page(&self, index: u64) -> Option<Page> {
        self.builder
            .pages
            .get(usize::try_from(index).ok()?)
            .copied()
    }

    pub fn section_count(&self) -> usize {
        self.builder.sections.len()
    }

    pub fn section(&self, index: u64) -> Option<Section> {
        self.builder
            .sections
            .get(usize::try_from(index).ok()?)
            .copied()
    }

    pub fn string(&self, offset: u64) -> Option<&str> {
        self.builder.string_pool.get_string(offset)
    }

    /// Returns a raw NUL-terminated string-pool entry by relative offset.
    ///
    /// This is the byte-oriented counterpart to [`Self::string`] and keeps
    /// non-UTF-8 metadata inspectable during sealed-file edits.
    pub fn string_bytes(&self, offset: u64) -> Option<&[u8]> {
        self.builder.string_pool.get_bytes(offset)
    }

    pub fn add_asset_file(
        &mut self,
        path: impl AsRef<Path>,
        media_type: u8,
        asset_flags: u32,
    ) -> Result<u64, AppendError> {
        self.builder
            .add_asset_file(path, media_type, asset_flags)
            .map_err(AppendError::Builder)
    }

    pub fn add_page_asset(
        &mut self,
        asset_index: u64,
        page_flags: u32,
    ) -> Result<u64, AppendError> {
        let page_index = self.page_count() as u64;
        self.builder
            .add_page_asset(asset_index, page_flags)
            .map_err(AppendError::Builder)?;
        Ok(page_index)
    }

    pub fn replace_page(
        &mut self,
        page_index: u64,
        asset_index: u64,
        page_flags: u32,
    ) -> Result<(), AppendError> {
        if asset_index >= self.builder.assets.len() as u64 {
            return Err(AppendError::InvalidAssetIndex {
                index: asset_index,
                count: self.builder.assets.len(),
            });
        }
        let page_count = self.page_count();
        let page = self
            .builder
            .pages
            .get_mut(usize::try_from(page_index).unwrap_or(usize::MAX))
            .ok_or(AppendError::InvalidPageIndex {
                index: page_index,
                count: page_count,
            })?;
        *page = Page {
            asset_index,
            flags: page_flags,
        };
        Ok(())
    }

    /// Removes a page from the replacement index and rebases section starts
    /// to the remaining page sequence. Existing payload bytes are retained.
    pub fn remove_page(&mut self, page_index: u64) -> Result<Page, AppendError> {
        let count = self.page_count();
        let index = usize::try_from(page_index).unwrap_or(usize::MAX);
        if index >= count {
            return Err(AppendError::InvalidPageIndex {
                index: page_index,
                count,
            });
        }
        let page = self.builder.pages.remove(index);
        let new_page_count = self.page_count() as u64;
        for section in &mut self.builder.sections {
            if section.start_index > page_index {
                section.start_index -= 1;
            }
            section.start_index = section.start_index.min(new_page_count);
        }
        Ok(page)
    }

    pub fn add_section(
        &mut self,
        name: &str,
        start_index: u64,
        parent: Option<&str>,
    ) -> Result<(), AppendError> {
        self.add_section_bytes(name.as_bytes(), start_index, parent.map(str::as_bytes))
    }

    /// Adds a section while preserving arbitrary non-NUL bytes in the string pool.
    pub fn add_section_bytes(
        &mut self,
        name: &[u8],
        start_index: u64,
        parent: Option<&[u8]>,
    ) -> Result<(), AppendError> {
        if self.builder.add_section_bytes(name, start_index, parent) {
            Ok(())
        } else if self.builder.is_failed() {
            Err(AppendError::Builder(BuilderError::FileBuilderFailed))
        } else {
            Err(AppendError::InvalidInput(
                "section starts after the page table",
            ))
        }
    }

    pub fn set_meta(
        &mut self,
        key: &str,
        value: &str,
        parent: Option<&str>,
    ) -> Result<(), AppendError> {
        self.set_meta_bytes(key.as_bytes(), value.as_bytes(), parent.map(str::as_bytes))
    }

    /// Inserts or replaces metadata while preserving arbitrary non-NUL bytes.
    pub fn set_meta_bytes(
        &mut self,
        key: &[u8],
        value: &[u8],
        parent: Option<&[u8]>,
    ) -> Result<(), AppendError> {
        let key = append_c_bytes(key);
        let parent = parent.map(append_c_bytes);
        for metadata in &mut self.builder.metadata {
            let same_key = self.builder.string_pool.get_bytes(metadata.key_offset) == Some(key);
            let same_parent = match parent {
                Some(parent) => {
                    self.builder.string_pool.get_bytes(metadata.parent_offset) == Some(parent)
                }
                None => metadata.parent_offset == NO_PARENT_OFFSET,
            };
            if same_key && same_parent {
                metadata.value_offset = self.builder.string_pool.add_bytes(value);
                return Ok(());
            }
        }
        if self.builder.add_meta_bytes(key, value, parent) {
            Ok(())
        } else if self.builder.is_failed() {
            Err(AppendError::Builder(BuilderError::FileBuilderFailed))
        } else {
            Err(AppendError::InvalidInput("could not append metadata"))
        }
    }

    /// Removes a metadata record from the replacement index. Existing payload
    /// and string-pool bytes remain untouched.
    pub fn remove_metadata(&mut self, index: u64) -> Result<Metadata, AppendError> {
        let requested = index;
        let count = self.builder.metadata.len();
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        if index >= count {
            return Err(AppendError::InvalidMetadataIndex {
                index: requested,
                count,
            });
        }
        Ok(self.builder.metadata.remove(index))
    }

    /// Removes a section from the replacement index. Existing payload bytes
    /// remain untouched.
    pub fn remove_section(&mut self, index: u64) -> Result<Section, AppendError> {
        let count = self.section_count();
        let index_usize = usize::try_from(index).unwrap_or(usize::MAX);
        if index_usize >= count {
            return Err(AppendError::InvalidSectionIndex { index, count });
        }
        Ok(self.builder.sections.remove(index_usize))
    }

    pub fn finalize(self) -> Result<(), AppendError> {
        self.builder.finalize().map_err(AppendError::Builder)
    }
}

fn append_c_bytes(value: &[u8]) -> &[u8] {
    value.split(|byte| *byte == 0).next().unwrap_or(value)
}

fn append_validate_string(pool: &[u8], offset: u64) -> Result<(), AppendError> {
    let start = usize::try_from(offset).map_err(|_| AppendError::MissingString { offset })?;
    let tail = pool
        .get(start..)
        .ok_or(AppendError::MissingString { offset })?;
    if tail.contains(&0) {
        Ok(())
    } else {
        Err(AppendError::MissingString { offset })
    }
}

fn append_read_at(file: &mut fs::File, offset: u64, bytes: &mut [u8]) -> Result<(), AppendError> {
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(bytes)?;
    Ok(())
}

fn append_table_slice<'a>(
    bytes: &'a [u8],
    index_start: u64,
    offset: u64,
    size: u64,
    name: &'static str,
) -> Result<&'a [u8], AppendError> {
    let relative = offset
        .checked_sub(index_start)
        .ok_or(AppendError::InvalidInput("table precedes index"))?;
    let end = relative
        .checked_add(size)
        .ok_or(AppendError::InvalidInput("table range overflows"))?;
    if end > bytes.len() as u64 {
        return Err(AppendError::InvalidInput(name));
    }
    let start = usize::try_from(relative)
        .map_err(|_| AppendError::InvalidInput("table offset does not fit usize"))?;
    let end = usize::try_from(end)
        .map_err(|_| AppendError::InvalidInput("table end does not fit usize"))?;
    Ok(&bytes[start..end])
}

fn append_decode_table<T>(
    bytes: &[u8],
    index_start: u64,
    offset: u64,
    count: u64,
    record_size: usize,
    decode: impl Fn(&[u8]) -> Result<T, DecodeError>,
    name: &'static str,
) -> Result<Vec<T>, AppendError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let size = count
        .checked_mul(record_size as u64)
        .ok_or(AppendError::InvalidInput("table range overflows"))?;
    let slice = append_table_slice(bytes, index_start, offset, size, name)?;
    let count = usize::try_from(count)
        .map_err(|_| AppendError::InvalidInput("table count does not fit usize"))?;
    let mut values = Vec::with_capacity(count);
    for record in slice.chunks_exact(record_size) {
        values.push(decode(record)?);
    }
    Ok(values)
}

fn shift_offset(value: u64, shift: i128) -> Result<u64, BuilderError> {
    let shifted = value as i128 + shift;
    if shifted < 0 || shifted > u64::MAX as i128 {
        return Err(BuilderError::InvalidInput("shifted offset overflows"));
    }
    Ok(shifted as u64)
}

fn align_up(offset: u64, alignment: u64) -> Result<u64, BuilderError> {
    let remainder = offset % alignment;
    if remainder == 0 {
        return Ok(offset);
    }
    offset
        .checked_add(alignment - remainder)
        .ok_or(BuilderError::InvalidAlignment(u32::MAX))
}

fn write_zeroes<W: Write>(output: &mut W, mut count: u64) -> Result<(), BuilderError> {
    const ZEROES: [u8; 4096] = [0; 4096];
    while count > 0 {
        let chunk = usize::try_from(count.min(ZEROES.len() as u64)).unwrap_or(ZEROES.len());
        output.write_all(&ZEROES[..chunk])?;
        count -= chunk as u64;
    }
    Ok(())
}

fn validate_config(config: BuilderConfig) -> Result<(), BuilderError> {
    if config.alignment > 16 {
        return Err(BuilderError::InvalidAlignment(config.alignment));
    }
    if config.ream_size > 63 {
        return Err(BuilderError::InvalidReamSize(config.ream_size));
    }
    Ok(())
}

fn create_unique_sibling(path: &Path) -> Result<(PathBuf, fs::File), BuilderError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or(BuilderError::InvalidInput(
        "destination path has no filename",
    ))?;

    for _ in 0..100 {
        let sequence = WRITE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary_name = format!(
            ".{}.libbbf-rs-{}-{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            sequence
        );
        let temporary_path = parent.join(temporary_name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(BuilderError::Io(error)),
        }
    }

    Err(BuilderError::Io(std::io::Error::new(
        ErrorKind::AlreadyExists,
        "could not create a unique BBF staging file",
    )))
}

fn hash_reader<R: Read>(reader: &mut R) -> Result<u128, BuilderError> {
    let mut hasher = Xxh3::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(hasher.digest128());
        }
        hasher.update(&buffer[..read]);
    }
}

fn copy_reader<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    mut remaining: u64,
) -> Result<u128, BuilderError> {
    let mut hasher = Xxh3::new();
    let mut buffer = [0; 64 * 1024];
    while remaining > 0 {
        let requested = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = reader.read(&mut buffer[..requested])?;
        if read == 0 {
            return Err(BuilderError::InvalidInput("asset changed while being read"));
        }
        writer.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(hasher.digest128())
}

fn write_hashed<W: Write>(
    output: &mut W,
    hasher: &mut Xxh3,
    bytes: &[u8],
) -> Result<(), BuilderError> {
    output.write_all(bytes)?;
    hasher.update(bytes);
    Ok(())
}

fn encode_pages(pages: &[Page]) -> Result<Vec<u8>, BuilderError> {
    let byte_len = pages
        .len()
        .checked_mul(PAGE_SIZE)
        .ok_or(BuilderError::InvalidInput("BBF output size overflows"))?;
    let mut encoded = vec![0u8; byte_len];
    Page::encode_many_zeroed(pages, &mut encoded);
    Ok(encoded)
}

fn copy_range<R: Read, W: Write>(
    input: &mut R,
    output: &mut W,
    mut remaining: u64,
) -> Result<(), BuilderError> {
    let mut buffer = [0; 64 * 1024];
    while remaining > 0 {
        let chunk = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        input.read_exact(&mut buffer[..chunk])?;
        output.write_all(&buffer[..chunk])?;
        remaining -= chunk as u64;
    }
    Ok(())
}

pub fn detect_media_type(path: impl AsRef<Path>) -> u8 {
    let extension = path
        .as_ref()
        .extension()
        .and_then(|extension| extension.to_str());
    let Some(extension) = extension else {
        return MediaType::Unknown.as_u8();
    };

    // The reference packs at most four extension bytes before matching. Keep
    // that behavior so names such as `cover.jpegx` retain reference parity.
    let extension = extension.as_bytes();
    let extension = &extension[..extension.len().min(4)];
    if extension.eq_ignore_ascii_case(b"avif") {
        MediaType::Avif.as_u8()
    } else if extension.eq_ignore_ascii_case(b"png") {
        MediaType::Png.as_u8()
    } else if extension.eq_ignore_ascii_case(b"webp") {
        MediaType::Webp.as_u8()
    } else if extension.eq_ignore_ascii_case(b"jxl") {
        MediaType::Jxl.as_u8()
    } else if extension.eq_ignore_ascii_case(b"bmp") {
        MediaType::Bmp.as_u8()
    } else if extension.eq_ignore_ascii_case(b"gif") {
        MediaType::Gif.as_u8()
    } else if extension.eq_ignore_ascii_case(b"tiff") {
        MediaType::Tiff.as_u8()
    } else if extension.eq_ignore_ascii_case(b"jpg") || extension.eq_ignore_ascii_case(b"jpeg") {
        MediaType::Jpg.as_u8()
    } else {
        MediaType::Unknown.as_u8()
    }
}

/// A deduplicating, null-terminated byte string pool.
///
/// The pool uses the same open-addressed layout as the reference muxer: table
/// capacity is a power of two, lookup uses the low hash bits, and the table
/// grows at 75% load. Pool offsets are relative to the beginning of the pool;
/// the file writer will turn them into absolute offsets later.
#[derive(Debug, Clone)]
pub struct StringPool {
    data: Vec<u8>,
    entries: Vec<Slot>,
    entry_count: usize,
}

impl Default for StringPool {
    fn default() -> Self {
        Self::new()
    }
}

impl StringPool {
    pub fn new() -> Self {
        Self::with_capacities(DEFAULT_POOL_CAPACITY, DEFAULT_TABLE_CAPACITY)
    }

    pub fn with_capacities(pool_capacity: usize, table_capacity: usize) -> Self {
        let table_capacity = table_capacity.max(1).next_power_of_two();
        Self {
            data: Vec::with_capacity(pool_capacity),
            entries: vec![Slot::EMPTY; table_capacity],
            entry_count: 0,
        }
    }

    fn from_raw(data: Vec<u8>) -> Self {
        let mut pool = Self::with_capacities(data.len(), DEFAULT_TABLE_CAPACITY);
        pool.data = data;

        let mut offset = 0usize;
        while offset < pool.data.len() {
            let tail = &pool.data[offset..];
            let Some(length) = tail.iter().position(|byte| *byte == 0) else {
                break;
            };
            let value = &tail[..length];
            let hash = xxhash_rust::xxh3::xxh3_64(value);
            if (pool.entry_count + 1) * 4 > pool.entries.len() * 3 {
                pool.grow_table();
            }
            let mut slot = pool.slot_for(hash);
            while !pool.entries[slot].is_empty() {
                slot = (slot + 1) & (pool.entries.len() - 1);
            }
            pool.entries[slot] = Slot {
                hash,
                offset: offset as u64,
            };
            pool.entry_count += 1;
            offset += length + 1;
        }
        pool
    }

    /// Adds a UTF-8 string and returns its pool-relative offset.
    ///
    /// Existing strings return their original offset. The terminating NUL is
    /// part of the stored data and is included in [`Self::used_size`].
    pub fn add_string(&mut self, value: &str) -> u64 {
        self.add_bytes(value.as_bytes())
    }

    /// Adds raw bytes and returns their pool-relative offset.
    ///
    /// The reference uses C strings, so bytes after the first NUL are not
    /// stored. The terminating NUL is included in the pool data.
    #[inline]
    pub fn add_bytes(&mut self, value: &[u8]) -> u64 {
        let value = value.split(|byte| *byte == 0).next().unwrap_or(value);
        let hash = xxhash_rust::xxh3::xxh3_64(value);
        let mut slot = self.slot_for(hash);

        loop {
            let entry = self.entries[slot];
            if entry.is_empty() {
                break;
            }

            if entry.hash == hash && self.bytes_at(entry.offset) == Some(value) {
                return entry.offset;
            }

            slot = (slot + 1) & (self.entries.len() - 1);
        }

        if (self.entry_count + 1) * 4 > self.entries.len() * 3 {
            self.grow_table();
            slot = self.slot_for(hash);
            while !self.entries[slot].is_empty() {
                slot = (slot + 1) & (self.entries.len() - 1);
            }
        }

        let offset = self.data.len() as u64;
        self.data.extend_from_slice(value);
        self.data.push(0);
        self.entries[slot] = Slot { hash, offset };
        self.entry_count += 1;
        offset
    }

    /// Returns a stored string by pool-relative offset.
    pub fn get_string(&self, offset: u64) -> Option<&str> {
        self.string_at(offset)
    }

    /// Returns stored raw bytes by pool-relative offset.
    pub fn get_bytes(&self, offset: u64) -> Option<&[u8]> {
        self.bytes_at(offset)
    }

    pub fn raw_data(&self) -> &[u8] {
        &self.data
    }

    pub fn used_size(&self) -> usize {
        self.data.len()
    }

    pub fn entry_count(&self) -> usize {
        self.entry_count
    }

    fn slot_for(&self, hash: u64) -> usize {
        hash as usize & (self.entries.len() - 1)
    }

    fn string_at(&self, offset: u64) -> Option<&str> {
        str::from_utf8(self.bytes_at(offset)?).ok()
    }

    fn bytes_at(&self, offset: u64) -> Option<&[u8]> {
        let start = usize::try_from(offset).ok()?;
        let tail = self.data.get(start..)?;
        let end = tail.iter().position(|byte| *byte == 0)?;
        Some(&tail[..end])
    }

    fn grow_table(&mut self) {
        let new_capacity = self.entries.len() * 2;
        let old_entries = std::mem::replace(&mut self.entries, vec![Slot::EMPTY; new_capacity]);
        for entry in old_entries.into_iter().filter(|entry| !entry.is_empty()) {
            let mut slot = self.slot_for(entry.hash);
            while !self.entries[slot].is_empty() {
                slot = (slot + 1) & (self.entries.len() - 1);
            }
            self.entries[slot] = entry;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error as StdError,
        fs,
        io::{self, Read, Seek, SeekFrom, Write},
        process,
        sync::{Mutex, OnceLock},
    };

    use bbf_format::{
        Expansion, FOOTER_SIZE, HEADER_SIZE, MediaType, NO_PARENT_OFFSET, PETRIFICATION_FLAG,
        VARIABLE_REAM_SIZE_FLAG,
    };

    use super::{
        ArchiveEditor, Builder, BuilderConfig, BuilderError, EditError, FileAppender, FileBuilder,
        PETRIFY_TEMP_PATH, StringPool, copy_reader, detect_media_type,
    };

    fn petrify_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn mux_errors_preserve_nested_sources() {
        let builder = BuilderError::Io(io::Error::other("injected write failure"));
        let edit = EditError::Builder(builder);

        assert!(StdError::source(&edit).is_some());
        assert!(StdError::source(StdError::source(&edit).expect("builder source")).is_some());
    }

    struct CurrentDirectoryGuard(std::path::PathBuf);

    impl Drop for CurrentDirectoryGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.0).expect("restore test directory");
        }
    }

    struct ChangingReader {
        first: Vec<u8>,
        second: Vec<u8>,
        position: usize,
        changed: bool,
    }

    struct LimitedWriter {
        bytes: Vec<u8>,
        max_write: usize,
        fail_after: Option<usize>,
    }

    impl Write for LimitedWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if self
                .fail_after
                .is_some_and(|limit| self.bytes.len() >= limit)
            {
                return Err(io::Error::other("injected write failure"));
            }
            let mut amount = buffer.len().min(self.max_write.max(1));
            if let Some(limit) = self.fail_after {
                amount = amount.min(limit.saturating_sub(self.bytes.len()));
            }
            if amount == 0 {
                return Err(io::Error::other("injected write failure"));
            }
            self.bytes.extend_from_slice(&buffer[..amount]);
            Ok(amount)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Read for ChangingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let source = if self.changed {
                &self.second
            } else {
                &self.first
            };
            let remaining = source.len().saturating_sub(self.position);
            let amount = remaining.min(buffer.len());
            buffer[..amount].copy_from_slice(&source[self.position..self.position + amount]);
            self.position += amount;
            Ok(amount)
        }
    }

    impl Seek for ChangingReader {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            let target = match position {
                SeekFrom::Start(offset) => i128::from(offset),
                SeekFrom::Current(offset) => self.position as i128 + offset as i128,
                SeekFrom::End(offset) => self.first.len() as i128 + offset as i128,
            };
            if target < 0 || target > self.first.len() as i128 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "changing reader seek outside input",
                ));
            }
            if target == 0 && self.position != 0 {
                self.changed = true;
            }
            self.position = target as usize;
            Ok(target as u64)
        }
    }

    #[test]
    fn stores_null_terminated_strings_and_deduplicates_them() {
        let mut pool = StringPool::new();

        let alpha = pool.add_string("alpha");
        let beta = pool.add_string("beta");
        let duplicate = pool.add_string("alpha");
        let empty = pool.add_string("");

        assert_eq!(alpha, 0);
        assert_eq!(beta, 6);
        assert_eq!(duplicate, alpha);
        assert_eq!(empty, 11);
        assert_eq!(pool.raw_data(), b"alpha\0beta\0\0");
        assert_eq!(pool.entry_count(), 3);
        assert_eq!(pool.used_size(), 12);
    }

    #[test]
    fn preserves_raw_string_pool_bytes_and_c_string_truncation() {
        let mut pool = StringPool::new();
        let invalid = pool.add_bytes(&[0xff, b'k', 0, b'i']);

        assert_eq!(pool.get_bytes(invalid), Some(&[0xff, b'k'][..]));
        assert_eq!(pool.get_string(invalid), None);
        assert_eq!(pool.raw_data(), &[0xff, b'k', 0]);
    }

    #[test]
    fn builder_serializes_non_utf8_metadata_and_section_bytes() {
        let mut builder = Builder::new();
        builder.add_page_bytes(b"page", MediaType::Unknown.as_u8(), 0, 0);
        assert!(builder.add_meta_bytes(&[0xff, b'k'], &[0xfe, b'v'], Some(&[0xfd])));
        assert!(builder.add_section_bytes(&[0xfc], 0, Some(&[0xfb])));

        let reader = bbf_io::Reader::from_bytes(builder.build_bytes().expect("build raw BBF"));
        let metadata = reader.metadata(0).unwrap().unwrap();
        let section = reader.section(0).unwrap().unwrap();
        assert_eq!(reader.string(metadata.key_offset).unwrap(), None);
        assert_eq!(reader.string(section.title_offset).unwrap(), None);
        assert_ne!(metadata.parent_offset, NO_PARENT_OFFSET);
        assert_ne!(section.parent_offset, NO_PARENT_OFFSET);
    }

    #[test]
    fn returns_none_for_offsets_outside_or_inside_invalid_pool_data() {
        let mut pool = StringPool::with_capacities(8, 4);
        let offset = pool.add_string("valid");

        assert_eq!(pool.get_string(offset), Some("valid"));
        assert_eq!(pool.get_string(pool.used_size() as u64), None);
        assert_eq!(pool.get_string(u64::MAX), None);
    }

    #[test]
    fn grows_the_byte_buffer_and_hash_table() {
        let mut pool = StringPool::with_capacities(1, 4);
        for value in ["one", "two", "three", "four"] {
            pool.add_string(value);
        }

        assert_eq!(pool.entry_count(), 4);
        let duplicate = pool.add_string("three");
        assert_eq!(pool.get_string(duplicate), Some("three"));
        assert_eq!(pool.used_size(), 19);
    }

    #[test]
    fn deduplicates_equal_payloads_but_keeps_each_page() {
        let mut builder = Builder::new();
        builder.add_page_bytes(b"same", MediaType::Png.as_u8(), 1, 2);
        builder.add_page_bytes(b"same", MediaType::Jpg.as_u8(), 3, 4);

        assert_eq!(builder.asset_count(), 1);
        assert_eq!(builder.page_count(), 2);
        assert_eq!(builder.pages()[0].asset_index, 0);
        assert_eq!(builder.pages()[1].asset_index, 0);
        assert_eq!(builder.pages()[0].flags, 1);
        assert_eq!(builder.pages()[1].flags, 3);
        assert_eq!(builder.assets()[0].flags, 2);
        assert_eq!(builder.assets()[0].media_type, MediaType::Png.as_u8());
    }

    #[test]
    fn builder_serializes_expansion_records_without_changing_empty_tables() {
        let expansion = Expansion {
            reserved: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            flags: 0x55aa,
        };
        let mut builder = Builder::new();
        builder.add_page_bytes(b"payload", MediaType::Unknown.as_u8(), 0, 0);
        builder.add_expansion(expansion);

        let reader = bbf_io::Reader::from_bytes(builder.build_bytes().expect("build BBF"));
        let footer = reader.footer().expect("footer");
        assert_eq!(footer.expansion_count, 1);
        assert_ne!(footer.expansion_offset, 0);
        assert_eq!(reader.expansion(0).unwrap(), Some(expansion));
        assert_eq!(reader.expansion(1).unwrap(), None);
    }

    #[test]
    fn editor_reencodes_existing_assets_pages_and_strings() {
        let mut source = Builder::new();
        source.add_page_bytes(b"original", MediaType::Png.as_u8(), 1, 2);
        source.add_page_bytes(b"original", MediaType::Png.as_u8(), 3, 2);
        source.add_meta("title", "before", None);
        assert!(source.add_section("chapter", 1, None));
        let source_bytes = source.build_bytes().expect("source BBF");

        let mut untouched =
            ArchiveEditor::from_bytes(source_bytes.clone()).expect("open BBF editor");
        assert_eq!(
            untouched.build_bytes().expect("unchanged BBF"),
            source_bytes
        );

        let mut editor = ArchiveEditor::from_bytes(source_bytes).expect("open BBF editor");
        assert_eq!(editor.asset_count(), 1);
        assert_eq!(editor.page_count(), 2);
        assert_eq!(editor.section_count(), 1);
        assert_eq!(editor.metadata_count(), 1);

        let added_asset = editor.add_asset_bytes(b"added", MediaType::Jpg.as_u8(), 9);
        assert_eq!(added_asset, 1);
        assert_eq!(editor.add_page(added_asset, 7).expect("add page"), 2);
        editor
            .replace_page_asset(0, added_asset)
            .expect("replace page asset");
        editor.set_page_flags(1, 8).expect("edit page flags");
        editor
            .replace_asset_bytes(0, b"replaced", MediaType::Webp.as_u8(), 10)
            .expect("replace asset");
        assert!(editor.add_meta("edited", "after", None));
        assert!(editor.add_section("appendix", 2, None));

        let output = editor.build_bytes().expect("edited BBF");
        let reader = bbf_io::Reader::from_bytes(output);
        let indexed = reader.indexed().expect("read edited BBF");
        assert_eq!(indexed.footer().asset_count, 2);
        assert_eq!(indexed.footer().page_count, 3);
        assert_eq!(indexed.footer().section_count, 2);
        assert_eq!(indexed.footer().metadata_count, 2);
        assert_eq!(indexed.page(0).unwrap().unwrap().asset_index, 1);
        assert_eq!(indexed.page(1).unwrap().unwrap().asset_index, 0);
        assert_eq!(indexed.page(1).unwrap().unwrap().flags, 8);
        assert_eq!(indexed.page(2).unwrap().unwrap().asset_index, 1);
        assert_eq!(
            indexed.asset(0).unwrap().unwrap().media_type,
            MediaType::Webp.as_u8()
        );
        assert_eq!(indexed.asset(0).unwrap().unwrap().flags, 10);
        assert_eq!(
            indexed.asset_data(&indexed.asset(0).unwrap().unwrap()),
            Some(&b"replaced"[..])
        );
        assert_eq!(
            indexed.asset_data(&indexed.asset(1).unwrap().unwrap()),
            Some(&b"added"[..])
        );
        let metadata = indexed.metadata(0).unwrap().unwrap();
        assert_eq!(indexed.string(metadata.key_offset).unwrap(), Some("title"));
        assert_eq!(
            indexed.string(metadata.value_offset).unwrap(),
            Some("before")
        );
        let section = indexed.section(0).unwrap().unwrap();
        assert_eq!(
            indexed.string(section.title_offset).unwrap(),
            Some("chapter")
        );
        let edited_metadata = indexed.metadata(1).unwrap().unwrap();
        assert_eq!(
            indexed.string(edited_metadata.key_offset).unwrap(),
            Some("edited")
        );
        assert_eq!(
            indexed.string(edited_metadata.value_offset).unwrap(),
            Some("after")
        );
        let added_section = indexed.section(1).unwrap().unwrap();
        assert_eq!(
            indexed.string(added_section.title_offset).unwrap(),
            Some("appendix")
        );
    }

    #[test]
    fn editor_replaces_and_removes_metadata_and_sections() {
        let mut source = Builder::new();
        source.add_page_bytes(b"payload", MediaType::Unknown.as_u8(), 0, 0);
        source.add_meta("old-key", "old-value", Some("old-parent"));
        assert!(source.add_section("old-section", 0, Some("old-parent")));

        let mut editor = ArchiveEditor::from_bytes(source.build_bytes().unwrap()).unwrap();
        editor
            .replace_metadata(0, "new-key", "new-value", Some("new-parent"))
            .expect("replace metadata");
        editor
            .replace_section(0, "new-section", 1, Some("new-parent"))
            .expect("replace section");
        assert!(editor.add_meta("discarded", "metadata", None));
        assert!(editor.add_section("discarded-section", 1, None));
        editor.remove_metadata(1).expect("remove metadata");
        editor.remove_section(1).expect("remove section");

        let reader = bbf_io::Reader::from_bytes(editor.build_bytes().unwrap());
        let indexed = reader.indexed().unwrap();
        assert_eq!(indexed.footer().metadata_count, 1);
        assert_eq!(indexed.footer().section_count, 1);
        let metadata = indexed.metadata(0).unwrap().unwrap();
        assert_eq!(
            indexed.string(metadata.key_offset).unwrap(),
            Some("new-key")
        );
        assert_eq!(
            indexed.string(metadata.value_offset).unwrap(),
            Some("new-value")
        );
        assert_eq!(
            indexed.string(metadata.parent_offset).unwrap(),
            Some("new-parent")
        );
        let section = indexed.section(0).unwrap().unwrap();
        assert_eq!(section.start_index, 1);
        assert_eq!(
            indexed.string(section.title_offset).unwrap(),
            Some("new-section")
        );
        assert!(editor.remove_metadata(1).is_err());
        assert!(editor.remove_section(1).is_err());
    }

    #[test]
    fn file_appender_preserves_payloads_and_publishes_a_new_index() {
        let stem = format!("libbbf-rs-file-appender-{}", process::id());
        let archive_path = std::env::temp_dir().join(format!("{stem}.bbf"));
        let added_path = std::env::temp_dir().join(format!("{stem}.bin"));

        let mut source = Builder::new();
        source.add_page_bytes(b"original payload", MediaType::Png.as_u8(), 1, 2);
        assert!(source.add_meta("title", "before", None));
        assert!(source.add_section("chapter", 0, None));
        let original = source.build_bytes().expect("source BBF");
        fs::write(&archive_path, &original).expect("write source archive");
        fs::write(&added_path, b"appended payload").expect("write added asset");

        let original_reader = bbf_io::Reader::open(&archive_path).expect("read source archive");
        let original_indexed = original_reader.indexed().expect("source index");
        let original_asset = original_indexed.asset(0).unwrap().unwrap();
        let original_offset = original_asset.file_offset;
        let original_size = fs::metadata(&archive_path).expect("source metadata").len();

        let mut appender = FileAppender::open(&archive_path).expect("open appender");
        assert_eq!(appender.page_count(), 1);
        assert_eq!(appender.section_count(), 1);
        assert_eq!(appender.string(0), Some("title"));
        assert_eq!(appender.string_bytes(0), Some(&b"title"[..]));
        let added_asset = appender
            .add_asset_file(&added_path, MediaType::Jpg.as_u8(), 9)
            .expect("append asset");
        assert_eq!(added_asset, 1);
        assert_eq!(
            appender
                .add_page_asset(added_asset, 7)
                .expect("append page"),
            1
        );
        appender
            .replace_page(0, added_asset, 8)
            .expect("replace page");
        appender
            .set_meta_bytes(b"title", b"\xffafter", None)
            .expect("replace metadata");
        appender
            .add_section_bytes(b"\xffappendix", 1, None)
            .expect("append section");
        appender.finalize().expect("finalize append");

        let final_size = fs::metadata(&archive_path).expect("final metadata").len();
        assert!(final_size > original_size);
        let final_bytes = fs::read(&archive_path).expect("read final archive");
        let original_end = original_offset as usize + original_asset.file_size as usize;
        assert_eq!(
            &final_bytes[original_offset as usize..original_end],
            b"original payload"
        );

        let reader = bbf_io::Reader::from_bytes(final_bytes);
        let indexed = reader.indexed().expect("read appended archive");
        assert_eq!(indexed.footer().asset_count, 2);
        assert_eq!(indexed.footer().page_count, 2);
        assert_eq!(indexed.footer().section_count, 2);
        assert_eq!(indexed.page(0).unwrap().unwrap().asset_index, 1);
        assert_eq!(indexed.page(1).unwrap().unwrap().asset_index, 1);
        let metadata = indexed.metadata(0).unwrap().unwrap();
        assert_eq!(indexed.string(metadata.key_offset).unwrap(), Some("title"));
        assert_eq!(indexed.string(metadata.value_offset).unwrap(), None);
        assert_eq!(
            indexed.string_bytes(metadata.value_offset).unwrap(),
            Some(&b"\xffafter"[..])
        );
        let appended_section = indexed.section(1).unwrap().unwrap();
        assert_eq!(indexed.string(appended_section.title_offset).unwrap(), None);
        assert_eq!(
            indexed.string_bytes(appended_section.title_offset).unwrap(),
            Some(&b"\xffappendix"[..])
        );
        let added = indexed.asset(1).unwrap().unwrap();
        assert_eq!(indexed.asset_data(&added), Some(&b"appended payload"[..]));

        let _ = fs::remove_file(archive_path);
        let _ = fs::remove_file(added_path);
    }

    #[test]
    fn file_appender_removes_records_without_copying_payloads() {
        let stem = format!("libbbf-rs-file-appender-remove-{}", process::id());
        let archive_path = std::env::temp_dir().join(format!("{stem}.bbf"));

        let mut source = Builder::new();
        source.add_page_bytes(b"first", MediaType::Png.as_u8(), 0, 0);
        source.add_page_bytes(b"second", MediaType::Jpg.as_u8(), 0, 0);
        assert!(source.add_meta("keep", "value", None));
        assert!(source.add_meta("remove", "value", None));
        assert!(source.add_section("first", 0, None));
        assert!(source.add_section("second", 1, None));
        fs::write(&archive_path, source.build_bytes().expect("source BBF"))
            .expect("write source archive");

        let mut appender = FileAppender::open(&archive_path).expect("open appender");
        let removed_page = appender.remove_page(0).expect("remove page");
        assert_eq!(removed_page.asset_index, 0);
        appender.remove_metadata(1).expect("remove metadata");
        appender.remove_section(1).expect("remove section");
        appender.finalize().expect("finalize append");

        let reader = bbf_io::Reader::open(&archive_path).expect("read archive");
        let indexed = reader.indexed().expect("read index");
        assert_eq!(indexed.footer().page_count, 1);
        assert_eq!(indexed.footer().metadata_count, 1);
        assert_eq!(indexed.footer().section_count, 1);
        assert_eq!(indexed.page(0).unwrap().unwrap().asset_index, 1);
        assert_eq!(indexed.section(0).unwrap().unwrap().start_index, 0);
        assert_eq!(
            indexed.asset_data(&indexed.asset(1).unwrap().unwrap()),
            Some(&b"second"[..])
        );

        let _ = fs::remove_file(archive_path);
    }

    #[test]
    fn file_appender_checks_index_limit_before_allocating() {
        let archive_path = std::env::temp_dir().join(format!(
            "libbbf-rs-file-appender-index-limit-{}.bbf",
            process::id()
        ));
        let mut source = Builder::new();
        source.add_page_bytes(b"payload", MediaType::Unknown.as_u8(), 0, 0);
        fs::write(&archive_path, source.build_bytes().expect("source BBF"))
            .expect("write source archive");

        assert!(matches!(
            FileAppender::open_with_index_limit(&archive_path, 1),
            Err(super::AppendError::IndexTooLarge { .. })
        ));
        let _ = fs::remove_file(archive_path);
    }

    #[test]
    fn dropping_file_appender_keeps_the_previous_archive_view() {
        let stem = format!("libbbf-rs-file-appender-drop-{}", process::id());
        let archive_path = std::env::temp_dir().join(format!("{stem}.bbf"));
        let added_path = std::env::temp_dir().join(format!("{stem}.bin"));

        let mut source = Builder::new();
        source.add_page_bytes(b"stable payload", MediaType::Unknown.as_u8(), 0, 0);
        fs::write(&archive_path, source.build_bytes().expect("source BBF")).expect("archive");
        fs::write(&added_path, vec![b'x'; 128 * 1024]).expect("asset");

        {
            let mut appender = FileAppender::open(&archive_path).expect("open appender");
            appender
                .add_asset_file(&added_path, MediaType::Unknown.as_u8(), 0)
                .expect("append asset");
        }

        let reader = bbf_io::Reader::open(&archive_path).expect("read old archive");
        let indexed = reader.indexed().expect("old index");
        assert_eq!(indexed.footer().asset_count, 1);
        let asset = indexed.asset(0).unwrap().unwrap();
        assert_eq!(indexed.asset_data(&asset), Some(&b"stable payload"[..]));

        let _ = fs::remove_file(archive_path);
        let _ = fs::remove_file(added_path);
    }

    #[test]
    fn file_appender_rewrites_a_petrified_archive_to_normal_layout() {
        let stem = format!("libbbf-rs-file-appender-petrified-{}", process::id());
        let input_path = std::env::temp_dir().join(format!("{stem}-input.bbf"));
        let archive_path = std::env::temp_dir().join(format!("{stem}-archive.bbf"));
        let added_path = std::env::temp_dir().join(format!("{stem}.bin"));

        let mut source = Builder::new();
        source.add_page_bytes(b"original", MediaType::Unknown.as_u8(), 0, 0);
        fs::write(&input_path, source.build_bytes().expect("source BBF")).expect("input");
        Builder::petrify_file(&input_path, &archive_path).expect("petrify");
        fs::write(&added_path, b"added").expect("added asset");

        let mut appender = FileAppender::open(&archive_path).expect("open petrified appender");
        let asset = appender
            .add_asset_file(&added_path, MediaType::Png.as_u8(), 0)
            .expect("append asset");
        appender.add_page_asset(asset, 0).expect("append page");
        appender.finalize().expect("finalize append");

        let reader = bbf_io::Reader::open(&archive_path).expect("read appended archive");
        let header = reader.header().expect("header");
        let indexed = reader.indexed().expect("index");
        assert_eq!(header.flags & PETRIFICATION_FLAG, 0);
        assert_eq!(indexed.footer().asset_count, 2);
        assert_eq!(indexed.page(0).unwrap().unwrap().asset_index, 0);
        assert_eq!(indexed.page(1).unwrap().unwrap().asset_index, 1);

        let _ = fs::remove_file(input_path);
        let _ = fs::remove_file(archive_path);
        let _ = fs::remove_file(added_path);
    }

    #[test]
    fn editor_rejects_invalid_page_asset_edits() {
        let mut source = Builder::new();
        source.add_page_bytes(b"payload", MediaType::Unknown.as_u8(), 0, 0);
        let mut editor = ArchiveEditor::from_bytes(source.build_bytes().unwrap()).unwrap();

        assert!(editor.add_page(9, 0).is_err());
        assert!(editor.replace_page_asset(9, 0).is_err());
        assert!(editor.set_asset_flags(9, 0).is_err());
    }

    #[test]
    fn editor_removes_pages_and_rebases_section_boundaries() {
        let mut source = Builder::new();
        source.add_page_bytes(b"first", MediaType::Unknown.as_u8(), 0, 0);
        source.add_page_bytes(b"second", MediaType::Unknown.as_u8(), 0, 0);
        assert!(source.add_section("second", 1, None));

        let mut editor = ArchiveEditor::from_bytes(source.build_bytes().unwrap()).unwrap();
        let removed = editor.remove_page(0).expect("remove page");
        assert_eq!(removed.asset_index, 0);
        assert_eq!(editor.page_count(), 1);
        assert_eq!(editor.sections()[0].start_index, 0);
    }

    #[test]
    fn editor_imports_petrified_archives_and_reencodes_normal_layout() {
        let mut source = Builder::new();
        source.add_page_bytes(b"petrified payload", MediaType::Png.as_u8(), 4, 5);
        let normal = source.build_bytes().expect("source BBF");
        let petrified = Builder::petrify_bytes(&normal).expect("petrified BBF");

        let mut editor = ArchiveEditor::from_bytes(petrified).expect("open petrified BBF");
        let output = editor.build_bytes().expect("reencoded BBF");
        let reader = bbf_io::Reader::from_bytes(output);
        let header = reader.header().expect("edited header");
        let indexed = reader.indexed().expect("edited archive");
        let asset = indexed.asset(0).unwrap().unwrap();

        assert_eq!(header.flags & PETRIFICATION_FLAG, 0);
        assert_eq!(indexed.page(0).unwrap().unwrap().flags, 4);
        assert_eq!(asset.flags, 5);
        assert_eq!(indexed.asset_data(&asset), Some(&b"petrified payload"[..]));
    }

    #[test]
    fn editor_preserves_and_edits_expansion_records() {
        let original = Expansion {
            reserved: [10; 10],
            flags: 1,
        };
        let replacement = Expansion {
            reserved: [20; 10],
            flags: 2,
        };
        let mut source = Builder::new();
        source.add_page_bytes(b"payload", MediaType::Unknown.as_u8(), 0, 0);
        source.add_expansion(original);
        let source_bytes = source.build_bytes().expect("source BBF");

        let mut untouched = ArchiveEditor::from_bytes(source_bytes.clone()).expect("open BBF");
        assert_eq!(untouched.expansions(), &[original]);
        assert_eq!(
            untouched.build_bytes().expect("unchanged BBF"),
            source_bytes
        );

        let mut editor = ArchiveEditor::from_bytes(source_bytes).expect("open BBF");
        editor
            .replace_expansion(0, replacement)
            .expect("replace expansion");
        editor.add_expansion(Expansion::default());
        let reader = bbf_io::Reader::from_bytes(editor.build_bytes().expect("edited BBF"));
        assert_eq!(reader.footer().unwrap().expansion_count, 2);
        assert_eq!(reader.expansion(0).unwrap(), Some(replacement));
        assert_eq!(reader.expansion(1).unwrap(), Some(Expansion::default()));
        assert!(editor.replace_expansion(2, replacement).is_err());
    }

    #[test]
    fn streaming_file_serialization_matches_in_memory_serialization() {
        let mut builder = Builder::new();
        builder.add_page_bytes(b"streamed", MediaType::Png.as_u8(), 1, 2);
        builder.add_meta("title", "streaming", None);

        let expected = builder.clone().build_bytes().expect("in-memory BBF");
        let path =
            std::env::temp_dir().join(format!("libbbf-rs-streaming-test-{}.bbf", process::id()));
        builder.write_to(&path).expect("streamed BBF");
        let actual = fs::read(&path).expect("streamed BBF bytes");
        fs::remove_file(path).expect("remove streamed BBF");

        assert_eq!(actual, expected);
    }

    #[test]
    fn file_builder_streams_assets_and_matches_in_memory_builder() {
        let stem = format!("libbbf-rs-file-builder-{}", process::id());
        let first_path = std::env::temp_dir().join(format!("{stem}.png"));
        let second_path = std::env::temp_dir().join(format!("{stem}.jpg"));
        let output_path = std::env::temp_dir().join(format!("{stem}.bbf"));
        fs::write(&first_path, b"first streamed payload").expect("write first fixture");
        fs::write(&second_path, b"second streamed payload").expect("write second fixture");

        let mut expected_builder = Builder::new();
        let first_asset = expected_builder
            .append_asset_file(&first_path, MediaType::Png.as_u8(), 2)
            .expect("read first fixture");
        let second_asset = expected_builder
            .append_asset_file(&second_path, MediaType::Jpg.as_u8(), 4)
            .expect("read second fixture");
        expected_builder
            .append_page(first_asset, 1)
            .expect("append first page");
        expected_builder
            .append_page(second_asset, 3)
            .expect("append second page");
        expected_builder
            .append_page(first_asset, 5)
            .expect("append duplicate page");
        expected_builder.add_meta("Title", "Streamed", None);
        expected_builder.add_section("Chapter", 1, None);
        expected_builder.add_expansion(Expansion {
            reserved: [3; 10],
            flags: 7,
        });
        let expected = expected_builder.build_bytes().expect("build expected BBF");

        let mut file_builder = FileBuilder::new(&output_path).expect("create file builder");
        let first_asset = file_builder
            .append_asset_file(&first_path, MediaType::Png.as_u8(), 2)
            .expect("stream first fixture");
        let second_asset = file_builder
            .append_asset_file(&second_path, MediaType::Jpg.as_u8(), 4)
            .expect("stream second fixture");
        file_builder
            .append_page(first_asset, 1)
            .expect("append first page");
        file_builder
            .append_page(second_asset, 3)
            .expect("append second page");
        file_builder
            .append_page(first_asset, 5)
            .expect("append duplicate page");
        file_builder.add_meta("Title", "Streamed", None);
        assert!(file_builder.add_section("Chapter", 1, None));
        file_builder.add_expansion(Expansion {
            reserved: [3; 10],
            flags: 7,
        });
        assert_eq!(file_builder.asset_count(), 2);
        assert_eq!(file_builder.page_count(), 3);
        assert_eq!(file_builder.expansion_count(), 1);
        file_builder.finalize().expect("finalize file builder");

        let actual = fs::read(&output_path).expect("read streamed output");
        fs::remove_file(first_path).expect("remove first fixture");
        fs::remove_file(second_path).expect("remove second fixture");
        fs::remove_file(output_path).expect("remove streamed output");

        assert_eq!(actual, expected);
    }

    #[test]
    fn file_builder_rejects_source_changes_and_enters_failed_state() {
        let output_path = std::env::temp_dir().join(format!(
            "libbbf-rs-file-builder-source-change-{}.bbf",
            process::id()
        ));
        let mut file_builder = FileBuilder::new(&output_path).expect("create file builder");
        let mut source = ChangingReader {
            first: b"original payload".to_vec(),
            second: b"changed payload!".to_vec(),
            position: 0,
            changed: false,
        };
        let source_size = source.first.len() as u64;

        assert!(matches!(
            file_builder.add_page_from_reader(
                &mut source,
                source_size,
                MediaType::Unknown.as_u8(),
                0,
                0,
            ),
            Err(BuilderError::InvalidInput("asset changed while being read"))
        ));
        assert!(file_builder.is_failed());
        assert!(matches!(
            file_builder.add_page(&output_path, 0, 0),
            Err(BuilderError::FileBuilderFailed)
        ));
        assert!(!file_builder.add_meta("ignored", "after failure", None));
        assert!(matches!(
            file_builder.finalize(),
            Err(BuilderError::FileBuilderFailed)
        ));

        fs::remove_file(output_path).expect("remove failed builder output");
    }

    #[test]
    fn copy_reader_handles_short_writes_without_changing_the_hash() {
        let payload = b"short writes must still produce the exact payload";
        let mut input = &payload[..];
        let mut output = LimitedWriter {
            bytes: Vec::new(),
            max_write: 3,
            fail_after: None,
        };

        let hash = copy_reader(&mut input, &mut output, payload.len() as u64)
            .expect("copy with short writes");

        assert_eq!(output.bytes, payload);
        assert_eq!(hash, xxhash_rust::xxh3::xxh3_128(payload));
    }

    #[test]
    fn copy_reader_propagates_injected_write_failures() {
        let payload = b"write failure payload";
        let mut input = &payload[..];
        let mut output = LimitedWriter {
            bytes: Vec::new(),
            max_write: 4,
            fail_after: Some(7),
        };

        assert!(matches!(
            copy_reader(&mut input, &mut output, payload.len() as u64),
            Err(BuilderError::Io(error)) if error.kind() == io::ErrorKind::Other
        ));
        assert_eq!(&output.bytes, b"write f");
    }

    #[test]
    fn file_builder_preserves_reference_petrification_flag_without_reordering() {
        let stem = format!("libbbf-rs-file-builder-petrified-{}", process::id());
        let page_path = std::env::temp_dir().join(format!("{stem}.dat"));
        let output_path = std::env::temp_dir().join(format!("{stem}.bbf"));
        fs::write(&page_path, b"flagged streamed payload").expect("write flagged page");

        let config = BuilderConfig {
            header_flags: PETRIFICATION_FLAG,
            ..BuilderConfig::default()
        };
        let mut file_builder =
            FileBuilder::with_config(&output_path, config).expect("create flagged file builder");
        file_builder
            .add_page(&page_path, 0, 0)
            .expect("stream flagged page");
        file_builder
            .finalize()
            .expect("finalize flagged file builder");

        let bytes = fs::read(&output_path).expect("read flagged BBF");
        let reader = bbf_io::Reader::from_bytes(bytes.clone());
        let header = reader.header().expect("read flagged file header");
        assert_ne!(header.flags & PETRIFICATION_FLAG, 0);
        assert_eq!(header.footer_offset as usize, bytes.len() - FOOTER_SIZE);
        assert_ne!(header.footer_offset, HEADER_SIZE as u64);

        fs::remove_file(page_path).expect("remove flagged page");
        fs::remove_file(output_path).expect("remove flagged BBF");
    }

    #[test]
    fn hashes_distinct_payloads_into_distinct_assets() {
        let mut builder = Builder::new();
        builder.add_page_bytes(b"one", MediaType::Unknown.as_u8(), 0, 0);
        builder.add_page_bytes(b"two", MediaType::Unknown.as_u8(), 0, 0);

        assert_eq!(builder.asset_count(), 2);
        assert_ne!(builder.assets()[0].hash_low, builder.assets()[1].hash_low);
        assert_eq!(builder.assets()[0].file_size, 3);
        assert_eq!(builder.assets()[1].data, b"two");
    }

    #[test]
    fn extension_detection_matches_reference_media_types() {
        assert_eq!(detect_media_type("cover.PNG"), MediaType::Png.as_u8());
        assert_eq!(detect_media_type("cover.jpeg"), MediaType::Jpg.as_u8());
        assert_eq!(detect_media_type("cover.jpegx"), MediaType::Jpg.as_u8());
        assert_eq!(detect_media_type("cover.avifx"), MediaType::Avif.as_u8());
        assert_eq!(detect_media_type("cover.JXL"), MediaType::Jxl.as_u8());
        assert_eq!(detect_media_type("cover.txt"), MediaType::Unknown.as_u8());
        assert_eq!(
            detect_media_type("no-extension"),
            MediaType::Unknown.as_u8()
        );
    }

    #[test]
    fn missing_page_paths_are_reported_without_mutating_builder_state() {
        let mut builder = Builder::new();
        let path =
            std::env::temp_dir().join(format!("libbbf-rs-missing-page-{}", std::process::id()));

        let error = builder
            .add_page(path, 0, 0)
            .expect_err("path must not exist");
        assert!(matches!(error, BuilderError::Io(_)));
        assert_eq!(builder.asset_count(), 0);
        assert_eq!(builder.page_count(), 0);
    }

    #[test]
    fn write_to_preserves_existing_destination_when_validation_fails() {
        let path = std::env::temp_dir().join(format!(
            "libbbf-rs-write-validation-{}.bbf",
            std::process::id()
        ));
        fs::write(&path, b"existing archive").expect("write existing destination");

        let mut builder = Builder::new();
        assert!(matches!(
            builder.write_to(&path),
            Err(BuilderError::NoAssets)
        ));
        assert_eq!(
            fs::read(&path).expect("read existing destination"),
            b"existing archive"
        );

        fs::remove_file(path).expect("remove destination fixture");
    }

    #[test]
    fn write_to_preserves_destination_when_final_rename_fails() {
        let directory =
            std::env::temp_dir().join(format!("libbbf-rs-write-rename-{}", std::process::id()));
        fs::create_dir(&directory).expect("create destination directory");
        let sentinel = directory.join("sentinel");
        fs::write(&sentinel, b"preserve me").expect("write directory sentinel");

        let mut builder = Builder::new();
        builder.add_page_bytes(b"payload", MediaType::Unknown.as_u8(), 0, 0);
        assert!(builder.write_to(&directory).is_err());
        assert_eq!(
            fs::read(&sentinel).expect("read directory sentinel"),
            b"preserve me"
        );

        fs::remove_file(sentinel).expect("remove directory sentinel");
        fs::remove_dir(directory).expect("remove destination directory");
    }

    #[test]
    fn file_backed_pages_preserve_payload_and_media_type() {
        let path = std::env::temp_dir().join(format!("libbbf-rs-owned-page-{}.png", process::id()));
        fs::write(&path, b"owned payload").expect("write page fixture");

        let mut builder = Builder::new();
        builder.add_page(&path, 7, 9).expect("read page fixture");
        fs::remove_file(path).expect("remove page fixture");

        assert_eq!(builder.asset_count(), 1);
        assert_eq!(builder.page_count(), 1);
        assert_eq!(builder.assets()[0].data, b"owned payload");
        assert_eq!(builder.assets()[0].media_type, MediaType::Png.as_u8());
        assert_eq!(builder.assets()[0].flags, 9);
        assert_eq!(builder.pages()[0].flags, 7);
    }

    #[test]
    fn default_builder_config_matches_reference_defaults() {
        assert_eq!(Builder::new().config(), BuilderConfig::default());
        assert_eq!(
            BuilderConfig::default().header_flags,
            VARIABLE_REAM_SIZE_FLAG
        );
        assert_eq!(BuilderConfig::default().alignment, 12);
        assert_eq!(BuilderConfig::default().ream_size, 16);
    }

    #[test]
    fn serializes_default_layout_with_variable_ream_alignment() {
        let mut builder = Builder::new();
        builder.add_page_bytes(b"abc", MediaType::Png.as_u8(), 0, 0);
        builder.add_page_bytes(b"12345", MediaType::Jpg.as_u8(), 0, 0);
        let bytes = builder.build_bytes().expect("assets can be finalized");
        let reader = bbf_io::Reader::from_bytes(bytes.clone());

        let header = reader.header().unwrap();
        let footer = reader.footer().unwrap();
        let first = reader.asset(0).unwrap().unwrap();
        let second = reader.asset(1).unwrap().unwrap();

        assert_eq!(header.version, 3);
        assert_eq!(
            header.footer_offset as usize,
            bytes.len() - bbf_format::FOOTER_SIZE
        );
        assert_eq!(footer.asset_count, 2);
        assert_eq!(footer.page_count, 2);
        assert_eq!(first.file_offset, 64);
        assert_eq!(second.file_offset, 72);
        assert_eq!(reader.asset_data(&first), Some(&b"abc"[..]));
        assert_eq!(reader.asset_data(&second), Some(&b"12345"[..]));
        assert_eq!(reader.page(1).unwrap().unwrap().asset_index, 1);

        let index_start = footer.asset_offset as usize;
        let index_end = footer.string_pool_offset as usize + footer.string_pool_size as usize;
        assert_eq!(
            footer.footer_hash,
            xxhash_rust::xxh3::xxh3_64(&bytes[index_start..index_end])
        );
    }

    #[test]
    fn serializes_and_reads_metadata_and_nested_sections() {
        let mut builder = Builder::new();
        builder.add_page_bytes(b"page", MediaType::Png.as_u8(), 0, 0);
        assert!(builder.add_meta("Title", "Example", None));
        assert!(builder.add_meta("Chapter", "One", Some("Volume")));
        assert!(builder.add_section("Volume", 0, None));
        assert!(builder.add_section("Chapter 1", 0, Some("Volume")));
        assert!(!builder.add_section("Invalid", 2, None));

        let bytes = builder.build_bytes().expect("metadata can be finalized");
        let reader = bbf_io::Reader::from_bytes(bytes);
        let footer = reader.footer().unwrap();

        assert_eq!(footer.section_count, 2);
        assert_eq!(footer.metadata_count, 2);

        let first_section = reader.section(0).unwrap().unwrap();
        let second_section = reader.section(1).unwrap().unwrap();
        assert_eq!(
            reader.string(first_section.title_offset).unwrap(),
            Some("Volume")
        );
        assert_eq!(first_section.parent_offset, NO_PARENT_OFFSET);
        assert_eq!(
            reader.string(second_section.title_offset).unwrap(),
            Some("Chapter 1")
        );
        assert_eq!(
            reader.string(second_section.parent_offset).unwrap(),
            Some("Volume")
        );

        let first_metadata = reader.metadata(0).unwrap().unwrap();
        let second_metadata = reader.metadata(1).unwrap().unwrap();
        assert_eq!(
            reader.string(first_metadata.key_offset).unwrap(),
            Some("Title")
        );
        assert_eq!(
            reader.string(first_metadata.value_offset).unwrap(),
            Some("Example")
        );
        assert_eq!(first_metadata.parent_offset, NO_PARENT_OFFSET);
        assert_eq!(
            reader.string(second_metadata.key_offset).unwrap(),
            Some("Chapter")
        );
        assert_eq!(
            reader.string(second_metadata.value_offset).unwrap(),
            Some("One")
        );
        assert_eq!(
            reader.string(second_metadata.parent_offset).unwrap(),
            Some("Volume")
        );
        assert_eq!(second_metadata.key_offset, 14);
        assert_eq!(second_metadata.value_offset, 22);
        assert_eq!(second_metadata.parent_offset, 26);
    }

    #[test]
    fn petrifies_default_layout_and_rebases_asset_offsets() {
        let mut builder = Builder::new();
        builder.add_page_bytes(b"first", MediaType::Png.as_u8(), 0, 0);
        builder.add_page_bytes(b"second", MediaType::Jpg.as_u8(), 0, 0);
        builder.add_meta("Title", "Petrified", None);
        builder.add_section("Chapter", 0, None);
        let original = builder.build_bytes().expect("default file can be built");
        let original_reader = bbf_io::Reader::from_bytes(original);
        let petrified = Builder::petrify_bytes(&builder.build_bytes().unwrap())
            .expect("default file can be petrified");
        let reader = bbf_io::Reader::from_bytes(petrified.clone());

        let header = reader.header().unwrap();
        let footer = reader.footer().unwrap();
        assert_ne!(header.flags & PETRIFICATION_FLAG, 0);
        assert_eq!(header.footer_offset, HEADER_SIZE as u64);
        assert_eq!(footer.asset_offset, (HEADER_SIZE + FOOTER_SIZE) as u64);
        assert_eq!(footer.section_count, 1);
        assert_eq!(reader.string(0).unwrap(), Some("Title"));
        assert_eq!(
            reader.asset_data(&reader.asset(0).unwrap().unwrap()),
            Some(&b"first"[..])
        );
        assert_eq!(
            reader.asset_data(&reader.asset(1).unwrap().unwrap()),
            Some(&b"second"[..])
        );
        assert_eq!(reader.page(1).unwrap().unwrap().asset_index, 1);
        assert_eq!(reader.len(), petrified.len());
        assert_eq!(
            original_reader.asset_data(&original_reader.asset(0).unwrap().unwrap()),
            Some(&b"first"[..])
        );
        assert!(matches!(
            Builder::petrify_bytes(&petrified),
            Err(BuilderError::InvalidInput("file is already petrified"))
        ));
    }

    #[test]
    fn streaming_petrification_matches_in_memory_transform() {
        let _lock = petrify_test_lock().lock().expect("petrify test lock");
        let mut builder = Builder::new();
        builder.add_page_bytes(b"streamed asset", MediaType::Png.as_u8(), 0, 0);
        builder.add_meta("Title", "Streaming", None);
        let input = builder.build_bytes().expect("default file can be built");
        let expected = Builder::petrify_bytes(&input).expect("in-memory petrification");

        let test_dir = std::env::current_dir().expect("repository test directory");
        let input_path = test_dir.join(format!(".libbbf-rs-petrify-input-{}.bbf", process::id()));
        let output_path = test_dir.join(format!(".libbbf-rs-petrify-output-{}.bbf", process::id()));
        fs::write(&input_path, input).expect("write petrify input");
        Builder::petrify_file(&input_path, &output_path).expect("streaming petrification");
        let actual = fs::read(&output_path).expect("read petrify output");
        assert!(!test_dir.join(PETRIFY_TEMP_PATH).exists());
        fs::remove_file(input_path).expect("remove petrify input");
        fs::remove_file(output_path).expect("remove petrify output");

        assert_eq!(actual, expected);
    }

    #[test]
    fn safe_petrification_uses_destination_local_staging() {
        let _lock = petrify_test_lock().lock().expect("petrify test lock");
        let original_directory = std::env::current_dir().expect("repository test directory");
        let directory =
            std::env::temp_dir().join(format!("libbbf-rs-safe-petrify-{}", process::id()));
        fs::create_dir(&directory).expect("create safe petrify directory");
        let _directory_guard = CurrentDirectoryGuard(original_directory);
        std::env::set_current_dir(&directory).expect("enter safe petrify directory");

        let input_path = directory.join("input.bbf");
        let output_path = directory.join("output.bbf");
        let mut builder = Builder::new();
        builder.add_page_bytes(b"safe staging", MediaType::Png.as_u8(), 0, 0);
        let input = builder.build_bytes().expect("build petrify input");
        let expected = Builder::petrify_bytes(&input).expect("petrify expected bytes");
        fs::write(&input_path, input).expect("write petrify input");
        fs::write(PETRIFY_TEMP_PATH, b"unrelated file").expect("write unrelated staging name");

        Builder::petrify_file(&input_path, &output_path).expect("safe petrification");
        assert_eq!(
            fs::read(&output_path).expect("read petrified output"),
            expected
        );
        assert_eq!(
            fs::read(PETRIFY_TEMP_PATH).expect("read unrelated staging name"),
            b"unrelated file"
        );

        drop(_directory_guard);
        fs::remove_dir_all(directory).expect("remove safe petrify directory");
    }

    #[test]
    fn safe_petrification_supports_concurrent_outputs() {
        let _lock = petrify_test_lock().lock().expect("petrify test lock");
        let original_directory = std::env::current_dir().expect("repository test directory");
        let directory =
            std::env::temp_dir().join(format!("libbbf-rs-concurrent-petrify-{}", process::id()));
        fs::create_dir(&directory).expect("create concurrent petrify directory");
        let _directory_guard = CurrentDirectoryGuard(original_directory);
        std::env::set_current_dir(&directory).expect("enter concurrent petrify directory");

        let input_path = directory.join("input.bbf");
        let first_output = directory.join("first.bbf");
        let second_output = directory.join("second.bbf");
        let mut builder = Builder::new();
        builder.add_page_bytes(b"concurrent staging", MediaType::Jpg.as_u8(), 0, 0);
        let input = builder.build_bytes().expect("build concurrent input");
        let expected = Builder::petrify_bytes(&input).expect("petrify concurrent expected bytes");
        fs::write(&input_path, input).expect("write concurrent input");

        std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                Builder::petrify_file(&input_path, &first_output).expect("first petrification")
            });
            let second = scope.spawn(|| {
                Builder::petrify_file(&input_path, &second_output).expect("second petrification")
            });
            first.join().expect("join first petrification");
            second.join().expect("join second petrification");
        });

        assert_eq!(
            fs::read(&first_output).expect("read first output"),
            expected
        );
        assert_eq!(
            fs::read(&second_output).expect("read second output"),
            expected
        );

        drop(_directory_guard);
        fs::remove_dir_all(directory).expect("remove concurrent petrify directory");
    }

    #[test]
    fn safe_petrification_cleans_unique_staging_when_rename_fails() {
        let _lock = petrify_test_lock().lock().expect("petrify test lock");
        let directory =
            std::env::temp_dir().join(format!("libbbf-rs-petrify-rename-{}", process::id()));
        fs::create_dir(&directory).expect("create rename directory");
        let input_path = directory.join("input.bbf");
        let occupied = directory.join("occupied");
        let mut builder = Builder::new();
        builder.add_page_bytes(b"rename failure", MediaType::Unknown.as_u8(), 0, 0);
        fs::write(
            &input_path,
            builder.build_bytes().expect("build rename input"),
        )
        .expect("write rename input");
        fs::create_dir(&occupied).expect("create occupied destination");
        let sentinel = occupied.join("sentinel");
        fs::write(&sentinel, b"preserve me").expect("write occupied sentinel");

        assert!(Builder::petrify_file(&input_path, &occupied).is_err());
        assert_eq!(
            fs::read(&sentinel).expect("read occupied sentinel"),
            b"preserve me"
        );
        assert!(
            !fs::read_dir(&directory)
                .expect("read rename directory")
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().contains(".libbbf-rs-"))
        );

        fs::remove_dir_all(directory).expect("remove rename directory");
    }

    #[test]
    fn same_path_petrification_does_not_truncate_input() {
        let _lock = petrify_test_lock().lock().expect("petrify test lock");
        let mut builder = Builder::new();
        builder.add_page_bytes(b"in-place asset", MediaType::Png.as_u8(), 0, 0);
        let input = builder.build_bytes().expect("default file can be built");
        let expected = Builder::petrify_bytes(&input).expect("in-memory petrification");
        let path = std::env::current_dir()
            .expect("repository test directory")
            .join(format!(".libbbf-rs-petrify-in-place-{}.bbf", process::id()));

        fs::write(&path, input).expect("write in-place input");
        Builder::petrify_file(&path, &path).expect("in-place petrification");
        let actual = fs::read(&path).expect("read in-place output");
        assert!(
            !std::env::current_dir()
                .expect("repository test directory")
                .join(PETRIFY_TEMP_PATH)
                .exists()
        );
        fs::remove_file(path).expect("remove in-place fixture");

        assert_eq!(actual, expected);
    }

    #[test]
    fn preserves_reference_builder_petrification_flag_without_reordering() {
        assert!(matches!(
            Builder::new().build_bytes(),
            Err(BuilderError::NoAssets)
        ));

        let config = BuilderConfig {
            header_flags: PETRIFICATION_FLAG,
            ..BuilderConfig::default()
        };
        let mut builder = Builder::with_config(config);
        builder.add_page_bytes(b"asset", MediaType::Unknown.as_u8(), 0, 0);
        let bytes = builder.build_bytes().expect("reference accepts the flag");
        let reader = bbf_io::Reader::from_bytes(bytes.clone());
        let header = reader.header().expect("read flagged header");
        assert_ne!(header.flags & PETRIFICATION_FLAG, 0);
        assert_eq!(header.footer_offset as usize, bytes.len() - FOOTER_SIZE);
        assert_ne!(header.footer_offset, HEADER_SIZE as u64);
    }

    #[test]
    fn rejects_alignment_exponents_above_the_spec_limit() {
        let config = BuilderConfig {
            alignment: 17,
            ..BuilderConfig::default()
        };
        let mut builder = Builder::with_config(config);
        builder.add_page_bytes(b"asset", MediaType::Unknown.as_u8(), 0, 0);
        assert!(matches!(
            builder.build_bytes(),
            Err(BuilderError::InvalidAlignment(17))
        ));
    }
}
