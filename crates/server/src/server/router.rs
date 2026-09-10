use super::state::AppState;
use crate::{api, app::settings, downloader};
use autometrics::autometrics;
use axum::{Router, error_handling::HandleErrorLayer, extract::MatchedPath, http::Request};
use axum_client_ip::ClientIpSource;
use axum_otel_metrics::HttpMetricsLayerBuilder;
use backend_persistence::DownloadWorkStatus;
use backend_runtime::truncate_for_log;
use http::{
    HeaderMap, HeaderName, StatusCode,
    header::{
        AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HOST, PROXY_AUTHORIZATION, REFERER,
        SET_COOKIE, USER_AGENT,
    },
};
use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::{
    auth::AsyncRequireAuthorizationLayer,
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    sensitive_headers::{SetSensitiveRequestHeadersLayer, SetSensitiveResponseHeadersLayer},
    trace::{DefaultOnFailure, DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tower_resilience_ratelimiter::{RateLimiterLayer, RateLimiterServiceError, WindowType};
use tracing::Level;

const HTTP_LOG_VALUE_MAX_CHARS: usize = 160;
const PUBLIC_API_RATE_LIMIT_PER_MINUTE: usize = 500_000_000;
const PUBLIC_API_RATE_LIMIT_WAIT: Duration = Duration::from_millis(5);
const METRICS_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

pub(super) fn build_router(state: &Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let sensitive_headers: Arc<[_]> =
        Arc::new([AUTHORIZATION, PROXY_AUTHORIZATION, COOKIE, SET_COOKIE]);

    let (backend_api, openapi) = api::routes::router()
        .with_state(Arc::clone(state))
        .layer(AsyncRequireAuthorizationLayer::new(
            api::auth::BearerAuthorization::new(Arc::clone(state)),
        ))
        .split_for_parts();
    let backend_api = backend_api.layer(
        ServiceBuilder::new()
            .layer(HandleErrorLayer::new(
                handle_public_api_rate_limit_error
                    as fn(RateLimiterServiceError<Infallible>) -> PublicApiRateLimitErrorFuture,
            ))
            .layer(
                RateLimiterLayer::builder()
                    .name("public-api")
                    .limit_for_period(PUBLIC_API_RATE_LIMIT_PER_MINUTE)
                    .refresh_period(Duration::from_mins(1))
                    .timeout_duration(PUBLIC_API_RATE_LIMIT_WAIT)
                    .window_type(WindowType::SlidingCounter)
                    .build(),
            ),
    );
    backend_api
        .merge(api::openapi::docs_router(openapi))
        .route_layer(HttpMetricsLayerBuilder::new().build())
        .layer(ClientIpSource::CfConnectingIp.into_extension())
        .layer(cors)
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetSensitiveResponseHeadersLayer::from_shared(Arc::clone(
            &sensitive_headers,
        )))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(http_trace_span)
                .on_request(DefaultOnRequest::new().level(Level::TRACE))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::TRACE)
                        .include_headers(true),
                )
                .on_failure(DefaultOnFailure::new().level(Level::WARN)),
        )
        .layer(SetSensitiveRequestHeadersLayer::from_shared(Arc::clone(
            &sensitive_headers,
        )))
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}

type PublicApiRateLimitErrorFuture = std::future::Ready<(StatusCode, &'static str)>;

#[allow(clippy::needless_pass_by_value)]
fn handle_public_api_rate_limit_error(
    error: RateLimiterServiceError<Infallible>,
) -> PublicApiRateLimitErrorFuture {
    match error {
        RateLimiterServiceError::RateLimited => std::future::ready((
            StatusCode::TOO_MANY_REQUESTS,
            "public api rate limit exceeded",
        )),
        RateLimiterServiceError::Inner(inner) => match inner {},
    }
}

pub(crate) fn spawn_metrics_refresh_loop(state: Arc<AppState>) {
    tokio::spawn(async move {
        refresh_scrape_metrics(&state).await;
        let mut interval = tokio::time::interval(METRICS_REFRESH_INTERVAL);
        loop {
            interval.tick().await;
            refresh_scrape_metrics(&state).await;
        }
    });
}

#[autometrics(track_concurrency)]
async fn refresh_scrape_metrics(state: &Arc<AppState>) {
    refresh_download_status_metrics(state).await;
    refresh_library_metrics(state).await;
    refresh_source_metrics(state).await;
    refresh_download_storage_metrics(state).await;
}

#[autometrics(track_concurrency)]
async fn refresh_download_status_metrics(state: &Arc<AppState>) {
    let downloads = match state.db.get_download_metric_rows().await {
        Ok(downloads) => downloads,
        Err(error) => {
            tracing::warn!(error = %error, "Download status metrics refresh failed");
            return;
        }
    };

    let source_names = {
        let plugins = state.source_registry.read().await;
        plugins
            .sources()
            .into_iter()
            .map(|source| source.name)
            .collect::<BTreeSet<_>>()
    };
    let mut counts = BTreeMap::<String, u64>::new();
    let mut source_status_counts = BTreeMap::<(String, String), u64>::new();
    for status in DownloadWorkStatus::EXTERNAL_LABELS {
        counts.insert(status.to_string(), 0);
        for source in &source_names {
            source_status_counts.insert((source.clone(), status.to_string()), 0);
        }
    }
    for download in downloads {
        let status = download.status;
        *counts.entry(status.clone()).or_default() += 1;
        *source_status_counts
            .entry((download.source, status))
            .or_default() += 1;
    }

    for (status, count) in counts {
        state
            .telemetry
            .metrics
            .set_downloads_by_status(&status, count);
    }
    for ((source, status), count) in source_status_counts {
        state
            .telemetry
            .metrics
            .set_downloads_by_source_status(&source, &status, count);
    }
}

#[autometrics(track_concurrency)]
async fn refresh_library_metrics(state: &Arc<AppState>) {
    let source_counts = match state.db.get_library_source_counts().await {
        Ok(source_counts) => source_counts,
        Err(error) => {
            tracing::warn!(error = %error, "Library metrics refresh failed");
            return;
        }
    };

    let source_names = {
        let plugins = state.source_registry.read().await;
        plugins
            .sources()
            .into_iter()
            .map(|source| source.name)
            .collect::<Vec<_>>()
    };
    let mut counts = BTreeMap::<String, u64>::new();
    for source in source_names {
        counts.insert(source, 0);
    }
    for source_count in source_counts {
        counts.insert(source_count.source, source_count.count);
    }

    for (source, count) in counts {
        state.telemetry.metrics.set_library_manga(&source, count);
    }
}

#[autometrics]
async fn refresh_source_metrics(state: &Arc<AppState>) {
    let plugins = state.source_registry.read().await;
    let sources = plugins.sources();
    let enabled = sources.iter().filter(|source| source.enabled).count();
    let loaded = sources.len();
    state
        .telemetry
        .metrics
        .set_sources("loaded", u64::try_from(loaded).unwrap_or(u64::MAX));
    state
        .telemetry
        .metrics
        .set_sources("enabled", u64::try_from(enabled).unwrap_or(u64::MAX));
    state.telemetry.metrics.set_sources(
        "disabled",
        u64::try_from(loaded.saturating_sub(enabled)).unwrap_or(u64::MAX),
    );
}

#[autometrics(track_concurrency)]
async fn refresh_download_storage_metrics(state: &Arc<AppState>) {
    let settings = settings::interface(&state.db);
    let download_path = match settings.download_path().await {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(error = %error, "Download storage path metrics refresh failed");
            return;
        }
    };

    if let Some(bytes) =
        downloader::cached_download_storage_usage_bytes(state, &download_path).await
    {
        state
            .telemetry
            .metrics
            .set_download_storage_bytes("used", bytes);
    }
    downloader::refresh_download_storage_usage_if_stale(Arc::clone(state), download_path.clone())
        .await;

    match settings.max_download_storage_bytes().await {
        Ok(Some(bytes)) => state
            .telemetry
            .metrics
            .set_download_storage_bytes("limit", bytes),
        Ok(None) => state
            .telemetry
            .metrics
            .set_download_storage_bytes("limit", 0),
        Err(error) => {
            tracing::warn!(error = %error, "Download storage limit metrics refresh failed");
        }
    }
}

fn http_trace_span<B>(request: &Request<B>) -> tracing::Span {
    if !tracing::enabled!(Level::INFO) {
        return tracing::info_span!("http_request");
    }

    let headers = request.headers();
    let uri = request.uri();
    let matched_path = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("", MatchedPath::as_str);
    let user_agent = truncated_header(headers, &USER_AGENT);
    let referer = truncated_header(headers, &REFERER);
    let client_ip = cloudflare_client_ip(headers).map_or_else(String::new, |ip| ip.to_string());

    tracing::info_span!(
        "http_request",
        request_id = %header_value(headers, &HeaderName::from_static("x-request-id")),
        method = %request.method(),
        path = %uri.path(),
        matched_path = %matched_path,
        query = %uri.query().unwrap_or(""),
        version = ?request.version(),
        host = %header_value(headers, &HOST),
        user_agent = %user_agent,
        referer = %referer,
        client_ip = %client_ip,
        content_type = %header_value(headers, &CONTENT_TYPE),
        content_length = %header_value(headers, &CONTENT_LENGTH),
        has_authorization = headers.contains_key(AUTHORIZATION),
    )
}

fn cloudflare_client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(HeaderName::from_static("cf-connecting-ip"))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

fn header_value<'a>(headers: &'a HeaderMap, name: &HeaderName) -> &'a str {
    headers
        .get(name.as_str())
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
}

fn truncated_header(headers: &HeaderMap, name: &HeaderName) -> String {
    headers
        .get(name.as_str())
        .and_then(|value| value.to_str().ok())
        .map(|value| truncate_for_log(value, HTTP_LOG_VALUE_MAX_CHARS))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::build_test_app_state;
    use axum::routing::get;
    use axum_test::TestServer;
    use backend_persistence::{ChapterInsert, MangaInsert};
    use std::fs;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn cloudflare_client_ip_parses_valid_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("cf-connecting-ip"),
            "203.0.113.10".parse().expect("header should parse"),
        );

        assert_eq!(
            cloudflare_client_ip(&headers),
            Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)))
        );
    }

    #[test]
    fn cloudflare_client_ip_ignores_missing_or_invalid_header() {
        let headers = HeaderMap::new();
        assert_eq!(cloudflare_client_ip(&headers), None);

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("cf-connecting-ip"),
            "not-an-ip".parse().expect("header should parse"),
        );
        assert_eq!(cloudflare_client_ip(&headers), None);
    }

    #[test]
    fn cloudflare_client_ip_parses_ipv6_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("cf-connecting-ip"),
            "2001:db8::1".parse().expect("header should parse"),
        );

        assert_eq!(
            cloudflare_client_ip(&headers),
            Some(IpAddr::V6(Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 1)))
        );
    }

    #[tokio::test]
    async fn client_ip_extractor_uses_cloudflare_header() {
        async fn echo_client_ip(axum_client_ip::ClientIp(ip): axum_client_ip::ClientIp) -> String {
            ip.to_string()
        }

        let app = Router::new()
            .route("/ip", get(echo_client_ip))
            .layer(ClientIpSource::CfConnectingIp.into_extension());
        let server = TestServer::new(app);

        let response = server
            .get("/ip")
            .add_header("cf-connecting-ip", "203.0.113.42")
            .await;

        response.assert_status_ok();
        assert_eq!(response.as_bytes().as_ref(), b"203.0.113.42");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bbf_page_routes_preserve_originals_select_variants_and_cache_only_conversions() {
        let state = test_state("bbf-page-routes").await;
        let pixels = image::RgbImage::from_pixel(1600, 24, image::Rgb([77, 88, 99]));
        let mut original = std::io::Cursor::new(Vec::new());
        pixels
            .write_to(&mut original, image::ImageFormat::Png)
            .unwrap();
        let original = original.into_inner();
        let chapter_id = seed_completed_archive(&state, &[("001.png", &original)]).await;
        let server = TestServer::new(api_router(&state));
        let url = format!("/v1/library/chapters/{chapter_id}/pages/0");
        let response = server.get(&url).await;
        response
            .assert_status_ok()
            .assert_header(CONTENT_TYPE, "image/png")
            .assert_header("X-Page-Variant", "original");
        assert_eq!(response.as_bytes().as_ref(), original);
        drop(response);
        server
            .get(&format!("{url}?variant=upscaled"))
            .await
            .assert_status_not_found();
        server
            .get(&format!("{url}?width=0"))
            .await
            .assert_status_bad_request();
        server
            .get(&format!("{url}?format=invalid"))
            .await
            .assert_status_bad_request();
        server
            .get(&format!("/v1/library/chapters/{chapter_id}/pages/1"))
            .await
            .assert_status_not_found();
        for format in ["avif", "webp", "jpeg"] {
            let converted_url = format!("{url}?format={format}");
            let first = server.get(&converted_url).await;
            first.assert_status_ok().assert_header("X-Cache", "MISS");
            assert_eq!(
                backend_image::decode_image(first.as_bytes())
                    .unwrap()
                    .width(),
                1600
            );
            let second = server.get(&converted_url).await;
            second.assert_status_ok().assert_header("X-Cache", "HIT");
            assert_eq!(first.as_bytes(), second.as_bytes());
        }
        let resized = server.get(&format!("{url}?width=320")).await;
        resized
            .assert_status_ok()
            .assert_header(CONTENT_TYPE, "image/avif");
        assert_eq!(
            backend_image::decode_image(resized.as_bytes())
                .unwrap()
                .width(),
            320
        );
        let archive =
            crate::app::downloaded_archive_resolution::existing_completed_archive_for_chapter(
                &state.db,
                &chapter_id,
            )
            .await
            .unwrap();
        let before = backend_storage::inspect(archive.archive_path.clone())
            .await
            .unwrap();
        let upscaled_pixels = image::RgbImage::from_pixel(3200, 48, image::Rgb([12, 34, 56]));
        let upscaled = backend_image::encode_lossless_avif_rgb(&upscaled_pixels).unwrap();
        let staged = archive.archive_path.with_extension("avif");
        tokio::fs::write(&staged, &upscaled).await.unwrap();
        backend_storage::append_upscaled(
            archive.archive_path,
            backend_storage::UpscaledChapter {
                pages: vec![staged],
                model: "route-fixture".into(),
                scale: 2,
                tile_size: 256,
                expected_footer_hash: before.footer_hash,
            },
            || false,
        )
        .await
        .unwrap();
        let response = server.get(&url).await;
        response
            .assert_status_ok()
            .assert_header("X-Page-Variant", "upscaled")
            .assert_header(CONTENT_TYPE, "image/avif");
        assert_eq!(response.as_bytes().as_ref(), upscaled);
        let response = server.get(&format!("{url}?variant=original")).await;
        response
            .assert_status_ok()
            .assert_header("X-Page-Variant", "original");
        assert_eq!(response.as_bytes().as_ref(), original);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn downloads_bulk_route_enqueues_and_filters_series_downloads() {
        let state = test_state("downloads-bulk-route").await;
        let (manga_id, first_chapter_id, second_chapter_id) = seed_library_chapters(&state).await;
        let server = TestServer::new(api_router(&state));

        let response = server
            .post("/v1/downloads/bulk")
            .json(&serde_json::json!({
                "manga_id": manga_id,
                "chapter_ids": [second_chapter_id, first_chapter_id],
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value =
            serde_json::from_slice(response.as_bytes()).expect("bulk response should be json");
        assert_eq!(body["enqueued"], 2);

        let response = server
            .get(&format!("/v1/downloads?manga_id={manga_id}"))
            .await;
        response.assert_status_ok();
        let body: serde_json::Value =
            serde_json::from_slice(response.as_bytes()).expect("download list should be json");
        let items = body["items"]
            .as_array()
            .expect("download list should contain items");
        let chapter_ids = items
            .iter()
            .map(|item| {
                item["chapter_id"]
                    .as_str()
                    .expect("download row should include chapter_id")
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            chapter_ids,
            BTreeSet::from([first_chapter_id.as_str(), second_chapter_id.as_str()])
        );
    }

    fn api_router(state: &Arc<AppState>) -> axum::Router {
        crate::api::routes::router()
            .with_state(Arc::clone(state))
            .split_for_parts()
            .0
    }

    async fn test_state(label: &str) -> Arc<AppState> {
        build_test_app_state(label, "manga_server_router_test").await
    }

    async fn seed_completed_archive(state: &Arc<AppState>, entries: &[(&str, &[u8])]) -> String {
        seed_completed_series_archives(state, entries, &[]).await.0
    }

    async fn seed_completed_series_archives(
        state: &Arc<AppState>,
        first_entries: &[(&str, &[u8])],
        second_entries: &[(&str, &[u8])],
    ) -> (String, String) {
        let (manga_id, first_chapter_id, second_chapter_id) = seed_library_chapters(state).await;
        complete_archive_for_chapter(state, &manga_id, &first_chapter_id, first_entries).await;
        if !second_entries.is_empty() {
            complete_archive_for_chapter(state, &manga_id, &second_chapter_id, second_entries)
                .await;
        }

        (first_chapter_id, second_chapter_id)
    }

    async fn seed_library_chapters(state: &Arc<AppState>) -> (String, String, String) {
        let manga_id = state
            .db
            .add_manga_to_library(&MangaInsert {
                source: "test-source",
                source_id: "series-1",
                title: "Test Manga",
                cover_url: "",
                cover_fetch_spec: None,
                description: "",
                author: "",
                genres: "",
                status: "ongoing",
                category: "",
                is_nsfw: false,
                language: None,
            })
            .await
            .expect("manga should be inserted");
        let chapters = state
            .db
            .upsert_chapters(
                &manga_id,
                vec![
                    ChapterInsert {
                        source_id: "chapter-1".to_string(),
                        title: "Chapter 1".to_string(),
                        chapter_number: 1.0,
                        date_uploaded: "2026-05-25".to_string(),
                    },
                    ChapterInsert {
                        source_id: "chapter-2".to_string(),
                        title: "Chapter 2".to_string(),
                        chapter_number: 2.0,
                        date_uploaded: "2026-05-25".to_string(),
                    },
                ],
            )
            .await
            .expect("chapter should be inserted")
            .into_iter()
            .collect::<Vec<_>>();
        let first_chapter_id = chapters
            .first()
            .expect("first chapter id should be returned")
            .clone();
        let second_chapter_id = chapters
            .get(1)
            .expect("second chapter id should be returned")
            .clone();

        (manga_id, first_chapter_id, second_chapter_id)
    }

    async fn complete_archive_for_chapter(
        state: &Arc<AppState>,
        manga_id: &str,
        chapter_id: &str,
        entries: &[(&str, &[u8])],
    ) {
        let download_id = state
            .db
            .enqueue_download(chapter_id, manga_id)
            .await
            .expect("download should be enqueued");
        let download = state
            .db
            .get_download_by_id(&download_id)
            .await
            .expect("download lookup should succeed")
            .expect("download should exist");
        let archive_resolver = crate::app::downloaded_archive_resolution::path_resolver(&state.db)
            .await
            .expect("archive resolver should resolve");
        let archive_path = archive_resolver.archive_path_for_download(&download);
        let page_dir = archive_path
            .parent()
            .expect("archive path should have parent")
            .join("fixture-pages");
        fs::create_dir_all(&page_dir).expect("page fixture dir should be created");
        let pages = entries
            .iter()
            .map(|(name, bytes)| {
                let path = page_dir.join(name);
                fs::write(&path, bytes).expect("page fixture should be written");
                ((*name).to_string(), path)
            })
            .collect::<Vec<_>>();
        backend_storage::write_originals(
            archive_path.clone(),
            backend_storage::OriginalChapter {
                pages: pages.into_iter().map(|(_, path)| path).collect(),
                comicinfo_xml: "<ComicInfo/>".into(),
                cover: None,
            },
            || false,
        )
        .await
        .expect("archive should be built");
        let archive_size = fs::metadata(&archive_path)
            .expect("archive metadata should load")
            .len();
        state
            .db
            .complete_download(
                &download_id,
                entries.len(),
                &archive_path.to_string_lossy(),
                archive_size,
            )
            .await
            .expect("download should be completed");
    }
}
