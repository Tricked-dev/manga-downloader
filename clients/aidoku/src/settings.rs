use aidoku::{
    alloc::{string::String, vec::Vec},
    imports::defaults::{DefaultValue, defaults_get, defaults_set},
};

const DEFAULT_SERVER_BASE_URL: &str = "http://localhost:4000";
const DEFAULT_SOURCE_NAMES: &str = "comix";

const SERVER_BASE_URL_KEY: &str = "serverBaseUrl";
const BACKEND_API_KEY: &str = "backendApiKey";
/// Written by the sign-in page into the web view's local storage.
const LOGIN_TOKEN_KEY: &str = "mangaApiToken";
/// The login control reads its URL from here, so it follows the configured server.
const LOGIN_URL_KEY: &str = "loginUrl";
const LOGIN_PATH: &str = "/auth/aidoku";

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

/// A token obtained by signing in wins over one pasted by hand, so signing in takes
/// effect without the old value having to be cleared first.
pub fn backend_api_key() -> String {
    let signed_in: String = defaults_get::<String>(LOGIN_TOKEN_KEY)
        .unwrap_or_default()
        .trim()
        .into();
    if !signed_in.is_empty() {
        return signed_in;
    }
    defaults_get::<String>(BACKEND_API_KEY)
        .unwrap_or_default()
        .trim()
        .into()
}

/// Keeps the login URL pointing at whichever server the user configured.
pub fn sync_login_url(base_url: &str) {
    let mut url = String::from(base_url);
    url.push_str(LOGIN_PATH);
    if defaults_get::<String>(LOGIN_URL_KEY).unwrap_or_default() != url {
        defaults_set(LOGIN_URL_KEY, DefaultValue::String(url));
    }
}
