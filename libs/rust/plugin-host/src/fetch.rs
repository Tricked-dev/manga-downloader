use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use anyhow::Result;
use reqwest::Method;
use reqwest::StatusCode;
use reqwest::header::{
    ACCEPT, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, REFERER, USER_AGENT,
};
use tower::{
    Service, ServiceBuilder, ServiceExt,
    limit::ConcurrencyLimitLayer,
    util::{BoxCloneSyncService, service_fn},
};
use tower_resilience_coalesce::{CoalesceError, CoalesceLayer};
use tower_resilience_retry::RetryLayer;

use backend_clearance::{
    BrowserJsonCaptureRequest as ClearanceBrowserJsonCaptureRequest, ClearanceSolution,
    ClearanceSolver,
};

use super::media::{FetchRequestSpec, MediaRefSpec, RequestPurposeSpec};

pub use backend_tls::ensure_graviola_rustls_provider;

pub(crate) const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Firefox/128.0";
const OUTBOUND_TIMEOUT: Duration = Duration::from_secs(30);
const OUTBOUND_CONCURRENCY_LIMIT: usize = 32;
const OUTBOUND_MAX_ATTEMPTS: usize = 3;
const OUTBOUND_RETRY_BASE_DELAY: Duration = Duration::from_millis(100);
const MEDIA_FETCH_CONCURRENCY_LIMIT: usize = 32;
const BROWSER_CAPTURE_CONCURRENCY_LIMIT: usize = 2;
const BROWSER_TEXT_CAPTURE_TIMEOUT: Duration = Duration::from_secs(75);
const BROWSER_TEXT_CAPTURE_POLL_INTERVAL: Duration = Duration::from_millis(500);
const CLEARANCE_BODY_SNIFF_LIMIT: usize = 16 * 1024;
const WOWPIC_UNSCRAMBLED_PATH_SEGMENT: &str = "/i/";
const WOWPIC_SCRAMBLED_PATH_SEGMENT: &str = "/si/";
const WOWPIC_SECONDARY_SCRAMBLED_PATH_SEGMENT: &str = "/sii/";
const WOWPIC_MEDIA_PATH_SEGMENTS: &[&str] = &[
    WOWPIC_UNSCRAMBLED_PATH_SEGMENT,
    WOWPIC_SCRAMBLED_PATH_SEGMENT,
    WOWPIC_SECONDARY_SCRAMBLED_PATH_SEGMENT,
];
const WOWPIC_UNSCRAMBLED_PREFERRED_SEGMENTS: &[&str] = &[
    WOWPIC_UNSCRAMBLED_PATH_SEGMENT,
    WOWPIC_SCRAMBLED_PATH_SEGMENT,
    WOWPIC_SECONDARY_SCRAMBLED_PATH_SEGMENT,
];
const WOWPIC_SCRAMBLED_PREFERRED_SEGMENTS: &[&str] = &[
    WOWPIC_SCRAMBLED_PATH_SEGMENT,
    WOWPIC_UNSCRAMBLED_PATH_SEGMENT,
    WOWPIC_SECONDARY_SCRAMBLED_PATH_SEGMENT,
];
const WOWPIC_SECONDARY_SCRAMBLED_PREFERRED_SEGMENTS: &[&str] = &[
    WOWPIC_SCRAMBLED_PATH_SEGMENT,
    WOWPIC_UNSCRAMBLED_PATH_SEGMENT,
    WOWPIC_SECONDARY_SCRAMBLED_PATH_SEGMENT,
];
const CLEARANCE_BODY_MARKERS: &[&str] = &[
    "attention required! | cloudflare",
    "just a moment...",
    "cf-challenge-running",
    "challenge-spinner",
    "turnstile-wrapper",
    "cf-error-title",
    "ddos-guard",
];
const API_JSON_CAPTURE_SCRIPT: &str = r"
(() => {
    if (window.__mangaServerApiCaptureInstalled) return;
    window.__mangaServerApiCaptureInstalled = true;
    window.__mangaServerApiCaptureDone = false;
    window.__mangaServerApiPayloads = [];

    const collect = () => {
        const text = (document.body && document.body.innerText)
            || (document.documentElement && document.documentElement.textContent)
            || '';
        const body = text.trim();
        if (body) {
            window.__mangaServerApiPayloads = [body];
            window.__mangaServerApiCaptureDone = true;
        }
    };

    document.addEventListener('DOMContentLoaded', collect);
    window.addEventListener('load', collect);
    setInterval(collect, 100);
    collect();
})();
";
const API_JSON_CAPTURE_DONE_EXPRESSION: &str = "window.__mangaServerApiCaptureDone";
const API_JSON_CAPTURE_PAYLOADS_EXPRESSION: &str = "window.__mangaServerApiPayloads";
static OUTBOUND_COALESCE_NONCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestProfile {
    ApiJson,
    HtmlPage,
    ImageHotlink,
    BinaryAsset,
    Custom,
}

#[derive(Clone, Debug)]
pub struct ExecutableHttpRequest {
    pub url: String,
    pub method: Method,
    pub headers: HeaderMap,
    pub body: Option<String>,
    pub profile: RequestProfile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutableHttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub final_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryFetchResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub final_url: String,
    pub content_type: String,
}

type OutboundService = BoxCloneSyncService<OutboundRequest, OutboundResponse, OutboundServiceError>;
type OutboundServiceError = CoalesceError<OutboundRequestError>;
type MediaFetchTowerService =
    BoxCloneSyncService<MediaFetchRequest, BinaryFetchResponse, anyhow::Error>;
type BrowserJsonCaptureTowerService =
    BoxCloneSyncService<ClearanceBrowserJsonCaptureRequest, Vec<String>, String>;
type MediaFetchFuture = Pin<Box<dyn Future<Output = Result<BinaryFetchResponse>> + Send + 'static>>;
type BrowserJsonCaptureFuture =
    Pin<Box<dyn Future<Output = std::result::Result<Vec<String>, String>> + Send + 'static>>;

#[derive(Clone, Debug)]
struct OutboundRequest {
    method: Method,
    url: String,
    headers: HeaderMap,
    body: Option<String>,
    retryable: bool,
    coalesce_key: OutboundCoalesceKey,
}

#[derive(Clone, Debug)]
struct OutboundResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
    final_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct OutboundCoalesceKey {
    method: String,
    url: String,
    headers: Vec<(String, Vec<u8>)>,
    body: Option<String>,
    unique: Option<u64>,
}

#[derive(Clone, Debug)]
struct OutboundRequestError {
    message: Arc<str>,
}

#[derive(Clone)]
struct HttpCore {
    service: OutboundService,
    cookie_jar: Arc<reqwest::cookie::Jar>,
    clearance_solver: Arc<ClearanceSolver>,
}

#[derive(Clone)]
pub struct PluginHttpClient {
    core: Arc<HttpCore>,
    media_fetch: MediaFetchTowerService,
    browser_json_capture: BrowserJsonCaptureTowerService,
}

#[derive(Clone, Debug)]
pub struct MediaFetchRequest {
    pub media: MediaRefSpec,
    pub default_profile: RequestProfile,
}

#[derive(Clone)]
struct MediaFetchHandler {
    core: Arc<HttpCore>,
}

#[derive(Clone)]
struct BrowserJsonCaptureHandler {
    core: Arc<HttpCore>,
}

impl PluginHttpClient {
    /// Builds the shared plugin HTTP client and outbound service stack.
    pub fn new() -> anyhow::Result<Self> {
        ensure_graviola_rustls_provider()?;

        let cookie_jar = Arc::new(reqwest::cookie::Jar::default());
        let client = reqwest::Client::builder()
            .user_agent(DEFAULT_USER_AGENT)
            .cookie_provider(Arc::clone(&cookie_jar))
            .no_brotli()
            .no_gzip()
            .zstd(true)
            .build()?;
        let service = build_outbound_service(client);
        let core = Arc::new(HttpCore {
            service,
            cookie_jar,
            clearance_solver: Arc::new(ClearanceSolver::new()),
        });

        Ok(Self {
            core: Arc::clone(&core),
            media_fetch: build_media_fetch_service(Arc::clone(&core)),
            browser_json_capture: build_browser_json_capture_service(core),
        })
    }

    /// Executes a plugin text HTTP request and returns a UTF-8 body.
    ///
    /// Browser clearance is attempted automatically for recognized challenge
    /// responses.
    pub async fn execute_http_request(
        &self,
        request: ExecutableHttpRequest,
    ) -> std::result::Result<ExecutableHttpResponse, String> {
        let started_at = Instant::now();
        let response = self.core.execute_text_request(&request).await?;

        tracing::trace!(
            method = %request.method,
            url = %request.url,
            profile = ?request.profile,
            mode = "native_clearance",
            status_code = response.status.as_u16(),
            final_url = %response.final_url,
            elapsed_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
            outcome = request_outcome(response.status),
            "Plugin HTTP Request Completed",
        );

        Ok(ExecutableHttpResponse {
            status: response.status.as_u16(),
            headers: response_headers(&response.headers),
            body: String::from_utf8(response.body)
                .map_err(|err| format!("Failed to decode response body as UTF-8: {err}"))?,
            final_url: response.final_url,
        })
    }

    /// Fetches binary media for a plugin media reference.
    ///
    /// The returned response includes the final URL and resolved content type.
    pub async fn fetch_media(
        &self,
        media: &MediaRefSpec,
        default_profile: RequestProfile,
    ) -> Result<BinaryFetchResponse> {
        self.media_fetch
            .clone()
            .oneshot(MediaFetchRequest {
                media: media.clone(),
                default_profile,
            })
            .await
    }

    /// Captures JSON payloads from a browser page using the clearance solver.
    pub async fn capture_browser_json_payloads(
        &self,
        request: ClearanceBrowserJsonCaptureRequest,
    ) -> std::result::Result<Vec<String>, String> {
        self.browser_json_capture.clone().oneshot(request).await
    }
}

impl MediaFetchHandler {
    async fn execute(
        core: Arc<HttpCore>,
        request: MediaFetchRequest,
    ) -> Result<BinaryFetchResponse> {
        let executable = HttpCore::media_request(&request.media, request.default_profile)?;
        let response = core.execute_binary_request(&executable).await?;
        let content_type = response
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        Ok(BinaryFetchResponse {
            status: response.status.as_u16(),
            headers: response_headers(&response.headers),
            body: response.body,
            final_url: response.final_url,
            content_type,
        })
    }
}

impl Service<MediaFetchRequest> for MediaFetchHandler {
    type Response = BinaryFetchResponse;
    type Error = anyhow::Error;
    type Future = MediaFetchFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: MediaFetchRequest) -> Self::Future {
        let core = Arc::clone(&self.core);
        Box::pin(async move { Self::execute(core, request).await })
    }
}

impl BrowserJsonCaptureHandler {
    async fn execute(
        core: Arc<HttpCore>,
        request: ClearanceBrowserJsonCaptureRequest,
    ) -> std::result::Result<Vec<String>, String> {
        let started_at = Instant::now();
        let url = request.url.clone();
        tracing::info!(
            url = %url,
            "Plugin Browser JSON Capture Started",
        );
        let payloads = core.clearance_solver.capture_json_payloads(request).await?;

        tracing::trace!(
            url = %url,
            elapsed_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
            payload_count = payloads.len(),
            "Plugin Browser JSON Capture Completed",
        );

        Ok(payloads)
    }
}

impl Service<ClearanceBrowserJsonCaptureRequest> for BrowserJsonCaptureHandler {
    type Response = Vec<String>;
    type Error = String;
    type Future = BrowserJsonCaptureFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<std::result::Result<(), String>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: ClearanceBrowserJsonCaptureRequest) -> Self::Future {
        let core = Arc::clone(&self.core);
        Box::pin(async move { Self::execute(core, request).await })
    }
}

impl std::fmt::Display for OutboundRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OutboundRequestError {}

impl OutboundRequestError {
    fn new(message: impl Into<Arc<str>>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl HttpCore {
    fn media_request(
        media: &MediaRefSpec,
        default_profile: RequestProfile,
    ) -> Result<ExecutableHttpRequest> {
        if let Some(request) = &media.request {
            return executable_request_from_spec(request);
        }

        Ok(ExecutableHttpRequest {
            url: media.url.clone(),
            method: Method::GET,
            headers: HeaderMap::new(),
            body: None,
            profile: default_profile,
        })
    }

    async fn execute_text_request(
        &self,
        request: &ExecutableHttpRequest,
    ) -> std::result::Result<OutboundResponse, String> {
        let response = self.send_request(request, false, None).await?;
        let needs_clearance = response_requires_clearance(&response, request);
        if response.status.is_success() && !needs_clearance {
            return Ok(response);
        }

        if !needs_clearance {
            return Err(status_error_message(&request.url, response.status));
        }

        if browser_text_capture_candidate(request) {
            match self.capture_text_response_in_browser(request).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    tracing::debug!(
                        url = %request.url,
                        error = %error,
                        "Browser API JSON Capture Failed; Trying Clearance Cookie Retry",
                    );
                }
            }
        }

        let solved = self.ensure_bypass_session(request).await?;
        let user_agent_candidates =
            user_agent_candidates(solved.user_agent.as_deref(), &request.method);

        for user_agent in user_agent_candidates {
            let response = self.send_request(request, false, user_agent).await?;
            let needs_clearance = response_requires_clearance(&response, request);
            if response.status.is_success() && !needs_clearance {
                return Ok(response);
            }
        }

        self.clearance_solver.invalidate(&request.url).await;

        Err(format!(
            "Request failed after embedded browser clearance for {}",
            request.url
        ))
    }

    async fn capture_text_response_in_browser(
        &self,
        request: &ExecutableHttpRequest,
    ) -> std::result::Result<OutboundResponse, String> {
        tracing::info!(
            url = %request.url,
            "Browser API JSON Capture Started",
        );
        let payloads = self
            .clearance_solver
            .capture_json_payloads(ClearanceBrowserJsonCaptureRequest {
                url: request.url.clone(),
                user_agent: DEFAULT_USER_AGENT.to_string(),
                init_script: API_JSON_CAPTURE_SCRIPT.to_string(),
                done_expression: API_JSON_CAPTURE_DONE_EXPRESSION.to_string(),
                payloads_expression: API_JSON_CAPTURE_PAYLOADS_EXPRESSION.to_string(),
                timeout: BROWSER_TEXT_CAPTURE_TIMEOUT,
                poll_interval: BROWSER_TEXT_CAPTURE_POLL_INTERVAL,
            })
            .await?;

        let body = payloads
            .into_iter()
            .next()
            .ok_or_else(|| format!("Browser capture produced no body for {}", request.url))?;

        tracing::info!(
            url = %request.url,
            body_bytes = body.len(),
            "Browser API JSON Capture Completed",
        );

        Ok(OutboundResponse {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: body.into_bytes(),
            final_url: request.url.clone(),
        })
    }

    async fn execute_binary_request(
        &self,
        request: &ExecutableHttpRequest,
    ) -> Result<OutboundResponse> {
        let candidate_urls = binary_candidate_urls(request);
        let primary = primary_binary_request(request, &candidate_urls);
        let response = self
            .send_request(&primary, true, None)
            .await
            .map_err(anyhow::Error::msg)?;
        let needs_clearance = response_requires_clearance(&response, &primary);
        if binary_response_accepted(&response, &primary) && !needs_clearance {
            return Ok(response);
        }

        if !needs_clearance {
            if request.profile == RequestProfile::ImageHotlink
                && host_matches_wowpic_media(request.url.as_str())
            {
                for alternate_url in candidate_urls.iter().skip(1) {
                    let mut candidate = request.clone();
                    candidate.url = alternate_url.clone();
                    let response = self
                        .send_request(&candidate, true, None)
                        .await
                        .map_err(anyhow::Error::msg)?;

                    if binary_response_accepted(&response, &candidate)
                        && !response_requires_clearance(&response, &candidate)
                    {
                        return Ok(response);
                    }
                }
            }

            anyhow::bail!(status_error_message(&request.url, response.status));
        }

        let solved = self
            .ensure_bypass_session(request)
            .await
            .map_err(anyhow::Error::msg)?;
        let user_agent_candidates =
            user_agent_candidates(solved.user_agent.as_deref(), &request.method);

        for user_agent in user_agent_candidates {
            for candidate_url in &candidate_urls {
                let mut candidate = request.clone();
                candidate.url = candidate_url.clone();
                let response = self
                    .send_request(&candidate, true, user_agent)
                    .await
                    .map_err(anyhow::Error::msg)?;
                let needs_clearance = response_requires_clearance(&response, &candidate);
                if binary_response_accepted(&response, &candidate) && !needs_clearance {
                    return Ok(response);
                }
            }
        }

        self.clearance_solver.invalidate(&request.url).await;

        anyhow::bail!(
            "Media fetch failed after embedded browser clearance for {}",
            request.url
        )
    }

    async fn ensure_bypass_session(
        &self,
        request: &ExecutableHttpRequest,
    ) -> std::result::Result<ClearanceSolution, String> {
        self.clearance_solver
            .solve(&request.url, &self.cookie_jar, DEFAULT_USER_AGENT)
            .await
    }

    async fn send_request(
        &self,
        request: &ExecutableHttpRequest,
        binary: bool,
        user_agent: Option<&str>,
    ) -> std::result::Result<OutboundResponse, String> {
        let mut headers = defaulted_headers(request, binary);
        if let Some(user_agent) = user_agent
            && !headers.contains_key(USER_AGENT)
            && let Ok(value) = HeaderValue::from_str(user_agent)
        {
            headers.insert(USER_AGENT, value);
        }

        let retryable = retryable_request(request);
        let coalesce_key = outbound_coalesce_key(
            &request.method,
            &request.url,
            &headers,
            request.body.as_ref(),
            retryable,
        );
        self.service
            .clone()
            .oneshot(OutboundRequest {
                method: request.method.clone(),
                url: request.url.clone(),
                headers,
                body: request.body.clone(),
                retryable,
                coalesce_key,
            })
            .await
            .map_err(|err| format!("Request failed: {err}"))
    }
}

fn response_requires_clearance(
    response: &OutboundResponse,
    request: &ExecutableHttpRequest,
) -> bool {
    let provider_headers_present = clearance_provider_header_present(&response.headers);
    if api_json_response_without_clearance(response, request) {
        return false;
    }

    if provider_headers_present
        && matches!(
            response.status,
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE
        )
    {
        return true;
    }

    if !should_sniff_clearance_body(response, request, provider_headers_present) {
        return false;
    }

    clearance_body_marker_present(&response.body)
}

fn api_json_response_without_clearance(
    response: &OutboundResponse,
    request: &ExecutableHttpRequest,
) -> bool {
    request.profile == RequestProfile::ApiJson
        && !clearance_body_marker_present(&response.body)
        && (response_content_type(&response.headers).is_some_and(is_json_content_type)
            || body_looks_like_json(&response.body))
}

fn browser_text_capture_candidate(request: &ExecutableHttpRequest) -> bool {
    request.profile == RequestProfile::ApiJson
        && request.method == Method::GET
        && request.body.is_none()
}

fn binary_response_accepted(response: &OutboundResponse, request: &ExecutableHttpRequest) -> bool {
    if !response.status.is_success() {
        return false;
    }

    request.profile != RequestProfile::ImageHotlink
        || response_content_type(&response.headers)
            .is_some_and(|content_type| content_type.starts_with("image/"))
}

fn clearance_provider_header_present(headers: &HeaderMap) -> bool {
    headers.contains_key("cf-ray")
        || headers.contains_key("cf-mitigated")
        || headers
            .get("server")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                let value = value.to_ascii_lowercase();
                value.contains("cloudflare") || value.contains("ddos-guard")
            })
}

fn clearance_body_marker_present(body: &[u8]) -> bool {
    let body = &body[..body.len().min(CLEARANCE_BODY_SNIFF_LIMIT)];
    let body = String::from_utf8_lossy(body).to_ascii_lowercase();
    CLEARANCE_BODY_MARKERS
        .iter()
        .any(|marker| body.contains(marker))
}

fn should_sniff_clearance_body(
    response: &OutboundResponse,
    request: &ExecutableHttpRequest,
    provider_headers_present: bool,
) -> bool {
    if let Some(content_type) = response_content_type(&response.headers) {
        return is_textual_response(content_type);
    }

    provider_headers_present
        || request.profile == RequestProfile::HtmlPage
        || matches!(
            request.profile,
            RequestProfile::ApiJson | RequestProfile::Custom
        )
        || matches!(
            response.status,
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE
        )
}

fn is_textual_response(content_type: &str) -> bool {
    backend_core::is_textual_content_type(content_type)
}

fn is_json_content_type(content_type: &str) -> bool {
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
        .to_ascii_lowercase();
    media_type == "application/json" || media_type.ends_with("+json")
}

fn body_looks_like_json(body: &[u8]) -> bool {
    body.iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| matches!(byte, b'{' | b'['))
}

fn response_content_type(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
}

fn status_error_message(url: &str, status: StatusCode) -> String {
    format!("Request failed with status {} for {url}", status.as_u16())
}

fn build_outbound_service(client: reqwest::Client) -> OutboundService {
    BoxCloneSyncService::new(
        ServiceBuilder::new()
            .layer(ConcurrencyLimitLayer::new(OUTBOUND_CONCURRENCY_LIMIT))
            .layer(
                CoalesceLayer::builder(|request: &OutboundRequest| request.coalesce_key.clone())
                    .name("plugin-outbound-http")
                    .build(),
            )
            .layer(
                RetryLayer::<OutboundRequest, OutboundResponse, OutboundRequestError>::builder()
                    .name("plugin-outbound-http")
                    .max_attempts_fn(|request: &OutboundRequest| {
                        if request.retryable {
                            OUTBOUND_MAX_ATTEMPTS
                        } else {
                            1
                        }
                    })
                    .exponential_backoff(OUTBOUND_RETRY_BASE_DELAY)
                    .retry_on(|_| true)
                    .retry_on_response(|response: &OutboundResponse| {
                        retryable_outbound_status(response.status)
                    })
                    .build(),
            )
            .service(service_fn(move |request: OutboundRequest| {
                let client = client.clone();
                async move { execute_outbound_request(&client, request).await }
            })),
    )
}

fn build_media_fetch_service(core: Arc<HttpCore>) -> MediaFetchTowerService {
    BoxCloneSyncService::new(
        ServiceBuilder::new()
            .layer(ConcurrencyLimitLayer::new(MEDIA_FETCH_CONCURRENCY_LIMIT))
            .service(MediaFetchHandler { core }),
    )
}

fn build_browser_json_capture_service(core: Arc<HttpCore>) -> BrowserJsonCaptureTowerService {
    BoxCloneSyncService::new(
        ServiceBuilder::new()
            .layer(ConcurrencyLimitLayer::new(
                BROWSER_CAPTURE_CONCURRENCY_LIMIT,
            ))
            .service(BrowserJsonCaptureHandler { core }),
    )
}

async fn execute_outbound_request(
    client: &reqwest::Client,
    request: OutboundRequest,
) -> std::result::Result<OutboundResponse, OutboundRequestError> {
    let response = build_request(
        client.request(request.method, &request.url),
        &request.headers,
        request.body.as_ref(),
    )
    .timeout(OUTBOUND_TIMEOUT)
    .send()
    .await
    .map_err(|error| OutboundRequestError::new(error.to_string()))?;

    let status = response.status();
    let final_url = response.url().to_string();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map_err(|error| OutboundRequestError::new(error.to_string()))?
        .to_vec();

    Ok(OutboundResponse {
        status,
        headers,
        body,
        final_url,
    })
}

fn retryable_outbound_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::BAD_GATEWAY
        || status == StatusCode::SERVICE_UNAVAILABLE
        || status == StatusCode::GATEWAY_TIMEOUT
        || status.is_server_error()
}

fn outbound_coalesce_key(
    method: &Method,
    url: &str,
    headers: &HeaderMap,
    body: Option<&String>,
    retryable: bool,
) -> OutboundCoalesceKey {
    let unique = (!retryable).then(|| OUTBOUND_COALESCE_NONCE.fetch_add(1, Ordering::Relaxed));

    OutboundCoalesceKey {
        method: method.as_str().to_string(),
        url: url.to_string(),
        headers: normalized_header_key(headers),
        body: body.cloned(),
        unique,
    }
}

fn normalized_header_key(headers: &HeaderMap) -> Vec<(String, Vec<u8>)> {
    let mut normalized = headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_ascii_lowercase(),
                value.as_bytes().to_vec(),
            )
        })
        .collect::<Vec<_>>();
    normalized.sort_unstable();
    normalized
}

fn retryable_request(request: &ExecutableHttpRequest) -> bool {
    request.body.is_none() && matches!(request.method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn user_agent_candidates<'a>(
    solved_user_agent: Option<&'a str>,
    method: &Method,
) -> Vec<Option<&'a str>> {
    if *method == Method::GET || *method == Method::HEAD {
        vec![solved_user_agent, Some(DEFAULT_USER_AGENT), None]
    } else {
        vec![solved_user_agent, Some(DEFAULT_USER_AGENT)]
    }
}

fn executable_request_from_spec(request: &FetchRequestSpec) -> Result<ExecutableHttpRequest> {
    let mut headers = HeaderMap::new();
    for header in &request.headers {
        let name = HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|err| anyhow::anyhow!("Invalid header name '{}': {err}", header.name))?;
        let value = HeaderValue::from_str(&header.value)
            .map_err(|err| anyhow::anyhow!("Invalid header value for '{}': {err}", header.name))?;
        headers.append(name, value);
    }

    Ok(ExecutableHttpRequest {
        url: request.url.clone(),
        method: method_from_string(&request.method)?,
        headers,
        body: request.body.clone(),
        profile: profile_from_spec(&request.purpose),
    })
}

fn method_from_string(method: &str) -> Result<Method> {
    Method::from_bytes(method.trim().as_bytes())
        .map_err(|err| anyhow::anyhow!("Invalid HTTP method '{method}': {err}"))
}

fn profile_from_spec(spec: &RequestPurposeSpec) -> RequestProfile {
    match spec {
        RequestPurposeSpec::Api => RequestProfile::ApiJson,
        RequestPurposeSpec::Document => RequestProfile::HtmlPage,
        RequestPurposeSpec::Image => RequestProfile::ImageHotlink,
        RequestPurposeSpec::Asset => RequestProfile::BinaryAsset,
        RequestPurposeSpec::Custom => RequestProfile::Custom,
    }
}

fn defaulted_headers(request: &ExecutableHttpRequest, binary: bool) -> HeaderMap {
    let mut headers = request.headers.clone();

    if !headers.contains_key(REFERER)
        && let Ok(value) = default_referer_value(&request.url)
    {
        headers.insert(REFERER, value);
    }

    match request.profile {
        RequestProfile::ApiJson => {
            insert_header_if_missing(&mut headers, ACCEPT, "application/json, text/plain, */*");
        }
        RequestProfile::HtmlPage => {
            insert_header_if_missing(
                &mut headers,
                ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            );
        }
        RequestProfile::ImageHotlink => {
            insert_header_if_missing(
                &mut headers,
                ACCEPT,
                "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8",
            );
        }
        RequestProfile::BinaryAsset if binary => {
            insert_header_if_missing(&mut headers, ACCEPT, "*/*");
        }
        RequestProfile::BinaryAsset | RequestProfile::Custom => {}
    }

    headers
}

fn insert_header_if_missing(headers: &mut HeaderMap, name: HeaderName, value: &'static str) {
    if !headers.contains_key(&name)
        && let Ok(value) = HeaderValue::from_str(value)
    {
        headers.insert(name, value);
    }
}

fn build_request(
    mut builder: reqwest::RequestBuilder,
    headers: &HeaderMap,
    body: Option<&String>,
) -> reqwest::RequestBuilder {
    builder = builder.headers(headers.clone());
    if let Some(body) = body {
        builder = builder.body(body.clone());
    }
    builder
}

fn response_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.to_string(), value.to_string()))
        })
        .collect()
}

fn default_referer_value(url: &str) -> std::result::Result<HeaderValue, String> {
    let referer = url::Url::parse(url)
        .ok()
        .and_then(|u| u.domain().map(|d| format!("{}://{d}/", u.scheme())))
        .unwrap_or_else(|| url.to_string());
    HeaderValue::from_str(&referer)
        .map_err(|err| format!("Invalid referer header for '{url}': {err}"))
}

fn request_outcome(status: StatusCode) -> &'static str {
    if status.is_server_error() {
        "error"
    } else if status.is_client_error() {
        "client_error"
    } else {
        "success"
    }
}

fn comix_host(host: Option<&str>) -> bool {
    host.is_some_and(|host| {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        host == "comix.to" || host.ends_with(".comix.to") || comix_media_cdn_host(&host)
    })
}

fn comix_media_cdn_host(host: &str) -> bool {
    let mut labels = host.rsplit('.');
    let Some("store") = labels.next() else {
        return false;
    };
    let Some(domain) = labels.next() else {
        return false;
    };

    domain == "wowpic"
        || domain.strip_prefix("wowpic").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn host_matches_wowpic_media(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .is_some_and(|host| comix_host(Some(&host.to_ascii_lowercase())))
}

fn wowpic_path_variants(url: &str) -> Vec<String> {
    let Ok(parsed) = url::Url::parse(url) else {
        return vec![url.to_string()];
    };
    let Some(current_segment) = wowpic_path_segment(parsed.path()) else {
        return vec![url.to_string()];
    };

    let preferred_segments = wowpic_preferred_path_segments(current_segment);
    let mut variants = Vec::with_capacity(preferred_segments.len());
    for segment in preferred_segments {
        if *segment == current_segment {
            variants.push(url.to_string());
        } else if let Some(alt) = with_path_segment_alternative(&parsed, current_segment, segment) {
            variants.push(alt);
        }
    }

    variants = variants.into_iter().fold(Vec::new(), |mut acc, url| {
        if !acc.contains(&url) {
            acc.push(url);
        }
        acc
    });

    variants
}

fn wowpic_path_segment(path: &str) -> Option<&'static str> {
    WOWPIC_MEDIA_PATH_SEGMENTS
        .iter()
        .copied()
        .find(|segment| path.contains(segment))
}

fn wowpic_preferred_path_segments(current_segment: &str) -> &'static [&'static str] {
    match current_segment {
        WOWPIC_UNSCRAMBLED_PATH_SEGMENT => WOWPIC_UNSCRAMBLED_PREFERRED_SEGMENTS,
        WOWPIC_SCRAMBLED_PATH_SEGMENT => WOWPIC_SCRAMBLED_PREFERRED_SEGMENTS,
        WOWPIC_SECONDARY_SCRAMBLED_PATH_SEGMENT => {
            // Some /sii/ URLs return HTTP 200 with truncated WebP bodies. Prefer stable variants.
            WOWPIC_SECONDARY_SCRAMBLED_PREFERRED_SEGMENTS
        }
        _ => WOWPIC_MEDIA_PATH_SEGMENTS,
    }
}

fn with_path_segment_alternative(url: &url::Url, from: &str, to: &str) -> Option<String> {
    let path = url.path();
    if let Some(after_from) = path.find(from) {
        let new_path = format!(
            "{}{}{}",
            &path[..after_from],
            to,
            &path[after_from + from.len()..]
        );
        if new_path == path {
            return None;
        }

        let mut alt = url.clone();
        alt.set_path(&new_path);
        return Some(alt.to_string());
    }

    None
}

fn binary_candidate_urls(request: &ExecutableHttpRequest) -> Vec<String> {
    if request.profile == RequestProfile::ImageHotlink
        && host_matches_wowpic_media(request.url.as_str())
    {
        return wowpic_path_variants(&request.url);
    }

    vec![request.url.clone()]
}

fn primary_binary_request(
    request: &ExecutableHttpRequest,
    candidate_urls: &[String],
) -> ExecutableHttpRequest {
    let mut primary = request.clone();
    if let Some(candidate_url) = candidate_urls.first() {
        primary.url.clone_from(candidate_url);
    }
    primary
}

#[cfg(test)]
mod tests {
    use super::{
        ExecutableHttpRequest, OutboundResponse, RequestProfile, binary_candidate_urls, comix_host,
        host_matches_wowpic_media, primary_binary_request, response_requires_clearance,
        wowpic_path_variants,
    };
    use reqwest::{
        Method, StatusCode,
        header::{CONTENT_TYPE, HeaderMap, HeaderValue},
    };

    #[test]
    fn comix_host_matches_comix_and_wowpic_hosts() {
        assert!(comix_host(Some("comix.to")));
        assert!(comix_host(Some("static.comix.to")));
        assert!(comix_host(Some("wowpic4.store")));
        assert!(comix_host(Some("80pd.wowpic4.store")));
        assert!(comix_host(Some("ek10.wowpic2.store")));
        assert!(!comix_host(Some("notcomix.to")));
        assert!(!comix_host(Some("comix.to.example")));
        assert!(!comix_host(Some("notwowpic.store")));
        assert!(!comix_host(Some("wowpic4.store.example")));
        assert!(!comix_host(None));
    }

    #[test]
    fn api_json_error_body_does_not_trigger_browser_clearance() {
        let mut headers = HeaderMap::new();
        headers.insert("server", HeaderValue::from_static("cloudflare"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let response = OutboundResponse {
            status: StatusCode::FORBIDDEN,
            headers,
            body: br#"{"message":"Invalid token."}"#.to_vec(),
            final_url: "https://comix.to/api/v1/manga/yegj1/chapters".to_string(),
        };
        let request = ExecutableHttpRequest {
            url: response.final_url.clone(),
            method: Method::GET,
            headers: HeaderMap::new(),
            body: None,
            profile: RequestProfile::ApiJson,
        };

        assert!(!response_requires_clearance(&response, &request));
    }

    #[test]
    fn html_challenge_body_triggers_browser_clearance() {
        let mut headers = HeaderMap::new();
        headers.insert("server", HeaderValue::from_static("cloudflare"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/html"));

        let response = OutboundResponse {
            status: StatusCode::FORBIDDEN,
            headers,
            body: b"<html><title>Just a moment...</title><div id=\"challenge-spinner\"></div>"
                .to_vec(),
            final_url: "https://comix.to/title/yegj1-i-became-a-hatchling".to_string(),
        };
        let request = ExecutableHttpRequest {
            url: response.final_url.clone(),
            method: Method::GET,
            headers: HeaderMap::new(),
            body: None,
            profile: RequestProfile::HtmlPage,
        };

        assert!(response_requires_clearance(&response, &request));
    }

    #[test]
    fn wowpic_media_host_is_detected_for_variants() {
        assert!(host_matches_wowpic_media(
            "https://80pd.wowpic4.store/si/bEqPbYfoKT0GmxHlNhqfsBpU0rkNe/071.webp"
        ));
        assert!(host_matches_wowpic_media(
            "https://ek10.wowpic2.store/i/bEqPbYfoMT0Gm03lHhqfoBpU1r0RUvA/071.webp"
        ));
    }

    #[test]
    fn wowpic_path_variants_include_alternatives() {
        let url = "https://80pd.wowpic4.store/si/bEqPbYfoKT0GmxHlNhqfsBpU0rkNe/071.webp";
        let variants = wowpic_path_variants(url);
        assert_eq!(
            variants,
            vec![
                url.to_string(),
                "https://80pd.wowpic4.store/i/bEqPbYfoKT0GmxHlNhqfsBpU0rkNe/071.webp".to_string(),
                "https://80pd.wowpic4.store/sii/bEqPbYfoKT0GmxHlNhqfsBpU0rkNe/071.webp".to_string()
            ]
        );
    }

    #[test]
    fn wowpic_path_variants_include_sii_alternatives() {
        let url = "https://80pd.wowpic2.store/sii/bEqPbYfoKT0Gm0XlFiqfsBpU0rkJb/04.webp";
        let variants = wowpic_path_variants(url);
        assert_eq!(
            variants,
            vec![
                "https://80pd.wowpic2.store/si/bEqPbYfoKT0Gm0XlFiqfsBpU0rkJb/04.webp".to_string(),
                "https://80pd.wowpic2.store/i/bEqPbYfoKT0Gm0XlFiqfsBpU0rkJb/04.webp".to_string(),
                url.to_string()
            ]
        );
    }

    #[test]
    fn binary_candidate_urls_prefer_si_for_sii_wowpic_urls() {
        let request = ExecutableHttpRequest {
            url: "https://jdpw.wowpic3.store/sii/bEqPbYfoKT0GmxHle1qfhAZY2q0NY/06.webp".to_string(),
            method: Method::GET,
            headers: HeaderMap::new(),
            body: None,
            profile: RequestProfile::ImageHotlink,
        };
        let candidate_urls = binary_candidate_urls(&request);
        assert_eq!(
            candidate_urls,
            vec![
                "https://jdpw.wowpic3.store/si/bEqPbYfoKT0GmxHle1qfhAZY2q0NY/06.webp".to_string(),
                "https://jdpw.wowpic3.store/i/bEqPbYfoKT0GmxHle1qfhAZY2q0NY/06.webp".to_string(),
                "https://jdpw.wowpic3.store/sii/bEqPbYfoKT0GmxHle1qfhAZY2q0NY/06.webp".to_string()
            ]
        );
    }

    #[test]
    fn primary_binary_request_uses_first_wowpic_candidate() {
        let request = ExecutableHttpRequest {
            url: "https://jdpw.wowpic3.store/sii/bEqPbYfoKT0GmxHle1qfhAZY2q0NY/06.webp".to_string(),
            method: Method::GET,
            headers: HeaderMap::new(),
            body: None,
            profile: RequestProfile::ImageHotlink,
        };
        let candidate_urls = binary_candidate_urls(&request);
        let primary = primary_binary_request(&request, &candidate_urls);

        assert_eq!(
            primary.url,
            "https://jdpw.wowpic3.store/si/bEqPbYfoKT0GmxHle1qfhAZY2q0NY/06.webp"
        );
    }

    #[test]
    fn binary_candidate_urls_include_wowpic_variants() {
        let request = ExecutableHttpRequest {
            url: "https://80pd.wowpic4.store/si/bEqPbYfoKT0GmxHlNhqfsBpU0rkNe/071.webp".to_string(),
            method: Method::GET,
            headers: HeaderMap::new(),
            body: None,
            profile: RequestProfile::ImageHotlink,
        };
        let candidate_urls = binary_candidate_urls(&request);
        assert_eq!(
            candidate_urls,
            vec![
                "https://80pd.wowpic4.store/si/bEqPbYfoKT0GmxHlNhqfsBpU0rkNe/071.webp".to_string(),
                "https://80pd.wowpic4.store/i/bEqPbYfoKT0GmxHlNhqfsBpU0rkNe/071.webp".to_string(),
                "https://80pd.wowpic4.store/sii/bEqPbYfoKT0GmxHlNhqfsBpU0rkNe/071.webp".to_string()
            ]
        );
    }
}
