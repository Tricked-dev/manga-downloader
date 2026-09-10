use anyhow::{Result, ensure};
use image::{ExtendedColorType, ImageEncoder};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Avif,
    Webp,
    Jpeg,
}
impl OutputFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Avif => "avif",
            Self::Webp => "webp",
            Self::Jpeg => "jpeg",
        }
    }
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Avif => "image/avif",
            Self::Webp => "image/webp",
            Self::Jpeg => "image/jpeg",
        }
    }
}

/// Re-encode at native dimensions unless the caller explicitly requests a width.
/// AVIF and WebP preserve pixels losslessly after any requested resize.
pub fn transform(input: &[u8], format: OutputFormat, width: Option<u32>) -> Result<Vec<u8>> {
    ensure!(width != Some(0), "width must be greater than zero");
    let mut decoded = crate::decode_image(input)?;
    if let Some(width) = width.filter(|width| *width < decoded.width()) {
        let height = (u64::from(decoded.height()) * u64::from(width) / u64::from(decoded.width()))
            .max(1) as u32;
        decoded = decoded.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
    }
    if format == OutputFormat::Avif {
        return if decoded.color().has_alpha() {
            crate::encode_lossless_avif_rgba(&decoded.to_rgba8())
        } else {
            crate::encode_lossless_avif_rgb(&decoded.to_rgb8())
        };
    }
    let mut output = Vec::new();
    match format {
        OutputFormat::Webp => {
            let pixels = decoded.to_rgba8();
            image::codecs::webp::WebPEncoder::new_lossless(&mut output).write_image(
                pixels.as_raw(),
                pixels.width(),
                pixels.height(),
                ExtendedColorType::Rgba8,
            )?;
        }
        OutputFormat::Jpeg => {
            let pixels = decoded.to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 95).write_image(
                pixels.as_raw(),
                pixels.width(),
                pixels.height(),
                ExtendedColorType::Rgb8,
            )?;
        }
        OutputFormat::Avif => unreachable!(),
    }
    Ok(output)
}
