//! BBF v3 wire-format types.
//!
//! This crate intentionally contains no file I/O. It owns the byte layout and
//! validation rules shared by the future reader, writer, and muxer crates.

use core::fmt;

pub const MAGIC: [u8; 4] = *b"BBF3";
pub const FORMAT_VERSION: u16 = 3;
pub const HEADER_SIZE: usize = 64;
pub const FOOTER_SIZE: usize = 256;
pub const ASSET_SIZE: usize = 48;
pub const PAGE_SIZE: usize = 16;
pub const SECTION_SIZE: usize = 32;
pub const METADATA_SIZE: usize = 32;
pub const EXPANSION_SIZE: usize = 128;
pub const NO_PARENT_OFFSET: u64 = u64::MAX;

pub const PETRIFICATION_FLAG: u32 = 0x0000_0001;
pub const VARIABLE_REAM_SIZE_FLAG: u32 = 0x0000_0002;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    TooShort { expected: usize, actual: usize },
    InvalidMagic([u8; 4]),
    UnsupportedVersion(u16),
    InvalidHeaderLength(u16),
    InvalidFooterLength(u8),
    NonZeroReserved(&'static str),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { expected, actual } => {
                write!(
                    formatter,
                    "BBF structure is too short: expected {expected}, got {actual}"
                )
            }
            Self::InvalidMagic(magic) => write!(formatter, "invalid BBF magic: {magic:?}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported BBF version: {version}")
            }
            Self::InvalidHeaderLength(length) => {
                write!(formatter, "invalid BBF header length: {length}")
            }
            Self::InvalidFooterLength(length) => {
                write!(formatter, "invalid BBF footer length: {length}")
            }
            Self::NonZeroReserved(field) => {
                write!(formatter, "non-zero reserved BBF field: {field}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

fn require_len(bytes: &[u8], expected: usize) -> Result<(), DecodeError> {
    if bytes.len() < expected {
        Err(DecodeError::TooShort {
            expected,
            actual: bytes.len(),
        })
    } else {
        Ok(())
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed-size read"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("fixed-size read"),
    )
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn require_zero(bytes: &[u8], field: &'static str) -> Result<(), DecodeError> {
    if bytes.iter().any(|byte| *byte != 0) {
        Err(DecodeError::NonZeroReserved(field))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub version: u16,
    pub flags: u32,
    pub alignment: u8,
    pub ream_size: u8,
    pub footer_offset: u64,
}

impl Header {
    pub const fn new(flags: u32, alignment: u8, ream_size: u8, footer_offset: u64) -> Self {
        Self {
            version: FORMAT_VERSION,
            flags,
            alignment,
            ream_size,
            footer_offset,
        }
    }

    pub fn encode(self) -> [u8; HEADER_SIZE] {
        let mut bytes = [0; HEADER_SIZE];
        bytes[..4].copy_from_slice(&MAGIC);
        write_u16(&mut bytes, 4, self.version);
        write_u16(&mut bytes, 6, HEADER_SIZE as u16);
        write_u32(&mut bytes, 8, self.flags);
        bytes[12] = self.alignment;
        bytes[13] = self.ream_size;
        write_u64(&mut bytes, 16, self.footer_offset);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        require_len(bytes, HEADER_SIZE)?;
        let magic = bytes[..4].try_into().expect("fixed-size magic");
        if magic != MAGIC {
            return Err(DecodeError::InvalidMagic(magic));
        }

        let version = read_u16(bytes, 4);
        if version != FORMAT_VERSION {
            return Err(DecodeError::UnsupportedVersion(version));
        }

        let header_length = read_u16(bytes, 6);
        if header_length != HEADER_SIZE as u16 {
            return Err(DecodeError::InvalidHeaderLength(header_length));
        }

        require_zero(&bytes[14..16], "header.reserved_extra")?;
        require_zero(&bytes[24..HEADER_SIZE], "header.reserved")?;

        Ok(Self {
            version,
            flags: read_u32(bytes, 8),
            alignment: bytes[12],
            ream_size: bytes[13],
            footer_offset: read_u64(bytes, 16),
        })
    }
}

impl Default for Header {
    fn default() -> Self {
        Self::new(VARIABLE_REAM_SIZE_FLAG, 12, 16, 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Footer {
    pub asset_offset: u64,
    pub page_offset: u64,
    pub section_offset: u64,
    pub metadata_offset: u64,
    pub expansion_offset: u64,
    pub string_pool_offset: u64,
    pub string_pool_size: u64,
    pub asset_count: u64,
    pub page_count: u64,
    pub section_count: u64,
    pub metadata_count: u64,
    pub expansion_count: u64,
    pub footer_hash: u64,
}

impl Footer {
    pub fn encode(self) -> [u8; FOOTER_SIZE] {
        let mut bytes = [0; FOOTER_SIZE];
        write_u64(&mut bytes, 0, self.asset_offset);
        write_u64(&mut bytes, 8, self.page_offset);
        write_u64(&mut bytes, 16, self.section_offset);
        write_u64(&mut bytes, 24, self.metadata_offset);
        write_u64(&mut bytes, 32, self.expansion_offset);
        write_u64(&mut bytes, 40, self.string_pool_offset);
        write_u64(&mut bytes, 48, self.string_pool_size);
        write_u64(&mut bytes, 56, self.asset_count);
        write_u64(&mut bytes, 64, self.page_count);
        write_u64(&mut bytes, 72, self.section_count);
        write_u64(&mut bytes, 80, self.metadata_count);
        write_u64(&mut bytes, 88, self.expansion_count);
        bytes[100] = FOOTER_SIZE as u8;
        write_u64(&mut bytes, 104, self.footer_hash);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        require_len(bytes, FOOTER_SIZE)?;
        if bytes[100] != FOOTER_SIZE as u8 {
            return Err(DecodeError::InvalidFooterLength(bytes[100]));
        }

        require_zero(&bytes[96..100], "footer.flags")?;
        require_zero(&bytes[101..104], "footer.padding")?;
        require_zero(&bytes[112..FOOTER_SIZE], "footer.reserved")?;

        Ok(Self {
            asset_offset: read_u64(bytes, 0),
            page_offset: read_u64(bytes, 8),
            section_offset: read_u64(bytes, 16),
            metadata_offset: read_u64(bytes, 24),
            expansion_offset: read_u64(bytes, 32),
            string_pool_offset: read_u64(bytes, 40),
            string_pool_size: read_u64(bytes, 48),
            asset_count: read_u64(bytes, 56),
            page_count: read_u64(bytes, 64),
            section_count: read_u64(bytes, 72),
            metadata_count: read_u64(bytes, 80),
            expansion_count: read_u64(bytes, 88),
            footer_hash: read_u64(bytes, 104),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Asset {
    pub file_offset: u64,
    pub hash_low: u64,
    pub hash_high: u64,
    pub file_size: u64,
    pub flags: u32,
    pub media_type: u8,
}

impl Asset {
    pub fn encode(self) -> [u8; ASSET_SIZE] {
        let mut bytes = [0; ASSET_SIZE];
        write_u64(&mut bytes, 0, self.file_offset);
        write_u64(&mut bytes, 8, self.hash_low);
        write_u64(&mut bytes, 16, self.hash_high);
        write_u64(&mut bytes, 24, self.file_size);
        write_u32(&mut bytes, 32, self.flags);
        bytes[38] = self.media_type;
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        require_len(bytes, ASSET_SIZE)?;
        require_zero(&bytes[36..38], "asset.reserved_value")?;
        require_zero(&bytes[39..ASSET_SIZE], "asset.reserved")?;
        Ok(Self {
            file_offset: read_u64(bytes, 0),
            hash_low: read_u64(bytes, 8),
            hash_high: read_u64(bytes, 16),
            file_size: read_u64(bytes, 24),
            flags: read_u32(bytes, 32),
            media_type: bytes[38],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Page {
    pub asset_index: u64,
    pub flags: u32,
}

impl Page {
    pub fn encode(self) -> [u8; PAGE_SIZE] {
        let mut bytes = [0; PAGE_SIZE];
        self.encode_into(&mut bytes);
        bytes
    }

    pub fn encode_into(self, bytes: &mut [u8; PAGE_SIZE]) {
        *bytes = [0; PAGE_SIZE];
        write_u64(bytes, 0, self.asset_index);
        write_u32(bytes, 8, self.flags);
    }

    /// Encodes a contiguous page table without constructing a temporary array
    /// or repeatedly converting each destination slice to an array reference.
    pub fn encode_many(pages: &[Self], bytes: &mut [u8]) {
        assert_eq!(bytes.len(), pages.len() * PAGE_SIZE);
        let (chunks, remainder) = bytes.as_chunks_mut::<PAGE_SIZE>();
        debug_assert!(remainder.is_empty());
        for (page, bytes) in pages.iter().zip(chunks) {
            bytes[..8].copy_from_slice(&page.asset_index.to_le_bytes());
            bytes[8..12].copy_from_slice(&page.flags.to_le_bytes());
            bytes[12..].fill(0);
        }
    }

    /// Encodes into a buffer that is already zeroed, matching the reference
    /// writer's zero-initialized page-table buffer.
    pub fn encode_many_zeroed(pages: &[Self], bytes: &mut [u8]) {
        assert_eq!(bytes.len(), pages.len() * PAGE_SIZE);
        let (chunks, remainder) = bytes.as_chunks_mut::<PAGE_SIZE>();
        debug_assert!(remainder.is_empty());
        for (page, bytes) in pages.iter().zip(chunks) {
            let wire = u128::from(page.asset_index) | (u128::from(page.flags) << 64);
            bytes.copy_from_slice(&wire.to_le_bytes());
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        require_len(bytes, PAGE_SIZE)?;
        require_zero(&bytes[12..PAGE_SIZE], "page.reserved")?;
        Ok(Self {
            asset_index: read_u64(bytes, 0),
            flags: read_u32(bytes, 8),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Section {
    pub title_offset: u64,
    pub start_index: u64,
    pub parent_offset: u64,
}

impl Section {
    pub fn encode(self) -> [u8; SECTION_SIZE] {
        let mut bytes = [0; SECTION_SIZE];
        write_u64(&mut bytes, 0, self.title_offset);
        write_u64(&mut bytes, 8, self.start_index);
        write_u64(&mut bytes, 16, self.parent_offset);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        require_len(bytes, SECTION_SIZE)?;
        require_zero(&bytes[24..SECTION_SIZE], "section.reserved")?;
        Ok(Self {
            title_offset: read_u64(bytes, 0),
            start_index: read_u64(bytes, 8),
            parent_offset: read_u64(bytes, 16),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Metadata {
    pub key_offset: u64,
    pub value_offset: u64,
    pub parent_offset: u64,
}

impl Metadata {
    pub fn encode(self) -> [u8; METADATA_SIZE] {
        let mut bytes = [0; METADATA_SIZE];
        write_u64(&mut bytes, 0, self.key_offset);
        write_u64(&mut bytes, 8, self.value_offset);
        write_u64(&mut bytes, 16, self.parent_offset);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        require_len(bytes, METADATA_SIZE)?;
        require_zero(&bytes[24..METADATA_SIZE], "metadata.reserved")?;
        Ok(Self {
            key_offset: read_u64(bytes, 0),
            value_offset: read_u64(bytes, 8),
            parent_offset: read_u64(bytes, 16),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Expansion {
    pub reserved: [u64; 10],
    pub flags: u32,
}

impl Expansion {
    pub fn encode(self) -> [u8; EXPANSION_SIZE] {
        let mut bytes = [0; EXPANSION_SIZE];
        for (index, value) in self.reserved.iter().copied().enumerate() {
            write_u64(&mut bytes, index * 8, value);
        }
        write_u32(&mut bytes, 80, self.flags);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        require_len(bytes, EXPANSION_SIZE)?;
        require_zero(&bytes[84..EXPANSION_SIZE], "expansion.reserved")?;
        let mut reserved = [0; 10];
        for (index, value) in reserved.iter_mut().enumerate() {
            *value = read_u64(bytes, index * 8);
        }
        Ok(Self {
            reserved,
            flags: read_u32(bytes, 80),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MediaType {
    Unknown = 0x00,
    Avif = 0x01,
    Png = 0x02,
    Webp = 0x03,
    Jxl = 0x04,
    Bmp = 0x05,
    Gif = 0x07,
    Tiff = 0x08,
    Jpg = 0x09,
}

impl MediaType {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn from_u8(value: u8) -> Self {
        match value {
            0x01 => Self::Avif,
            0x02 => Self::Png,
            0x03 => Self::Webp,
            0x04 => Self::Jxl,
            0x05 => Self::Bmp,
            0x07 => Self::Gif,
            0x08 => Self::Tiff,
            0x09 => Self::Jpg,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_matches_the_v3_wire_layout() {
        let header = Header::new(PETRIFICATION_FLAG, 12, 16, 0x0102_0304_0506_0708);
        let bytes = header.encode();

        assert_eq!(&bytes[..8], b"BBF3\x03\x00\x40\x00");
        assert_eq!(&bytes[8..16], &[1, 0, 0, 0, 12, 16, 0, 0]);
        assert_eq!(&bytes[16..24], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert!(bytes[24..].iter().all(|byte| *byte == 0));
        assert_eq!(Header::decode(&bytes), Ok(header));
    }

    #[test]
    fn footer_sets_the_required_length_and_round_trips() {
        let footer = Footer {
            asset_offset: 64,
            page_offset: 112,
            section_offset: 144,
            metadata_offset: 176,
            expansion_offset: 0,
            string_pool_offset: 208,
            string_pool_size: 12,
            asset_count: 1,
            page_count: 2,
            section_count: 3,
            metadata_count: 4,
            expansion_count: 0,
            footer_hash: 0x1122_3344_5566_7788,
        };

        let bytes = footer.encode();
        assert_eq!(bytes.len(), FOOTER_SIZE);
        assert_eq!(bytes[100], FOOTER_SIZE as u8);
        assert_eq!(Footer::decode(&bytes), Ok(footer));
    }

    #[test]
    fn records_have_the_reference_sizes() {
        assert_eq!(Asset::default().encode().len(), ASSET_SIZE);
        assert_eq!(Page::default().encode().len(), PAGE_SIZE);
        assert_eq!(Section::default().encode().len(), SECTION_SIZE);
        assert_eq!(Metadata::default().encode().len(), METADATA_SIZE);
        assert_eq!(Expansion::default().encode().len(), EXPANSION_SIZE);
    }

    #[test]
    fn page_encode_into_matches_owned_encoding() {
        let page = Page {
            asset_index: 4,
            flags: 8,
        };
        let mut encoded = [0xa5; PAGE_SIZE];
        page.encode_into(&mut encoded);
        assert_eq!(encoded, page.encode());
    }

    #[test]
    fn page_encode_many_matches_individual_encoding() {
        let pages = [
            Page {
                asset_index: 4,
                flags: 8,
            },
            Page {
                asset_index: 17,
                flags: 3,
            },
        ];
        let mut encoded = [0xa5; PAGE_SIZE * 2];
        Page::encode_many(&pages, &mut encoded);
        assert_eq!(&encoded[..PAGE_SIZE], &pages[0].encode());
        assert_eq!(&encoded[PAGE_SIZE..], &pages[1].encode());

        let mut zeroed = [0; PAGE_SIZE * 2];
        Page::encode_many_zeroed(&pages, &mut zeroed);
        assert_eq!(zeroed, encoded);
    }

    #[test]
    fn records_round_trip_without_native_layout_assumptions() {
        let asset = Asset {
            file_offset: 4096,
            hash_low: 1,
            hash_high: 2,
            file_size: 99,
            flags: 7,
            media_type: MediaType::Png.as_u8(),
        };
        let page = Page {
            asset_index: 4,
            flags: 8,
        };
        let section = Section {
            title_offset: 12,
            start_index: 3,
            parent_offset: NO_PARENT_OFFSET,
        };
        let metadata = Metadata {
            key_offset: 1,
            value_offset: 5,
            parent_offset: NO_PARENT_OFFSET,
        };
        let expansion = Expansion {
            reserved: [1; 10],
            flags: 9,
        };

        assert_eq!(Asset::decode(&asset.encode()), Ok(asset));
        assert_eq!(Page::decode(&page.encode()), Ok(page));
        assert_eq!(Section::decode(&section.encode()), Ok(section));
        assert_eq!(Metadata::decode(&metadata.encode()), Ok(metadata));
        assert_eq!(Expansion::decode(&expansion.encode()), Ok(expansion));
    }

    #[test]
    fn header_rejects_invalid_magic_and_reserved_bytes() {
        let mut bytes = Header::default().encode();
        bytes[0] = b'X';
        assert!(matches!(
            Header::decode(&bytes),
            Err(DecodeError::InvalidMagic(_))
        ));

        let mut bytes = Header::default().encode();
        bytes[24] = 1;
        assert_eq!(
            Header::decode(&bytes),
            Err(DecodeError::NonZeroReserved("header.reserved"))
        );
    }

    #[test]
    fn header_rejects_unsupported_versions_before_reader_views() {
        let mut bytes = Header::default().encode();
        bytes[4..6].copy_from_slice(&u16::MAX.to_le_bytes());

        assert!(matches!(
            Header::decode(&bytes),
            Err(DecodeError::UnsupportedVersion(u16::MAX))
        ));
    }

    #[test]
    fn unknown_media_types_are_preserved_as_unknown() {
        assert_eq!(MediaType::from_u8(0x06), MediaType::Unknown);
        assert_eq!(MediaType::from_u8(0xf0), MediaType::Unknown);
    }
}
