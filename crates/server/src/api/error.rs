use axum::{Json, http::StatusCode, response::IntoResponse};
use backend_sources::{SourceError, SourceRegistryError};
use backend_telemetry::{record_request_error, trace};
use serde::Serialize;

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
    details: Option<serde_json::Value>,
    source: Option<anyhow::Error>,
}

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorPayload,
}

#[derive(Serialize)]
struct ErrorPayload {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

impl AppError {
    pub fn new(
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
        details: Option<serde_json::Value>,
    ) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            details,
            source: None,
        }
    }

    #[must_use]
    pub fn with_source(mut self, source: impl Into<anyhow::Error>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn bad_request(err: impl Into<anyhow::Error>) -> Self {
        let error = err.into();
        Self::new(
            StatusCode::BAD_REQUEST,
            "validation_error",
            error.to_string(),
            None,
        )
        .with_source(error)
    }

    pub fn not_found(err: impl Into<anyhow::Error>) -> Self {
        let error = err.into();
        Self::new(StatusCode::NOT_FOUND, "not_found", error.to_string(), None).with_source(error)
    }

    pub fn internal(err: impl Into<anyhow::Error>) -> Self {
        let error = err.into();
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "internal server error",
            None,
        )
        .with_source(error)
    }

    pub fn upstream_blocked(err: impl Into<anyhow::Error>) -> Self {
        let error = err.into();
        Self::new(
            StatusCode::BAD_GATEWAY,
            "upstream_blocked",
            "upstream source blocked this server's request",
            plugin_error_details(&error),
        )
        .with_source(error)
    }

    pub fn downloaded_page_conflict(err: impl Into<anyhow::Error>) -> Self {
        let error = err.into();
        Self::new(
            StatusCode::CONFLICT,
            "downloaded_page_conflict",
            error.to_string(),
            None,
        )
        .with_source(error)
    }

    #[must_use]
    pub fn is_downloaded_page_conflict(&self) -> bool {
        self.code == "downloaded_page_conflict"
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status_code = self.status.as_u16();
        let outcome = if self.status.is_server_error() {
            "error"
        } else {
            "client_error"
        };
        record_request_error(self.status.as_u16(), self.code, outcome);
        let correlation = trace::current_trace_context();
        let trace_id = correlation.trace_id();
        let span_id = correlation.span_id();

        if let Some(source) = &self.source {
            if self.status.is_server_error() {
                tracing::error!(
                    trace_id,
                    span_id,
                    status_code,
                    code = self.code,
                    error = %source,
                    outcome,
                    "Request Error",
                );
            } else {
                tracing::warn!(
                    trace_id,
                    span_id,
                    status_code,
                    code = self.code,
                    error = %source,
                    outcome,
                    "Request Error",
                );
            }
        } else {
            tracing::warn!(
                trace_id,
                span_id,
                status_code,
                code = self.code,
                message = %self.message,
                outcome,
                "Request Error",
            );
        }

        (
            self.status,
            Json(ErrorEnvelope {
                error: ErrorPayload {
                    code: self.code,
                    message: self.message,
                    details: self.details,
                },
            }),
        )
            .into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        if plugin_runtime_error(&err).is_some_and(SourceError::is_upstream_blocked) {
            return Self::upstream_blocked(err);
        }

        let plugin_status = err
            .downcast_ref::<SourceRegistryError>()
            .map(|plugin_error| match plugin_error {
                SourceRegistryError::NotFound { .. } => StatusCode::NOT_FOUND,
                SourceRegistryError::Disabled { .. }
                | SourceRegistryError::UnsupportedCapability { .. } => StatusCode::CONFLICT,
            });

        if let Some(status) = plugin_status {
            return match status {
                StatusCode::NOT_FOUND => Self::not_found(err),
                StatusCode::CONFLICT => {
                    Self::new(StatusCode::CONFLICT, "conflict", err.to_string(), None)
                        .with_source(err)
                }
                _ => Self::internal(err),
            };
        }

        Self::internal(err)
    }
}

impl From<http::Error> for AppError {
    fn from(err: http::Error) -> Self {
        Self::internal(err)
    }
}

fn plugin_runtime_error(error: &anyhow::Error) -> Option<&SourceError> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<SourceError>())
}

fn plugin_error_details(error: &anyhow::Error) -> Option<serde_json::Value> {
    plugin_runtime_error(error).map(|plugin_error| {
        serde_json::json!({
            "plugin_code": &plugin_error.code,
            "retryable": plugin_error.retryable,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_cloudflare_plugin_block_to_upstream_blocked() {
        let error = anyhow::Error::new(SourceError {
            code: "search_failed".to_string(),
            message: "fetch_failed: Cloudflare blocked access while solving challenge for https://comix.to/api/v1/manga?page=1&limit=28".to_string(),
            retryable: true,
        });

        let app_error = AppError::from(error);

        assert_eq!(app_error.status, StatusCode::BAD_GATEWAY);
        assert_eq!(app_error.code, "upstream_blocked");
        assert_eq!(
            app_error.message,
            "upstream source blocked this server's request"
        );
        assert_eq!(
            app_error.details,
            Some(serde_json::json!({
                "plugin_code": "search_failed",
                "retryable": true,
            }))
        );
    }

    #[test]
    fn leaves_non_blocked_plugin_errors_internal() {
        let error = anyhow::Error::new(SourceError {
            code: "parse_search_response".to_string(),
            message: "Failed to parse Comix search response".to_string(),
            retryable: false,
        });

        let app_error = AppError::from(error);

        assert_eq!(app_error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(app_error.code, "internal_error");
        assert_eq!(app_error.message, "internal server error");
        assert!(app_error.details.is_none());
    }
}
