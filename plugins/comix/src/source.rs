use std::collections::HashMap;

use manga_plugin_kit::prelude::{SourceCapability as KitCapability, SourceManifestBuilder};

use crate::chapters::{retain_plausible_chapters, upsert_chapter};
use crate::http::{
    BrowserCaptureKind, capture_browser_json_payloads, get_html_body, get_json_body,
    media_hotlink_ref, media_hotlink_ref_with_fragment, resolve_final_url,
};
use crate::ids::{
    build_search_page_url, build_search_urls, candidate_manga_ids, chapter_id_from_reader_url,
    chapter_indexes_url, chapter_list_urls, chapter_pages_url, chapter_reader_url,
    chapter_reader_url_from_title_url, manga_details_url, title_hash_url, title_url,
};
use crate::logging::log_plugin_loading;
use crate::manga::source::types::{Chapter, Page, SearchOptions, SourceCapability};
use crate::models::{ComixChapter, SearchResponse, SingleMangaResponse};
use crate::parse::{
    ParsedPage, parse_chapter_details_response, parse_chapter_indexes_response,
    parse_genre_terms_response, parse_manga_details_page, parse_manga_details_response,
    parse_pages_response, parse_search_response, try_parse_chapter_list_response,
};
use crate::{
    BASE_URL, ComixSource, Guest, Manga, PLUGIN_VERSION, PluginResult, SearchQuery, SearchResults,
    SourceMetadata, plugin_error,
};
use url::Url;

const CHAPTER_LIST_LIMIT: u32 = 100;
const COMIX_DESCRAMBLE_FRAGMENT: &str = "manga-server-transform=comix-descramble-5x5";

impl Guest for ComixSource {
    fn metadata() -> SourceMetadata {
        log_plugin_loading();
        let manifest = SourceManifestBuilder::new("comix", "Comix", BASE_URL, PLUGIN_VERSION)
            .search()
            .manga_details()
            .chapter_list()
            .page_list()
            .search_categories([
                "best_match",
                "updated_date",
                "created_date",
                "title_ascending",
                "year_descending",
                "average_score",
                "most_views_7d",
                "most_views_1mo",
                "total_views",
                "most_follows",
            ])
            .default_search_category("best_match")
            .supports_popular_sort()
            .homepage(BASE_URL)
            .repository("https://github.com/samuel")
            .build();

        SourceMetadata {
            id: manifest.id,
            name: manifest.name,
            base_url: manifest.base_url,
            version: manifest.version,
            api_version: manifest.api_version,
            build_metadata: manifest.build_metadata,
            homepage: manifest.homepage,
            repository: manifest.repository,
            capabilities: manifest
                .capabilities
                .into_iter()
                .map(to_source_capability)
                .collect(),
            search_options: SearchOptions {
                categories: manifest.search_options.categories,
                default_category: manifest.search_options.default_category,
                supports_popular_sort: manifest.search_options.supports_popular_sort,
            },
        }
    }

    fn search(request: SearchQuery) -> PluginResult<SearchResults> {
        let browser_url = build_search_page_url(
            &request.text,
            request.category.as_deref(),
            request.popular,
            request.page,
        );
        let urls = build_search_urls(
            &request.text,
            request.category.as_deref(),
            request.popular,
            request.page,
        );
        let mut last_error: Option<String> = None;
        let mut parsed_response: Option<SearchResponse> = None;

        tracing::trace!(
            plugin = "comix",
            query = %request.text,
            page = request.page,
            category = request.category.as_deref().unwrap_or(""),
            popular = request.popular,
            url_candidates = urls.len(),
            "Source Search Started",
        );

        match capture_browser_json_payloads(&browser_url, BrowserCaptureKind::Search) {
            Ok(payloads) => {
                for payload in payloads {
                    match parse_search_response(&payload) {
                        Ok(response) => {
                            let has_next_page = response.result.has_next_page();
                            return Ok(SearchResults {
                                items: response.result.items.into_iter().map(Into::into).collect(),
                                has_next_page,
                            });
                        }
                        Err(err) => {
                            tracing::debug!(
                                plugin = "comix",
                                query = %request.text,
                                page = request.page,
                                url = %browser_url,
                                error = %err.message,
                                "Source Browser Search Payload Parse Failed",
                            );
                            last_error = Some(format!("{} ({browser_url})", err.message));
                        }
                    }
                }
                if last_error.is_none() {
                    last_error = Some(format!("Comix browser search captured no payloads ({browser_url})"));
                }
            }
            Err(err) => {
                tracing::debug!(
                    plugin = "comix",
                    query = %request.text,
                    page = request.page,
                    url = %browser_url,
                    error = %err.message,
                    "Source Browser Search Capture Failed",
                );
                last_error = Some(format!("{} ({browser_url})", err.message));
            }
        }

        for (index, url) in urls.iter().enumerate() {
            match get_json_body(url).and_then(|body| parse_search_response(&body)) {
                Ok(response) => {
                    if index > 0 {
                        tracing::debug!(
                            plugin = "comix",
                            query = %request.text,
                            page = request.page,
                            url = %url,
                            "Source Search Fallback Succeeded",
                        );
                    }
                    parsed_response = Some(response);
                    break;
                }
                Err(err) => {
                    tracing::debug!(
                        plugin = "comix",
                        query = %request.text,
                        page = request.page,
                        url = %url,
                        error = %err,
                        "Source Search Attempt Failed",
                    );
                    last_error = Some(format!("{err} ({url})"));
                }
            }
        }

        let response = parsed_response.ok_or_else(|| {
            plugin_error(
                "search_failed",
                last_error
                    .unwrap_or_else(|| "Comix search failed for an unknown reason".to_string()),
                true,
            )
        })?;

        let has_next_page = response.result.has_next_page();

        Ok(SearchResults {
            items: response.result.items.into_iter().map(Into::into).collect(),
            has_next_page,
        })
    }

    fn get_manga(manga_id: String) -> PluginResult<Manga> {
        let mut last_error: Option<String> = None;
        let candidates = candidate_manga_ids(&manga_id, true);

        tracing::trace!(
            plugin = "comix",
            manga_id = %manga_id,
            candidate_ids = ?candidates,
            "Source Series Lookup Started",
        );

        for actual_id in candidates {
            if let Some(url) = title_hash_url(&actual_id) {
                match get_html_body(&url)
                    .and_then(|body| parse_manga_details_page(&body, &actual_id))
                {
                    Ok(response) => return Ok(complete_manga_details(&actual_id, response)),
                    Err(err) => {
                        tracing::debug!(
                            plugin = "comix",
                            requested_id = %manga_id,
                            candidate_id = %actual_id,
                            url = %url,
                            error = %err,
                            "Source Series Page Lookup Failed",
                        );
                    }
                }
            }

            let url = manga_details_url(&actual_id);

            match get_json_body(&url).and_then(|body| parse_manga_details_response(&body)) {
                Ok(response) => {
                    return Ok(complete_manga_details(&actual_id, response));
                }
                Err(err) => {
                    tracing::debug!(
                        plugin = "comix",
                        requested_id = %manga_id,
                        candidate_id = %actual_id,
                        error = %err,
                        "Source Series Lookup Failed",
                    );
                    last_error = Some(format!("{err} ({url})"));
                }
            }
        }

        Err(plugin_error(
            "manga_not_found",
            last_error.unwrap_or_else(|| format!("No manga details found for '{manga_id}'")),
            false,
        ))
    }

    fn get_chapters(manga_id: String) -> PluginResult<Vec<Chapter>> {
        let id_candidates = candidate_manga_ids(&manga_id, true);
        let mut chapter_map = HashMap::new();
        let mut last_chapter_error = None;

        for actual_id in id_candidates {
            let details_response = fetch_chapter_manga_details(&manga_id, &actual_id);
            if details_response
                .as_ref()
                .is_some_and(details_reports_no_chapters)
            {
                tracing::debug!(
                    plugin = "comix",
                    manga_id = %manga_id,
                    candidate_id = %actual_id,
                    "Source Chapter List Skipped For Title Without Chapters",
                );
                return Ok(Vec::new());
            }
            let reported_latest_chapter = details_response
                .as_ref()
                .and_then(|response| response.result.latest_chapter);
            collect_api_chapters(&manga_id, &actual_id, &mut chapter_map);
            collect_index_fallback_chapters(
                &manga_id,
                &actual_id,
                details_response.as_ref(),
                &mut chapter_map,
            );
            if let Some(error) = collect_browser_captured_chapters(
                &manga_id,
                &actual_id,
                details_response.as_ref(),
                &mut chapter_map,
            ) {
                last_chapter_error = Some(error);
            }

            if !chapter_map.is_empty() {
                attach_reader_urls(details_response.as_ref(), &mut chapter_map);
                retain_plausible_chapters(&mut chapter_map, reported_latest_chapter);
                break;
            }
        }

        if chapter_map.is_empty() {
            let suffix = last_chapter_error
                .map(|error| format!("; browser capture fallback failed: {error}"))
                .unwrap_or_default();
            return Err(plugin_error(
                "chapters_not_found",
                format!(
                    "No chapters found for manga id '{manga_id}' from either the Comix chapter API or the chapter index plus chapter detail fallback{suffix}"
                ),
                false,
            ));
        }

        let mut chapters: Vec<Chapter> = chapter_map.into_values().map(Into::into).collect();
        chapters.sort_by(|a, b| {
            b.number
                .partial_cmp(&a.number)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(chapters)
    }

    fn get_pages(chapter_id: String) -> PluginResult<Vec<Page>> {
        let numeric_chapter_id =
            chapter_id_from_reader_url(&chapter_id).or_else(|| chapter_id.parse::<i32>().ok());
        let url = numeric_chapter_id
            .map(|chapter_id| chapter_pages_url(&chapter_id.to_string()))
            .unwrap_or_default();
        tracing::trace!(
            plugin = "comix",
            chapter_id = %chapter_id,
            url = %url,
            "Source Page List Started",
        );
        let pages = match (!url.is_empty())
            .then(|| get_json_body(&url).and_then(|body| parse_pages_response(&body)))
        {
            Some(Ok(pages))
                if is_reader_url(&chapter_id) && pages_need_browser_descramble(&pages) =>
            {
                match fetch_browser_captured_pages(&chapter_id) {
                    Ok(captured_pages) if pages_have_descramble_maps(&captured_pages) => {
                        captured_pages
                    }
                    Ok(_) | Err(_) => pages,
                }
            }
            Some(Ok(pages)) => pages,
            Some(Err(err)) if is_reader_url(&chapter_id) => {
                tracing::debug!(
                    plugin = "comix",
                    chapter_id = %chapter_id,
                    error = %err.message,
                    "Source Page List API Failed; Trying Browser Capture",
                );
                fetch_browser_captured_pages(&chapter_id)?
            }
            Some(Err(err)) => return Err(err),
            None if is_reader_url(&chapter_id) => fetch_browser_captured_pages(&chapter_id)?,
            None => {
                return Err(plugin_error(
                    "invalid_chapter_id",
                    format!("Comix chapter id '{chapter_id}' was not numeric or a reader URL"),
                    false,
                ));
            }
        };
        tracing::trace!(
            plugin = "comix",
            chapter_id = %chapter_id,
            page_count = pages.len(),
            "Source Page List Resolved",
        );
        Ok(pages
            .into_iter()
            .enumerate()
            .map(|(index, page)| Page {
                index: u32::try_from(index)
                    .expect("page index should fit in the plugin ABI u32 range"),
                image: page_media_ref(&page),
            })
            .collect())
    }
}

fn complete_manga_details(actual_id: &str, response: SingleMangaResponse) -> Manga {
    let term_ids = response.result.term_ids.clone();
    let mut manga: Manga = response.result.into();
    if manga.genres.is_empty() {
        match resolve_genres(&term_ids) {
            Ok(genres) => manga.genres = genres,
            Err(err) => {
                tracing::warn!(
                    plugin = "comix",
                    manga_id = %actual_id,
                    error = %err.message,
                    "Source Genre Resolution Failed",
                );
            }
        }
    }
    manga
}

fn to_source_capability(capability: KitCapability) -> SourceCapability {
    match capability {
        KitCapability::Search => SourceCapability::Search,
        KitCapability::MangaDetails => SourceCapability::MangaDetails,
        KitCapability::ChapterList => SourceCapability::ChapterList,
        KitCapability::PageList => SourceCapability::PageList,
    }
}

fn fetch_chapter_manga_details(
    requested_manga_id: &str,
    actual_id: &str,
) -> Option<SingleMangaResponse> {
    let details_response = title_hash_url(actual_id)
        .and_then(|url| {
            get_html_body(&url)
                .ok()
                .and_then(|body| parse_manga_details_page(&body, actual_id).ok())
        })
        .or_else(|| {
            get_json_body(&manga_details_url(actual_id))
                .ok()
                .and_then(|body| parse_manga_details_response(&body).ok())
        });
    tracing::debug!(
        plugin = "comix",
        requested_manga_id,
        candidate_id = %actual_id,
        details_found = details_response.is_some(),
        "Source Chapter List Details Lookup Finished",
    );
    details_response
}

fn details_reports_no_chapters(details: &SingleMangaResponse) -> bool {
    details.result.has_chapters == Some(false)
}

fn collect_api_chapters(
    requested_manga_id: &str,
    actual_id: &str,
    chapter_map: &mut HashMap<String, ComixChapter>,
) {
    let mut page = 1;
    while let Some(has_next_page) =
        fetch_chapter_list_page(requested_manga_id, actual_id, page, chapter_map)
    {
        if !has_next_page {
            break;
        }
        page += 1;
    }
}

fn fetch_chapter_list_page(
    requested_manga_id: &str,
    actual_id: &str,
    page: u32,
    chapter_map: &mut HashMap<String, ComixChapter>,
) -> Option<bool> {
    for url in chapter_list_urls(actual_id, CHAPTER_LIST_LIMIT, page) {
        let Ok(json_str) = get_json_body(&url) else {
            tracing::debug!(
                plugin = "comix",
                requested_manga_id,
                candidate_id = %actual_id,
                page,
                url = %url,
                "Source Chapter List API Fetch Failed",
            );
            continue;
        };
        if json_str.trim().is_empty() {
            tracing::debug!(
                plugin = "comix",
                requested_manga_id,
                candidate_id = %actual_id,
                page,
                url = %url,
                "Source Chapter List API Returned Empty Body",
            );
            continue;
        }

        if let Some((chapters, has_next_page)) = try_parse_chapter_list_response(&json_str) {
            for chapter in chapters {
                upsert_chapter(chapter_map, chapter);
            }
            return Some(has_next_page);
        }

        tracing::debug!(
            plugin = "comix",
            requested_manga_id,
            candidate_id = %actual_id,
            page,
            url = %url,
            body_prefix = %json_str.chars().take(180).collect::<String>(),
            "Source Chapter List API Parse Failed",
        );
    }

    None
}

fn collect_index_fallback_chapters(
    requested_manga_id: &str,
    actual_id: &str,
    details: Option<&SingleMangaResponse>,
    chapter_map: &mut HashMap<String, ComixChapter>,
) {
    if !chapter_map.is_empty() {
        return;
    }

    let Some(details) = details else {
        return;
    };
    let hash_id = details.result.hash_id.as_deref().unwrap_or(actual_id);
    let Ok(indexes_json) = get_json_body(&chapter_indexes_url(hash_id)) else {
        return;
    };
    let Some(indexes) = parse_chapter_indexes_response(&indexes_json) else {
        return;
    };

    tracing::debug!(
        plugin = "comix",
        manga_id = %requested_manga_id,
        candidate_id = %actual_id,
        chapter_index_count = indexes.len(),
        "Source Chapter Index Fallback Started",
    );

    let slug = details.result.slug.as_deref();
    let mut fallback_count = 0usize;
    for chapter_index in &indexes {
        if let Some(chapter) = fetch_index_fallback_chapter(
            requested_manga_id,
            actual_id,
            hash_id,
            slug,
            chapter_index,
        ) {
            upsert_chapter(chapter_map, chapter);
            fallback_count += 1;
        }
    }

    if !chapter_map.is_empty() {
        tracing::debug!(
            plugin = "comix",
            manga_id = %requested_manga_id,
            candidate_id = %actual_id,
            fallback_count,
            chapter_count = chapter_map.len(),
            "Source Chapter Index Fallback Succeeded",
        );
    }
}

fn collect_browser_captured_chapters(
    requested_manga_id: &str,
    actual_id: &str,
    details: Option<&SingleMangaResponse>,
    chapter_map: &mut HashMap<String, ComixChapter>,
) -> Option<String> {
    if !chapter_map.is_empty() {
        return None;
    }

    let details = details?;
    let url = manga_reader_root_url(details, actual_id)?;

    let payloads = match capture_browser_json_payloads(&url, BrowserCaptureKind::ChapterList) {
        Ok(payloads) => payloads,
        Err(err) => {
            tracing::warn!(
                plugin = "comix",
                manga_id = %requested_manga_id,
                candidate_id = %actual_id,
                url = %url,
                error = %err.message,
                "Source Chapter Browser Capture Failed",
            );
            return Some(err.message);
        }
    };

    for payload in payloads {
        if let Some((chapters, _)) = try_parse_chapter_list_response(&payload) {
            for chapter in chapters {
                upsert_chapter(chapter_map, chapter);
            }
        }
    }

    if !chapter_map.is_empty() {
        tracing::debug!(
            plugin = "comix",
            manga_id = %requested_manga_id,
            candidate_id = %actual_id,
            chapter_count = chapter_map.len(),
            "Source Chapter Browser Capture Succeeded",
        );
    }

    chapter_map
        .is_empty()
        .then(|| format!("No chapter list payloads could be parsed from browser capture for {url}"))
}

fn attach_reader_urls(
    details: Option<&SingleMangaResponse>,
    chapter_map: &mut HashMap<String, ComixChapter>,
) {
    let Some(details) = details else {
        return;
    };
    let hash_id = details.result.hash_id.as_deref();
    let slug = details.result.slug.as_deref();
    let title_url = details
        .result
        .url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(absolute_comix_url)
        .or_else(|| hash_id.and_then(|hash_id| title_url(hash_id, slug)));
    let Some(title_url) = title_url else {
        return;
    };

    for chapter in chapter_map.values_mut() {
        if chapter.reader_url.is_none() {
            chapter.reader_url =
                chapter_reader_url_from_title_url(&title_url, chapter.chapter_id, chapter.number);
        }
    }
}

fn manga_reader_root_url(details: &SingleMangaResponse, actual_id: &str) -> Option<String> {
    if let Some(url) = details
        .result
        .url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(absolute_comix_url)
    {
        return Some(url);
    }

    let hash_id = details.result.hash_id.as_deref().unwrap_or(actual_id);
    title_url(hash_id, details.result.slug.as_deref())
}

fn absolute_comix_url(url: &str) -> Option<String> {
    Url::parse(BASE_URL).ok()?.join(url).ok().map(Into::into)
}

fn fetch_index_fallback_chapter(
    requested_manga_id: &str,
    actual_id: &str,
    hash_id: &str,
    slug: Option<&str>,
    chapter_index: &crate::models::ChapterIndex,
) -> Option<ComixChapter> {
    let reader_url = chapter_reader_url(hash_id, slug, chapter_index.number)?;
    let final_url = resolve_fallback_reader_url(
        requested_manga_id,
        actual_id,
        chapter_index.number,
        &reader_url,
    )?;
    let chapter_id = parse_fallback_chapter_id(
        requested_manga_id,
        actual_id,
        chapter_index.number,
        &reader_url,
        &final_url,
    )?;
    fetch_fallback_chapter_detail(
        requested_manga_id,
        actual_id,
        chapter_index.number,
        chapter_id,
    )
    .map(|mut chapter| {
        chapter.reader_url = Some(final_url);
        chapter
    })
}

fn resolve_fallback_reader_url(
    requested_manga_id: &str,
    actual_id: &str,
    chapter_number: f32,
    reader_url: &str,
) -> Option<String> {
    if let Ok(final_url) = resolve_final_url(reader_url) {
        return Some(final_url);
    }

    tracing::debug!(
        plugin = "comix",
        manga_id = %requested_manga_id,
        candidate_id = %actual_id,
        chapter_number,
        reader_url,
        "Source Chapter Redirect Resolve Failed",
    );
    None
}

fn parse_fallback_chapter_id(
    requested_manga_id: &str,
    actual_id: &str,
    chapter_number: f32,
    reader_url: &str,
    final_url: &str,
) -> Option<i32> {
    if let Some(chapter_id) = chapter_id_from_reader_url(final_url) {
        return Some(chapter_id);
    }

    tracing::debug!(
        plugin = "comix",
        manga_id = %requested_manga_id,
        candidate_id = %actual_id,
        chapter_number,
        reader_url,
        final_url,
        "Source Chapter Redirect Missing Chapter Id",
    );
    None
}

fn fetch_fallback_chapter_detail(
    requested_manga_id: &str,
    actual_id: &str,
    chapter_number: f32,
    chapter_id: i32,
) -> Option<ComixChapter> {
    let Ok(chapter_json) = get_json_body(&chapter_pages_url(&chapter_id.to_string())) else {
        tracing::debug!(
            plugin = "comix",
            manga_id = %requested_manga_id,
            candidate_id = %actual_id,
            chapter_number,
            chapter_id,
            "Source Chapter Detail API Fetch Failed",
        );
        return None;
    };
    let Some(chapter) = parse_chapter_details_response(&chapter_json) else {
        tracing::debug!(
            plugin = "comix",
            manga_id = %requested_manga_id,
            candidate_id = %actual_id,
            chapter_number,
            chapter_id,
            "Source Chapter Detail API Parse Failed",
        );
        return None;
    };
    Some(chapter)
}

fn fetch_browser_captured_pages(reader_url: &str) -> PluginResult<Vec<ParsedPage>> {
    let url = normalize_reader_url(reader_url);
    let payloads = capture_browser_json_payloads(&url, BrowserCaptureKind::PageList)?;
    for payload in payloads {
        if let Ok(pages) = parse_pages_response(&payload)
            && !pages.is_empty()
        {
            return Ok(pages);
        }
    }

    Err(plugin_error(
        "page_urls_missing",
        format!("Browser capture did not find any page URLs for '{reader_url}'"),
        true,
    ))
}

fn page_media_ref(page: &ParsedPage) -> crate::manga::source::types::MediaRef {
    if page.descramble {
        let fragment = page.descramble_map.as_ref().map_or_else(
            || COMIX_DESCRAMBLE_FRAGMENT.to_string(),
            |map| descramble_fragment(map),
        );
        media_hotlink_ref_with_fragment(&page.url, BASE_URL, &fragment)
    } else {
        media_hotlink_ref(&page.url, BASE_URL)
    }
}

fn pages_need_browser_descramble(pages: &[ParsedPage]) -> bool {
    pages
        .iter()
        .any(|page| page.source_scrambled && !page.descramble)
}

fn pages_have_descramble_maps(pages: &[ParsedPage]) -> bool {
    pages.iter().any(|page| page.descramble_map.is_some())
}

fn descramble_fragment(map: &[usize]) -> String {
    let values = map
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!("{COMIX_DESCRAMBLE_FRAGMENT}:{values}")
}

fn normalize_reader_url(value: &str) -> String {
    if value.starts_with("http://") || value.starts_with("https://") {
        value.to_string()
    } else if value.starts_with('/') {
        format!("{BASE_URL}{value}")
    } else {
        format!("{BASE_URL}/{value}")
    }
}

fn is_reader_url(value: &str) -> bool {
    value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("title/")
        || value.starts_with("/title/")
}

impl From<ComixChapter> for Chapter {
    fn from(value: ComixChapter) -> Self {
        let published_at = value.published_at();
        Self {
            id: value
                .reader_url
                .unwrap_or_else(|| value.chapter_id.to_string()),
            title: if value.name.is_empty() {
                format!("Chapter {}", value.number)
            } else {
                value.name
            },
            number: f64::from(value.number),
            volume: None,
            published_at,
        }
    }
}

fn resolve_genres(term_ids: &[i32]) -> PluginResult<Vec<String>> {
    if term_ids.is_empty() {
        return Ok(Vec::new());
    }

    let genre_terms = get_json_body(&format!("{BASE_URL}/api/v2/terms?type=genre"))
        .and_then(|body| parse_genre_terms_response(&body))?;
    let desired_ids = term_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();

    Ok(genre_terms
        .into_iter()
        .filter(|term| desired_ids.contains(&term.term_id))
        .map(|term| term.title)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{absolute_comix_url, details_reports_no_chapters, pages_need_browser_descramble};
    use crate::parse::{ParsedPage, parse_manga_details_response};

    #[test]
    fn absolute_comix_url_expands_site_relative_paths() {
        assert_eq!(
            absolute_comix_url("/title/gx6x-prices-crash-i-rise").as_deref(),
            Some("https://comix.to/title/gx6x-prices-crash-i-rise")
        );
    }

    #[test]
    fn absolute_comix_url_keeps_absolute_urls() {
        assert_eq!(
            absolute_comix_url("https://comix.to/title/yegj1-i-became-a-hatchling").as_deref(),
            Some("https://comix.to/title/yegj1-i-became-a-hatchling")
        );
    }

    #[test]
    fn details_reports_no_chapters_when_comix_marks_title_empty() {
        let details = parse_manga_details_response(
            r#"{"status":"ok","result":{"id":682,"hid":"mw8z","title":"Kimi","hasChapters":false,"latestChapter":0}}"#,
        )
        .expect("fixture should parse");

        assert!(details_reports_no_chapters(&details));
    }

    #[test]
    fn scrambled_api_pages_without_maps_need_browser_descramble() {
        let pages = vec![ParsedPage {
            url: "https://static.example.test/sii/bexample/001.webp".to_string(),
            descramble: false,
            descramble_map: None,
            source_scrambled: true,
        }];

        assert!(pages_need_browser_descramble(&pages));
    }

    #[test]
    fn mapped_scrambled_pages_do_not_need_browser_descramble() {
        let pages = vec![ParsedPage {
            url: "https://static.example.test/si/bexample/001.webp".to_string(),
            descramble: true,
            descramble_map: Some((0..25).collect()),
            source_scrambled: true,
        }];

        assert!(!pages_need_browser_descramble(&pages));
    }
}
