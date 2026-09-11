use super::{BASE_URL, hotlink};
use crate::{SourceResult, source_error, types::*};
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use std::collections::HashSet;

fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("static Rawkuma selector")
}
fn text(node: ElementRef<'_>) -> String {
    node.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn first_text(document: &Html, query: &str) -> String {
    document
        .select(&selector(query))
        .next()
        .map(text)
        .unwrap_or_default()
}
fn absolute(value: &str) -> SourceResult<String> {
    let url = url::Url::parse(BASE_URL)
        .unwrap()
        .join(value.trim())
        .map_err(|e| source_error("invalid_url", e.to_string(), false))?;
    if !["https", "http"].contains(&url.scheme()) {
        return Err(source_error("invalid_url", "Unsupported image URL", false));
    }
    Ok(url.into())
}
pub(super) fn document_url(id: &str) -> SourceResult<String> {
    let url = if id.contains('/') || id.starts_with("http") {
        absolute(id)?
    } else {
        absolute(&format!("/manga/{id}/"))?
    };
    let parsed = url::Url::parse(&url).unwrap();
    if parsed.host_str() != Some("rawkuma.net")
        || !parsed.path().starts_with("/manga/")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(source_error(
            "invalid_id",
            "Rawkuma IDs must refer to a Rawkuma manga or chapter",
            false,
        ));
    }
    Ok(url)
}
fn manga_id(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let parts: Vec<_> = parsed.path_segments()?.filter(|p| !p.is_empty()).collect();
    if parsed.host_str() == Some("rawkuma.net") && parts.len() == 2 && parts[0] == "manga" {
        Some(parts[1].into())
    } else {
        None
    }
}
fn is_nsfw(genres: &[String]) -> bool {
    genres
        .iter()
        .any(|g| ["adult", "hentai", "mature", "smut"].contains(&g.to_ascii_lowercase().as_str()))
}
fn empty_manga(id: String, title: String, cover: String) -> Manga {
    Manga {
        id,
        title,
        cover: hotlink(cover),
        description: String::new(),
        author: String::new(),
        genres: vec![],
        status: "Unknown".into(),
        is_nsfw: false,
        alt_titles: vec![],
    }
}

pub(super) fn search(html: &str, page: u32) -> SourceResult<SearchResults> {
    let doc = Html::parse_document(html);
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for node in doc.select(&selector("#search-results > div, .bs, .card")) {
        let Some(link) = node.select(&selector("a[href*='/manga/']")).find(|a| {
            a.value()
                .attr("href")
                .and_then(|u| absolute(u).ok())
                .and_then(|u| manga_id(&u))
                .is_some()
        }) else {
            continue;
        };
        let url = absolute(link.value().attr("href").unwrap())?;
        let id = manga_id(&url).unwrap();
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(img) = node.select(&selector("img")).next() else {
            continue;
        };
        let title = img
            .value()
            .attr("alt")
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| text(link));
        let cover = img
            .value()
            .attr("data-src")
            .or_else(|| img.value().attr("src"))
            .unwrap_or_default();
        let mut manga = empty_manga(id, title, absolute(cover)?);
        manga.description = node
            .select(&selector("p.line-clamp-3"))
            .next()
            .map(text)
            .unwrap_or_default();
        manga.genres = node
            .select(&selector("a[href*='genre']"))
            .map(text)
            .collect();
        manga.is_nsfw = is_nsfw(&manga.genres)
            || node
                .select(&selector(".adult, .mature, [data-nsfw='true']"))
                .next()
                .is_some();
        items.push(manga);
    }
    let has_next_page = doc
        .select(&selector("a.next, a[rel='next']"))
        .next()
        .is_some()
        || doc.select(&selector("button[onclick]")).any(|button| {
            let click = button.value().attr("onclick").unwrap_or_default();
            click.contains("'page'")
                && click
                    .split(',')
                    .nth(1)
                    .and_then(|v| v.trim().trim_matches('\'').parse::<u32>().ok())
                    .is_some_and(|next| next > page)
        });
    if items.is_empty()
        && doc
            .select(&selector("#search-results, .listupd"))
            .next()
            .is_none()
    {
        return Err(source_error(
            "search_parse_failed",
            "Rawkuma search document has no results container",
            true,
        ));
    }
    Ok(SearchResults {
        items,
        has_next_page,
    })
}

pub(super) fn manga(html: &str, url: &str) -> SourceResult<Manga> {
    let doc = Html::parse_document(html);
    for script in doc.select(&selector("script[type='application/ld+json']")) {
        let Ok(data) = serde_json::from_str::<Value>(&script.inner_html()) else {
            continue;
        };
        let kind = &data["@type"];
        if kind.as_str() != Some("ComicSeries")
            && !kind
                .as_array()
                .is_some_and(|k| k.iter().any(|v| v == "ComicSeries"))
        {
            continue;
        }
        let mut manga = empty_manga(
            manga_id(url).ok_or_else(|| source_error("invalid_id", "Expected manga URL", false))?,
            data["name"].as_str().unwrap_or_default().into(),
            absolute(
                data["image"]["url"]
                    .as_str()
                    .or_else(|| data["image"].as_str())
                    .unwrap_or_default(),
            )?,
        );
        manga.description = data["description"].as_str().unwrap_or_default().into();
        let full_description = first_text(&doc, "[itemprop='description'], .entry-content, .desc");
        if !full_description.is_empty() {
            manga.description = full_description;
        }
        manga.author = data["author"]["name"]
            .as_str()
            .or_else(|| data["author"].as_str())
            .unwrap_or_default()
            .into();
        manga.genres = data["genre"]
            .as_array()
            .map(|v| v.iter().filter_map(Value::as_str).map(Into::into).collect())
            .unwrap_or_default();
        manga.status = data["creativeWorkStatus"]
            .as_str()
            .unwrap_or("Unknown")
            .into();
        manga.alt_titles = data["alternateName"]
            .as_str()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(Into::into)
            .collect();
        manga.is_nsfw = is_nsfw(&manga.genres);
        if !manga.title.is_empty() {
            return Ok(manga);
        }
    }
    Err(source_error(
        "manga_parse_failed",
        "Rawkuma document has no ComicSeries metadata",
        true,
    ))
}

pub(super) fn chapters(html: &str) -> SourceResult<Vec<Chapter>> {
    let doc = Html::parse_document(html);
    let mut seen = HashSet::new();
    let mut chapters: Vec<Chapter> = Vec::new();
    for link in doc.select(&selector("#chapter-list a[href], #chapterlist a[href]")) {
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        if href.starts_with("https://drive.google.com/") {
            if let Some(chapter) = chapters.last_mut() {
                chapter.download_url = Some(href.to_owned());
            }
            continue;
        }
        // Rawkuma places external download links (currently Google Drive) in the
        // same chapter container. They are not chapter identities; skip them and
        // keep parsing the actual Rawkuma chapter anchors.
        let Ok(url) = document_url(href) else {
            continue;
        };
        if !seen.insert(url.clone()) {
            continue;
        }
        let label = link
            .select(&selector(".chapternum, span"))
            .next()
            .map(text)
            .unwrap_or_else(|| text(link));
        let lower = label.to_ascii_lowercase();
        let Some(number_text) = lower
            .strip_prefix("chapter")
            .or_else(|| lower.strip_prefix("ch."))
        else {
            continue;
        };
        let number: String = number_text
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let Ok(number) = number.parse::<f64>() else {
            continue;
        };
        let published_at = link
            .select(&selector("time[datetime]"))
            .next()
            .and_then(|t| t.value().attr("datetime"))
            .unwrap_or_default()
            .into();
        chapters.push(Chapter {
            id: url,
            title: label,
            number,
            volume: None,
            published_at,
            download_url: None,
        });
    }
    if doc
        .select(&selector("#chapter-list, #chapterlist"))
        .next()
        .is_none()
    {
        return Err(source_error(
            "chapters_parse_failed",
            "Rawkuma chapter list missing",
            true,
        ));
    }
    chapters.sort_by(|a, b| b.number.total_cmp(&a.number));
    Ok(chapters)
}

pub(super) fn pages(payload: &str) -> SourceResult<Vec<Page>> {
    let data: Value = serde_json::from_str(payload)
        .map_err(|e| source_error("pages_parse_failed", e.to_string(), true))?;
    let images = data["sources"]
        .as_array()
        .and_then(|sources| sources.iter().find_map(|s| s["images"].as_array()))
        .filter(|images| !images.is_empty())
        .ok_or_else(|| source_error("pages_parse_failed", "Rawkuma reader has no images", true))?;
    images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let url = image
                .as_str()
                .ok_or_else(|| source_error("pages_parse_failed", "Invalid Rawkuma image", true))?;
            Ok(Page {
                index: u32::try_from(index)
                    .map_err(|e| source_error("pages_parse_failed", e.to_string(), false))?,
                image: hotlink(absolute(url)?),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_search_layout_deduplicates_display_modes_and_has_pagination() {
        let result = search(include_str!("fixtures/search.html"), 1).unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].id, "one-punch-man");
        assert_eq!(result.items[0].title, "One Punch-Man");
        assert!(result.has_next_page);
        assert!(
            !search(include_str!("fixtures/search.html"), 2)
                .unwrap()
                .has_next_page
        );
    }
    #[test]
    fn live_metadata_and_chapters_preserve_dates_and_fractional_numbers() {
        let html = include_str!("fixtures/manga.html");
        let manga = manga(html, "https://rawkuma.net/manga/one-punch-man/").unwrap();
        assert_eq!(manga.author, "ONE");
        assert_eq!(manga.status, "Ongoing");
        assert!(manga.alt_titles.contains(&"ワンパンマン".into()));
        let chapters = chapters(html).unwrap();
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].number, 284.5);
        assert_eq!(chapters[1].published_at, "2026-09-09T15:24:20Z");
    }
    #[test]
    fn chapters_ignore_unrelated_download_links() {
        let html = r#"
            <div id="chapter-list">
                <a href="https://rawkuma.net/manga/one-punch-man/chapter-1.1/"><span>Chapter 1</span></a>
                <a href="https://drive.google.com/uc?id=example&amp;export=download">Download</a>
            </div>
        "#;
        let chapters = chapters(html).expect("unrelated links should not invalidate chapters");
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].number, 1.0);
        assert_eq!(
            chapters[0].download_url.as_deref(),
            Some("https://drive.google.com/uc?id=example&export=download")
        );
    }
    #[test]
    fn reader_json_preserves_native_urls_order_and_referer() {
        let pages = pages(include_str!("fixtures/reader.json")).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[1].index, 1);
        assert!(pages[0].image.url.ends_with("/284/1.jpg"));
        let request = pages[0].image.request.as_ref().unwrap();
        assert_eq!(request.headers[0].value, "https://rawkuma.net/");
        assert_eq!(request.purpose, RequestPurpose::Image);
    }
    #[test]
    fn blocked_and_malformed_documents_are_errors_not_empty_catalogs() {
        assert!(search("<title>Just a moment...</title>", 1).is_err());
        assert!(chapters("<html></html>").is_err());
        assert!(pages(r#"{"sources":[]}"#).is_err());
        assert!(document_url("https://example.com/manga/test/").is_err());
    }
}
