//! Analysis of thin strokes: furigana, small kanji and hairlines.
//!
//! This module is **analysis only**. It deliberately does not modify any
//! image. Phase 8 of the plan is about preserving weak 1-2px strokes, and the
//! first requirement there is being able to say where they are and whether
//! they survived -- asserting an improvement without being able to measure it
//! is how restoration work goes wrong.
//!
//! Stroke thickness is estimated from a distance transform: for a pixel of
//! ink, the distance to the nearest non-ink pixel is roughly half the stroke
//! width, so a stroke of width `w` has a maximum interior distance of about
//! `(w + 1) / 2`. That handles curves and joins, which a run-length measure
//! along rows and columns does not.

/// Levels at or below this count as ink. Manga line art is near-bilevel, so
/// the midpoint is a reasonable default and avoids a tuned constant.
pub const DEFAULT_INK_THRESHOLD: u8 = 128;

/// Chamfer weights approximating Euclidean distance. Using 1 for orthogonal
/// and sqrt(2) for diagonal steps keeps diagonal hairlines from measuring
/// thicker than horizontal ones, which a pure 4-connected transform does.
const ORTHOGONAL: f32 = 1.0;
const DIAGONAL: f32 = std::f32::consts::SQRT_2;

/// Distance from every ink pixel to the nearest non-ink pixel.
///
/// Non-ink pixels are 0. Computed with a two-pass chamfer transform, which is
/// linear in the number of pixels.
pub fn distance_to_paper(ink: &[bool], width: usize, height: usize) -> Vec<f32> {
    assert_eq!(ink.len(), width * height);
    let mut distance = vec![0.0f32; width * height];
    // A bound larger than any achievable distance in this image.
    let far = (width + height) as f32;

    for (index, &is_ink) in ink.iter().enumerate() {
        distance[index] = if is_ink { far } else { 0.0 };
    }

    // Forward pass: up, up-left, up-right, left.
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if distance[index] == 0.0 {
                continue;
            }
            let mut best = distance[index];
            if y > 0 {
                best = best.min(distance[index - width] + ORTHOGONAL);
                if x > 0 {
                    best = best.min(distance[index - width - 1] + DIAGONAL);
                }
                if x + 1 < width {
                    best = best.min(distance[index - width + 1] + DIAGONAL);
                }
            }
            if x > 0 {
                best = best.min(distance[index - 1] + ORTHOGONAL);
            }
            distance[index] = best;
        }
    }

    // Backward pass: down, down-right, down-left, right.
    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let index = y * width + x;
            if distance[index] == 0.0 {
                continue;
            }
            let mut best = distance[index];
            if y + 1 < height {
                best = best.min(distance[index + width] + ORTHOGONAL);
                if x + 1 < width {
                    best = best.min(distance[index + width + 1] + DIAGONAL);
                }
                if x > 0 {
                    best = best.min(distance[index + width - 1] + DIAGONAL);
                }
            }
            if x + 1 < width {
                best = best.min(distance[index + 1] + ORTHOGONAL);
            }
            distance[index] = best;
        }
    }

    distance
}

/// Ink mask for a greyscale buffer.
pub fn ink_mask(luma: &[u8], threshold: u8) -> Vec<bool> {
    luma.iter().map(|value| *value <= threshold).collect()
}

/// Summary of the thin strokes in an image.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeAnalysis {
    /// Per-pixel: ink belonging to a stroke no wider than the limit.
    pub thin: Vec<bool>,
    pub ink_pixels: usize,
    pub thin_pixels: usize,
    /// Widest stroke found, in pixels. 0 when there is no ink.
    pub max_stroke_width: f32,
}

impl StrokeAnalysis {
    /// Fraction of ink that belongs to thin strokes; 0.0 when there is no ink.
    pub fn thin_fraction(&self) -> f32 {
        if self.ink_pixels == 0 {
            0.0
        } else {
            self.thin_pixels as f32 / self.ink_pixels as f32
        }
    }
}

/// Locate ink belonging to strokes no wider than `max_stroke_width` pixels.
///
/// `max_stroke_width` is in *input* pixels: 2 selects the 1-2px strokes that
/// furigana and fine hatching are made of.
pub fn analyse_strokes(
    luma: &[u8],
    width: usize,
    height: usize,
    ink_threshold: u8,
    max_stroke_width: f32,
) -> StrokeAnalysis {
    let ink = ink_mask(luma, ink_threshold);
    let distance = distance_to_paper(&ink, width, height);

    // A stroke of width w has a maximum interior distance of about (w+1)/2.
    let distance_limit = (max_stroke_width + 1.0) / 2.0;

    let mut thin = vec![false; width * height];
    let mut ink_pixels = 0usize;
    let mut thin_pixels = 0usize;
    let mut peak = 0.0f32;

    for index in 0..width * height {
        if !ink[index] {
            continue;
        }
        ink_pixels += 1;
        peak = peak.max(distance[index]);
        if distance[index] <= distance_limit {
            thin[index] = true;
            thin_pixels += 1;
        }
    }

    StrokeAnalysis {
        thin,
        ink_pixels,
        thin_pixels,
        max_stroke_width: if ink_pixels == 0 {
            0.0
        } else {
            2.0 * peak - 1.0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a greyscale buffer from an ASCII picture: '#' is ink.
    fn picture(rows: &[&str]) -> (Vec<u8>, usize, usize) {
        let height = rows.len();
        let width = rows[0].len();
        let mut luma = vec![255u8; width * height];
        for (y, row) in rows.iter().enumerate() {
            assert_eq!(row.len(), width, "ragged picture");
            for (x, cell) in row.chars().enumerate() {
                if cell == '#' {
                    luma[y * width + x] = 0;
                }
            }
        }
        (luma, width, height)
    }

    #[test]
    fn a_blank_image_has_no_ink() {
        let (luma, width, height) = picture(&["....", "....", "...."]);
        let analysis = analyse_strokes(&luma, width, height, DEFAULT_INK_THRESHOLD, 2.0);
        assert_eq!(analysis.ink_pixels, 0);
        assert_eq!(analysis.thin_pixels, 0);
        assert_eq!(analysis.thin_fraction(), 0.0);
        assert_eq!(analysis.max_stroke_width, 0.0);
    }

    #[test]
    fn a_one_pixel_line_is_entirely_thin() {
        let (luma, width, height) = picture(&[".......", "#######", "......."]);
        let analysis = analyse_strokes(&luma, width, height, DEFAULT_INK_THRESHOLD, 2.0);
        assert_eq!(analysis.ink_pixels, 7);
        assert_eq!(analysis.thin_pixels, 7, "every pixel of a 1px line is thin");
        assert_eq!(analysis.thin_fraction(), 1.0);
        assert_eq!(analysis.max_stroke_width, 1.0);
    }

    #[test]
    fn a_thick_bar_has_a_core_that_is_not_thin() {
        // A 7px tall bar: the middle rows are far from any paper.
        let (luma, width, height) = picture(&[
            ".........",
            ".#######.",
            ".#######.",
            ".#######.",
            ".#######.",
            ".#######.",
            ".#######.",
            ".#######.",
            ".........",
        ]);
        let analysis = analyse_strokes(&luma, width, height, DEFAULT_INK_THRESHOLD, 2.0);
        assert!(analysis.ink_pixels > 0);
        assert!(
            analysis.thin_fraction() < 0.7,
            "a thick bar should not be mostly thin, got {}",
            analysis.thin_fraction()
        );
        // Only the outer ring is within the thin limit.
        assert!(!analysis.thin[4 * width + 4], "the core must not be thin");
        assert!(analysis.thin[width + 4], "the edge row is thin");
        assert!(analysis.max_stroke_width >= 6.0);
    }

    #[test]
    fn a_diagonal_hairline_is_thin() {
        // A pure 4-connected distance transform would over-measure this.
        let (luma, width, height) =
            picture(&["#.....", ".#....", "..#...", "...#..", "....#.", ".....#"]);
        let analysis = analyse_strokes(&luma, width, height, DEFAULT_INK_THRESHOLD, 2.0);
        assert_eq!(analysis.ink_pixels, 6);
        assert_eq!(analysis.thin_pixels, 6);
    }

    #[test]
    fn raising_the_width_limit_admits_thicker_strokes() {
        let (luma, width, height) = picture(&[".....", ".###.", ".###.", ".###.", "....."]);
        let strict = analyse_strokes(&luma, width, height, DEFAULT_INK_THRESHOLD, 1.0);
        let loose = analyse_strokes(&luma, width, height, DEFAULT_INK_THRESHOLD, 3.0);
        assert!(strict.thin_pixels < loose.thin_pixels);
        assert_eq!(loose.thin_pixels, loose.ink_pixels);
    }

    #[test]
    fn the_ink_threshold_is_respected() {
        let luma = vec![200u8; 9];
        let none = analyse_strokes(&luma, 3, 3, 128, 2.0);
        assert_eq!(none.ink_pixels, 0);
        let all = analyse_strokes(&luma, 3, 3, 220, 2.0);
        assert_eq!(all.ink_pixels, 9);
    }

    #[test]
    fn distance_is_zero_on_paper_and_positive_on_ink() {
        let (luma, width, height) = picture(&["...", ".#.", "..."]);
        let ink = ink_mask(&luma, DEFAULT_INK_THRESHOLD);
        let distance = distance_to_paper(&ink, width, height);
        assert_eq!(distance[0], 0.0);
        assert_eq!(distance[width + 1], 1.0);
    }
}
