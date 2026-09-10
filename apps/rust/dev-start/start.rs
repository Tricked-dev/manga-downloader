use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitCode, ExitStatus},
    sync::atomic::{AtomicI32, Ordering},
    thread,
    time::Duration,
};

use rusqlite::{Connection, OpenFlags, OptionalExtension};

static SIGNAL: AtomicI32 = AtomicI32::new(0);

const DEFAULT_RUST_LOG: &str = "info,manga_server=info,chromiumoxide=error";
const CACHE_TRACE_RUST_LOG: &str =
    "info,manga_server=trace,manga_server::cache=trace,chromiumoxide=error";
const BACKEND_API_KEY_ENV: &str = "BACKEND_API_KEY";
const BACKEND_API_KEY_SETTING: &str = "backend_api_key";

fn main() -> ExitCode {
    install_signal_handlers();

    match run() {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<u8, String> {
    let options = DevOptions::parse(env::args().skip(1))?;
    if options.show_help {
        print_help();
        return Ok(0);
    }

    let workspace = workspace_dir()?;
    env::set_current_dir(&workspace)
        .map_err(|err| format!("failed to enter workspace {}: {err}", workspace.display()))?;

    let local_env = load_local_env(&workspace);
    let dev_env = DevEnv::load(&workspace, &options, &local_env);
    dev_env.prepare_directories()?;

    let mut supervisor = Supervisor::new(workspace, dev_env.values.clone());

    println!("[start] backend: bazel run //apps/rust/server:server -- serve");
    supervisor.spawn_bazel(
        "backend",
        &["run", "//apps/rust/server:server", "--", "serve"],
    )?;

    println!("[start] frontend: pnpm --filter @manga-server/web exec vp dev");
    supervisor.spawn_command(
        "frontend",
        "pnpm",
        &["--filter", "@manga-server/web", "exec", "vp", "dev"],
    )?;

    println!("[start] BACKEND_URL={}", dev_env.values["BACKEND_URL"]);
    println!(
        "[start] PUBLIC_API_BASE={}",
        dev_env.values["PUBLIC_API_BASE"]
    );
    println!("[start] frontend dev server will use Vite's configured port");

    supervisor.wait()
}

#[derive(Debug, Default)]
struct DevOptions {
    cache_trace: bool,
    show_help: bool,
}

impl DevOptions {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();

        for arg in args {
            match arg.as_str() {
                "--cache-trace" => options.cache_trace = true,
                "-h" | "--help" => options.show_help = true,
                _ => {
                    return Err(format!(
                        "unknown argument {arg:?}; use --help for supported options"
                    ));
                }
            }
        }

        Ok(options)
    }
}

fn print_help() {
    println!("Start the Manga Downloader backend and frontend through Bazel.");
    println!();
    println!("Usage:");
    println!("  bazel run //apps/rust/dev-start:start -- [--cache-trace]");
    println!();
    println!("Options:");
    println!("  --cache-trace  Enable extra manga_server cache tracing in RUST_LOG");
    println!("  -h, --help     Print this help text");
}

#[derive(Debug)]
struct DevEnv {
    values: BTreeMap<String, String>,
}

impl DevEnv {
    fn load(workspace: &Path, options: &DevOptions, local_env: &BTreeMap<String, String>) -> Self {
        let mut values = BTreeMap::new();
        values.insert(
            "DB_PATH".to_owned(),
            path_env_or_default("DB_PATH", "data/manga.db", workspace, local_env),
        );
        values.insert(
            "SERVER_ADDR".to_owned(),
            env_or_default("SERVER_ADDR", "0.0.0.0:4000", local_env),
        );
        values.insert(
            "DOWNLOAD_PATH".to_owned(),
            path_env_or_default("DOWNLOAD_PATH", "data/downloads", workspace, local_env),
        );
        values.insert(
            "PLUGINS_PATH".to_owned(),
            path_env_or_default("PLUGINS_PATH", "plugins", workspace, local_env),
        );
        values.insert(
            "SOURCE_PLUGIN_REGISTRY_URL".to_owned(),
            env_or_default("SOURCE_PLUGIN_REGISTRY_URL", "", local_env),
        );
        values.insert(
            BACKEND_API_KEY_ENV.to_owned(),
            backend_api_key_env(Path::new(&values["DB_PATH"]), local_env),
        );
        values.insert("RUST_LOG".to_owned(), rust_log(options, local_env));

        let server_port = server_port(&values["SERVER_ADDR"]);
        let backend_url = env_or_default(
            "BACKEND_URL",
            &format!("http://127.0.0.1:{server_port}"),
            local_env,
        );
        values.insert("BACKEND_URL".to_owned(), backend_url.clone());
        values.insert(
            "BACKEND_INTERNAL_URL".to_owned(),
            env_or_default("BACKEND_INTERNAL_URL", &backend_url, local_env),
        );
        values.insert(
            "PUBLIC_API_BASE".to_owned(),
            env_or_default("PUBLIC_API_BASE", &backend_url, local_env),
        );

        for key in [
            "BROWSER_USE_API_KEY",
            "BROWSER_USE_CONNECT_URL",
            "BROWSER_USE_PROFILE_ID",
            "BROWSER_USE_TIMEOUT_MINUTES",
            "MANGA_SERVER_BROWSER_CDP_URL",
        ] {
            insert_optional_env(&mut values, key, local_env);
        }

        Self { values }
    }

    fn prepare_directories(&self) -> Result<(), String> {
        ensure_parent_dir(Path::new(&self.values["DB_PATH"]))?;
        ensure_dir(Path::new(&self.values["DOWNLOAD_PATH"]))?;
        ensure_dir(Path::new(&self.values["PLUGINS_PATH"]))
    }
}

fn rust_log(options: &DevOptions, local_env: &BTreeMap<String, String>) -> String {
    if options.cache_trace {
        CACHE_TRACE_RUST_LOG.to_owned()
    } else {
        env_or_default("RUST_LOG", DEFAULT_RUST_LOG, local_env)
    }
}

fn env_or_default(key: &str, default: &str, local_env: &BTreeMap<String, String>) -> String {
    env_value(key, local_env).unwrap_or_else(|| default.to_owned())
}

fn backend_api_key_env(db_path: &Path, local_env: &BTreeMap<String, String>) -> String {
    env_value(BACKEND_API_KEY_ENV, local_env)
        .or_else(|| persisted_backend_api_key(db_path).ok().flatten())
        .unwrap_or_default()
}

fn persisted_backend_api_key(db_path: &Path) -> rusqlite::Result<Option<String>> {
    if !db_path.exists() {
        return Ok(None);
    }

    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        [BACKEND_API_KEY_SETTING],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map(|value| {
        value
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}

fn path_env_or_default(
    key: &str,
    default: &str,
    workspace: &Path,
    local_env: &BTreeMap<String, String>,
) -> String {
    let path = env_value(key, local_env).unwrap_or_else(|| default.to_owned());
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    }
    .to_string_lossy()
    .into_owned()
}

fn insert_optional_env(
    values: &mut BTreeMap<String, String>,
    key: &str,
    local_env: &BTreeMap<String, String>,
) {
    if let Some(value) = env_value(key, local_env).filter(|value| !value.trim().is_empty()) {
        values.insert(key.to_owned(), value);
    }
}

fn env_value(key: &str, local_env: &BTreeMap<String, String>) -> Option<String> {
    env::var(key).ok().or_else(|| local_env.get(key).cloned())
}

fn load_local_env(workspace: &Path) -> BTreeMap<String, String> {
    let path = workspace.join(".env");
    let Ok(contents) = fs::read_to_string(&path) else {
        return BTreeMap::new();
    };

    contents
        .lines()
        .filter_map(parse_env_line)
        .collect::<BTreeMap<_, _>>()
}

fn parse_env_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }

    Some((key.to_owned(), unquote_env_value(value.trim()).to_owned()))
}

fn unquote_env_value(value: &str) -> &str {
    if value.len() >= 2 {
        let first = value.as_bytes()[0];
        let last = value.as_bytes()[value.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn server_port(addr: &str) -> &str {
    addr.rsplit_once(':').map_or(addr, |(_, port)| port)
}

fn workspace_dir() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("BUILD_WORKSPACE_DIRECTORY") {
        return Ok(PathBuf::from(path));
    }

    env::current_dir().map_err(|err| format!("failed to resolve current directory: {err}"))
}

fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    match path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        Some(parent) => ensure_dir(parent),
        None => Ok(()),
    }
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|err| format!("failed to create {}: {err}", path.display()))
}

struct Supervisor {
    workspace: PathBuf,
    env: BTreeMap<String, String>,
    children: Vec<ManagedChild>,
}

impl Supervisor {
    fn new(workspace: PathBuf, env: BTreeMap<String, String>) -> Self {
        Self {
            workspace,
            env,
            children: Vec::new(),
        }
    }

    fn spawn_bazel(&mut self, label: &'static str, args: &[&str]) -> Result<(), String> {
        self.spawn_command(label, "bazel", args)
    }

    fn spawn_command(
        &mut self,
        label: &'static str,
        program: &str,
        args: &[&str],
    ) -> Result<(), String> {
        let child = Command::new(program)
            .args(args)
            .current_dir(&self.workspace)
            .envs(&self.env)
            .spawn()
            .map_err(|err| format!("failed to start {label}: {err}"))?;

        self.children.push(ManagedChild { label, child });
        Ok(())
    }

    fn wait(&mut self) -> Result<u8, String> {
        loop {
            let signal = SIGNAL.load(Ordering::SeqCst);
            if signal != 0 {
                eprintln!("[start] received signal {signal}; stopping children");
                self.kill_all();
                self.wait_all();
                return Ok(signal_exit_code(signal));
            }

            for index in 0..self.children.len() {
                if let Some(status) = self.children[index].child.try_wait().map_err(|err| {
                    format!("failed to poll {}: {err}", self.children[index].label)
                })? {
                    let label = self.children[index].label;
                    let code = exit_code(status);
                    eprintln!("[start] {label} exited with {status}; stopping remaining children");
                    self.kill_except(index);
                    self.wait_all();
                    return Ok(code);
                }
            }

            thread::sleep(Duration::from_millis(500));
        }
    }

    fn kill_except(&mut self, index_to_keep: usize) {
        for (index, child) in self.children.iter_mut().enumerate() {
            if index != index_to_keep {
                child.kill();
            }
        }
    }

    fn kill_all(&mut self) {
        for child in &mut self.children {
            child.kill();
        }
    }

    fn wait_all(&mut self) {
        for child in &mut self.children {
            let _ = child.child.wait();
        }
    }
}

struct ManagedChild {
    label: &'static str,
    child: Child,
}

impl ManagedChild {
    fn kill(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
        }
    }
}

fn exit_code(status: ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return truncate_exit_code(code);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;

        if let Some(signal) = status.signal() {
            return signal_exit_code(signal);
        }
    }

    1
}

fn signal_exit_code(signal: i32) -> u8 {
    truncate_exit_code(128 + signal)
}

fn truncate_exit_code(code: i32) -> u8 {
    u8::try_from(code).unwrap_or(1)
}

#[cfg(unix)]
fn install_signal_handlers() {
    const SIGINT: i32 = 2;
    const SIGTERM: i32 = 15;

    unsafe extern "C" {
        fn signal(sig: i32, handler: extern "C" fn(i32)) -> extern "C" fn(i32);
    }

    extern "C" fn handle_signal(signal: i32) {
        SIGNAL.store(signal, Ordering::SeqCst);
    }

    // The handler only stores an atomic flag; process cleanup stays in the main loop.
    unsafe {
        let _ = signal(SIGINT, handle_signal);
        let _ = signal(SIGTERM, handle_signal);
    }
}

#[cfg(not(unix))]
fn install_signal_handlers() {}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        sync::{Mutex, MutexGuard, OnceLock},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvSnapshot<'a> {
        _guard: MutexGuard<'a, ()>,
        values: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvSnapshot<'_> {
        fn clear(keys: &[&'static str]) -> Self {
            let guard = ENV_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .expect("env test lock should not be poisoned");
            let values = keys
                .iter()
                .map(|key| (*key, env::var_os(key)))
                .collect::<Vec<_>>();

            for key in keys {
                unsafe {
                    env::remove_var(key);
                }
            }

            Self {
                _guard: guard,
                values,
            }
        }
    }

    impl Drop for EnvSnapshot<'_> {
        fn drop(&mut self) {
            for (key, value) in &self.values {
                unsafe {
                    match value {
                        Some(value) => env::set_var(key, value),
                        None => env::remove_var(key),
                    }
                }
            }
        }
    }

    #[test]
    fn dev_env_uses_persisted_backend_api_key_when_env_is_absent() {
        let _env = EnvSnapshot::clear(&[BACKEND_API_KEY_ENV, "DB_PATH"]);
        let db_path = test_db_path("persisted");
        write_backend_api_key_db(&db_path, "persisted-secret");

        let local_env =
            BTreeMap::from([("DB_PATH".to_owned(), db_path.to_string_lossy().into_owned())]);
        let dev_env = DevEnv::load(Path::new("/workspace"), &DevOptions::default(), &local_env);

        assert_eq!(
            dev_env.values.get(BACKEND_API_KEY_ENV).map(String::as_str),
            Some("persisted-secret"),
        );

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn dev_env_prefers_explicit_backend_api_key_over_persisted_setting() {
        let _env = EnvSnapshot::clear(&[BACKEND_API_KEY_ENV, "DB_PATH"]);
        let db_path = test_db_path("explicit");
        write_backend_api_key_db(&db_path, "persisted-secret");

        let local_env = BTreeMap::from([
            ("DB_PATH".to_owned(), db_path.to_string_lossy().into_owned()),
            (BACKEND_API_KEY_ENV.to_owned(), "explicit-secret".to_owned()),
        ]);
        let dev_env = DevEnv::load(Path::new("/workspace"), &DevOptions::default(), &local_env);

        assert_eq!(
            dev_env.values.get(BACKEND_API_KEY_ENV).map(String::as_str),
            Some("explicit-secret"),
        );

        let _ = fs::remove_file(db_path);
    }

    fn test_db_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        env::temp_dir().join(format!(
            "manga-dev-start-{name}-{}-{nanos}.sqlite",
            std::process::id()
        ))
    }

    fn write_backend_api_key_db(path: &Path, value: &str) {
        let conn = Connection::open(path).expect("test database should open");
        conn.execute(
            "CREATE TABLE app_settings (key TEXT NOT NULL PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .expect("settings table should be created");
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)",
            [BACKEND_API_KEY_SETTING, value],
        )
        .expect("backend api key setting should be inserted");
    }
}
