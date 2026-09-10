//! Rust-only C ABI exports matching the reference reader binding.
//!
//! The original project exposes these functions for its optional Emscripten
//! build. Rust keeps the same symbol names and pointer-oriented shape while
//! retaining bounded validation before returning a view into the owned file
//! bytes. The opaque reader must outlive every pointer returned from it.

#![allow(non_camel_case_types)]

use std::{
    ffi::{CStr, c_char},
    path::PathBuf,
    ptr,
};

use bbf_format::{
    ASSET_SIZE, EXPANSION_SIZE, FOOTER_SIZE, HEADER_SIZE, MAGIC, METADATA_SIZE, PAGE_SIZE,
    SECTION_SIZE,
};
use bbf_io::Reader;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct XXH128_hash_t {
    pub low64: u64,
    pub high64: u64,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct BBFHeader {
    pub magic: [u8; 4],
    pub version: u16,
    pub header_len: u16,
    pub flags: u32,
    pub alignment: u8,
    pub ream_size: u8,
    pub reserved_extra: u16,
    pub footer_offset: u64,
    pub reserved: [u8; 40],
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct BBFFooter {
    pub asset_offset: u64,
    pub page_offset: u64,
    pub section_offset: u64,
    pub meta_offset: u64,
    pub expansion_offset: u64,
    pub string_pool_offset: u64,
    pub string_pool_size: u64,
    pub asset_count: u64,
    pub page_count: u64,
    pub section_count: u64,
    pub meta_count: u64,
    pub expansion_count: u64,
    pub flags: u32,
    pub footer_len: u8,
    pub padding: [u8; 3],
    pub footer_hash: u64,
    pub reserved: [u8; 144],
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct BBFAsset {
    pub file_offset: u64,
    pub asset_hash: [u64; 2],
    pub file_size: u64,
    pub flags: u32,
    pub reserved_value: u16,
    pub media_type: u8,
    pub reserved: [u8; 9],
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct BBFPage {
    pub asset_index: u64,
    pub flags: u32,
    pub reserved: [u8; 4],
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct BBFSection {
    pub section_title_offset: u64,
    pub section_start_index: u64,
    pub section_parent_offset: u64,
    pub reserved: [u8; 8],
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct BBFMeta {
    pub key_offset: u64,
    pub value_offset: u64,
    pub parent_offset: u64,
    pub reserved: [u8; 8],
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct BBFExpansion {
    pub exp_reserved: [u64; 10],
    pub flags: u32,
    pub reserved: [u8; 44],
}

/// Opaque reader handle. Pointers returned by this crate borrow its owned data.
pub struct BBFReader {
    reader: Reader<Vec<u8>>,
    footer_cached: bool,
}

fn valid_range(reader: &BBFReader, offset: u64, size: u64) -> Option<usize> {
    let end = offset.checked_add(size)?;
    if end > reader.reader.len() as u64 {
        return None;
    }
    usize::try_from(offset).ok()
}

fn raw_ptr<T>(reader: &BBFReader, offset: u64, size: u64) -> *const T {
    let Some(offset) = valid_range(reader, offset, size) else {
        return ptr::null();
    };
    // Packed records have alignment 1, and this pointer is only used as an
    // opaque C view by the exported API.
    unsafe { reader.reader.bytes().as_ptr().add(offset).cast() }
}

fn raw_mut_ptr<T>(reader: &BBFReader, offset: u64, size: u64) -> *mut T {
    raw_ptr::<T>(reader, offset, size).cast_mut()
}

fn table_ptr<T>(reader: &BBFReader, offset: u64, count: u64, size: usize) -> *const T {
    let Some(byte_size) = count.checked_mul(size as u64) else {
        return ptr::null();
    };
    raw_ptr(reader, offset, byte_size)
}

unsafe fn reader_ref<'a>(reader: *const BBFReader) -> Option<&'a BBFReader> {
    (!reader.is_null()).then(|| unsafe { &*reader })
}

fn slice_for_ptr(reader: &BBFReader, pointer: *const u8, size: usize) -> Option<&[u8]> {
    let base = reader.reader.bytes().as_ptr() as usize;
    let address = pointer as usize;
    let offset = address.checked_sub(base)?;
    reader.reader.bytes().get(offset..offset.checked_add(size)?)
}

unsafe fn entry_ptr<T>(
    reader: *const BBFReader,
    table: *const u8,
    expected_offset: u64,
    count: u64,
    index: i64,
    size: usize,
) -> *const T {
    let Some(reader) = (unsafe { reader_ref(reader) }) else {
        return ptr::null();
    };
    if index < 0 || index as u64 >= count {
        return ptr::null();
    }
    let Some(table_offset) = valid_range(reader, expected_offset, 0) else {
        return ptr::null();
    };
    let base = reader.reader.bytes().as_ptr() as usize;
    if table as usize != base.saturating_add(table_offset) {
        return ptr::null();
    }
    let Some(index_offset) = (index as u64).checked_mul(size as u64) else {
        return ptr::null();
    };
    let Some(offset) = expected_offset.checked_add(index_offset) else {
        return ptr::null();
    };
    raw_ptr(reader, offset, size as u64)
}

fn decode_asset(reader: &BBFReader, asset: *const BBFAsset) -> Option<bbf_format::Asset> {
    let bytes = slice_for_ptr(reader, asset.cast(), ASSET_SIZE)?;
    Some(bbf_format::Asset {
        file_offset: u64::from_le_bytes(bytes[0..8].try_into().ok()?),
        hash_low: u64::from_le_bytes(bytes[8..16].try_into().ok()?),
        hash_high: u64::from_le_bytes(bytes[16..24].try_into().ok()?),
        file_size: u64::from_le_bytes(bytes[24..32].try_into().ok()?),
        flags: u32::from_le_bytes(bytes[32..36].try_into().ok()?),
        media_type: bytes[38],
    })
}

fn hash_asset(reader: &BBFReader, asset: bbf_format::Asset) -> XXH128_hash_t {
    let Some(data) = reader.reader.asset_data(&asset) else {
        return XXH128_hash_t::default();
    };
    let hash = xxhash_rust::xxh3::xxh3_128(data);
    XXH128_hash_t {
        low64: hash as u64,
        high64: (hash >> 64) as u64,
    }
}

/// Creates an owned reader handle from a NUL-terminated path.
///
/// # Safety
///
/// `file` must be null or point to a readable NUL-terminated C string for the
/// duration of this call. The returned handle must be released exactly once
/// with [`close_bbf_reader`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn create_bbf_reader(file: *const c_char) -> *mut BBFReader {
    if file.is_null() {
        return ptr::null_mut();
    }
    let bytes = unsafe { CStr::from_ptr(file).to_bytes() };
    #[cfg(unix)]
    let path = {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        PathBuf::from(OsStr::from_bytes(bytes))
    };
    #[cfg(not(unix))]
    let path = match std::str::from_utf8(bytes) {
        Ok(path) => PathBuf::from(path),
        Err(_) => return ptr::null_mut(),
    };
    let Ok(reader) = Reader::open(path) else {
        return ptr::null_mut();
    };
    Box::into_raw(Box::new(BBFReader {
        reader,
        footer_cached: false,
    }))
}

/// Releases a reader handle and all pointers borrowed from it.
///
/// # Safety
///
/// `reader` must be null or a handle returned by [`create_bbf_reader`] that
/// has not already been closed. No caller may use pointers borrowed from the
/// handle after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn close_bbf_reader(reader: *mut BBFReader) {
    if !reader.is_null() {
        drop(unsafe { Box::from_raw(reader) });
    }
}

/// Returns a pointer to the decoded header view owned by `reader`.
///
/// # Safety
///
/// `reader` must be null or a live handle returned by [`create_bbf_reader`].
/// The returned pointer is borrowed from `reader` and must not outlive it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_header(reader: *mut BBFReader) -> *mut BBFHeader {
    let Some(reader) = (unsafe { reader_ref(reader) }) else {
        return ptr::null_mut();
    };
    if reader.reader.header().is_err() {
        return ptr::null_mut();
    }
    raw_mut_ptr(reader, 0, HEADER_SIZE as u64)
}

/// Returns a pointer to the footer view after validating the header pointer.
///
/// # Safety
///
/// `reader` must be a live handle or null. `header` must be null or a header
/// pointer returned for that same handle. The returned pointer is borrowed
/// from `reader` and becomes invalid when the handle is closed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_footer(
    reader: *mut BBFReader,
    header: *mut BBFHeader,
) -> *mut BBFFooter {
    if header.is_null() {
        return ptr::null_mut();
    }
    let Some(reader_ref) = (unsafe { reader_ref(reader) }) else {
        return ptr::null_mut();
    };
    let Ok(header) = reader_ref.reader.header() else {
        return ptr::null_mut();
    };
    if reader_ref.reader.footer().is_err() {
        return ptr::null_mut();
    }
    let pointer: *mut BBFFooter = raw_mut_ptr(reader_ref, header.footer_offset, FOOTER_SIZE as u64);
    if pointer.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        (*reader).footer_cached = true;
    }
    pointer
}

unsafe fn table_view(
    reader: *mut BBFReader,
    footer: *mut BBFFooter,
    view: impl FnOnce(bbf_format::Footer) -> (u64, u64, usize),
) -> *const u8 {
    if footer.is_null() {
        return ptr::null();
    }
    let Some(reader) = (unsafe { reader_ref(reader) }) else {
        return ptr::null();
    };
    let Ok(decoded) = reader.reader.footer() else {
        return ptr::null();
    };
    let (offset, count, size) = view(decoded);
    table_ptr(reader, offset, count, size)
}

/// Returns the page table view borrowed from `reader`.
///
/// # Safety
///
/// `reader` and `footer` must be null or live pointers obtained from the same
/// reader; any non-null footer must be the reader's returned footer pointer.
/// The byte view must not outlive the reader.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_page_table(
    reader: *mut BBFReader,
    footer: *mut BBFFooter,
) -> *const u8 {
    unsafe {
        table_view(reader, footer, |footer| {
            (footer.page_offset, footer.page_count, PAGE_SIZE)
        })
    }
}

/// Returns the asset table view borrowed from `reader`.
///
/// # Safety
///
/// `reader` and `footer` must be null or live pointers obtained from the same
/// reader; any non-null footer must be the reader's returned footer pointer.
/// The byte view must not outlive the reader.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_asset_table(
    reader: *mut BBFReader,
    footer: *mut BBFFooter,
) -> *const u8 {
    unsafe {
        table_view(reader, footer, |footer| {
            (footer.asset_offset, footer.asset_count, ASSET_SIZE)
        })
    }
}

/// Returns the section table view borrowed from `reader`.
///
/// # Safety
///
/// `reader` and `footer` must be null or live pointers obtained from the same
/// reader; any non-null footer must be the reader's returned footer pointer.
/// The byte view must not outlive the reader.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_section_table(
    reader: *mut BBFReader,
    footer: *mut BBFFooter,
) -> *const u8 {
    unsafe {
        table_view(reader, footer, |footer| {
            (footer.section_offset, footer.section_count, SECTION_SIZE)
        })
    }
}

/// Returns the metadata table view borrowed from `reader`.
///
/// # Safety
///
/// `reader` and `footer` must be null or live pointers obtained from the same
/// reader; any non-null footer must be the reader's returned footer pointer.
/// The byte view must not outlive the reader.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_meta_table(
    reader: *mut BBFReader,
    footer: *mut BBFFooter,
) -> *const u8 {
    unsafe {
        table_view(reader, footer, |footer| {
            (footer.metadata_offset, footer.metadata_count, METADATA_SIZE)
        })
    }
}

/// Returns the expansion table view borrowed from `reader`.
///
/// # Safety
///
/// `reader` and `footer` must be null or live pointers obtained from the same
/// reader; any non-null footer must be the reader's returned footer pointer.
/// The byte view must not outlive the reader.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_expansion_table(
    reader: *mut BBFReader,
    footer: *mut BBFFooter,
) -> *const u8 {
    unsafe {
        table_view(reader, footer, |footer| {
            (
                footer.expansion_offset,
                footer.expansion_count,
                EXPANSION_SIZE,
            )
        })
    }
}

/// Returns one page entry from the reader's page table.
///
/// # Safety
///
/// `reader` must be live, and `table` must be the page-table pointer returned
/// for that reader or null. The returned entry is borrowed from `reader`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_page_entry(
    reader: *mut BBFReader,
    table: *const u8,
    index: i16,
) -> *const BBFPage {
    let Some(reader_ref) = (unsafe { reader_ref(reader) }) else {
        return ptr::null();
    };
    if !reader_ref.footer_cached {
        return ptr::null();
    }
    let Ok(footer) = reader_ref.reader.footer() else {
        return ptr::null();
    };
    unsafe {
        entry_ptr(
            reader,
            table,
            footer.page_offset,
            footer.page_count,
            index as i64,
            PAGE_SIZE,
        )
    }
}

/// Returns one asset entry from the reader's asset table.
///
/// # Safety
///
/// `reader` must be live, and `table` must be the asset-table pointer returned
/// for that reader or null. The returned entry is borrowed from `reader`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_asset_entry(
    reader: *mut BBFReader,
    table: *const u8,
    index: i32,
) -> *const BBFAsset {
    let Some(reader_ref) = (unsafe { reader_ref(reader) }) else {
        return ptr::null();
    };
    if !reader_ref.footer_cached {
        return ptr::null();
    }
    let Ok(footer) = reader_ref.reader.footer() else {
        return ptr::null();
    };
    unsafe {
        entry_ptr(
            reader,
            table,
            footer.asset_offset,
            footer.asset_count,
            index as i64,
            ASSET_SIZE,
        )
    }
}

/// Returns one section entry from the reader's section table.
///
/// # Safety
///
/// `reader` must be live, and `table` must be the section-table pointer
/// returned for that reader or null. The returned entry is borrowed from it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_section_entry(
    reader: *mut BBFReader,
    table: *const u8,
    index: i32,
) -> *const BBFSection {
    let Some(reader_ref) = (unsafe { reader_ref(reader) }) else {
        return ptr::null();
    };
    if !reader_ref.footer_cached {
        return ptr::null();
    }
    let Ok(footer) = reader_ref.reader.footer() else {
        return ptr::null();
    };
    unsafe {
        entry_ptr(
            reader,
            table,
            footer.section_offset,
            footer.section_count,
            index as i64,
            SECTION_SIZE,
        )
    }
}

/// Returns one metadata entry from the reader's metadata table.
///
/// # Safety
///
/// `reader` must be live, and `table` must be the metadata-table pointer
/// returned for that reader or null. The returned entry is borrowed from it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_meta_entry(
    reader: *mut BBFReader,
    table: *const u8,
    index: i32,
) -> *const BBFMeta {
    let Some(reader_ref) = (unsafe { reader_ref(reader) }) else {
        return ptr::null();
    };
    if !reader_ref.footer_cached {
        return ptr::null();
    }
    let Ok(footer) = reader_ref.reader.footer() else {
        return ptr::null();
    };
    unsafe {
        entry_ptr(
            reader,
            table,
            footer.metadata_offset,
            footer.metadata_count,
            index as i64,
            METADATA_SIZE,
        )
    }
}

/// Returns one expansion entry from the reader's expansion table.
///
/// # Safety
///
/// `reader` must be live, and `table` must be the expansion-table pointer
/// returned for that reader or null. The returned entry is borrowed from it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_expansion_entry(
    reader: *mut BBFReader,
    table: *const u8,
    index: i32,
) -> *const BBFExpansion {
    let Some(reader_ref) = (unsafe { reader_ref(reader) }) else {
        return ptr::null();
    };
    if !reader_ref.footer_cached {
        return ptr::null();
    }
    let Ok(footer) = reader_ref.reader.footer() else {
        return ptr::null();
    };
    unsafe {
        entry_ptr(
            reader,
            table,
            footer.expansion_offset,
            footer.expansion_count,
            index as i64,
            EXPANSION_SIZE,
        )
    }
}

/// Returns a pointer to payload data at a validated file offset.
///
/// # Safety
///
/// `reader` must be null or a live handle returned by [`create_bbf_reader`].
/// The returned pointer is borrowed from the handle and may be one-past the
/// byte buffer when `file_offset == file_size`; callers must not dereference
/// that sentinel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_asset_data(reader: *mut BBFReader, file_offset: u64) -> *const u8 {
    let Some(reader) = (unsafe { reader_ref(reader) }) else {
        return ptr::null();
    };
    if file_offset > reader.reader.len() as u64 {
        return ptr::null();
    }
    unsafe { reader.reader.bytes().as_ptr().add(file_offset as usize) }
}

/// Returns a NUL-terminated string-pool entry borrowed from `reader`.
///
/// # Safety
///
/// `reader` must be null or a live handle returned by [`create_bbf_reader`].
/// The returned C string must not outlive the handle or be mutated by the
/// caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_bbf_string(
    reader: *mut BBFReader,
    string_offset: u64,
) -> *const c_char {
    let Some(reader) = (unsafe { reader_ref(reader) }) else {
        return ptr::null();
    };
    if !reader.footer_cached {
        return ptr::null();
    }
    match reader.reader.string_bytes(string_offset) {
        Ok(Some(bytes)) => bytes.as_ptr().cast(),
        _ => ptr::null(),
    }
}

/// Checks the BBF magic bytes for a reader/header pair.
///
/// # Safety
///
/// `reader` must be null or a live handle. `header` must be null or a header
/// pointer returned for that same handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn check_bbf_magic(reader: *mut BBFReader, header: *mut BBFHeader) -> i32 {
    if header.is_null() {
        return 0;
    }
    let Some(reader) = (unsafe { reader_ref(reader) }) else {
        return 0;
    };
    if reader.reader.header().is_err() {
        return 0;
    }
    i32::from(reader.reader.bytes().get(..4) == Some(&MAGIC))
}

/// Computes the XXH3-128 digest for an asset entry pointer.
///
/// # Safety
///
/// `reader` must be live or null, and `asset` must be null or an asset-entry
/// pointer returned from the same reader. The pointer must remain valid for
/// the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn compute_asset_hash_from_struct(
    reader: *mut BBFReader,
    asset: *const BBFAsset,
) -> XXH128_hash_t {
    let Some(reader_ref) = (unsafe { reader_ref(reader) }) else {
        return XXH128_hash_t::default();
    };
    let Some(asset) = decode_asset(reader_ref, asset) else {
        return XXH128_hash_t::default();
    };
    hash_asset(reader_ref, asset)
}

/// Computes the XXH3-128 digest for an indexed asset entry.
///
/// # Safety
///
/// `reader` must be live or null. `table` must be null or the asset-table
/// pointer returned for that reader; the index must be valid for the table.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn compute_asset_hash_from_index(
    reader: *mut BBFReader,
    table: *mut u8,
    index: i32,
) -> XXH128_hash_t {
    let asset = unsafe { get_bbf_asset_entry(reader, table.cast_const(), index) };
    unsafe { compute_asset_hash_from_struct(reader, asset) }
}

#[cfg(test)]
mod tests {
    use std::{ffi::CString, fs, ptr};

    use super::*;

    fn decode_hex(hex: &str) -> Vec<u8> {
        let hex = hex.trim();
        (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn exports_bounded_reader_views_and_hashing() {
        assert_eq!(std::mem::size_of::<BBFHeader>(), HEADER_SIZE);
        assert_eq!(std::mem::size_of::<BBFFooter>(), FOOTER_SIZE);
        assert_eq!(std::mem::size_of::<BBFAsset>(), ASSET_SIZE);
        assert_eq!(std::mem::size_of::<BBFPage>(), PAGE_SIZE);
        assert_eq!(std::mem::size_of::<BBFSection>(), SECTION_SIZE);
        assert_eq!(std::mem::size_of::<BBFMeta>(), METADATA_SIZE);
        assert_eq!(std::mem::size_of::<BBFExpansion>(), EXPANSION_SIZE);

        let path = std::env::temp_dir().join(format!("bbf-ffi-test-{}", std::process::id()));
        fs::write(
            &path,
            decode_hex(include_str!(
                "../../bbfmux/tests/fixtures/cpp_basic.bbf.hex"
            )),
        )
        .unwrap();
        let path_string = CString::new(path.to_str().unwrap()).unwrap();
        let missing_path = path.with_extension("missing");
        let _ = fs::remove_file(&missing_path);
        let missing_path_string = CString::new(missing_path.to_str().unwrap()).unwrap();
        assert!(unsafe { create_bbf_reader(missing_path_string.as_ptr()) }.is_null());

        let uncached_reader = unsafe { create_bbf_reader(path_string.as_ptr()) };
        assert!(!uncached_reader.is_null());
        assert!(unsafe { get_bbf_string(uncached_reader, 0) }.is_null());
        let uncached_header = unsafe { get_bbf_header(uncached_reader) };
        let uncached_footer = unsafe { get_bbf_footer(uncached_reader, uncached_header) };
        assert!(!uncached_footer.is_null());
        let uncached_assets = unsafe { get_bbf_asset_table(uncached_reader, uncached_footer) };
        assert!(!unsafe { get_bbf_asset_entry(uncached_reader, uncached_assets, 0) }.is_null());
        assert!(!unsafe { get_bbf_string(uncached_reader, 0) }.is_null());
        unsafe { close_bbf_reader(uncached_reader) };

        let reader = unsafe { create_bbf_reader(path_string.as_ptr()) };
        assert!(!reader.is_null());
        let header = unsafe { get_bbf_header(reader) };
        assert!(!header.is_null());
        assert_eq!(unsafe { check_bbf_magic(reader, header) }, 1);
        let footer = unsafe { get_bbf_footer(reader, header) };
        assert!(!footer.is_null());
        assert!(unsafe { get_bbf_footer(reader, ptr::null_mut()) }.is_null());
        assert_eq!(unsafe { check_bbf_magic(reader, ptr::null_mut()) }, 0);

        let assets = unsafe { get_bbf_asset_table(reader, footer) };
        let pages = unsafe { get_bbf_page_table(reader, footer) };
        let sections = unsafe { get_bbf_section_table(reader, footer) };
        let metadata = unsafe { get_bbf_meta_table(reader, footer) };
        let expansions = unsafe { get_bbf_expansion_table(reader, footer) };
        assert!(!assets.is_null());
        assert!(!pages.is_null());
        assert!(!sections.is_null());
        assert!(!metadata.is_null());
        assert!(!expansions.is_null());
        assert!(!unsafe { get_bbf_asset_entry(reader, assets, 0) }.is_null());
        assert!(!unsafe { get_bbf_page_entry(reader, pages, 0) }.is_null());
        assert!(!unsafe { get_bbf_section_entry(reader, sections, 0) }.is_null());
        assert!(unsafe { get_bbf_asset_entry(reader, assets, 2) }.is_null());
        assert!(unsafe { get_bbf_page_entry(reader, pages, -1) }.is_null());
        assert!(unsafe { get_bbf_page_entry(reader, pages, i16::MAX) }.is_null());
        assert!(unsafe { get_bbf_section_entry(reader, sections, -1) }.is_null());
        assert!(unsafe { get_bbf_section_entry(reader, sections, i32::MAX) }.is_null());
        assert!(unsafe { get_bbf_meta_entry(reader, metadata, -1) }.is_null());
        assert!(unsafe { get_bbf_meta_entry(reader, metadata, i32::MAX) }.is_null());
        assert!(unsafe { get_bbf_expansion_entry(reader, expansions, -1) }.is_null());
        assert!(unsafe { get_bbf_expansion_entry(reader, expansions, 0) }.is_null());
        assert!(unsafe { get_bbf_asset_entry(reader, ptr::null(), 0) }.is_null());

        let from_index = unsafe { compute_asset_hash_from_index(reader, assets.cast_mut(), 0) };
        let from_struct = unsafe {
            compute_asset_hash_from_struct(reader, get_bbf_asset_entry(reader, assets, 0))
        };
        assert_eq!(from_index, from_struct);
        assert_eq!(
            unsafe { compute_asset_hash_from_index(reader, assets.cast_mut(), 2) },
            XXH128_hash_t::default()
        );
        assert!(!unsafe { get_bbf_string(reader, 0) }.is_null());
        assert!(!unsafe { get_bbf_asset_data(reader, 0) }.is_null());
        let file_size = fs::metadata(&path).unwrap().len();
        assert!(!unsafe { get_bbf_asset_data(reader, file_size) }.is_null());
        assert!(unsafe { get_bbf_asset_data(reader, u64::MAX) }.is_null());
        assert!(unsafe { get_bbf_header(ptr::null_mut()) }.is_null());
        assert!(unsafe { get_bbf_footer(ptr::null_mut(), header) }.is_null());
        unsafe { close_bbf_reader(reader) };

        let unsupported_path = path.with_extension("unsupported-version");
        let mut unsupported_bytes = decode_hex(include_str!(
            "../../bbfmux/tests/fixtures/cpp_basic.bbf.hex"
        ));
        unsupported_bytes[4] = 4;
        unsupported_bytes[5] = 0;
        fs::write(&unsupported_path, unsupported_bytes).unwrap();
        let unsupported_path_string = CString::new(unsupported_path.to_str().unwrap()).unwrap();
        let unsupported_reader = unsafe { create_bbf_reader(unsupported_path_string.as_ptr()) };
        assert!(!unsupported_reader.is_null());
        assert!(unsafe { get_bbf_header(unsupported_reader) }.is_null());
        unsafe { close_bbf_reader(unsupported_reader) };
        fs::remove_file(unsupported_path).unwrap();

        fs::remove_file(path).unwrap();
    }
}
