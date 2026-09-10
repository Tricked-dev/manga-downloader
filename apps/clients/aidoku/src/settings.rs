use aidoku::{
    alloc::{string::String, vec::Vec},
    imports::defaults::defaults_get,
};

const DEFAULT_SERVER_BASE_URL: &str = "http://localhost:4000";
const DEFAULT_SOURCE_NAMES: &str = "comix";

const SERVER_BASE_URL_KEY: &str = "serverBaseUrl";
const BACKEND_API_KEY: &str = "backendApiKey";

pub fn server_base_url() -> String {
    let value = defaults_get::<String>(SERVER_BASE_URL_KEY).unwrap_or_default();
    let trimmed = value.trim().trim_end_matches('/');

    if trimmed.is_empty() {
        DEFAULT_SERVER_BASE_URL.into()
    } else {
        trimmed.into()
    }
}

pub fn fallback_source_names() -> Vec<String> {
    parse_csv(DEFAULT_SOURCE_NAMES)
}

fn parse_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(Into::into)
        .collect()
}

pub fn backend_api_key() -> String {
    defaults_get::<String>(BACKEND_API_KEY)
        .unwrap_or_default()
        .trim()
        .into()
}
