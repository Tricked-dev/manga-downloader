use crate::server;
use backend_config::{ServerConfig, ServerConfigOverrides};
use backend_plugin_host::PluginManager;
use backend_runtime::{color_logs_enabled, init_tracing};
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
    /// Plugin inspection commands.
    Plugins(PluginsCommand),
    /// Database maintenance commands.
    Db(DbCommand),
}

#[derive(Args, Debug, Default, Clone)]
struct ServerOptions {
    /// `SQLite` database path.
    #[arg(long)]
    db_path: Option<PathBuf>,

    /// HTTP bind address.
    #[arg(long, alias = "server-addr")]
    addr: Option<String>,

    /// Plugin directory path.
    #[arg(long)]
    plugins_path: Option<PathBuf>,

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
struct PluginsCommand {
    #[command(subcommand)]
    command: PluginsSubcommand,
}

#[derive(Subcommand, Debug)]
enum PluginsSubcommand {
    /// List plugins found in the configured plugin directory.
    List(PluginsListOptions),
}

#[derive(Args, Debug)]
struct PluginsListOptions {
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
    db_path: PathBuf,
    server_addr: String,
    plugins_path: PathBuf,
    source_plugin_registry_url: Option<String>,
    backend_api_key: SecretStatus,
    discord_bot_token: SecretStatus,
    discord_channel_id: Option<u64>,
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
        Command::Plugins(command) => run_plugins_command(command, cli.server).await,
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

async fn run_plugins_command(
    command: PluginsCommand,
    inherited: ServerOptions,
) -> anyhow::Result<()> {
    init_command_tracing();

    match command.command {
        PluginsSubcommand::List(options) => {
            let config = load_config(inherited.merge(options.server))?;
            let plugin_manager = PluginManager::new(&config.plugins_path).await?;
            let sources = plugin_manager.sources();
            if options.json {
                write_json(&sources)?;
            } else if sources.is_empty() {
                println!("No plugins found in {}", config.plugins_path.display());
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
            backend_persistence::migrate_database(&config.db_path.to_string_lossy()).await?;
            println!(
                "Database migrations applied to {}",
                config.db_path.display()
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
    println!("db_path: {}", report.db_path.display());
    println!("server_addr: {}", report.server_addr);
    println!("plugins_path: {}", report.plugins_path.display());
    println!(
        "source_plugin_registry_url: {}",
        report
            .source_plugin_registry_url
            .as_deref()
            .unwrap_or("not set")
    );
    println!(
        "backend_api_key: {}",
        match report.backend_api_key {
            SecretStatus::Set => "set",
            SecretStatus::NotSet => "not set",
        }
    );
    println!(
        "discord_bot_token: {}",
        match report.discord_bot_token {
            SecretStatus::Set => "set",
            SecretStatus::NotSet => "not set",
        }
    );
    println!(
        "discord_channel_id: {}",
        report
            .discord_channel_id
            .map_or_else(|| "not set".to_string(), |id| id.to_string())
    );
}

fn init_command_tracing() {
    init_tracing("info,chromiumoxide=error", color_logs_enabled());
}

impl ServerOptions {
    fn merge(self, overrides: Self) -> Self {
        Self {
            db_path: overrides.db_path.or(self.db_path),
            addr: overrides.addr.or(self.addr),
            plugins_path: overrides.plugins_path.or(self.plugins_path),
            backend_api_key: overrides.backend_api_key.or(self.backend_api_key),
        }
    }
}

impl From<ServerOptions> for ServerConfigOverrides {
    fn from(options: ServerOptions) -> Self {
        Self {
            db_path: options.db_path,
            server_addr: options.addr,
            plugins_path: options.plugins_path,
            source_plugin_registry_url: None,
            backend_api_key: options.backend_api_key,
            discord_bot_token: None,
            discord_channel_id: None,
        }
    }
}

impl ConfigReport {
    fn from_config(config: &ServerConfig) -> Self {
        Self {
            db_path: config.db_path.clone(),
            server_addr: config.server_addr.clone(),
            plugins_path: config.plugins_path.clone(),
            source_plugin_registry_url: config.source_plugin_registry_url.clone(),
            backend_api_key: if config.backend_api_key.is_some() {
                SecretStatus::Set
            } else {
                SecretStatus::NotSet
            },
            discord_bot_token: if config.discord_bot_token.is_some() {
                SecretStatus::Set
            } else {
                SecretStatus::NotSet
            },
            discord_channel_id: config.discord_channel_id,
        }
    }
}
