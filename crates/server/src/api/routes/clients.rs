use std::sync::Arc;

use axum::{extract::Path, response::IntoResponse};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    api::{dto::ErrorEnvelopeResponse, error::AppError},
    app::clients,
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(serve_client_package))
}

#[utoipa::path(
    get,
    path = "/v1/clients/{client}/package",
    tag = "clients",
    params(("client" = String, Path, description = "Client id")),
    responses(
        (status = OK, description = "Built client package", content_type = "application/octet-stream"),
        (status = NOT_FOUND, body = ErrorEnvelopeResponse),
        (status = INTERNAL_SERVER_ERROR, body = ErrorEnvelopeResponse)
    )
)]
async fn serve_client_package(Path(client): Path<String>) -> Result<impl IntoResponse, AppError> {
    let package = clients::build_package(&client).await?;
    let response = axum::response::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, package.content_type)
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", package.filename),
        )
        .body(axum::body::Body::from(package.bytes))
        .map_err(AppError::from)?;

    Ok(response)
}
