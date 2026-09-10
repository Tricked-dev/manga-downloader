//! BBF chapter storage. All file/index work runs on the blocking pool.
//!
//! The download directory is owned by the server. Every writer must take the
//! same advisory lock; external tools must not modify archives behind the server.
//! A served page retains the mapping and shared lock until the last byte owner
//! is dropped, so a later section append cannot invalidate an in-flight response.
use anyhow::{Context, Result, bail, ensure};
use bytes::Bytes;
use libbbf::{FileBuilder, IndexedReader, MappedFile, MediaType};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    ops::Range,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PageVariant {
    Original,
    Upscaled,
}
impl PageVariant {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Upscaled => "upscaled",
        }
    }
}

#[derive(Debug)]
pub struct OriginalChapter {
    pub pages: Vec<PathBuf>,
    pub comicinfo_xml: String,
    pub cover: Option<PathBuf>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ChapterInfo {
    pub page_count: usize,
    pub has_upscaled: bool,
    pub asset_count: u64,
    pub file_size: u64,
    pub footer_hash: u64,
}
#[derive(Debug, Clone)]
pub struct StoredPage {
    pub bytes: Bytes,
    pub content_type: &'static str,
    pub variant: PageVariant,
    pub content_hash: String,
}
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("Page {page} is outside a chapter with {page_count} pages")]
    PageNotFound { page: usize, page_count: usize },
    #[error("This chapter has no {0} section")]
    VariantNotFound(&'static str),
}

struct LockedMapping {
    mapped: MappedFile,
    _lock: File,
    range: Range<usize>,
}
impl AsRef<[u8]> for LockedMapping {
    fn as_ref(&self) -> &[u8] {
        &self.mapped.as_bytes()[self.range.clone()]
    }
}
fn lock(path: &Path, exclusive: bool) -> Result<File> {
    let lock_path = path.with_extension("bbf.lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("open archive lock {}", lock_path.display()))?;
    if exclusive {
        file.lock()?;
    } else {
        file.lock_shared()?;
    }
    Ok(file)
}
fn map(path: &Path) -> Result<LockedMapping> {
    let lock = lock(path, false)?;
    // SAFETY: this server owns the archive. All mutation takes the exclusive
    // companion-file lock; the shared lock outlives the mapping, including
    // when Bytes retains it for an HTTP response. Renames replace inodes.
    let mapped = unsafe { MappedFile::open(path) }
        .with_context(|| format!("open BBF chapter {}", path.display()))?;
    Ok(LockedMapping {
        range: 0..mapped.len(),
        mapped,
        _lock: lock,
    })
}
fn validate_index(index: &IndexedReader<'_, &[u8]>) -> Result<()> {
    ensure!(index.verify_footer_hash()?, "BBF index checksum mismatch");
    Ok(())
}
fn section_range(
    index: &IndexedReader<'_, &[u8]>,
    variant: PageVariant,
) -> Result<Option<Range<u64>>> {
    let mut sections = Vec::new();
    for offset in 0..index.footer().section_count {
        let section = index.section(offset)?.context("missing BBF section")?;
        ensure!(
            section.start_index <= index.footer().page_count,
            "BBF section starts outside the page table"
        );
        let name = index
            .string(section.title_offset)?
            .context("invalid BBF section title")?;
        sections.push((name, section.start_index));
    }
    let Some(start) = sections
        .iter()
        .filter(|(name, _)| *name == variant.as_str())
        .map(|(_, start)| *start)
        .max()
    else {
        return Ok(None);
    };
    let end = sections
        .iter()
        .map(|(_, start)| *start)
        .filter(|next| *next > start)
        .min()
        .unwrap_or(index.footer().page_count);
    Ok(Some(start..end))
}
fn selected_range(
    index: &IndexedReader<'_, &[u8]>,
    variant: Option<PageVariant>,
) -> Result<(PageVariant, Range<u64>)> {
    let original = section_range(index, PageVariant::Original)?
        .ok_or(ReadError::VariantNotFound("original"))?;
    let upscaled = section_range(index, PageVariant::Upscaled)?;
    if let Some(upscaled) = &upscaled {
        ensure!(
            upscaled.end - upscaled.start == original.end - original.start,
            "BBF original and upscaled page counts differ"
        );
    }
    let selected = variant.unwrap_or(if upscaled.is_some() {
        PageVariant::Upscaled
    } else {
        PageVariant::Original
    });
    let range = match selected {
        PageVariant::Original => original,
        PageVariant::Upscaled => upscaled.ok_or(ReadError::VariantNotFound("upscaled"))?,
    };
    Ok((selected, range))
}

pub async fn write_originals(
    path: PathBuf,
    chapter: OriginalChapter,
    cancelled: impl Fn() -> bool + Send + 'static,
) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        ensure!(
            !chapter.pages.is_empty(),
            "A BBF chapter must have at least one page"
        );
        let parent = path.parent().context("BBF path has no parent")?;
        std::fs::create_dir_all(parent)?;
        let _lock = lock(&path, true)?;
        let temporary = tempfile::NamedTempFile::new_in(parent)?;
        let mut writer = FileBuilder::new(temporary.path())?;
        ensure!(
            writer.add_section("original", 0, None),
            "failed to create original section"
        );
        for page in chapter.pages {
            ensure!(!cancelled(), "BBF write canceled");
            let media = file_media_type(&page)?;
            writer.add_page_with_media_type(&page, media.as_u8(), 0, 0)?;
        }
        ensure!(
            writer.add_meta("ComicInfo.xml", &chapter.comicinfo_xml, None),
            "failed to add ComicInfo metadata"
        );
        if let Some(cover) = chapter.cover {
            let media = file_media_type(&cover)?;
            let asset = writer.add_asset_file(&cover, media.as_u8(), 0)?;
            ensure!(
                writer.add_meta("cover.asset_index", &asset.to_string(), None),
                "failed to add cover metadata"
            );
        }
        ensure!(!cancelled(), "BBF write canceled");
        writer.finalize()?;
        temporary.as_file().sync_all()?;
        ensure!(!cancelled(), "BBF write canceled");
        temporary.persist(&path).map_err(|error| error.error)?;
        Ok(())
    })
    .await
    .context("BBF writer task failed")?
}

pub async fn inspect(path: PathBuf) -> Result<ChapterInfo> {
    tokio::task::spawn_blocking(move || {
        let mapped = map(&path)?;
        let reader = mapped.mapped.reader();
        let index = reader.indexed()?;
        validate_index(&index)?;
        let (_, range) = selected_range(&index, None)?;
        Ok(ChapterInfo {
            page_count: usize::try_from(range.end - range.start)?,
            has_upscaled: section_range(&index, PageVariant::Upscaled)?.is_some(),
            asset_count: index.footer().asset_count,
            file_size: mapped.mapped.len() as u64,
            footer_hash: index.footer().footer_hash,
        })
    })
    .await
    .context("BBF inspection task failed")?
}

pub async fn read_page(
    path: PathBuf,
    page: usize,
    variant: Option<PageVariant>,
) -> Result<StoredPage> {
    tokio::task::spawn_blocking(move || {
        let mapped = map(&path)?;
        let reader = mapped.mapped.reader();
        let index = reader.indexed()?;
        validate_index(&index)?;
        let (variant, range) = selected_range(&index, variant)?;
        let page_count = usize::try_from(range.end - range.start)?;
        if page >= page_count {
            return Err(ReadError::PageNotFound { page, page_count }.into());
        }
        let entry = index
            .page(range.start + page as u64)?
            .context("missing BBF page")?;
        let asset = index
            .asset(entry.asset_index)?
            .context("missing BBF page asset")?;
        ensure!(
            index.verify_asset_hash(entry.asset_index)? == Some(true),
            "BBF asset checksum mismatch"
        );
        let data = index
            .asset_data(&asset)
            .context("BBF asset outside file bounds")?;
        let content_type = content_type(asset.media_type)?;
        let start = usize::try_from(asset.file_offset)?;
        let end = start
            .checked_add(data.len())
            .context("BBF asset range overflow")?;
        let content_hash = format!("{:016x}{:016x}", asset.hash_high, asset.hash_low);
        let mut mapped = mapped;
        mapped.range = start..end;
        Ok(StoredPage {
            bytes: Bytes::from_owner(mapped),
            content_type,
            variant,
            content_hash,
        })
    })
    .await
    .context("BBF page read task failed")?
}

pub async fn read_comicinfo(path: PathBuf) -> Result<Option<String>> {
    tokio::task::spawn_blocking(move || {
        let mapped = map(&path)?;
        let reader = mapped.mapped.reader();
        let index = reader.indexed()?;
        validate_index(&index)?;
        for entry in 0..index.footer().metadata_count {
            let metadata = index.metadata(entry)?.context("missing BBF metadata")?;
            if index.string(metadata.key_offset)? == Some("ComicInfo.xml") {
                // BBF permits arbitrary string-pool entries. The library's convenience
                // string() accessor caps scans at 2048 bytes, shorter than a synopsis.
                let footer = index.footer();
                ensure!(
                    metadata.value_offset < footer.string_pool_size,
                    "invalid ComicInfo string offset"
                );
                let start = usize::try_from(
                    footer
                        .string_pool_offset
                        .checked_add(metadata.value_offset)
                        .context("metadata offset overflow")?,
                )?;
                let end = usize::try_from(
                    footer
                        .string_pool_offset
                        .checked_add(footer.string_pool_size)
                        .context("string pool overflow")?,
                )?;
                let value = mapped
                    .mapped
                    .as_bytes()
                    .get(start..end)
                    .context("metadata outside file")?;
                let length = value
                    .iter()
                    .position(|byte| *byte == 0)
                    .context("unterminated ComicInfo metadata")?;
                return Ok(Some(std::str::from_utf8(&value[..length])?.to_owned()));
            }
        }
        Ok(None)
    })
    .await
    .context("BBF metadata read task failed")?
}

fn file_media_type(path: &Path) -> Result<MediaType> {
    let mut header = [0; 64];
    let length = File::open(path)?.read(&mut header)?;
    media_type(&header[..length])
}
fn media_type(bytes: &[u8]) -> Result<MediaType> {
    Ok(
        match image::guess_format(bytes).context("Unrecognized chapter image format")? {
            image::ImageFormat::Jpeg => MediaType::Jpg,
            image::ImageFormat::Png => MediaType::Png,
            image::ImageFormat::WebP => MediaType::Webp,
            image::ImageFormat::Avif => MediaType::Avif,
            image::ImageFormat::Gif => MediaType::Gif,
            format => bail!("Unsupported chapter image format: {format:?}"),
        },
    )
}
fn content_type(media: u8) -> Result<&'static str> {
    Ok(match MediaType::from_u8(media) {
        MediaType::Jpg => "image/jpeg",
        MediaType::Png => "image/png",
        MediaType::Webp => "image/webp",
        MediaType::Avif => "image/avif",
        MediaType::Gif => "image/gif",
        _ => bail!("Unsupported BBF page media type {media}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn encoded(format: image::ImageFormat, width: u32) -> Vec<u8> {
        let image = image::RgbImage::from_pixel(width, 12, image::Rgb([200, 160, 40]));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }
    #[tokio::test]
    async fn preserves_native_bytes_dimensions_order_and_long_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let png = encoded(image::ImageFormat::Png, 1600);
        let jpeg = encoded(image::ImageFormat::Jpeg, 1400);
        // File extensions deliberately disagree; the asset type comes from bytes.
        let first = directory.path().join("001.jpg");
        std::fs::write(&first, &png).unwrap();
        let second = directory.path().join("002.png");
        std::fs::write(&second, &jpeg).unwrap();
        let path = directory.path().join("chapter.bbf");
        let xml = format!(
            "<ComicInfo><Summary>{}</Summary></ComicInfo>",
            "Manga synopsis. ".repeat(250)
        );
        write_originals(
            path.clone(),
            OriginalChapter {
                pages: vec![first, second],
                comicinfo_xml: xml.clone(),
                cover: None,
            },
            || false,
        )
        .await
        .unwrap();
        let info = inspect(path.clone()).await.unwrap();
        assert_eq!(info.page_count, 2);
        assert!(!info.has_upscaled);
        let page = read_page(path.clone(), 0, None).await.unwrap();
        assert_eq!(&page.bytes[..], &png);
        assert_eq!(page.content_type, "image/png");
        assert_eq!(image::load_from_memory(&page.bytes).unwrap().width(), 1600);
        assert_eq!(
            &read_page(path.clone(), 1, None).await.unwrap().bytes[..],
            &jpeg
        );
        assert_eq!(read_comicinfo(path.clone()).await.unwrap(), Some(xml));
        assert!(
            read_page(path.clone(), 0, Some(PageVariant::Upscaled))
                .await
                .unwrap_err()
                .downcast_ref::<ReadError>()
                .is_some()
        );
        assert!(matches!(
            read_page(path, 2, None)
                .await
                .unwrap_err()
                .downcast_ref::<ReadError>(),
            Some(ReadError::PageNotFound { .. })
        ));
    }
    #[tokio::test]
    async fn chooses_upscaled_by_default_and_keeps_original_explicit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("two-sections.bbf");
        let png = encoded(image::ImageFormat::Png, 1024);
        let webp = encoded(image::ImageFormat::WebP, 2048);
        // Reader fixture only: production upscaling must use the upstream sealed-file append API.
        let mut fixture = libbbf::Builder::new();
        fixture.add_section("original", 0, None);
        fixture.add_page_bytes(&png, MediaType::Png.as_u8(), 0, 0);
        fixture.add_section("upscaled", 1, None);
        fixture.add_page_bytes(&webp, MediaType::Webp.as_u8(), 0, 0);
        fixture.write_to(&path).unwrap();
        assert!(inspect(path.clone()).await.unwrap().has_upscaled);
        let best = read_page(path.clone(), 0, None).await.unwrap();
        assert_eq!(best.variant, PageVariant::Upscaled);
        assert_eq!(best.content_type, "image/webp");
        assert_eq!(&best.bytes[..], &webp);
        assert_eq!(
            &read_page(path, 0, Some(PageVariant::Original))
                .await
                .unwrap()
                .bytes[..],
            &png
        );
    }
    #[tokio::test]
    async fn served_bytes_hold_read_lock_and_failed_write_preserves_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("chapter.bbf");
        let page_path = directory.path().join("page.png");
        std::fs::write(&page_path, encoded(image::ImageFormat::Png, 20)).unwrap();
        write_originals(
            path.clone(),
            OriginalChapter {
                pages: vec![page_path.clone()],
                comicinfo_xml: String::new(),
                cover: None,
            },
            || false,
        )
        .await
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        let page = read_page(path.clone(), 0, None).await.unwrap();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.with_extension("bbf.lock"))
            .unwrap();
        assert!(file.try_lock().is_err());
        let clone = page.bytes.clone();
        drop(page);
        assert!(file.try_lock().is_err());
        drop(clone);
        file.try_lock().unwrap();
        file.unlock().unwrap();
        assert!(
            write_originals(
                path.clone(),
                OriginalChapter {
                    pages: vec![page_path],
                    comicinfo_xml: String::new(),
                    cover: None
                },
                || true
            )
            .await
            .is_err()
        );
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    #[tokio::test]
    async fn rejects_corrupt_index() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bad.bbf");
        let mut fixture = libbbf::Builder::new();
        fixture.add_section("original", 0, None);
        fixture.add_page_bytes(
            &encoded(image::ImageFormat::Png, 20),
            MediaType::Png.as_u8(),
            0,
            0,
        );
        let mut bytes = fixture.build_bytes().unwrap();
        let reader = libbbf::Reader::from_data(bytes.as_slice());
        let offset = reader.footer().unwrap().page_offset as usize;
        bytes[offset] ^= 1;
        std::fs::write(&path, bytes).unwrap();
        assert!(inspect(path).await.is_err());
    }
}
