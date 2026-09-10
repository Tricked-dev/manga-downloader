//! Reading and writing CBZ/ZIP archives of manga pages.
//!
//! Two properties matter more than speed here:
//!
//! * **Ordering and structure survive.** Readers present pages in archive
//!   order, so entries are written back in exactly the order they were read,
//!   with their original names and directory prefixes.
//! * **Nothing is re-encoded needlessly.** A processed page is encoded to
//!   bytes once and those bytes are stored in the archive verbatim. PNG and
//!   WebP payloads are already compressed, so they go in with
//!   `CompressionMethod::Stored`; running deflate over them again costs time
//!   and gains nothing. Entries we do not process are copied through byte for
//!   byte without ever being decoded.

use std::io::{Read, Seek, Write};
use std::path::Path;

use anyhow::{Context, Result};

/// Image extensions we will attempt to upscale. Anything else is passed
/// through untouched -- ComicInfo.xml, covers in unusual formats, and so on.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "avif"];

/// Whether a name looks like a page we should process.
pub fn is_image_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            let lowered = extension.to_ascii_lowercase();
            IMAGE_EXTENSIONS.contains(&lowered.as_str())
        })
        .unwrap_or(false)
}

/// One entry read out of an archive, in archive order.
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    /// Full path within the archive, directory prefix included.
    pub name: String,
    pub data: Vec<u8>,
    /// Whether the name looks like a page to upscale.
    pub is_image: bool,
}

/// Read every file entry from a ZIP/CBZ, preserving order.
///
/// Directory entries are dropped: they are recreated implicitly by the paths
/// of the files inside them, and carrying them adds nothing.
pub fn read_archive<R: Read + Seek>(reader: R) -> Result<Vec<ArchiveEntry>> {
    let mut archive = zip::ZipArchive::new(reader).context("failed to open the archive")?;
    let mut entries = Vec::with_capacity(archive.len());

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .with_context(|| format!("failed to read archive entry {index}"))?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_owned();
        let mut data = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut data)
            .with_context(|| format!("failed to read `{name}` from the archive"))?;
        let is_image = is_image_name(&name);
        entries.push(ArchiveEntry {
            name,
            data,
            is_image,
        });
    }

    Ok(entries)
}

/// What the caller wants done with one archive entry.
pub enum Processed {
    /// Copy the entry through unchanged.
    Passthrough,
    /// Replace it. The name may change too, because upscaling a `.jpg` page
    /// and re-encoding it as PNG has to rename the entry to match.
    Replaced { name: String, data: Vec<u8> },
}

/// Convert an archive entry by entry, without holding it all in memory.
///
/// A 200-page CBZ upscaled 4x is several gigabytes of output; reading every
/// entry up front and writing at the end would need all of it resident at
/// once. This reads one entry, hands it to `process`, writes the result, and
/// moves on, so peak memory is one page rather than a whole volume.
///
/// Order is preserved exactly, which is what page order in a reader depends
/// on.
pub fn convert_archive<R, W, F>(reader: R, writer: W, mut process: F) -> Result<ConvertStats>
where
    R: Read + Seek,
    W: Write + Seek,
    F: FnMut(&str, &[u8]) -> Result<Processed>,
{
    let mut archive = zip::ZipArchive::new(reader).context("failed to open the archive")?;
    let mut zip = zip::ZipWriter::new(writer);
    let mut stats = ConvertStats::default();

    for index in 0..archive.len() {
        let (name, data, is_image) = {
            let mut file = archive
                .by_index(index)
                .with_context(|| format!("failed to read archive entry {index}"))?;
            if file.is_dir() {
                continue;
            }
            let name = file.name().to_owned();
            let mut data = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut data)
                .with_context(|| format!("failed to read `{name}` from the archive"))?;
            let is_image = is_image_name(&name);
            (name, data, is_image)
        };

        let (out_name, out_data, store_uncompressed) = match process(&name, &data)? {
            Processed::Passthrough => {
                stats.passed_through += 1;
                (name, data, is_image)
            }
            Processed::Replaced {
                name: new_name,
                data: new_data,
            } => {
                stats.processed += 1;
                (new_name, new_data, true)
            }
        };

        write_entry(&mut zip, &out_name, &out_data, store_uncompressed)?;
    }

    zip.finish().context("failed to finalise the archive")?;
    Ok(stats)
}

/// Counts from a [`convert_archive`] run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ConvertStats {
    pub processed: usize,
    pub passed_through: usize,
}

fn write_entry<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    name: &str,
    data: &[u8],
    store_uncompressed: bool,
) -> Result<()> {
    // Image payloads are already compressed formats; deflating them again
    // burns CPU for a fraction of a percent. Everything else (ComicInfo.xml
    // and friends) is text-like and compresses well.
    let method = if store_uncompressed {
        zip::CompressionMethod::Stored
    } else {
        zip::CompressionMethod::Deflated
    };
    let options = zip::write::SimpleFileOptions::default().compression_method(method);

    zip.start_file(name, options)
        .with_context(|| format!("failed to start archive entry `{name}`"))?;
    zip.write_all(data)
        .with_context(|| format!("failed to write archive entry `{name}`"))?;
    Ok(())
}

/// Write entries into a ZIP/CBZ in the given order.
pub fn write_archive<W: Write + Seek>(writer: W, entries: &[ArchiveEntry]) -> Result<()> {
    let mut zip = zip::ZipWriter::new(writer);

    for entry in entries {
        // Image payloads are already compressed formats; deflating them again
        // burns CPU for a fraction of a percent. Everything else (ComicInfo.xml
        // and friends) is text-like and compresses well.
        let method = if entry.is_image {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        let options = zip::write::SimpleFileOptions::default().compression_method(method);

        zip.start_file(&entry.name, options)
            .with_context(|| format!("failed to start archive entry `{}`", entry.name))?;
        zip.write_all(&entry.data)
            .with_context(|| format!("failed to write archive entry `{}`", entry.name))?;
    }

    zip.finish().context("failed to finalise the archive")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn image_names_are_recognised_case_insensitively() {
        assert!(is_image_name("page01.png"));
        assert!(is_image_name("Page01.PNG"));
        assert!(is_image_name("chapter 1/page01.jpeg"));
        assert!(is_image_name("a.WebP"));
        assert!(!is_image_name("ComicInfo.xml"));
        assert!(!is_image_name("README"));
        assert!(!is_image_name("thumbs.db"));
    }

    fn sample_entries() -> Vec<ArchiveEntry> {
        vec![
            ArchiveEntry {
                name: "ComicInfo.xml".to_owned(),
                data: b"<ComicInfo/>".to_vec(),
                is_image: false,
            },
            ArchiveEntry {
                name: "chapter 1/002.png".to_owned(),
                data: vec![0x89, b'P', b'N', b'G', 1, 2, 3],
                is_image: true,
            },
            ArchiveEntry {
                name: "chapter 1/001.png".to_owned(),
                data: vec![0x89, b'P', b'N', b'G', 4, 5, 6],
                is_image: true,
            },
        ]
    }

    #[test]
    fn a_round_trip_preserves_order_names_and_bytes() {
        let entries = sample_entries();
        let mut buffer = Vec::new();
        write_archive(Cursor::new(&mut buffer), &entries).unwrap();

        let read_back = read_archive(Cursor::new(&buffer)).unwrap();
        assert_eq!(read_back.len(), entries.len());
        for (original, restored) in entries.iter().zip(&read_back) {
            // Order is archive order, not sorted order: "002" stays before
            // "001" because that is how it was written.
            assert_eq!(original.name, restored.name);
            assert_eq!(original.data, restored.data);
            assert_eq!(original.is_image, restored.is_image);
        }
    }

    #[test]
    fn directory_prefixes_survive() {
        let entries = sample_entries();
        let mut buffer = Vec::new();
        write_archive(Cursor::new(&mut buffer), &entries).unwrap();
        let read_back = read_archive(Cursor::new(&buffer)).unwrap();
        assert!(read_back.iter().any(|e| e.name == "chapter 1/001.png"));
    }

    #[test]
    fn streaming_conversion_preserves_order_and_renames() {
        let entries = sample_entries();
        let mut input = Vec::new();
        write_archive(Cursor::new(&mut input), &entries).unwrap();

        let mut output = Vec::new();
        let stats = convert_archive(
            Cursor::new(&input),
            Cursor::new(&mut output),
            |name, data| {
                if !is_image_name(name) {
                    return Ok(Processed::Passthrough);
                }
                let stem = Path::new(name).with_extension("webp");
                let mut replaced = data.to_vec();
                replaced.push(0xFF);
                Ok(Processed::Replaced {
                    name: stem.to_string_lossy().into_owned(),
                    data: replaced,
                })
            },
        )
        .unwrap();

        assert_eq!(stats.processed, 2);
        assert_eq!(stats.passed_through, 1);

        let read_back = read_archive(Cursor::new(&output)).unwrap();
        let names: Vec<&str> = read_back.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["ComicInfo.xml", "chapter 1/002.webp", "chapter 1/001.webp"]
        );
        assert_eq!(read_back[0].data, b"<ComicInfo/>");
        assert_eq!(*read_back[1].data.last().unwrap(), 0xFF);
    }

    #[test]
    fn non_image_entries_are_carried_through() {
        let entries = sample_entries();
        let mut buffer = Vec::new();
        write_archive(Cursor::new(&mut buffer), &entries).unwrap();
        let read_back = read_archive(Cursor::new(&buffer)).unwrap();
        let comic_info = read_back
            .iter()
            .find(|e| e.name == "ComicInfo.xml")
            .expect("ComicInfo.xml missing");
        assert!(!comic_info.is_image);
        assert_eq!(comic_info.data, b"<ComicInfo/>");
    }
}
