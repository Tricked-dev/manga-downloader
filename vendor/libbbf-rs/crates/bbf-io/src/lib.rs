//! Safe BBF reader views.
//!
//! The first reader slice is byte-backed so its bounds and parity behavior can
//! be tested independently of the later memory-mapping optimization.

use std::{fmt, fs, path::Path, str, sync::OnceLock};

use bbf_format::{
    ASSET_SIZE, Asset, DecodeError, EXPANSION_SIZE, Expansion, FOOTER_SIZE, Footer, Header,
    METADATA_SIZE, Metadata, PAGE_SIZE, Page, SECTION_SIZE, Section,
};

#[derive(Debug)]
pub enum ReaderError {
    Io(std::io::Error),
    Format(DecodeError),
    OutOfBounds {
        offset: u64,
        size: u64,
        file_size: u64,
    },
}

impl fmt::Display for ReaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "could not read BBF file: {error}"),
            Self::Format(error) => write!(formatter, "invalid BBF structure: {error}"),
            Self::OutOfBounds {
                offset,
                size,
                file_size,
            } => write!(
                formatter,
                "BBF range {offset}..{} exceeds file size {file_size}",
                offset.saturating_add(*size)
            ),
        }
    }
}

impl std::error::Error for ReaderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Format(error) => Some(error),
            Self::OutOfBounds { .. } => None,
        }
    }
}

impl From<DecodeError> for ReaderError {
    fn from(error: DecodeError) -> Self {
        Self::Format(error)
    }
}

impl From<std::io::Error> for ReaderError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// A read-only BBF byte view.
#[derive(Debug)]
pub struct Reader<D = Vec<u8>> {
    data: D,
    header: OnceLock<Header>,
    footer: OnceLock<Footer>,
}

impl<D: Clone> Clone for Reader<D> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            header: OnceLock::new(),
            footer: OnceLock::new(),
        }
    }
}

/// A reader view with the BBF footer decoded once and reused for table access.
///
/// This mirrors the reference reader's cached footer and avoids reparsing the
/// header/footer for every asset, page, or string lookup in a read operation.
pub struct IndexedReader<'a, D> {
    reader: &'a Reader<D>,
    footer: Footer,
}

impl Reader<Vec<u8>> {
    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self::from_data(data)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, ReaderError> {
        Ok(Self::from_bytes(fs::read(path)?))
    }
}

impl<D: AsRef<[u8]>> Reader<D> {
    pub fn from_data(data: D) -> Self {
        Self {
            data,
            header: OnceLock::new(),
            footer: OnceLock::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.data.as_ref().len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.as_ref().is_empty()
    }

    pub fn bytes(&self) -> &[u8] {
        self.data.as_ref()
    }

    pub fn header(&self) -> Result<Header, ReaderError> {
        if let Some(header) = self.header.get() {
            return Ok(*header);
        }
        let header = Header::decode(self.range(0, bbf_format::HEADER_SIZE as u64)?)?;
        let _ = self.header.set(header);
        Ok(header)
    }

    pub fn footer(&self) -> Result<Footer, ReaderError> {
        if let Some(footer) = self.footer.get() {
            return Ok(*footer);
        }
        let offset = self.header()?.footer_offset;
        let footer = Footer::decode(self.range(offset, FOOTER_SIZE as u64)?)?;
        let _ = self.footer.set(footer);
        Ok(footer)
    }

    pub fn indexed(&self) -> Result<IndexedReader<'_, D>, ReaderError> {
        Ok(IndexedReader {
            reader: self,
            footer: self.footer()?,
        })
    }

    pub fn asset(&self, index: u64) -> Result<Option<Asset>, ReaderError> {
        let footer = self.footer()?;
        self.table_entry(
            footer.asset_offset,
            footer.asset_count,
            index,
            ASSET_SIZE,
            Asset::decode,
        )
    }

    pub fn page(&self, index: u64) -> Result<Option<Page>, ReaderError> {
        let footer = self.footer()?;
        self.table_entry(
            footer.page_offset,
            footer.page_count,
            index,
            PAGE_SIZE,
            Page::decode,
        )
    }

    pub fn section(&self, index: u64) -> Result<Option<Section>, ReaderError> {
        let footer = self.footer()?;
        self.table_entry(
            footer.section_offset,
            footer.section_count,
            index,
            SECTION_SIZE,
            Section::decode,
        )
    }

    pub fn metadata(&self, index: u64) -> Result<Option<Metadata>, ReaderError> {
        let footer = self.footer()?;
        self.table_entry(
            footer.metadata_offset,
            footer.metadata_count,
            index,
            METADATA_SIZE,
            Metadata::decode,
        )
    }

    pub fn expansion(&self, index: u64) -> Result<Option<Expansion>, ReaderError> {
        let footer = self.footer()?;
        if footer.expansion_offset == 0 {
            return Ok(None);
        }
        self.table_entry(
            footer.expansion_offset,
            footer.expansion_count,
            index,
            EXPANSION_SIZE,
            Expansion::decode,
        )
    }

    /// Reads a string-pool-relative offset, matching the C++ writer.
    pub fn string(&self, offset: u64) -> Result<Option<&str>, ReaderError> {
        let footer = self.footer()?;
        Ok(self
            .string_bytes_with_footer(&footer, offset)?
            .and_then(|bytes| str::from_utf8(bytes).ok()))
    }

    /// Reads a raw NUL-terminated string-pool entry without requiring UTF-8.
    ///
    /// The reference C++ reader exposes `const char*` and preserves arbitrary
    /// non-NUL bytes. Keep this byte-oriented API as the parity primitive;
    /// [`Self::string`] remains the UTF-8 convenience view.
    pub fn string_bytes(&self, offset: u64) -> Result<Option<&[u8]>, ReaderError> {
        let footer = self.footer()?;
        self.string_bytes_with_footer(&footer, offset)
    }

    fn string_bytes_with_footer(
        &self,
        footer: &Footer,
        offset: u64,
    ) -> Result<Option<&[u8]>, ReaderError> {
        if offset >= footer.string_pool_size {
            return Ok(None);
        }

        let pool_end = footer
            .string_pool_offset
            .checked_add(footer.string_pool_size)
            .ok_or(ReaderError::OutOfBounds {
                offset: footer.string_pool_offset,
                size: footer.string_pool_size,
                file_size: self.len() as u64,
            })?;
        let string_start =
            footer
                .string_pool_offset
                .checked_add(offset)
                .ok_or(ReaderError::OutOfBounds {
                    offset: footer.string_pool_offset,
                    size: offset,
                    file_size: self.len() as u64,
                })?;
        let bytes = self.range(string_start, pool_end - string_start)?;
        let end = bytes.iter().position(|byte| *byte == 0);
        let Some(end) = end else {
            return Ok(None);
        };
        Ok(Some(&bytes[..end]))
    }

    pub fn asset_data(&self, asset: &Asset) -> Option<&[u8]> {
        self.range(asset.file_offset, asset.file_size).ok()
    }

    pub fn compute_asset_hash(&self, index: u64) -> Result<Option<(u64, u64)>, ReaderError> {
        let Some(asset) = self.asset(index)? else {
            return Ok(None);
        };
        Ok(self.compute_asset_hash_for(&asset))
    }

    pub fn verify_asset_hash(&self, index: u64) -> Result<Option<bool>, ReaderError> {
        let Some(asset) = self.asset(index)? else {
            return Ok(None);
        };
        let Some((low, high)) = self.compute_asset_hash_for(&asset) else {
            return Ok(Some(false));
        };
        Ok(Some(low == asset.hash_low && high == asset.hash_high))
    }

    fn compute_asset_hash_for(&self, asset: &Asset) -> Option<(u64, u64)> {
        let data = self.asset_data(asset)?;
        let hash = xxhash_rust::xxh3::xxh3_128(data);
        Some((hash as u64, (hash >> 64) as u64))
    }

    pub fn verify_footer_hash(&self) -> Result<bool, ReaderError> {
        let footer = self.footer()?;
        self.verify_footer_hash_with(&footer)
    }

    fn verify_footer_hash_with(&self, footer: &Footer) -> Result<bool, ReaderError> {
        let index_end = footer
            .string_pool_offset
            .checked_add(footer.string_pool_size)
            .ok_or(ReaderError::OutOfBounds {
                offset: footer.string_pool_offset,
                size: footer.string_pool_size,
                file_size: self.len() as u64,
            })?;
        if footer.asset_offset > index_end {
            return Ok(false);
        }
        let index = self.range(footer.asset_offset, index_end - footer.asset_offset)?;
        Ok(xxhash_rust::xxh3::xxh3_64(index) == footer.footer_hash)
    }

    fn table_entry<T>(
        &self,
        offset: u64,
        count: u64,
        index: u64,
        size: usize,
        decode: impl Fn(&[u8]) -> Result<T, DecodeError>,
    ) -> Result<Option<T>, ReaderError> {
        if index >= count {
            return Ok(None);
        }
        let byte_offset = offset
            .checked_add(
                index
                    .checked_mul(size as u64)
                    .ok_or(ReaderError::OutOfBounds {
                        offset,
                        size: size as u64,
                        file_size: self.len() as u64,
                    })?,
            )
            .ok_or(ReaderError::OutOfBounds {
                offset,
                size: size as u64,
                file_size: self.len() as u64,
            })?;
        Ok(Some(decode(self.range(byte_offset, size as u64)?)?))
    }

    fn range(&self, offset: u64, size: u64) -> Result<&[u8], ReaderError> {
        let end = offset.checked_add(size).ok_or(ReaderError::OutOfBounds {
            offset,
            size,
            file_size: self.len() as u64,
        })?;
        let file_size = self.len() as u64;
        if end > file_size {
            return Err(ReaderError::OutOfBounds {
                offset,
                size,
                file_size,
            });
        }
        let start = usize::try_from(offset).map_err(|_| ReaderError::OutOfBounds {
            offset,
            size,
            file_size,
        })?;
        let end = usize::try_from(end).map_err(|_| ReaderError::OutOfBounds {
            offset,
            size,
            file_size,
        })?;
        Ok(&self.bytes()[start..end])
    }
}

impl<'a, D: AsRef<[u8]>> IndexedReader<'a, D> {
    pub fn footer(&self) -> &Footer {
        &self.footer
    }

    pub fn asset(&self, index: u64) -> Result<Option<Asset>, ReaderError> {
        self.reader.table_entry(
            self.footer.asset_offset,
            self.footer.asset_count,
            index,
            ASSET_SIZE,
            Asset::decode,
        )
    }

    pub fn page(&self, index: u64) -> Result<Option<Page>, ReaderError> {
        self.reader.table_entry(
            self.footer.page_offset,
            self.footer.page_count,
            index,
            PAGE_SIZE,
            Page::decode,
        )
    }

    pub fn section(&self, index: u64) -> Result<Option<Section>, ReaderError> {
        self.reader.table_entry(
            self.footer.section_offset,
            self.footer.section_count,
            index,
            SECTION_SIZE,
            Section::decode,
        )
    }

    pub fn metadata(&self, index: u64) -> Result<Option<Metadata>, ReaderError> {
        self.reader.table_entry(
            self.footer.metadata_offset,
            self.footer.metadata_count,
            index,
            METADATA_SIZE,
            Metadata::decode,
        )
    }

    pub fn expansion(&self, index: u64) -> Result<Option<Expansion>, ReaderError> {
        if self.footer.expansion_offset == 0 {
            return Ok(None);
        }
        self.reader.table_entry(
            self.footer.expansion_offset,
            self.footer.expansion_count,
            index,
            EXPANSION_SIZE,
            Expansion::decode,
        )
    }

    pub fn string(&self, offset: u64) -> Result<Option<&'a str>, ReaderError> {
        Ok(self
            .string_bytes(offset)?
            .and_then(|bytes| str::from_utf8(bytes).ok()))
    }

    pub fn string_bytes(&self, offset: u64) -> Result<Option<&'a [u8]>, ReaderError> {
        self.reader.string_bytes_with_footer(&self.footer, offset)
    }

    pub fn asset_data(&self, asset: &Asset) -> Option<&'a [u8]> {
        self.reader.asset_data(asset)
    }

    pub fn compute_asset_hash(&self, index: u64) -> Result<Option<(u64, u64)>, ReaderError> {
        let Some(asset) = self.asset(index)? else {
            return Ok(None);
        };
        Ok(self.reader.compute_asset_hash_for(&asset))
    }

    pub fn verify_asset_hash(&self, index: u64) -> Result<Option<bool>, ReaderError> {
        let Some(asset) = self.asset(index)? else {
            return Ok(None);
        };
        let Some((low, high)) = self.reader.compute_asset_hash_for(&asset) else {
            return Ok(Some(false));
        };
        Ok(Some(low == asset.hash_low && high == asset.hash_high))
    }

    pub fn verify_footer_hash(&self) -> Result<bool, ReaderError> {
        self.reader.verify_footer_hash_with(&self.footer)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as StdError;

    use bbf_format::{Asset, Footer, Header, MediaType, Page};

    use super::{Reader, ReaderError};

    fn fixture() -> (Reader, Vec<u8>) {
        fixture_with_strings(b"Title\0")
    }

    #[test]
    fn reader_errors_preserve_io_sources() {
        let error = ReaderError::Io(std::io::Error::other("injected read failure"));

        assert!(StdError::source(&error).is_some());
    }

    fn fixture_with_strings(strings: &[u8]) -> (Reader, Vec<u8>) {
        let payload = b"asset payload";
        let payload_offset = 64u64;
        let asset_offset = payload_offset + payload.len() as u64;
        let page_offset = asset_offset + bbf_format::ASSET_SIZE as u64;
        let string_offset = page_offset + bbf_format::PAGE_SIZE as u64;
        let footer_offset = string_offset + strings.len() as u64;
        let asset_hash = xxhash_rust::xxh3::xxh3_128(payload);
        let asset = Asset {
            file_offset: payload_offset,
            hash_low: asset_hash as u64,
            hash_high: (asset_hash >> 64) as u64,
            file_size: payload.len() as u64,
            flags: 0,
            media_type: MediaType::Png.as_u8(),
        };
        let page = Page {
            asset_index: 0,
            flags: 0,
        };
        let footer = Footer {
            asset_offset,
            page_offset,
            string_pool_offset: string_offset,
            string_pool_size: strings.len() as u64,
            asset_count: 1,
            page_count: 1,
            footer_hash: 0,
            ..Footer::default()
        };
        let header = Header::new(0, 12, 16, footer_offset);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&header.encode());
        bytes.extend_from_slice(payload);
        bytes.extend_from_slice(&asset.encode());
        bytes.extend_from_slice(&page.encode());
        bytes.extend_from_slice(strings);
        let mut footer = footer;
        footer.footer_hash = xxhash_rust::xxh3::xxh3_64(&bytes[asset_offset as usize..]);
        bytes.extend_from_slice(&footer.encode());
        (Reader::from_bytes(bytes.clone()), bytes)
    }

    #[test]
    fn exposes_header_tables_payload_strings_and_hash() {
        let (reader, _) = fixture();
        let asset = reader
            .asset(0)
            .expect("asset table is valid")
            .expect("asset exists");

        assert_eq!(
            reader.header().unwrap().footer_offset as usize,
            reader.len() - 256
        );
        assert_eq!(reader.page(0).unwrap().unwrap().asset_index, 0);
        assert_eq!(reader.string(0).unwrap(), Some("Title"));
        assert_eq!(reader.asset_data(&asset), Some(&b"asset payload"[..]));

        let expected = xxhash_rust::xxh3::xxh3_128(b"asset payload");
        assert_eq!(
            reader.compute_asset_hash(0).unwrap(),
            Some((expected as u64, (expected >> 64) as u64))
        );
        assert_eq!(reader.verify_asset_hash(0).unwrap(), Some(true));
        assert!(reader.verify_footer_hash().unwrap());
    }

    #[test]
    fn indexed_reader_reuses_footer_for_table_and_hash_views() {
        let (reader, _) = fixture();
        let indexed = reader.indexed().unwrap();
        let asset = indexed.asset(0).unwrap().unwrap();

        assert_eq!(indexed.footer().asset_count, 1);
        assert_eq!(indexed.page(0).unwrap().unwrap().asset_index, 0);
        assert_eq!(indexed.string(0).unwrap(), Some("Title"));
        assert_eq!(indexed.asset_data(&asset), Some(&b"asset payload"[..]));
        assert_eq!(indexed.verify_asset_hash(0).unwrap(), Some(true));
        assert!(indexed.verify_footer_hash().unwrap());
    }

    #[test]
    fn returns_none_for_missing_entries_and_invalid_string_offsets() {
        let (reader, _) = fixture();

        assert_eq!(reader.asset(1).unwrap(), None);
        assert_eq!(reader.page(1).unwrap(), None);
        assert_eq!(reader.string(6).unwrap(), None);
        let string_pool_size = reader.footer().unwrap().string_pool_size;
        assert_eq!(reader.string(string_pool_size).unwrap(), None);
        assert_eq!(reader.expansion(0).unwrap(), None);
        assert_eq!(reader.verify_asset_hash(1).unwrap(), None);
    }

    #[test]
    fn exposes_non_utf8_string_pool_entries_as_raw_bytes() {
        let (reader, mut bytes) = fixture();
        let string_offset = reader.footer().unwrap().string_pool_offset as usize;
        bytes[string_offset] = 0xff;
        let reader = Reader::from_bytes(bytes);
        let indexed = reader.indexed().unwrap();

        assert_eq!(reader.string(0).unwrap(), None);
        assert_eq!(
            reader.string_bytes(0).unwrap(),
            Some(&[0xff, b'i', b't', b'l', b'e'][..])
        );
        assert_eq!(indexed.string(0).unwrap(), None);
        assert_eq!(
            indexed.string_bytes(0).unwrap(),
            Some(&[0xff, b'i', b't', b'l', b'e'][..])
        );
    }

    #[test]
    fn string_scan_uses_the_string_pool_boundary() {
        // ComicInfo.xml values can exceed the old fixed 2,048-byte scan
        // limit. The reader must use the declared pool boundary instead.
        let mut strings = vec![b'x'; 4096];
        strings.push(0);
        let (reader, _) = fixture_with_strings(&strings);

        assert_eq!(reader.string_bytes(0).unwrap(), Some(&strings[..4096]));
    }

    #[test]
    fn rejects_truncated_footer_and_table_ranges() {
        let (reader, bytes) = fixture();
        let mut truncated = bytes;
        truncated.truncate(truncated.len() - 1);
        assert!(reader.footer().is_ok());
        assert!(Reader::from_bytes(truncated).footer().is_err());

        let mut malformed = fixture().1;
        let footer_offset = reader.header().unwrap().footer_offset as usize;
        let mut footer = reader.footer().unwrap();
        footer.asset_offset = u64::MAX;
        malformed[footer_offset..footer_offset + bbf_format::FOOTER_SIZE]
            .copy_from_slice(&footer.encode());
        assert!(Reader::from_bytes(malformed).asset(0).is_err());
    }

    #[test]
    fn reports_footer_hash_mismatches_as_false() {
        let (_, mut bytes) = fixture();
        let footer_offset = Header::decode(&bytes).unwrap().footer_offset as usize;
        bytes[footer_offset + 104] ^= 1;
        assert!(!Reader::from_bytes(bytes).verify_footer_hash().unwrap());
    }

    #[test]
    fn arbitrary_bounded_bytes_do_not_panic() {
        for length in 0..=4096 {
            let bytes: Vec<_> = (0..length)
                .map(|index| {
                    let value = (index as u64)
                        .wrapping_mul(0x9e37_79b9)
                        .rotate_left((index % 63) as u32);
                    (value ^ (value >> 17) ^ (value >> 31)) as u8
                })
                .collect();
            let result = std::panic::catch_unwind(|| {
                let reader = Reader::from_bytes(bytes);
                let _ = reader.header();
                let _ = reader.footer();
                let _ = reader.indexed();
                let _ = reader.asset(0);
                let _ = reader.page(0);
                let _ = reader.section(0);
                let _ = reader.metadata(0);
                let _ = reader.expansion(0);
                let _ = reader.string(0);
                let _ = reader.string_bytes(0);
                let _ = reader.compute_asset_hash(0);
                let _ = reader.verify_footer_hash();
            });
            assert!(result.is_ok(), "reader panicked for {length}-byte input");
        }
    }
}
