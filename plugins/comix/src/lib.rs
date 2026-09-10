#![allow(clippy::same_length_and_capacity)]

wit_bindgen::generate!({
    path: "../../crates/plugin-host/wit/manga-source.wit",
    world: "manga-source",
});

mod browser_capture;
mod chapters;
mod http;
mod ids;
mod logging;
mod models;
mod parse;
mod source;

pub const BASE_URL: &str = "https://comix.to";
pub const API_URL: &str = "https://comix.to/api/v1";
pub const PLUGIN_VERSION: &str = "3.1.5";

pub struct ComixSource;

pub type PluginResult<T> = Result<T, manga::source::types::PluginError>;

pub fn plugin_error(
    code: &str,
    message: impl Into<String>,
    retryable: bool,
) -> manga::source::types::PluginError {
    manga::source::types::PluginError {
        code: code.to_string(),
        message: message.into(),
        retryable,
    }
}

pub fn non_retryable_plugin_error(
    code: &str,
    message: impl Into<String>,
) -> manga::source::types::PluginError {
    plugin_error(code, message, false)
}

export!(ComixSource);
