use super::*;
use axum_test::TestServer;
use backend_persistence::{ChapterInsert, MangaInsert};
use tower_http::auth::AsyncRequireAuthorizationLayer;

fn app(state: &Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/v1/private",
            get(|| async { "ok" }).post(|| async { "ok" }),
        )
        .layer(AsyncRequireAuthorizationLayer::new(
            BearerAuthorization::new(state.clone()),
        ))
        .merge(router().with_state(state.clone()))
}
async fn state(label: &str) -> Arc<AppState> {
    crate::server::build_test_app_state(label, "auth_test").await
}
async fn session_cookie(state: &AppState, token: &str, expiry: &str) -> HeaderMap {
    let config = AuthConfig::read(state).await.unwrap();
    state
        .db
        .create_auth_session(
            &token_hash(token),
            &config.fingerprint,
            &AuthUser {
                id: "user".into(),
                name: Some("Reader".into()),
                email: None,
            },
            expiry,
        )
        .await
        .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        format!("{SESSION_COOKIE}={token}").parse().unwrap(),
    );
    headers
}

#[test]
fn redirect_and_origin_validation() {
    for next in [
        "//evil.example",
        "/\\evil.example",
        "https://evil.example",
        "/auth/callback",
        "/\r\nLocation: evil",
    ] {
        assert_eq!(safe_next(Some(next)), "/");
    }
    assert_eq!(
        safe_next(Some("/library/one?sort=new")),
        "/library/one?sort=new"
    );
    assert_eq!(
        normalize_public_url("https://manga.example/").unwrap(),
        "https://manga.example"
    );
    assert!(normalize_public_url("http://127.0.0.1:4000").is_ok());
    for url in [
        "http://manga.example",
        "https://user:password@manga.example",
        "https://manga.example/path",
        "https://manga.example/?code=secret",
    ] {
        assert!(normalize_public_url(url).is_err());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn incomplete_oidc_fails_closed_and_bearer_keys_change_live() {
    let state = state("auth-configuration").await;
    let server = TestServer::new(app(&state));
    server.get("/v1/private").await.assert_status_ok();
    state.db.set_setting("auth_enabled", "true").await.unwrap();
    server.get("/v1/private").await.assert_status_unauthorized();
    server
        .get("/auth/login")
        .await
        .assert_status(StatusCode::SERVICE_UNAVAILABLE);
    state
        .db
        .set_setting("backend_api_key", "first-key")
        .await
        .unwrap();
    server
        .get("/v1/private")
        .add_header(header::AUTHORIZATION, "Bearer first-key")
        .await
        .assert_status_ok();
    state
        .db
        .set_setting("backend_api_key", "second-key")
        .await
        .unwrap();
    server
        .get("/v1/private")
        .add_header(header::AUTHORIZATION, "Bearer first-key")
        .await
        .assert_status_unauthorized();
    server
        .post("/v1/private")
        .add_header(header::AUTHORIZATION, "Bearer second-key")
        .await
        .assert_status_ok();
    state.db.set_setting("backend_api_key", "").await.unwrap();
    server
        .get("/v1/private")
        .add_header(header::AUTHORIZATION, "Bearer second-key")
        .await
        .assert_status_unauthorized();
}

#[tokio::test(flavor = "multi_thread")]
async fn cookies_expire_require_origin_and_logout_revokes_persisted_sessions() {
    let state = state("auth-sessions").await;
    state.db.set_setting("auth_enabled", "true").await.unwrap();
    let server = TestServer::new(app(&state));
    let headers = session_cookie(&state, "valid", "99999999999999999999").await;
    let cookie = headers[header::COOKIE].to_str().unwrap();
    server
        .get("/v1/private")
        .add_header(header::COOKIE, cookie)
        .await
        .assert_status_ok();
    server
        .post("/v1/private")
        .add_header(header::COOKIE, cookie)
        .await
        .assert_status_forbidden();
    server
        .post("/v1/private")
        .add_header(header::COOKIE, cookie)
        .add_header(header::ORIGIN, "https://evil.example")
        .await
        .assert_status_forbidden();
    server
        .post("/v1/private")
        .add_header(header::COOKIE, cookie)
        .add_header(header::ORIGIN, "http://localhost")
        .await
        .assert_status_ok();
    // A new handle can still resolve the session after process state is recreated.
    let reopened = backend_persistence::Database::open(state.db.connection_url())
        .await
        .unwrap();
    assert!(
        reopened
            .auth_session(&token_hash("valid"))
            .await
            .unwrap()
            .is_some()
    );
    server
        .post("/auth/logout")
        .add_header(header::COOKIE, cookie)
        .add_header(header::ORIGIN, "http://localhost")
        .await
        .assert_status_no_content();
    server
        .get("/v1/private")
        .add_header(header::COOKIE, cookie)
        .await
        .assert_status_unauthorized();
    assert!(
        reopened
            .auth_session(&token_hash("valid"))
            .await
            .unwrap()
            .is_none()
    );
    session_cookie(&state, "expired", "00000000000000000001").await;
    server
        .get("/v1/private")
        .add_header(header::COOKIE, "manga_session=expired")
        .await
        .assert_status_unauthorized();
    session_cookie(&state, "old-config", "99999999999999999999").await;
    state
        .db
        .set_setting("auth_oidc_client_id", "new-client")
        .await
        .unwrap();
    server
        .get("/v1/private")
        .add_header(header::COOKIE, "manga_session=old-config")
        .await
        .assert_status_unauthorized();
}

async fn seed_series(state: &AppState, id: &str) -> (String, String) {
    let series = state
        .db
        .add_manga_to_library(&MangaInsert {
            source: "comix",
            source_id: id,
            title: id,
            cover_url: "https://example.com/cover.jpg",
            cover_fetch_spec: Some("saved-spec"),
            description: "",
            author: "",
            genres: "",
            status: "ongoing",
            category: "default",
            is_nsfw: false,
            language: None,
        })
        .await
        .unwrap();
    state
        .db
        .upsert_chapters(
            &series,
            vec![ChapterInsert {
                source_id: format!("{id}-chapter"),
                title: "Chapter".into(),
                chapter_number: 1.0,
                date_uploaded: "0".into(),
            }],
        )
        .await
        .unwrap();
    let chapter = state.db.get_chapters(&series).await.unwrap()[0].id.clone();
    (series, chapter)
}

#[tokio::test(flavor = "multi_thread")]
async fn public_shares_validate_chapter_ownership_and_media_spec_and_can_be_revoked() {
    let state = state("auth-sharing").await;
    state.db.set_setting("auth_enabled", "true").await.unwrap();
    let (series, chapter) = seed_series(&state, "shared").await;
    let (other, other_chapter) = seed_series(&state, "private").await;
    let share = state.db.create_public_share(&series).await.unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        format!("{SHARE_COOKIE}={}", share.id).parse().unwrap(),
    );
    for path in [
        format!("/v1/library/{series}"),
        format!("/v1/library/{series}/chapters"),
        format!("/v1/library/chapters/{chapter}/pages"),
        format!("/v1/library/chapters/{chapter}/pages/0"),
    ] {
        assert!(
            authorize(&state, &headers, &Method::GET, &path.parse().unwrap())
                .await
                .is_ok()
        );
        assert!(
            authorize(&state, &headers, &Method::POST, &path.parse().unwrap())
                .await
                .is_err()
        );
    }
    for path in [
        format!("/v1/library/{other}"),
        format!("/v1/library/chapters/{other_chapter}/pages/0?libraryId={series}"),
        "/v1/library".into(),
        "/v1/settings".into(),
        "/v1/media/image?url=https://example.com/cover.jpg&spec=attacker".into(),
    ] {
        assert!(
            authorize(&state, &headers, &Method::GET, &path.parse().unwrap())
                .await
                .is_err(),
            "{path}"
        );
    }
    assert!(
        authorize(
            &state,
            &headers,
            &Method::GET,
            &"/v1/media/image?spec=saved-spec&source=comix"
                .parse()
                .unwrap()
        )
        .await
        .is_ok()
    );
    state.db.delete_public_share(&series).await.unwrap();
    assert!(
        authorize(
            &state,
            &headers,
            &Method::GET,
            &format!("/v1/library/{series}").parse().unwrap()
        )
        .await
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_mask_oidc_secret_and_keep_the_readable_api_key_server_owned() {
    let state = state("auth-settings").await;
    state
        .db
        .set_setting("auth_oidc_client_secret", "private-value")
        .await
        .unwrap();
    state
        .db
        .set_setting("backend_api_key", "private-key")
        .await
        .unwrap();
    let mut settings = crate::app::settings::get(&state).await.unwrap();
    assert_eq!(settings.settings["auth_oidc_client_secret"], "********");
    // Reader clients copy the bearer key out of the settings UI, so it is returned whole.
    assert_eq!(settings.settings["backend_api_key"], "private-key");

    settings
        .settings
        .insert("backend_api_key".into(), "client-chosen".into());
    crate::app::settings::update(&state, &settings.settings)
        .await
        .unwrap();
    assert_eq!(
        state
            .db
            .get_setting("auth_oidc_client_secret")
            .await
            .unwrap()
            .as_deref(),
        Some("private-value")
    );
    assert_eq!(
        state.db.get_setting("backend_api_key").await.unwrap().as_deref(),
        Some("private-key")
    );
}

// An actual HTTP issuer fixture exercises discovery, JWKS, code exchange and signed tokens.
// These disposable signing keys are test data, never production credentials.
mod issuer {
    use super::*;
    use axum::Form;
    use openidconnect::{
        PrivateSigningKey,
        core::{CoreEdDsaPrivateSigningKey, CoreJwsSigningAlgorithm},
    };
    use std::{
        collections::HashMap,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::Mutex;

    struct Code {
        challenge: String,
        nonce: String,
    }
    struct Provider {
        url: String,
        key_version: AtomicUsize,
        jwks_reads: AtomicUsize,
        mode: Mutex<String>,
        codes: Mutex<HashMap<String, Code>>,
    }
    fn signing_key(version: usize) -> CoreEdDsaPrivateSigningKey {
        let pem = if version == 0 {
            include_str!("../../../tests/fixtures/oidc-test-key.pem")
        } else {
            include_str!("../../../tests/fixtures/oidc-rotated-test-key.pem")
        };
        CoreEdDsaPrivateSigningKey::from_ed25519_pem(
            pem,
            Some(openidconnect::JsonWebKeyId::new(format!("test-{version}"))),
        )
        .unwrap()
    }
    async fn discovery(State(provider): State<Arc<Provider>>) -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "issuer": provider.url,
            "authorization_endpoint": format!("{}/authorize", provider.url),
            "token_endpoint": format!("{}/token", provider.url),
            "jwks_uri": format!("{}/jwks", provider.url),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["EdDSA"],
            "code_challenge_methods_supported": ["S256"]
        }))
    }
    async fn jwks(State(provider): State<Arc<Provider>>) -> Json<serde_json::Value> {
        provider.jwks_reads.fetch_add(1, Ordering::SeqCst);
        Json(
            serde_json::json!({ "keys": [signing_key(provider.key_version.load(Ordering::SeqCst)).as_verification_key()] }),
        )
    }
    async fn authorize(
        State(provider): State<Arc<Provider>>,
        Query(query): Query<HashMap<String, String>>,
    ) -> Redirect {
        assert_eq!(query["client_id"], "manga-test");
        assert_eq!(query["response_type"], "code");
        assert_eq!(query["redirect_uri"], "http://localhost/auth/callback");
        assert_eq!(query["code_challenge_method"], "S256");
        let code = uuid::Uuid::now_v7().to_string();
        provider.codes.lock().await.insert(
            code.clone(),
            Code {
                challenge: query["code_challenge"].clone(),
                nonce: query["nonce"].clone(),
            },
        );
        let mut url = url::Url::parse(&query["redirect_uri"]).unwrap();
        url.query_pairs_mut()
            .append_pair("code", &code)
            .append_pair("state", &query["state"]);
        Redirect::temporary(url.as_str())
    }
    async fn token(
        State(provider): State<Arc<Provider>>,
        Form(form): Form<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        assert_eq!(form["grant_type"], "authorization_code");
        assert_eq!(form["client_id"], "manga-test");
        let code = provider.codes.lock().await.remove(&form["code"]).unwrap();
        assert_eq!(
            token_hash(&form["code_verifier"]),
            code.challenge,
            "PKCE binds the code to this browser's login attempt"
        );
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mode = provider.mode.lock().await.clone();
        let version = provider.key_version.load(Ordering::SeqCst);
        let mut claims = serde_json::json!({ "iss": provider.url, "aud": "manga-test", "sub": "verified-user", "name": "Reader", "email": "reader@example.test", "iat": now, "exp": now + 300, "nonce": code.nonce });
        match mode.as_str() {
            "issuer" => claims["iss"] = "https://wrong.example".into(),
            "audience" => claims["aud"] = "another-client".into(),
            "expiry" => claims["exp"] = (now - 60).into(),
            "nonce" => claims["nonce"] = "wrong-nonce".into(),
            "access_hash" => claims["at_hash"] = "incorrect".into(),
            _ => (),
        }
        let encode = |value: &serde_json::Value| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(value).unwrap())
        };
        let header = serde_json::json!({ "alg": "EdDSA", "kid": format!("test-{version}") });
        let message = format!("{}.{}", encode(&header), encode(&claims));
        let key = signing_key(if mode == "signature" {
            1 - version
        } else {
            version
        });
        let signature = key
            .sign(&CoreJwsSigningAlgorithm::EdDsa, message.as_bytes())
            .unwrap();
        let jwt = format!(
            "{message}.{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature)
        );
        Json(
            serde_json::json!({ "access_token": "test-access", "token_type": "Bearer", "id_token": jwt }),
        )
    }
    async fn attempt(server: &TestServer, browser: &reqwest::Client) -> (String, String) {
        let login = server.get("/auth/login?next=%2Flibrary").await;
        login.assert_status(StatusCode::TEMPORARY_REDIRECT);
        let cookie = login.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        assert!(
            login.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .contains("HttpOnly")
        );
        assert!(
            login.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .contains("SameSite=Lax")
        );
        let authorize_url = login.headers()[header::LOCATION].to_str().unwrap();
        let authorization = browser.get(authorize_url).send().await.unwrap();
        let callback =
            url::Url::parse(authorization.headers()[header::LOCATION].to_str().unwrap()).unwrap();
        (
            format!("{}?{}", callback.path(), callback.query().unwrap()),
            cookie,
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn oidc_exchange_verifies_claims_browser_binding_replay_and_key_rotation() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let provider = Arc::new(Provider {
            url: url.clone(),
            key_version: AtomicUsize::new(0),
            jwks_reads: AtomicUsize::new(0),
            mode: Mutex::new(String::new()),
            codes: Mutex::new(HashMap::new()),
        });
        let fixture = Router::new()
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/jwks", get(jwks))
            .route("/authorize", get(authorize))
            .route("/token", post(token))
            .with_state(provider.clone());
        let task = tokio::spawn(async move { axum::serve(listener, fixture).await.unwrap() });
        let state = state("auth-oidc-exchange").await;
        for (key, value) in [
            ("auth_enabled", "true"),
            ("auth_oidc_issuer_url", url.as_str()),
            ("auth_oidc_client_id", "manga-test"),
        ] {
            state.db.set_setting(key, value).await.unwrap();
        }
        let server = TestServer::new(app(&state));
        let browser = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let (callback, cookie) = attempt(&server, &browser).await;
        let response = server
            .get(&callback)
            .add_header(header::COOKIE, cookie.as_str())
            .await;
        response
            .assert_status(StatusCode::SEE_OTHER)
            .assert_header(header::LOCATION, "/library");
        let session = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .find_map(|value| {
                value
                    .to_str()
                    .ok()
                    .filter(|value| value.starts_with("manga_session="))
            })
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        server
            .get("/v1/private")
            .add_header(header::COOKIE, session)
            .await
            .assert_status_ok();
        let session_info = server
            .get("/auth/session")
            .add_header(header::COOKIE, session)
            .await;
        assert_eq!(
            session_info.json::<serde_json::Value>()["user"]["id"],
            "verified-user"
        );
        server
            .get(&callback)
            .add_header(header::COOKIE, cookie.as_str())
            .await
            .assert_status_unauthorized();
        assert_eq!(provider.jwks_reads.load(Ordering::SeqCst), 1);
        provider.key_version.store(1, Ordering::SeqCst);
        let (callback, cookie) = attempt(&server, &browser).await;
        server
            .get(&callback)
            .add_header(header::COOKIE, cookie.as_str())
            .await
            .assert_status(StatusCode::SEE_OTHER);
        assert_eq!(
            provider.jwks_reads.load(Ordering::SeqCst),
            2,
            "rotated signing key refreshes JWKS"
        );
        for invalid in [
            "issuer",
            "audience",
            "expiry",
            "nonce",
            "access_hash",
            "signature",
        ] {
            *provider.mode.lock().await = invalid.into();
            let (callback, cookie) = attempt(&server, &browser).await;
            server
                .get(&callback)
                .add_header(header::COOKIE, cookie.as_str())
                .await
                .assert_status_unauthorized();
        }
        *provider.mode.lock().await = String::new();
        let (callback, _) = attempt(&server, &browser).await;
        server
            .get(&callback)
            .add_header(header::COOKIE, "manga_login=different-browser")
            .await
            .assert_status_unauthorized();
        task.abort();
    }
}
