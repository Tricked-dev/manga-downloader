// This crate is only consumed inside the workspace; requiring rustdoc `# Errors`
// sections on every helper adds noise without improving call sites.
#![allow(clippy::missing_errors_doc)]

mod archive;
mod avif;

pub use avif::{decode_avif, encode_lossless_avif_rgb, encode_lossless_avif_rgba};

/// Decode supported source formats, including AVIF through the native decoder.
pub fn decode_image(input: &[u8]) -> anyhow::Result<image::DynamicImage> {
    if image::guess_format(input)? == image::ImageFormat::Avif {
        decode_avif(input)
    } else {
        Ok(image::load_from_memory(input)?)
    }
}

use anyhow::{Context, Result};
use fs4::{FileExt, TryLockError};
use image::DynamicImage;
use image::ExtendedColorType;
use image::ImageEncoder;
use image::ImageFormat;
use image::imageops::FilterType;
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

pub use archive::{
    ArchiveEntry, ArchiveInfo, ComicInfo, EntryBytes, FileDimension, content_type_for,
    extract_cover, extract_cover_or_first_page, extract_named_image, extract_named_images,
    extract_page, extract_positioned_images, extract_positioned_images_with_content_types,
    image_dimensions, image_dimensions_for_entries, inspect_archive,
    is_supported_image_content_type, thumbnail_jpeg,
};

pub const DEFAULT_AVIF_CONVERSION_WORKERS: usize = 5;
pub const DEFAULT_AVIF_TARGET_WIDTH: u32 = 800;
const DEFAULT_AVIF_ENCODING_SPEED: u8 = 4;
const ZSTD_COMPRESSION_LEVEL: i32 = 16;
const COMIX_SCRAMBLE_GRID: u32 = 5;
const COMIX_DESCRAMBLE_MAP: [usize; 25] = [
    2, 13, 4, 9, 0, 14, 18, 24, 8, 6, 1, 19, 11, 21, 5, 23, 3, 20, 22, 12, 10, 17, 16, 7, 15,
];

#[derive(Debug, Clone, Copy, Default)]
pub struct AvifConversionTimings {
    pub decode: Duration,
    pub resize: Duration,
    pub colorspace: Duration,
    pub encode: Duration,
}

#[must_use]
/// Returns the default number of CPU workers reserved for image-heavy work.
pub fn cpu_worker_budget() -> usize {
    let available = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    ((available * 4) / 5).max(1)
}

/// Converts image bytes to AVIF using the default maximum width.
pub fn convert_to_avif(input: &[u8], quality: u8) -> Result<Vec<u8>> {
    convert_to_avif_with_max_width(input, quality, DEFAULT_AVIF_TARGET_WIDTH)
}

/// Converts image bytes to AVIF, resizing wider images down to `max_width`.
pub fn convert_to_avif_with_max_width(
    input: &[u8],
    quality: u8,
    max_width: u32,
) -> Result<Vec<u8>> {
    convert_to_avif_with_max_width_timed(input, quality, max_width).map(|(body, _timings)| body)
}

/// Converts image bytes to AVIF and returns per-stage conversion timings.
pub fn convert_to_avif_with_max_width_timed(
    input: &[u8],
    quality: u8,
    max_width: u32,
) -> Result<(Vec<u8>, AvifConversionTimings)> {
    let mut timings = AvifConversionTimings::default();

    let started = Instant::now();
    let img = image::load_from_memory(input).context("failed to decode image bytes")?;
    timings.decode = started.elapsed();

    let started = Instant::now();
    let img = resize_to_max_width(img, max_width);
    timings.resize = started.elapsed();

    let started = Instant::now();
    let rgb = img.to_rgb8();
    timings.colorspace = started.elapsed();

    let mut buf = Vec::with_capacity(input.len().min(1024 * 1024));
    let cursor = std::io::Cursor::new(&mut buf);
    let encoder = image::codecs::avif::AvifEncoder::new_with_speed_quality(
        cursor,
        DEFAULT_AVIF_ENCODING_SPEED,
        quality,
    );
    let started = Instant::now();
    encoder
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )
        .context("failed to encode image as AVIF")?;
    timings.encode = started.elapsed();

    Ok((buf, timings))
}

/// Converts image bytes to JPEG at the requested quality.
pub fn convert_to_jpeg(input: &[u8], quality: u8) -> Result<Vec<u8>> {
    let img = image::load_from_memory(input).context("failed to decode image bytes")?;
    let rgb = img.to_rgb8();
    let mut buf = Vec::with_capacity(input.len().min(1024 * 1024));
    let cursor = std::io::Cursor::new(&mut buf);
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(cursor, quality);
    encoder
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )
        .context("failed to encode image as JPEG")?;

    Ok(buf)
}

/// Detects whether bytes are one of the image formats this crate can decode.
pub fn detect_supported_image_format(input: &[u8]) -> Result<&'static str> {
    let format = image::guess_format(input).context("image format could not be determined")?;
    supported_image_format_name(format)
        .ok_or_else(|| anyhow::anyhow!("unsupported image format detected: {format:?}"))
}

/// Reorders a Comix 5x5 scrambled image into a PNG.
pub fn descramble_comix_5x5_to_png(input: &[u8]) -> Result<Vec<u8>> {
    descramble_comix_5x5_with_map_to_png(input, &COMIX_DESCRAMBLE_MAP)
}

/// Reorders a 5x5 tiled image using `map` and encodes the result as PNG.
pub fn descramble_comix_5x5_with_map_to_png(input: &[u8], map: &[usize; 25]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(input)
        .context("failed to decode Comix scrambled image")?
        .to_rgba8();
    let width = img.width();
    let height = img.height();
    let mut output = image::RgbaImage::new(width, height);

    for (destination_index, source_index) in map.iter().copied().enumerate() {
        let destination = tile_box(destination_index, width, height);
        let source = tile_box(source_index, width, height);
        let tile = image::imageops::crop_imm(&img, source.x, source.y, source.width, source.height)
            .to_image();
        image::imageops::replace(
            &mut output,
            &tile,
            i64::from(destination.x),
            i64::from(destination.y),
        );
    }

    let mut buf = Vec::with_capacity(input.len());
    let cursor = Cursor::new(&mut buf);
    let encoder = image::codecs::png::PngEncoder::new(cursor);
    encoder
        .write_image(
            output.as_raw(),
            output.width(),
            output.height(),
            ExtendedColorType::Rgba8,
        )
        .context("failed to encode descrambled Comix image as PNG")?;
    Ok(buf)
}

/// Converts image entries in memory to AVIF on a bounded Rayon pool.
///
/// Non-image entries are returned unchanged.
pub fn convert_pages_to_avif_with_workers(
    pages: Vec<(String, Vec<u8>)>,
    quality: u8,
    worker_threads: usize,
) -> Result<Vec<(String, Vec<u8>)>> {
    let pool = ThreadPoolBuilder::new()
        .num_threads(avif_worker_threads(worker_threads, pages.len()))
        .build()?;

    pool.install(|| -> Result<Vec<(String, Vec<u8>)>> {
        pages
            .into_par_iter()
            .map(|(name, data)| {
                if is_image_entry(&name) {
                    let avif_data = convert_to_avif(&data, quality)
                        .with_context(|| format!("failed to convert page {name} to AVIF"))?;
                    Ok((replace_with_avif_extension(&name), avif_data))
                } else {
                    Ok((name, data))
                }
            })
            .collect()
    })
}

/// Converts staged image files to AVIF on a bounded Rayon pool.
pub fn convert_staged_pages_to_avif_with_workers(
    pages: Vec<(String, PathBuf)>,
    quality: u8,
    worker_threads: usize,
) -> Result<Vec<(String, PathBuf)>> {
    convert_staged_pages_to_avif_with_workers_cancelable(pages, quality, worker_threads, None)
}

/// Converts staged image files to AVIF unless `cancel_flag` is raised.
pub fn convert_staged_pages_to_avif_with_workers_cancelable(
    pages: Vec<(String, PathBuf)>,
    quality: u8,
    worker_threads: usize,
    cancel_flag: Option<&AtomicBool>,
) -> Result<Vec<(String, PathBuf)>> {
    convert_staged_pages_to_avif_with_workers_cancelable_progress(
        pages,
        quality,
        worker_threads,
        cancel_flag,
        None,
    )
}

/// Converts staged image files to AVIF with optional cancellation and progress counting.
pub fn convert_staged_pages_to_avif_with_workers_cancelable_progress(
    pages: Vec<(String, PathBuf)>,
    quality: u8,
    worker_threads: usize,
    cancel_flag: Option<&AtomicBool>,
    converted_pages: Option<&AtomicUsize>,
) -> Result<Vec<(String, PathBuf)>> {
    let pool = ThreadPoolBuilder::new()
        .num_threads(avif_worker_threads(worker_threads, pages.len()))
        .build()?;

    pool.install(|| -> Result<Vec<(String, PathBuf)>> {
        pages
            .into_par_iter()
            .map(|(name, path)| {
                ensure_not_cancelled(cancel_flag)?;
                let data = backend_fs::read_bytes_sync(&path).with_context(|| {
                    format!("failed to read staged page {name} from {}", path.display())
                })?;
                ensure_not_cancelled(cancel_flag)?;
                let avif_data = convert_to_avif(&data, quality).with_context(|| {
                    format!(
                        "failed to convert staged page {name} at {} to AVIF",
                        path.display()
                    )
                })?;
                ensure_not_cancelled(cancel_flag)?;
                backend_fs::write_bytes_sync(&path, &avif_data).with_context(|| {
                    format!(
                        "failed to write converted staged page {name} to {}",
                        path.display()
                    )
                })?;
                ensure_not_cancelled(cancel_flag)?;
                if let Some(counter) = converted_pages {
                    counter.fetch_add(1, Ordering::Relaxed);
                }
                Ok((replace_with_avif_extension(&name), path))
            })
            .collect()
    })
}

/// Builds a zstd-compressed tar archive from staged page paths.
pub fn build_zstd_folder_from_paths(
    pages: &[(String, PathBuf)],
    output_path: &Path,
    comicinfo_xml: Option<&str>,
    leading_entry: Option<(String, Vec<u8>)>,
) -> Result<()> {
    build_zstd_folder_from_paths_cancelable(pages, output_path, comicinfo_xml, leading_entry, None)
}

/// Builds a zstd-compressed tar archive unless `cancel_flag` is raised.
pub fn build_zstd_folder_from_paths_cancelable(
    pages: &[(String, PathBuf)],
    output_path: &Path,
    comicinfo_xml: Option<&str>,
    leading_entry: Option<(String, Vec<u8>)>,
    cancel_flag: Option<&AtomicBool>,
) -> Result<()> {
    build_zstd_folder_from_paths_cancelable_with_workers(
        pages,
        output_path,
        comicinfo_xml,
        leading_entry,
        cancel_flag,
        cpu_worker_budget(),
    )
}

/// Builds a zstd-compressed tar archive with an explicit zstd worker count.
pub fn build_zstd_folder_from_paths_cancelable_with_workers(
    pages: &[(String, PathBuf)],
    output_path: &Path,
    comicinfo_xml: Option<&str>,
    leading_entry: Option<(String, Vec<u8>)>,
    cancel_flag: Option<&AtomicBool>,
    compression_workers: usize,
) -> Result<()> {
    let _lock = acquire_archive_lock(output_path)?;
    let temp_file = create_archive_temp_output(output_path)?;
    (|| -> Result<()> {
        let encoder = zstd_encoder(
            temp_file,
            compression_workers,
            archive_entry_count(
                pages.len(),
                comicinfo_xml.is_some(),
                leading_entry.is_some(),
            ),
        )?;
        let mut archive = tar::Builder::new(encoder);

        ensure_not_cancelled(cancel_flag)?;

        if let Some(xml) = comicinfo_xml {
            append_archive_bytes(&mut archive, "ComicInfo.xml", xml.as_bytes())?;
        }

        if let Some((filename, data)) = leading_entry {
            ensure_not_cancelled(cancel_flag)?;
            append_archive_bytes(&mut archive, &filename, &data)?;
        }

        for (filename, path) in pages {
            ensure_not_cancelled(cancel_flag)?;
            let mut input = backend_fs::open_file(path)?;
            let size = backend_fs::file_size_sync(path)?;
            append_archive_reader(&mut archive, filename, size, &mut input)?;
        }

        ensure_not_cancelled(cancel_flag)?;
        archive.finish()?;
        let temp_file = archive.into_inner()?.finish()?;
        persist_archive_temp(temp_file, output_path)?;
        Ok(())
    })()
}

/// Rewrites all image entries in a zstd folder archive as AVIF.
pub fn reencode_zstd_folder_to_avif(archive_path: &Path, quality: u8) -> Result<usize> {
    reencode_zstd_folder_to_avif_with_workers(archive_path, quality, cpu_worker_budget())
}

/// Rewrites all image entries in a zstd folder archive as AVIF with explicit workers.
pub fn reencode_zstd_folder_to_avif_with_workers(
    archive_path: &Path,
    quality: u8,
    worker_threads: usize,
) -> Result<usize> {
    let _lock = acquire_archive_lock(archive_path)?;
    let mut entries = read_zstd_folder_entries(archive_path)?;

    let pool = ThreadPoolBuilder::new()
        .num_threads(avif_worker_threads(worker_threads, entries.len()))
        .build()?;
    let converted = pool.install(|| -> Result<usize> {
        entries
            .par_iter_mut()
            .map(|entry| match entry {
                ZstdArchiveEntry::File {
                    name,
                    data,
                    is_image: true,
                } => {
                    let avif_data = convert_to_avif(data, quality)
                        .with_context(|| format!("failed to convert archive entry {name}"))?;
                    *data = avif_data;
                    *name = replace_with_avif_extension(name);
                    Ok(1)
                }
                ZstdArchiveEntry::Directory(_) | ZstdArchiveEntry::File { .. } => Ok(0),
            })
            .try_reduce(|| 0usize, |acc, converted| Ok(acc + converted))
    })?;

    let temp_file = create_archive_temp_output(archive_path)?;
    let result = (|| -> Result<()> {
        let temp_file = write_zstd_folder_entries(temp_file, entries, worker_threads)?;
        persist_archive_temp(temp_file, archive_path)
    })();

    result?;

    Ok(converted)
}

/// Counts image entries in a zstd folder archive.
pub fn count_zstd_folder_images(archive_path: &Path) -> Result<usize> {
    let input = backend_fs::open_file(archive_path)
        .with_context(|| format!("failed to open {}", archive_path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(input)
        .with_context(|| format!("failed to decode zstd archive {}", archive_path.display()))?;
    let mut archive = tar::Archive::new(decoder);
    let mut image_count = 0usize;

    for entry in archive.entries()? {
        let entry = entry?;
        if entry.header().entry_type().is_file() && is_image_entry(&archive_entry_name(&entry)?) {
            image_count += 1;
        }
    }

    Ok(image_count)
}

/// Replaces or inserts the `ComicInfo.xml` entry in a zstd folder archive.
pub fn rewrite_zstd_folder_comicinfo_xml(archive_path: &Path, comicinfo_xml: &str) -> Result<()> {
    let _lock = acquire_archive_lock(archive_path)?;
    let mut entries = read_zstd_folder_entries(archive_path)?;
    let mut replaced = false;

    for entry in &mut entries {
        if let ZstdArchiveEntry::File { name, data, .. } = entry
            && name.eq_ignore_ascii_case("ComicInfo.xml")
        {
            *name = "ComicInfo.xml".to_string();
            *data = comicinfo_xml.as_bytes().to_vec();
            replaced = true;
        }
    }

    if !replaced {
        entries.insert(
            0,
            ZstdArchiveEntry::File {
                name: "ComicInfo.xml".to_string(),
                data: comicinfo_xml.as_bytes().to_vec(),
                is_image: false,
            },
        );
    }

    let temp_file = create_archive_temp_output(archive_path)?;
    let result = write_zstd_folder_entries(temp_file, entries, cpu_worker_budget())
        .and_then(|temp_file| persist_archive_temp(temp_file, archive_path));

    result?;
    Ok(())
}

fn avif_worker_threads(worker_threads: usize, entry_count: usize) -> usize {
    worker_threads.max(1).min(entry_count.max(1))
}

#[must_use]
/// Bounds the zstd worker count by both worker availability and archive entry count.
pub fn archive_compression_worker_threads(worker_threads: usize, entry_count: usize) -> usize {
    worker_threads.max(1).min(entry_count.max(1))
}

fn ensure_not_cancelled(cancel_flag: Option<&AtomicBool>) -> Result<()> {
    if cancel_flag.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
        anyhow::bail!("operation cancelled");
    }

    Ok(())
}

enum ZstdArchiveEntry {
    Directory(String),
    File {
        name: String,
        data: Vec<u8>,
        is_image: bool,
    },
}

fn read_zstd_folder_entries(path: &Path) -> Result<Vec<ZstdArchiveEntry>> {
    let input = backend_fs::open_file(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(input)
        .with_context(|| format!("failed to decode zstd archive {}", path.display()))?;
    let mut archive = tar::Archive::new(decoder);
    let mut entries = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = archive_entry_name(&entry)?;

        if entry.header().entry_type().is_dir() {
            entries.push(ZstdArchiveEntry::Directory(name));
            continue;
        }

        if !entry.header().entry_type().is_file() {
            continue;
        }

        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .with_context(|| format!("failed to read archive entry {name}"))?;
        let is_image = is_image_entry(&name);
        entries.push(ZstdArchiveEntry::File {
            name,
            data,
            is_image,
        });
    }

    Ok(entries)
}

fn write_zstd_folder_entries<W: Write>(
    output: W,
    entries: Vec<ZstdArchiveEntry>,
    compression_workers: usize,
) -> Result<W> {
    let encoder = zstd_encoder(output, compression_workers, entries.len())?;
    let mut archive = tar::Builder::new(encoder);

    for entry in entries {
        match entry {
            ZstdArchiveEntry::Directory(name) => {
                archive.append_dir(Path::new(&name), Path::new("."))?;
            }
            ZstdArchiveEntry::File { name, data, .. } => {
                append_archive_bytes(&mut archive, &name, &data)?;
            }
        }
    }

    archive.finish()?;
    Ok(archive.into_inner()?.finish()?)
}

fn zstd_encoder<W: Write>(
    output: W,
    compression_workers: usize,
    entry_count: usize,
) -> Result<zstd::stream::write::Encoder<'static, W>> {
    let mut encoder = zstd::stream::write::Encoder::new(output, ZSTD_COMPRESSION_LEVEL)?;
    let workers = archive_compression_worker_threads(compression_workers, entry_count);
    encoder.multithread(u32::try_from(workers).unwrap_or(u32::MAX))?;
    Ok(encoder)
}

fn archive_entry_count(page_count: usize, has_comicinfo: bool, has_leading_entry: bool) -> usize {
    page_count + usize::from(has_comicinfo) + usize::from(has_leading_entry)
}

fn append_archive_bytes<W: Write>(
    archive: &mut tar::Builder<W>,
    name: &str,
    data: &[u8],
) -> Result<()> {
    append_archive_reader(
        archive,
        name,
        u64::try_from(data.len()).context("archive entry too large")?,
        Cursor::new(data),
    )
}

fn append_archive_reader<W: Write, R: Read>(
    archive: &mut tar::Builder<W>,
    name: &str,
    size: u64,
    reader: R,
) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(size);
    header.set_mode(0o644);
    header.set_cksum();
    archive.append_data(&mut header, Path::new(name), reader)?;
    Ok(())
}

fn archive_entry_name<R: Read>(entry: &tar::Entry<'_, R>) -> Result<String> {
    Ok(entry.path()?.to_string_lossy().into_owned())
}

fn resize_to_max_width(img: DynamicImage, max_width: u32) -> DynamicImage {
    if max_width == 0 || img.width() <= max_width {
        return img;
    }

    let target_height = rounded_scale(img.height(), max_width, img.width());
    img.resize_exact(max_width, target_height, FilterType::Lanczos3)
}

fn rounded_scale(value: u32, numerator: u32, denominator: u32) -> u32 {
    let scaled = (u64::from(value) * u64::from(numerator) + (u64::from(denominator) / 2))
        / u64::from(denominator);
    u32::try_from(scaled).unwrap_or(u32::MAX).max(1)
}

fn tile_box(index: usize, width: u32, height: u32) -> TileBox {
    let grid = usize::try_from(COMIX_SCRAMBLE_GRID).expect("grid should fit usize");
    let column = index % grid;
    let row = index / grid;
    let x = rounded_tile_edge(column, width);
    let y = rounded_tile_edge(row, height);
    let right = rounded_tile_edge(column + 1, width);
    let bottom = rounded_tile_edge(row + 1, height);

    TileBox {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}

fn rounded_tile_edge(position: usize, dimension: u32) -> u32 {
    let position = u64::try_from(position).expect("tile position should fit u64");
    let scaled = (u64::from(dimension) * position + (u64::from(COMIX_SCRAMBLE_GRID) / 2))
        / u64::from(COMIX_SCRAMBLE_GRID);
    u32::try_from(scaled).unwrap_or(dimension)
}

struct TileBox {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}


const ARCHIVE_LOCK_WAIT: Duration = Duration::from_secs(30);
const ARCHIVE_LOCK_POLL: Duration = Duration::from_millis(50);

struct ArchiveLock {
    file: std::fs::File,
}

impl Drop for ArchiveLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn acquire_archive_lock(archive_path: &Path) -> Result<ArchiveLock> {
    let lock_path = archive_lock_path(archive_path);
    if let Some(parent) = lock_path.parent() {
        backend_fs::create_dir_all_sync(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("failed to open archive lock {}", lock_path.display()))?;
    let started_at = Instant::now();

    loop {
        match FileExt::try_lock(&file) {
            Ok(()) => return Ok(ArchiveLock { file }),
            Err(TryLockError::WouldBlock) => {
                if started_at.elapsed() >= ARCHIVE_LOCK_WAIT {
                    anyhow::bail!(
                        "timed out waiting for archive lock on {}",
                        archive_path.display()
                    );
                }
                std::thread::sleep(ARCHIVE_LOCK_POLL);
            }
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
    }
}

fn archive_lock_path(archive_path: &Path) -> PathBuf {
    archive_path.with_extension("tar.zst.lock")
}

fn create_archive_temp_output(path: &Path) -> Result<NamedTempFile> {
    if let Some(parent) = path.parent() {
        backend_fs::create_dir_all_sync(parent)?;
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("archive.tar.zst");
    tempfile::Builder::new()
        .prefix(&format!(".{name}."))
        .suffix(".tmp")
        .tempfile_in(parent)
        .with_context(|| format!("failed to create temp output for {}", path.display()))
}

fn persist_archive_temp(temp_file: NamedTempFile, path: &Path) -> Result<()> {
    temp_file
        .persist(path)
        .map(|_| ())
        .map_err(|error| error.error)
        .with_context(|| format!("failed to replace {}", path.display()))
}

fn supported_image_format_name(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Avif => Some("avif"),
        ImageFormat::Gif => Some("gif"),
        ImageFormat::Jpeg => Some("jpeg"),
        ImageFormat::Png => Some("png"),
        ImageFormat::WebP => Some("webp"),
        _ => None,
    }
}

fn is_image_entry(name: &str) -> bool {
    is_supported_image_content_type(content_type_for(name))
}

fn replace_with_avif_extension(name: &str) -> String {
    if let Some((base, _)) = name.rsplit_once('.') {
        format!("{base}.avif")
    } else {
        format!("{name}.avif")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comix_descrambler_restores_5x5_tile_order() {
        let original = synthetic_tile_image();
        let scrambled = scramble_like_comix(&original);
        let encoded = encode_png(&scrambled);

        let decoded = descramble_comix_5x5_to_png(&encoded).unwrap();
        let restored = image::load_from_memory(&decoded).unwrap().to_rgba8();

        assert_eq!(restored.as_raw(), original.as_raw());
    }

    fn synthetic_tile_image() -> image::RgbaImage {
        let tile = 4;
        let mut image =
            image::RgbaImage::new(COMIX_SCRAMBLE_GRID * tile, COMIX_SCRAMBLE_GRID * tile);
        for index in 0..25usize {
            let bounds = tile_box(index, image.width(), image.height());
            let color = image::Rgba([
                u8::try_from(index * 7).unwrap(),
                u8::try_from(index * 5).unwrap(),
                u8::try_from(index * 3).unwrap(),
                255,
            ]);
            for y in bounds.y..bounds.y + bounds.height {
                for x in bounds.x..bounds.x + bounds.width {
                    image.put_pixel(x, y, color);
                }
            }
        }
        image
    }

    fn scramble_like_comix(original: &image::RgbaImage) -> image::RgbaImage {
        let mut scrambled = image::RgbaImage::new(original.width(), original.height());
        for (destination_index, source_index) in COMIX_DESCRAMBLE_MAP.iter().copied().enumerate() {
            let destination = tile_box(destination_index, original.width(), original.height());
            let source = tile_box(source_index, original.width(), original.height());
            let tile = image::imageops::crop_imm(
                original,
                destination.x,
                destination.y,
                destination.width,
                destination.height,
            )
            .to_image();
            image::imageops::replace(
                &mut scrambled,
                &tile,
                i64::from(source.x),
                i64::from(source.y),
            );
        }
        scrambled
    }

    fn encode_png(image: &image::RgbaImage) -> Vec<u8> {
        let mut encoded = Vec::new();
        let cursor = Cursor::new(&mut encoded);
        let encoder = image::codecs::png::PngEncoder::new(cursor);
        encoder
            .write_image(
                image.as_raw(),
                image.width(),
                image.height(),
                ExtendedColorType::Rgba8,
            )
            .unwrap();
        encoded
    }
}

