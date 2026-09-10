// This crate is only consumed inside the workspace; requiring rustdoc `# Errors`
// sections on every helper adds noise without improving call sites.
#![allow(clippy::missing_errors_doc)]

use anyhow::Result;
use byte_unit::Byte;
use jiff::{Timestamp, civil::DateTime, tz::TimeZone};
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use std::path::{Path, PathBuf};

pub mod settings;

pub const DEFAULT_LANGUAGE_ISO: &str = "en";
const APPLICATION_JSON_CONTENT_TYPE: &str = "application/json";
const APPLICATION_XHTML_XML_CONTENT_TYPE: &str = "application/xhtml+xml";
const APPLICATION_XML_CONTENT_TYPE: &str = "application/xml";
const IMAGE_AVIF_CONTENT_TYPE: &str = "image/avif";

#[must_use]
/// Sanitizes a path component using the platform-specific filename rules.
pub fn sanitize_filename(name: &str) -> String {
    sanitize_filename::sanitize_with_options(
        name,
        sanitize_filename::Options {
            windows: cfg!(windows),
            truncate: false,
            replacement: "_",
        },
    )
}

#[must_use]
/// Returns whether `name` is already a safe single filename component.
pub fn is_safe_filename_component(name: &str) -> bool {
    !name.is_empty()
        && sanitize_filename(name) == name
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
}

#[must_use]
/// Sanitizes an arbitrary value into a stable ASCII filename segment.
pub fn sanitize_filename_segment(value: &str, fallback: &str) -> String {
    let mut segment = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    while segment.contains("..") {
        segment = segment.replace("..", ".");
    }

    let segment = sanitize_filename(&segment);
    let segment = segment.trim_matches(['.', '-', '_']);
    if segment.is_empty() {
        fallback.to_string()
    } else {
        segment.to_string()
    }
}

/// Parses a human-readable byte size.
///
/// Blank values return `None`; non-blank invalid values return an error.
pub fn parse_byte_size(value: &str) -> Result<Option<u64>> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Ok(None);
    }

    Ok(Some(
        Byte::parse_str(normalized, true)
            .map_err(|_| anyhow::anyhow!("invalid byte size value: {normalized}"))?
            .as_u64(),
    ))
}

/// Parses a human-readable byte size, treating zero as unset.
pub fn parse_positive_byte_size(value: &str) -> Result<Option<u64>> {
    Ok(parse_byte_size(value)?.filter(|amount| *amount > 0))
}

#[must_use]
/// Returns the media type portion before any content-type parameters.
pub fn primary_content_type(content_type: &str) -> &str {
    content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
}

#[must_use]
/// Compares content types while ignoring parameters and ASCII case.
pub fn content_type_matches(content_type: &str, expected: &str) -> bool {
    primary_content_type(content_type).eq_ignore_ascii_case(expected)
}

#[must_use]
/// Returns whether a content type is AVIF, ignoring parameters and ASCII case.
pub fn is_avif_content_type(content_type: &str) -> bool {
    content_type_matches(content_type, IMAGE_AVIF_CONTENT_TYPE)
}

#[must_use]
/// Returns whether a content type should be treated as textual.
pub fn is_textual_content_type(content_type: &str) -> bool {
    let content_type = primary_content_type(content_type).to_ascii_lowercase();
    content_type.starts_with("text/")
        || content_type == APPLICATION_JSON_CONTENT_TYPE
        || content_type == APPLICATION_XHTML_XML_CONTENT_TYPE
        || content_type == APPLICATION_XML_CONTENT_TYPE
        || content_type.ends_with("+json")
        || content_type.ends_with("+xml")
}

#[must_use]
/// Returns the archive path for a downloaded manga chapter.
pub fn download_archive_path(
    download_path: &str,
    source: &str,
    manga_title: &str,
    chapter_number: f64,
) -> PathBuf {
    Path::new(download_path)
        .join(sanitize_filename(source))
        .join(sanitize_filename(manga_title))
        .join(download_archive_filename(chapter_number))
}

#[must_use]
/// Formats a chapter number as the canonical `.bbf` archive filename.
pub fn download_archive_filename(chapter_number: f64) -> String {
    format!(
        "{}.bbf",
        sanitize_filename(&format_chapter_number(chapter_number))
    )
}

/// Recursively collects downloaded chapter archive files under `root`.
pub fn collect_download_archive_files(root: &Path) -> Result<Vec<PathBuf>> {
    backend_fs::collect_files_with_suffix(root, ".bbf")
}

#[must_use]
/// Chooses how many archive files should be re-encoded concurrently.
pub fn reencode_file_parallelism(file_count: usize, available_workers: usize) -> usize {
    if file_count == 0 {
        return 1;
    }

    if available_workers < 4 {
        1
    } else {
        2.min(file_count).min(available_workers)
    }
}

#[must_use]
/// Chooses the per-file worker count for a re-encode batch.
pub fn reencode_per_file_workers(file_parallelism: usize, available_workers: usize) -> usize {
    (available_workers / file_parallelism.max(1)).max(1)
}

pub struct ComicInfoChapterMetadata<'a> {
    pub title: &'a str,
    pub number: f64,
    pub count: Option<usize>,
    pub date_uploaded: &'a str,
    pub page_count: usize,
    pub age_rating: Option<&'a str>,
    pub source_url: Option<&'a str>,
    pub language: Option<&'a str>,
    pub credits: ComicInfoCredits<'a>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ComicInfoCredits<'a> {
    pub writer: Option<&'a str>,
    pub artist: Option<&'a str>,
    pub penciller: Option<&'a str>,
    pub inker: Option<&'a str>,
    pub letterer: Option<&'a str>,
    pub editor: Option<&'a str>,
    pub publisher: Option<&'a str>,
    pub imprint: Option<&'a str>,
}

#[must_use]
/// Builds a ComicInfo.xml document for one chapter archive.
pub fn build_comicinfo_xml(
    manga_title: &str,
    manga_description: &str,
    manga_author: &str,
    manga_genres: &str,
    chapter: &ComicInfoChapterMetadata<'_>,
) -> String {
    let release_date = parse_chapter_release_date(chapter.date_uploaded);
    let mut writer = new_xml_writer();
    write_event(
        &mut writer,
        Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)),
    );
    write_event(&mut writer, Event::Start(BytesStart::new("ComicInfo")));

    write_text_element(&mut writer, "Title", chapter.title);
    write_text_element(&mut writer, "Series", manga_title);
    write_text_element(
        &mut writer,
        "Number",
        &format_chapter_number(chapter.number),
    );
    if let Some(count) = chapter.count {
        write_text_element(&mut writer, "Count", &count.to_string());
    }
    write_text_element(&mut writer, "PageCount", &chapter.page_count.to_string());
    write_text_element(&mut writer, "Format", "Digital");
    write_text_element(&mut writer, "Manga", "YesAndRightToLeft");

    if let Some(language) = chapter.language {
        write_text_element(&mut writer, "LanguageISO", language);
    }

    let summary = normalize_comicinfo_summary(manga_description);
    if !summary.is_empty() {
        write_text_element(&mut writer, "Summary", &summary);
    }

    write_optional_text_element(
        &mut writer,
        "Writer",
        chapter
            .credits
            .writer
            .or_else(|| (!manga_author.is_empty()).then_some(manga_author)),
    );
    write_optional_text_element(&mut writer, "Artist", chapter.credits.artist);
    write_optional_text_element(&mut writer, "Penciller", chapter.credits.penciller);
    write_optional_text_element(&mut writer, "Inker", chapter.credits.inker);
    write_optional_text_element(&mut writer, "Letterer", chapter.credits.letterer);
    write_optional_text_element(&mut writer, "Editor", chapter.credits.editor);
    write_optional_text_element(&mut writer, "Publisher", chapter.credits.publisher);
    write_optional_text_element(&mut writer, "Imprint", chapter.credits.imprint);

    if !manga_genres.trim().is_empty() {
        write_text_element(&mut writer, "Genre", manga_genres.trim());
    }

    if let Some(date) = release_date {
        write_text_element(&mut writer, "Year", &date.year().to_string());
        write_text_element(&mut writer, "Month", &date.month().to_string());
        write_text_element(&mut writer, "Day", &date.day().to_string());
    }

    if let Some(rating) = chapter.age_rating {
        write_text_element(&mut writer, "AgeRating", rating);
    }

    write_event(&mut writer, Event::Start(BytesStart::new("Pages")));
    let mut page = BytesStart::new("Page");
    page.push_attribute(("Image", "0"));
    page.push_attribute(("Type", "FrontCover"));
    write_event(&mut writer, Event::Empty(page));
    write_event(&mut writer, Event::End(BytesEnd::new("Pages")));

    if let Some(url) = chapter.source_url {
        write_text_element(&mut writer, "Web", url);
    }
    write_text_element(&mut writer, "Notes", "Generated by Manga Downloader");
    write_event(&mut writer, Event::End(BytesEnd::new("ComicInfo")));

    finish_xml_writer(writer)
}

#[must_use]
/// Builds a series-level ComicInfo.xml document.
pub fn build_series_comicinfo_xml(
    manga_title: &str,
    manga_description: &str,
    manga_author: &str,
    manga_genres: &str,
    chapter_count: usize,
    credits: ComicInfoCredits<'_>,
) -> String {
    let mut writer = new_xml_writer();
    write_event(
        &mut writer,
        Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)),
    );
    write_event(&mut writer, Event::Start(BytesStart::new("ComicInfo")));

    write_text_element(&mut writer, "Title", manga_title);
    write_text_element(&mut writer, "Series", manga_title);
    write_text_element(&mut writer, "Count", &chapter_count.to_string());
    write_text_element(&mut writer, "Format", "Digital");
    write_text_element(&mut writer, "Manga", "YesAndRightToLeft");

    let summary = normalize_comicinfo_summary(manga_description);
    if !summary.is_empty() {
        write_text_element(&mut writer, "Summary", &summary);
    }

    write_optional_text_element(
        &mut writer,
        "Writer",
        credits
            .writer
            .or_else(|| (!manga_author.is_empty()).then_some(manga_author)),
    );
    write_optional_text_element(&mut writer, "Artist", credits.artist);
    write_optional_text_element(&mut writer, "Penciller", credits.penciller);
    write_optional_text_element(&mut writer, "Inker", credits.inker);
    write_optional_text_element(&mut writer, "Letterer", credits.letterer);
    write_optional_text_element(&mut writer, "Editor", credits.editor);
    write_optional_text_element(&mut writer, "Publisher", credits.publisher);
    write_optional_text_element(&mut writer, "Imprint", credits.imprint);

    if !manga_genres.trim().is_empty() {
        write_text_element(&mut writer, "Genre", manga_genres.trim());
    }

    write_text_element(&mut writer, "Notes", "Generated by Manga Downloader");
    write_event(&mut writer, Event::End(BytesEnd::new("ComicInfo")));

    finish_xml_writer(writer)
}

fn new_xml_writer() -> Writer<Vec<u8>> {
    Writer::new_with_indent(Vec::new(), b' ', 2)
}

fn write_event(writer: &mut Writer<Vec<u8>>, event: Event<'_>) {
    writer
        .write_event(event)
        .expect("writing ComicInfo XML to memory should not fail");
}

fn write_text_element(writer: &mut Writer<Vec<u8>>, name: &str, value: &str) {
    write_event(writer, Event::Start(BytesStart::new(name)));
    write_event(writer, Event::Text(BytesText::new(value)));
    write_event(writer, Event::End(BytesEnd::new(name)));
}

fn write_optional_text_element(writer: &mut Writer<Vec<u8>>, name: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        write_text_element(writer, name, value);
    }
}

fn finish_xml_writer(writer: Writer<Vec<u8>>) -> String {
    String::from_utf8(writer.into_inner()).expect("ComicInfo XML must be valid UTF-8")
}

fn normalize_comicinfo_summary(value: &str) -> String {
    let text = value.replace("\r\n", "\n").replace('\r', "\n");
    let mut normalized_lines = Vec::new();

    for raw_line in text.lines() {
        let mut line = replace_markdown_links(raw_line.trim());
        line = strip_markdown_formatting(&line);
        let line = clean_summary_line(&line);

        if line.is_empty() {
            if normalized_lines
                .last()
                .is_none_or(|previous: &String| !previous.is_empty())
            {
                normalized_lines.push(String::new());
            }
            continue;
        }

        normalized_lines.push(line);
    }

    while normalized_lines.first().is_some_and(String::is_empty) {
        normalized_lines.remove(0);
    }
    while normalized_lines.last().is_some_and(String::is_empty) {
        normalized_lines.pop();
    }

    let mut collapsed = Vec::new();
    let mut previous_blank = false;
    for line in normalized_lines {
        if line.is_empty() {
            if !previous_blank {
                collapsed.push(line);
            }
            previous_blank = true;
        } else {
            previous_blank = false;
            collapsed.push(line);
        }
    }

    collapsed.join("\n")
}

fn replace_markdown_links(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;

    while let Some(open_text) = rest.find('[') {
        result.push_str(&rest[..open_text]);
        let after_open = &rest[open_text + 1..];
        let Some(close_text) = after_open.find(']') else {
            result.push_str(&rest[open_text..]);
            return result;
        };
        let text = &after_open[..close_text];
        let after_text = &after_open[close_text + 1..];

        if let Some(after_paren) = after_text.strip_prefix('(')
            && let Some(close_url) = after_paren.find(')')
        {
            let url = &after_paren[..close_url];
            result.push_str(text.trim());
            if !url.trim().is_empty() {
                result.push_str(" (");
                result.push_str(url.trim());
                result.push(')');
            }
            rest = &after_paren[close_url + 1..];
            continue;
        }

        result.push('[');
        result.push_str(text);
        result.push(']');
        rest = after_text;
    }

    result.push_str(rest);
    result
}

fn strip_markdown_formatting(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut previous_was_space = false;

    for ch in value.chars() {
        if matches!(ch, '*' | '`') {
            continue;
        }

        if ch.is_whitespace() {
            if !previous_was_space {
                result.push(' ');
            }
            previous_was_space = true;
        } else {
            previous_was_space = false;
            result.push(ch);
        }
    }

    result.trim().to_string()
}

fn clean_summary_line(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if trimmed
        .chars()
        .all(|ch| matches!(ch, '-' | '*' | '_' | '~'))
    {
        return String::new();
    }

    let trimmed = trimmed.trim_start_matches(['#', '>']).trim();
    let trimmed = trimmed.trim_start_matches(['-', '*', '+', '•']).trim();

    let mut result = String::with_capacity(trimmed.len());
    let mut previous_was_space = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !previous_was_space {
                result.push(' ');
            }
            previous_was_space = true;
        } else {
            previous_was_space = false;
            result.push(ch);
        }
    }

    result.trim().to_string()
}

#[must_use]
/// Formats a chapter number without a decimal suffix when it is an integer.
pub fn format_chapter_number(chapter_number: f64) -> String {
    if chapter_number.fract() == 0.0 {
        format!("{chapter_number:.0}")
    } else {
        chapter_number.to_string()
    }
}

fn parse_chapter_release_date(value: &str) -> Option<DateTime> {
    let trimmed = value.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(timestamp) = trimmed.parse::<i64>() {
        let parsed = if trimmed.len() > 10 {
            Timestamp::from_millisecond(timestamp)
        } else {
            Timestamp::from_second(timestamp)
        };
        if let Ok(timestamp) = parsed {
            return Some(utc_datetime(timestamp));
        }
    }

    if let Ok(timestamp) = trimmed.parse::<Timestamp>() {
        return Some(utc_datetime(timestamp));
    }

    if let Ok(date) = jiff::civil::Date::strptime("%Y-%m-%d", trimmed) {
        return Some(DateTime::from(date));
    }

    if let Ok(date) = DateTime::strptime("%Y-%m-%d %H:%M:%S", trimmed) {
        return Some(date);
    }

    None
}

fn utc_datetime(timestamp: Timestamp) -> DateTime {
    timestamp.to_zoned(TimeZone::UTC).datetime()
}
