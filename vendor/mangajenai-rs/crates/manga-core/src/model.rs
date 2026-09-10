//! Loading MangaJaNai ONNX graphs and running them through ONNX Runtime.
//!
//! No architecture is reimplemented here. The graph is produced once by
//! `tools/export_onnx.py`, which also writes the model's properties into the
//! ONNX `metadata_props`, so a `.onnx` file carries its own scale factor,
//! channel count and size requirements rather than depending on a sidecar that
//! could be separated from it.

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use ndarray::{Array4, ArrayView4, Ix4};
use ort::session::{Session, builder::SessionBuilder};
use ort::value::TensorRef;

use crate::device::{self, Device, DeviceOptions};
use crate::padding::{SizeRequirements, pad_nchw};
use crate::tensor::clamp_to_unit_interval;

/// Metadata key prefix written by `tools/export_onnx.py`.
const METADATA_PREFIX: &str = "mangajanai";

/// Everything the pipeline needs to know about a model, read from the graph.
#[derive(Debug, Clone)]
pub struct ModelProperties {
    pub name: Option<String>,
    pub architecture: Option<String>,
    /// Upscaling factor, e.g. 4 for a 4x model.
    pub scale: u32,
    pub input_channels: usize,
    pub output_channels: usize,
    pub size_requirements: SizeRequirements,
    /// Source page height this model was trained for, if the name encoded one.
    pub target_height: Option<u32>,
}

/// A loaded model, ready to run.
pub struct UpscaleModel {
    session: Session,
    properties: ModelProperties,
    input_name: String,
    output_name: String,
    device: Device,
}

fn parse_metadata<T: std::str::FromStr>(
    metadata: &ort::session::ModelMetadata<'_>,
    key: &str,
) -> Result<Option<T>> {
    let full_key = format!("{METADATA_PREFIX}.{key}");
    match metadata.custom(&full_key) {
        None => Ok(None),
        Some(raw) => raw
            .parse::<T>()
            .map(Some)
            .map_err(|_| anyhow!("model metadata `{full_key}` is not parseable: {raw:?}")),
    }
}

impl UpscaleModel {
    /// Load a model exported by `tools/export_onnx.py`, on the default device.
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_on(path, Device::default())
    }

    /// Load a model, running it on a specific execution provider.
    ///
    /// An explicitly requested accelerator that cannot be used is an error;
    /// only [`Device::Auto`] falls back to CPU. See [`crate::device`].
    pub fn open_on(path: &Path, device: Device) -> Result<Self> {
        Self::open_with(path, device, &DeviceOptions::default())
    }

    /// Load a model with backend-specific options.
    pub fn open_with(path: &Path, device: Device, options: &DeviceOptions) -> Result<Self> {
        let mut builder =
            Session::builder().context("failed to create an ONNX Runtime session builder")?;
        let device = device::configure(&mut builder, device, options)?;

        Self::from_builder(path, builder, device)
    }

    /// Explicit CPU execution with bounded parallelism and no busy-wait spinning.
    pub fn open_cpu(path: &Path, threads: usize, profile: Option<&Path>) -> Result<Self> {
        if threads == 0 || threads > 8 {
            bail!("CPU thread count must be between 1 and 8");
        }
        let option = |result: ort::session::builder::BuilderResult| {
            result.map_err(|error| anyhow!("ONNX session option failed: {error}"))
        };
        let mut builder = Session::builder()?;
        builder = option(builder.with_no_environment_execution_providers())?;
        builder = option(builder.with_intra_threads(threads))?;
        builder = option(builder.with_inter_threads(1))?;
        builder = option(builder.with_intra_op_spinning(false))?;
        builder = option(builder.with_inter_op_spinning(false))?;
        if let Some(profile) = profile {
            builder = option(builder.with_profiling(profile))?;
        }
        device::configure(&mut builder, Device::Cpu, &DeviceOptions::default())?;
        Self::from_builder(path, builder, Device::Cpu)
    }

    /// Load on an explicit GPU with no CPU execution-provider fallback.
    /// Unsupported operators fail during session creation, before inference.
    pub fn open_gpu(
        path: &Path,
        device: Device,
        options: &DeviceOptions,
        profile: Option<&Path>,
    ) -> Result<Self> {
        if !device.is_accelerator() {
            bail!("GPU inference requires an explicit accelerator; CPU and auto are forbidden");
        }
        if device == Device::OpenVino
            && !options
                .openvino_device_type
                .as_deref()
                .is_some_and(|value| {
                    value == "GPU"
                        || value
                            .strip_prefix("GPU.")
                            .is_some_and(|id| id.parse::<u32>().is_ok())
                })
        {
            bail!("GPU inference requires OpenVINO GPU or GPU.N, with no CPU or AUTO target");
        }
        // Builder errors own a non-Send recovery value. Discard that value
        // while retaining the error text before crossing our anyhow boundary.
        let option = |result: ort::session::builder::BuilderResult| {
            result.map_err(|error| anyhow!("ONNX session option failed: {error}"))
        };
        let mut builder = Session::builder()?;
        builder = option(builder.with_no_environment_execution_providers())?;
        builder = option(builder.with_disable_cpu_fallback())?;
        builder = option(builder.with_intra_threads(1))?;
        builder = option(builder.with_inter_threads(1))?;
        builder = option(builder.with_intra_op_spinning(false))?;
        builder = option(builder.with_inter_op_spinning(false))?;
        if let Some(profile) = profile {
            builder = option(builder.with_profiling(profile))?;
        }
        let device = device::configure(&mut builder, device, options)?;
        Self::from_builder(path, builder, device)
    }

    pub fn end_profiling(&mut self) -> Result<String> {
        Ok(self.session.end_profiling()?)
    }

    fn from_builder(path: &Path, mut builder: SessionBuilder, device: Device) -> Result<Self> {
        let session = builder
            .commit_from_file(path)
            .with_context(|| format!("failed to load model {}", path.display()))?;

        let inputs = session.inputs();
        let outputs = session.outputs();
        if inputs.len() != 1 || outputs.len() != 1 {
            bail!(
                "expected a single-input single-output graph, got {} input(s) and {} output(s)",
                inputs.len(),
                outputs.len()
            );
        }
        let input_name = inputs[0].name().to_owned();
        let output_name = outputs[0].name().to_owned();

        let properties = {
            let metadata = session
                .metadata()
                .context("failed to read ONNX model metadata")?;

            let scale: u32 = parse_metadata(&metadata, "scale")?.ok_or_else(|| {
                anyhow!(
                    "{} has no `{METADATA_PREFIX}.scale` metadata. Re-export it with \
                     tools/export_onnx.py, which records the scale factor in the graph.",
                    path.display()
                )
            })?;
            if scale == 0 {
                bail!("model reports a scale of 0");
            }

            let input_channels: usize =
                parse_metadata(&metadata, "input_channels")?.ok_or_else(|| {
                    anyhow!(
                        "{} has no `{METADATA_PREFIX}.input_channels` metadata; re-export it",
                        path.display()
                    )
                })?;
            let output_channels: usize =
                parse_metadata(&metadata, "output_channels")?.unwrap_or(input_channels);

            let size_requirements = SizeRequirements::new(
                parse_metadata(&metadata, "size_minimum")?.unwrap_or(0),
                parse_metadata(&metadata, "size_multiple_of")?.unwrap_or(1),
                parse_metadata::<u8>(&metadata, "size_square")?.unwrap_or(0) != 0,
            );

            // A recorded target height of 0 means "the filename did not encode
            // one", which is different from a real height.
            let target_height =
                parse_metadata::<u32>(&metadata, "target_height")?.filter(|height| *height > 0);

            ModelProperties {
                name: metadata.custom(&format!("{METADATA_PREFIX}.name")),
                architecture: metadata.custom(&format!("{METADATA_PREFIX}.architecture")),
                scale,
                input_channels,
                output_channels,
                size_requirements,
                target_height,
            }
        };

        Ok(Self {
            session,
            properties,
            input_name,
            output_name,
            device,
        })
    }

    /// The execution provider this session actually runs on.
    pub fn device(&self) -> Device {
        self.device
    }

    pub fn properties(&self) -> &ModelProperties {
        &self.properties
    }

    pub fn scale(&self) -> u32 {
        self.properties.scale
    }

    /// Run the graph with no pre- or postprocessing.
    ///
    /// The caller is responsible for padding and clamping; use
    /// [`UpscaleModel::upscale_tensor`] unless you are deliberately bypassing
    /// the reference contract (the tiling code does its own padding).
    pub fn infer(&mut self, input: ArrayView4<f32>) -> Result<Array4<f32>> {
        let (_, channels, height, width) = input.dim();
        if channels != self.properties.input_channels {
            bail!(
                "model expects {} input channel(s), got {channels}",
                self.properties.input_channels
            );
        }

        let value = TensorRef::from_array_view(input)
            .context("failed to wrap the input tensor for ONNX Runtime")?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => value])
            .context("ONNX Runtime inference failed")?;

        let output = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| anyhow!("model produced no output named `{}`", self.output_name))?;
        let array = output
            .try_extract_array::<f32>()
            .context("model output was not a float32 tensor")?
            .into_dimensionality::<Ix4>()
            .context("model output was not 4-dimensional")?;

        let expected = (
            1,
            self.properties.output_channels,
            height * self.properties.scale as usize,
            width * self.properties.scale as usize,
        );
        if array.dim() != expected {
            bail!(
                "model output shape {:?} does not match the declared scale of {}x \
                 (expected {expected:?})",
                array.dim(),
                self.properties.scale
            );
        }

        Ok(array.to_owned())
    }

    /// Run the model the way Spandrel's `ImageModelDescriptor.__call__` does.
    ///
    /// Pad to the size requirements, infer, clamp to `[0, 1]`, then crop the
    /// padding back off the (scaled) output. See `docs/REFERENCE-SEMANTICS.md`.
    pub fn upscale_tensor(&mut self, input: ArrayView4<f32>) -> Result<Array4<f32>> {
        let (_, _, height, width) = input.dim();
        let (padded, did_pad) = pad_nchw(input, self.properties.size_requirements);

        let mut output = self.infer(padded.view())?;
        clamp_to_unit_interval(&mut output);

        if did_pad {
            let scale = self.properties.scale as usize;
            output = output
                .slice(ndarray::s![.., .., ..height * scale, ..width * scale])
                .to_owned();
        }

        Ok(output)
    }
}
