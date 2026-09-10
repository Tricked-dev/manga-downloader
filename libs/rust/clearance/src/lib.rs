// This crate wraps an operational browser challenge solver. The public errors preserve the
// existing caller-facing strings instead of introducing a larger error taxonomy for now.
#![allow(clippy::missing_errors_doc)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_tungstenite::WebSocketStream;
use async_tungstenite::tokio::{ConnectStream, connect_async};
use async_tungstenite::tungstenite::Message;
use chromiumoxide::browser::Browser;
use chromiumoxide::cdp::browser_protocol::browser::BrowserContextId;
use chromiumoxide::cdp::browser_protocol::network::{Cookie, CookieSameSite};
use chromiumoxide::cdp::browser_protocol::target::{
    CreateBrowserContextParams, CreateTargetParams,
};
use chromiumoxide::handler::HandlerConfig;
use chromiumoxide::page::Page;
use futures_util::StreamExt;
use reqwest::cookie::Jar;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{Mutex, Notify};

const BROWSER_USE_API_KEY_ENV: &str = "BROWSER_USE_API_KEY";
const BROWSER_USE_CONNECT_URL_ENV: &str = "BROWSER_USE_CONNECT_URL";
const BROWSER_USE_PROFILE_ID_ENV: &str = "BROWSER_USE_PROFILE_ID";
const BROWSER_USE_TIMEOUT_MINUTES_ENV: &str = "BROWSER_USE_TIMEOUT_MINUTES";
const BROWSER_USE_DEFAULT_CONNECT_URL: &str = "wss://connect.browser-use.com";
const BROWSER_CDP_URL_ENV: &str = "MANGA_SERVER_BROWSER_CDP_URL";
const CLEARANCE_TIMEOUT: Duration = Duration::from_mins(1);
const CLEARANCE_SESSION_TTL: Duration = Duration::from_hours(1);
const CHALLENGE_POLL_INTERVAL: Duration = Duration::from_millis(500);
const CLEARANCE_WINDOW_WIDTH: u32 = 1280;
const CLEARANCE_WINDOW_HEIGHT: u32 = 720;
const POST_CHALLENGE_STABLE_POLLS: usize = 2;
const CLEARANCE_COOKIE_NAME: &str = "cf_clearance";

const ACCESS_DENIED_TITLES: &[&str] = &["Access denied", "Attention Required! | Cloudflare"];

const ACCESS_DENIED_SELECTORS: &[&str] = &[
    "div.cf-error-title span.cf-code-label span",
    "#cf-error-details div.cf-error-overview h1",
];

const CHALLENGE_TITLES: &[&str] = &["Just a moment...", "DDoS-Guard"];

const CHALLENGE_SELECTORS: &[&str] = &[
    "#cf-challenge-running",
    ".ray_id",
    ".attack-box",
    "#cf-please-wait",
    "#challenge-spinner",
    "#trk_jschal_js",
    "#turnstile-wrapper",
    ".lds-ring",
    "td.info #js_info",
    "div.vc div.text-box h2",
];

#[derive(Clone, Debug)]
pub struct ClearanceSolution {
    pub cookies: Vec<ClearanceCookie>,
    pub user_agent: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BrowserJsonCaptureRequest {
    pub url: String,
    pub user_agent: String,
    pub init_script: String,
    pub done_expression: String,
    pub payloads_expression: String,
    pub timeout: Duration,
    pub poll_interval: Duration,
}

#[derive(Debug, Deserialize)]
struct BrowserJsonCaptureState {
    #[serde(default)]
    done: bool,
    #[serde(default)]
    payloads: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserUseCdpCookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    #[serde(default)]
    secure: bool,
    #[serde(default)]
    http_only: bool,
    #[serde(default)]
    same_site: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ClearanceCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<ClearanceCookieSameSite>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearanceCookieSameSite {
    Strict,
    Lax,
    None,
}

impl ClearanceCookieSameSite {
    const fn as_cookie_value(self) -> &'static str {
        match self {
            Self::Strict => "Strict",
            Self::Lax => "Lax",
            Self::None => "None",
        }
    }
}

impl From<CookieSameSite> for ClearanceCookieSameSite {
    fn from(value: CookieSameSite) -> Self {
        match value {
            CookieSameSite::Strict => Self::Strict,
            CookieSameSite::Lax => Self::Lax,
            CookieSameSite::None => Self::None,
        }
    }
}

#[derive(Clone, Debug)]
struct CachedClearance {
    solution: ClearanceSolution,
    solved_at: Instant,
}

#[derive(Clone)]
enum SessionState {
    Pending(Arc<Notify>),
    Ready(CachedClearance),
}

pub struct ClearanceSolver {
    sessions: Mutex<HashMap<String, SessionState>>,
    browser: Mutex<Option<Arc<BrowserRuntime>>>,
    browser_use: Mutex<Option<BrowserUseCdpClient>>,
}

struct BrowserRuntime {
    browser: Mutex<Browser>,
    handler_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    config: BrowserRuntimeConfig,
}

struct BrowserUseCdpClient {
    endpoint: String,
    socket: WebSocketStream<ConnectStream>,
    next_id: u64,
    session_id: Option<String>,
    target_id: Option<String>,
    stealth_script_id: Option<String>,
    capture_script_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BrowserRuntimeConfig {
    source: BrowserRuntimeSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BrowserRuntimeSource {
    BrowserUse { endpoint: String },
    RemoteCdp { endpoint: String },
}

impl BrowserRuntimeSource {
    const fn provider(&self) -> &'static str {
        match self {
            Self::BrowserUse { .. } => "browser-use",
            Self::RemoteCdp { .. } => "remote-cdp",
        }
    }

    const fn uses_default_context(&self) -> bool {
        matches!(self, Self::BrowserUse { .. } | Self::RemoteCdp { .. })
    }
}

enum BrowserRuntimeContext {
    Browser(BrowserContextId),
    Default,
}

enum SolveError {
    Browser(String),
    Challenge(String),
}

impl SolveError {
    fn into_string(self) -> String {
        match self {
            Self::Browser(error) | Self::Challenge(error) => error,
        }
    }

    fn is_transient_browser_poll_error(&self) -> bool {
        match self {
            Self::Browser(error) => is_transient_browser_poll_error(error),
            Self::Challenge(_) => false,
        }
    }
}

impl BrowserRuntime {
    async fn create_context(&self) -> std::result::Result<BrowserRuntimeContext, String> {
        if self.config.source.uses_default_context() {
            return Ok(BrowserRuntimeContext::Default);
        }

        let browser = self.browser.lock().await;
        browser
            .create_browser_context(CreateBrowserContextParams::default())
            .await
            .map(BrowserRuntimeContext::Browser)
            .map_err(|error| format!("Failed to create embedded browser context: {error}"))
    }

    async fn new_page(
        &self,
        browser_context: &BrowserRuntimeContext,
    ) -> std::result::Result<Page, String> {
        let mut params = CreateTargetParams::builder().url("about:blank");
        if let BrowserRuntimeContext::Browser(browser_context_id) = browser_context {
            params = params.browser_context_id(browser_context_id.clone());
        }
        let params = params
            .build()
            .map_err(|error| format!("Failed to build embedded browser page target: {error}"))?;
        let browser = self.browser.lock().await;
        browser
            .new_page(params)
            .await
            .map_err(|error| format!("Failed to create embedded browser page: {error}"))
    }

    async fn dispose_context(
        &self,
        browser_context: BrowserRuntimeContext,
    ) -> std::result::Result<(), String> {
        let BrowserRuntimeContext::Browser(browser_context_id) = browser_context else {
            return Ok(());
        };

        let browser = self.browser.lock().await;
        browser
            .dispose_browser_context(browser_context_id)
            .await
            .map_err(|error| format!("Failed to dispose embedded browser context: {error}"))
    }

    async fn shutdown(&self) {
        let handler_task = self.handler_task.lock().await.take();
        if let Some(handler_task) = handler_task {
            let mut browser = self.browser.lock().await;
            shutdown_browser(&mut browser, handler_task).await;
        }
    }
}

impl ClearanceSolver {
    #[must_use]
    /// Creates an empty browser-clearance solver.
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            browser: Mutex::new(None),
            browser_use: Mutex::new(None),
        }
    }
    #[allow(clippy::missing_errors_doc)]
    /// Solves a browser challenge for `url` and applies clearance cookies to `jar`.
    ///
    /// Solutions are cached per host for a short session TTL so concurrent
    /// requests do not launch duplicate browser challenges.
    pub async fn solve(
        &self,
        url: &str,
        jar: &Arc<Jar>,
        user_agent: &str,
    ) -> std::result::Result<ClearanceSolution, String> {
        let Some(host_key) = session_host_key(url) else {
            let solution = self.solve_once(url, user_agent).await?;
            apply_clearance_cookies(jar, url, &solution.cookies);
            return Ok(solution);
        };

        loop {
            let waiter = {
                let mut sessions = self.sessions.lock().await;
                match sessions.get(&host_key) {
                    Some(SessionState::Ready(cached))
                        if cached.solved_at.elapsed() < CLEARANCE_SESSION_TTL =>
                    {
                        apply_clearance_cookies(jar, url, &cached.solution.cookies);
                        return Ok(cached.solution.clone());
                    }
                    Some(SessionState::Ready(_)) => {
                        sessions.remove(&host_key);
                        continue;
                    }
                    Some(SessionState::Pending(waiter)) => Err(Arc::clone(waiter)),
                    None => {
                        let waiter = Arc::new(Notify::new());
                        sessions
                            .insert(host_key.clone(), SessionState::Pending(Arc::clone(&waiter)));
                        Ok(waiter)
                    }
                }
            };

            let waiter = match waiter {
                Ok(waiter) => waiter,
                Err(waiter) => {
                    waiter.notified().await;
                    continue;
                }
            };

            let solve_result = self.solve_once(url, user_agent).await;

            let mut sessions = self.sessions.lock().await;
            match &solve_result {
                Ok(solution) => {
                    apply_clearance_cookies(jar, url, &solution.cookies);
                    sessions.insert(
                        host_key.clone(),
                        SessionState::Ready(CachedClearance {
                            solution: solution.clone(),
                            solved_at: Instant::now(),
                        }),
                    );
                }
                Err(_) => {
                    sessions.remove(&host_key);
                }
            }
            drop(sessions);
            waiter.notify_waiters();

            return solve_result;
        }
    }

    /// Invalidates any cached clearance session for `url`'s host.
    pub async fn invalidate(&self, url: &str) {
        if let Some(host_key) = session_host_key(url) {
            let mut sessions = self.sessions.lock().await;
            sessions.remove(&host_key);
        }
    }

    #[allow(clippy::missing_errors_doc)]
    /// Opens a browser page and captures JSON payloads using caller-provided page scripts.
    pub async fn capture_json_payloads(
        &self,
        request: BrowserJsonCaptureRequest,
    ) -> std::result::Result<Vec<String>, String> {
        let runtime_config = configured_browser_runtime_config()?;
        if let BrowserRuntimeSource::BrowserUse { endpoint } = &runtime_config.source {
            return self
                .capture_browser_use_json_payloads(endpoint, &request)
                .await;
        }

        let runtime = self.ensure_browser(runtime_config).await?;
        let browser_context = runtime
            .create_context()
            .await
            .map_err(|error| SolveError::Browser(error).into_string())?;
        let capture_result = self
            .capture_json_payloads_in_context(&runtime, &browser_context, &request)
            .await;

        self.finish_browser_context(&runtime, browser_context, capture_result)
            .await
    }

    async fn solve_once(
        &self,
        url: &str,
        user_agent: &str,
    ) -> std::result::Result<ClearanceSolution, String> {
        let runtime_config = configured_browser_runtime_config()?;
        if let BrowserRuntimeSource::BrowserUse { endpoint } = &runtime_config.source {
            return self.solve_browser_use(endpoint, url, user_agent).await;
        }

        let runtime = self.ensure_browser(runtime_config).await?;
        let browser_context = runtime
            .create_context()
            .await
            .map_err(|error| SolveError::Browser(error).into_string())?;
        let solve_result = self
            .solve_in_context(&runtime, &browser_context, url, user_agent)
            .await;

        if let Err(error) = runtime.dispose_context(browser_context).await {
            tracing::debug!(error = %error, "Embedded Browser Context Dispose Failed");
            self.invalidate_browser_if_current(&runtime).await;
        }

        match solve_result {
            Ok(solution) => Ok(solution),
            Err(SolveError::Browser(error)) => {
                self.invalidate_browser_if_current(&runtime).await;
                Err(error)
            }
            Err(SolveError::Challenge(error)) => Err(error),
        }
    }

    async fn finish_browser_context<T>(
        &self,
        runtime: &Arc<BrowserRuntime>,
        browser_context: BrowserRuntimeContext,
        result: std::result::Result<T, SolveError>,
    ) -> std::result::Result<T, String> {
        if let Err(error) = runtime.dispose_context(browser_context).await {
            tracing::debug!(error = %error, "Embedded Browser Context Dispose Failed");
            self.invalidate_browser_if_current(runtime).await;
        }

        match result {
            Ok(value) => Ok(value),
            Err(SolveError::Browser(error)) => {
                self.invalidate_browser_if_current(runtime).await;
                Err(error)
            }
            Err(SolveError::Challenge(error)) => Err(error),
        }
    }

    async fn solve_in_context(
        &self,
        runtime: &Arc<BrowserRuntime>,
        browser_context: &BrowserRuntimeContext,
        url: &str,
        user_agent: &str,
    ) -> std::result::Result<ClearanceSolution, SolveError> {
        let page = runtime
            .new_page(browser_context)
            .await
            .map_err(SolveError::Browser)?;

        page.enable_stealth_mode_with_agent(user_agent)
            .await
            .map_err(|error| {
                SolveError::Browser(format!("Failed to enable browser stealth mode: {error}"))
            })?;
        page.goto(url).await.map_err(|error| {
            SolveError::Browser(format!("Failed to open challenge page: {error}"))
        })?;

        wait_for_clearance(&page, url).await?;

        let cookies = page.get_cookies().await.map_err(|error| {
            SolveError::Browser(format!("Failed to read browser cookies: {error}"))
        })?;
        if let Err(error) = page.close().await {
            tracing::debug!(error = %error, "Embedded Browser Page Close Failed");
        }

        Ok(ClearanceSolution {
            cookies: cookies.into_iter().map(map_cookie).collect(),
            user_agent: Some(user_agent.to_string()),
        })
    }

    async fn capture_json_payloads_in_context(
        &self,
        runtime: &Arc<BrowserRuntime>,
        browser_context: &BrowserRuntimeContext,
        request: &BrowserJsonCaptureRequest,
    ) -> std::result::Result<Vec<String>, SolveError> {
        let page = runtime
            .new_page(browser_context)
            .await
            .map_err(SolveError::Browser)?;

        page.enable_stealth_mode_with_agent(&request.user_agent)
            .await
            .map_err(|error| {
                SolveError::Browser(format!("Failed to enable browser stealth mode: {error}"))
            })?;
        page.add_init_script(&request.init_script)
            .await
            .map_err(|error| {
                SolveError::Browser(format!("Failed to install browser capture script: {error}"))
            })?;
        page.goto(&request.url).await.map_err(|error| {
            SolveError::Browser(format!(
                "Failed to open browser capture page {}: {error}",
                request.url
            ))
        })?;

        wait_for_clearance(&page, &request.url).await?;
        let payloads = wait_for_json_capture_payloads(&page, request).await?;

        if let Err(error) = page.close().await {
            tracing::debug!(error = %error, "Embedded Browser Page Close Failed");
        }

        Ok(payloads)
    }

    async fn ensure_browser(
        &self,
        runtime_config: BrowserRuntimeConfig,
    ) -> std::result::Result<Arc<BrowserRuntime>, String> {
        let mut browser = self.browser.lock().await;
        if let Some(runtime) = browser.as_ref()
            && runtime.config == runtime_config
        {
            return Ok(Arc::clone(runtime));
        }

        let old_runtime = browser.take();
        drop(browser);
        if let Some(runtime) = old_runtime {
            runtime.shutdown().await;
        }

        let runtime = Arc::new(launch_browser_runtime(runtime_config).await?);
        let mut browser = self.browser.lock().await;
        *browser = Some(Arc::clone(&runtime));
        Ok(runtime)
    }

    async fn invalidate_browser_if_current(&self, runtime: &Arc<BrowserRuntime>) {
        let maybe_runtime = {
            let mut browser = self.browser.lock().await;
            match browser.as_ref() {
                Some(current) if Arc::ptr_eq(current, runtime) => browser.take(),
                _ => None,
            }
        };

        if let Some(runtime) = maybe_runtime {
            runtime.shutdown().await;
        }
    }
}

impl Default for ClearanceSolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Applies clearance cookies to a reqwest cookie jar for the target URL.
pub fn apply_clearance_cookies(jar: &Arc<Jar>, url: &str, cookies: &[ClearanceCookie]) {
    for cookie in cookies {
        let mut cookie_parts = vec![format!("{}={}", cookie.name, cookie.value)];
        cookie_parts.push(format!("Domain={}", cookie.domain));
        cookie_parts.push(format!("Path={}", cookie.path));
        if cookie.secure {
            cookie_parts.push("Secure".to_string());
        }
        if cookie.http_only {
            cookie_parts.push("HttpOnly".to_string());
        }
        if let Some(same_site) = cookie.same_site {
            cookie_parts.push(format!("SameSite={}", same_site.as_cookie_value()));
        }

        if let Ok(cookie_url) = cookie_url(url, &cookie.domain) {
            jar.add_cookie_str(&cookie_parts.join("; "), &cookie_url);
        }
    }
}

impl ClearanceSolver {
    async fn solve_browser_use(
        &self,
        endpoint: &str,
        url: &str,
        user_agent: &str,
    ) -> std::result::Result<ClearanceSolution, String> {
        tracing::info!(url = %url, "Browser Use Clearance Solve Started");
        let mut browser_use = self.browser_use.lock().await;
        let solve_result: std::result::Result<ClearanceSolution, SolveError> = {
            let client = browser_use_client(&mut browser_use, endpoint).await?;
            async {
                client.open_page(url, user_agent, None).await?;
                wait_for_browser_use_clearance(client, url).await?;
                let cookies = client.cookies(url).await.map_err(|error| {
                    SolveError::Browser(format!("Failed to read browser cookies: {error}"))
                })?;

                Ok(ClearanceSolution {
                    cookies: cookies.into_iter().map(map_browser_use_cookie).collect(),
                    user_agent: Some(user_agent.to_string()),
                })
            }
            .await
        };

        if browser_use_operation_broke_client(&solve_result) {
            discard_browser_use_client(&mut browser_use).await;
        }

        if let Ok(solution) = &solve_result {
            tracing::info!(
                url = %url,
                cookie_count = solution.cookies.len(),
                clearance_cookie = solution.cookies.iter().any(|cookie| cookie.name == CLEARANCE_COOKIE_NAME),
                "Browser Use Clearance Solve Completed",
            );
        }

        solve_result.map_err(SolveError::into_string)
    }

    async fn capture_browser_use_json_payloads(
        &self,
        endpoint: &str,
        request: &BrowserJsonCaptureRequest,
    ) -> std::result::Result<Vec<String>, String> {
        tracing::info!(url = %request.url, "Browser Use JSON Capture Started");
        let mut browser_use = self.browser_use.lock().await;
        let capture_result: std::result::Result<Vec<String>, SolveError> = {
            let client = browser_use_client(&mut browser_use, endpoint).await?;
            async {
                client
                    .open_page(
                        &request.url,
                        &request.user_agent,
                        Some(&request.init_script),
                    )
                    .await?;
                wait_for_browser_use_clearance(client, &request.url).await?;
                wait_for_browser_use_json_capture_payloads(client, request).await
            }
            .await
        };

        if browser_use_operation_broke_client(&capture_result) {
            discard_browser_use_client(&mut browser_use).await;
        }

        if let Ok(payloads) = &capture_result {
            tracing::info!(
                url = %request.url,
                payload_count = payloads.len(),
                "Browser Use JSON Capture Completed",
            );
        }

        capture_result.map_err(SolveError::into_string)
    }
}

async fn browser_use_client<'a>(
    slot: &'a mut Option<BrowserUseCdpClient>,
    endpoint: &str,
) -> std::result::Result<&'a mut BrowserUseCdpClient, String> {
    if slot
        .as_ref()
        .is_some_and(|client| client.endpoint != endpoint)
    {
        discard_browser_use_client(slot).await;
    }

    if slot.is_none() {
        *slot = Some(BrowserUseCdpClient::connect(endpoint).await?);
    }

    slot.as_mut()
        .ok_or_else(|| "Browser Use CDP client was not initialized".to_string())
}

async fn discard_browser_use_client(slot: &mut Option<BrowserUseCdpClient>) {
    if let Some(mut client) = slot.take()
        && let Err(error) = client.close_target().await
    {
        tracing::debug!(error = %error, "Browser Use CDP Target Close Failed");
    }
}

fn browser_use_operation_broke_client<T>(result: &std::result::Result<T, SolveError>) -> bool {
    matches!(result, Err(SolveError::Browser(_)))
}

impl BrowserUseCdpClient {
    async fn connect(endpoint: &str) -> std::result::Result<Self, String> {
        backend_tls::ensure_graviola_rustls_provider()
            .map_err(|error| format!("Failed to initialize Browser Use TLS: {error}"))?;
        let (socket, _) = tokio::time::timeout(CLEARANCE_TIMEOUT, connect_async(endpoint))
            .await
            .map_err(|_| {
                format!(
                    "Timed out connecting to Browser Use browser endpoint {}",
                    redacted_browser_endpoint(endpoint)
                )
            })?
            .map_err(|error| {
                format!(
                    "Failed to connect to Browser Use browser endpoint: {}",
                    redact_browser_endpoint_error(&error.to_string(), endpoint)
                )
            })?;

        tracing::info!("Browser Use CDP Connected");

        Ok(Self {
            endpoint: endpoint.to_string(),
            socket,
            next_id: 1,
            session_id: None,
            target_id: None,
            stealth_script_id: None,
            capture_script_id: None,
        })
    }

    async fn open_page(
        &mut self,
        url: &str,
        user_agent: &str,
        init_script: Option<&str>,
    ) -> std::result::Result<(), SolveError> {
        self.ensure_page_target().await?;

        if !user_agent.trim().is_empty() {
            self.page_command(
                "Network.setUserAgentOverride",
                json!({ "userAgent": user_agent }),
            )
            .await
            .map_err(SolveError::Browser)?;
        }

        self.remove_capture_script()
            .await
            .map_err(SolveError::Browser)?;

        if let Some(init_script) = init_script {
            self.capture_script_id = Some(self.add_new_document_script(init_script).await?);
        }

        self.page_command("Page.navigate", json!({ "url": url }))
            .await
            .map_err(SolveError::Browser)?;

        tracing::debug!(url = %url, "Browser Use CDP Page Navigation Started");

        Ok(())
    }

    async fn ensure_page_target(&mut self) -> std::result::Result<(), SolveError> {
        if self.target_id.is_some() && self.session_id.is_some() {
            return Ok(());
        }

        let target = self
            .browser_command("Target.createTarget", json!({ "url": "about:blank" }))
            .await
            .map_err(|error| {
                SolveError::Browser(format!("Failed to create Browser Use CDP target: {error}"))
            })?;
        let target_id = cdp_string_field(&target, "targetId").ok_or_else(|| {
            SolveError::Browser(
                "Browser Use CDP target response did not include targetId".to_string(),
            )
        })?;
        self.target_id = Some(target_id.clone());

        let session = self
            .browser_command(
                "Target.attachToTarget",
                json!({ "targetId": target_id, "flatten": true }),
            )
            .await
            .map_err(|error| {
                SolveError::Browser(format!("Failed to attach Browser Use CDP target: {error}"))
            })?;
        self.session_id = Some(cdp_string_field(&session, "sessionId").ok_or_else(|| {
            SolveError::Browser(
                "Browser Use CDP attach response did not include sessionId".to_string(),
            )
        })?);

        self.page_command("Runtime.enable", json!({}))
            .await
            .map_err(SolveError::Browser)?;
        self.page_command("Page.enable", json!({}))
            .await
            .map_err(SolveError::Browser)?;
        self.page_command("Network.enable", json!({}))
            .await
            .map_err(SolveError::Browser)?;

        self.stealth_script_id = Some(
            self.add_new_document_script(browser_use_stealth_script())
                .await?,
        );

        Ok(())
    }

    async fn add_new_document_script(
        &mut self,
        source: &str,
    ) -> std::result::Result<String, SolveError> {
        let response = self
            .page_command(
                "Page.addScriptToEvaluateOnNewDocument",
                json!({ "source": source }),
            )
            .await
            .map_err(SolveError::Browser)?;
        cdp_string_field(&response, "identifier").ok_or_else(|| {
            SolveError::Browser(
                "Browser Use CDP add script response did not include identifier".to_string(),
            )
        })
    }

    async fn remove_capture_script(&mut self) -> std::result::Result<(), String> {
        if let Some(identifier) = self.capture_script_id.take() {
            self.page_command(
                "Page.removeScriptToEvaluateOnNewDocument",
                json!({ "identifier": identifier }),
            )
            .await?;
        }

        Ok(())
    }

    async fn close_target(&mut self) -> std::result::Result<(), String> {
        self.session_id = None;
        self.stealth_script_id = None;
        self.capture_script_id = None;
        if let Some(target_id) = self.target_id.take() {
            self.browser_command("Target.closeTarget", json!({ "targetId": target_id }))
                .await?;
        }
        self.socket
            .close(None)
            .await
            .map_err(|error| format!("Failed to close Browser Use CDP socket: {error}"))
    }

    async fn title(&mut self) -> std::result::Result<String, String> {
        self.evaluate_string("document.title").await
    }

    async fn current_url(&mut self) -> std::result::Result<String, String> {
        self.evaluate_string("window.location.href").await
    }

    async fn selector_present(&mut self, selector: &str) -> std::result::Result<bool, String> {
        let selector_json = serde_json::to_string(selector)
            .map_err(|error| format!("Failed to encode selector for CDP evaluation: {error}"))?;
        let value = self
            .evaluate_value(&format!("Boolean(document.querySelector({selector_json}))"))
            .await?;
        value.as_bool().ok_or_else(|| {
            format!("Browser Use CDP selector check returned non-boolean value for {selector}")
        })
    }

    async fn cookies(
        &mut self,
        url: &str,
    ) -> std::result::Result<Vec<BrowserUseCdpCookie>, String> {
        let response = self
            .page_command("Network.getCookies", json!({ "urls": [url] }))
            .await?;
        let cookies = response
            .get("cookies")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        serde_json::from_value(cookies)
            .map_err(|error| format!("Failed to decode Browser Use CDP cookies: {error}"))
    }

    async fn evaluate_string(&mut self, expression: &str) -> std::result::Result<String, String> {
        let value = self.evaluate_value(expression).await?;
        Ok(value.as_str().unwrap_or_default().to_string())
    }

    async fn evaluate_value(&mut self, expression: &str) -> std::result::Result<Value, String> {
        let response = self
            .page_command(
                "Runtime.evaluate",
                json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true,
                }),
            )
            .await?;
        if let Some(exception) = response.get("exceptionDetails") {
            return Err(format!(
                "Browser Use CDP evaluation failed: {}",
                cdp_exception_message(exception)
            ));
        }

        Ok(response
            .get("result")
            .and_then(|result| result.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    async fn browser_command(
        &mut self,
        method: &str,
        params: Value,
    ) -> std::result::Result<Value, String> {
        self.send_command(method, params, None).await
    }

    async fn page_command(
        &mut self,
        method: &str,
        params: Value,
    ) -> std::result::Result<Value, String> {
        let session_id = self
            .session_id
            .clone()
            .ok_or_else(|| "Browser Use CDP session is not attached".to_string())?;
        self.send_command(method, params, Some(&session_id))
            .await
            .map_err(|error| format!("{method} failed: {error}"))
    }

    async fn send_command(
        &mut self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> std::result::Result<Value, String> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);

        let mut command = serde_json::Map::new();
        command.insert("id".to_string(), json!(id));
        command.insert("method".to_string(), json!(method));
        command.insert("params".to_string(), params);
        if let Some(session_id) = session_id {
            command.insert("sessionId".to_string(), json!(session_id));
        }

        self.socket
            .send(Message::text(Value::Object(command).to_string()))
            .await
            .map_err(|error| format!("Failed to send Browser Use CDP command {method}: {error}"))?;

        loop {
            let message = tokio::time::timeout(CLEARANCE_TIMEOUT, self.socket.next())
                .await
                .map_err(|_| {
                    format!(
                        "Timed out waiting for Browser Use CDP response to {method} after {} seconds",
                        CLEARANCE_TIMEOUT.as_secs()
                    )
                })?
                .ok_or_else(|| {
                    format!("Browser Use CDP socket closed while waiting for {method}")
                })?
                .map_err(|error| {
                    format!("Browser Use CDP socket failed while waiting for {method}: {error}")
                })?;

            let text = match message {
                Message::Text(text) => text.to_string(),
                Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).map_err(|error| {
                    format!("Browser Use CDP binary response was not UTF-8: {error}")
                })?,
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
                Message::Close(frame) => {
                    return Err(format!(
                        "Browser Use CDP socket closed while waiting for {method}: {frame:?}"
                    ));
                }
            };

            let value = match serde_json::from_str::<Value>(&text) {
                Ok(value) => value,
                Err(error) => {
                    tracing::debug!(
                        error = %error,
                        "Ignoring malformed Browser Use CDP websocket message",
                    );
                    continue;
                }
            };

            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }

            if let Some(error) = value.get("error") {
                return Err(cdp_error_message(method, error));
            }

            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

async fn launch_browser_runtime(
    config: BrowserRuntimeConfig,
) -> std::result::Result<BrowserRuntime, String> {
    tracing::info!(
        provider = config.source.provider(),
        "Browser Runtime Starting",
    );
    match config.source.clone() {
        BrowserRuntimeSource::BrowserUse { endpoint } => {
            connect_browser_runtime(config, "Browser Use", &endpoint).await
        }
        BrowserRuntimeSource::RemoteCdp { endpoint } => {
            connect_browser_runtime(config, "remote CDP", &endpoint).await
        }
    }
}

async fn connect_browser_runtime(
    config: BrowserRuntimeConfig,
    label: &str,
    endpoint: &str,
) -> std::result::Result<BrowserRuntime, String> {
    let handler_config = HandlerConfig {
        request_timeout: CLEARANCE_TIMEOUT,
        ..HandlerConfig::default()
    };
    let (browser, mut handler) = Browser::connect_with_config(endpoint, handler_config)
        .await
        .map_err(|error| {
            format!(
                "Failed to connect to {label} browser endpoint: {}",
                redact_browser_endpoint_error(&error.to_string(), endpoint)
            )
        })?;

    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(error) = event {
                tracing::debug!(error = %error, "Remote Browser Handler Event Failed");
                break;
            }
        }
    });

    Ok(BrowserRuntime {
        browser: Mutex::new(browser),
        handler_task: Mutex::new(Some(handler_task)),
        config,
    })
}

fn configured_browser_runtime_config() -> std::result::Result<BrowserRuntimeConfig, String> {
    if let Some(endpoint) = non_empty_env(BROWSER_CDP_URL_ENV) {
        return Ok(BrowserRuntimeConfig {
            source: BrowserRuntimeSource::RemoteCdp { endpoint },
        });
    }

    if let Some(api_key) = non_empty_env(BROWSER_USE_API_KEY_ENV) {
        let endpoint = browser_use_endpoint(&api_key)?;
        return Ok(BrowserRuntimeConfig {
            source: BrowserRuntimeSource::BrowserUse { endpoint },
        });
    }

    Err(format!(
        "Browser automation is disabled; set {BROWSER_USE_API_KEY_ENV} for Browser Use or set {BROWSER_CDP_URL_ENV} for a remote CDP browser"
    ))
}

fn browser_use_endpoint(api_key: &str) -> std::result::Result<String, String> {
    let base_url = non_empty_env(BROWSER_USE_CONNECT_URL_ENV)
        .unwrap_or_else(|| BROWSER_USE_DEFAULT_CONNECT_URL.to_string());
    let mut endpoint = url::Url::parse(&base_url)
        .map_err(|error| format!("Invalid {BROWSER_USE_CONNECT_URL_ENV} value: {error}"))?;

    set_query_pair(&mut endpoint, "apiKey", api_key.trim());
    set_query_pair(
        &mut endpoint,
        "browserScreenWidth",
        &CLEARANCE_WINDOW_WIDTH.to_string(),
    );
    set_query_pair(
        &mut endpoint,
        "browserScreenHeight",
        &CLEARANCE_WINDOW_HEIGHT.to_string(),
    );

    let timeout_minutes =
        non_empty_env(BROWSER_USE_TIMEOUT_MINUTES_ENV).unwrap_or_else(|| "5".to_string());
    set_query_pair(&mut endpoint, "timeout", &timeout_minutes);

    if let Some(profile_id) = non_empty_env(BROWSER_USE_PROFILE_ID_ENV) {
        set_query_pair(&mut endpoint, "profileId", &profile_id);
    }

    Ok(endpoint.to_string())
}

fn set_query_pair(endpoint: &mut url::Url, key: &str, value: &str) {
    let mut pairs = endpoint
        .query_pairs()
        .filter(|(existing_key, _)| existing_key != key)
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    pairs.push((key.to_string(), value.to_string()));

    endpoint.set_query(None);
    {
        let mut query = endpoint.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(&key, &value);
        }
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn redact_browser_endpoint_error(error: &str, endpoint: &str) -> String {
    let mut redacted = error.replace(endpoint, &redacted_browser_endpoint(endpoint));
    if let Ok(endpoint) = url::Url::parse(endpoint) {
        for (key, value) in endpoint.query_pairs() {
            if key.eq_ignore_ascii_case("apiKey") && !value.is_empty() {
                redacted = redacted.replace(value.as_ref(), "<redacted>");
            }
        }
    }
    redacted
}

fn redacted_browser_endpoint(endpoint: &str) -> String {
    let Ok(mut endpoint) = url::Url::parse(endpoint) else {
        return "<redacted browser endpoint>".to_string();
    };

    let pairs = endpoint
        .query_pairs()
        .map(|(key, value)| {
            let value = if key.eq_ignore_ascii_case("apiKey") {
                "<redacted>".to_string()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect::<Vec<_>>();

    endpoint.set_query(None);
    if !pairs.is_empty() {
        let mut query = endpoint.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(&key, &value);
        }
    }

    endpoint.to_string()
}

async fn shutdown_browser(browser: &mut Browser, handler_task: tokio::task::JoinHandle<()>) {
    if let Err(error) = browser.close().await {
        tracing::debug!(error = %error, "Embedded Browser Close Failed");
        if let Some(kill_result) = browser.kill().await
            && let Err(kill_error) = kill_result
        {
            tracing::debug!(error = %kill_error, "Embedded Browser Kill Failed");
        }
    }

    if let Err(error) = browser.wait().await {
        tracing::debug!(error = %error, "Embedded Browser Wait Failed");
    }

    if let Err(error) = handler_task.await {
        tracing::debug!(error = %error, "Embedded Browser Handler Join Failed");
    }
}

async fn wait_for_browser_use_clearance(
    client: &mut BrowserUseCdpClient,
    url: &str,
) -> std::result::Result<(), SolveError> {
    let started_at = Instant::now();
    let mut challenge_seen = false;
    let mut stable_url: Option<String> = None;
    let mut stable_url_polls = 0usize;

    loop {
        let title = match client.title().await {
            Ok(title) => title,
            Err(error) => {
                let error = SolveError::Browser(format!(
                    "Failed to read page title while solving challenge: {error}"
                ));
                if retry_transient_clearance_poll(&error, started_at, url).await? {
                    continue;
                }
                return Err(error);
            }
        };

        match is_access_denied_browser_use(client, &title).await {
            Ok(true) => {
                return Err(SolveError::Challenge(format!(
                    "Cloudflare blocked access while solving challenge for {url}"
                )));
            }
            Ok(false) => {}
            Err(error) => {
                if retry_transient_clearance_poll(&error, started_at, url).await? {
                    continue;
                }
                return Err(error);
            }
        }

        let challenge_present = match is_challenge_page_browser_use(client, &title).await {
            Ok(challenge_present) => challenge_present,
            Err(error) => {
                if retry_transient_clearance_poll(&error, started_at, url).await? {
                    continue;
                }
                return Err(error);
            }
        };
        if challenge_present {
            stable_url = None;
            stable_url_polls = 0;
            challenge_seen = true;
        } else {
            if !challenge_seen {
                return Ok(());
            }

            match has_clearance_cookie_browser_use(client, url).await {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(error) => {
                    if retry_transient_clearance_poll(&error, started_at, url).await? {
                        continue;
                    }
                    return Err(error);
                }
            }

            let current_url = match client.current_url().await {
                Ok(current_url) => current_url,
                Err(error) => {
                    let error = SolveError::Browser(format!(
                        "Failed to read page URL while solving challenge: {error}"
                    ));
                    if retry_transient_clearance_poll(&error, started_at, url).await? {
                        continue;
                    }
                    return Err(error);
                }
            };

            if current_url == url {
                stable_url = None;
                stable_url_polls = 0;
            } else {
                if stable_url.as_deref() == Some(current_url.as_str()) {
                    stable_url_polls += 1;
                } else {
                    stable_url = Some(current_url);
                    stable_url_polls = 1;
                }

                if stable_url_polls >= POST_CHALLENGE_STABLE_POLLS {
                    return Ok(());
                }
            }
        }

        wait_before_next_clearance_poll(started_at, url).await?;
    }
}

async fn wait_for_clearance(page: &Page, url: &str) -> std::result::Result<(), SolveError> {
    let started_at = Instant::now();
    let mut challenge_seen = false;
    let mut stable_url: Option<String> = None;
    let mut stable_url_polls = 0usize;

    loop {
        let title = match page.get_title().await {
            Ok(title) => title.unwrap_or_default(),
            Err(error) => {
                let error = SolveError::Browser(format!(
                    "Failed to read page title while solving challenge: {error}"
                ));
                if retry_transient_clearance_poll(&error, started_at, url).await? {
                    continue;
                }
                return Err(error);
            }
        };

        match is_access_denied(page, &title).await {
            Ok(true) => {
                return Err(SolveError::Challenge(format!(
                    "Cloudflare blocked access while solving challenge for {url}"
                )));
            }
            Ok(false) => {}
            Err(error) => {
                if retry_transient_clearance_poll(&error, started_at, url).await? {
                    continue;
                }
                return Err(error);
            }
        }

        let challenge_present = match is_challenge_page(page, &title).await {
            Ok(challenge_present) => challenge_present,
            Err(error) => {
                if retry_transient_clearance_poll(&error, started_at, url).await? {
                    continue;
                }
                return Err(error);
            }
        };
        if challenge_present {
            stable_url = None;
            stable_url_polls = 0;
            challenge_seen = true;
        } else {
            if !challenge_seen {
                return Ok(());
            }

            match has_clearance_cookie(page).await {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(error) => {
                    if retry_transient_clearance_poll(&error, started_at, url).await? {
                        continue;
                    }
                    return Err(error);
                }
            }

            let current_url = match page.url().await {
                Ok(current_url) => current_url.unwrap_or_default(),
                Err(error) => {
                    let error = SolveError::Browser(format!(
                        "Failed to read page URL while solving challenge: {error}"
                    ));
                    if retry_transient_clearance_poll(&error, started_at, url).await? {
                        continue;
                    }
                    return Err(error);
                }
            };

            if current_url == url {
                stable_url = None;
                stable_url_polls = 0;
            } else {
                if stable_url.as_deref() == Some(current_url.as_str()) {
                    stable_url_polls += 1;
                } else {
                    stable_url = Some(current_url);
                    stable_url_polls = 1;
                }

                if stable_url_polls >= POST_CHALLENGE_STABLE_POLLS {
                    return Ok(());
                }
            }
        }

        wait_before_next_clearance_poll(started_at, url).await?;
    }
}

async fn wait_for_browser_use_json_capture_payloads(
    client: &mut BrowserUseCdpClient,
    request: &BrowserJsonCaptureRequest,
) -> std::result::Result<Vec<String>, SolveError> {
    let started_at = Instant::now();
    let expression = format!(
        r"(() => ({{
            done: Boolean({done_expression}),
            payloads: Array.isArray({payloads_expression})
                ? {payloads_expression}.slice()
                : []
        }}))()",
        done_expression = request.done_expression,
        payloads_expression = request.payloads_expression,
    );

    loop {
        let state = match client.evaluate_value(expression.as_str()).await {
            Ok(state) => {
                serde_json::from_value::<BrowserJsonCaptureState>(state).map_err(|error| {
                    SolveError::Browser(format!(
                        "Failed to decode browser captured payloads for {}: {error}",
                        request.url
                    ))
                })?
            }
            Err(error) => {
                let error = SolveError::Browser(format!(
                    "Failed to inspect browser captured payloads for {}: {error}",
                    request.url
                ));
                if retry_transient_browser_capture_poll(&error, started_at, request).await? {
                    continue;
                }
                return Err(error);
            }
        };

        if state.done && !state.payloads.is_empty() {
            return Ok(state.payloads);
        }

        if started_at.elapsed() >= request.timeout {
            return Err(SolveError::Challenge(format!(
                "Browser capture did not produce JSON within {} seconds for {}",
                request.timeout.as_secs(),
                request.url
            )));
        }

        tokio::time::sleep(request.poll_interval).await;
    }
}

async fn wait_for_json_capture_payloads(
    page: &Page,
    request: &BrowserJsonCaptureRequest,
) -> std::result::Result<Vec<String>, SolveError> {
    let started_at = Instant::now();
    let expression = format!(
        r"(() => ({{
            done: Boolean({done_expression}),
            payloads: Array.isArray({payloads_expression})
                ? {payloads_expression}.slice()
                : []
        }}))()",
        done_expression = request.done_expression,
        payloads_expression = request.payloads_expression,
    );

    loop {
        let state = match page.evaluate_expression(expression.as_str()).await {
            Ok(state) => state
                .into_value::<BrowserJsonCaptureState>()
                .map_err(|error| {
                    SolveError::Browser(format!(
                        "Failed to decode browser captured payloads for {}: {error}",
                        request.url
                    ))
                })?,
            Err(error) => {
                let error = SolveError::Browser(format!(
                    "Failed to inspect browser captured payloads for {}: {error}",
                    request.url
                ));
                if retry_transient_browser_capture_poll(&error, started_at, request).await? {
                    continue;
                }
                return Err(error);
            }
        };

        if state.done && !state.payloads.is_empty() {
            return Ok(state.payloads);
        }

        if started_at.elapsed() >= request.timeout {
            return Err(SolveError::Challenge(format!(
                "Browser capture did not produce JSON within {} seconds for {}",
                request.timeout.as_secs(),
                request.url
            )));
        }

        tokio::time::sleep(request.poll_interval).await;
    }
}

async fn retry_transient_clearance_poll(
    error: &SolveError,
    started_at: Instant,
    url: &str,
) -> std::result::Result<bool, SolveError> {
    if !error.is_transient_browser_poll_error() {
        return Ok(false);
    }

    tracing::debug!(
        error = %error_message(error),
        "Retrying embedded browser challenge poll after transient navigation error",
    );
    wait_before_next_clearance_poll(started_at, url).await?;
    Ok(true)
}

async fn wait_before_next_clearance_poll(
    started_at: Instant,
    url: &str,
) -> std::result::Result<(), SolveError> {
    if started_at.elapsed() >= CLEARANCE_TIMEOUT {
        return Err(SolveError::Challenge(format!(
            "Cloudflare challenge did not clear within {} seconds for {url}",
            CLEARANCE_TIMEOUT.as_secs()
        )));
    }

    tokio::time::sleep(CHALLENGE_POLL_INTERVAL).await;
    Ok(())
}

async fn retry_transient_browser_capture_poll(
    error: &SolveError,
    started_at: Instant,
    request: &BrowserJsonCaptureRequest,
) -> std::result::Result<bool, SolveError> {
    if !error.is_transient_browser_poll_error() {
        return Ok(false);
    }

    tracing::debug!(
        url = %request.url,
        error = %error_message(error),
        "Retrying embedded browser JSON capture poll after transient navigation error",
    );

    if started_at.elapsed() >= request.timeout {
        return Err(SolveError::Challenge(format!(
            "Browser capture did not produce JSON within {} seconds for {}",
            request.timeout.as_secs(),
            request.url
        )));
    }

    tokio::time::sleep(request.poll_interval).await;
    Ok(true)
}

fn error_message(error: &SolveError) -> &str {
    match error {
        SolveError::Browser(error) | SolveError::Challenge(error) => error,
    }
}

fn is_transient_browser_poll_error(error: &str) -> bool {
    const TRANSIENT_BROWSER_POLL_ERRORS: &[&str] = &[
        "Cannot find context with specified id",
        "Execution context was destroyed",
        "Cannot find context",
    ];

    TRANSIENT_BROWSER_POLL_ERRORS
        .iter()
        .any(|candidate| error.contains(candidate))
}

async fn is_access_denied(page: &Page, title: &str) -> std::result::Result<bool, SolveError> {
    if ACCESS_DENIED_TITLES
        .iter()
        .any(|candidate| title.starts_with(candidate))
    {
        return Ok(true);
    }

    selectors_present(page, ACCESS_DENIED_SELECTORS).await
}

async fn is_access_denied_browser_use(
    client: &mut BrowserUseCdpClient,
    title: &str,
) -> std::result::Result<bool, SolveError> {
    if ACCESS_DENIED_TITLES
        .iter()
        .any(|candidate| title.starts_with(candidate))
    {
        return Ok(true);
    }

    selectors_present_browser_use(client, ACCESS_DENIED_SELECTORS).await
}

async fn is_challenge_page(page: &Page, title: &str) -> std::result::Result<bool, SolveError> {
    if CHALLENGE_TITLES
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(title))
    {
        return Ok(true);
    }

    selectors_present(page, CHALLENGE_SELECTORS).await
}

async fn is_challenge_page_browser_use(
    client: &mut BrowserUseCdpClient,
    title: &str,
) -> std::result::Result<bool, SolveError> {
    if CHALLENGE_TITLES
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(title))
    {
        return Ok(true);
    }

    selectors_present_browser_use(client, CHALLENGE_SELECTORS).await
}

async fn has_clearance_cookie(page: &Page) -> std::result::Result<bool, SolveError> {
    let cookies = page.get_cookies().await.map_err(|error| {
        SolveError::Browser(format!(
            "Failed to inspect browser cookies while solving challenge: {error}"
        ))
    })?;
    Ok(cookies
        .iter()
        .any(|cookie| cookie.name == CLEARANCE_COOKIE_NAME))
}

async fn has_clearance_cookie_browser_use(
    client: &mut BrowserUseCdpClient,
    url: &str,
) -> std::result::Result<bool, SolveError> {
    let cookies = client.cookies(url).await.map_err(|error| {
        SolveError::Browser(format!(
            "Failed to inspect browser cookies while solving challenge: {error}"
        ))
    })?;
    Ok(cookies
        .iter()
        .any(|cookie| cookie.name == CLEARANCE_COOKIE_NAME))
}

async fn selectors_present(
    page: &Page,
    selectors: &[&str],
) -> std::result::Result<bool, SolveError> {
    for selector in selectors {
        let elements = page.find_elements(*selector).await.map_err(|error| {
            SolveError::Browser(format!(
                "Failed to inspect challenge selector '{selector}': {error}"
            ))
        })?;
        if !elements.is_empty() {
            return Ok(true);
        }
    }

    Ok(false)
}

async fn selectors_present_browser_use(
    client: &mut BrowserUseCdpClient,
    selectors: &[&str],
) -> std::result::Result<bool, SolveError> {
    for selector in selectors {
        let present = client.selector_present(selector).await.map_err(|error| {
            SolveError::Browser(format!(
                "Failed to inspect challenge selector '{selector}': {error}"
            ))
        })?;
        if present {
            return Ok(true);
        }
    }

    Ok(false)
}

fn map_cookie(cookie: Cookie) -> ClearanceCookie {
    ClearanceCookie {
        name: cookie.name,
        value: cookie.value,
        domain: cookie.domain,
        path: cookie.path,
        secure: cookie.secure,
        http_only: cookie.http_only,
        same_site: cookie.same_site.map(Into::into),
    }
}

fn map_browser_use_cookie(cookie: BrowserUseCdpCookie) -> ClearanceCookie {
    ClearanceCookie {
        name: cookie.name,
        value: cookie.value,
        domain: cookie.domain,
        path: cookie.path,
        secure: cookie.secure,
        http_only: cookie.http_only,
        same_site: cookie.same_site.as_deref().and_then(map_same_site_value),
    }
}

fn cdp_string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn cdp_error_message(method: &str, error: &Value) -> String {
    if let Some(message) = error.get("message").and_then(Value::as_str) {
        return format!("{method}: {message}");
    }
    format!("{method}: {error}")
}

fn cdp_exception_message(exception: &Value) -> String {
    if let Some(description) = exception
        .get("exception")
        .and_then(|exception| exception.get("description"))
        .and_then(Value::as_str)
    {
        return description.to_string();
    }
    if let Some(text) = exception.get("text").and_then(Value::as_str) {
        return text.to_string();
    }
    exception.to_string()
}

const fn browser_use_stealth_script() -> &'static str {
    r"
        Object.defineProperty(navigator, 'webdriver', { get: () => undefined });
        window.chrome = window.chrome || { runtime: {} };
    "
}

fn map_same_site_value(value: &str) -> Option<ClearanceCookieSameSite> {
    match value {
        "Strict" => Some(ClearanceCookieSameSite::Strict),
        "Lax" => Some(ClearanceCookieSameSite::Lax),
        "None" => Some(ClearanceCookieSameSite::None),
        _ => None,
    }
}

fn session_host_key(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(ToString::to_string))
}

fn cookie_url(original_url: &str, domain: &str) -> std::result::Result<url::Url, String> {
    let parsed = url::Url::parse(original_url)
        .map_err(|error| format!("Invalid challenge URL '{original_url}': {error}"))?;
    let host = domain.trim_start_matches('.');
    let cookie_url = format!("{}://{host}/", parsed.scheme());
    url::Url::parse(&cookie_url)
        .map_err(|error| format!("Invalid cookie domain '{domain}': {error}"))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::{
        BROWSER_CDP_URL_ENV, BROWSER_USE_API_KEY_ENV, BROWSER_USE_CONNECT_URL_ENV,
        BROWSER_USE_PROFILE_ID_ENV, BROWSER_USE_TIMEOUT_MINUTES_ENV, BrowserRuntimeSource,
        configured_browser_runtime_config,
    };

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvSnapshot<'a> {
        _guard: MutexGuard<'a, ()>,
        values: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvSnapshot<'_> {
        fn capture(keys: &[&'static str]) -> Self {
            let guard = ENV_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .expect("env test lock should not be poisoned");
            let values = keys
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect();

            Self {
                _guard: guard,
                values,
            }
        }
    }

    impl Drop for EnvSnapshot<'_> {
        fn drop(&mut self) {
            for (key, value) in &self.values {
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }

    fn capture_browser_env() -> EnvSnapshot<'static> {
        EnvSnapshot::capture(&[
            BROWSER_CDP_URL_ENV,
            BROWSER_USE_API_KEY_ENV,
            BROWSER_USE_CONNECT_URL_ENV,
            BROWSER_USE_PROFILE_ID_ENV,
            BROWSER_USE_TIMEOUT_MINUTES_ENV,
        ])
    }

    #[test]
    fn browser_use_config_is_selected_from_api_key() {
        let _env = capture_browser_env();
        unsafe {
            std::env::remove_var(BROWSER_CDP_URL_ENV);
            std::env::remove_var(BROWSER_USE_CONNECT_URL_ENV);
            std::env::remove_var(BROWSER_USE_PROFILE_ID_ENV);
            std::env::remove_var(BROWSER_USE_TIMEOUT_MINUTES_ENV);
            std::env::set_var(BROWSER_USE_API_KEY_ENV, "test-key");
        }

        let config = configured_browser_runtime_config().unwrap();

        let BrowserRuntimeSource::BrowserUse { endpoint } = config.source else {
            panic!("expected Browser Use runtime");
        };
        assert!(endpoint.starts_with("wss://connect.browser-use.com/?"));
        assert!(endpoint.contains("apiKey=test-key"));
        assert!(endpoint.contains("timeout=5"));
    }
}
