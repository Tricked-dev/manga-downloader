//! Dedicated inference worker with explicit devices and bounded CPU parallelism.
//! Model selection/cache follows the vendored CLI's convert::ModelCache.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Mutex,
    thread::JoinHandle,
    time::{Instant, SystemTime},
};

use anyhow::{Context, Result, bail, ensure};
use manga_core::{Device, DeviceOptions, ModelManifest, TileConfig, UpscaleModel};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

/// Explicit devices only. Accelerator requests never silently fall back to CPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UpscaleDevice {
    Cpu,
    MiGraphX,
    Cuda,
    OpenVino,
    CoreMl,
}

impl Default for UpscaleDevice {
    fn default() -> Self {
        if cfg!(target_os = "macos") {
            Self::CoreMl
        } else {
            Self::Cpu
        }
    }
}
impl FromStr for UpscaleDevice {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "cpu" => Ok(Self::Cpu),
            "migraphx" => Ok(Self::MiGraphX),
            "cuda" => Ok(Self::Cuda),
            "openvino" => Ok(Self::OpenVino),
            "coreml" => Ok(Self::CoreMl),
            _ => bail!(
                "choose an explicit device: cpu, migraphx, cuda, openvino, or coreml; auto is disabled"
            ),
        }
    }
}
impl UpscaleDevice {
    fn device(self) -> Device {
        match self {
            Self::Cpu => Device::Cpu,
            Self::MiGraphX => Device::MiGraphX,
            Self::Cuda => Device::Cuda,
            Self::OpenVino => Device::OpenVino,
            Self::CoreMl => Device::CoreMl,
        }
    }
}

#[derive(Clone, Debug)]
pub struct UpscaleConfig {
    pub models_dir: PathBuf,
    pub device: UpscaleDevice,
    /// OpenVINO's `device_type`. `GPU` keeps a missing accelerator an error rather
    /// than a silent CPU run; `CPU` selects OpenVINO's own CPU plugin deliberately.
    pub openvino_device: String,
    /// Threads for OpenVINO's own pool, or 0 to let it size itself. It sizes from the
    /// visible cores and cannot see a cgroup quota, so it undershoots a generous one.
    pub openvino_threads: usize,
    /// OpenVINO inference precision. `FP32` keeps output identical to the CPU
    /// provider rather than letting the plugin downcast for speed.
    pub openvino_precision: String,
    /// Intra-op threads for the ONNX Runtime CPU provider, which is otherwise the
    /// binding constraint on that path regardless of the cgroup CPU quota. Accelerator
    /// providers manage their own pools and ignore this.
    pub cpu_threads: usize,
    pub tile_size: u32,
    pub overlap: u32,
    /// Optional ONNX execution profiles for a diagnostic run.
    pub profile_dir: Option<PathBuf>,
}
impl Default for UpscaleConfig {
    fn default() -> Self {
        Self {
            models_dir: "./data/models".into(),
            device: UpscaleDevice::default(),
            openvino_device: "GPU".into(),
            openvino_threads: 0,
            openvino_precision: "FP32".into(),
            cpu_threads: 2,
            tile_size: 256,
            overlap: 32,
            profile_dir: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UpscaledPage {
    #[serde(skip)]
    pub bytes: Vec<u8>,
    pub model: String,
    pub model_path: PathBuf,
    pub scale: u32,
    pub tile_size: u32,
    pub width: u32,
    pub height: u32,
    pub device: UpscaleDevice,
}
#[derive(Debug)]
pub enum UpscaleOutcome {
    Complete(UpscaledPage),
    /// Terminal for this job. Installing models permits a later explicit retry.
    MissingModels {
        reason: String,
    },
}

enum Request {
    Page {
        bytes: Vec<u8>,
        scale: u32,
        response: oneshot::Sender<Result<UpscaleOutcome>>,
    },
    Shutdown,
}

pub struct Upscaler {
    requests: mpsc::Sender<Request>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl Upscaler {
    pub fn start(config: UpscaleConfig) -> Result<Self> {
        TileConfig {
            tile_size: config.tile_size,
            overlap: config.overlap,
        }
        .validate()?;
        // One inference at a time and at most one queued page, independent of
        // Tokio worker count. ONNX sessions never leave this OS thread.
        let (requests, mut receiver) = mpsc::channel(1);
        let worker = std::thread::Builder::new().name("manga-upscale".into()).spawn(move || {
            let mut models = ModelCache::new(config);
            match models.load_manifest() {
                Ok(Some(_)) => {},
                Ok(None) => tracing::warn!(models_dir = %models.config.models_dir.display(), "Upscale models absent; original pages remain available"),
                Err(error) => tracing::warn!(%error, "Upscale model manifest is unusable; original pages remain available"),
            }
            while let Some(request) = receiver.blocking_recv() {
                match request {
                    Request::Page { bytes, scale, response } => {
                        if !response.is_closed() { let _ = response.send(models.upscale(&bytes, scale)); }
                    }
                    Request::Shutdown => break,
                }
            }
            models.finish_profiles();
        }).context("start inference worker")?;
        Ok(Self {
            requests,
            worker: Mutex::new(Some(worker)),
        })
    }

    pub async fn upscale(&self, bytes: Vec<u8>, scale: u32) -> Result<UpscaleOutcome> {
        ensure!(scale == 2 || scale == 4, "upscale scale must be 2 or 4");
        let (response, result) = oneshot::channel();
        self.requests
            .send(Request::Page {
                bytes,
                scale,
                response,
            })
            .await
            .context("upscale worker stopped")?;
        result
            .await
            .context("upscale worker exited before returning the page")?
    }

    pub async fn shutdown(&self) -> Result<()> {
        let worker = self
            .worker
            .lock()
            .map_err(|_| anyhow::anyhow!("upscale worker lock poisoned"))?
            .take();
        if let Some(worker) = worker {
            let _ = self.requests.send(Request::Shutdown).await;
            tokio::task::spawn_blocking(move || {
                worker
                    .join()
                    .map_err(|_| anyhow::anyhow!("upscale worker panicked"))
            })
            .await??;
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    length: u64,
    modified: SystemTime,
}
fn stamp(path: &Path) -> Result<Option<FileStamp>> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(Some(FileStamp {
            length: metadata.len(),
            modified: metadata.modified()?,
        })),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
struct CachedModel {
    stamp: FileStamp,
    model: UpscaleModel,
    used: u64,
}
struct ModelCache {
    config: UpscaleConfig,
    manifest: Option<(FileStamp, ModelManifest)>,
    loaded: HashMap<PathBuf, CachedModel>,
    tick: u64,
}
impl ModelCache {
    fn new(config: UpscaleConfig) -> Self {
        Self {
            config,
            manifest: None,
            loaded: HashMap::new(),
            tick: 0,
        }
    }
    fn load_manifest(&mut self) -> Result<Option<&ModelManifest>> {
        let path = self.config.models_dir.join("models.json");
        let Some(current) = stamp(&path)? else {
            self.manifest = None;
            return Ok(None);
        };
        if self
            .manifest
            .as_ref()
            .is_none_or(|(stamp, _)| stamp != &current)
        {
            self.manifest = Some((current, ModelManifest::load(&path)?));
        }
        Ok(self.manifest.as_ref().map(|(_, manifest)| manifest))
    }
    fn upscale(&mut self, bytes: &[u8], scale: u32) -> Result<UpscaleOutcome> {
        let Some(manifest) = self.load_manifest()? else {
            return Ok(UpscaleOutcome::MissingModels {
                reason: "models.json is absent".into(),
            });
        };
        let source = backend_image::decode_image(bytes)
            .context("decode original page for upscaling")?
            .to_rgb8();
        let entry = match manifest.select(source.height(), scale) {
            Ok(entry) => entry,
            Err(error) => {
                return Ok(UpscaleOutcome::MissingModels {
                    reason: error.to_string(),
                });
            }
        };
        let path = manifest.resolve(entry);
        let name = entry.name.clone();
        let Some(current) = stamp(&path)? else {
            return Ok(UpscaleOutcome::MissingModels {
                reason: format!("model file is absent: {}", path.display()),
            });
        };
        self.tick += 1;
        if self
            .loaded
            .get(&path)
            .is_some_and(|cached| cached.stamp != current)
        {
            self.finish_profile(&path);
            self.loaded.remove(&path);
        }
        if !self.loaded.contains_key(&path) {
            // Limit resident graphs without losing the upstream path-keyed cache.
            if self.loaded.len() >= 2
                && let Some(oldest) = self
                    .loaded
                    .iter()
                    .min_by_key(|(_, value)| value.used)
                    .map(|(path, _)| path.clone())
            {
                self.finish_profile(&oldest);
                self.loaded.remove(&oldest);
            }
            let device = self.config.device.device();
            ensure!(
                device.is_available()?,
                "requested provider {device} is unavailable; CPU fallback is disabled"
            );
            let options = DeviceOptions {
                openvino_device_type: Some(self.config.openvino_device.clone()),
                openvino_num_threads: (self.config.openvino_threads > 0)
                    .then_some(self.config.openvino_threads),
                openvino_precision: Some(self.config.openvino_precision.clone()),
            };
            let profile = if let Some(directory) = &self.config.profile_dir {
                std::fs::create_dir_all(directory)?;
                Some(directory.join(format!("upscale-{}", self.tick)))
            } else {
                None
            };
            let started = Instant::now();
            tracing::info!(model = %name, %device, "Loading upscale model");
            let model = if device == Device::Cpu {
                UpscaleModel::open_cpu(&path, self.config.cpu_threads.clamp(1, 8), profile.as_deref())?
            } else {
                UpscaleModel::open_gpu(&path, device, &options, profile.as_deref())?
            };
            ensure!(
                model.device() == device,
                "requested provider was not retained"
            );
            ensure!(
                model.scale() == scale,
                "model scale disagrees with manifest selection"
            );
            tracing::info!(model = %name, %device, load_ms = started.elapsed().as_millis(), "Upscale model loaded on the requested device");
            self.loaded.insert(
                path.clone(),
                CachedModel {
                    model,
                    stamp: current,
                    used: self.tick,
                },
            );
        }
        let cached = self
            .loaded
            .get_mut(&path)
            .context("selected model missing from cache")?;
        cached.used = self.tick;
        let tile = TileConfig {
            tile_size: self.config.tile_size,
            overlap: self.config.overlap,
        };
        tracing::debug!(model = %name, width = source.width(), height = source.height(), "Starting tiled inference");
        let upscaled = manga_core::upscale_tiled(&mut cached.model, &source, tile)?;
        ensure!(
            upscaled.width()
                == source
                    .width()
                    .checked_mul(scale)
                    .context("upscale width overflow")?
                && upscaled.height()
                    == source
                        .height()
                        .checked_mul(scale)
                        .context("upscale height overflow")?,
            "upscaled dimensions do not match model scale"
        );
        Ok(UpscaleOutcome::Complete(UpscaledPage {
            bytes: backend_image::encode_lossless_avif_rgb(&upscaled)?,
            model: name,
            model_path: path,
            scale,
            tile_size: tile.tile_size,
            width: upscaled.width(),
            height: upscaled.height(),
            device: self.config.device,
        }))
    }
    fn finish_profile(&mut self, path: &Path) {
        if self.config.profile_dir.is_some()
            && let Some(cached) = self.loaded.get_mut(path)
        {
            match cached.model.end_profiling() {
                Ok(path) => tracing::info!(%path, "Execution profile saved"),
                Err(error) => tracing::warn!(%error, "Execution profile could not be saved"),
            }
        }
    }
    fn finish_profiles(&mut self) {
        let paths: Vec<_> = self.loaded.keys().cloned().collect();
        for path in paths {
            self.finish_profile(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn devices_are_explicit_and_gpu_entry_point_rejects_cpu() {
        assert_eq!("cpu".parse::<UpscaleDevice>().unwrap(), UpscaleDevice::Cpu);
        for value in ["auto", "AUTO", ""] {
            assert!(value.parse::<UpscaleDevice>().is_err());
        }
        assert_eq!(
            "migraphx".parse::<UpscaleDevice>().unwrap(),
            UpscaleDevice::MiGraphX
        );
        assert!(
            UpscaleModel::open_gpu(
                Path::new("absent.onnx"),
                Device::Cpu,
                &DeviceOptions::default(),
                None
            )
            .is_err()
        );
        assert!(
            UpscaleModel::open_gpu(
                Path::new("absent.onnx"),
                Device::Auto,
                &DeviceOptions::default(),
                None
            )
            .is_err()
        );
    }
    #[tokio::test]
    async fn missing_models_are_terminal_and_worker_shuts_down() {
        let root = tempfile::tempdir().unwrap();
        let worker = Upscaler::start(UpscaleConfig {
            models_dir: root.path().join("absent"),
            ..Default::default()
        })
        .unwrap();
        assert!(matches!(
            worker.upscale(vec![], 2).await.unwrap(),
            UpscaleOutcome::MissingModels { .. }
        ));
        worker.shutdown().await.unwrap();
        assert!(worker.upscale(vec![], 2).await.is_err());
    }

    #[tokio::test]
    async fn manifest_without_a_matching_band_is_a_terminal_skip() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("models.json"),
            r#"{"version":1,"models":[]}"#,
        )
        .unwrap();
        let mut input = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(4, 4)
            .write_to(&mut input, image::ImageFormat::Png)
            .unwrap();
        let worker = Upscaler::start(UpscaleConfig {
            models_dir: root.path().to_path_buf(),
            ..Default::default()
        })
        .unwrap();
        let outcome = worker.upscale(input.into_inner(), 2).await;
        worker.shutdown().await.unwrap();
        assert!(matches!(
            outcome.unwrap(),
            UpscaleOutcome::MissingModels { .. }
        ));
    }
}
