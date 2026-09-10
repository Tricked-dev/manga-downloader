#[cfg(test)]
use serde::de::DeserializeOwned;

use crate::kit::error::format_http_error;

/// # Errors
///
/// Returns a formatted HTTP error when `status` is outside the 2xx range.
pub fn expect_success(url: &str, status: u16, body: String) -> Result<String, String> {
    if (200..=299).contains(&status) {
        Ok(body)
    } else {
        Err(format_http_error(url, status, &body))
    }
}

/// # Errors
///
/// Returns a contextual parse error when `body` is not valid JSON for `T`.
#[cfg(test)]
pub fn parse_json<T: DeserializeOwned>(body: &str, context: &str) -> Result<T, String> {
    serde_json::from_str(body).map_err(|err| format!("Failed to parse {context}: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Fixture {
        title: String,
    }

    #[test]
    fn expect_success_returns_body_for_success_statuses() {
        let body = expect_success("https://example.test/page", 204, "ok".to_string())
            .expect("success response should keep body");

        assert_eq!(body, "ok");
    }

    #[test]
    fn expect_success_formats_error_for_non_success_statuses() {
        let error = expect_success("https://example.test/page", 404, "missing".to_string())
            .expect_err("error response should be reported");

        assert!(error.contains("https://example.test/page"));
        assert!(error.contains("404"));
        assert!(error.contains("missing"));
    }

    #[test]
    fn parse_json_reports_context_on_invalid_json() {
        let parsed: Fixture =
            parse_json(r#"{"title":"A"}"#, "fixture").expect("fixture should parse");
        assert_eq!(
            parsed,
            Fixture {
                title: "A".to_string()
            }
        );

        let error = parse_json::<Fixture>("not json", "fixture")
            .expect_err("invalid json should include context");
        assert!(error.contains("fixture"));
    }
}
