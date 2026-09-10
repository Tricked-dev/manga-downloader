#![allow(clippy::missing_errors_doc)]

mod api;
mod app;
mod archive_index;
mod build_info;
mod cli;
mod comicinfo;
mod database_maintenance;
mod downloader;
mod jobs;
mod scheduler;
mod search_cache;
mod server;
#[cfg(test)]
mod test_support;

pub(crate) use server::{AppState, DownloadStorageUsage};

pub async fn run_cli() -> anyhow::Result<()> {
    cli::run().await
}

pub fn export_openapi_to_stdout() -> anyhow::Result<()> {
    let (_, openapi) = api::routes::router().split_for_parts();
    serde_json::to_writer_pretty(std::io::stdout().lock(), &openapi)?;
    println!();
    Ok(())
}
