use std::collections::{BTreeMap, HashMap};

use jiff::{Timestamp, civil::DateTime, tz::TimeZone};

use super::SETTINGS_LIST;

pub(super) fn library_matches_query(
    manga: &backend_persistence::MangaRow,
    normalized_query: &str,
) -> bool {
    manga.title.to_lowercase().contains(normalized_query)
        || manga.source.to_lowercase().contains(normalized_query)
        || manga.category.to_lowercase().contains(normalized_query)
}

fn setting_value<'a>(
    settings: &'a HashMap<String, String>,
    key: &str,
    default: &'a str,
) -> &'a str {
    settings.get(key).map_or(default, String::as_str)
}

pub(super) fn format_settings_list(settings: &HashMap<String, String>) -> String {
    SETTINGS_LIST
        .iter()
        .map(|(key, label, fallback)| {
            format!(
                "- **{label}** (`{key}`): `{}`",
                setting_value(settings, key, fallback)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn format_library_item(manga: &backend_persistence::MangaRow) -> String {
    format!(
        "- **{}** on `{}` - {}/{} chapters",
        manga.title, manga.source, manga.downloaded_chapters, manga.total_chapters
    )
}

pub(super) fn format_download_item(download: &backend_persistence::DownloadRow) -> String {
    let progress = format!("{:.0}%", download.progress);
    let chapter = backend_core::format_chapter_number(download.chapter_number);
    let error = download
        .error
        .as_deref()
        .filter(|error| !error.trim().is_empty())
        .map(|error| format!(" - {error}"))
        .unwrap_or_default();

    format!(
        "- `{}` **{}** chapter {} - {}{}",
        download.status, download.manga_title, chapter, progress, error
    )
}

pub(super) fn format_update_sections(updates: impl Iterator<Item = CommandUpdate>) -> String {
    let mut sections = Vec::<(String, Vec<String>)>::new();

    for update in updates {
        if let Some((_, lines)) = sections
            .iter_mut()
            .find(|(date, _)| *date == update.release_date)
        {
            lines.push(update.line);
        } else {
            sections.push((update.release_date, vec![update.line]));
        }
    }

    sections
        .into_iter()
        .map(|(date, lines)| format!("**{date}**\n{}", lines.join("\n")))
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(super) struct CommandUpdate {
    release_date: String,
    line: String,
}

pub(super) fn format_command_update(
    chapter: &backend_persistence::ChapterRow,
    manga_title: Option<&str>,
) -> CommandUpdate {
    let release_date = format_date(&chapter.date_uploaded);
    CommandUpdate {
        release_date: release_date.clone(),
        line: format!(
            "- **{}** chapter {}: {} - released {release_date}",
            manga_title.unwrap_or("unknown series"),
            backend_core::format_chapter_number(chapter.chapter_number),
            chapter.title,
        ),
    }
}

pub(super) fn format_source_item(source: &backend_plugin_host::SourceInfo) -> String {
    let state = if source.enabled {
        "enabled"
    } else {
        "disabled"
    };
    let capabilities = if source.capabilities.is_empty() {
        "none".to_owned()
    } else {
        source.capabilities.join(", ")
    };

    format!(
        "- **{}** (`{}`) - {}, v{}, {}",
        source.display_name, source.name, state, source.plugin_version, capabilities
    )
}

pub(super) fn format_counts<'a>(items: impl Iterator<Item = &'a str>, max_items: usize) -> String {
    let mut counts = BTreeMap::<&str, usize>::new();
    for item in items {
        *counts.entry(item).or_default() += 1;
    }

    if counts.is_empty() {
        return "None".to_owned();
    }

    let mut counts = counts.into_iter().collect::<Vec<_>>();
    counts.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    let hidden_count = counts.len().saturating_sub(max_items);
    let mut lines = counts
        .into_iter()
        .take(max_items)
        .map(|(item, count)| format!("- `{item}`: {count}"))
        .collect::<Vec<_>>();
    if hidden_count > 0 {
        lines.push(format!("- ...and {hidden_count} more"));
    }

    lines.join("\n")
}

fn format_date(value: &str) -> String {
    let trimmed = value.trim().trim_matches('"');
    if trimmed.is_empty() {
        return "Unknown date".to_owned();
    }

    if let Some(date) = parse_date_value(trimmed) {
        return date.strftime("%b %-d, %Y").to_string();
    }

    trimmed
        .split_once('T')
        .or_else(|| trimmed.split_once(' '))
        .map_or_else(|| trimmed.to_owned(), |(date, _)| date.to_owned())
}

fn parse_date_value(value: &str) -> Option<DateTime> {
    if let Ok(timestamp) = value.parse::<i64>() {
        let parsed = if value.len() > 10 {
            Timestamp::from_millisecond(timestamp)
        } else {
            Timestamp::from_second(timestamp)
        };
        if let Ok(timestamp) = parsed {
            return Some(utc_datetime(timestamp));
        }
    }

    if let Ok(timestamp) = value.parse::<Timestamp>() {
        return Some(utc_datetime(timestamp));
    }

    if let Ok(date) = jiff::civil::Date::strptime("%Y-%m-%d", value) {
        return Some(DateTime::from(date));
    }

    if let Ok(date) = DateTime::strptime("%Y-%m-%d %H:%M:%S", value) {
        return Some(date);
    }

    None
}

fn utc_datetime(timestamp: Timestamp) -> DateTime {
    timestamp.to_zoned(TimeZone::UTC).datetime()
}

pub(super) fn blank_as_uncategorized(value: &str) -> &str {
    let value = value.trim();
    if value.is_empty() {
        "Uncategorized"
    } else {
        value
    }
}

pub(super) fn is_active_download_status(status: &str) -> bool {
    matches!(status, "canceling" | "fetch" | "conversion" | "archive")
}
