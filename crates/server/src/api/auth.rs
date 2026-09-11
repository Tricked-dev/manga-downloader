//! Browser authentication and API authorization share the application's database.
pub(crate) mod oidc;

use crate::{AppState, api::error::AppError};
use anyhow::{Result, ensure};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use backend_core::settings::SettingKey;
use backend_persistence::AuthUser;
use base64::Engine as _;
use openidconnect::{ClientSecret, CsrfToken};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use tower_http::auth::AsyncAuthorizeRequest;

const SESSION_COOKIE: &str = "manga_session";
const LOGIN_COOKIE: &str = "manga_login";
const SHARE_COOKIE: &str = "manga_public_share";
const SESSION_SECONDS: u64 = 7 * 24 * 60 * 60;

pub(super) struct AuthConfig {
    enabled: bool,
    issuer: String,
    client_id: String,
    client_secret: Option<ClientSecret>,
    scopes: String,
    public_url: Option<String>,
    api_key: Option<String>,
    fingerprint: String,
}

impl AuthConfig {
    async fn read(state: &AppState) -> Result<Self> {
        let settings = state.db.get_all_settings().await?;
        let value = |key: SettingKey| {
            settings
                .get(key.as_str())
                .cloned()
                .unwrap_or_else(|| key.default_value().into())
        };
        let api_key = value(SettingKey::BackendApiKey);
        let api_key = if api_key.is_empty() {
            state
                .config
                .backend_api_key
                .as_ref()
                .map(|key| key.expose_secret().to_owned())
        } else {
            Some(api_key)
        };
        let secret = value(SettingKey::AuthOidcClientSecret);
        let enabled = value(SettingKey::AuthEnabled) != "false";
        let issuer = value(SettingKey::AuthOidcIssuerUrl);
        let client_id = value(SettingKey::AuthOidcClientId);
        let scopes = value(SettingKey::AuthOidcScopes);
        let public_url = state.config.public_url.clone();
        let fingerprint = token_hash(&serde_json::to_string(&(
            enabled,
            &issuer,
            &client_id,
            &secret,
            &scopes,
            &public_url,
            &api_key,
        ))?);
        Ok(Self {
            enabled,
            issuer,
            client_id,
            client_secret: (!secret.is_empty()).then(|| ClientSecret::new(secret)),
            scopes,
            public_url,
            api_key,
            fingerprint,
        })
    }
    fn protected(&self) -> bool {
        self.enabled || self.api_key.is_some()
    }
    fn oidc_configured(&self) -> bool {
        self.enabled
            && !self.issuer.is_empty()
            && !self.client_id.is_empty()
            && self.public_url.is_some()
    }
    fn callback_url(&self) -> Result<String> {
        Ok(format!(
            "{}/auth/callback",
            self.public_url
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("PUBLIC_URL is required for sign-in"))?
        ))
    }
    fn secure_cookie(&self) -> bool {
        self.public_url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://"))
    }
    fn valid_bearer(&self, headers: &HeaderMap) -> bool {
        self.api_key
            .as_deref()
            .zip(bearer_token(headers))
            .is_some_and(|(expected, actual)| constant_time_equal(expected, actual))
    }
}

pub(crate) fn normalize_public_url(value: &str) -> Result<String> {
    let url = url::Url::parse(value)?;
    let loopback = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && loopback),
        "PUBLIC_URL requires HTTPS except on loopback"
    );
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/",
        "PUBLIC_URL must be an origin without a path, credentials, query or fragment"
    );
    Ok(url.origin().ascii_serialization())
}

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/session", get(session))
        .route("/auth/aidoku", get(aidoku_login))
        .route("/auth/logout", post(logout))
        .route("/auth/public-share/{id}/open", get(open_share))
        .route(
            "/auth/public-share/{id}",
            get(share_status).post(create_share).delete(delete_share),
        )
        .layer(axum::middleware::map_response(
            async |response: Response| no_store(response),
        ))
}

#[derive(Clone)]
pub struct BearerAuthorization {
    state: Arc<AppState>,
}
impl BearerAuthorization {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}
impl<B: Send + 'static> AsyncAuthorizeRequest<B> for BearerAuthorization {
    type RequestBody = B;
    type ResponseBody = Body;
    type Future = Pin<Box<dyn Future<Output = Result<Request<B>, Response>> + Send>>;
    fn authorize(&mut self, request: Request<B>) -> Self::Future {
        let state = self.state.clone();
        Box::pin(async move {
            if request.method() == Method::OPTIONS || request.uri().path() == "/v1/health" {
                return Ok(request);
            }
            // A browser following a link wants the sign-in page, not an error envelope
            // it will render as raw JSON. API clients still get the envelope.
            let html_navigation = wants_html(request.headers(), request.method());
            let target = request
                .uri()
                .path_and_query()
                .map(|value| value.as_str().to_owned());
            match authorize(&state, request.headers(), request.method(), request.uri()).await {
                Ok(()) => Ok(request),
                Err(error) if error.is_unauthorized() && html_navigation => {
                    Err(login_redirect(target.as_deref()))
                }
                Err(error) => Err(error.into_response()),
            }
        })
    }
}

async fn authorize(
    state: &AppState,
    headers: &HeaderMap,
    method: &Method,
    uri: &axum::http::Uri,
) -> Result<(), AppError> {
    let config = AuthConfig::read(state).await?;
    if config.valid_bearer(headers) {
        return Ok(());
    }
    // Issued tokens sit alongside the single configured key rather than replacing it,
    // so a client already holding that key keeps working.
    if let Some(token) = bearer_token(headers)
        && state.db.consume_api_token(&token_hash(token)).await?
    {
        return Ok(());
    }
    if !config.protected() {
        return Ok(());
    }
    if session_user(state, &config, headers).await?.is_some() {
        if !safe_method(method) {
            require_origin(&config, headers)?;
        }
        return Ok(());
    }
    if safe_method(method) && shared_request_allowed(state, headers, uri).await? {
        return Ok(());
    }
    Err(unauthorized())
}

async fn session_user(
    state: &AppState,
    config: &AuthConfig,
    headers: &HeaderMap,
) -> Result<Option<AuthUser>> {
    let Some(token) = cookie_value(headers, SESSION_COOKIE) else {
        return Ok(None);
    };
    let session = state.db.auth_session(&token_hash(token)).await?;
    Ok(session
        .filter(|session| session.config_hash == config.fingerprint)
        .map(|session| session.user))
}

async fn require_owner(
    state: &AppState,
    headers: &HeaderMap,
    mutate: bool,
) -> Result<AuthConfig, AppError> {
    let config = AuthConfig::read(state).await?;
    if config.valid_bearer(headers) {
        return Ok(config);
    }
    if config.protected() && session_user(state, &config, headers).await?.is_none() {
        return Err(unauthorized());
    }
    if mutate {
        require_origin(&config, headers)?;
    }
    Ok(config)
}

fn require_origin(config: &AuthConfig, headers: &HeaderMap) -> Result<(), AppError> {
    let actual = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    if actual.is_some() && actual == config.public_url.as_deref() {
        return Ok(());
    }
    Err(AppError::new(
        StatusCode::FORBIDDEN,
        "invalid_origin",
        "This request must originate from PUBLIC_URL",
        None,
    ))
}

#[derive(Deserialize, Default)]
struct LoginQuery {
    next: Option<String>,
}
#[derive(Deserialize)]
struct AidokuLoginQuery {
    name: Option<String>,
}

/// Reader clients cannot complete an OIDC redirect themselves, so they open this in a
/// web view. Sign-in happens normally, then the minted token is handed over through
/// local storage for the client to read back.
async fn aidoku_login(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AidokuLoginQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let config = AuthConfig::read(&state).await?;
    if config.protected() && session_user(&state, &config, &headers).await?.is_none() {
        return Ok(login_redirect(Some("/auth/aidoku")));
    }

    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let name = query
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Aidoku");
    let row = state.db.create_api_token(name, &token_hash(&token)).await?;
    tracing::info!(token_id = %row.id, name = %row.name, "Reader Token Issued");

    // The token is hex, so it needs no escaping to sit inside a JSON string literal.
    let page = format!(
        r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Signed in</title></head>
<body style="font-family:system-ui;margin:2rem;text-align:center">
<h1>Signed in</h1>
<p>You can close this page and return to the app.</p>
<p style="color:#666;font-size:.9rem">Token name: {name}</p>
<script>localStorage.setItem("mangaApiToken", "{token}");</script>
</body></html>"#
    );
    Ok(no_store(axum::response::Html(page).into_response()))
}

async fn login(
    State(state): State<Arc<AppState>>,
    Query(query): Query<LoginQuery>,
) -> Result<Response, AppError> {
    let config = AuthConfig::read(&state).await?;
    if !config.oidc_configured() {
        return Err(AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "auth_not_configured",
            "Configure OIDC and PUBLIC_URL to sign in",
            None,
        ));
    }
    let next = safe_next(query.next.as_deref());
    let runtime = state
        .oidc
        .get_or_try_init(|| async { oidc::OidcRuntime::new() })
        .await?;
    let login = runtime
        .begin(&config, next)
        .await
        .map_err(|_| auth_failed())?;
    let mut response = Redirect::temporary(&login.url).into_response();
    set_cookie(
        &mut response,
        LOGIN_COOKIE,
        &login.browser_token,
        600,
        config.secure_cookie(),
    );
    Ok(no_store(response))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}
async fn callback(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, AppError> {
    let config = AuthConfig::read(&state).await?;
    if !config.oidc_configured() {
        return Err(auth_failed());
    }
    let code = query.code.as_deref().ok_or_else(auth_failed)?;
    let login_state = query.state.as_deref().ok_or_else(auth_failed)?;
    let browser = cookie_value(&headers, LOGIN_COOKIE).ok_or_else(auth_failed)?;
    let runtime = state
        .oidc
        .get_or_try_init(|| async { oidc::OidcRuntime::new() })
        .await?;
    let (user, next) = runtime
        .finish(&config, login_state, browser, code)
        .await
        .map_err(|_| auth_failed())?;
    let token = CsrfToken::new_random().secret().to_owned();
    let expires = format!(
        "{:020}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            + u128::from(SESSION_SECONDS) * 1000
    );
    state
        .db
        .create_auth_session(&token_hash(&token), &config.fingerprint, &user, &expires)
        .await?;
    let mut response = Redirect::to(&next).into_response();
    set_cookie(
        &mut response,
        SESSION_COOKIE,
        &token,
        SESSION_SECONDS,
        config.secure_cookie(),
    );
    set_cookie(&mut response, LOGIN_COOKIE, "", 0, config.secure_cookie());
    Ok(no_store(response))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    auth_enabled: bool,
    oidc_configured: bool,
    user: Option<AuthUser>,
    public_access: Option<ShareResponse>,
}
async fn session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let config = AuthConfig::read(&state).await?;
    let user = session_user(&state, &config, &headers).await?;
    let share = if let Some(id) = cookie_value(&headers, SHARE_COOKIE) {
        state.db.public_share(id).await?
    } else {
        None
    };
    Ok(no_store(
        Json(SessionResponse {
            auth_enabled: config.protected(),
            oidc_configured: config.oidc_configured(),
            user,
            public_access: share.map(|share| ShareResponse::new(&share.series_id)),
        })
        .into_response(),
    ))
}
async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let config = AuthConfig::read(&state).await?;
    require_origin(&config, &headers)?;
    if let Some(token) = cookie_value(&headers, SESSION_COOKIE) {
        state.db.delete_auth_session(&token_hash(token)).await?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    for name in [SESSION_COOKIE, LOGIN_COOKIE, SHARE_COOKIE] {
        set_cookie(&mut response, name, "", 0, config.secure_cookie());
    }
    Ok(no_store(response))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareResponse {
    pathname: String,
    route_key: String,
    url: String,
}
impl ShareResponse {
    fn new(id: &str) -> Self {
        let pathname = format!("/library/{id}");
        Self {
            route_key: pathname.clone(),
            url: format!("{pathname}?public=1"),
            pathname,
        }
    }
}
async fn share_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    require_owner(&state, &headers, false).await?;
    let share = state
        .db
        .public_share_for_series(&id)
        .await?
        .map(|_| ShareResponse::new(&id));
    Ok(no_store(
        Json(serde_json::json!({ "share": share })).into_response(),
    ))
}
async fn create_share(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    require_owner(&state, &headers, true).await?;
    if state.db.get_manga_by_id(&id).await?.is_none() {
        return Err(AppError::not_found(anyhow::anyhow!("Series not found")));
    }
    state.db.create_public_share(&id).await?;
    Ok(no_store(
        Json(serde_json::json!({ "share": ShareResponse::new(&id) })).into_response(),
    ))
}
async fn delete_share(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    require_owner(&state, &headers, true).await?;
    state.db.delete_public_share(&id).await?;
    Ok(no_store(StatusCode::NO_CONTENT.into_response()))
}
#[derive(Deserialize, Default)]
struct OpenShareQuery {
    #[serde(default)]
    json: bool,
}

async fn open_share(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<OpenShareQuery>,
) -> Result<Response, AppError> {
    let config = AuthConfig::read(&state).await?;
    let share = state
        .db
        .public_share_for_series(&id)
        .await?
        .ok_or_else(|| AppError::not_found(anyhow::anyhow!("Public share not found")))?;
    let mut response = if query.json {
        Json(ShareResponse::new(&share.series_id)).into_response()
    } else {
        Redirect::to(&format!("/library/{}?public=1", share.series_id)).into_response()
    };
    set_cookie(
        &mut response,
        SHARE_COOKIE,
        &share.id,
        12 * 60 * 60,
        config.secure_cookie(),
    );
    Ok(no_store(response))
}

async fn shared_request_allowed(
    state: &AppState,
    headers: &HeaderMap,
    uri: &axum::http::Uri,
) -> Result<bool> {
    let Some(id) = cookie_value(headers, SHARE_COOKIE) else {
        return Ok(false);
    };
    let Some(share) = state.db.public_share(id).await? else {
        return Ok(false);
    };
    let path = uri.path();
    if path == format!("/v1/library/{}", share.series_id)
        || path == format!("/v1/library/{}/chapters", share.series_id)
    {
        return Ok(true);
    }
    if let Some(rest) = path.strip_prefix("/v1/library/chapters/") {
        let parts: Vec<_> = rest.split('/').collect();
        if (parts.len() == 2 || parts.len() == 3) && parts[1] == "pages" {
            return Ok(state
                .db
                .get_chapter_by_id(parts[0])
                .await?
                .is_some_and(|chapter| chapter.manga_id == share.series_id));
        }
    }
    if path == "/v1/media/image" {
        let query: std::collections::HashMap<_, _> =
            url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes())
                .into_owned()
                .collect();
        let manga = state.db.get_manga_by_id(&share.series_id).await?;
        return Ok(manga.is_some_and(|manga| {
            if query
                .get("source")
                .is_some_and(|source| source != &manga.source)
            {
                return false;
            }
            if let Some(spec) = query.get("spec") {
                manga.cover_fetch_spec.as_ref() == Some(spec)
            } else {
                query.get("url").is_some_and(|url| &manga.cover_url == url)
            }
        }));
    }
    Ok(false)
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .find_map(|cookie| {
            let (key, value) = cookie.trim().split_once('=')?;
            (key == name && !value.is_empty() && value.len() <= 256).then_some(value)
        })
}
fn wants_html(headers: &HeaderMap, method: &Method) -> bool {
    safe_method(method)
        && headers
            .get(header::ACCEPT)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("text/html"))
}

/// Sends the browser through sign-in and back to whatever it was trying to open.
fn login_redirect(next: Option<&str>) -> Response {
    let target = match next.filter(|value| value.starts_with('/')) {
        Some(next) => format!(
            "/auth/login?next={}",
            url::form_urlencoded::byte_serialize(next.as_bytes()).collect::<String>()
        ),
        None => "/auth/login".to_owned(),
    };
    no_store(Redirect::temporary(&target).into_response())
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
}
pub(crate) fn token_hash(value: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(value.as_bytes()))
}
fn constant_time_equal(left: &str, right: &str) -> bool {
    Sha256::digest(left.as_bytes())
        .ct_eq(&Sha256::digest(right.as_bytes()))
        .into()
}
fn safe_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}
fn safe_next(value: Option<&str>) -> String {
    value
        .filter(|path| {
            path.starts_with('/')
                && !path.starts_with("//")
                && !path.contains('\\')
                && !path.chars().any(char::is_control)
                && !path.starts_with("/auth/")
        })
        .unwrap_or("/")
        .to_owned()
}
fn unauthorized() -> AppError {
    AppError::new(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "Sign in or provide a valid bearer token",
        None,
    )
}
fn auth_failed() -> AppError {
    AppError::new(
        StatusCode::UNAUTHORIZED,
        "login_failed",
        "Sign-in failed or expired; start again",
        None,
    )
}
fn set_cookie(response: &mut Response, name: &str, value: &str, seconds: u64, secure: bool) {
    let secure = if secure { "; Secure" } else { "" };
    response.headers_mut().append(
        header::SET_COOKIE,
        format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={seconds}{secure}")
            .parse()
            .expect("generated cookie is valid"),
    );
}
fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response
}

#[cfg(test)]
mod tests;

/// Authentication is evaluated by the origin on every API request, including page reads.
pub(crate) async fn private_api_response(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "private, no-cache".parse().unwrap());
    for name in ["CDN-Cache-Control", "Cloudflare-CDN-Cache-Control"] {
        response.headers_mut().insert(
            axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            "no-store".parse().unwrap(),
        );
    }
    response
}
