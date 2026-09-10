// This crate is an internal runtime boundary around plugins and IO. Pedantic `# Errors` docs on
// every public helper would mostly restate anyhow/sqlx propagation without adding signal.
// `SourceInfo` mirrors the UI/API payload shape, so splitting each boolean into enums would make
// serialization and downstream consumers noisier without clarifying the model.
#![allow(clippy::missing_errors_doc)]

pub const PLUGIN_API_VERSION: u32 = 6;

pub mod fetch;
mod host;
mod manager;
pub mod media;
pub mod registry;
pub mod runtime;

pub use manager::{
    PluginInstallResult, PluginManager, PluginManagerError, PluginMediaClient,
    PluginRegistryInstallResult, PluginRuntimeActivity, PluginRuntimeError, SourceInfo,
};
