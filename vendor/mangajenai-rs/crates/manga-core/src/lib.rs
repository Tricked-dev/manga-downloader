//! Core pipeline for running MangaJaNai super-resolution models through ONNX
//! Runtime.
//!
//! The scope here is deliberately narrow: this crate does *not* reimplement any
//! neural network architecture. Model graphs are produced once, ahead of time,
//! by `tools/export_onnx.py` (PyTorch + Spandrel) and are consumed here purely
//! as ONNX files. Everything in this crate is the surrounding pipeline --
//! decoding, normalisation, padding, inference, quantisation and encoding --
//! which must reproduce the reference MangaJaNaiConverter/Spandrel behaviour
//! exactly. Those reference semantics are written down in
//! `docs/REFERENCE-SEMANTICS.md`.

pub mod archive;
pub mod blending;
pub mod device;
pub mod image;
pub mod manifest;
pub mod model;
pub mod padding;
pub mod tensor;
pub mod text;
pub mod tiling;

pub use device::{Device, DeviceOptions};
pub use manifest::{ModelEntry, ModelManifest};
pub use model::{ModelProperties, UpscaleModel};
pub use padding::SizeRequirements;
pub use tiling::{TileConfig, upscale_tiled};

/// Upscale a decoded image end to end.
///
/// This is the whole-image path: the entire page goes through the model as one
/// tensor. Large pages should use the tiled path instead once it exists.
pub fn upscale_image(
    model: &mut UpscaleModel,
    source: &::image::RgbImage,
) -> anyhow::Result<::image::RgbImage> {
    let tensor = image::rgb8_to_tensor(source);
    let output = model.upscale_tensor(tensor.view())?;
    image::tensor_to_rgb8(output.view())
}
