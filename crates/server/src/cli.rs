use crate::server;
use backend_config::{ServerConfig, ServerConfigOverrides};
use backend_runtime::{color_logs_enabled, init_tracing};
use backend_sources::SourceRegistry;
use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "manga-server",
    version,
    about = "Run and inspect the manga server"
)]
struct Cli {
    #[command(flatten)]
    server: ServerOptions,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Start the HTTP server.
    Serve(ServerOptions),
    /// `OpenAPI` document commands.
    Openapi(OpenapiCommand),
    /// Configuration commands.
    Config(ConfigCommand),
    /// Source inspection commands.
    Sources(SourcesCommand),
    /// Database maintenance commands.
    Db(DbCommand),
}

#[derive(Args, Debug, Default, Clone)]
struct ServerOptions {
    /// Load the built web UI from disk instead of the embedded assets.
    #[arg(long)]
    web_root: Option<PathBuf>,
    /// External origin, e.g. https://manga.example.com; required for browser sign-in.
    #[arg(long)]
    public_url: Option<String>,
    /// SQLite path or PostgreSQL connection URL.
    #[arg(long)]
    database_url: Option<String>,

    /// Directory containing models.json and exported ONNX graphs.
    #[arg(long)]
    models_dir: Option<PathBuf>,

    /// Explicit device. CPU uses two threads; accelerators never fall back to CPU.
    #[arg(long, value_parser = ["cpu", "migraphx", "cuda", "openvino", "coreml"])]
    upscale_device: Option<String>,

    /// HTTP bind address.
    #[arg(long, alias = "server-addr")]
    addr: Option<String>,

    /// Backend bearer token. Prefer `BACKEND_API_KEY` for shared environments.
    #[arg(long, alias = "api-key")]
    backend_api_key: Option<String>,
}

#[derive(Args, Debug)]
struct OpenapiCommand {
    #[command(subcommand)]
    command: OpenapiSubcommand,
}

#[derive(Subcommand, Debug)]
enum OpenapiSubcommand {
    /// Export the server `OpenAPI` document as JSON.
    Export,
}

#[derive(Args, Debug)]
struct ConfigCommand {
    #[command(subcommand)]
    command: ConfigSubcommand,
}

#[derive(Subcommand, Debug)]
enum ConfigSubcommand {
    /// Print resolved server configuration.
    Print(ConfigPrintOptions),
}

#[derive(Args, Debug)]
struct ConfigPrintOptions {
    #[command(flatten)]
    server: ServerOptions,

    /// Emit JSON instead of text.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct SourcesCommand {
    #[command(subcommand)]
    command: SourcesSubcommand,
}

#[derive(Subcommand, Debug)]
enum SourcesSubcommand {
    /// List compiled-in sources.
    List(SourcesListOptions),
}

#[derive(Args, Debug)]
struct SourcesListOptions {
    #[command(flatten)]
    server: ServerOptions,

    /// Emit JSON instead of text.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct DbCommand {
    #[command(subcommand)]
    command: DbSubcommand,
}

#[derive(Subcommand, Debug)]
enum DbSubcommand {
    /// Create the database if needed and apply pending migrations.
    Migrate(DbMigrateOptions),
}

#[derive(Args, Debug)]
struct DbMigrateOptions {
    #[command(flatten)]
    server: ServerOptions,
}

#[derive(Serialize)]
struct ConfigReport {
    web_root: Option<PathBuf>,
    public_url: Option<String>,
    database_url: String,
    database_backend: backend_persistence::DatabaseBackend,
    models_dir: PathBuf,
    upscale_device: String,
    server_addr: String,
    backend_api_key: SecretStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum SecretStatus {
    NotSet,
    Set,
}

pub(crate) async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let command = cli
        .command
        .unwrap_or(Command::Serve(ServerOptions::default()));

    match command {
        Command::Serve(options) => {
            let config = load_config(cli.server.merge(options))?;
            server::run_server(config).await
        }
        Command::Openapi(command) => run_openapi_command(command),
        Command::Config(command) => run_config_command(command, cli.server),
        Command::Sources(command) => run_sources_command(command, cli.server).await,
        Command::Db(command) => run_db_command(command, cli.server).await,
    }
}

fn run_openapi_command(command: OpenapiCommand) -> anyhow::Result<()> {
    let OpenapiCommand { command } = command;
    match command {
        OpenapiSubcommand::Export => export_openapi(),
    }
}

fn run_config_command(command: ConfigCommand, inherited: ServerOptions) -> anyhow::Result<()> {
    match command.command {
        ConfigSubcommand::Print(options) => {
            let config = load_config(inherited.merge(options.server))?;
            let report = ConfigReport::from_config(&config);
            if options.json {
                write_json(&report)?;
            } else {
                print_config_report(&report);
            }
            Ok(())
        }
    }
}

async fn run_sources_command(
    command: SourcesCommand,
    inherited: ServerOptions,
) -> anyhow::Result<()> {
    init_command_tracing();

    match command.command {
        SourcesSubcommand::List(options) => {
            let _config = load_config(inherited.merge(options.server))?;
            let source_registry = SourceRegistry::new()?;
            let sources = source_registry.sources();
            if options.json {
                write_json(&sources)?;
            } else {
                for source in sources {
                    println!(
                        "{}\t{}\t{}\tapi-v{}\t{}",
                        source.name,
                        source.display_name,
                        source.plugin_version,
                        source.plugin_api_version,
                        if source.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    );
                }
            }
            Ok(())
        }
    }
}

async fn run_db_command(command: DbCommand, inherited: ServerOptions) -> anyhow::Result<()> {
    init_command_tracing();

    match command.command {
        DbSubcommand::Migrate(options) => {
            let config = load_config(inherited.merge(options.server))?;
            backend_persistence::migrate_database(&config.database_url).await?;
            println!(
                "Database migrations applied to {}",
                backend_persistence::redacted_database_url(&config.database_url)
            );
            Ok(())
        }
    }
}

fn export_openapi() -> anyhow::Result<()> {
    crate::export_openapi_to_stdout()
}

fn write_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    serde_json::to_writer_pretty(std::io::stdout().lock(), value)?;
    println!();
    Ok(())
}

fn load_config(options: ServerOptions) -> anyhow::Result<ServerConfig> {
    ServerConfig::load_with_overrides(options.into())
}

fn print_config_report(report: &ConfigReport) {
    println!("database_url: {}", report.database_url);
    println!("database_backend: {}", report.database_backend.as_str());
    println!(
        "public_url: {}",
        report.public_url.as_deref().unwrap_or("not set")
    );
    println!(
        "web_root: {}",
        report
            .web_root
            .as_deref()
            .map_or("embedded".into(), |path| path.display().to_string())
    );
    println!("server_addr: {}", report.server_addr);
    println!("models_dir: {}", report.models_dir.display());
    println!(
        "upscale_device: {} (explicit device; CPU limited to two threads)",
        report.upscale_device
    );
    println!(
        "backend_api_key: {}",
        match report.backend_api_key {
            SecretStatus::Set => "set",
            SecretStatus::NotSet => "not set",
        }
    );
}

fn init_command_tracing() {
    init_tracing("info,chromiumoxide=error", color_logs_enabled());
}

impl ServerOptions {
    fn merge(self, overrides: Self) -> Self {
        Self {
            web_root: overrides.web_root.or(self.web_root),
            public_url: overrides.public_url.or(self.public_url),
            database_url: overrides.database_url.or(self.database_url),
            models_dir: overrides.models_dir.or(self.models_dir),
            upscale_device: overrides.upscale_device.or(self.upscale_device),
            addr: overrides.addr.or(self.addr),
            backend_api_key: overrides.backend_api_key.or(self.backend_api_key),
        }
    }
}

impl From<ServerOptions> for ServerConfigOverrides {
    fn from(options: ServerOptions) -> Self {
        Self {
            web_root: options.web_root,
            public_url: options.public_url,
            database_url: options.database_url,
            models_dir: options.models_dir,
            upscale_device: options.upscale_device,
            server_addr: options.addr,
            backend_api_key: options.backend_api_key,
        }
    }
}

impl ConfigReport {
    fn from_config(config: &ServerConfig) -> Self {
        Self {
            web_root: config.web_root.clone(),
            public_url: config.public_url.clone(),
            database_url: backend_persistence::redacted_database_url(&config.database_url),
            database_backend: backend_persistence::DatabaseBackend::from_url(&config.database_url),
            models_dir: config.models_dir.clone(),
            upscale_device: config.upscale_device.clone(),
            server_addr: config.server_addr.clone(),
            backend_api_key: if config.backend_api_key.is_some() {
                SecretStatus::Set
            } else {
                SecretStatus::NotSet
            },
        }
    }
}
