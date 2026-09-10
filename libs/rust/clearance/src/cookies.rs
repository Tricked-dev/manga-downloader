use std::sync::Arc;

use chromiumoxide::cdp::browser_protocol::network::Cookie;
use reqwest::cookie::Jar;

use super::ClearanceCookie;

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

pub(super) fn map_cookie(cookie: Cookie) -> ClearanceCookie {
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

pub(super) fn session_host_key(url: &str) -> Option<String> {
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
