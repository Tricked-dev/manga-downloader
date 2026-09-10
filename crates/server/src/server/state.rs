use crate::downloader;
use backend_cache::MangaCache;
use backend_sources::SourceRegistry;
use backend_telemetry::Telemetry;
use secrecy::SecretString;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;
use tokio::sync::{Mutex, Notify, RwLock};

#[derive(Clone)]
pub(crate) struct AppConfig {
    pub(crate) web_root: Option<PathBuf>,
    pub(crate) public_url: Option<String>,
    pub(crate) cache_disk_path: PathBuf,
    pub(crate) backend_api_key: Option<SecretString>,
}

#[derive(Debug, Default)]
pub(crate) struct DownloadStorageUsage {
    pub(crate) path: Option<PathBuf>,
    pub(crate) bytes: u64,
    pub(crate) refreshed_at: Option<Instant>,
    pub(crate) refresh_in_progress: bool,
}

pub(crate) struct AppState {
    pub(crate) oidc: tokio::sync::OnceCell<crate::api::auth::oidc::OidcRuntime>,
    pub(crate) config: AppConfig,
    pub(crate) db: backend_persistence::Database,
    pub(crate) source_registry: RwLock<SourceRegistry>,
    pub(crate) cache: MangaCache,
    pub(crate) upscaler: backend_upscale::Upscaler,
    pub(crate) upscale_queue: crate::jobs::UpscaleQueue,
    pub(crate) telemetry: Telemetry,
    pub(crate) download_queue_notify: Notify,
    pub(crate) active_download_cancellations:
        Mutex<HashMap<String, Arc<downloader::DownloadCancellation>>>,
    pub(crate) download_storage_usage: Mutex<DownloadStorageUsage>,
    pub(crate) shutdown_drain: ShutdownDrain,
}

pub(crate) struct AppStateParts {
    pub(crate) config: AppConfig,
    pub(crate) db: backend_persistence::Database,
    pub(crate) source_registry: SourceRegistry,
    pub(crate) cache: MangaCache,
    pub(crate) upscaler: backend_upscale::Upscaler,
    pub(crate) upscale_queue: crate::jobs::UpscaleQueue,
    pub(crate) telemetry: Telemetry,
}

pub(crate) fn build_app_state(parts: AppStateParts) -> Arc<AppState> {
    let AppStateParts {
        config,
        db,
        source_registry,
        cache,
        upscaler,
        upscale_queue,
        telemetry,
    } = parts;

    Arc::new(AppState {
        oidc: tokio::sync::OnceCell::new(),
        config,
        db,
        source_registry: RwLock::new(source_registry),
        cache,
        upscaler,
        upscale_queue,
        telemetry,
        download_queue_notify: Notify::new(),
        active_download_cancellations: Mutex::new(HashMap::new()),
        download_storage_usage: Mutex::new(DownloadStorageUsage::default()),
        shutdown_drain: ShutdownDrain::default(),
    })
}

#[cfg(test)]
pub(crate) async fn build_test_app_state(
    label: &str,
    telemetry_name: &'static str,
) -> Arc<AppState> {
    backend_tls::ensure_graviola_rustls_provider().expect("initialize TLS before database clients");
    let root = crate::test_support::temp_path(label);
    let db_path = root.join("test.sqlite3");
    let cache_path = root.join("cache");
    let download_path = root.join("downloads");

    std::fs::create_dir_all(&root).expect("test root should be created");
    std::fs::create_dir_all(&download_path).expect("test download path should be created");

    let database_url = if let Ok(url) = std::env::var("TEST_POSTGRES_URL") {
        let pool = sqlx::PgPool::connect(&url)
            .await
            .expect("connect to PostgreSQL test service");
        let name = format!("manga_server_test_{}", uuid::Uuid::now_v7().simple());
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&pool)
            .await
            .expect("create isolated test database");
        let mut url = url::Url::parse(&url).expect("valid PostgreSQL URL");
        url.set_path(&name);
        pool.close().await;
        url.to_string()
    } else {
        db_path.to_string_lossy().into_owned()
    };
    let db = backend_persistence::Database::open(&database_url)
        .await
        .expect("database should initialize");
    db.set_setting("download_path", &download_path.to_string_lossy())
        .await
        .expect("download path setting should be stored");

    let source_registry = SourceRegistry::new().expect("plugin manager should initialize");
    let cache = MangaCache::new(&cache_path.to_string_lossy(), 1024 * 1024)
        .await
        .expect("cache should initialize");

    build_app_state(AppStateParts {
        config: AppConfig {
            web_root: None,
            public_url: Some("http://localhost".into()),
            cache_disk_path: cache_path,
            backend_api_key: None,
        },
        upscale_queue: crate::jobs::UpscaleQueue::open(&db)
            .await
            .expect("initialize test queue"),
        db,
        source_registry,
        cache,
        upscaler: backend_upscale::Upscaler::start(backend_upscale::UpscaleConfig {
            models_dir: root.join("models"),
            ..Default::default()
        })
        .expect("test upscale worker should start without loading a model"),
        telemetry: Telemetry::for_test(telemetry_name),
    })
}

#[derive(Debug, Default)]
pub(crate) struct ShutdownDrain {
    draining: AtomicBool,
}

impl ShutdownDrain {
    pub(crate) fn begin(&self) {
        self.draining.store(true, Ordering::Release);
    }

    pub(crate) fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Acquire)
    }
}
