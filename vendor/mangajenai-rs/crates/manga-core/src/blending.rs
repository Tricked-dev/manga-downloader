//! Cross-fade weights for reassembling overlapping tiles.
//!
//! Neighbouring tiles see different context, so their outputs disagree
//! slightly in the region they share. Butting them together turns that
//! disagreement into a hard vertical or horizontal line -- exactly the seam
//! artefact the `fixtures/tiling/` corpus is built to catch. Instead each tile
//! is faded in and out across its overlap.
//!
//! Weights are kept per axis rather than as a 2-D map. The tile grid is a full
//! Cartesian product of column spans and row spans, so the total weight at a
//! point factorises: `total(x, y) == totals_x(x) * totals_y(y)`. That turns a
//! page-sized weight buffer into two small vectors.

use crate::tiling::TileSpan;

/// Per-tile ramps along one axis, plus the total weight at each output column.
#[derive(Debug, Clone)]
pub struct AxisWeights {
    /// `per_tile[i][k]` is the weight tile `i` contributes at offset `k`
    /// within its own output extent.
    pub per_tile: Vec<Vec<f32>>,
    /// `totals[x]` is the sum of every tile's weight at output coordinate `x`.
    pub totals: Vec<f32>,
}

/// Build the cross-fade ramps for one axis.
///
/// `overlap_out` is the nominal overlap in *output* pixels. The ramp is
/// clamped to the real overlap with the neighbour, which matters for the final
/// tile: it is shifted back to sit flush with the edge, so it can overlap its
/// predecessor by much more than the nominal amount.
pub fn axis_weights(
    spans: &[TileSpan],
    out_len: usize,
    scale: u32,
    overlap_out: usize,
) -> AxisWeights {
    let scale = scale as usize;
    let mut per_tile = Vec::with_capacity(spans.len());
    let mut totals = vec![0.0f32; out_len];

    for (index, span) in spans.iter().enumerate() {
        let tile_out_len = span.len as usize * scale;
        let tile_out_start = span.start as usize * scale;

        // Fade in over the region shared with the previous tile, and out over
        // the region shared with the next one.
        let lead = if index == 0 {
            0
        } else {
            let previous_end = spans[index - 1].end() as usize * scale;
            overlap_out.min(previous_end.saturating_sub(tile_out_start))
        };
        let trail = if index + 1 == spans.len() {
            0
        } else {
            let next_start = spans[index + 1].start as usize * scale;
            overlap_out.min((tile_out_start + tile_out_len).saturating_sub(next_start))
        };

        let mut ramp = vec![1.0f32; tile_out_len];
        for (offset, weight) in ramp.iter_mut().enumerate() {
            if lead > 0 && offset < lead {
                // Half-sample offsets keep two equal-length opposing ramps
                // summing to exactly 1, and keep the weight strictly positive
                // so no output pixel is left with zero total weight.
                *weight *= (offset as f32 + 0.5) / lead as f32;
            }
            if trail > 0 && offset >= tile_out_len - trail {
                let from_end = tile_out_len - offset;
                *weight *= (from_end as f32 - 0.5) / trail as f32;
            }
        }

        for (offset, weight) in ramp.iter().enumerate() {
            totals[tile_out_start + offset] += *weight;
        }
        per_tile.push(ramp);
    }

    AxisWeights { per_tile, totals }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiling::plan_axis;

    fn spans(len: u32, tile: u32, overlap: u32) -> Vec<TileSpan> {
        plan_axis(len, tile, overlap)
    }

    #[test]
    fn a_single_tile_has_uniform_weight() {
        let weights = axis_weights(&spans(200, 512, 32), 200, 1, 32);
        assert_eq!(weights.per_tile.len(), 1);
        assert!(weights.per_tile[0].iter().all(|w| *w == 1.0));
        assert!(weights.totals.iter().all(|t| *t == 1.0));
    }

    #[test]
    fn every_output_position_has_positive_total_weight() {
        for len in [513u32, 800, 1200, 1600, 2048] {
            for (tile, overlap) in [(512u32, 32u32), (256, 16), (128, 64)] {
                for scale in [1u32, 4] {
                    let spans = spans(len, tile, overlap);
                    let out_len = len as usize * scale as usize;
                    let weights =
                        axis_weights(&spans, out_len, scale, overlap as usize * scale as usize);
                    assert_eq!(weights.totals.len(), out_len);
                    for (x, total) in weights.totals.iter().enumerate() {
                        assert!(
                            *total > 0.0,
                            "zero weight at {x} (len={len}, tile={tile}, overlap={overlap})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn interior_overlaps_cross_fade_to_exactly_one() {
        // Two tiles overlapping by the nominal amount: the ramps are equal
        // length and opposite, so no normalisation is needed there.
        let spans = vec![
            TileSpan { start: 0, len: 100 },
            TileSpan {
                start: 80,
                len: 100,
            },
        ];
        let weights = axis_weights(&spans, 180, 1, 20);
        for (x, total) in weights.totals.iter().enumerate() {
            assert!(
                (total - 1.0).abs() < 1e-6,
                "total at {x} was {total}, expected 1.0"
            );
        }
    }

    #[test]
    fn a_tile_fades_in_and_out_monotonically() {
        let spans = vec![
            TileSpan { start: 0, len: 100 },
            TileSpan {
                start: 80,
                len: 100,
            },
            TileSpan {
                start: 160,
                len: 100,
            },
        ];
        let weights = axis_weights(&spans, 260, 1, 20);
        let middle = &weights.per_tile[1];

        // Rises across the leading overlap...
        for pair in middle[..20].windows(2) {
            assert!(pair[1] > pair[0]);
        }
        // ...is flat in the interior...
        assert!(middle[20..80].iter().all(|w| (w - 1.0).abs() < 1e-6));
        // ...and falls across the trailing overlap.
        for pair in middle[80..].windows(2) {
            assert!(pair[1] < pair[0]);
        }
    }

    #[test]
    fn the_first_and_last_tiles_do_not_fade_at_the_page_edge() {
        let spans = vec![
            TileSpan { start: 0, len: 100 },
            TileSpan {
                start: 80,
                len: 100,
            },
        ];
        let weights = axis_weights(&spans, 180, 1, 20);
        // Full weight at the very first and very last output positions,
        // otherwise the page edges would be normalised against a tiny weight.
        assert_eq!(weights.per_tile[0][0], 1.0);
        assert_eq!(*weights.per_tile[1].last().unwrap(), 1.0);
    }

    #[test]
    fn a_clamped_final_tile_still_normalises_to_one() {
        // 300 wide, 128 tiles, 32 overlap: the last tile is pulled back and
        // overlaps its predecessor by more than the nominal 32.
        let spans = spans(300, 128, 32);
        let weights = axis_weights(&spans, 300, 1, 32);
        assert!(spans.len() >= 3);
        for total in &weights.totals {
            assert!(*total > 0.0);
        }
    }
}
