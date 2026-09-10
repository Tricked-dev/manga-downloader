//! Image decoding, encoding, and the boundary between pixels and tensors.

use std::path::Path;

use anyhow::{Context, Result, bail};
use image::{ImageReader, RgbImage};
use ndarray::{Array4, ArrayView4};

use crate::tensor::{U8_SCALE, hwc_u8_to_nchw, nchw_to_hwc_u8};

/// Decode an image file as 8-bit RGB.
///
/// Greyscale sources are expanded to three identical channels, which is what
/// the reference pipeline does (`PIL.Image.convert("RGB")`) -- MangaJaNai
/// models take three channels even though manga is black and white.
pub fn load_rgb8(path: &Path) -> Result<RgbImage> {
    let reader = ImageReader::open(path)
        .with_context(|| format!("failed to open {}", path.display()))?
        .with_guessed_format()
        .with_context(|| format!("failed to detect the image format of {}", path.display()))?;
    let decoded = reader
        .decode()
        .with_context(|| format!("failed to decode {}", path.display()))?;
    Ok(decoded.to_rgb8())
}

/// Encode an image, choosing the format from the path's extension.
pub fn save_rgb8(path: &Path, image: &RgbImage) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    image
        .save(path)
        .with_context(|| format!("failed to write {}", path.display()))
}

/// Convert a decoded image into the NCHW float tensor the model consumes.
pub fn rgb8_to_tensor(image: &RgbImage) -> Array4<f32> {
    let (width, height) = image.dimensions();
    hwc_u8_to_nchw(image.as_raw(), width as usize, height as usize, 3)
}

/// Convert a rectangular region of an image straight into a model tensor.
///
/// Reads from the source buffer directly rather than materialising a cropped
/// sub-image first, so tiling costs one pass over the region instead of two.
///
/// The region must lie inside the image.
pub fn rgb8_region_to_tensor(
    image: &RgbImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Array4<f32> {
    let (image_width, image_height) = image.dimensions();
    debug_assert!(x + width <= image_width && y + height <= image_height);

    let (x, y) = (x as usize, y as usize);
    let (width, height) = (width as usize, height as usize);
    let stride = image_width as usize * 3;
    let raw = image.as_raw();

    let pixels = width * height;
    let mut data = vec![0.0f32; pixels * 3];
    for channel in 0..3 {
        let plane = &mut data[channel * pixels..(channel + 1) * pixels];
        for row in 0..height {
            let source = (y + row) * stride + x * 3;
            let destination = row * width;
            for column in 0..width {
                plane[destination + column] =
                    f32::from(raw[source + column * 3 + channel]) / U8_SCALE;
            }
        }
    }

    Array4::from_shape_vec((1, 3, height, width), data).expect("shape matches the allocated length")
}

/// Convert a model output tensor back into an 8-bit RGB image.
///
/// Values are clamped to `[0, 1]` and quantised; see
/// [`crate::tensor::quantize_u8`].
pub fn tensor_to_rgb8(tensor: ArrayView4<f32>) -> Result<RgbImage> {
    let (batch, channels, height, width) = tensor.dim();
    if batch != 1 {
        bail!("expected a single-image batch, got {batch}");
    }
    if channels != 3 {
        bail!("expected 3 output channels, got {channels}");
    }

    let raw = nchw_to_hwc_u8(tensor);
    RgbImage::from_raw(width as u32, height as u32, raw)
        .context("output buffer did not match the image dimensions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_tensor_round_trip_is_lossless() {
        let mut image = RgbImage::new(4, 3);
        for (index, pixel) in image.pixels_mut().enumerate() {
            let base = (index * 7) as u8;
            *pixel = image::Rgb([base, base.wrapping_add(50), base.wrapping_add(100)]);
        }

        let tensor = rgb8_to_tensor(&image);
        assert_eq!(tensor.dim(), (1, 3, 3, 4));

        let restored = tensor_to_rgb8(tensor.view()).unwrap();
        assert_eq!(restored.dimensions(), image.dimensions());
        assert_eq!(restored.as_raw(), image.as_raw());
    }

    #[test]
    fn a_region_tensor_matches_cropping_first() {
        let mut image = RgbImage::new(7, 5);
        for (index, pixel) in image.pixels_mut().enumerate() {
            let base = (index * 3) as u8;
            *pixel = image::Rgb([base, base.wrapping_add(11), base.wrapping_add(22)]);
        }

        let region = rgb8_region_to_tensor(&image, 2, 1, 4, 3);
        let cropped = image::imageops::crop_imm(&image, 2, 1, 4, 3).to_image();
        assert_eq!(region, rgb8_to_tensor(&cropped));
    }

    #[test]
    fn a_full_image_region_equals_the_whole_image_tensor() {
        let mut image = RgbImage::new(3, 2);
        for (index, pixel) in image.pixels_mut().enumerate() {
            let base = (index * 9) as u8;
            *pixel = image::Rgb([base, base, base]);
        }
        assert_eq!(
            rgb8_region_to_tensor(&image, 0, 0, 3, 2),
            rgb8_to_tensor(&image)
        );
    }

    #[test]
    fn tensor_to_image_rejects_wrong_channel_counts() {
        let tensor = Array4::<f32>::zeros((1, 1, 2, 2));
        assert!(tensor_to_rgb8(tensor.view()).is_err());
    }
}
