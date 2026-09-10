use anyhow::{Context, Result};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Component, Path};
use std::time::UNIX_EPOCH;

const MAX_IMAGE_BYTES: u64 = 80 * 1024 * 1024;
const MAX_XML_BYTES: u64 = 2 * 1024 * 1024;
const SUPPORTED_IMAGE_CONTENT_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/gif",
    "image/avif",
];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileDimension {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub name: String,
    pub file_position: usize,
}

#[derive(Debug, Clone)]
pub struct ArchiveInfo {
    pub pages: Vec<ArchiveEntry>,
    pub cover_entry: Option<String>,
    pub comic_info: ComicInfo,
    pub mtime_ms: i64,
    pub size_bytes: i64,
}

#[derive(Debug, Clone)]
pub struct EntryBytes {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComicInfo {
    pub title: Option<String>,
    pub series: Option<String>,
    pub summary: Option<String>,
    pub writer: Option<String>,
    pub artist: Option<String>,
    pub penciller: Option<String>,
    pub inker: Option<String>,
    pub letterer: Option<String>,
    pub editor: Option<String>,
    pub publisher: Option<String>,
    pub imprint: Option<String>,
    pub genre: Option<String>,
    pub age_rating: Option<String>,
    pub language_iso: Option<String>,
    pub web: Option<String>,
    pub page_count: Option<i64>,
    pub year: Option<i64>,
    pub month: Option<i64>,
    pub day: Option<i64>,
}

impl ComicInfo {
    #[must_use]
    /// Parses a ComicInfo.xml payload into known metadata fields.
    ///
    /// Unknown, malformed, or undecodable elements are ignored.
    pub fn parse(xml: &str) -> Self {
        quick_xml::de::from_str::<ComicInfoXml>(xml)
            .map(Into::into)
            .unwrap_or_default()
    }
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, rename_all = "PascalCase")]
struct ComicInfoXml {
    title: Option<String>,
    series: Option<String>,
    summary: Option<String>,
    writer: Option<String>,
    artist: Option<String>,
    penciller: Option<String>,
    inker: Option<String>,
    letterer: Option<String>,
    editor: Option<String>,
    publisher: Option<String>,
    imprint: Option<String>,
    genre: Option<String>,
    age_rating: Option<String>,
    #[serde(rename = "LanguageISO")]
    language_iso: Option<String>,
    web: Option<String>,
    page_count: Option<String>,
    year: Option<String>,
    month: Option<String>,
    day: Option<String>,
}

impl From<ComicInfoXml> for ComicInfo {
    fn from(xml: ComicInfoXml) -> Self {
        Self {
            title: non_empty(xml.title),
            series: non_empty(xml.series),
            summary: non_empty(xml.summary),
            writer: non_empty(xml.writer),
            artist: non_empty(xml.artist),
            penciller: non_empty(xml.penciller),
            inker: non_empty(xml.inker),
            letterer: non_empty(xml.letterer),
            editor: non_empty(xml.editor),
            publisher: non_empty(xml.publisher),
            imprint: non_empty(xml.imprint),
            genre: non_empty(xml.genre),
            age_rating: non_empty(xml.age_rating),
            language_iso: non_empty(xml.language_iso),
            web: non_empty(xml.web),
            page_count: parse_optional_i64(xml.page_count),
            year: parse_optional_i64(xml.year),
            month: parse_optional_i64(xml.month),
            day: parse_optional_i64(xml.day),
        }
    }
}

/// Inspects a zstd-compressed tar archive and returns its readable manga entries.
///
/// Unsafe entry names and non-image files are skipped. `ComicInfo.xml` is parsed
/// when present.
pub fn inspect_archive(path: &Path) -> Result<ArchiveInfo> {
    let metadata = backend_fs::metadata_sync(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    let mtime_ms = metadata
        .modified()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or_default();
    let size_bytes = i64::try_from(metadata.len()).unwrap_or(i64::MAX);

    let input = backend_fs::open_file(path)
        .with_context(|| format!("failed to open archive {}", path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(input)
        .with_context(|| format!("failed to decode archive {}", path.display()))?;
    let mut archive = tar::Archive::new(decoder);

    let mut pages = Vec::new();
    let mut cover_entry = None;
    let mut comic_info = ComicInfo::default();

    let mut file_position = 0usize;
    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let entry_position = file_position;
        file_position = file_position.saturating_add(1);
        let name = entry_name(&entry)?;
        if !is_safe_entry_name(&name) {
            continue;
        }
        let lower_name = name.to_ascii_lowercase();
        let file_name = Path::new(&lower_name)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");

        if file_name == "comicinfo.xml" {
            let xml = read_limited(&mut entry, MAX_XML_BYTES)?;
            let text = String::from_utf8_lossy(&xml);
            comic_info = ComicInfo::parse(&text);
            continue;
        }

        if is_image_name(&lower_name) {
            if file_name.starts_with("cover.") {
                cover_entry = Some(name);
                continue;
            }
            pages.push(ArchiveEntry {
                name,
                file_position: entry_position,
            });
        }
    }

    pages.sort_by(|left, right| natural_cmp(&left.name, &right.name));

    Ok(ArchiveInfo {
        pages,
        cover_entry,
        comic_info,
        mtime_ms,
        size_bytes,
    })
}

/// Extracts a page by its sorted page index.
pub fn extract_page(path: &Path, page: usize) -> Result<EntryBytes> {
    let info = inspect_archive(path)?;
    let entry = info
        .pages
        .get(page)
        .with_context(|| format!("page {page} out of bounds"))?;
    extract_named_image(path, &entry.name)
}

/// Extracts the explicit cover entry when the archive contains one.
pub fn extract_cover(path: &Path) -> Result<Option<EntryBytes>> {
    let info = inspect_archive(path)?;
    match info.cover_entry {
        Some(name) => extract_named_image(path, &name).map(Some),
        None => Ok(None),
    }
}

/// Extracts the cover entry or, if absent, the first image by natural sort order.
pub fn extract_cover_or_first_page(path: &Path) -> Result<Option<EntryBytes>> {
    let input = backend_fs::open_file(path)
        .with_context(|| format!("failed to open archive {}", path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(input)
        .with_context(|| format!("failed to decode archive {}", path.display()))?;
    let mut archive = tar::Archive::new(decoder);
    let mut first_page: Option<(String, EntryBytes)> = None;

    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        let name = entry_name(&entry)?;
        if !is_safe_entry_name(&name) || !is_image_name(&name) {
            continue;
        }

        let file_name = Path::new(&name)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let content_type = content_type_for(&name);
        if is_cover_file_name(file_name) {
            let bytes = read_limited(&mut entry, MAX_IMAGE_BYTES)?;
            return Ok(Some(EntryBytes {
                bytes,
                content_type,
            }));
        }

        let should_replace = first_page
            .as_ref()
            .is_none_or(|(first_name, _)| natural_cmp(&name, first_name) == Ordering::Less);
        if should_replace {
            let bytes = read_limited(&mut entry, MAX_IMAGE_BYTES)?;
            first_page = Some((
                name,
                EntryBytes {
                    bytes,
                    content_type,
                },
            ));
        }
    }

    Ok(first_page.map(|(_, entry)| entry))
}

/// Extracts one image entry by exact archive entry name.
pub fn extract_named_image(path: &Path, target_name: &str) -> Result<EntryBytes> {
    let input = backend_fs::open_file(path)
        .with_context(|| format!("failed to open archive {}", path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(input)
        .with_context(|| format!("failed to decode archive {}", path.display()))?;
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let name = entry_name(&entry)?;
        if name == target_name {
            let content_type = content_type_for(&name);
            let bytes = read_limited(&mut entry, MAX_IMAGE_BYTES)?;
            return Ok(EntryBytes {
                bytes,
                content_type,
            });
        }
    }

    anyhow::bail!("archive entry {target_name} not found")
}

/// Extracts image entries by exact archive entry name.
///
/// Output order matches `target_names`.
pub fn extract_named_images<S>(path: &Path, target_names: &[S]) -> Result<Vec<EntryBytes>>
where
    S: AsRef<str>,
{
    if target_names.is_empty() {
        return Ok(Vec::new());
    }
    if let [target_name] = target_names {
        return extract_named_image(path, target_name.as_ref()).map(|entry| vec![entry]);
    }

    let positions = target_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_ref(), index))
        .collect::<HashMap<_, _>>();
    let mut entries = vec![None; target_names.len()];
    let mut remaining = target_names.len();

    let input = backend_fs::open_file(path)
        .with_context(|| format!("failed to open archive {}", path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(input)
        .with_context(|| format!("failed to decode archive {}", path.display()))?;
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        if remaining == 0 {
            break;
        }

        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        let name = entry_name(&entry)?;
        let Some(&index) = positions.get(name.as_str()) else {
            continue;
        };
        if entries[index].is_some() {
            continue;
        }

        let content_type = content_type_for(&name);
        let bytes = read_limited(&mut entry, MAX_IMAGE_BYTES)?;
        entries[index] = Some(EntryBytes {
            bytes,
            content_type,
        });
        remaining -= 1;
    }

    entries
        .into_iter()
        .zip(target_names)
        .map(|(entry, name)| {
            entry.with_context(|| format!("archive entry {} not found", name.as_ref()))
        })
        .collect()
}

/// Extracts file entries by their raw archive file positions.
///
/// Output order matches `target_positions`.
pub fn extract_positioned_images(
    path: &Path,
    target_positions: &[usize],
) -> Result<Vec<EntryBytes>> {
    if target_positions.is_empty() {
        return Ok(Vec::new());
    }

    let mut positions = target_positions
        .iter()
        .copied()
        .enumerate()
        .map(|(index, position)| (position, index))
        .collect::<Vec<_>>();
    positions.sort_unstable_by_key(|(position, _)| *position);

    let input = backend_fs::open_file(path)
        .with_context(|| format!("failed to open archive {}", path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(input)
        .with_context(|| format!("failed to decode archive {}", path.display()))?;
    let mut archive = tar::Archive::new(decoder);

    let mut entries = vec![None; target_positions.len()];
    let mut next_target = 0usize;
    let mut file_position = 0usize;

    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        while next_target < positions.len() && positions[next_target].0 < file_position {
            next_target += 1;
        }
        if next_target >= positions.len() {
            break;
        }

        if positions[next_target].0 == file_position {
            let name = entry_name(&entry)?;
            let content_type = content_type_for(&name);
            let bytes = read_limited(&mut entry, MAX_IMAGE_BYTES)?;

            while next_target < positions.len() && positions[next_target].0 == file_position {
                let output_index = positions[next_target].1;
                entries[output_index] = Some(EntryBytes {
                    bytes: bytes.clone(),
                    content_type,
                });
                next_target += 1;
            }
        }

        file_position = file_position.saturating_add(1);
    }

    entries
        .into_iter()
        .zip(target_positions)
        .map(|(entry, position)| {
            entry.with_context(|| format!("archive file position {position} not found"))
        })
        .collect()
}

/// Extracts file entries by raw archive position while using caller-provided content types.
///
/// This avoids deriving content types during indexed archive reads.
pub fn extract_positioned_images_with_content_types(
    path: &Path,
    target_positions: &[(usize, &'static str)],
) -> Result<Vec<EntryBytes>> {
    if target_positions.is_empty() {
        return Ok(Vec::new());
    }

    let mut positions = target_positions
        .iter()
        .copied()
        .enumerate()
        .map(|(index, (position, content_type))| (position, index, content_type))
        .collect::<Vec<_>>();
    positions.sort_unstable_by_key(|(position, _, _)| *position);

    let input = backend_fs::open_file(path)
        .with_context(|| format!("failed to open archive {}", path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(input)
        .with_context(|| format!("failed to decode archive {}", path.display()))?;
    let mut archive = tar::Archive::new(decoder);

    let mut entries = vec![None; target_positions.len()];
    let mut next_target = 0usize;
    let mut file_position = 0usize;

    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        while next_target < positions.len() && positions[next_target].0 < file_position {
            next_target += 1;
        }
        if next_target >= positions.len() {
            break;
        }

        if positions[next_target].0 == file_position {
            let bytes = read_limited(&mut entry, MAX_IMAGE_BYTES)?;

            while next_target < positions.len() && positions[next_target].0 == file_position {
                let output_index = positions[next_target].1;
                let content_type = positions[next_target].2;
                entries[output_index] = Some(EntryBytes {
                    bytes: bytes.clone(),
                    content_type,
                });
                next_target += 1;
            }
        }

        file_position = file_position.saturating_add(1);
    }

    entries
        .into_iter()
        .zip(target_positions)
        .map(|(entry, (position, _))| {
            entry.with_context(|| format!("archive file position {position} not found"))
        })
        .collect()
}

/// Reads image dimensions from encoded image bytes without decoding full pixels.
pub fn image_dimensions(bytes: &[u8]) -> Result<FileDimension> {
    let reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let (width, height) = reader.into_dimensions()?;
    Ok(FileDimension { width, height })
}

/// Reads dimensions for named image entries in an archive.
///
/// Output order matches `names`.
pub fn image_dimensions_for_entries(path: &Path, names: &[String]) -> Result<Vec<FileDimension>> {
    if names.is_empty() {
        return Ok(Vec::new());
    }

    let positions = names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut dimensions = vec![None; names.len()];
    let mut remaining = names.len();

    let input = backend_fs::open_file(path)
        .with_context(|| format!("failed to open archive {}", path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(input)
        .with_context(|| format!("failed to decode archive {}", path.display()))?;
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        if remaining == 0 {
            break;
        }

        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        let name = entry_name(&entry)?;
        let Some(&index) = positions.get(name.as_str()) else {
            continue;
        };
        if dimensions[index].is_some() {
            continue;
        }

        let bytes = read_limited(&mut entry, MAX_IMAGE_BYTES)?;
        dimensions[index] = Some(image_dimensions(&bytes)?);
        remaining -= 1;
    }

    dimensions
        .into_iter()
        .zip(names)
        .map(|(dimension, name)| {
            dimension.with_context(|| format!("archive entry {name} not found"))
        })
        .collect()
}

/// Generates a bounded JPEG thumbnail from encoded image bytes.
pub fn thumbnail_jpeg(bytes: &[u8]) -> Result<Vec<u8>> {
    let image = image::load_from_memory(bytes)?;
    let thumb = image.thumbnail(320, 480);
    let mut output = Cursor::new(Vec::new());
    thumb.write_to(&mut output, image::ImageFormat::Jpeg)?;
    Ok(output.into_inner())
}

#[must_use]
/// Returns the HTTP content type implied by an archive entry name.
pub fn content_type_for(name: &str) -> &'static str {
    mime_guess::from_path(name)
        .first_raw()
        .unwrap_or("application/octet-stream")
}

#[must_use]
/// Returns whether a content type is one of the image formats this crate handles in archives.
pub fn is_supported_image_content_type(content_type: &str) -> bool {
    SUPPORTED_IMAGE_CONTENT_TYPES.contains(&content_type)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_optional_i64(value: Option<String>) -> Option<i64> {
    value.as_deref().map(str::trim)?.parse().ok()
}

fn read_limited<R: Read>(reader: &mut R, limit: u64) -> Result<Vec<u8>> {
    let mut limited = reader.take(limit + 1);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        anyhow::bail!("archive entry exceeds decompression limit");
    }
    Ok(bytes)
}

fn entry_name<R: Read>(entry: &tar::Entry<'_, R>) -> Result<String> {
    entry
        .path()?
        .to_str()
        .map(ToOwned::to_owned)
        .context("archive entry path is not valid utf-8")
}

fn is_safe_entry_name(name: &str) -> bool {
    Path::new(name)
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn is_image_name(name: &str) -> bool {
    let content_type = content_type_for(name);
    is_supported_image_content_type(content_type)
}

fn is_cover_file_name(file_name: &str) -> bool {
    file_name
        .get(..6)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("cover."))
}

fn natural_cmp(left: &str, right: &str) -> Ordering {
    natord::compare(left, right)
}
