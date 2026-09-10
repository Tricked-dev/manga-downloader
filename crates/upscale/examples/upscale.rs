//! Exercise the same dedicated worker used by the server, with GPU profiling.
//! Usage: cargo run -p backend-upscale --example upscale -- MODELS INPUT OUTPUT [DEVICE]
use anyhow::{Context, Result, bail};
use backend_upscale::{GpuDevice, UpscaleConfig, UpscaleOutcome, Upscaler};
use std::path::PathBuf;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let models_dir = PathBuf::from(arguments.next().context("models directory required")?);
    let input = PathBuf::from(arguments.next().context("input image required")?);
    let output = PathBuf::from(arguments.next().context("output AVIF path required")?);
    let device = arguments
        .next()
        .map(|name| name.parse())
        .transpose()?
        .unwrap_or_else(GpuDevice::default);
    let profile_dir = output
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("gpu-profiles");
    let worker = Upscaler::start(UpscaleConfig {
        models_dir,
        device,
        profile_dir: Some(profile_dir),
        ..Default::default()
    })?;
    let result = worker.upscale(tokio::fs::read(input).await?, 2).await;
    worker.shutdown().await?;
    match result? {
        UpscaleOutcome::Complete(page) => {
            tokio::fs::write(output, &page.bytes).await?;
            println!("{}", serde_json::to_string_pretty(&page)?);
        }
        UpscaleOutcome::MissingModels { reason } => bail!("{reason}"),
    }
    Ok(())
}
