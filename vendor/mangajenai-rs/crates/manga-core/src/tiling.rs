//! Splitting a page into overlapping tiles and reassembling the result.
//!
//! A full manga page at 4x does not fit in one tensor on most hardware, so the
//! page is processed in tiles. Tiles are cut with an overlap, and the overlap
//! is cross-faded on reassembly (see [`crate::blending`]) rather than butted
//! together: neighbouring tiles see different context and so produce slightly
//! different pixels, which shows up as a visible seam if the two are simply
//! abutted.
//!
//! Memory is bounded independently of page size. Output rows are accumulated
//! in a sliding band just tall enough to hold the tile row in flight plus its
//! overlap with the next one; rows that no future tile can touch are quantised
//! into the output image and dropped.

use anyhow::{Context, Result, bail};
use image::RgbImage;

use crate::blending::{AxisWeights, axis_weights};
use crate::image::rgb8_region_to_tensor;
use crate::model::UpscaleModel;
use crate::tensor::quantize_u8;

/// How to cut a page into tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileConfig {
    /// Maximum tile width/height in *input* pixels.
    pub tile_size: u32,
    /// Overlap between neighbouring tiles, in *input* pixels.
    pub overlap: u32,
}

impl Default for TileConfig {
    fn default() -> Self {
        Self {
            tile_size: 512,
            overlap: 32,
        }
    }
}

impl TileConfig {
    pub fn validate(&self) -> Result<()> {
        if self.tile_size == 0 {
            bail!("tile size must be greater than 0");
        }
        if self.overlap >= self.tile_size {
            bail!(
                "overlap ({}) must be smaller than the tile size ({}); otherwise tiles \
                 never advance",
                self.overlap,
                self.tile_size
            );
        }
        Ok(())
    }
}

/// One tile's extent along a single axis, in input pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileSpan {
    pub start: u32,
    pub len: u32,
}

impl TileSpan {
    pub fn end(&self) -> u32 {
        self.start + self.len
    }
}

/// Lay out tile positions along one axis.
///
/// The final tile is shifted back to end exactly on the edge rather than being
/// truncated, so every tile is the full size and the model never sees a
/// narrow sliver whose context differs from every other tile.
pub fn plan_axis(len: u32, tile_size: u32, overlap: u32) -> Vec<TileSpan> {
    if len <= tile_size {
        return vec![TileSpan { start: 0, len }];
    }

    let step = tile_size - overlap;
    let mut spans = Vec::new();
    let mut start = 0u32;

    loop {
        if start + tile_size >= len {
            let final_start = len - tile_size;
            // The previous tile may already have reached the edge.
            if spans.last()
                != Some(&TileSpan {
                    start: final_start,
                    len: tile_size,
                })
            {
                spans.push(TileSpan {
                    start: final_start,
                    len: tile_size,
                });
            }
            break;
        }
        spans.push(TileSpan {
            start,
            len: tile_size,
        });
        start += step;
    }

    spans
}

/// Upscale a page tile by tile.
pub fn upscale_tiled(
    model: &mut UpscaleModel,
    source: &RgbImage,
    config: TileConfig,
) -> Result<RgbImage> {
    let scale = model.scale();
    tiled_with(source, scale, config, |tile| model.upscale_tensor(tile))
}

/// The tiling and reassembly machinery, independent of how a tile is upscaled.
///
/// Taking the upscaler as a closure keeps the seam logic testable without an
/// ONNX model: the tests drive it with an exact nearest-neighbour upscaler, so
/// every tile agrees on the pixels it shares with its neighbours and the
/// reassembled page must come back bit-identical to a direct upscale. Any
/// indexing, ramp or normalisation mistake breaks that equality.
fn tiled_with<F>(
    source: &RgbImage,
    scale: u32,
    config: TileConfig,
    mut upscale: F,
) -> Result<RgbImage>
where
    F: FnMut(ndarray::ArrayView4<f32>) -> Result<ndarray::Array4<f32>>,
{
    config.validate()?;
    if scale == 0 {
        bail!("scale must be greater than 0");
    }

    let (width, height) = source.dimensions();
    let out_width = width as usize * scale as usize;
    let out_height = height as usize * scale as usize;

    let columns = plan_axis(width, config.tile_size, config.overlap);
    let rows = plan_axis(height, config.tile_size, config.overlap);
    let overlap_out = config.overlap as usize * scale as usize;

    let column_weights = axis_weights(&columns, out_width, scale, overlap_out);
    let row_weights = axis_weights(&rows, out_height, scale, overlap_out);

    let mut output = RgbImage::new(out_width as u32, out_height as u32);

    // Sliding accumulation band. It only ever needs to span from the start of
    // the tile row currently in flight to the end of that row's output, which
    // is at most one tile plus one overlap tall.
    let band_capacity_rows = (config.tile_size as usize + config.overlap as usize) * scale as usize;
    let mut band = vec![0.0f32; band_capacity_rows * out_width * 3];
    let mut band_start = 0usize;
    let mut band_filled = 0usize;

    for (row_index, row) in rows.iter().enumerate() {
        let row_out_start = row.start as usize * scale as usize;
        let row_out_end = row.end() as usize * scale as usize;

        // Everything above this tile row is final: no later tile can reach it.
        if row_out_start > band_start {
            flush_band(
                &mut output,
                &mut band,
                &mut band_start,
                &mut band_filled,
                row_out_start,
                out_width,
                &column_weights,
                &row_weights,
            );
        }

        let needed = row_out_end - band_start;
        if needed > band_capacity_rows {
            bail!(
                "internal error: accumulation band needs {needed} rows but only \
                 {band_capacity_rows} were reserved"
            );
        }
        // Zero any rows newly entering the band.
        if needed > band_filled {
            let from = band_filled * out_width * 3;
            let to = needed * out_width * 3;
            band[from..to].fill(0.0);
            band_filled = needed;
        }

        for (column_index, column) in columns.iter().enumerate() {
            let tile = rgb8_region_to_tensor(source, column.start, row.start, column.len, row.len);
            let upscaled = upscale(tile.view()).with_context(|| {
                format!(
                    "failed to upscale tile at ({}, {}) {}x{}",
                    column.start, row.start, column.len, row.len
                )
            })?;

            accumulate_tile(
                &mut band,
                band_start,
                out_width,
                upscaled.view(),
                column.start as usize * scale as usize,
                row_out_start,
                &column_weights.per_tile[column_index],
                &row_weights.per_tile[row_index],
            );
        }
    }

    flush_band(
        &mut output,
        &mut band,
        &mut band_start,
        &mut band_filled,
        out_height,
        out_width,
        &column_weights,
        &row_weights,
    );

    Ok(output)
}

/// Add one upscaled tile into the accumulation band, weighted by its ramps.
#[allow(clippy::too_many_arguments)]
fn accumulate_tile(
    band: &mut [f32],
    band_start: usize,
    out_width: usize,
    tile: ndarray::ArrayView4<f32>,
    tile_x: usize,
    tile_y: usize,
    column_ramp: &[f32],
    row_ramp: &[f32],
) {
    let (_, channels, tile_height, tile_width) = tile.dim();

    for (y, &weight_y) in row_ramp.iter().enumerate().take(tile_height) {
        let band_row = tile_y + y - band_start;
        let row_offset = band_row * out_width * 3;

        for channel in 0..channels {
            let plane = tile.index_axis(ndarray::Axis(0), 0);
            let plane = plane.index_axis(ndarray::Axis(0), channel);
            let source_row = plane.index_axis(ndarray::Axis(0), y);

            for (x, value) in source_row.iter().enumerate().take(tile_width) {
                let weight = weight_y * column_ramp[x];
                band[row_offset + (tile_x + x) * 3 + channel] += value * weight;
            }
        }
    }
}

/// Normalise and quantise finished rows into the output image.
#[allow(clippy::too_many_arguments)]
fn flush_band(
    output: &mut RgbImage,
    band: &mut [f32],
    band_start: &mut usize,
    band_filled: &mut usize,
    up_to: usize,
    out_width: usize,
    column_weights: &AxisWeights,
    row_weights: &AxisWeights,
) {
    if up_to <= *band_start {
        return;
    }

    let rows_to_flush = up_to - *band_start;
    let raw = output.as_mut();

    for local_row in 0..rows_to_flush.min(*band_filled) {
        let global_row = *band_start + local_row;
        let row_total = row_weights.totals[global_row];
        let band_offset = local_row * out_width * 3;
        let out_offset = global_row * out_width * 3;

        for x in 0..out_width {
            // Total weight is separable because the tile grid is a full
            // Cartesian product, so a 2-D weight buffer is never needed.
            let total = row_total * column_weights.totals[x];
            for channel in 0..3 {
                let value = band[band_offset + x * 3 + channel] / total;
                raw[out_offset + x * 3 + channel] = quantize_u8(value);
            }
        }
    }

    // Slide the retained tail of the band to the front.
    let retained = band_filled.saturating_sub(rows_to_flush);
    if retained > 0 {
        let from = rows_to_flush * out_width * 3;
        let to = from + retained * out_width * 3;
        band.copy_within(from..to, 0);
    }
    *band_filled = retained;
    *band_start = up_to;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_smaller_than_one_tile_is_a_single_tile() {
        assert_eq!(
            plan_axis(300, 512, 32),
            vec![TileSpan { start: 0, len: 300 }]
        );
        assert_eq!(
            plan_axis(512, 512, 32),
            vec![TileSpan { start: 0, len: 512 }]
        );
    }

    #[test]
    fn tiles_advance_by_tile_size_minus_overlap() {
        let spans = plan_axis(1200, 512, 32);
        assert_eq!(spans[0], TileSpan { start: 0, len: 512 });
        assert_eq!(
            spans[1],
            TileSpan {
                start: 480,
                len: 512
            }
        );
        // The last tile is pulled back to end exactly on the edge.
        assert_eq!(spans.last().unwrap().end(), 1200);
        assert!(spans.iter().all(|span| span.len == 512));
    }

    #[test]
    fn tiles_cover_the_whole_axis_without_gaps() {
        for len in [513u32, 800, 1024, 1200, 1600, 2048, 4096] {
            for (tile, overlap) in [(512u32, 32u32), (256, 16), (128, 64), (512, 0)] {
                let spans = plan_axis(len, tile, overlap);
                assert_eq!(spans[0].start, 0, "len={len} tile={tile}");
                assert_eq!(spans.last().unwrap().end(), len, "len={len} tile={tile}");
                for pair in spans.windows(2) {
                    assert!(
                        pair[1].start <= pair[0].end(),
                        "gap between {:?} and {:?} (len={len}, tile={tile})",
                        pair[0],
                        pair[1]
                    );
                    assert!(pair[1].start > pair[0].start, "tiles must advance");
                }
            }
        }
    }

    #[test]
    fn the_final_tile_is_not_duplicated() {
        // 1024 with tile 512 and overlap 0 lands exactly on the edge.
        let spans = plan_axis(1024, 512, 0);
        assert_eq!(
            spans,
            vec![
                TileSpan { start: 0, len: 512 },
                TileSpan {
                    start: 512,
                    len: 512
                }
            ]
        );
    }

    /// Exact nearest-neighbour upscale, used as a stand-in model.
    ///
    /// Works on flat slices rather than 4-D indices: these run in debug
    /// builds, where bounds-checked `ndarray` indexing makes the obvious
    /// version take minutes instead of milliseconds.
    fn nearest_neighbour(tile: ndarray::ArrayView4<f32>, scale: usize) -> ndarray::Array4<f32> {
        let (_, channels, height, width) = tile.dim();
        let owned = tile.to_owned();
        let source = owned.as_slice().expect("standard layout");

        let (out_height, out_width) = (height * scale, width * scale);
        let mut data = vec![0.0f32; channels * out_height * out_width];
        for c in 0..channels {
            let source_plane = &source[c * height * width..(c + 1) * height * width];
            let destination_plane =
                &mut data[c * out_height * out_width..(c + 1) * out_height * out_width];
            for y in 0..out_height {
                let source_row = &source_plane[(y / scale) * width..(y / scale + 1) * width];
                let destination_row = &mut destination_plane[y * out_width..(y + 1) * out_width];
                for (x, sample) in destination_row.iter_mut().enumerate() {
                    *sample = source_row[x / scale];
                }
            }
        }

        ndarray::Array4::from_shape_vec((1, channels, out_height, out_width), data).unwrap()
    }

    fn test_page(width: u32, height: u32) -> RgbImage {
        let mut image = RgbImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            // Content that varies fast in both axes, so a misplaced tile or a
            // botched ramp cannot hide in a smooth gradient.
            *pixel = image::Rgb([
                (x.wrapping_mul(7) ^ y.wrapping_mul(13)) as u8,
                (x.wrapping_add(y).wrapping_mul(31)) as u8,
                ((x / 3).wrapping_mul(y / 2)) as u8,
            ]);
        }
        image
    }

    #[test]
    fn reassembly_is_exact_when_tiles_agree() {
        // Deliberately small. The point is awkward geometry -- odd sizes, a
        // clamped final tile, an overlap larger than half a tile -- not moving
        // a lot of pixels; `reassembly_is_exact_at_a_realistic_page_size`
        // covers the shipped tile geometry separately.
        for (width, height) in [(300u32, 200u32), (131, 97)] {
            for (tile_size, overlap) in [(128u32, 16u32), (100, 40), (64, 32)] {
                for scale in [1u32, 2, 4] {
                    let page = test_page(width, height);
                    let config = TileConfig { tile_size, overlap };

                    let tiled = tiled_with(&page, scale, config, |tile| {
                        Ok(nearest_neighbour(tile, scale as usize))
                    })
                    .unwrap();

                    let direct = crate::image::tensor_to_rgb8(
                        nearest_neighbour(
                            crate::image::rgb8_to_tensor(&page).view(),
                            scale as usize,
                        )
                        .view(),
                    )
                    .unwrap();

                    assert_eq!(
                        tiled.dimensions(),
                        direct.dimensions(),
                        "{width}x{height} tile={tile_size} overlap={overlap} scale={scale}"
                    );
                    let differing = tiled
                        .as_raw()
                        .iter()
                        .zip(direct.as_raw())
                        .filter(|(a, b)| a != b)
                        .count();
                    assert_eq!(
                        differing, 0,
                        "{differing} differing samples for {width}x{height} \
                         tile={tile_size} overlap={overlap} scale={scale}"
                    );
                }
            }
        }
    }

    #[test]
    fn reassembly_is_exact_at_a_realistic_page_size() {
        // The shipped default tile geometry, on a page needing two tiles per
        // axis. Scale 1 keeps it quick; the 4x path is covered by the matrix.
        let page = test_page(736, 736);
        let config = TileConfig {
            tile_size: 512,
            overlap: 32,
        };
        let tiled = tiled_with(&page, 1, config, |tile| Ok(nearest_neighbour(tile, 1))).unwrap();
        let direct = crate::image::tensor_to_rgb8(
            nearest_neighbour(crate::image::rgb8_to_tensor(&page).view(), 1).view(),
        )
        .unwrap();
        assert_eq!(tiled.as_raw(), direct.as_raw());
    }

    #[test]
    fn a_page_smaller_than_a_tile_still_round_trips() {
        let page = test_page(64, 48);
        let tiled = tiled_with(&page, 1, TileConfig::default(), |tile| {
            Ok(nearest_neighbour(tile, 1))
        })
        .unwrap();
        assert_eq!(tiled.as_raw(), page.as_raw());
    }

    #[test]
    fn zero_overlap_still_covers_the_page() {
        let page = test_page(300, 200);
        let config = TileConfig {
            tile_size: 128,
            overlap: 0,
        };
        let tiled = tiled_with(&page, 1, config, |tile| Ok(nearest_neighbour(tile, 1))).unwrap();
        assert_eq!(tiled.as_raw(), page.as_raw());
    }

    #[test]
    fn config_rejects_an_overlap_that_prevents_progress() {
        assert!(
            TileConfig {
                tile_size: 512,
                overlap: 512
            }
            .validate()
            .is_err()
        );
        assert!(
            TileConfig {
                tile_size: 0,
                overlap: 0
            }
            .validate()
            .is_err()
        );
        assert!(TileConfig::default().validate().is_ok());
    }
}
