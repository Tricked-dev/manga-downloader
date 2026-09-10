//! Native sources and the shared HTTP, browser, and media boundary.
#![allow(clippy::missing_errors_doc)]
mod comix;
pub mod fetch;
mod http;
mod kit;
pub mod media;
mod media_client;
#[path = "rawkuma/mod.rs"]
mod rawkuma;
mod registry;
pub mod types;

pub use fetch::SourceHttpClient;
pub use media_client::SourceMediaClient;
pub use registry::{SourceActivity, SourceInfo, SourceRegistry, SourceRegistryError};
pub use types::*;
pub type SourceResult<T> = Result<T, SourceError>;

#[async_trait::async_trait]
pub trait Source: Send + Sync {
    fn metadata(&self) -> &SourceMetadata;
    async fn search(&self, query: SearchQuery) -> SourceResult<SearchResults>;
    async fn manga(&self, id: &str) -> SourceResult<Manga>;
    async fn chapters(&self, id: &str) -> SourceResult<Vec<Chapter>>;
    async fn pages(&self, id: &str) -> SourceResult<Vec<Page>>;
}

pub fn source_error(code: &str, message: impl Into<String>, retryable: bool) -> SourceError {
    SourceError {
        code: code.into(),
        message: message.into(),
        retryable,
    }
}
pub fn non_retryable_source_error(code: &str, message: impl Into<String>) -> SourceError {
    source_error(code, message, false)
}
impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for SourceError {}
impl SourceError {
    pub fn is_upstream_blocked(&self) -> bool {
        let message = self.message.to_ascii_lowercase();
        message.contains("cloudflare blocked access")
            || message.contains("sorry, you have been blocked")
            || message.contains("attention required! | cloudflare")
    }
}
