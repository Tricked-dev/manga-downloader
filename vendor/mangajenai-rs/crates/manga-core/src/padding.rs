//! Input size requirements and the padding used to satisfy them.
//!
//! Faithful port of `spandrel/__helpers/size_req.py` and `pad_tensor`. For the
//! MangaJaNai ESRGAN models this is effectively a no-op (`minimum = 2`,
//! `multiple_of = 1`), but the rule is easy to get subtly wrong and phase 9
//! architectures commonly require multiples of 8, 16 or 64 -- so it is
//! implemented properly and tested rather than assumed away.

use ndarray::{Array4, ArrayView4};

/// Constraints an architecture places on its input dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeRequirements {
    /// Minimum width and height, in pixels.
    pub minimum: u32,
    /// Width and height must both be a multiple of this. Always >= 1.
    pub multiple_of: u32,
    /// Whether the input must be square.
    pub square: bool,
}

impl Default for SizeRequirements {
    /// The neutral requirement: anything goes.
    fn default() -> Self {
        Self {
            minimum: 0,
            multiple_of: 1,
            square: false,
        }
    }
}

const fn ceil_to_multiple(value: u32, multiple: u32) -> u32 {
    if value.is_multiple_of(multiple) {
        value
    } else {
        (value / multiple + 1) * multiple
    }
}

impl SizeRequirements {
    pub fn new(minimum: u32, multiple_of: u32, square: bool) -> Self {
        let multiple_of = multiple_of.max(1);
        // Spandrel rounds `minimum` up to a multiple of `multiple_of` on
        // construction, so the two constraints can never contradict.
        Self {
            minimum: ceil_to_multiple(minimum, multiple_of),
            multiple_of,
            square,
        }
    }

    /// True when no padding would be added for any input.
    pub fn is_neutral(&self) -> bool {
        self.minimum == 0 && self.multiple_of == 1 && !self.square
    }

    /// Padding needed on the right and bottom, as `(pad_width, pad_height)`.
    pub fn padding_for(&self, width: u32, height: u32) -> (u32, u32) {
        let mut target_width = ceil_to_multiple(width.max(self.minimum), self.multiple_of);
        let mut target_height = ceil_to_multiple(height.max(self.minimum), self.multiple_of);

        if self.square {
            let side = target_width.max(target_height);
            target_width = side;
            target_height = side;
        }

        (target_width - width, target_height - height)
    }

    /// True when the given size needs no padding.
    pub fn satisfied_by(&self, width: u32, height: u32) -> bool {
        self.padding_for(width, height) == (0, 0)
    }
}

/// Source index for a padded output position along one axis.
///
/// Reproduces torch's reflect-then-replicate composition. `F.pad(..., "reflect")`
/// refuses to pad by more than `len - 1`, so Spandrel caps the reflected amount
/// and applies the remainder with replicate padding; composing the two gives
/// this index map.
#[inline]
fn source_index(position: usize, len: usize, reflect: usize) -> usize {
    if position < len {
        position
    } else if position < len + reflect {
        // Reflect without repeating the edge sample: ... w-3, w-2 for the
        // first padded positions.
        len - 2 - (position - len)
    } else {
        // Replicate whatever the reflected region ended on.
        len - 1 - reflect
    }
}

/// Pad an NCHW tensor on the right and bottom to satisfy `requirements`.
///
/// Returns `(padded, was_padded)`. When no padding is needed the input is
/// returned unchanged, so the common case costs one comparison.
pub fn pad_nchw(input: ArrayView4<f32>, requirements: SizeRequirements) -> (Array4<f32>, bool) {
    let (batch, channels, height, width) = input.dim();
    let (pad_width, pad_height) = requirements.padding_for(width as u32, height as u32);

    if pad_width == 0 && pad_height == 0 {
        return (input.to_owned(), false);
    }

    let (pad_width, pad_height) = (pad_width as usize, pad_height as usize);
    // Reflection cannot exceed len - 1; the rest becomes replication.
    let reflect_width = pad_width.min(width.saturating_sub(1));
    let reflect_height = pad_height.min(height.saturating_sub(1));

    let new_width = width + pad_width;
    let new_height = height + pad_height;

    let columns: Vec<usize> = (0..new_width)
        .map(|x| source_index(x, width, reflect_width))
        .collect();

    let mut output = Array4::<f32>::zeros((batch, channels, new_height, new_width));
    for n in 0..batch {
        for c in 0..channels {
            for y in 0..new_height {
                let source_row = source_index(y, height, reflect_height);
                for (x, &source_column) in columns.iter().enumerate() {
                    output[[n, c, y, x]] = input[[n, c, source_row, source_column]];
                }
            }
        }
    }

    (output, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array4;

    #[test]
    fn neutral_requirements_never_pad() {
        let requirements = SizeRequirements::default();
        assert!(requirements.is_neutral());
        assert_eq!(requirements.padding_for(1, 1), (0, 0));
        assert_eq!(requirements.padding_for(1337, 42), (0, 0));
    }

    #[test]
    fn mangajanai_esrgan_only_pads_degenerate_inputs() {
        // minimum=2, multiple_of=1, square=false -- as reported by Spandrel.
        let requirements = SizeRequirements::new(2, 1, false);
        assert_eq!(requirements.padding_for(1, 1), (1, 1));
        assert_eq!(requirements.padding_for(2, 2), (0, 0));
        assert_eq!(requirements.padding_for(1600, 1131), (0, 0));
    }

    #[test]
    fn minimum_is_rounded_up_to_a_multiple() {
        // Spandrel guarantees minimum % multiple_of == 0 after construction.
        let requirements = SizeRequirements::new(10, 16, false);
        assert_eq!(requirements.minimum, 16);
        assert_eq!(requirements.padding_for(8, 8), (8, 8));
    }

    #[test]
    fn multiple_of_rounds_each_axis_up() {
        let requirements = SizeRequirements::new(0, 8, false);
        assert_eq!(requirements.padding_for(8, 8), (0, 0));
        assert_eq!(requirements.padding_for(9, 16), (7, 0));
        assert_eq!(requirements.padding_for(1, 1), (7, 7));
    }

    #[test]
    fn square_requirement_matches_the_longer_axis() {
        let requirements = SizeRequirements::new(0, 1, true);
        assert_eq!(requirements.padding_for(10, 4), (0, 6));
        assert_eq!(requirements.padding_for(4, 10), (6, 0));
    }

    /// Build an input whose height already satisfies `height`, so a
    /// `minimum` requirement pads the width only. `minimum` constrains *both*
    /// axes, which makes single-row fixtures misleading.
    fn rows(row: &[f32], height: usize) -> Array4<f32> {
        let mut data = Vec::with_capacity(row.len() * height);
        for _ in 0..height {
            data.extend_from_slice(row);
        }
        Array4::from_shape_vec((1, 1, height, row.len()), data).unwrap()
    }

    fn first_row(tensor: &Array4<f32>) -> Vec<f32> {
        tensor
            .slice(ndarray::s![0, 0, 0, ..])
            .iter()
            .copied()
            .collect()
    }

    fn first_column(tensor: &Array4<f32>) -> Vec<f32> {
        tensor
            .slice(ndarray::s![0, 0, .., 0])
            .iter()
            .copied()
            .collect()
    }

    #[test]
    fn reflect_padding_does_not_repeat_the_edge() {
        // torch.nn.functional.pad([1,2,3,4], (0,2), "reflect") -> [1,2,3,4,3,2]
        let input = rows(&[1.0, 2.0, 3.0, 4.0], 6);
        let (padded, did_pad) = pad_nchw(input.view(), SizeRequirements::new(6, 1, false));
        assert!(did_pad);
        assert_eq!(padded.dim(), (1, 1, 6, 6));
        assert_eq!(first_row(&padded), vec![1.0, 2.0, 3.0, 4.0, 3.0, 2.0]);
    }

    #[test]
    fn padding_beyond_the_reflect_limit_replicates() {
        // Reflect is capped at len-1 = 1, so [1,2] padded by 3 gives
        // reflect -> [1,2,1] then replicate -> [1,2,1,1,1].
        let input = rows(&[1.0, 2.0], 5);
        let (padded, did_pad) = pad_nchw(input.view(), SizeRequirements::new(5, 1, false));
        assert!(did_pad);
        assert_eq!(first_row(&padded), vec![1.0, 2.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn single_sample_axis_replicates() {
        let input = rows(&[7.0], 3);
        let (padded, _) = pad_nchw(input.view(), SizeRequirements::new(3, 1, false));
        assert_eq!(padded.dim(), (1, 1, 3, 3));
        assert!(padded.iter().all(|value| *value == 7.0));
    }

    #[test]
    fn vertical_padding_reflects_the_same_way() {
        // A single column [1,2,3,4] padded to height 6.
        let input = Array4::from_shape_vec((1, 1, 4, 1), vec![1.0, 2.0, 3.0, 4.0]).unwrap();
        let (padded, _) = pad_nchw(input.view(), SizeRequirements::new(0, 6, false));
        assert_eq!(padded.dim(), (1, 1, 6, 6));
        assert_eq!(first_column(&padded), vec![1.0, 2.0, 3.0, 4.0, 3.0, 2.0]);
    }

    #[test]
    fn padding_is_applied_to_the_right_and_bottom_only() {
        let input = Array4::from_shape_vec((1, 1, 2, 2), vec![1.0, 2.0, 3.0, 4.0]).unwrap();
        let (padded, _) = pad_nchw(input.view(), SizeRequirements::new(0, 3, false));
        assert_eq!(padded.dim(), (1, 1, 3, 3));
        // Original content stays anchored at the top-left corner.
        assert_eq!(padded[[0, 0, 0, 0]], 1.0);
        assert_eq!(padded[[0, 0, 0, 1]], 2.0);
        assert_eq!(padded[[0, 0, 1, 0]], 3.0);
        assert_eq!(padded[[0, 0, 1, 1]], 4.0);
    }

    #[test]
    fn no_padding_needed_returns_the_input_unchanged() {
        let input = Array4::from_shape_vec((1, 1, 2, 2), vec![1.0, 2.0, 3.0, 4.0]).unwrap();
        let (padded, did_pad) = pad_nchw(input.view(), SizeRequirements::new(2, 1, false));
        assert!(!did_pad);
        assert_eq!(padded, input);
    }
}
