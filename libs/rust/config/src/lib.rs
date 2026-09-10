#![allow(clippy::missing_errors_doc)]

use schematic::ConfigLoader;
use std::path::{Path, PathBuf};

#[derive(Debug, schematic::Config)]
#[config(rename_all = "snake_case")]
pub struct ServerConfig {
    #[setting(default = PathBuf::from("./data/manga.db"), env = "DB_PATH", parse_env = schematic::env::ignore_empty)]
    pub db_path: PathBuf,

    #[setting(default = "0.0.0.0:4000", env = "SERVER_ADDR", parse_env = schematic::env::ignore_empty)]
    pub server_addr: String,

    #[setting(default = default_plugins_path(), env = "PLUGINS_PATH", parse_env = schematic::env::ignore_empty)]
    pub plugins_path: PathBuf,

    #[setting(env = "SOURCE_PLUGIN_REGISTRY_URL", parse_env = schematic::env::ignore_empty)]
    pub source_plugin_registry_url: Option<String>,

    #[setting(env = "BACKEND_API_KEY", parse_env = schematic::env::ignore_empty)]
    pub backend_api_key: Option<String>,

    #[setting(env = "DISCORD_BOT_TOKEN", parse_env = schematic::env::ignore_empty)]
    pub discord_bot_token: Option<String>,

    #[setting(env = "DISCORD_CHANNEL_ID", parse_env = schematic::env::ignore_empty)]
    pub discord_channel_id: Option<u64>,
}

#[derive(Debug, Default)]
pub struct ServerConfigOverrides {
    pub db_path: Option<PathBuf>,
    pub server_addr: Option<String>,
    pub plugins_path: Option<PathBuf>,
    pub source_plugin_registry_url: Option<String>,
    pub backend_api_key: Option<String>,
    pub discord_bot_token: Option<String>,
    pub discord_channel_id: Option<u64>,
}

impl ServerConfig {
    /// Loads server configuration from the default schematic sources.
    pub fn load() -> anyhow::Result<Self> {
        Ok(ConfigLoader::<Self>::new().load()?.config)
    }

    /// Loads server configuration and applies explicit test or CLI overrides.
    pub fn load_with_overrides(overrides: ServerConfigOverrides) -> anyhow::Result<Self> {
        let mut config = Self::load()?;
        config.apply_overrides(overrides);
        Ok(config)
    }

    fn apply_overrides(&mut self, overrides: ServerConfigOverrides) {
        if let Some(db_path) = overrides.db_path {
            self.db_path = db_path;
        }
        if let Some(server_addr) = overrides.server_addr {
            self.server_addr = server_addr;
        }
        if let Some(plugins_path) = overrides.plugins_path {
            self.plugins_path = plugins_path;
        }
        if let Some(source_plugin_registry_url) = overrides.source_plugin_registry_url {
            self.source_plugin_registry_url = Some(source_plugin_registry_url);
        }
        if let Some(backend_api_key) = overrides.backend_api_key {
            self.backend_api_key = Some(backend_api_key);
        }
        if let Some(discord_bot_token) = overrides.discord_bot_token {
            self.discord_bot_token = Some(discord_bot_token);
        }
        if let Some(discord_channel_id) = overrides.discord_channel_id {
            self.discord_channel_id = Some(discord_channel_id);
        }
    }
}

fn default_plugins_path() -> PathBuf {
    for candidate in [
        Path::new("./plugins"),
        Path::new("../plugins"),
        Path::new("../../plugins"),
    ] {
        if backend_fs::path_exists(candidate) {
            return candidate.to_path_buf();
        }
    }

    PathBuf::from("./plugins")
}
