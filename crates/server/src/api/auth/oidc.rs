//! OIDC protocol state: discovery/JWKS cache and single-use, browser-bound PKCE attempts.
use super::{AuthConfig, token_hash};
use anyhow::{Context, Result, ensure};
use backend_persistence::AuthUser;
use openidconnect::{
    AccessTokenHash, AuthorizationCode, ClientId, CsrfToken, IssuerUrl, Nonce, OAuth2TokenResponse,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const DISCOVERY_TTL: Duration = Duration::from_secs(300);
const LOGIN_TTL: Duration = Duration::from_secs(600);
const MAX_PENDING_LOGINS: usize = 1024;

pub(crate) struct OidcRuntime {
    http: reqwest::Client,
    provider: Mutex<Option<CachedProvider>>,
    pending: Mutex<HashMap<String, PendingLogin>>,
}
struct CachedProvider {
    issuer: String,
    expires: Instant,
    metadata: CoreProviderMetadata,
}
struct PendingLogin {
    expires: Instant,
    browser_hash: String,
    config_hash: String,
    verifier: PkceCodeVerifier,
    nonce: Nonce,
    next: String,
}
pub(super) struct LoginRedirect {
    pub url: String,
    pub browser_token: String,
}

impl OidcRuntime {
    pub(crate) fn new() -> Result<Self> {
        backend_tls::ensure_graviola_rustls_provider()?;
        Ok(Self {
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(15))
                .build()?,
            provider: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
        })
    }

    async fn request(
        &self,
        request: openidconnect::HttpRequest,
    ) -> Result<openidconnect::HttpResponse, std::io::Error> {
        let (parts, body) = request.into_parts();
        validate_endpoint(&parts.uri.to_string()).map_err(std::io::Error::other)?;
        let mut response = self
            .http
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .body(body)
            .send()
            .await
            .map_err(std::io::Error::other)?;
        let status = response.status();
        let headers = response.headers().clone();
        if response
            .content_length()
            .is_some_and(|length| length > 1_048_576)
        {
            return Err(std::io::Error::other("OIDC response exceeds size limit"));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(std::io::Error::other)? {
            if body.len() + chunk.len() > 1_048_576 {
                return Err(std::io::Error::other("OIDC response exceeds size limit"));
            }
            body.extend_from_slice(&chunk);
        }
        let mut response = openidconnect::HttpResponse::new(body.to_vec());
        *response.status_mut() = status;
        *response.headers_mut() = headers;
        Ok(response)
    }

    async fn metadata(&self, issuer: &str, refresh: bool) -> Result<CoreProviderMetadata> {
        let mut cache = self.provider.lock().await;
        if !refresh
            && let Some(cached) = cache.as_ref()
            && cached.issuer == issuer
            && cached.expires > Instant::now()
        {
            return Ok(cached.metadata.clone());
        }
        validate_endpoint(issuer)?;
        let metadata =
            CoreProviderMetadata::discover_async(IssuerUrl::new(issuer.into())?, self).await?;
        validate_endpoint(metadata.authorization_endpoint().as_str())?;
        if let Some(endpoint) = metadata.token_endpoint() {
            validate_endpoint(endpoint.as_str())?;
        }
        validate_endpoint(metadata.jwks_uri().as_str())?;
        *cache = Some(CachedProvider {
            issuer: issuer.to_owned(),
            expires: Instant::now() + DISCOVERY_TTL,
            metadata: metadata.clone(),
        });
        Ok(metadata)
    }

    pub(super) async fn begin(&self, config: &AuthConfig, next: String) -> Result<LoginRedirect> {
        ensure!(config.oidc_configured(), "OIDC is not configured");
        let metadata = self.metadata(&config.issuer, false).await?;
        let client = CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(config.client_id.clone()),
            config.client_secret.clone(),
        )
        .set_redirect_uri(RedirectUrl::new(config.callback_url()?)?);
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut authorization = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .set_pkce_challenge(challenge);
        for scope in config
            .scopes
            .split_whitespace()
            .filter(|scope| *scope != "openid")
        {
            authorization = authorization.add_scope(Scope::new(scope.to_owned()));
        }
        let (url, state, nonce) = authorization.url();
        let browser_token = CsrfToken::new_random().secret().to_owned();
        let mut pending = self.pending.lock().await;
        pending.retain(|_, login| login.expires > Instant::now());
        ensure!(
            pending.len() < MAX_PENDING_LOGINS,
            "too many pending sign-ins"
        );
        pending.insert(
            token_hash(state.secret()),
            PendingLogin {
                expires: Instant::now() + LOGIN_TTL,
                browser_hash: token_hash(&browser_token),
                config_hash: config.fingerprint.clone(),
                verifier,
                nonce,
                next,
            },
        );
        Ok(LoginRedirect {
            url: url.to_string(),
            browser_token,
        })
    }

    pub(super) async fn finish(
        &self,
        config: &AuthConfig,
        state: &str,
        browser_token: &str,
        code: &str,
    ) -> Result<(AuthUser, String)> {
        let login = self
            .pending
            .lock()
            .await
            .remove(&token_hash(state))
            .context("unknown or reused login state")?;
        ensure!(login.expires > Instant::now(), "login expired");
        ensure!(
            login.browser_hash == token_hash(browser_token),
            "login belongs to a different browser"
        );
        ensure!(
            login.config_hash == config.fingerprint,
            "authentication configuration changed"
        );
        let metadata = self.metadata(&config.issuer, false).await?;
        let client = CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(config.client_id.clone()),
            config.client_secret.clone(),
        )
        .set_redirect_uri(RedirectUrl::new(config.callback_url()?)?);
        let token = client
            .exchange_code(AuthorizationCode::new(code.to_owned()))?
            .set_pkce_verifier(login.verifier)
            .request_async(self)
            .await?;
        let id_token = token
            .id_token()
            .context("issuer did not return an ID token")?;
        // Retry verification once with refreshed discovery/JWKS to handle key rotation.
        // The authorization code is exchanged exactly once.
        let refreshed;
        let original_verifier = client.id_token_verifier();
        let verifier = if id_token.claims(&original_verifier, &login.nonce).is_ok() {
            original_verifier
        } else {
            refreshed = CoreClient::from_provider_metadata(
                self.metadata(&config.issuer, true).await?,
                ClientId::new(config.client_id.clone()),
                config.client_secret.clone(),
            );
            refreshed.id_token_verifier()
        };
        let claims = id_token.claims(&verifier, &login.nonce)?;
        if let Some(expected) = claims.access_token_hash() {
            let actual = AccessTokenHash::from_token(
                token.access_token(),
                id_token.signing_alg()?,
                id_token.signing_key(&verifier)?,
            )?;
            ensure!(&actual == expected, "access token hash did not match");
        }
        let user = AuthUser {
            id: claims.subject().as_str().to_owned(),
            name: claims
                .name()
                .and_then(|name| name.get(None))
                .map(|name| name.as_str().to_owned()),
            email: claims.email().map(|email| email.as_str().to_owned()),
        };
        Ok((user, login.next))
    }
}

fn validate_endpoint(value: &str) -> Result<()> {
    let url = url::Url::parse(value)?;
    let loopback = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && loopback),
        "OIDC endpoints require HTTPS except on loopback"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none() && url.fragment().is_none(),
        "invalid OIDC endpoint URL"
    );
    Ok(())
}

impl<'a> openidconnect::AsyncHttpClient<'a> for OidcRuntime {
    type Error = std::io::Error;
    type Future = std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<openidconnect::HttpResponse, Self::Error>>
                + Send
                + 'a,
        >,
    >;
    fn call(&'a self, request: openidconnect::HttpRequest) -> Self::Future {
        Box::pin(self.request(request))
    }
}
