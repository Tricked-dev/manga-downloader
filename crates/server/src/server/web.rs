//! The same SPA routes work with compiled assets and a development directory.
use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use std::{convert::Infallible, path::PathBuf, sync::Arc};
use tower::ServiceExt;
use tower_http::services::ServeDir;

#[cfg(feature = "web")]
#[derive(rust_embed::Embed)]
#[folder = "../../web/build/"]
struct EmbeddedWeb;

pub(super) fn router(root: Option<PathBuf>) -> Router {
    let root = Arc::new(root);
    Router::new().fallback_service(tower::service_fn(move |request: Request| {
        let root = Arc::clone(&root);
        async move { Ok::<_, Infallible>(serve(request, root.as_ref().as_ref()).await) }
    }))
}

async fn serve(mut request: Request, root: Option<&PathBuf>) -> Response {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let decoded = match percent_encoding::percent_decode_str(request.uri().path()).decode_utf8() {
        Ok(path) => path.into_owned(),
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    if decoded.split('/').any(|part| matches!(part, "." | ".."))
        || decoded.contains('\\')
        || decoded.chars().any(char::is_control)
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if ["/v1", "/auth", "/api"]
        .iter()
        .any(|prefix| decoded == *prefix || decoded.starts_with(&format!("{prefix}/")))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let asset = if is_app_path(&decoded) {
        "index.html"
    } else {
        decoded.trim_start_matches('/')
    };
    let mut response = if let Some(root) = root {
        if asset == "index.html" {
            *request.uri_mut() = "/index.html".parse().unwrap();
        }
        match ServeDir::new(root).oneshot(request).await {
            Ok(response) => response.map(Body::new),
            Err(error) => match error {},
        }
    } else {
        embedded(asset, request.method(), request.headers())
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        if asset.starts_with("_app/immutable/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        }
        .parse()
        .unwrap(),
    );
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
}

fn is_app_path(path: &str) -> bool {
    matches!(
        path,
        "/" | "/login"
            | "/about"
            | "/clients"
            | "/downloads"
            | "/library"
            | "/settings"
            | "/sources"
            | "/stats"
            | "/updates"
    ) || ["/library/", "/manga/", "/read/", "/sources/"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

#[cfg(feature = "web")]
fn embedded(asset: &str, method: &Method, headers: &axum::http::HeaderMap) -> Response {
    use base64::Engine as _;
    let Some(file) = EmbeddedWeb::get(asset) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let etag = format!(
        "\"{}\"",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(file.metadata.sha256_hash())
    );
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.split(',').any(|candidate| {
                candidate.trim().trim_start_matches("W/") == etag || candidate.trim() == "*"
            })
        })
    {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response();
    }
    let size = file.data.len();
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(file.data)
    };
    Response::builder()
        .header(
            header::CONTENT_TYPE,
            mime_guess::from_path(asset)
                .first_or_octet_stream()
                .as_ref(),
        )
        .header(header::CONTENT_LENGTH, size)
        .header(header::ETAG, etag)
        .body(body)
        .unwrap()
}

#[cfg(not(feature = "web"))]
fn embedded(_asset: &str, _method: &Method, _headers: &axum::http::HeaderMap) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "The web UI is not embedded. Build with the web feature or provide --web-root.",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;

    #[tokio::test]
    async fn disk_override_serves_spa_and_assets_without_masking_missing_api_routes() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("index.html"), "<html>Disk UI</html>").unwrap();
        std::fs::write(root.path().join("app.js"), "export const ok = true;").unwrap();
        let server = TestServer::new(router(Some(root.path().to_owned())));
        server
            .get("/library/one")
            .await
            .assert_status_ok()
            .assert_text("<html>Disk UI</html>");
        server.get("/app.js").await.assert_status_ok();
        server.get("/missing.js").await.assert_status_not_found();
        server.get("/v1/unknown").await.assert_status_not_found();
        server.get("/auth/unknown").await.assert_status_not_found();
        server
            .post("/library/one")
            .await
            .assert_status(StatusCode::METHOD_NOT_ALLOWED);
        // Send the raw URI: a browser/TestServer URL parser normalizes dot segments first.
        let response = serve(
            Request::builder()
                .uri("/%2e%2e/secret")
                .body(Body::empty())
                .unwrap(),
            Some(&root.path().to_owned()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "web")]
    #[tokio::test]
    async fn embedded_ui_serves_html_and_supports_conditional_requests() {
        let server = TestServer::new(router(None));
        let response = server.get("/").await;
        response
            .assert_status_ok()
            .assert_header(header::CONTENT_TYPE, "text/html");
        assert!(response.text().contains("_app/immutable/"));
        let etag = response.headers()[header::ETAG].to_str().unwrap();
        server
            .get("/library/one")
            .add_header(header::IF_NONE_MATCH, etag)
            .await
            .assert_status(StatusCode::NOT_MODIFIED);
        server.get("/missing.js").await.assert_status_not_found();
        server.get("/v1/unknown").await.assert_status_not_found();
    }
}
