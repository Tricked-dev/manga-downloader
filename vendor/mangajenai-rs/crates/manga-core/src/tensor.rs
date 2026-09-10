//! Conversions between packed 8-bit image buffers and the NCHW float tensors
//! the models consume, plus the quantisation rule used on the way back.
//!
//! The conventions here are not free choices; they mirror what Spandrel does,
//! as recorded in `docs/REFERENCE-SEMANTICS.md`.

use ndarray::{Array4, ArrayView4};

/// Divisor taking 8-bit samples into the `[0, 1]` range the models expect.
pub const U8_SCALE: f32 = 255.0;

/// Quantise a model output sample back to 8 bits.
///
/// Clamp first, then round half away from zero. The reference does this as
/// `floor(clamp(x, 0, 1) * 255 + 0.5)`; `f32::round` agrees for non-negative
/// inputs, but the explicit form is kept so the correspondence to the Python
/// side is obvious rather than something to re-derive.
#[inline]
pub fn quantize_u8(value: f32) -> u8 {
    ((value.clamp(0.0, 1.0) * U8_SCALE) + 0.5).floor() as u8
}

/// Clamp a tensor to `[0, 1]` in place.
///
/// Spandrel's `ImageModelDescriptor.__call__` does this before returning, and
/// it is load-bearing: ESRGAN outputs routinely leave the range (excursions of
/// over 0.5 are measured by `tools/verify_onnx.py`).
pub fn clamp_to_unit_interval(tensor: &mut Array4<f32>) {
    tensor.mapv_inplace(|value| value.clamp(0.0, 1.0));
}

/// Convert an interleaved (HWC) 8-bit buffer into a planar NCHW float tensor.
///
/// `raw` must hold exactly `height * width * channels` samples.
pub fn hwc_u8_to_nchw(raw: &[u8], width: usize, height: usize, channels: usize) -> Array4<f32> {
    let pixels = width * height;
    assert_eq!(
        raw.len(),
        pixels * channels,
        "buffer is {} samples, expected {}x{}x{}",
        raw.len(),
        height,
        width,
        channels
    );

    let mut data = vec![0.0f32; pixels * channels];
    for channel in 0..channels {
        let plane = &mut data[channel * pixels..(channel + 1) * pixels];
        for (index, sample) in plane.iter_mut().enumerate() {
            *sample = f32::from(raw[index * channels + channel]) / U8_SCALE;
        }
    }

    Array4::from_shape_vec((1, channels, height, width), data)
        .expect("shape matches the allocated length")
}

/// Convert a planar NCHW float tensor back into an interleaved 8-bit buffer.
///
/// Values are clamped and quantised per [`quantize_u8`]. The batch dimension
/// must be 1.
pub fn nchw_to_hwc_u8(tensor: ArrayView4<f32>) -> Vec<u8> {
    let (batch, channels, height, width) = tensor.dim();
    assert_eq!(batch, 1, "expected a single-image batch, got {batch}");

    let pixels = width * height;
    let mut out = vec![0u8; pixels * channels];

    // Walking the source in memory order keeps this a linear read even though
    // the destination is strided.
    for channel in 0..channels {
        let plane = tensor.index_axis(ndarray::Axis(0), 0);
        let plane = plane.index_axis(ndarray::Axis(0), channel);
        for (index, value) in plane.iter().enumerate() {
            out[index * channels + channel] = quantize_u8(*value);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantisation_saturates_out_of_range_values() {
        assert_eq!(quantize_u8(-1.0), 0);
        assert_eq!(quantize_u8(0.0), 0);
        assert_eq!(quantize_u8(1.0), 255);
        assert_eq!(quantize_u8(2.0), 255);
    }

    #[test]
    fn quantisation_rounds_half_away_from_zero() {
        // 0.5/255 scales to exactly 0.5, which banker's rounding would send to
        // 0 and the reference sends to 1.
        assert_eq!(quantize_u8(0.5 / U8_SCALE), 1);
        assert_eq!(quantize_u8(1.5 / U8_SCALE), 2);
    }

    #[test]
    fn every_8bit_value_survives_a_round_trip() {
        let raw: Vec<u8> = (0..=255).collect();
        let tensor = hwc_u8_to_nchw(&raw, 256, 1, 1);
        assert_eq!(nchw_to_hwc_u8(tensor.view()), raw);
    }

    #[test]
    fn interleaved_channels_land_in_the_right_planes() {
        // One 2x1 RGB row: red then green.
        let raw = [255u8, 0, 0, 0, 255, 0];
        let tensor = hwc_u8_to_nchw(&raw, 2, 1, 3);
        assert_eq!(tensor.dim(), (1, 3, 1, 2));
        assert_eq!(tensor[[0, 0, 0, 0]], 1.0); // red channel, first pixel
        assert_eq!(tensor[[0, 1, 0, 0]], 0.0);
        assert_eq!(tensor[[0, 1, 0, 1]], 1.0); // green channel, second pixel
        assert_eq!(nchw_to_hwc_u8(tensor.view()), raw);
    }

    #[test]
    fn clamping_matches_the_reference_range() {
        let mut tensor = Array4::from_shape_vec((1, 1, 1, 3), vec![-0.5, 0.25, 1.7]).unwrap();
        clamp_to_unit_interval(&mut tensor);
        assert_eq!(tensor.as_slice().unwrap(), &[0.0, 0.25, 1.0]);
    }
}
