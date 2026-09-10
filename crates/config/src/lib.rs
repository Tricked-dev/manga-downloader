#![allow(clippy::missing_errors_doc)]

use schematic::ConfigLoader;
use std::path::PathBuf;

#[derive(Debug, schematic::Config)]
#[config(rename_all = "snake_case")]
pub struct ServerConfig {
    #[setting(default = "./data/manga.db", env = "DATABASE_URL", parse_env = schematic::env::ignore_empty)]
    pub database_url: String,

    #[setting(default = PathBuf::from("./data/models"), env = "MODELS_DIR", parse_env = schematic::env::ignore_empty)]
    pub models_dir: PathBuf,

    #[setting(default = default_upscale_device(), env = "UPSCALE_DEVICE", parse_env = schematic::env::ignore_empty)]
    pub upscale_device: String,

    #[setting(default = "0.0.0.0:4000", env = "SERVER_ADDR", parse_env = schematic::env::ignore_empty)]
    pub server_addr: String,

    #[setting(env = "PUBLIC_URL", parse_env = schematic::env::ignore_empty)]
    pub public_url: Option<String>,

    #[setting(env = "BACKEND_API_KEY", parse_env = schematic::env::ignore_empty)]
    pub backend_api_key: Option<String>,
}

#[derive(Debug, Default)]
pub struct ServerConfigOverrides {
    pub public_url: Option<String>,
    pub database_url: Option<String>,
    pub models_dir: Option<PathBuf>,
    pub upscale_device: Option<String>,
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
        if let Some(public_url) = overrides.public_url {
            self.public_url = Some(public_url);
        }
        if let Some(models_dir) = overrides.models_dir {
            self.models_dir = models_dir;
        }
        if let Some(upscale_device) = overrides.upscale_device {
            self.upscale_device = upscale_device;
        }
        if let Some(database_url) = overrides.database_url {
            self.database_url = database_url;
        }
        if let Some(server_addr) = overrides.server_addr {
            self.server_addr = server_addr;
        }
        if let Some(backend_api_key) = overrides.backend_api_key {
            self.backend_api_key = Some(backend_api_key);
        }
    }
}

fn default_upscale_device() -> String {
    if cfg!(target_os = "macos") {
        "coreml"
    } else {
        "cpu"
    }
    .into()
}
