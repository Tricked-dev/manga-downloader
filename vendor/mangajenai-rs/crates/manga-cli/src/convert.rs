//! The `convert` subcommand: single images, directories, ZIP and CBZ.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use image::RgbImage;
use manga_core::archive::{Processed, convert_archive, is_image_name};
use manga_core::{Device, DeviceOptions, ModelManifest, TileConfig, UpscaleModel};
use tracing::{info, warn};

/// Archive extensions we treat as containers of pages.
const ARCHIVE_EXTENSIONS: &[&str] = &["cbz", "zip"];

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            let lowered = extension.to_ascii_lowercase();
            extensions.contains(&lowered.as_str())
        })
        .unwrap_or(false)
}

/// How the model for a given page is chosen, and the loaded models so far.
///
/// Pages within one volume are usually the same height, but not always (a
/// double-page spread or a bonus illustration can differ), so selection is per
/// page. Loading a 70 MB graph per page would dominate the runtime, hence the
/// cache.
pub struct ModelCache {
    explicit: Option<PathBuf>,
    manifest: Option<ModelManifest>,
    target_scale: u32,
    device: Device,
    device_options: DeviceOptions,
    loaded: HashMap<PathBuf, UpscaleModel>,
}

impl ModelCache {
    pub fn new(
        model: &str,
        models_dir: &Path,
        target_scale: u32,
        device: Device,
        device_options: DeviceOptions,
    ) -> Result<Self> {
        if model != "auto" {
            return Ok(Self {
                explicit: Some(PathBuf::from(model)),
                manifest: None,
                target_scale,
                device,
                device_options,
                loaded: HashMap::new(),
            });
        }

        let manifest_path = models_dir.join("models.json");
        let manifest = ModelManifest::load(&manifest_path).with_context(|| {
            format!(
                "--model auto needs a manifest at {}. Generate one with \
                 tools/make_manifest.py after exporting models.",
                manifest_path.display()
            )
        })?;
        Ok(Self {
            explicit: None,
            manifest: Some(manifest),
            target_scale,
            device,
            device_options,
            loaded: HashMap::new(),
        })
    }

    fn path_for(&self, page_height: u32) -> Result<PathBuf> {
        if let Some(path) = &self.explicit {
            return Ok(path.clone());
        }
        let manifest = self.manifest.as_ref().expect("auto mode has a manifest");
        let entry = manifest.select(page_height, self.target_scale)?;
        Ok(manifest.resolve(entry))
    }

    /// Get (loading if necessary) the model for a page of this height.
    pub fn get(&mut self, page_height: u32) -> Result<&mut UpscaleModel> {
        let path = self.path_for(page_height)?;
        if !self.loaded.contains_key(&path) {
            let started = Instant::now();
            let model = UpscaleModel::open_with(&path, self.device, &self.device_options)?;
            info!(
                model = model.properties().name.as_deref().unwrap_or("unknown"),
                scale = model.scale(),
                device = %model.device(),
                load_ms = started.elapsed().as_millis() as u64,
                "loaded model"
            );
            self.loaded.insert(path.clone(), model);
        }
        Ok(self.loaded.get_mut(&path).expect("just inserted"))
    }
}

fn encode_png(image: &RgbImage) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .context("failed to encode PNG")?;
    Ok(bytes)
}

/// Upscale one already-decoded page.
fn upscale_page(
    models: &mut ModelCache,
    source: &RgbImage,
    tiling: Option<TileConfig>,
) -> Result<RgbImage> {
    let (_, height) = source.dimensions();
    let model = models.get(height)?;
    match tiling {
        Some(config) => manga_core::upscale_tiled(model, source, config),
        None => manga_core::upscale_image(model, source),
    }
}

/// Rewrite a page name so its extension matches what we encoded.
fn with_png_extension(name: &str) -> String {
    Path::new(name)
        .with_extension("png")
        .to_string_lossy()
        .into_owned()
}

pub fn convert(
    input: &Path,
    output: &Path,
    models: &mut ModelCache,
    tiling: Option<TileConfig>,
) -> Result<()> {
    if input.is_dir() {
        return convert_directory(input, output, models, tiling);
    }
    if has_extension(input, ARCHIVE_EXTENSIONS) {
        return convert_archive_file(input, output, models, tiling);
    }
    if is_image_name(&input.to_string_lossy()) {
        let source = manga_core::image::load_rgb8(input)?;
        let upscaled = upscale_page(models, &source, tiling)?;
        manga_core::image::save_rgb8(output, &upscaled)?;
        return Ok(());
    }
    bail!(
        "{} is not an image, a directory, or a .cbz/.zip archive",
        input.display()
    )
}

fn convert_archive_file(
    input: &Path,
    output: &Path,
    models: &mut ModelCache,
    tiling: Option<TileConfig>,
) -> Result<()> {
    let started = Instant::now();

    let reader = BufReader::new(
        File::open(input).with_context(|| format!("failed to open {}", input.display()))?,
    );
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let writer = BufWriter::new(
        File::create(output).with_context(|| format!("failed to create {}", output.display()))?,
    );

    let mut page_number = 0usize;
    let stats = convert_archive(reader, writer, |name, data| {
        if !is_image_name(name) {
            return Ok(Processed::Passthrough);
        }
        page_number += 1;
        let page_started = Instant::now();

        let source = image::load_from_memory(data)
            .with_context(|| format!("failed to decode `{name}`"))?
            .to_rgb8();
        let (width, height) = source.dimensions();

        let upscaled = upscale_page(models, &source, tiling)
            .with_context(|| format!("failed to upscale `{name}`"))?;
        let (out_width, out_height) = upscaled.dimensions();

        // Encoded once, here, and the bytes go straight into the archive.
        let encoded = encode_png(&upscaled)?;

        info!(
            page = page_number,
            name,
            input = %format!("{width}x{height}"),
            output = %format!("{out_width}x{out_height}"),
            ms = page_started.elapsed().as_millis() as u64,
            "page done"
        );

        Ok(Processed::Replaced {
            name: with_png_extension(name),
            data: encoded,
        })
    })?;

    info!(
        pages = stats.processed,
        passed_through = stats.passed_through,
        total_s = started.elapsed().as_secs(),
        "wrote {}",
        output.display()
    );
    Ok(())
}

fn convert_directory(
    input: &Path,
    output: &Path,
    models: &mut ModelCache,
    tiling: Option<TileConfig>,
) -> Result<()> {
    let mut pages = Vec::new();
    collect_files(input, &mut pages)?;
    // Archive order does not exist for a directory, so use a stable sort by
    // path -- which is also the order a reader would present them in.
    pages.sort();

    let mut processed = 0usize;
    let mut copied = 0usize;

    for path in &pages {
        let relative = path
            .strip_prefix(input)
            .expect("collected under the input root");
        let destination = output.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if !is_image_name(&path.to_string_lossy()) {
            std::fs::copy(path, &destination)
                .with_context(|| format!("failed to copy {}", path.display()))?;
            copied += 1;
            continue;
        }

        let source = manga_core::image::load_rgb8(path)?;
        let upscaled = upscale_page(models, &source, tiling)?;
        let destination = destination.with_extension("png");
        manga_core::image::save_rgb8(&destination, &upscaled)?;
        processed += 1;
        info!(page = processed, "wrote {}", destination.display());
    }

    if processed == 0 {
        warn!("no images found under {}", input.display());
    }
    info!(pages = processed, copied, "wrote {}", output.display());
    Ok(())
}

fn collect_files(directory: &Path, into: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("failed to list {}", directory.display()))?
    {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, into)?;
        } else {
            into.push(path);
        }
    }
    Ok(())
}
