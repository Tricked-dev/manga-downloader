use serde::{Deserialize, Deserializer};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::http::media_hotlink_ref;
use crate::ids::manga_identity;
use crate::manga::source::types::MediaRef;
use crate::{BASE_URL, Manga};

#[derive(Deserialize)]
pub struct SearchResponse {
    pub result: MangaItems,
}

#[derive(Deserialize)]
pub struct MangaItems {
    pub items: Vec<ComixManga>,
    #[serde(default = "default_pagination")]
    pub pagination: Pagination,
    #[serde(default)]
    pub meta: Option<Pagination>,
}

fn default_pagination() -> Pagination {
    Pagination {
        current_page: 1,
        last_page: 1,
        has_next: false,
    }
}

#[derive(Debug)]
pub struct Pagination {
    pub current_page: i32,
    pub last_page: i32,
    pub has_next: bool,
}

#[derive(Deserialize)]
struct RawPagination {
    #[serde(default)]
    current_page: Option<i32>,
    #[serde(default)]
    page: Option<i32>,
    #[serde(default)]
    last_page: Option<i32>,
    #[serde(default, rename = "lastPage")]
    last_page_camel: Option<i32>,
    #[serde(default)]
    has_next: bool,
    #[serde(default, rename = "hasNext")]
    has_next_camel: bool,
}

impl<'de> Deserialize<'de> for Pagination {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawPagination::deserialize(deserializer)?;
        Ok(Self {
            current_page: raw.current_page.or(raw.page).unwrap_or(1),
            last_page: raw
                .last_page
                .unwrap_or(1)
                .max(raw.last_page_camel.unwrap_or(1)),
            has_next: raw.has_next || raw.has_next_camel,
        })
    }
}

impl MangaItems {
    pub const fn has_next_page(&self) -> bool {
        let pagination = match self.meta.as_ref() {
            Some(meta) => meta,
            None => &self.pagination,
        };
        pagination.has_next || pagination.current_page < pagination.last_page
    }
}

#[derive(Deserialize)]
pub struct ComixManga {
    #[serde(default, alias = "id")]
    pub manga_id: i32,
    #[serde(alias = "hid")]
    pub hash_id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default, alias = "altTitles")]
    pub alt_titles: Vec<String>,
    pub slug: Option<String>,
    pub synopsis: Option<String>,
    #[serde(default, rename = "type", alias = "mediaType")]
    pub media_type: String,
    #[serde(default)]
    pub status: String,
    pub poster: Option<Poster>,
    #[serde(alias = "authors")]
    pub author: Option<Vec<Term>>,
    #[serde(default, alias = "genres", alias = "genre")]
    pub genres: Vec<Term>,
    #[serde(default, alias = "theme")]
    pub tags: Vec<Term>,
    #[serde(default, alias = "demographic")]
    pub demographics: Vec<Term>,
    #[serde(default)]
    pub formats: Vec<Term>,
    #[serde(default, alias = "contentRating")]
    pub content_rating: String,
    #[serde(
        default,
        alias = "is_nsfw",
        alias = "nsfw",
        alias = "is_adult",
        alias = "adult",
        alias = "is_mature",
        alias = "mature",
        alias = "explicit"
    )]
    pub nsfw: serde_json::Value,
}

#[derive(Deserialize)]
pub struct Poster {
    pub small: Option<String>,
    pub medium: Option<String>,
    pub large: Option<String>,
}

impl Poster {
    fn best_url(self) -> String {
        self.large
            .or(self.medium)
            .or(self.small)
            .filter(|url| !url.is_empty())
            .unwrap_or_default()
    }
}

#[derive(Deserialize)]
pub struct Term {
    #[serde(alias = "title")]
    pub name: String,
}

#[derive(Deserialize)]
pub struct SingleMangaResponse {
    pub result: SingleManga,
}

#[derive(Deserialize)]
pub struct SingleManga {
    #[serde(default, alias = "id")]
    pub manga_id: Option<i32>,
    #[serde(alias = "hid")]
    pub hash_id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default, alias = "altTitles")]
    pub alt_titles: Vec<String>,
    pub slug: Option<String>,
    pub synopsis: Option<String>,
    #[serde(default, rename = "type", alias = "mediaType")]
    pub media_type: String,
    #[serde(default)]
    pub status: String,
    pub poster: Option<Poster>,
    #[serde(alias = "authors")]
    pub author: Option<Vec<Term>>,
    #[serde(default, alias = "genres", alias = "genre")]
    pub genres: Vec<Term>,
    #[serde(default, alias = "theme")]
    pub tags: Vec<Term>,
    #[serde(default, alias = "demographic")]
    pub demographics: Vec<Term>,
    #[serde(default)]
    pub formats: Vec<Term>,
    #[serde(default)]
    pub term_ids: Vec<i32>,
    #[serde(default, alias = "latestChapter")]
    pub latest_chapter: Option<f32>,
    #[serde(default, alias = "hasChapters")]
    pub has_chapters: Option<bool>,
    pub url: Option<String>,
    #[serde(default, alias = "contentRating")]
    pub content_rating: String,
    #[serde(
        default,
        alias = "is_nsfw",
        alias = "nsfw",
        alias = "is_adult",
        alias = "adult",
        alias = "is_mature",
        alias = "mature",
        alias = "explicit"
    )]
    pub nsfw: serde_json::Value,
}

#[derive(Deserialize)]
pub struct ChapterDetailsResponse {
    pub result: ChapterItems,
}

#[derive(Deserialize)]
pub struct ChapterItems {
    pub items: Vec<ComixChapter>,
    #[serde(default = "default_pagination")]
    pub pagination: Pagination,
    #[serde(default)]
    pub meta: Option<Pagination>,
}

impl ChapterItems {
    pub const fn has_next_page(&self) -> bool {
        let pagination = match self.meta.as_ref() {
            Some(meta) => meta,
            None => &self.pagination,
        };
        pagination.has_next || pagination.current_page < pagination.last_page
    }
}

#[derive(Deserialize)]
pub struct ChapterListResponse {
    pub result: Vec<ComixChapter>,
}

#[derive(Deserialize, Clone)]
pub struct ChapterIndex {
    pub number: f32,
}

#[derive(Deserialize)]
pub struct SingleMangaWithChaptersResponse {
    pub result: MangaWithChapters,
}

#[derive(Deserialize)]
pub struct MangaWithChapters {
    pub chapters: Vec<ComixChapter>,
}

#[derive(Deserialize, Clone)]
pub struct ComixChapter {
    #[serde(default, alias = "id")]
    pub chapter_id: i32,
    pub number: f32,
    #[serde(default)]
    pub name: String,
    #[serde(
        default,
        alias = "updatedAt",
        alias = "createdAt",
        alias = "publishedAt",
        alias = "publishAt",
        alias = "uploadedAt"
    )]
    pub updated_at: serde_json::Value,
    #[serde(default, alias = "createdAtFormatted")]
    pub created_at_formatted: String,
    pub votes: Option<i32>,
    #[serde(default)]
    pub scanlation_group_id: Option<i32>,
    #[serde(default)]
    pub group: Option<ScanlationGroup>,
    #[serde(default)]
    #[serde(alias = "isOfficial")]
    pub is_official: serde_json::Value,
    #[serde(skip)]
    pub reader_url: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct ScanlationGroup {
    pub id: Option<i32>,
}

impl ComixChapter {
    pub fn is_official_bool(&self) -> bool {
        self.is_official.as_bool().unwrap_or_else(|| {
            self.is_official
                .as_i64()
                .map(|value| value != 0)
                .or_else(|| self.is_official.as_u64().map(|value| value != 0))
                .unwrap_or(false)
        })
    }

    pub fn group_id(&self) -> Option<i32> {
        self.group
            .as_ref()
            .and_then(|group| group.id)
            .or(self.scanlation_group_id)
    }

    pub fn published_at(&self) -> String {
        if let Some(timestamp) = self.updated_at.as_i64() {
            return timestamp.to_string();
        }
        if let Some(timestamp) = self.updated_at.as_u64() {
            return timestamp.to_string();
        }
        if let Some(value) = self.updated_at.as_str().filter(|value| !value.is_empty()) {
            return value.to_string();
        }
        if let Some(timestamp) = relative_timestamp_seconds(&self.created_at_formatted) {
            return timestamp;
        }
        self.created_at_formatted.clone()
    }
}

fn relative_timestamp_seconds(value: &str) -> Option<String> {
    let seconds = relative_duration_seconds(value)?;
    let now_seconds = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();

    Some(now_seconds.saturating_sub(seconds).to_string())
}

fn relative_duration_seconds(value: &str) -> Option<u64> {
    let lowercase = value.trim().to_ascii_lowercase();
    let normalized = lowercase
        .strip_suffix("ago")
        .map_or_else(|| lowercase.as_str(), str::trim);
    let mut amount_end = 0usize;
    for (index, character) in normalized.char_indices() {
        if character.is_ascii_digit() {
            amount_end = index + character.len_utf8();
        } else {
            break;
        }
    }

    if amount_end == 0 {
        return match normalized {
            "yesterday" => Some(24 * 60 * 60),
            _ => None,
        };
    }

    let amount = normalized[..amount_end].parse::<u64>().ok()?;
    let unit = normalized[amount_end..].trim().trim_end_matches('.').trim();
    let multiplier = match unit {
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 60 * 60,
        "d" | "day" | "days" => 24 * 60 * 60,
        "w" | "wk" | "wks" | "week" | "weeks" => 7 * 24 * 60 * 60,
        "mo" | "mos" | "mon" | "mons" | "month" | "months" => 30 * 24 * 60 * 60,
        "y" | "yr" | "yrs" | "year" | "years" => 365 * 24 * 60 * 60,
        _ => return None,
    };

    amount.checked_mul(multiplier)
}

#[derive(Deserialize)]
pub struct ChapterResponse {
    pub result: Option<ChapterResult>,
}

#[derive(Deserialize)]
pub struct ChapterResult {
    #[serde(default, alias = "pages")]
    pub images: Vec<ChapterImage>,
}

#[derive(Deserialize)]
pub struct ChapterImage {
    pub url: String,
    #[serde(default)]
    pub s: Option<serde_json::Value>,
    #[serde(default)]
    pub scramble: Option<serde_json::Value>,
    #[serde(default)]
    pub scrambled: Option<serde_json::Value>,
    #[serde(default)]
    pub manga_server_descramble_map: Option<serde_json::Value>,
    #[serde(default, alias = "descrambleMap")]
    pub descramble_map: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct ApiTerm {
    pub term_id: i32,
    #[serde(alias = "name", alias = "title")]
    pub title: String,
}

impl From<ComixManga> for Manga {
    fn from(value: ComixManga) -> Self {
        let is_nsfw = value_is_nsfw(&value.nsfw) || content_rating_is_nsfw(&value.content_rating);
        let cover_url = value.poster.map(Poster::best_url).unwrap_or_default();
        let title = preferred_title(
            &value.title,
            &value.alt_titles,
            value.slug.as_deref(),
            Some(value.manga_id),
            value.hash_id.as_deref(),
        );
        Manga {
            id: manga_identity(value.manga_id, value.hash_id.as_deref()),
            title,
            cover: if cover_url.is_empty() {
                MediaRef {
                    url: String::new(),
                    request: None,
                }
            } else {
                media_hotlink_ref(&cover_url, BASE_URL)
            },
            description: value.synopsis.unwrap_or_default(),
            author: value
                .author
                .unwrap_or_default()
                .into_iter()
                .map(|a| a.name)
                .collect::<Vec<_>>()
                .join(", "),
            genres: combined_genres(&value.media_type, value.genres, is_nsfw),
            status: value.status,
            is_nsfw,
            alt_titles: value.alt_titles,
        }
    }
}

impl From<SingleManga> for Manga {
    fn from(value: SingleManga) -> Self {
        let is_nsfw = value_is_nsfw(&value.nsfw) || content_rating_is_nsfw(&value.content_rating);
        let cover_url = value.poster.map(Poster::best_url).unwrap_or_default();
        let title = preferred_title(
            &value.title,
            &value.alt_titles,
            value.slug.as_deref(),
            value.manga_id,
            value.hash_id.as_deref(),
        );
        Manga {
            id: match (
                value.manga_id,
                value
                    .hash_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|hash_id| !hash_id.is_empty()),
            ) {
                (Some(manga_id), hash_id) => manga_identity(manga_id, hash_id),
                (None, Some(hash_id)) => hash_id.to_string(),
                (None, None) => String::new(),
            },
            title,
            cover: if cover_url.is_empty() {
                MediaRef {
                    url: String::new(),
                    request: None,
                }
            } else {
                media_hotlink_ref(&cover_url, BASE_URL)
            },
            description: value.synopsis.unwrap_or_default(),
            author: value
                .author
                .unwrap_or_default()
                .into_iter()
                .map(|a| a.name)
                .collect::<Vec<_>>()
                .join(", "),
            genres: combined_genres(&value.media_type, value.genres, is_nsfw),
            status: value.status,
            is_nsfw,
            alt_titles: value.alt_titles,
        }
    }
}

fn content_rating_is_nsfw(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "erotica" | "pornographic" | "nsfw" | "adult" | "mature" | "explicit"
    )
}

fn combined_genres(media_type: &str, genres: Vec<Term>, is_nsfw: bool) -> Vec<String> {
    let mut values = Vec::new();

    match media_type {
        "manhwa" => values.push("Manhwa".to_string()),
        "manhua" => values.push("Manhua".to_string()),
        "manga" => values.push("Manga".to_string()),
        "" => {}
        _ => values.push("Other".to_string()),
    }

    for term in genres {
        if !term.name.is_empty() && !values.iter().any(|value| value == &term.name) {
            values.push(term.name);
        }
    }

    if is_nsfw && !values.iter().any(|value| value == "NSFW") {
        values.push("NSFW".to_string());
    }

    values
}

fn value_is_nsfw(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(flag) => *flag,
        serde_json::Value::Number(number) => {
            number.as_i64().is_some_and(|parsed| parsed != 0)
                || number.as_u64().is_some_and(|parsed| parsed != 0)
        }
        serde_json::Value::String(text) => {
            let normalized = text.trim().to_ascii_lowercase();
            matches!(
                normalized.as_str(),
                "1" | "true" | "yes" | "nsfw" | "adult" | "mature" | "explicit" | "18+"
            )
        }
        _ => false,
    }
}

fn preferred_title(
    title: &str,
    alt_titles: &[String],
    slug: Option<&str>,
    manga_id: Option<i32>,
    hash_id: Option<&str>,
) -> String {
    let trimmed = title.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    if let Some(alt_title) = alt_titles
        .iter()
        .map(String::as_str)
        .find(|value| !value.trim().is_empty())
    {
        return alt_title.trim().to_string();
    }

    if let Some(slug) = slug.filter(|value| !value.trim().is_empty()) {
        return slug
            .split('-')
            .filter(|segment| !segment.is_empty())
            .map(|segment| {
                let mut chars = segment.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
    }

    if let Some(hash_id) = hash_id.filter(|value| !value.trim().is_empty()) {
        return format!("Comix {hash_id}");
    }

    manga_id.map_or_else(
        || "Unknown title".to_string(),
        |value| format!("Comix {value}"),
    )
}

#[cfg(test)]
mod tests {
    use super::{Term, combined_genres};

    fn term(name: &str) -> Term {
        Term {
            name: name.to_string(),
        }
    }

    #[test]
    fn combined_genres_keeps_broad_classification_terms() {
        assert_eq!(
            combined_genres(
                "manhwa",
                vec![term("Action"), term("Adventure"), term("Fantasy")],
                false,
            ),
            ["Manhwa", "Action", "Adventure", "Fantasy"]
        );
    }

    #[test]
    fn combined_genres_deduplicates_and_flags_nsfw() {
        assert_eq!(
            combined_genres(
                "manga",
                vec![term("Action"), term("Action"), term("")],
                true,
            ),
            ["Manga", "Action", "NSFW"]
        );
    }
}
