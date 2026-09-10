use crate::{
    fetch::{DEFAULT_USER_AGENT, ExecutableHttpRequest, RequestProfile, SourceHttpClient},
    types,
};
use backend_clearance::BrowserJsonCaptureRequest as ClearanceBrowserJsonCaptureRequest;
use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::time::Duration;
impl SourceHttpClient {
    pub async fn fetch(
        &self,
        request: &types::FetchRequest,
    ) -> Result<types::FetchResponse, String> {
        let response = self
            .execute_http_request(to_executable_request(request.clone())?)
            .await?;
        Ok(types::FetchResponse {
            status: response.status,
            headers: response
                .headers
                .into_iter()
                .map(|(name, value)| types::HttpHeader { name, value })
                .collect(),
            body: response.body,
            final_url: response.final_url,
        })
    }
    pub async fn capture_browser_json(
        &self,
        request: &types::BrowserJsonCaptureRequest,
    ) -> Result<types::BrowserJsonCaptureResponse, String> {
        let payloads = self
            .capture_browser_json_payloads(to_browser_json_capture_request(request.clone()))
            .await
            .map_err(|e| e.to_string())?;
        Ok(types::BrowserJsonCaptureResponse { payloads })
    }
}
fn to_executable_request(
    request: types::FetchRequest,
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

fn to_reqwest_method(method: types::HttpMethod) -> Method {
    match method {
        types::HttpMethod::Get => Method::GET,
        types::HttpMethod::Post => Method::POST,
        types::HttpMethod::Put => Method::PUT,
        types::HttpMethod::Patch => Method::PATCH,
        types::HttpMethod::Delete => Method::DELETE,
        types::HttpMethod::Head => Method::HEAD,
        types::HttpMethod::Options => Method::OPTIONS,
    }
}

fn to_request_profile(purpose: types::RequestPurpose) -> RequestProfile {
    match purpose {
        types::RequestPurpose::Api => RequestProfile::ApiJson,
        types::RequestPurpose::Document => RequestProfile::HtmlPage,
        types::RequestPurpose::Image => RequestProfile::ImageHotlink,
        types::RequestPurpose::Asset => RequestProfile::BinaryAsset,
        types::RequestPurpose::Custom => RequestProfile::Custom,
    }
}

fn to_browser_json_capture_request(
    request: types::BrowserJsonCaptureRequest,
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
