use aidoku::{
    Chapter, ContentRating, HashMap, Listing, ListingKind, Manga, MangaPageResult, MangaStatus,
    Viewer,
    alloc::{String, Vec, format, vec},
    helpers::uri::encode_uri_component,
};
use alloc::collections::{BTreeMap, BTreeSet};
use serde::Deserialize;

pub const ALL_CATEGORIES_LISTING_ID: &str = "category:*";
pub const CATEGORY_LISTING_PREFIX: &str = "category:";
pub const LOCAL_MANGA_PREFIX: &str = "local:";
pub const REMOTE_MANGA_PREFIX: &str = "remote:";
pub const LOCAL_CHAPTER_PREFIX: &str = "local:";
pub const REMOTE_CHAPTER_PREFIX: &str = "remote:";

#[derive(Debug, Deserialize)]
pub struct ApiListResponse<T> {
    pub items: Vec<T>,
}

pub type PageListResponse = ApiListResponse<String>;

#[derive(Debug, Deserialize)]
pub struct SettingsResponse {
    pub settings: HashMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ComicInfoDto {
    pub title: Option<String>,
    pub series: Option<String>,
    pub summary: Option<String>,
    pub writer: Option<String>,
    pub genre: Option<String>,
    pub age_rating: Option<String>,
    pub language_iso: Option<String>,
    pub web: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LibraryMangaDto {
    pub id: String,
    pub source: String,
    pub source_id: String,
    pub category: String,
    pub title: String,
    pub cover_url: String,
    pub cover_proxy_url: Option<String>,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub genres: Vec<String>,
    pub status: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub comic_info: ComicInfoDto,
    pub downloaded_chapters: usize,
    #[serde(default)]
    pub is_nsfw: bool,
}

impl LibraryMangaDto {
    pub fn has_downloads(&self) -> bool {
        self.downloaded_chapters > 0
    }

    pub fn is_in_category(&self, category: Option<&str>) -> bool {
        category.is_none_or(|category| self.category == category)
    }

    pub fn title_key(&self) -> String {
        normalize_title(&self.title)
    }

    pub fn matches_query(&self, query: Option<&str>) -> bool {
        let Some(query) = normalized_filter(query) else {
            return true;
        };

        text_contains(&self.title, &query)
            || text_contains(&self.source, &query)
            || text_contains(&self.source_id, &query)
            || text_contains(&self.category, &query)
            || text_contains(&self.description, &query)
            || text_contains(&self.author, &query)
            || text_contains(&self.status, &query)
            || self.genres.iter().any(|genre| text_contains(genre, &query))
            || self
                .language
                .as_deref()
                .is_some_and(|language| text_contains(language, &query))
            || self.comic_info.matches_query(&query)
    }

    pub fn matches_source(&self, sources: &[String]) -> bool {
        sources.is_empty()
            || sources
                .iter()
                .any(|source| self.source.eq_ignore_ascii_case(source))
    }

    pub fn matches_category(&self, category: Option<&str>) -> bool {
        normalized_filter(category).is_none_or(|category| {
            text_eq(&self.category, &category)
                || self.genres.iter().any(|genre| text_eq(genre, &category))
                || self
                    .comic_info
                    .genre
                    .as_deref()
                    .is_some_and(|genre| text_contains(genre, &category))
        })
    }

    pub fn matches_status(&self, status: Option<&str>) -> bool {
        normalized_filter(status).is_none_or(|status| text_eq(&self.status, &status))
    }

    pub fn matches_genre(&self, genre: Option<&str>) -> bool {
        normalized_filter(genre).is_none_or(|genre| {
            self.genres
                .iter()
                .any(|manga_genre| text_contains(manga_genre, &genre))
                || self
                    .comic_info
                    .genre
                    .as_deref()
                    .is_some_and(|value| text_contains(value, &genre))
        })
    }

    pub fn matches_language(&self, language: Option<&str>) -> bool {
        normalized_filter(language).is_none_or(|language| {
            self.language
                .as_deref()
                .is_some_and(|value| text_eq(value, &language))
                || self
                    .comic_info
                    .language_iso
                    .as_deref()
                    .is_some_and(|value| text_eq(value, &language))
        })
    }

    pub fn matches_age_rating(&self, age_rating: Option<&str>) -> bool {
        let Some(age_rating) = normalized_filter(age_rating) else {
            return true;
        };

        self.comic_info
            .age_rating
            .as_deref()
            .is_some_and(|value| text_contains(value, &age_rating))
            || match age_rating.as_str() {
                "adults only 18+" | "nsfw" => self.is_nsfw,
                "rating pending" | "safe" => !self.is_nsfw,
                _ => false,
            }
    }

    pub fn into_manga(self, base_url: &str) -> Manga {
        let cover = self
            .cover_proxy_url
            .as_deref()
            .filter(|url| !url.is_empty())
            .unwrap_or(&self.cover_url);
        let tags = if self.genres.is_empty() {
            self.comic_info.genre.as_deref().map(split_comicinfo_genre)
        } else {
            Some(self.genres)
        };
        Manga {
            key: local_manga_key(&self.id),
            title: self.comic_info.series.unwrap_or(self.title),
            cover: Some(absolute_url(base_url, cover)),
            description: Some(self.comic_info.summary.unwrap_or(self.description)),
            authors: non_empty_string_vec(self.comic_info.writer.unwrap_or(self.author)),
            url: Some(
                self.comic_info
                    .web
                    .unwrap_or(local_manga_details_url(base_url, &self.id)),
            ),
            tags,
            status: manga_status(&self.status),
            content_rating: if self.is_nsfw {
                ContentRating::NSFW
            } else {
                ContentRating::Safe
            },
            viewer: viewer_for_source(&self.source),
            ..Default::default()
        }
    }
}

impl ComicInfoDto {
    fn matches_query(&self, query: &str) -> bool {
        option_contains(self.title.as_deref(), query)
            || option_contains(self.series.as_deref(), query)
            || option_contains(self.summary.as_deref(), query)
            || option_contains(self.writer.as_deref(), query)
            || option_contains(self.genre.as_deref(), query)
            || option_contains(self.age_rating.as_deref(), query)
            || option_contains(self.language_iso.as_deref(), query)
            || option_contains(self.web.as_deref(), query)
    }
}

#[derive(Debug, Deserialize)]
pub struct LibraryChapterDto {
    pub id: String,
    pub source_id: String,
    pub title: String,
    pub chapter_number: f32,
    pub date_uploaded: String,
    pub downloaded: bool,
}

impl LibraryChapterDto {
    pub fn into_chapter(self, base_url: &str, manga_id: &str) -> Chapter {
        Chapter {
            key: local_chapter_key(&self.id),
            title: non_empty_string(self.title),
            chapter_number: Some(self.chapter_number),
            date_uploaded: parse_timestamp(&self.date_uploaded),
            url: Some(local_chapter_pages_url(base_url, manga_id)),
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SourceInfoDto {
    pub name: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub search_categories: Vec<String>,
    pub default_search_category: Option<String>,
    #[serde(default)]
    pub supports_search_popularity: bool,
    #[serde(default)]
    pub enabled: bool,
}

impl SourceInfoDto {
    pub fn supports_search(&self) -> bool {
        self.enabled
            && self
                .capabilities
                .iter()
                .any(|capability| capability == "search")
    }

    pub fn supported_category(&self, candidates: &[&str]) -> Option<String> {
        candidates.iter().find_map(|candidate| {
            self.search_categories
                .iter()
                .find(|category| category.eq_ignore_ascii_case(candidate))
                .cloned()
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct RemoteSearchResponse {
    pub mangas: Vec<RemoteMangaDto>,
    pub has_next_page: bool,
}

#[derive(Debug, Deserialize)]
pub struct RemoteMangaDto {
    pub id: String,
    pub title: String,
    pub cover_url: String,
    pub cover_proxy_url: Option<String>,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub genres: Vec<String>,
    pub status: String,
    #[serde(default)]
    pub is_nsfw: bool,
}

impl RemoteMangaDto {
    pub fn title_key(&self) -> String {
        normalize_title(&self.title)
    }

    pub fn into_manga(self, base_url: &str, source_name: &str) -> Manga {
        let id = self.id;
        let url = Some(remote_manga_details_url(base_url, source_name, &id));
        let cover = self
            .cover_proxy_url
            .as_deref()
            .filter(|url| !url.is_empty())
            .unwrap_or(&self.cover_url);

        Manga {
            key: remote_manga_key(source_name, &id),
            title: self.title,
            cover: Some(absolute_url(base_url, cover)),
            description: Some(self.description),
            authors: non_empty_string_vec(self.author),
            url,
            tags: Some(self.genres),
            status: manga_status(&self.status),
            content_rating: if self.is_nsfw {
                ContentRating::NSFW
            } else {
                ContentRating::Safe
            },
            viewer: viewer_for_source(source_name),
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RemoteChapterDto {
    pub id: String,
    pub title: String,
    pub chapter_number: f32,
    pub date_uploaded: String,
}

impl RemoteChapterDto {
    pub fn into_chapter(self, base_url: &str, source_name: &str, _manga_id: &str) -> Chapter {
        let id = self.id;
        Chapter {
            key: remote_chapter_key(source_name, &id),
            title: non_empty_string(self.title),
            chapter_number: Some(self.chapter_number),
            date_uploaded: parse_timestamp(&self.date_uploaded),
            url: Some(remote_chapter_pages_url(base_url, source_name, &id)),
            ..Default::default()
        }
    }
}

pub struct DownloadedMangaIndex {
    sources: BTreeMap<String, DownloadedSourceIndex>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CategoryListing<'a> {
    All,
    Category(&'a str),
}

impl<'a> CategoryListing<'a> {
    pub fn as_filter(self) -> Option<&'a str> {
        match self {
            Self::All => None,
            Self::Category(category) => Some(category),
        }
    }
}

impl DownloadedMangaIndex {
    pub fn new(downloaded: Vec<LibraryMangaDto>) -> Self {
        let mut sources = BTreeMap::<String, DownloadedSourceIndex>::new();
        for manga in downloaded
            .into_iter()
            .filter(LibraryMangaDto::has_downloads)
        {
            let title_key = manga.title_key();
            let source = sources.entry(manga.source).or_default();
            source.source_ids.insert(manga.source_id);
            source.title_keys.insert(title_key);
        }
        Self { sources }
    }

    fn contains_remote(&self, source_name: &str, manga: &RemoteMangaDto) -> bool {
        let Some(source) = self.sources.get(source_name) else {
            return false;
        };

        source.source_ids.contains(manga.id.as_str())
            || source.title_keys.contains(manga.title_key().as_str())
    }
}

#[derive(Default)]
struct DownloadedSourceIndex {
    source_ids: BTreeSet<String>,
    title_keys: BTreeSet<String>,
}

pub fn merge_search_results(
    base_url: &str,
    downloaded: &DownloadedMangaIndex,
    remote: Vec<(String, RemoteSearchResponse)>,
) -> MangaPageResult {
    let mut entries = Vec::new();
    let mut has_next_page = false;

    for (source_name, response) in remote {
        has_next_page = has_next_page || response.has_next_page;
        for manga in response.mangas {
            if !downloaded.contains_remote(&source_name, &manga) {
                entries.push(manga.into_manga(base_url, &source_name));
            }
        }
    }

    MangaPageResult {
        entries,
        has_next_page,
    }
}

pub fn category_listing(category: String) -> Listing {
    Listing {
        id: format!("{CATEGORY_LISTING_PREFIX}{category}"),
        name: category,
        kind: ListingKind::Default,
    }
}

pub fn all_categories_listing() -> Listing {
    Listing {
        id: ALL_CATEGORIES_LISTING_ID.into(),
        name: "All Categories".into(),
        kind: ListingKind::Default,
    }
}

pub fn absolute_url(base_url: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.into()
    } else if url.starts_with('/') {
        format!("{base_url}{url}")
    } else {
        format!("{base_url}/{url}")
    }
}

// These populate the `url` Aidoku opens in a browser, so they address the web UI. The
// API paths they used to point at rendered as raw JSON.
pub fn local_manga_details_url(base_url: &str, manga_id: &str) -> String {
    format!("{base_url}/library/{}", encode_uri_component(manga_id))
}

/// A downloaded chapter opens its series page, which is the page that can read it.
pub fn local_chapter_pages_url(base_url: &str, manga_id: &str) -> String {
    format!("{base_url}/library/{}", encode_uri_component(manga_id))
}

pub fn remote_manga_details_url(base_url: &str, source_name: &str, manga_id: &str) -> String {
    format!(
        "{base_url}/manga/{}/{}",
        encode_uri_component(source_name),
        encode_uri_component(manga_id)
    )
}

pub fn remote_chapter_pages_url(base_url: &str, source_name: &str, chapter_id: &str) -> String {
    format!(
        "{base_url}/read/{}/{}",
        encode_uri_component(source_name),
        encode_uri_component(chapter_id)
    )
}

pub fn local_manga_id(key: &str) -> Option<&str> {
    key.strip_prefix(LOCAL_MANGA_PREFIX)
}

pub fn remote_manga_parts(key: &str) -> Option<(&str, &str)> {
    let value = key.strip_prefix(REMOTE_MANGA_PREFIX)?;
    let (source_name, manga_id) = value.split_once(':')?;
    Some((source_name, manga_id))
}

pub fn local_chapter_id(key: &str) -> Option<&str> {
    key.strip_prefix(LOCAL_CHAPTER_PREFIX)
}

pub fn remote_chapter_parts(key: &str) -> Option<(&str, &str)> {
    let value = key.strip_prefix(REMOTE_CHAPTER_PREFIX)?;
    let (source_name, chapter_id) = value.split_once(':')?;
    Some((source_name, chapter_id))
}

pub fn category_from_listing_id(id: &str) -> Option<CategoryListing<'_>> {
    if id == ALL_CATEGORIES_LISTING_ID {
        Some(CategoryListing::All)
    } else {
        id.strip_prefix(CATEGORY_LISTING_PREFIX)
            .map(CategoryListing::Category)
    }
}

pub fn local_manga_key(id: &str) -> String {
    format!("{LOCAL_MANGA_PREFIX}{id}")
}

pub fn remote_manga_key(source_name: &str, id: &str) -> String {
    format!("{REMOTE_MANGA_PREFIX}{source_name}:{id}")
}

pub fn local_chapter_key(id: &str) -> String {
    format!("{LOCAL_CHAPTER_PREFIX}{id}")
}

pub fn remote_chapter_key(source_name: &str, id: &str) -> String {
    format!("{REMOTE_CHAPTER_PREFIX}{source_name}:{id}")
}

fn non_empty_string(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn non_empty_string_vec(value: String) -> Option<Vec<String>> {
    if value.trim().is_empty() {
        None
    } else {
        Some(vec![value])
    }
}

fn split_comicinfo_genre(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(Into::into)
        .collect()
}

fn option_contains(value: Option<&str>, query: &str) -> bool {
    value.is_some_and(|value| text_contains(value, query))
}

fn manga_status(status: &str) -> MangaStatus {
    match status.to_ascii_lowercase().as_str() {
        "ongoing" | "publishing" => MangaStatus::Ongoing,
        "completed" | "complete" | "finished" => MangaStatus::Completed,
        "hiatus" | "on_hiatus" | "on hiatus" => MangaStatus::Hiatus,
        "cancelled" | "canceled" => MangaStatus::Cancelled,
        _ => MangaStatus::Unknown,
    }
}

fn normalize_title(title: &str) -> String {
    title.trim().to_ascii_lowercase()
}

fn normalized_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

fn text_contains(value: &str, normalized_query: &str) -> bool {
    value.to_ascii_lowercase().contains(normalized_query)
}

fn text_eq(value: &str, normalized_query: &str) -> bool {
    value.trim().eq_ignore_ascii_case(normalized_query)
}

fn parse_timestamp(value: &str) -> Option<i64> {
    let timestamp = value.parse::<i64>().ok()?;

    if timestamp > 10_000_000_000 {
        Some(timestamp / 1000)
    } else {
        Some(timestamp)
    }
}

fn viewer_for_source(source_name: &str) -> Viewer {
    if source_name.eq_ignore_ascii_case("comix") {
        Viewer::RightToLeft
    } else {
        Viewer::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library_manga(source_id: &str, title: &str, downloaded_chapters: usize) -> LibraryMangaDto {
        LibraryMangaDto {
            id: format!("local-{source_id}"),
            source: "comix".into(),
            source_id: source_id.into(),
            category: "downloaded".into(),
            title: title.into(),
            cover_url: "/cover.jpg".into(),
            cover_proxy_url: None,
            description: String::new(),
            author: String::new(),
            genres: Vec::new(),
            status: "ongoing".into(),
            language: None,
            comic_info: ComicInfoDto::default(),
            downloaded_chapters,
            is_nsfw: false,
        }
    }

    fn remote_manga(id: &str, title: &str) -> RemoteMangaDto {
        RemoteMangaDto {
            id: id.into(),
            title: title.into(),
            cover_url: "/cover.jpg".into(),
            cover_proxy_url: None,
            description: String::new(),
            author: String::new(),
            genres: Vec::new(),
            status: "ongoing".into(),
            is_nsfw: false,
        }
    }

    #[test]
    fn category_listing_ids_distinguish_all_specific_and_unknown_listings() {
        assert_eq!(
            category_from_listing_id("category:*"),
            Some(CategoryListing::All)
        );
        assert_eq!(
            category_from_listing_id("category:downloaded"),
            Some(CategoryListing::Category("downloaded"))
        );
        assert_eq!(category_from_listing_id("latest"), None);
    }

    #[test]
    fn merged_remote_search_results_skip_downloaded_manga_by_id_or_title() {
        let downloaded = DownloadedMangaIndex::new(vec![
            library_manga("remote-1", "Different Title", 1),
            library_manga("remote-2", "Already Downloaded", 1),
            library_manga("remote-3", "Ignored Without Chapters", 0),
        ]);
        let page = merge_search_results(
            "https://manga.example.test",
            &downloaded,
            vec![(
                "comix".into(),
                RemoteSearchResponse {
                    has_next_page: true,
                    mangas: vec![
                        remote_manga("remote-1", "Fresh Title"),
                        remote_manga("fresh", "Already Downloaded"),
                        remote_manga("fresh-2", "New Manga"),
                    ],
                },
            )],
        );

        assert!(page.has_next_page);
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].title, "New Manga");
    }
}
