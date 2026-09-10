//! `mangajanai-rs` command line entry point.

mod convert;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use manga_core::{Device, DeviceOptions, ModelManifest, TileConfig, UpscaleModel};
use tracing::info;

use crate::convert::ModelCache;

#[derive(Debug, Parser)]
#[command(
    name = "mangajanai-rs",
    version,
    about = "Upscale manga pages with MangaJaNai models via ONNX Runtime"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Upscale a single image.
    Upscale(UpscaleArgs),
    /// Upscale an image, a directory, or a ZIP/CBZ archive.
    Convert(ConvertArgs),
}

/// Model choice, shared by both subcommands.
#[derive(Debug, Args)]
struct ModelArgs {
    /// Path to an exported MangaJaNai `.onnx` model, or `auto` to pick one
    /// from the manifest based on the page's height.
    #[arg(long, default_value = "auto")]
    model: String,
    /// Directory holding `models.json`, used when `--model auto` is given.
    #[arg(long, default_value = "models")]
    models_dir: PathBuf,
    /// Target upscale factor, used to choose between the 2x and 4x models.
    /// A value of 2 selects the 2x model, matching upstream.
    #[arg(long, default_value_t = 4)]
    scale: u32,
    /// Execution provider. `auto` prefers a GPU backend and falls back to CPU,
    /// logging which it chose; naming one explicitly never falls back.
    #[arg(long, default_value = "auto", value_parser = Device::from_str)]
    device: Device,
    /// OpenVINO target: `CPU`, `GPU`, `GPU.0`, `NPU`, `AUTO`. Left unset,
    /// OpenVINO chooses for itself and may well pick the CPU, so set this
    /// explicitly if you mean to use the GPU.
    #[arg(long)]
    openvino_device_type: Option<String>,
}

impl ModelArgs {
    fn device_options(&self) -> DeviceOptions {
        DeviceOptions {
            openvino_device_type: self.openvino_device_type.clone(),
        }
    }
}

/// Tiling, shared by both subcommands.
#[derive(Debug, Args)]
struct TilingArgs {
    /// Tile size in source pixels. Use 0 to process the whole page as one
    /// tensor, which needs far more memory but avoids blending entirely.
    #[arg(long, default_value_t = 512)]
    tile_size: u32,
    /// Overlap between neighbouring tiles, in source pixels.
    #[arg(long, default_value_t = 32)]
    overlap: u32,
}

impl TilingArgs {
    /// `None` means "no tiling at all", which is the exact-parity path.
    fn config(&self) -> Result<Option<TileConfig>> {
        if self.tile_size == 0 {
            return Ok(None);
        }
        let config = TileConfig {
            tile_size: self.tile_size,
            overlap: self.overlap,
        };
        config.validate()?;
        Ok(Some(config))
    }

    fn describe(&self) -> String {
        if self.tile_size == 0 {
            "whole page".to_owned()
        } else {
            format!("{}px tiles, {}px overlap", self.tile_size, self.overlap)
        }
    }
}

#[derive(Debug, Args)]
struct UpscaleArgs {
    #[command(flatten)]
    model: ModelArgs,
    #[command(flatten)]
    tiling: TilingArgs,
    /// Source image.
    input: PathBuf,
    /// Destination image.
    output: PathBuf,
}

#[derive(Debug, Args)]
struct ConvertArgs {
    #[command(flatten)]
    model: ModelArgs,
    #[command(flatten)]
    tiling: TilingArgs,
    /// Source image, directory, `.zip` or `.cbz`.
    input: PathBuf,
    /// Destination, matching the kind of the input.
    output: PathBuf,
}

fn main() -> Result<()> {
    // Colour only when stderr is a terminal, and honour NO_COLOR. Without
    // this the escape sequences end up in redirected logs, where they defeat
    // grep and make the output awkward to read.
    let use_colour = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    tracing_subscriber::fmt()
        .with_ansi(use_colour)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    match Cli::parse().command {
        Command::Upscale(args) => upscale(args),
        Command::Convert(args) => run_convert(args),
    }
}

/// Work out which `.onnx` file to run, honouring `--model auto`.
fn resolve_model_path(args: &ModelArgs, page_height: u32) -> Result<PathBuf> {
    if args.model != "auto" {
        return Ok(PathBuf::from(&args.model));
    }

    let manifest_path = args.models_dir.join("models.json");
    let manifest = ModelManifest::load(&manifest_path).with_context(|| {
        format!(
            "--model auto needs a manifest at {}. Generate one with \
             tools/make_manifest.py after exporting models.",
            manifest_path.display()
        )
    })?;

    let entry = manifest.select(page_height, args.scale)?;
    info!(
        model = %entry.name,
        target_height = entry.target_height.unwrap_or(0),
        scale = entry.scale,
        page_height,
        "selected model automatically"
    );
    Ok(manifest.resolve(entry))
}

fn upscale(args: UpscaleArgs) -> Result<()> {
    let started = Instant::now();
    let tiling = args.tiling.config()?;

    // The page has to be decoded before the model can be chosen, because
    // selection depends on its height.
    let source = manga_core::image::load_rgb8(&args.input)?;
    let (width, height) = source.dimensions();

    let model_path = resolve_model_path(&args.model, height)?;
    let mut model =
        UpscaleModel::open_with(&model_path, args.model.device, &args.model.device_options())?;
    let properties = model.properties().clone();
    info!(
        model = properties.name.as_deref().unwrap_or("unknown"),
        architecture = properties.architecture.as_deref().unwrap_or("unknown"),
        scale = properties.scale,
        device = %model.device(),
        load_ms = started.elapsed().as_millis() as u64,
        "loaded model"
    );

    let inference_started = Instant::now();
    let upscaled = match tiling {
        Some(config) => manga_core::upscale_tiled(&mut model, &source, config),
        None => manga_core::upscale_image(&mut model, &source),
    }
    .with_context(|| format!("failed to upscale {}", args.input.display()))?;
    let inference_ms = inference_started.elapsed().as_millis() as u64;

    manga_core::image::save_rgb8(&args.output, &upscaled)?;

    let (out_width, out_height) = upscaled.dimensions();
    info!(
        input = %format!("{width}x{height}"),
        output = %format!("{out_width}x{out_height}"),
        tiling = %args.tiling.describe(),
        inference_ms,
        total_ms = started.elapsed().as_millis() as u64,
        "wrote {}",
        args.output.display()
    );
    Ok(())
}

fn run_convert(args: ConvertArgs) -> Result<()> {
    let started = Instant::now();
    let tiling = args.tiling.config()?;
    let mut models = ModelCache::new(
        &args.model.model,
        &args.model.models_dir,
        args.model.scale,
        args.model.device,
        args.model.device_options(),
    )?;

    info!(
        input = %args.input.display(),
        output = %args.output.display(),
        tiling = %args.tiling.describe(),
        "starting conversion"
    );
    convert::convert(&args.input, &args.output, &mut models, tiling)?;
    info!(total_s = started.elapsed().as_secs(), "conversion complete");
    Ok(())
}
