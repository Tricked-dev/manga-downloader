#![allow(clippy::ref_option, clippy::trivially_copy_pass_by_ref)]

use axum::http::StatusCode;
use garde::Validate;

use crate::api::error::AppError;

pub fn validate<T>(value: T) -> Result<T, AppError>
where
    T: Validate<Context = ()>,
{
    value.validate().map_err(validation_error)?;
    Ok(value)
}

pub fn trimmed_non_empty(value: &str, _: &()) -> garde::Result {
    if value.trim().is_empty() {
        Err(garde::Error::new("must not be blank"))
    } else {
        Ok(())
    }
}

pub fn optional_trimmed_non_empty(value: &Option<String>, _: &()) -> garde::Result {
    if value
        .as_deref()
        .is_none_or(|value| !value.trim().is_empty())
    {
        Ok(())
    } else {
        Err(garde::Error::new("must not be blank"))
    }
}

fn validation_error(report: garde::Report) -> AppError {
    let violations = report
        .iter()
        .map(|(path, error)| {
            let path = path.to_string();
            if path.is_empty() {
                serde_json::json!({
                    "message": error.message(),
                })
            } else {
                serde_json::json!({
                    "path": path,
                    "message": error.message(),
                })
            }
        })
        .collect::<Vec<_>>();

    AppError::new(
        StatusCode::BAD_REQUEST,
        "validation_error",
        "Request validation failed",
        Some(serde_json::json!({ "violations": violations })),
    )
    .with_source(report)
}
