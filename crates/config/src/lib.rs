#![allow(clippy::missing_errors_doc)]

use schematic::ConfigLoader;
use std::path::PathBuf;

#[derive(Debug, schematic::Config)]
#[config(rename_all = "snake_case")]
pub struct ServerConfig {
    #[setting(default = PathBuf::from("./data/manga.db"), env = "DB_PATH", parse_env = schematic::env::ignore_empty)]
    pub db_path: PathBuf,

    #[setting(default = "0.0.0.0:4000", env = "SERVER_ADDR", parse_env = schematic::env::ignore_empty)]
    pub server_addr: String,



    #[setting(env = "BACKEND_API_KEY", parse_env = schematic::env::ignore_empty)]
    pub backend_api_key: Option<String>,


}

#[derive(Debug, Default)]
pub struct ServerConfigOverrides {
    pub db_path: Option<PathBuf>,
    pub server_addr: Option<String>,
    pub backend_api_key: Option<String>,
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
        if let Some(backend_api_key) = overrides.backend_api_key {
            self.backend_api_key = Some(backend_api_key);
        }
    }
}
