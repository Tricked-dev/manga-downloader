use std::{future::Future, pin::Pin, sync::Arc};

use axum::{
    body::Body,
    extract::Request,
    http::{Method, Response, StatusCode, header},
};
use secrecy::ExposeSecret;
use tower_http::auth::AsyncAuthorizeRequest;

use crate::AppState;

#[derive(Clone)]
pub struct BearerAuthorization {
    state: Arc<AppState>,
}

impl BearerAuthorization {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl<B> AsyncAuthorizeRequest<B> for BearerAuthorization
where
    B: Send + 'static,
{
    type RequestBody = B;
    type ResponseBody = Body;
    type Future = Pin<Box<dyn Future<Output = Result<Request<B>, Response<Body>>> + Send>>;

    fn authorize(&mut self, request: Request<B>) -> Self::Future {
        let expected_api_key = self.state.config.backend_api_key.clone();
        let path = request.uri().path().to_string();

        Box::pin(async move {
            let Some(expected_api_key) = expected_api_key else {
                return Ok(request);
            };

            if request.method() == Method::OPTIONS {
                return Ok(request);
            }

            if is_public_path(&path) {
                return Ok(request);
            }

            if bearer_token(request.headers())
                .is_some_and(|token| token == expected_api_key.expose_secret())
            {
                return Ok(request);
            }

            Err(Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::WWW_AUTHENTICATE, "Bearer")
                .body(Body::from(
                    serde_json::json!({
                        "error": {
                            "code": "invalid_api_key",
                            "message": "A valid bearer token is required",
                        }
                    })
                    .to_string(),
                ))
                .expect("unauthorized auth response"))
        })
    }
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    let authorization = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    authorization.strip_prefix("Bearer ").map(str::trim)
}

fn is_public_path(path: &str) -> bool {
    path == "/v1/health"
}
