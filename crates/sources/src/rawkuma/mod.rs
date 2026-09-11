//! Browser-backed Rawkuma documents; CDN images use the shared HTTP client.
mod parse;
use crate::{Source, SourceHttpClient, SourceResult, source_error, types::*};
const BASE_URL: &str = "https://rawkuma.net";

pub struct RawkumaSource {
    http: SourceHttpClient,
    metadata: SourceMetadata,
}
impl RawkumaSource {
    pub fn new(http: SourceHttpClient) -> Self {
        Self {
            http,
            metadata: SourceMetadata {
                id: "rawkuma".into(),
                name: "Rawkuma".into(),
                base_url: BASE_URL.into(),
                version: "1.0.0".into(),
                api_version: 6,
                build_metadata: None,
                homepage: Some(BASE_URL.into()),
                repository: None,
                capabilities: vec![
                    SourceCapability::Search,
                    SourceCapability::MangaDetails,
                    SourceCapability::ChapterList,
                    SourceCapability::PageList,
                ],
                search_options: SearchOptions {
                    categories: vec![
                        "popular".into(),
                        "updated".into(),
                        "rating".into(),
                        "title".into(),
                    ],
                    default_category: Some("updated".into()),
                    supports_popular_sort: true,
                },
            },
        }
    }
    async fn capture(&self, url: String, done: &str, payloads: &str) -> SourceResult<String> {
        let response = self
            .http
            .capture_browser_json(&BrowserJsonCaptureRequest {
                url,
                init_script: String::new(),
                done_expression: done.into(),
                payloads_expression: payloads.into(),
                timeout_ms: 75_000,
                poll_interval_ms: 500,
            })
            .await
            .map_err(|e| source_error("browser_capture_failed", e, true))?;
        response
            .payloads
            .into_iter()
            .find(|p| !p.trim().is_empty())
            .ok_or_else(|| {
                source_error(
                    "empty_capture",
                    "Rawkuma browser returned no document",
                    true,
                )
            })
    }
    async fn document(&self, url: String, selector: &str) -> SourceResult<String> {
        let done = format!(
            "document.readyState !== 'loading' && !!document.querySelector({})",
            serde_json::to_string(selector).expect("selector JSON")
        );
        self.capture(url, &done, "[document.documentElement.outerHTML]")
            .await
    }
}
#[async_trait::async_trait]
impl Source for RawkumaSource {
    fn metadata(&self) -> &SourceMetadata {
        &self.metadata
    }
    async fn search(&self, query: SearchQuery) -> SourceResult<SearchResults> {
        let page = query.page.max(1);
        let order = if query.popular {
            "popular"
        } else {
            query.category.as_deref().unwrap_or("updated")
        };
        if !["popular", "updated", "rating", "title"].contains(&order) {
            return Err(source_error(
                "invalid_category",
                "Unknown Rawkuma search category",
                false,
            ));
        }
        let mut url = url::Url::parse(&format!("{BASE_URL}/library/")).expect("source URL");
        url.query_pairs_mut()
            .append_pair("the_page", &page.to_string())
            .append_pair("search_term", &query.text)
            .append_pair("orderby", order)
            .append_pair("order", if order == "title" { "asc" } else { "desc" });
        let html = self.document(url.into(), "#search-results > *").await?;
        parse::search(&html, page)
    }
    async fn manga(&self, id: &str) -> SourceResult<Manga> {
        let url = parse::document_url(id)?;
        let html = self.document(url.clone(), "article, .infox").await?;
        parse::manga(&html, &url)
    }
    async fn chapters(&self, id: &str) -> SourceResult<Vec<Chapter>> {
        let html = self
            .document(parse::document_url(id)?, "#chapter-list, #chapterlist")
            .await?;
        parse::chapters(&html)
    }
    async fn pages(&self, id: &str) -> SourceResult<Vec<Page>> {
        let url = parse::document_url(id)?;
        // Older themes expose ts_reader.run JSON. The current theme renders a dedicated
        // data-image-data container. Serialize that list in the browser, never unrelated
        // logos, covers or ads. No screen-size or image-width filtering is applied.
        let expression = r#"(() => {
            for (const script of document.scripts) {
                const text = script.textContent || '';
                const start = text.indexOf('ts_reader.run(');
                if (start >= 0) {
                    const value = text.slice(start + 'ts_reader.run('.length).trim();
                    // JSON.parse needs the object, not the trailing function call.
                    for (let end = value.lastIndexOf('}'); end >= 0; end = value.lastIndexOf('}', end - 1)) {
                        try { return [JSON.stringify(JSON.parse(value.slice(0, end + 1)))]; } catch (_) {}
                    }
                }
            }
            // The container is itself the scope that excludes logos, covers and ads, so its
            // images are taken whatever host serves them. Themes without it served pages
            // from /wp-content/scr/, which is the only case that still needs a path filter.
            const container = document.querySelector('[data-image-data]');
            const candidates = container
                ? container.querySelectorAll('img')
                : document.querySelectorAll('img[src*="/wp-content/scr/"], img[data-src*="/wp-content/scr/"]');
            const images = [...candidates]
                .map(img => img.getAttribute('data-src') || img.getAttribute('src'))
                .filter(url => url && !url.startsWith('data:'));
            return images.length ? [JSON.stringify({sources: [{images}]})] : [];
        })()"#;
        let done = format!("document.readyState !== 'loading' && ({expression}).length > 0");
        let payload = self.capture(url, &done, expression).await?;
        parse::pages(&payload)
    }
}

fn hotlink(url: String) -> MediaRef {
    MediaRef {
        request: Some(FetchRequest {
            url: url.clone(),
            method: HttpMethod::Get,
            headers: vec![HttpHeader {
                name: "Referer".into(),
                value: format!("{BASE_URL}/"),
            }],
            body: None,
            purpose: RequestPurpose::Image,
        }),
        url,
    }
}
