use std::time::Duration;

use backend_clearance::BrowserJsonCaptureRequest as ClearanceBrowserJsonCaptureRequest;
use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use wasmtime::{Store, component::ResourceTable};
use wasmtime_wasi::WasiCtxBuilder;

use super::fetch::{DEFAULT_USER_AGENT, ExecutableHttpRequest, PluginHttpClient, RequestProfile};
use super::runtime::{MangaSourceImports, manga};

pub struct PluginState {
    wasi_ctx: wasmtime_wasi::WasiCtx,
    table: ResourceTable,
    http: PluginHttpClient,
}

impl PluginState {
    /// Creates host state for one plugin component store.
    pub fn new(http: PluginHttpClient) -> Self {
        let wasi_ctx = WasiCtxBuilder::new()
            .inherit_stdout()
            .inherit_stderr()
            .build();

        Self {
            wasi_ctx,
            table: ResourceTable::new(),
            http,
        }
    }
}

/// Creates a Wasmtime store with the plugin host state installed.
pub fn create_store(engine: &wasmtime::Engine, http: PluginHttpClient) -> Store<PluginState> {
    let state = PluginState::new(http);
    Store::new(engine, state)
}

impl wasmtime_wasi::WasiView for PluginState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.table,
        }
    }
}

impl manga::source::types::Host for PluginState {}

impl MangaSourceImports for PluginState {
    fn fetch(
        &mut self,
        request: manga::source::types::FetchRequest,
    ) -> std::result::Result<manga::source::types::FetchResponse, String> {
        let executable = to_executable_request(request)?;
        let http = &self.http;

        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let response = http.execute_http_request(executable).await?;
                Ok(manga::source::types::FetchResponse {
                    status: response.status,
                    headers: response
                        .headers
                        .into_iter()
                        .map(|(name, value)| manga::source::types::HttpHeader { name, value })
                        .collect(),
                    body: response.body,
                    final_url: response.final_url,
                })
            })
        })
    }

    fn capture_browser_json(
        &mut self,
        request: manga::source::types::BrowserJsonCaptureRequest,
    ) -> std::result::Result<manga::source::types::BrowserJsonCaptureResponse, String> {
        let capture_request = to_browser_json_capture_request(request);
        let http = &self.http;

        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let payloads = match http.capture_browser_json_payloads(capture_request).await {
                    Ok(payloads) => payloads,
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "Plugin Browser JSON Capture Failed",
                        );
                        Vec::new()
                    }
                };
                Ok(manga::source::types::BrowserJsonCaptureResponse { payloads })
            })
        })
    }
}

fn to_executable_request(
    request: manga::source::types::FetchRequest,
) -> std::result::Result<ExecutableHttpRequest, String> {
    let mut headers = HeaderMap::new();
    for header in request.headers {
        let name = HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|err| format!("Invalid header name '{}': {err}", header.name))?;
        let value = HeaderValue::from_str(&header.value)
            .map_err(|err| format!("Invalid header value for '{}': {err}", header.name))?;
        headers.append(name, value);
    }

    Ok(ExecutableHttpRequest {
        url: request.url,
        method: to_reqwest_method(request.method),
        headers,
        body: request.body,
        profile: to_request_profile(request.purpose),
    })
}

fn to_reqwest_method(method: manga::source::types::HttpMethod) -> Method {
    match method {
        manga::source::types::HttpMethod::Get => Method::GET,
        manga::source::types::HttpMethod::Post => Method::POST,
        manga::source::types::HttpMethod::Put => Method::PUT,
        manga::source::types::HttpMethod::Patch => Method::PATCH,
        manga::source::types::HttpMethod::Delete => Method::DELETE,
        manga::source::types::HttpMethod::Head => Method::HEAD,
        manga::source::types::HttpMethod::Options => Method::OPTIONS,
    }
}

fn to_request_profile(purpose: manga::source::types::RequestPurpose) -> RequestProfile {
    match purpose {
        manga::source::types::RequestPurpose::Api => RequestProfile::ApiJson,
        manga::source::types::RequestPurpose::Document => RequestProfile::HtmlPage,
        manga::source::types::RequestPurpose::Image => RequestProfile::ImageHotlink,
        manga::source::types::RequestPurpose::Asset => RequestProfile::BinaryAsset,
        manga::source::types::RequestPurpose::Custom => RequestProfile::Custom,
    }
}

fn to_browser_json_capture_request(
    request: manga::source::types::BrowserJsonCaptureRequest,
) -> ClearanceBrowserJsonCaptureRequest {
    ClearanceBrowserJsonCaptureRequest {
        url: request.url,
        user_agent: DEFAULT_USER_AGENT.to_string(),
        init_script: request.init_script,
        done_expression: request.done_expression,
        payloads_expression: request.payloads_expression,
        timeout: duration_from_millis(request.timeout_ms),
        poll_interval: duration_from_millis(request.poll_interval_ms),
    }
}

fn duration_from_millis(millis: u32) -> Duration {
    Duration::from_millis(u64::from(millis.max(1)))
}
