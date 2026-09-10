//! Exercise the same dedicated worker used by the server, with execution profiling.
//! Usage: cargo run -p backend-upscale --example upscale -- MODELS INPUT OUTPUT [DEVICE] [SCALE]
use anyhow::{Context, Result, bail};
use backend_upscale::{UpscaleConfig, UpscaleDevice, UpscaleOutcome, Upscaler};
use std::path::PathBuf;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let mut arguments = std::env::args().skip(1);
    let models_dir = PathBuf::from(arguments.next().context("models directory required")?);
    let input = PathBuf::from(arguments.next().context("input image required")?);
    let output = PathBuf::from(arguments.next().context("output AVIF path required")?);
    let device = arguments
        .next()
        .map(|name| name.parse())
        .transpose()?
        .unwrap_or_else(UpscaleDevice::default);
    let scale = arguments
        .next()
        .map(|value| value.parse::<u32>())
        .transpose()?
        .unwrap_or(2);
    anyhow::ensure!(matches!(scale, 2 | 4), "scale must be 2 or 4");
    let profile_dir = output
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("execution-profiles");
    let worker = Upscaler::start(UpscaleConfig {
        models_dir,
        device,
        profile_dir: Some(profile_dir),
        ..Default::default()
    })?;
    let input = tokio::fs::read(input).await?;
    let started = std::time::Instant::now();
    let result = worker.upscale(input, scale).await;
    let elapsed_ms = started.elapsed().as_millis();
    worker.shutdown().await?;
    match result? {
        UpscaleOutcome::Complete(page) => {
            tokio::fs::write(output, &page.bytes).await?;
            let mut report = serde_json::to_value(&page)?;
            report["elapsed_ms"] = serde_json::json!(elapsed_ms);
            report["output_bytes"] = serde_json::json!(page.bytes.len());
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        UpscaleOutcome::MissingModels { reason } => bail!("{reason}"),
    }
    Ok(())
}
