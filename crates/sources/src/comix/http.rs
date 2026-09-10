use crate::SourceHttpClient;
use crate::kit::prelude::{
    HttpMethod as KitMethod, MediaRefParts, RequestParts, RequestPurpose as KitPurpose, api_json,
    expect_success, get_html, head_document, media_hotlink,
};

use crate::comix::browser_capture::{
    CHAPTER_LIST_SCRIPT, DONE_EXPRESSION, PAGE_LIST_SCRIPT, PAYLOADS_EXPRESSION, POLL_INTERVAL_MS,
    TIMEOUT_MS,
};
use crate::comix::{SourceResult, source_error};
use crate::types::{
    BrowserJsonCaptureRequest, FetchRequest, HttpHeader, HttpMethod, MediaRef, RequestPurpose,
};

pub async fn get_json_body(http: &SourceHttpClient, url: &str) -> SourceResult<String> {
    fetch_ok(http, &to_fetch_request(api_json(url))).await
}

pub async fn get_html_body(http: &SourceHttpClient, url: &str) -> SourceResult<String> {
    fetch_ok(http, &to_fetch_request(get_html(url))).await
}

#[derive(Clone, Copy)]
pub enum BrowserCaptureKind {
    Search,
    ChapterList,
    PageList,
}

pub async fn capture_browser_json_payloads(
    http: &SourceHttpClient,
    url: &str,
    kind: BrowserCaptureKind,
) -> SourceResult<Vec<String>> {
    let request = BrowserJsonCaptureRequest {
        url: url.to_string(),
        init_script: capture_script(kind).to_string(),
        done_expression: DONE_EXPRESSION.to_string(),
        payloads_expression: PAYLOADS_EXPRESSION.to_string(),
        timeout_ms: TIMEOUT_MS,
        poll_interval_ms: POLL_INTERVAL_MS,
    };

    http.capture_browser_json(&request)
        .await
        .map(|response| response.payloads)
        .map_err(|message| source_error("browser_capture_failed", message, true))
}

pub async fn resolve_final_url(http: &SourceHttpClient, url: &str) -> SourceResult<String> {
    let request = to_fetch_request(head_document(url));
    let response = http
        .fetch(&request)
        .await
        .map_err(|message| source_error("fetch_failed", message, true))?;
    if (200..400).contains(&response.status) {
        Ok(response.final_url)
    } else {
        Err(source_error(
            "http_error",
            format!(
                "Expected success for '{url}', got status {}",
                response.status
            ),
            true,
        ))
    }
}

pub fn media_hotlink_ref(url: &str, referer_root: &str) -> MediaRef {
    let MediaRefParts { url, request } = media_hotlink(url, referer_root);
    MediaRef {
        url,
        request: request.map(to_fetch_request),
    }
}

pub fn media_hotlink_ref_with_fragment(url: &str, referer_root: &str, fragment: &str) -> MediaRef {
    let MediaRefParts { url, request } = media_hotlink(url, referer_root);
    MediaRef {
        url: format!("{url}#{fragment}"),
        request: request.map(to_fetch_request),
    }
}

pub async fn fetch_ok(http: &SourceHttpClient, request: &FetchRequest) -> SourceResult<String> {
    let url = request.url.clone();
    let response = http
        .fetch(request)
        .await
        .map_err(|message| source_error("fetch_failed", message, true))?;
    expect_success(&url, response.status, response.body)
        .map_err(|message| source_error("http_error", message, true))
}

fn to_fetch_request(parts: RequestParts) -> FetchRequest {
    let RequestParts {
        url,
        method,
        headers,
        body,
        purpose,
    } = parts;

    FetchRequest {
        url,
        method: to_http_method(method),
        headers: headers
            .into_iter()
            .map(|header| HttpHeader {
                name: header.name,
                value: header.value,
            })
            .collect(),
        body,
        purpose: match purpose {
            KitPurpose::Api => RequestPurpose::Api,
            KitPurpose::Document => RequestPurpose::Document,
            KitPurpose::Image => RequestPurpose::Image,
            KitPurpose::Asset => RequestPurpose::Asset,
            KitPurpose::Custom => RequestPurpose::Custom,
        },
    }
}

fn to_http_method(method: KitMethod) -> HttpMethod {
    match method {
        KitMethod::Get => HttpMethod::Get,
        KitMethod::Post => HttpMethod::Post,
        KitMethod::Put => HttpMethod::Put,
        KitMethod::Patch => HttpMethod::Patch,
        KitMethod::Delete => HttpMethod::Delete,
        KitMethod::Head => HttpMethod::Head,
        KitMethod::Options => HttpMethod::Options,
    }
}

fn capture_script(kind: BrowserCaptureKind) -> &'static str {
    match kind {
        BrowserCaptureKind::Search => crate::comix::browser_capture::SEARCH_SCRIPT,
        BrowserCaptureKind::ChapterList => CHAPTER_LIST_SCRIPT,
        BrowserCaptureKind::PageList => PAGE_LIST_SCRIPT,
    }
}
