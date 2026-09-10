// This crate is only consumed inside the workspace; requiring rustdoc `# Errors`
// sections on every helper adds noise without improving call sites.
#![allow(clippy::missing_errors_doc)]

mod avif;
mod transform;
pub use transform::{OutputFormat, transform};

pub use avif::{decode_avif, encode_lossless_avif_rgb, encode_lossless_avif_rgba};

/// Decode supported source formats, including AVIF through the native decoder.
pub fn decode_image(input: &[u8]) -> anyhow::Result<image::DynamicImage> {
    if image::guess_format(input)? == image::ImageFormat::Avif {
        decode_avif(input)
    } else {
        Ok(image::load_from_memory(input)?)
    }
}

use anyhow::{Context, Result};
use image::{ExtendedColorType, ImageEncoder, ImageFormat};
use std::io::Cursor;

const COMIX_SCRAMBLE_GRID: u32 = 5;
const COMIX_DESCRAMBLE_MAP: [usize; 25] = [
    2, 13, 4, 9, 0, 14, 18, 24, 8, 6, 1, 19, 11, 21, 5, 23, 3, 20, 22, 12, 10, 17, 16, 7, 15,
];

pub fn cpu_worker_budget() -> usize {
    2
}

/// Detects whether bytes are one of the image formats this crate can decode.
pub fn detect_supported_image_format(input: &[u8]) -> Result<&'static str> {
    let format = image::guess_format(input).context("image format could not be determined")?;
    supported_image_format_name(format)
        .ok_or_else(|| anyhow::anyhow!("unsupported image format detected: {format:?}"))
}

/// Reorders a Comix 5x5 scrambled image into a PNG.
pub fn descramble_comix_5x5_to_png(input: &[u8]) -> Result<Vec<u8>> {
    descramble_comix_5x5_with_map_to_png(input, &COMIX_DESCRAMBLE_MAP)
}

/// Reorders a 5x5 tiled image using `map` and encodes the result as PNG.
pub fn descramble_comix_5x5_with_map_to_png(input: &[u8], map: &[usize; 25]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(input)
        .context("failed to decode Comix scrambled image")?
        .to_rgba8();
    let width = img.width();
    let height = img.height();
    let mut output = image::RgbaImage::new(width, height);

    for (destination_index, source_index) in map.iter().copied().enumerate() {
        let destination = tile_box(destination_index, width, height);
        let source = tile_box(source_index, width, height);
        let tile = image::imageops::crop_imm(&img, source.x, source.y, source.width, source.height)
            .to_image();
        image::imageops::replace(
            &mut output,
            &tile,
            i64::from(destination.x),
            i64::from(destination.y),
        );
    }

    let mut buf = Vec::with_capacity(input.len());
    let cursor = Cursor::new(&mut buf);
    let encoder = image::codecs::png::PngEncoder::new(cursor);
    encoder
        .write_image(
            output.as_raw(),
            output.width(),
            output.height(),
            ExtendedColorType::Rgba8,
        )
        .context("failed to encode descrambled Comix image as PNG")?;
    Ok(buf)
}

fn tile_box(index: usize, width: u32, height: u32) -> TileBox {
    let grid = usize::try_from(COMIX_SCRAMBLE_GRID).expect("grid should fit usize");
    let column = index % grid;
    let row = index / grid;
    let x = rounded_tile_edge(column, width);
    let y = rounded_tile_edge(row, height);
    let right = rounded_tile_edge(column + 1, width);
    let bottom = rounded_tile_edge(row + 1, height);

    TileBox {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}

fn rounded_tile_edge(position: usize, dimension: u32) -> u32 {
    let position = u64::try_from(position).expect("tile position should fit u64");
    let scaled = (u64::from(dimension) * position + (u64::from(COMIX_SCRAMBLE_GRID) / 2))
        / u64::from(COMIX_SCRAMBLE_GRID);
    u32::try_from(scaled).unwrap_or(dimension)
}

struct TileBox {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn supported_image_format_name(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Avif => Some("avif"),
        ImageFormat::Gif => Some("gif"),
        ImageFormat::Jpeg => Some("jpeg"),
        ImageFormat::Png => Some("png"),
        ImageFormat::WebP => Some("webp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comix_descrambler_restores_5x5_tile_order() {
        let original = synthetic_tile_image();
        let scrambled = scramble_like_comix(&original);
        let encoded = encode_png(&scrambled);

        let decoded = descramble_comix_5x5_to_png(&encoded).unwrap();
        let restored = image::load_from_memory(&decoded).unwrap().to_rgba8();

        assert_eq!(restored.as_raw(), original.as_raw());
    }

    fn synthetic_tile_image() -> image::RgbaImage {
        let tile = 4;
        let mut image =
            image::RgbaImage::new(COMIX_SCRAMBLE_GRID * tile, COMIX_SCRAMBLE_GRID * tile);
        for index in 0..25usize {
            let bounds = tile_box(index, image.width(), image.height());
            let color = image::Rgba([
                u8::try_from(index * 7).unwrap(),
                u8::try_from(index * 5).unwrap(),
                u8::try_from(index * 3).unwrap(),
                255,
            ]);
            for y in bounds.y..bounds.y + bounds.height {
                for x in bounds.x..bounds.x + bounds.width {
                    image.put_pixel(x, y, color);
                }
            }
        }
        image
    }

    fn scramble_like_comix(original: &image::RgbaImage) -> image::RgbaImage {
        let mut scrambled = image::RgbaImage::new(original.width(), original.height());
        for (destination_index, source_index) in COMIX_DESCRAMBLE_MAP.iter().copied().enumerate() {
            let destination = tile_box(destination_index, original.width(), original.height());
            let source = tile_box(source_index, original.width(), original.height());
            let tile = image::imageops::crop_imm(
                original,
                destination.x,
                destination.y,
                destination.width,
                destination.height,
            )
            .to_image();
            image::imageops::replace(
                &mut scrambled,
                &tile,
                i64::from(source.x),
                i64::from(source.y),
            );
        }
        scrambled
    }

    fn encode_png(image: &image::RgbaImage) -> Vec<u8> {
        let mut encoded = Vec::new();
        let cursor = Cursor::new(&mut encoded);
        let encoder = image::codecs::png::PngEncoder::new(cursor);
        encoder
            .write_image(
                image.as_raw(),
                image.width(),
                image.height(),
                ExtendedColorType::Rgba8,
            )
            .unwrap();
        encoded
    }
}
