#[must_use]
pub fn format_http_error(url: &str, status: u16, body: &str) -> String {
    let snippet = body.trim().chars().take(200).collect::<String>();
    if snippet.is_empty() {
        format!("HTTP {status} from {url}")
    } else {
        format!("HTTP {status} from {url}: {snippet}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_error_omits_blank_body_snippets() {
        assert_eq!(
            format_http_error("https://example.test", 500, " \n\t "),
            "HTTP 500 from https://example.test"
        );
    }

    #[test]
    fn http_error_trims_and_limits_body_snippets() {
        let body = format!("  {}extra", "a".repeat(200));
        let error = format_http_error("https://example.test", 502, &body);

        assert_eq!(
            error,
            format!("HTTP 502 from https://example.test: {}", "a".repeat(200))
        );
    }
}
