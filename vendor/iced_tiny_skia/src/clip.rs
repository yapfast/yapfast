//! Anomi port (patch 4): the clip mask, kept as the one rectangle it is.
//!
//! The renderer sets its window-sized clip mask to a rectangle many times a
//! frame: once for each layer in each damaged region, and again for each
//! group of primitives and each picture. Stock, each time cleared the whole
//! mask and filled the rectangle through tiny-skia's path pipeline. Here only
//! the rows of the rectangle the mask held are cleared and the new one's
//! filled, and setting the rectangle it already holds does nothing.

use crate::core::Rectangle;

/// Which pixels of the clip mask are set, as `Engine` keeps it.
///
/// A mask holds nothing between draws: [`Clip::clear`] empties it at the end
/// of `Renderer::draw`, and a new mask is empty, so a draw may be handed the
/// surface's mask or a screenshot's.
#[derive(Debug, Default)]
pub struct Clip {
    /// The pixels the mask holds: left, top, right and bottom, the last two
    /// past its edge. `None` when it holds none.
    held: Option<[usize; 4]>,
}

impl Clip {
    /// Sets `mask` to `bounds`, in its pixels: the pixels whose centres are
    /// inside, as tiny-skia fills the rectangle without anti-aliasing.
    pub fn set(&mut self, mask: &mut tiny_skia::Mask, bounds: Rectangle) {
        let width = mask.width() as usize;
        let height = mask.height() as usize;
        let edge = |value: f32, limit: usize| (value.round().max(0.0) as usize).min(limit);
        let [left, top, right, bottom] = [
            edge(bounds.x, width),
            edge(bounds.y, height),
            edge(bounds.x + bounds.width, width),
            edge(bounds.y + bounds.height, height),
        ];
        let wanted = (left < right && top < bottom).then_some([left, top, right, bottom]);

        if wanted == self.held {
            return;
        }

        let data = mask.data_mut();
        fill(data, width, self.held, 0);
        fill(data, width, wanted, u8::MAX);
        self.held = wanted;
    }

    /// Empties `mask`, as it is between draws.
    pub fn clear(&mut self, mask: &mut tiny_skia::Mask) {
        let width = mask.width() as usize;
        fill(mask.data_mut(), width, self.held.take(), 0);
    }
}

/// Sets the pixels of `rectangle` in `data`, a mask `width` across.
fn fill(data: &mut [u8], width: usize, rectangle: Option<[usize; 4]>, value: u8) {
    if let Some([left, top, right, bottom]) = rectangle {
        for row in top..bottom {
            data[row * width + left..row * width + right].fill(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mask as stock set it: cleared, then the rectangle filled as a
    /// path without anti-aliasing.
    fn stock(mask: &mut tiny_skia::Mask, bounds: Rectangle) {
        mask.clear();
        let path = tiny_skia::PathBuilder::from_rect(
            tiny_skia::Rect::from_xywh(bounds.x, bounds.y, bounds.width, bounds.height)
                .expect("Create clip rectangle"),
        );
        mask.fill_path(
            &path,
            tiny_skia::FillRule::EvenOdd,
            false,
            tiny_skia::Transform::default(),
        );
    }

    #[test]
    fn each_rectangle_masks_the_pixels_tiny_skia_filled() {
        let (width, height) = (97, 61);
        let mut ours = tiny_skia::Mask::new(width, height).expect("mask");
        let mut theirs = tiny_skia::Mask::new(width, height).expect("mask");
        let mut clip = Clip::default();
        // Whole pixels, halves either way, fractions round either way, past
        // each edge, the whole mask, the same one twice, and one inside the
        // last, set one after another on the same mask.
        let rectangles = [
            (10.0, 5.0, 30.0, 20.0),
            (10.5, 5.5, 30.0, 20.0),
            (10.49, 5.51, 29.98, 20.02),
            (0.25, 0.75, 96.5, 60.5),
            (-12.3, -4.6, 40.2, 30.1),
            (80.4, 50.6, 40.0, 40.0),
            (0.0, 0.0, 97.0, 61.0),
            (0.0, 0.0, 97.0, 61.0),
            (33.3, 22.2, 0.4, 10.0),
            (20.0, 10.0, 50.0, 40.0),
            (25.5, 15.5, 10.0, 10.0),
            (96.6, 60.6, 3.0, 3.0),
            (3.0, 3.0, 1.0, 1.0),
        ];
        for (x, y, w, h) in rectangles {
            let bounds = Rectangle {
                x,
                y,
                width: w,
                height: h,
            };
            clip.set(&mut ours, bounds);
            stock(&mut theirs, bounds);
            assert!(ours.data() == theirs.data(), "{bounds:?}");
        }

        clip.clear(&mut ours);
        assert!(ours.data().iter().all(|&pixel| pixel == 0));
    }
}
