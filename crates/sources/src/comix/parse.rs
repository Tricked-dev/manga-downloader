use serde::Deserialize;

use crate::comix::models::{
    ApiTerm, ChapterDetailsResponse, ChapterImage, ChapterIndex, ChapterListResponse,
    ChapterResponse, ComixChapter, ComixManga, MangaItems, Pagination, SearchResponse, SingleManga,
    SingleMangaResponse, SingleMangaWithChaptersResponse,
};
use crate::comix::{SourceResult, non_retryable_source_error as source_error};

const PAGE_URL_PATHS: &[&[&str]] = &[
    &["result"],
    &["result", "images"],
    &["result", "pages"],
    &["result", "data"],
    &["result", "items"],
    &["result", "data", "items"],
    &["images"],
    &["pages"],
    &["data"],
    &["items"],
    &["chapter", "images"],
    &["chapter", "pages"],
    &["chapter", "data"],
    &["chapter", "items"],
    &["result", "chapter", "images"],
    &["result", "chapter", "pages"],
    &["result", "chapter", "data"],
    &["result", "chapter", "items"],
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedPage {
    pub url: String,
    pub descramble: bool,
    pub descramble_map: Option<Vec<usize>>,
    pub source_scrambled: bool,
}

pub fn parse_search_response(body: &str) -> SourceResult<SearchResponse> {
    let normalized = extract_json_body(body);
    let value = parse_json_value(&normalized, "search response", "parse_search_response")?;

    if let Some(api_error) = extract_api_error_message(&value) {
        return Err(source_error(
            "search_api_error",
            format!("Comix search API returned an error: {api_error}"),
        ));
    }

    let result = value.get("result").unwrap_or(&value);
    let items_value = search_items_value(&value, result).ok_or_else(|| {
        source_error(
            "parse_search_response",
            format!(
                "Search response did not contain any manga items (top-level keys: {}; result keys: {})",
                summarize_object_keys(&value),
                summarize_object_keys(result)
            ),
        )
    })?;

    let raw_items = items_value.as_array().ok_or_else(|| {
        source_error(
            "parse_search_response",
            format!(
                "Search items payload was not an array (type: {})",
                json_type_name(items_value)
            ),
        )
    })?;

    let items: Vec<ComixManga> = raw_items
        .iter()
        .filter_map(|raw_item| ComixManga::deserialize(raw_item).ok())
        .collect();

    let skipped_items = raw_items.len().saturating_sub(items.len());

    if items.is_empty() && !raw_items.is_empty() {
        return Err(source_error(
            "parse_search_response",
            "Failed to parse any manga records from the search response".to_string(),
        ));
    }

    if skipped_items > 0 {
        tracing::debug!(
            plugin = "comix",
            skipped_items,
            parsed_items = items.len(),
            "Skipped malformed manga records while parsing search results",
        );
    }

    Ok(SearchResponse {
        result: MangaItems {
            items,
            pagination: extract_pagination(result, &value),
            meta: extract_meta(result, &value),
        },
    })
}

pub fn parse_manga_details_response(body: &str) -> SourceResult<SingleMangaResponse> {
    let normalized = extract_json_body(body);
    let value = parse_json_value(&normalized, "manga details", "parse_manga_details")?;

    if let Some(api_error) = extract_api_error_message(&value) {
        return Err(source_error(
            "manga_api_error",
            format!("Comix manga API returned an error: {api_error}"),
        ));
    }

    let manga_value = manga_details_value(&value).ok_or_else(|| {
        source_error(
            "parse_manga_details",
            format!(
                "Manga details response did not contain a manga object (top-level keys: {}; result keys: {})",
                summarize_object_keys(&value),
                value.get("result")
                    .map_or_else(|| "<missing>".to_string(), summarize_object_keys)
            ),
        )
    })?;

    let manga = SingleManga::deserialize(manga_value).map_err(|err| {
        source_error(
            "parse_manga_details",
            format!("Failed to parse manga details: {err}"),
        )
    })?;

    Ok(SingleMangaResponse { result: manga })
}

pub fn parse_manga_details_page(body: &str, hash_id: &str) -> SourceResult<SingleMangaResponse> {
    let initial_data = extract_initial_data_json(body)?;
    let value = parse_json_value(
        initial_data,
        "manga details page",
        "parse_manga_details_page",
    )?;
    let queries = value
        .get("queries")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            source_error(
                "parse_manga_details_page",
                "Comix title page initial data did not contain a queries object".to_string(),
            )
        })?;

    let detail_key = serde_json::to_string(&["manga", "detail", hash_id]).map_err(|err| {
        source_error(
            "parse_manga_details_page",
            format!("Failed to build Comix details query key: {err}"),
        )
    })?;
    let manga_value = queries
        .get(&detail_key)
        .or_else(|| {
            queries
                .iter()
                .find(|(key, value)| {
                    key.contains("\"manga\"")
                        && key.contains("\"detail\"")
                        && looks_like_manga_object(value)
                })
                .map(|(_, value)| value)
        })
        .ok_or_else(|| {
            source_error(
                "parse_manga_details_page",
                format!("Comix title page did not contain manga details for '{hash_id}'"),
            )
        })?;

    let manga = SingleManga::deserialize(manga_value).map_err(|err| {
        source_error(
            "parse_manga_details_page",
            format!("Failed to parse Comix title page manga details: {err}"),
        )
    })?;

    Ok(SingleMangaResponse { result: manga })
}

pub fn parse_pages_response(body: &str) -> SourceResult<Vec<ParsedPage>> {
    let normalized = extract_json_body(body);

    if let Ok(response) = serde_json::from_str::<ChapterResponse>(&normalized)
        && let Some(result) = response.result
    {
        let urls: Vec<ParsedPage> = result
            .images
            .into_iter()
            .map(|img| {
                let descramble = page_image_is_scrambled(&img);
                let descramble_map = descramble.then(|| image_descramble_map(&img)).flatten();
                let url = if descramble_map.is_some() {
                    comix_tile_url(&img.url)
                } else if descramble {
                    comix_unscrambled_url(&img.url)
                } else {
                    img.url
                };
                ParsedPage {
                    url,
                    descramble: descramble && descramble_map.is_some(),
                    descramble_map,
                    source_scrambled: descramble,
                }
            })
            .collect();
        if !urls.is_empty() {
            return Ok(urls);
        }
    }

    let value = serde_json::from_str::<serde_json::Value>(&normalized).map_err(|err| {
        source_error(
            "parse_page_list",
            format!("Failed to parse chapter page list: {err}"),
        )
    })?;

    if let Some(api_error) = extract_api_error_message(&value) {
        return Err(source_error(
            "chapter_api_error",
            format!("Comix chapter API returned an error: {api_error}"),
        ));
    }

    if let Some(urls) = extract_page_urls_with_base(&value) {
        return Ok(urls);
    }

    extract_page_urls(&value).map(pages_from_urls).ok_or_else(|| {
        let result_keys = value
            .get("result")
            .map_or_else(|| "<missing>".to_string(), summarize_object_keys);
        source_error(
            "page_urls_missing",
            format!(
                "Chapter response did not contain any image URLs (top-level keys: {}; result keys: {})",
                summarize_object_keys(&value),
                result_keys
            ),
        )
    })
}

pub fn try_parse_chapter_list_response(body: &str) -> Option<(Vec<ComixChapter>, bool)> {
    let normalized = extract_json_body(body);

    if let Ok(response) = serde_json::from_str::<ChapterDetailsResponse>(&normalized) {
        let has_next_page = response.result.has_next_page();
        return Some((response.result.items, has_next_page));
    }

    if let Ok(response) = serde_json::from_str::<ChapterListResponse>(&normalized) {
        return Some((response.result, false));
    }

    if let Ok(response) = serde_json::from_str::<SingleMangaWithChaptersResponse>(&normalized) {
        return Some((response.result.chapters, false));
    }

    None
}

pub fn parse_chapter_details_response(body: &str) -> Option<ComixChapter> {
    parse_result_or_root(body)
}

pub fn parse_chapter_indexes_response(body: &str) -> Option<Vec<ChapterIndex>> {
    parse_result_or_root(body)
}

pub fn parse_genre_terms_response(body: &str) -> SourceResult<Vec<ApiTerm>> {
    let normalized = extract_json_body(body);
    let value = parse_json_value(&normalized, "genre terms", "parse_genre_terms")?;

    if let Some(api_error) = extract_api_error_message(&value) {
        return Err(source_error(
            "genre_terms_api_error",
            format!("Comix genre terms API returned an error: {api_error}"),
        ));
    }

    let items = value
        .get("result")
        .and_then(|result| result.get("items"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            source_error(
                "parse_genre_terms",
                format!(
                    "Genre terms response did not contain an items array (top-level keys: {}; result keys: {})",
                    summarize_object_keys(&value),
                    value
                        .get("result")
                        .map_or_else(|| "<missing>".to_string(), summarize_object_keys)
                ),
            )
        })?;

    let parsed = items
        .iter()
        .filter_map(|item| {
            let term_id = item
                .get("term_id")
                .or_else(|| item.get("id"))
                .and_then(serde_json::Value::as_i64)
                .and_then(|value| i32::try_from(value).ok());
            let title = item
                .get("title")
                .or_else(|| item.get("name"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);

            match (term_id, title) {
                (Some(term_id), Some(title)) => Some(ApiTerm { term_id, title }),
                _ => None,
            }
        })
        .collect();

    Ok(parsed)
}

fn extract_api_error_message(value: &serde_json::Value) -> Option<String> {
    let status = value.get("status")?;
    let is_error_status = match status {
        serde_json::Value::Number(number) => number.as_i64().is_some_and(|code| code >= 400),
        serde_json::Value::String(text) => !text.eq_ignore_ascii_case("ok") && text != "200",
        _ => false,
    };

    if !is_error_status {
        return None;
    }

    let message = value
        .get("message")
        .and_then(|candidate| candidate.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string);

    let messages = value
        .get("messages")
        .and_then(|candidate| candidate.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|text| !text.is_empty());

    message.or(messages)
}

fn parse_json_value(body: &str, label: &str, code: &str) -> SourceResult<serde_json::Value> {
    serde_json::from_str(body)
        .map_err(|err| source_error(code, format!("Failed to parse {label}: {err}")))
}

fn parse_result_or_root<T>(body: &str) -> Option<T>
where
    T: serde::de::DeserializeOwned,
{
    let normalized = extract_json_body(body);
    let value = serde_json::from_str::<serde_json::Value>(&normalized).ok()?;
    [value.get("result"), Some(&value)]
        .into_iter()
        .flatten()
        .find_map(|candidate| T::deserialize(candidate).ok())
}

pub fn extract_json_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return trimmed.to_string();
    }

    if let Some(start_idx) = trimmed.find("<pre")
        && let Some(open_end) = trimmed[start_idx..].find('>')
    {
        let content_start = start_idx + open_end + 1;
        if let Some(close_idx) = trimmed[content_start..].find("</pre>") {
            let content_end = content_start + close_idx;
            return html_unescape(&trimmed[content_start..content_end]);
        }
    }

    trimmed.to_string()
}

fn html_unescape(input: &str) -> String {
    input
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn search_items_value<'a>(
    root: &'a serde_json::Value,
    result: &'a serde_json::Value,
) -> Option<&'a serde_json::Value> {
    [
        result.get("items"),
        result.get("data").and_then(|value| value.get("items")),
        result
            .get("data")
            .filter(|value| matches!(value, serde_json::Value::Array(_))),
        root.get("items"),
        root.get("data").and_then(|value| value.get("items")),
        root.get("data")
            .filter(|value| matches!(value, serde_json::Value::Array(_))),
        matches!(result, serde_json::Value::Array(_)).then_some(result),
        matches!(root, serde_json::Value::Array(_)).then_some(root),
    ]
    .into_iter()
    .flatten()
    .next()
}

fn extract_pagination(result: &serde_json::Value, root: &serde_json::Value) -> Pagination {
    [
        result.get("pagination"),
        result.get("meta"),
        result.get("data").and_then(|value| value.get("pagination")),
        result.get("data").and_then(|value| value.get("meta")),
        root.get("pagination"),
        root.get("meta"),
        root.get("data").and_then(|value| value.get("pagination")),
        root.get("data").and_then(|value| value.get("meta")),
    ]
    .into_iter()
    .flatten()
    .find_map(|value| Pagination::deserialize(value).ok())
    .unwrap_or(Pagination {
        current_page: 1,
        last_page: 1,
        has_next: false,
    })
}

fn extract_meta(result: &serde_json::Value, root: &serde_json::Value) -> Option<Pagination> {
    [
        result.get("meta"),
        result.get("data").and_then(|value| value.get("meta")),
        root.get("meta"),
        root.get("data").and_then(|value| value.get("meta")),
    ]
    .into_iter()
    .flatten()
    .find_map(|value| Pagination::deserialize(value).ok())
}

fn manga_details_value(value: &serde_json::Value) -> Option<&serde_json::Value> {
    [
        value.get("result").and_then(|result| result.get("comic")),
        value.get("comic"),
        value.get("result"),
        Some(value),
    ]
    .into_iter()
    .flatten()
    .find(|candidate| looks_like_manga_object(candidate))
}

fn extract_initial_data_json(body: &str) -> SourceResult<&str> {
    let marker = "id=\"initial-data\"";
    let marker_start = body.find(marker).ok_or_else(|| {
        source_error(
            "parse_manga_details_page",
            "Comix title page did not contain the initial-data script".to_string(),
        )
    })?;
    let script_start = body[..marker_start].rfind("<script").ok_or_else(|| {
        source_error(
            "parse_manga_details_page",
            "Comix title page initial-data script start was malformed".to_string(),
        )
    })?;
    let content_start = body[script_start..]
        .find('>')
        .map(|index| script_start + index + 1)
        .ok_or_else(|| {
            source_error(
                "parse_manga_details_page",
                "Comix title page initial-data script did not have an opening tag end".to_string(),
            )
        })?;
    let content_end = body[content_start..]
        .find("</script>")
        .map(|index| content_start + index)
        .ok_or_else(|| {
            source_error(
                "parse_manga_details_page",
                "Comix title page initial-data script did not have a closing tag".to_string(),
            )
        })?;

    Ok(body[content_start..content_end].trim())
}

fn looks_like_manga_object(value: &serde_json::Value) -> bool {
    let Some(map) = value.as_object() else {
        return false;
    };

    map.contains_key("title")
        || map.contains_key("slug")
        || map.contains_key("hash_id")
        || map.contains_key("manga_id")
        || map.contains_key("id")
}

fn extract_page_urls(value: &serde_json::Value) -> Option<Vec<String>> {
    for path in PAGE_URL_PATHS {
        if let Some(urls) = extract_urls_at_path(value, path)
            && !urls.is_empty()
        {
            return Some(urls);
        }
    }

    find_image_url_array(value).or_else(|| collect_all_image_like_urls(value))
}

fn extract_page_urls_with_base(value: &serde_json::Value) -> Option<Vec<ParsedPage>> {
    'paths: for path in [
        &["result", "pages"][..],
        &["pages"][..],
        &["chapter", "pages"][..],
        &["result", "chapter", "pages"][..],
    ] {
        let mut current = value;
        for segment in path {
            let Some(next) = current.get(*segment) else {
                continue 'paths;
            };
            current = next;
        }

        if let Some(urls) = urls_from_base_items(current)
            && !urls.is_empty()
        {
            return Some(urls);
        }
    }

    None
}

fn urls_from_base_items(value: &serde_json::Value) -> Option<Vec<ParsedPage>> {
    let object = value.as_object()?;
    let base_url = object
        .get("baseUrl")
        .or_else(|| object.get("base_url"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let tile_base_url = base_url.replace("/i/b", "/si/b").replace("/i/h", "/si/h");
    let unscrambled_base_url = base_url
        .replace("/si/b", "/sii/b")
        .replace("/si/h", "/sii/h")
        .replace("/i/b", "/sii/b")
        .replace("/i/h", "/sii/h");
    let items = object.get("items")?.as_array()?;

    let urls = items
        .iter()
        .filter_map(|item| {
            let (url, descramble, descramble_map) = match item {
                serde_json::Value::String(url) => (url.as_str(), false, None),
                serde_json::Value::Object(map) => {
                    let url = [
                        "url",
                        "src",
                        "image_url",
                        "imageUrl",
                        "page_url",
                        "pageUrl",
                        "path",
                    ]
                    .iter()
                    .filter_map(|key| map.get(*key))
                    .find_map(serde_json::Value::as_str)?;
                    let descramble = map
                        .get("s")
                        .or_else(|| map.get("scramble"))
                        .or_else(|| map.get("scrambled"))
                        .is_some_and(is_scrambled_page_flag);
                    let descramble_map = descramble.then(|| map_descramble_map(map)).flatten();
                    (url, descramble, descramble_map)
                }
                _ => return None,
            };
            let base_url = if descramble_map.is_some() {
                tile_base_url.as_str()
            } else if descramble {
                unscrambled_base_url.as_str()
            } else {
                base_url
            };
            let url = absolute_or_joined_url(base_url, url);
            let source_scrambled = descramble;
            let descramble = descramble && descramble_map.is_some();
            Some(ParsedPage {
                url,
                descramble,
                descramble_map,
                source_scrambled,
            })
        })
        .filter(|page| looks_like_image_url(&page.url))
        .collect::<Vec<_>>();

    (!urls.is_empty()).then_some(urls)
}

fn comix_unscrambled_url(url: &str) -> String {
    url.replace("/si/b", "/sii/b")
        .replace("/si/h", "/sii/h")
        .replace("/i/b", "/sii/b")
        .replace("/i/h", "/sii/h")
}

fn comix_tile_url(url: &str) -> String {
    url.replace("/sii/b", "/si/b")
        .replace("/sii/h", "/si/h")
        .replace("/i/b", "/si/b")
        .replace("/i/h", "/si/h")
}

fn pages_from_urls(urls: Vec<String>) -> Vec<ParsedPage> {
    urls.into_iter()
        .map(|url| ParsedPage {
            url,
            descramble: false,
            descramble_map: None,
            source_scrambled: false,
        })
        .collect()
}

fn is_scrambled_page_flag(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::Number(value) => value.as_i64() == Some(1),
        serde_json::Value::String(value) => matches!(value.as_str(), "1" | "true" | "yes"),
        _ => false,
    }
}

fn page_image_is_scrambled(page: &ChapterImage) -> bool {
    page.s
        .as_ref()
        .or(page.scramble.as_ref())
        .or(page.scrambled.as_ref())
        .is_some_and(is_scrambled_page_flag)
}

fn image_descramble_map(page: &ChapterImage) -> Option<Vec<usize>> {
    page.manga_server_descramble_map
        .as_ref()
        .or(page.descramble_map.as_ref())
        .and_then(valid_descramble_map)
}

fn map_descramble_map(map: &serde_json::Map<String, serde_json::Value>) -> Option<Vec<usize>> {
    map.get("manga_server_descramble_map")
        .or_else(|| map.get("descrambleMap"))
        .or_else(|| map.get("descramble_map"))
        .and_then(valid_descramble_map)
}

fn valid_descramble_map(value: &serde_json::Value) -> Option<Vec<usize>> {
    let values = value.as_array()?;
    if values.len() != 25 {
        return None;
    }

    let mut seen = [false; 25];
    let mut map = Vec::with_capacity(25);
    for value in values {
        let value = value.as_u64()?;
        let value = usize::try_from(value).ok()?;
        if value >= 25 || seen[value] {
            return None;
        }
        seen[value] = true;
        map.push(value);
    }
    Some(map)
}

fn absolute_or_joined_url(base_url: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") || base_url.is_empty() {
        return url.to_string();
    }

    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        url.trim_start_matches('/')
    )
}

fn extract_urls_at_path(value: &serde_json::Value, path: &[&str]) -> Option<Vec<String>> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }

    urls_from_array(current)
}

fn urls_from_array(value: &serde_json::Value) -> Option<Vec<String>> {
    let array = value.as_array()?;
    let urls = array
        .iter()
        .filter_map(extract_url_from_value)
        .collect::<Vec<_>>();

    (!urls.is_empty()).then_some(urls)
}

fn extract_url_from_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(url) if looks_like_image_url(url) => Some(url.clone()),
        serde_json::Value::Object(map) => [
            "url",
            "src",
            "image_url",
            "imageUrl",
            "page_url",
            "pageUrl",
            "original_url",
            "originalUrl",
            "original",
            "storage_url",
            "storageUrl",
            "file",
        ]
        .iter()
        .filter_map(|key| map.get(*key))
        .find_map(|candidate| candidate.as_str())
        .filter(|url| looks_like_image_url(url))
        .map(ToString::to_string),
        _ => None,
    }
}

fn looks_like_image_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://"))
        && (lower.contains(".jpg")
            || lower.contains(".jpeg")
            || lower.contains(".png")
            || lower.contains(".webp")
            || lower.contains(".avif")
            || lower.contains("/images/"))
}

fn find_image_url_array(value: &serde_json::Value) -> Option<Vec<String>> {
    match value {
        serde_json::Value::Array(_) => urls_from_array(value),
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let key_lower = key.to_ascii_lowercase();
                if (key_lower.contains("image") || key_lower.contains("page"))
                    && let Some(urls) = urls_from_array(child)
                    && !urls.is_empty()
                {
                    return Some(urls);
                }

                if let Some(urls) = find_image_url_array(child)
                    && !urls.is_empty()
                {
                    return Some(urls);
                }
            }
            None
        }
        _ => None,
    }
}

fn collect_all_image_like_urls(value: &serde_json::Value) -> Option<Vec<String>> {
    let mut urls = Vec::new();
    collect_image_like_urls_into(value, &mut urls);
    dedupe_urls(&mut urls);
    (!urls.is_empty()).then_some(urls)
}

fn collect_image_like_urls_into(value: &serde_json::Value, urls: &mut Vec<String>) {
    match value {
        serde_json::Value::String(url) if looks_like_image_url(url) => {
            urls.push(url.clone());
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_image_like_urls_into(item, urls);
            }
        }
        serde_json::Value::Object(map) => {
            for child in map.values() {
                collect_image_like_urls_into(child, urls);
            }
        }
        _ => {}
    }
}

fn dedupe_urls(urls: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    urls.retain(|url| seen.insert(url.clone()));
}

fn summarize_object_keys(value: &serde_json::Value) -> String {
    if let Some(map) = value.as_object() {
        return Some({
            let mut keys = map.keys().take(8).cloned().collect::<Vec<_>>();
            if map.len() > keys.len() {
                keys.push("...".to_string());
            }
            keys.join(", ")
        })
        .filter(|keys| !keys.is_empty())
        .unwrap_or_else(|| "<empty-object>".to_string());
    }

    if let Some(array) = value.as_array() {
        return format!("<array len={}>", array.len());
    }

    if value.is_null() {
        return "<null>".to_string();
    }

    format!("<{}>", json_type_name(value))
}

fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pages_response_uses_unscrambled_cdn_for_scrambled_base_items() {
        let body = serde_json::json!({
            "result": {
                "pages": {
                    "baseUrl": "https://ek10.wowpic2.store/i/bexample",
                    "items": [
                        { "url": "001.webp", "s": 1 },
                        { "url": "002.webp", "s": 0 }
                    ]
                }
            }
        })
        .to_string();

        let pages = parse_pages_response(&body).expect("pages should parse");

        assert_eq!(
            pages,
            vec![
                ParsedPage {
                    url: "https://ek10.wowpic2.store/sii/bexample/001.webp".to_string(),
                    descramble: false,
                    descramble_map: None,
                    source_scrambled: true,
                },
                ParsedPage {
                    url: "https://ek10.wowpic2.store/i/bexample/002.webp".to_string(),
                    descramble: false,
                    descramble_map: None,
                    source_scrambled: false,
                },
            ]
        );
    }

    #[test]
    fn parse_pages_response_uses_unscrambled_cdn_for_scrambled_images() {
        let body = serde_json::json!({
            "result": {
                "images": [
                    { "url": "https://static.example.test/si/bexample/001.webp", "s": 1 },
                    { "url": "https://static.example.test/002.webp", "s": 0 },
                ]
            }
        })
        .to_string();

        let pages = parse_pages_response(&body).expect("pages should parse");

        assert_eq!(
            pages,
            vec![
                ParsedPage {
                    url: "https://static.example.test/sii/bexample/001.webp".to_string(),
                    descramble: false,
                    descramble_map: None,
                    source_scrambled: true,
                },
                ParsedPage {
                    url: "https://static.example.test/002.webp".to_string(),
                    descramble: false,
                    descramble_map: None,
                    source_scrambled: false,
                },
            ]
        );
    }

    #[test]
    fn parse_pages_response_preserves_dynamic_descramble_map() {
        let map = (0..25).rev().collect::<Vec<_>>();
        let body = serde_json::json!({
            "result": {
                "images": [
                    {
                        "url": "https://static.example.test/sii/bexample/001.webp",
                        "s": 1,
                        "manga_server_descramble_map": map
                    }
                ]
            }
        })
        .to_string();

        let pages = parse_pages_response(&body).expect("pages should parse");

        assert_eq!(
            pages,
            vec![ParsedPage {
                url: "https://static.example.test/si/bexample/001.webp".to_string(),
                descramble: true,
                descramble_map: Some((0..25).rev().collect()),
                source_scrambled: true,
            }]
        );
    }
}
