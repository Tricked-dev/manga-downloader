use anyhow::{Context, Result, bail, ensure};
use image::{DynamicImage, RgbImage, RgbaImage};
use std::{
    ffi::{CStr, c_char},
    ptr,
};

unsafe extern "C" {
    fn manga_image_free(bytes: *mut u8);
    fn manga_avif_encode(
        pixels: *const u8,
        width: u32,
        height: u32,
        channels: u32,
        output: *mut *mut u8,
        length: *mut usize,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
    fn manga_avif_decode(
        input: *const u8,
        input_length: usize,
        width: *mut u32,
        height: *mut u32,
        output: *mut *mut u8,
        length: *mut usize,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
}
struct NativeBytes {
    bytes: *mut u8,
    len: usize,
}
impl Default for NativeBytes {
    fn default() -> Self {
        Self {
            bytes: ptr::null_mut(),
            len: 0,
        }
    }
}
impl NativeBytes {
    fn copy(&self) -> Result<Vec<u8>> {
        ensure!(
            !self.bytes.is_null() && self.len > 0 && self.len <= isize::MAX as usize,
            "invalid AVIF output buffer"
        );
        // SAFETY: the bridge returns this allocation and its exact initialized
        // length on success. It remains alive until NativeBytes::drop.
        Ok(unsafe { std::slice::from_raw_parts(self.bytes, self.len) }.to_vec())
    }
}
impl Drop for NativeBytes {
    fn drop(&mut self) {
        // SAFETY: the bridge allocated this pointer with malloc; free accepts
        // null and each successful output has exactly one NativeBytes owner.
        unsafe { manga_image_free(self.bytes) };
    }
}
fn status(result: i32, error: &[c_char]) -> Result<()> {
    if result != 0 {
        // SAFETY: the zero-initialized error buffer is written with snprintf,
        // which always terminates within the supplied nonzero capacity.
        let message = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
        bail!("AVIF codec failed: {message}");
    }
    Ok(())
}
fn encode(pixels: &[u8], width: u32, height: u32, channels: u32) -> Result<Vec<u8>> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(channels as usize))
        .context("AVIF pixel count overflow")?;
    ensure!(
        pixels.len() == expected && width > 0 && height > 0,
        "invalid AVIF pixel buffer"
    );
    let mut output = NativeBytes::default();
    let mut error = [0; 256];
    // SAFETY: the checked input has width*height*channels bytes. It is borrowed
    // only for the call; all output pointers and the error buffer are writable.
    let result = unsafe {
        manga_avif_encode(
            pixels.as_ptr(),
            width,
            height,
            channels,
            &mut output.bytes,
            &mut output.len,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    status(result, &error)?;
    output.copy()
}

/// Encode RGB pixels without chroma conversion, quantization, or resizing.
pub fn encode_lossless_avif_rgb(image: &RgbImage) -> Result<Vec<u8>> {
    encode(image.as_raw(), image.width(), image.height(), 3)
}
pub fn encode_lossless_avif_rgba(image: &RgbaImage) -> Result<Vec<u8>> {
    encode(image.as_raw(), image.width(), image.height(), 4)
}
pub fn decode_avif(bytes: &[u8]) -> Result<DynamicImage> {
    let mut output = NativeBytes::default();
    let mut width = 0;
    let mut height = 0;
    let mut error = [0; 256];
    // SAFETY: all buffers match their declared lengths and remain live for the
    // call. The bridge validates the file and checks output-size arithmetic.
    let result = unsafe {
        manga_avif_decode(
            bytes.as_ptr(),
            bytes.len(),
            &mut width,
            &mut height,
            &mut output.bytes,
            &mut output.len,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    status(result, &error)?;
    Ok(DynamicImage::ImageRgba8(
        RgbaImage::from_raw(width, height, output.copy()?)
            .context("AVIF decoded pixel count mismatch")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lossless_avif_round_trips_rgb_pixels_at_full_resolution() {
        let image = RgbImage::from_fn(1600, 17, |x, y| {
            image::Rgb([
                (x % 256) as u8,
                ((x * 37 + y * 13) % 256) as u8,
                ((x + y * 19) % 256) as u8,
            ])
        });
        let bytes = encode_lossless_avif_rgb(&image).unwrap();
        assert_eq!(
            image::guess_format(&bytes).unwrap(),
            image::ImageFormat::Avif
        );
        assert_eq!(decode_avif(&bytes).unwrap().to_rgb8(), image);
    }
    #[test]
    fn lossless_avif_preserves_alpha_and_hidden_color() {
        let image = RgbaImage::from_fn(47, 31, |x, y| {
            image::Rgba([
                (x * 5) as u8,
                (y * 7) as u8,
                ((x + y) * 3) as u8,
                ((x * 17 + y * 29) % 256) as u8,
            ])
        });
        let bytes = encode_lossless_avif_rgba(&image).unwrap();
        assert_eq!(decode_avif(&bytes).unwrap().to_rgba8(), image);
    }
    #[test]
    fn malformed_avif_returns_an_error() {
        assert!(decode_avif(b"not an AVIF").is_err());
    }
}
