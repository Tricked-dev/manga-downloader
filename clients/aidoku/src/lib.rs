#![no_std]
extern crate alloc;

mod models;
mod settings;

use aidoku::{
    BaseUrlProvider, Chapter, DynamicListings, FilterValue, ImageRequestProvider, ImageResponse,
    Listing, ListingProvider, Manga, MangaPageResult, Page, PageContent, PageImageProcessor,
    NotificationHandler, Result, Source,
    alloc::{String, Vec, format},
    helpers::uri::{QueryParameters, encode_uri_component},
    imports::{canvas::ImageRef, net::Request},
    prelude::*,
};
use alloc::collections::BTreeSet;
use alloc::string::ToString;
use models::{
    ApiListResponse, DownloadedMangaIndex, LibraryChapterDto, LibraryMangaDto, PageListResponse,
    RemoteChapterDto, RemoteMangaDto, RemoteSearchResponse, SettingsResponse, SourceInfoDto,
    absolute_url, all_categories_listing, category_from_listing_id, category_listing,
    local_chapter_id, local_manga_id, merge_search_results, remote_chapter_parts,
    remote_manga_parts,
};

type RemoteSearchPage = (String, RemoteSearchResponse);

const DEFAULT_LIBRARY_CATEGORIES: &str = "default,downloaded";
const UPDATED_CATEGORIES: &[&str] = &["updated_date", "updated", "latest", "last_updated"];
const CREATED_CATEGORIES: &[&str] = &["created_date", "created", "newest", "added_date"];
const TITLE_CATEGORIES: &[&str] = &["title_ascending", "title", "title_az", "alphabetical"];
const RATING_CATEGORIES: &[&str] = &["average_score", "rating", "score"];
const DOWNLOADED_CHAPTER_PAGE_ROUTE_PREFIX: &str = "/v1/library/chapters/";
const DOWNLOADED_CHAPTER_PAGE_ROUTE_SEGMENT: &str = "/pages/";
const POPULAR_CATEGORIES: &[&str] = &[
    "popular",
    "most_views_7d",
    "most_views_1mo",
    "total_views",
    "most_follows",
];

#[derive(Default)]
struct SourceSearchFilters {
    sources: Vec<String>,
    sort: SearchSort,
    category: Option<String>,
    status: Option<String>,
    genre: Option<String>,
    language: Option<String>,
    age_rating: Option<String>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum SearchSort {
    #[default]
    BestMatch,
    RecentlyUpdated,
    RecentlyAdded,
    Title,
    Rating,
    Popular,
    MostViews7d,
    MostViews1mo,
    TotalViews,
    MostFollows,
}

struct EffectiveSourceSearchFilters {
    category: Option<String>,
    popular: bool,
}

impl SourceSearchFilters {
    fn is_default_catalog_request(&self, query: Option<&str>) -> bool {
        query.is_none_or(|query| query.trim().is_empty())
            && self.sources.is_empty()
            && self.category.is_none()
            && self.status.is_none()
            && self.genre.is_none()
            && self.language.is_none()
            && self.age_rating.is_none()
            && self.sort == SearchSort::BestMatch
    }

    fn needs_local_search(&self, query: Option<&str>) -> bool {
        self.is_default_catalog_request(query)
            || query.is_some_and(|query| !query.trim().is_empty())
            || !self.sources.is_empty()
            || self.category.is_some()
            || self.status.is_some()
            || self.genre.is_some()
            || self.language.is_some()
            || self.age_rating.is_some()
            || self.sort != SearchSort::BestMatch
    }

    fn needs_remote_search(&self, query: Option<&str>) -> bool {
        self.is_default_catalog_request(query)
            || query.is_some_and(|query| !query.trim().is_empty())
            || !self.sources.is_empty()
            || self.category.is_some()
            || self.sort != SearchSort::BestMatch
    }

    fn for_source(&self, source: Option<&SourceInfoDto>) -> Option<EffectiveSourceSearchFilters> {
        match self.sort {
            SearchSort::BestMatch => Some(EffectiveSourceSearchFilters {
                category: self
                    .category
                    .clone()
                    .or_else(|| source.and_then(|source| source.default_search_category.clone())),
                popular: false,
            }),
            SearchSort::RecentlyUpdated => self.category_filters(source, UPDATED_CATEGORIES),
            SearchSort::RecentlyAdded => self.category_filters(source, CREATED_CATEGORIES),
            SearchSort::Title => self.category_filters(source, TITLE_CATEGORIES),
            SearchSort::Rating => self.category_filters(source, RATING_CATEGORIES),
            SearchSort::Popular => {
                if source.is_some_and(|source| source.supports_search_popularity) {
                    Some(EffectiveSourceSearchFilters {
                        category: self.category.clone(),
                        popular: true,
                    })
                } else {
                    self.category_filters(source, POPULAR_CATEGORIES)
                }
            }
            SearchSort::MostViews7d => self.category_filters(source, &["most_views_7d"]),
            SearchSort::MostViews1mo => self.category_filters(source, &["most_views_1mo"]),
            SearchSort::TotalViews => self.category_filters(source, &["total_views"]),
            SearchSort::MostFollows => self.category_filters(source, &["most_follows"]),
        }
    }

    fn category_filters(
        &self,
        source: Option<&SourceInfoDto>,
        candidates: &[&str],
    ) -> Option<EffectiveSourceSearchFilters> {
        let category = match source {
            Some(source) => source.supported_category(candidates)?,
            None => candidates.first()?.to_string(),
        };
        Some(EffectiveSourceSearchFilters {
            category: self.category.clone().or(Some(category)),
            popular: false,
        })
    }

    fn local_entries(
        &self,
        base_url: &str,
        downloaded_library: &[LibraryMangaDto],
        query: Option<&str>,
    ) -> Vec<Manga> {
        let mut mangas = downloaded_library
            .iter()
            .filter(|manga| manga.matches_query(query))
            .filter(|manga| manga.matches_source(&self.sources))
            .filter(|manga| manga.matches_category(self.category.as_deref()))
            .filter(|manga| manga.matches_status(self.status.as_deref()))
            .filter(|manga| manga.matches_genre(self.genre.as_deref()))
            .filter(|manga| manga.matches_language(self.language.as_deref()))
            .filter(|manga| manga.matches_age_rating(self.age_rating.as_deref()))
            .cloned()
            .collect::<Vec<_>>();

        if self.sort == SearchSort::Title {
            mangas.sort_by_key(LibraryMangaDto::title_key);
        }

        mangas
            .into_iter()
            .map(|manga| manga.into_manga(base_url))
            .collect()
    }
}

struct ApiClient {
    base_url: String,
    api_base: String,
    auth_header: Option<String>,
}

impl ApiClient {
    fn new() -> Self {
        let base_url = settings::server_base_url();
        settings::sync_login_url(&base_url);
        let api_base = format!("{base_url}/v1");
        let api_key = settings::backend_api_key();
        let auth_header = if api_key.is_empty() {
            None
        } else {
            Some(format!("Bearer {api_key}"))
        };

        Self {
            base_url,
            api_base,
            auth_header,
        }
    }

    fn source_api_base(&self, source_name: &str) -> String {
        format!(
            "{}/sources/{}",
            self.api_base,
            encode_uri_component(source_name)
        )
    }

    fn get(&self, url: String) -> Result<Request> {
        let request = Request::get(url)?;
        Ok(self.authenticated(request))
    }

    fn authenticated(&self, mut request: Request) -> Request {
        if let Some(auth_header) = &self.auth_header {
            request.set_header("Authorization", auth_header.as_str());
        }
        request
    }

    fn library(&self) -> Result<Vec<LibraryMangaDto>> {
        self.get(format!("{}/library", self.api_base))?
            .json_owned::<ApiListResponse<LibraryMangaDto>>()
            .map(|response| response.items)
    }

    fn downloaded_library(&self, category: Option<&str>) -> Result<Vec<LibraryMangaDto>> {
        self.library().map(|items| {
            items
                .into_iter()
                .filter(LibraryMangaDto::has_downloads)
                .filter(|manga| manga.is_in_category(category))
                .collect()
        })
    }

    fn source_names(filters: &SourceSearchFilters, sources: &[SourceInfoDto]) -> Vec<String> {
        let mut names = sources
            .iter()
            .filter(|source| source.supports_search())
            .map(|source| source.name.clone())
            .collect::<Vec<_>>();
        dedupe_strings(&mut names);

        if filters.sources.is_empty() {
            if names.is_empty() {
                names = settings::fallback_source_names();
            }
            return names;
        }

        if names.is_empty() {
            return filters.sources.clone();
        }

        names.retain(|source_name| {
            filters
                .sources
                .iter()
                .any(|requested| source_name.eq_ignore_ascii_case(requested))
        });

        names
    }

    fn sources(&self) -> Result<Vec<SourceInfoDto>> {
        self.get(format!("{}/sources", self.api_base))?
            .json_owned::<ApiListResponse<SourceInfoDto>>()
            .map(|response| response.items)
    }

    fn search_source(
        &self,
        source_name: &str,
        query: Option<&str>,
        page: i32,
        filters: &EffectiveSourceSearchFilters,
    ) -> Result<RemoteSearchResponse> {
        let mut qs = QueryParameters::new();
        qs.push("page", Some(&page.to_string()));
        qs.push("q", query.or(Some("")));
        qs.push("category", filters.category.as_deref());
        if filters.popular {
            qs.push("popular", Some("true"));
        }

        self.get(format!("{}/search?{qs}", self.source_api_base(source_name)))?
            .json_owned::<RemoteSearchResponse>()
    }

    fn library_categories(&self) -> Result<Vec<String>> {
        let response = self
            .get(format!("{}/settings", self.api_base))?
            .json_owned::<SettingsResponse>()?;
        let value = response
            .settings
            .get("library_categories")
            .map_or(DEFAULT_LIBRARY_CATEGORIES, String::as_str);
        let categories = parse_csv(value);
        if categories.is_empty() {
            Ok(parse_csv(DEFAULT_LIBRARY_CATEGORIES))
        } else {
            Ok(categories)
        }
    }

    fn local_manga(&self, manga_id: &str) -> Result<Manga> {
        self.get(format!(
            "{}/library/{}",
            self.api_base,
            encode_uri_component(manga_id)
        ))?
        .json_owned::<LibraryMangaDto>()
        .map(|manga| manga.into_manga(&self.base_url))
    }

    fn local_chapters(&self, manga_id: &str) -> Result<Vec<Chapter>> {
        Ok(self
            .library_chapters(manga_id)?
            .into_iter()
            .filter(|chapter| chapter.downloaded)
            .map(|chapter| chapter.into_chapter(&self.base_url))
            .collect())
    }

    fn library_chapters(&self, manga_id: &str) -> Result<Vec<LibraryChapterDto>> {
        self.get(format!(
            "{}/library/{}/chapters",
            self.api_base,
            encode_uri_component(manga_id)
        ))?
        .json_owned::<ApiListResponse<LibraryChapterDto>>()
        .map(|response| response.items)
    }

    fn downloaded_pages(&self, chapter_id: &str) -> Result<Vec<Page>> {
        self.get(format!(
            "{}/library/chapters/{}/pages",
            self.api_base,
            encode_uri_component(chapter_id)
        ))?
        .json_owned::<PageListResponse>()
        .map(|response| pages_from_urls(&self.base_url, response))
    }

    fn downloaded_manga_for_remote(
        &self,
        source_name: &str,
        manga_id: &str,
    ) -> Result<Option<LibraryMangaDto>> {
        Ok(self
            .downloaded_library(None)?
            .into_iter()
            .find(|manga| downloaded_manga_matches_remote(manga, source_name, manga_id)))
    }

    fn downloaded_chapter_id_for_remote(
        &self,
        local_manga_id: &str,
        chapter_id: &str,
    ) -> Result<Option<String>> {
        Ok(self
            .library_chapters(local_manga_id)?
            .into_iter()
            .find(|chapter| chapter.downloaded && chapter.source_id == chapter_id)
            .map(|chapter| chapter.id))
    }

    fn remote_manga(&self, source_name: &str, manga_id: &str) -> Result<Manga> {
        self.get(format!(
            "{}/manga/{}",
            self.source_api_base(source_name),
            encode_uri_component(manga_id)
        ))?
        .json_owned::<RemoteMangaDto>()
        .map(|manga| manga.into_manga(&self.base_url, source_name))
    }

    fn remote_chapters(&self, source_name: &str, manga_id: &str) -> Result<Vec<Chapter>> {
        let response = self
            .get(format!(
                "{}/manga/{}/chapters",
                self.source_api_base(source_name),
                encode_uri_component(manga_id)
            ))?
            .json_owned::<ApiListResponse<RemoteChapterDto>>()?;

        Ok(response
            .items
            .into_iter()
            .map(|chapter| chapter.into_chapter(&self.base_url, source_name, manga_id))
            .collect())
    }
}

struct MangaDownloader;

impl MangaDownloader {
    fn api() -> ApiClient {
        ApiClient::new()
    }
}

impl Source for MangaDownloader {
    fn new() -> Self {
        // The login control reads its URL from defaults, and the settings screen can be
        // opened before any request has run, so publish it here rather than on first use.
        settings::sync_login_url(&settings::server_base_url());
        Self
    }

    fn get_search_manga_list(
        &self,
        query: Option<String>,
        page: i32,
        filters: Vec<FilterValue>,
    ) -> Result<MangaPageResult> {
        let source_filters = source_search_filters(filters);
        if !source_filters.needs_local_search(query.as_deref())
            && !source_filters.needs_remote_search(query.as_deref())
        {
            return Ok(empty_manga_page());
        }

        let api = MangaDownloader::api();
        let downloaded_library = api.downloaded_library(None).unwrap_or_default();
        let mut local_entries = if source_filters.needs_local_search(query.as_deref()) {
            source_filters.local_entries(&api.base_url, &downloaded_library, query.as_deref())
        } else {
            Vec::new()
        };

        if !source_filters.needs_remote_search(query.as_deref()) {
            return Ok(MangaPageResult {
                entries: local_entries,
                has_next_page: false,
            });
        }

        let sources = api.sources().unwrap_or_default();
        let mut remote = Vec::<RemoteSearchPage>::new();

        let source_names = ApiClient::source_names(&source_filters, &sources);
        let source_count = source_names.len();
        for source_name in source_names {
            let Some(effective_filters) =
                source_filters.for_source(source_info(&sources, &source_name))
            else {
                continue;
            };
            if source_count == 1 {
                remote.push((
                    source_name.clone(),
                    api.search_source(&source_name, query.as_deref(), page, &effective_filters)?,
                ));
            } else if let Ok(response) =
                api.search_source(&source_name, query.as_deref(), page, &effective_filters)
            {
                remote.push((source_name, response));
            }
        }

        if remote.is_empty() {
            return Ok(MangaPageResult {
                entries: local_entries,
                has_next_page: false,
            });
        }

        let downloaded = DownloadedMangaIndex::new(downloaded_library);
        let mut page = merge_search_results(&api.base_url, &downloaded, remote);
        local_entries.append(&mut page.entries);
        page.entries = local_entries;
        Ok(page)
    }

    fn get_manga_update(
        &self,
        mut manga: Manga,
        needs_details: bool,
        needs_chapters: bool,
    ) -> Result<Manga> {
        if let Some(manga_id) = local_manga_id(&manga.key) {
            let manga_id = manga_id.to_string();
            if needs_details || needs_chapters {
                let api = MangaDownloader::api();
                if needs_details {
                    manga.copy_from(api.local_manga(&manga_id)?);
                }
                if needs_chapters {
                    manga.chapters = Some(api.local_chapters(&manga_id)?);
                }
            }
            return Ok(manga);
        }

        let (source_name, manga_id) =
            remote_manga_parts(&manga.key).ok_or_else(|| error!("Invalid manga key"))?;
        let source_name = source_name.to_string();
        let manga_id = manga_id.to_string();

        if needs_details || needs_chapters {
            let api = MangaDownloader::api();
            if needs_details {
                manga.copy_from(api.remote_manga(&source_name, &manga_id)?);
            }
            if needs_chapters {
                manga.chapters = Some(api.remote_chapters(&source_name, &manga_id)?);
            }
        }

        Ok(manga)
    }

    fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
        let api = MangaDownloader::api();
        if let Some(chapter_id) = local_chapter_id(&chapter.key) {
            return api.downloaded_pages(chapter_id);
        }

        let (source_name, chapter_id) =
            remote_chapter_parts(&chapter.key).ok_or_else(|| error!("Invalid chapter key"))?;
        if let Some((manga_source_name, manga_id)) = remote_manga_parts(&_manga.key)
            && manga_source_name.eq_ignore_ascii_case(source_name)
            && let Some(local_manga) = api.downloaded_manga_for_remote(source_name, manga_id)?
        {
            let Some(local_chapter_id) =
                api.downloaded_chapter_id_for_remote(&local_manga.id, chapter_id)?
            else {
                return Ok(Vec::new());
            };
            return api.downloaded_pages(&local_chapter_id);
        }

        api.get(format!(
            "{}/chapters/{}/pages?proxy=true",
            api.source_api_base(source_name),
            encode_uri_component(chapter_id)
        ))?
        .json_owned::<PageListResponse>()
        .map(|response| pages_from_urls(&api.base_url, response))
    }
}

impl ListingProvider for MangaDownloader {
    fn get_manga_list(&self, listing: Listing, _page: i32) -> Result<MangaPageResult> {
        let category =
            category_from_listing_id(&listing.id).ok_or_else(|| error!("Unknown listing"))?;
        let api = MangaDownloader::api();
        let entries = api
            .downloaded_library(category.as_filter())?
            .into_iter()
            .map(|manga| manga.into_manga(&api.base_url))
            .collect();
        Ok(MangaPageResult {
            entries,
            has_next_page: false,
        })
    }
}

impl DynamicListings for MangaDownloader {
    fn get_dynamic_listings(&self) -> Result<Vec<Listing>> {
        let mut listings = Vec::new();
        listings.push(all_categories_listing());
        listings.extend(
            MangaDownloader::api()
                .library_categories()?
                .into_iter()
                .map(category_listing),
        );
        Ok(listings)
    }
}

impl ImageRequestProvider for MangaDownloader {
    fn get_image_request(
        &self,
        url: String,
        _context: Option<aidoku::PageContext>,
    ) -> Result<Request> {
        MangaDownloader::api().get(url)
    }
}

impl PageImageProcessor for MangaDownloader {
    fn process_page_image(
        &self,
        response: ImageResponse,
        _context: Option<aidoku::PageContext>,
    ) -> Result<ImageRef> {
        if response.code == 409
            && let Some(url) = response.request.url.as_deref()
            && is_downloaded_page_image_url(url)
        {
            return Ok(MangaDownloader::api()
                .get(downloaded_page_retry_url(url))?
                .image()?);
        }

        Ok(response.image)
    }
}

impl BaseUrlProvider for MangaDownloader {
    fn get_base_url(&self) -> Result<String> {
        Ok(settings::server_base_url())
    }
}

impl NotificationHandler for MangaDownloader {
    fn handle_notification(&self, _notification: String) {
        settings::sync_login_url(&settings::server_base_url());
    }
}

register_source!(
    MangaDownloader,
    BaseUrlProvider,
    ListingProvider,
    DynamicListings,
    PageImageProcessor,
    ImageRequestProvider,
    NotificationHandler
);

fn source_search_filters(filters: Vec<FilterValue>) -> SourceSearchFilters {
    let mut result = SourceSearchFilters::default();
    for filter in filters {
        match filter {
            FilterValue::Sort { index, .. } => {
                result.sort = match index {
                    1 => SearchSort::RecentlyUpdated,
                    2 => SearchSort::RecentlyAdded,
                    3 => SearchSort::Title,
                    4 => SearchSort::Rating,
                    5 => SearchSort::Popular,
                    6 => SearchSort::MostViews7d,
                    7 => SearchSort::MostViews1mo,
                    8 => SearchSort::TotalViews,
                    9 => SearchSort::MostFollows,
                    _ => SearchSort::BestMatch,
                };
            }
            FilterValue::Text { id, value } if id == "sources" || id == "source" => {
                result.sources = parse_csv(&value);
            }
            FilterValue::Text { id, value } if id == "genre" => {
                result.genre = non_empty_string(value);
            }
            FilterValue::Text { id, value } if id == "language" => {
                result.language = non_empty_string(value);
            }
            FilterValue::Select { id, value }
                if (id == "type" || id == "demographic")
                    && !value.trim().is_empty()
                    && result.category.is_none() =>
            {
                result.category = Some(value);
            }
            FilterValue::Select { id, value } if id == "status" && !value.trim().is_empty() => {
                result.status = Some(value);
            }
            FilterValue::Select { id, value } if id == "age_rating" && !value.trim().is_empty() => {
                result.age_rating = Some(value);
            }
            FilterValue::Check { id, value } if id == "popular" && value > 0 => {
                result.sort = SearchSort::Popular;
            }
            _ => {}
        }
    }
    result
}

fn non_empty_string(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn source_info<'a>(sources: &'a [SourceInfoDto], source_name: &str) -> Option<&'a SourceInfoDto> {
    sources
        .iter()
        .find(|source| source.name.eq_ignore_ascii_case(source_name))
}

fn parse_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(Into::into)
        .collect()
}

fn pages_from_urls(base_url: &str, response: PageListResponse) -> Vec<Page> {
    response
        .items
        .into_iter()
        .map(|url| Page {
            content: page_content(base_url, &url),
            ..Default::default()
        })
        .collect()
}

fn page_content(base_url: &str, url: &str) -> PageContent {
    let url = absolute_url(base_url, url);
    PageContent::url(url)
}

fn is_downloaded_page_image_url(url: &str) -> bool {
    let path = url.split('?').next().unwrap_or(url);
    let Some((_, chapter_and_page)) = path.split_once(DOWNLOADED_CHAPTER_PAGE_ROUTE_PREFIX) else {
        return false;
    };
    let Some((chapter_id, page)) =
        chapter_and_page.split_once(DOWNLOADED_CHAPTER_PAGE_ROUTE_SEGMENT)
    else {
        return false;
    };

    !chapter_id.is_empty() && !page.is_empty() && page.bytes().all(|byte| byte.is_ascii_digit())
}

fn downloaded_page_retry_url(url: &str) -> String {
    if url.contains("skip_page_cache=") {
        return url.into();
    }

    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}skip_page_cache=true")
}

fn downloaded_manga_matches_remote(
    manga: &LibraryMangaDto,
    source_name: &str,
    manga_id: &str,
) -> bool {
    manga.has_downloads()
        && manga.source.eq_ignore_ascii_case(source_name)
        && manga.source_id == manga_id
}

fn empty_manga_page() -> MangaPageResult {
    MangaPageResult {
        entries: Vec::new(),
        has_next_page: false,
    }
}

fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloaded_page_image_url_matches_only_page_image_routes() {
        assert!(is_downloaded_page_image_url(
            "http://localhost:4000/v1/library/chapters/chapter-1/pages/0"
        ));
        assert!(is_downloaded_page_image_url(
            "http://localhost:4000/v1/library/chapters/chapter-1/pages/12?skip_page_cache=true"
        ));
        assert!(!is_downloaded_page_image_url(
            "http://localhost:4000/v1/library/chapters/chapter-1/pages"
        ));
        assert!(!is_downloaded_page_image_url(
            "http://localhost:4000/v1/sources/comix/chapters/chapter-1/pages/0"
        ));
        assert!(!is_downloaded_page_image_url(
            "http://localhost:4000/v1/library/chapters/chapter-1/pages/not-a-page"
        ));
    }

    #[test]
    fn downloaded_page_retry_url_skips_page_cache_once() {
        assert_eq!(
            downloaded_page_retry_url(
                "http://localhost:4000/v1/library/chapters/chapter-1/pages/0"
            ),
            "http://localhost:4000/v1/library/chapters/chapter-1/pages/0?skip_page_cache=true"
        );
        assert_eq!(
            downloaded_page_retry_url(
                "http://localhost:4000/v1/library/chapters/chapter-1/pages/0?format=avif"
            ),
            "http://localhost:4000/v1/library/chapters/chapter-1/pages/0?format=avif&skip_page_cache=true"
        );
        assert_eq!(
            downloaded_page_retry_url(
                "http://localhost:4000/v1/library/chapters/chapter-1/pages/0?skip_page_cache=true"
            ),
            "http://localhost:4000/v1/library/chapters/chapter-1/pages/0?skip_page_cache=true"
        );
    }

    #[test]
    fn downloaded_manga_match_requires_same_source_and_source_id() {
        let mut manga = LibraryMangaDto {
            id: "local-1".into(),
            source: "comix".into(),
            source_id: "remote-manga-1".into(),
            category: "downloaded".into(),
            title: "Example".into(),
            cover_url: String::new(),
            cover_proxy_url: None,
            description: String::new(),
            author: String::new(),
            genres: Vec::new(),
            status: String::new(),
            language: None,
            comic_info: Default::default(),
            downloaded_chapters: 1,
            is_nsfw: false,
        };

        assert!(downloaded_manga_matches_remote(
            &manga,
            "Comix",
            "remote-manga-1"
        ));
        assert!(!downloaded_manga_matches_remote(
            &manga,
            "other",
            "remote-manga-1"
        ));
        assert!(!downloaded_manga_matches_remote(&manga, "comix", "other"));

        manga.downloaded_chapters = 0;
        assert!(!downloaded_manga_matches_remote(
            &manga,
            "comix",
            "remote-manga-1"
        ));
    }

    #[test]
    fn default_search_loads_catalog_instead_of_returning_empty_page() {
        let filters = SourceSearchFilters::default();

        assert!(filters.needs_local_search(None));
        assert!(filters.needs_remote_search(None));
        assert!(filters.needs_local_search(Some("   ")));
        assert!(filters.needs_remote_search(Some("   ")));
    }

    #[test]
    fn local_only_filters_do_not_force_remote_search() {
        let mut filters = SourceSearchFilters {
            status: Some("completed".into()),
            ..Default::default()
        };

        assert!(filters.needs_local_search(None));
        assert!(!filters.needs_remote_search(None));

        filters.genre = Some("action".into());
        assert!(filters.needs_local_search(None));
        assert!(!filters.needs_remote_search(None));
    }
}
