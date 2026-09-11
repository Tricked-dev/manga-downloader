use super::router::build_router;
use super::shutdown::run_until_shutdown;
use super::state::{AppConfig, AppState, AppStateParts, build_app_state};
use crate::{
    app::{settings, source_catalog_changes},
    build_info,
};
use anyhow::Context as _;
use autometrics::autometrics;
use backend_cache::MangaCache;
use backend_config::ServerConfig;
use backend_runtime::color_logs_enabled;
use backend_sources::SourceRegistry;
use backend_telemetry::Telemetry;
use secrecy::SecretString;
use std::env;
use std::os::fd::{FromRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::TcpListener;

const SYSTEMD_LISTEN_FDS_START: RawFd = 3;

pub(crate) async fn run_server(config: ServerConfig) -> anyhow::Result<()> {
    backend_tls::ensure_graviola_rustls_provider()?;

    let telemetry = Telemetry::init("info,chromiumoxide=error", color_logs_enabled())?;
    let ServerContext { state, server_addr } = bootstrap_server(config, telemetry).await?;
    let app = build_router(&state);
    let (listener, listener_source) = open_listener(&server_addr).await?;
    let local_addr = listener
        .local_addr()
        .context("failed to inspect HTTP listener address")?;

    tracing::info!(
        address = %local_addr,
        configured_address = %server_addr,
        listener = listener_source.as_str(),
        "Server Listening",
    );

    run_until_shutdown(listener, app, &state).await
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListenerSource {
    ConfiguredBind,
    SystemdSocketActivation,
}

impl ListenerSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::ConfiguredBind => "configured-bind",
            Self::SystemdSocketActivation => "systemd-socket-activation",
        }
    }
}

async fn open_listener(server_addr: &str) -> anyhow::Result<(TcpListener, ListenerSource)> {
    match systemd_listener_fd()? {
        Some(fd) => {
            let listener = tcp_listener_from_raw_fd(fd)?;
            Ok((listener, ListenerSource::SystemdSocketActivation))
        }
        None => {
            let listener = TcpListener::bind(server_addr)
                .await
                .with_context(|| format!("failed to bind HTTP listener to {server_addr}"))?;
            Ok((listener, ListenerSource::ConfiguredBind))
        }
    }
}

fn systemd_listener_fd() -> anyhow::Result<Option<RawFd>> {
    systemd_listener_fd_from_env(
        std::process::id(),
        env::var("LISTEN_PID").ok().as_deref(),
        env::var("LISTEN_FDS").ok().as_deref(),
    )
}

fn systemd_listener_fd_from_env(
    current_pid: u32,
    listen_pid: Option<&str>,
    listen_fds: Option<&str>,
) -> anyhow::Result<Option<RawFd>> {
    let Some(listen_pid) = listen_pid else {
        return Ok(None);
    };

    let listen_pid = listen_pid
        .parse::<u32>()
        .context("invalid LISTEN_PID from systemd socket activation")?;
    if listen_pid != current_pid {
        return Ok(None);
    }

    let listen_fds = listen_fds
        .unwrap_or("0")
        .parse::<usize>()
        .context("invalid LISTEN_FDS from systemd socket activation")?;

    match listen_fds {
        0 => Ok(None),
        1 => Ok(Some(SYSTEMD_LISTEN_FDS_START)),
        count => Err(anyhow::anyhow!(
            "expected exactly one systemd activation socket, got {count}"
        )),
    }
}

fn tcp_listener_from_raw_fd(fd: RawFd) -> anyhow::Result<TcpListener> {
    // SAFETY: systemd socket activation transfers ownership of listening file
    // descriptors starting at fd 3 to the service process. This function is
    // only called after LISTEN_PID confirms those descriptors are for us.
    let std_listener = unsafe { std::net::TcpListener::from_raw_fd(fd) };
    std_listener
        .set_nonblocking(true)
        .with_context(|| format!("failed to set inherited listener fd {fd} nonblocking"))?;
    TcpListener::from_std(std_listener)
        .with_context(|| format!("failed to convert inherited listener fd {fd}"))
}

struct ServerContext {
    state: Arc<AppState>,
    server_addr: String,
}

#[autometrics(track_concurrency)]
async fn bootstrap_server(
    config: ServerConfig,
    telemetry: Telemetry,
) -> anyhow::Result<ServerContext> {
    let ServerConfig {
        web_root,
        public_url,
        database_url,
        models_dir,
        upscale_device,
        upscale_openvino_device,
        server_addr,
        backend_api_key,
    } = config;

    if let Some(root) = &web_root {
        anyhow::ensure!(
            root.join("index.html").is_file(),
            "WEB_ROOT must contain the built UI index.html"
        );
    }
    let db = backend_persistence::Database::open(&database_url).await?;
    db.apply_env_overrides().await?;
    let backend_api_key = match backend_api_key {
        Some(key) => key,
        None => settings::ensure_backend_api_key(&db).await?,
    };

    let settings = settings::interface(&db);
    let download_path = settings.download_path().await?;
    let cache_disk_path = settings.cache_disk_path().await?;
    let cache_max_memory_bytes = settings.cache_max_memory_bytes().await?;

    backend_fs::create_dir_all(Path::new(&download_path)).await?;
    backend_fs::create_dir_all(Path::new(&cache_disk_path)).await?;
    backend_fs::create_dir_all(&std::env::temp_dir()).await?;

    let mut source_registry = SourceRegistry::new()?;
    let disabled_plugins = db.get_disabled_source_keys().await?;
    source_registry.set_disabled_sources(&disabled_plugins);
    source_catalog_changes::sync_startup_source_catalog(&source_registry, &db).await?;
    let recovered_downloads = db.recover_interrupted_downloads().await?;
    if recovered_downloads > 0 {
        tracing::warn!(
            recovered_downloads,
            "Interrupted Downloads Recovered On Startup",
        );
    }

    let backend_api_key = Some(SecretString::from(backend_api_key));
    let cache = MangaCache::new(&cache_disk_path, cache_max_memory_bytes).await?;
    let build_info = build_info::server_build_info();
    tracing::info!(
        version = %build_info.version,
        branch = %build_info.branch.as_deref().unwrap_or("unknown"),
        commit = %build_info.commit_short_hash.as_deref().unwrap_or("unknown"),
        build_target = %build_info.build_target.as_deref().unwrap_or("unknown"),
        database_url = %backend_persistence::redacted_database_url(&database_url),
        database_backend = db.backend().as_str(),
        loaded_plugins = source_registry.sources().len(),
        disabled_plugins = disabled_plugins.len(),
        api_key_enabled = backend_api_key.is_some(),
        cache_disk_path = %cache_disk_path,
        cache_max_memory_bytes,
        "Server Boot",
    );

    let upscaler = backend_upscale::Upscaler::start(backend_upscale::UpscaleConfig {
        models_dir,
        device: upscale_device.parse()?,
        openvino_device: upscale_openvino_device,
        ..Default::default()
    })?;
    let upscale_queue = crate::jobs::UpscaleQueue::open(&db).await?;
    let state = build_app_state(AppStateParts {
        config: AppConfig {
            web_root,
            public_url: public_url
                .map(|url| crate::api::auth::normalize_public_url(&url))
                .transpose()?,
            cache_disk_path: PathBuf::from(cache_disk_path),
            backend_api_key,
        },
        db,
        cache,
        source_registry,
        upscaler,
        upscale_queue,
        telemetry,
    });
    super::router::spawn_metrics_refresh_loop(Arc::clone(&state));

    Ok(ServerContext { state, server_addr })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_listener_fd_ignores_absent_activation_env() {
        let fd = systemd_listener_fd_from_env(123, None, None).unwrap();

        assert_eq!(fd, None);
    }

    #[test]
    fn systemd_listener_fd_ignores_parent_activation_env() {
        let fd = systemd_listener_fd_from_env(123, Some("456"), Some("1")).unwrap();

        assert_eq!(fd, None);
    }

    #[test]
    fn systemd_listener_fd_uses_first_activation_fd() {
        let fd = systemd_listener_fd_from_env(123, Some("123"), Some("1")).unwrap();

        assert_eq!(fd, Some(SYSTEMD_LISTEN_FDS_START));
    }

    #[test]
    fn systemd_listener_fd_treats_zero_fds_as_no_activation() {
        let fd = systemd_listener_fd_from_env(123, Some("123"), Some("0")).unwrap();

        assert_eq!(fd, None);
    }

    #[test]
    fn systemd_listener_fd_rejects_multiple_activation_fds() {
        let error = systemd_listener_fd_from_env(123, Some("123"), Some("2")).unwrap_err();

        assert!(error.to_string().contains("expected exactly one"));
    }
}
