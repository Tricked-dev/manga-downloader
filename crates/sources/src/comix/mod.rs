mod browser_capture;
mod chapters;
mod http;
mod ids;
mod logging;
mod models;
mod parse;
mod source;
use crate::{SourceHttpClient, types::*};
use crate::{SourceResult, non_retryable_source_error, source_error};
pub const BASE_URL: &str = "https://comix.to";
pub const API_URL: &str = "https://comix.to/api/v1";
pub const PLUGIN_VERSION: &str = "3.1.5";
pub struct ComixSource {
    http: SourceHttpClient,
    metadata: SourceMetadata,
}
impl ComixSource {
    pub fn new(http: SourceHttpClient) -> Self {
        Self {
            http,
            metadata: source::metadata(),
        }
    }
}
