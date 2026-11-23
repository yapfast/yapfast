//! The colour that stands out most in a picture, which a profile without a
//! banner, accent or theme tints its banner with, as Discord does. No
//! toolkit in it: the window gives it pictures in either byte order.

/// The order of a pixel's bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Rgba,
    /// tiny-skia's, which the window keeps pictures in on the CPU.
    Bgra,
}

/// The colour Discord's client tints a banner with from an avatar, as
/// 0xRRGGBB, in `pixels` of four bytes each: the first of color-thief's
/// five-colour palette (`getColor`). Every tenth pixel counts, unless it is
/// mostly see-through or near white; MMCQ cuts them into five boxes of
/// colour, and the box with the most pixels times the room it spans gives
/// its average. A white ground so never wins, and a dark avatar on white
/// gives a dark banner, as on Discord. None for a picture with no such
/// pixel.
pub fn dominant(pixels: &[u8], order: Order) -> Option<u32> {
    let samples: Vec<[u8; 3]> = pixels
        .chunks_exact(4)
        .step_by(10)
        .filter(|pixel| pixel[3] >= 125)
        .map(|pixel| match order {
            Order::Rgba => [pixel[0], pixel[1], pixel[2]],
            Order::Bgra => [pixel[2], pixel[1], pixel[0]],
        })
        .filter(|[r, g, b]| !(*r > 250 && *g > 250 && *b > 250))
        .collect();
    let [red, green, blue] = mmcq::palette(&samples, 5)?;
    Some((u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue))
}

/// Modified median cut quantization, as color-thief's `quantize` does it,
/// down to its quirks, so that the colour it picks matches Discord's.
mod mmcq {
    const SIGBITS: u32 = 5;
    const RSHIFT: u32 = 8 - SIGBITS;
    const MAX_ITERATIONS: usize = 1000;

    fn index(r: usize, g: usize, b: usize) -> usize {
        (r << (2 * SIGBITS)) + (g << SIGBITS) + b
    }

    #[derive(Clone, Copy)]
    struct VBox {
        /// Low and high ends, inclusive, in red, green and blue.
        low: [usize; 3],
        high: [usize; 3],
    }

    impl VBox {
        fn volume(&self) -> u64 {
            (0..3)
                .map(|at| (self.high[at] + 1).saturating_sub(self.low[at]) as u64)
                .product()
        }

        fn cells(&self) -> impl Iterator<Item = [usize; 3]> + '_ {
            (self.low[0]..=self.high[0]).flat_map(move |r| {
                (self.low[1]..=self.high[1])
                    .flat_map(move |g| (self.low[2]..=self.high[2]).map(move |b| [r, g, b]))
            })
        }

        fn count(&self, histo: &[u32]) -> u64 {
            self.cells()
                .map(|[r, g, b]| u64::from(histo[index(r, g, b)]))
                .sum()
        }

        fn average(&self, histo: &[u32]) -> [u8; 3] {
            let mult = f64::from(1u32 << RSHIFT);
            let (mut total, mut sum) = (0.0, [0.0f64; 3]);
            for cell in self.cells() {
                let count = f64::from(histo[index(cell[0], cell[1], cell[2])]);
                total += count;
                for at in 0..3 {
                    sum[at] += count * (cell[at] as f64 + 0.5) * mult;
                }
            }
            std::array::from_fn(|at| {
                let value = if total > 0.0 {
                    sum[at] / total
                } else {
                    mult * (self.low[at] + self.high[at] + 1) as f64 / 2.0
                };
                value.clamp(0.0, 255.0) as u8
            })
        }

        /// Cuts it in two across its widest side, at its median, as
        /// `medianCutApply`; the second is none for a single pixel.
        fn cut(&self, histo: &[u32]) -> Option<(VBox, Option<VBox>)> {
            let count = self.count(histo);
            if count == 0 {
                return None;
            }
            if count == 1 {
                return Some((*self, None));
            }
            let widths: [usize; 3] = std::array::from_fn(|at| self.high[at] - self.low[at] + 1);
            let widest = widths.iter().copied().max().unwrap_or(0);
            // Red wins a tie, then green, as the original checks them.
            let axis = (0..3).find(|&at| widths[at] == widest).unwrap_or(0);
            let (low, high) = (self.low[axis], self.high[axis]);
            // partial[i]: pixels in the slices up to i along the axis.
            let mut partial = vec![0u64; high + 2];
            let mut total = 0;
            for (slice, sum) in partial.iter_mut().enumerate().take(high + 1).skip(low) {
                let mut part = *self;
                part.low[axis] = slice;
                part.high[axis] = slice;
                total += part.count(histo);
                *sum = total;
            }
            let ahead = |at: usize| total - partial[at];
            let slice = (low..=high).find(|&at| partial[at] as f64 > total as f64 / 2.0)?;
            let (left, right) = (slice - low, high - slice);
            let mut cut = if left <= right {
                (high.saturating_sub(1)).min(slice + right / 2)
            } else {
                low.max(((slice as f64 - 1.0 - left as f64 / 2.0).max(0.0)) as usize)
            };
            // Avoid empty boxes, as the original does.
            while cut <= high && partial[cut] == 0 {
                cut += 1;
            }
            let mut count2 = if cut <= high { ahead(cut) } else { 0 };
            while count2 == 0 && cut > low && partial[cut - 1] > 0 {
                cut -= 1;
                count2 = ahead(cut);
            }
            let (mut first, mut second) = (*self, *self);
            first.high[axis] = cut;
            second.low[axis] = cut + 1;
            Some((first, Some(second)))
        }
    }

    /// Splits the boxes, the one with the largest `key` first, until there
    /// are `target`, as `iter` does.
    fn split(boxes: &mut Vec<VBox>, histo: &[u32], target: f64, key: impl Fn(&VBox) -> u64) {
        let mut colors = boxes.len();
        for _ in 0..MAX_ITERATIONS {
            if colors as f64 >= target {
                return;
            }
            // A stable sort, then the last, as the original's queue pops.
            boxes.sort_by_key(&key);
            let Some(largest) = boxes.pop() else { return };
            match largest.cut(histo) {
                None => {
                    boxes.push(largest);
                    continue;
                }
                Some((first, second)) => {
                    boxes.push(first);
                    if let Some(second) = second {
                        boxes.push(second);
                        colors += 1;
                    }
                }
            }
        }
    }

    /// The first colour of a palette of `colors` for `pixels`.
    pub(crate) fn palette(pixels: &[[u8; 3]], colors: usize) -> Option<[u8; 3]> {
        if pixels.is_empty() {
            return None;
        }
        let mut histo = vec![0u32; 1 << (3 * SIGBITS)];
        let (mut low, mut high) = ([1_000_000usize; 3], [0usize; 3]);
        for pixel in pixels {
            let cell = pixel.map(|channel| usize::from(channel >> RSHIFT));
            histo[index(cell[0], cell[1], cell[2])] += 1;
            for side in 0..3 {
                // The original's min/max is `if … else if`, so the first pixel
                // only lowers the minimum.
                if cell[side] < low[side] {
                    low[side] = cell[side];
                } else if cell[side] > high[side] {
                    high[side] = cell[side];
                }
            }
        }
        // Where that leaves a maximum under its minimum, as with one colour
        // only, the box is empty and its colour its middle, as there.
        let mut boxes = vec![VBox { low, high }];
        split(&mut boxes, &histo, 0.75 * colors as f64, |b| {
            b.count(&histo)
        });
        split(&mut boxes, &histo, colors as f64, |b| {
            b.count(&histo) * b.volume()
        });
        boxes.sort_by_key(|b| b.count(&histo) * b.volume());
        boxes.last().map(|b| b.average(&histo))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(pixels: impl Fn(usize) -> [u8; 4]) -> Vec<u8> {
        (0..4000).flat_map(pixels).collect()
    }

    #[test]
    fn a_dark_avatar_on_white_gives_a_dark_banner_in_either_order() {
        let rgba = picture(|at| {
            if at % 3 == 0 {
                [20, 30, 120, 255]
            } else {
                [255, 255, 255, 255]
            }
        });
        let color = dominant(&rgba, Order::Rgba).unwrap();
        let [red, green, blue] = [color >> 16, (color >> 8) & 0xff, color & 0xff];
        assert!(
            red < 60 && green < 60 && blue > 90,
            "white never wins: {color:06x}"
        );
        let bgra: Vec<u8> = rgba
            .chunks(4)
            .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]])
            .collect();
        assert_eq!(dominant(&bgra, Order::Bgra), Some(color));
    }

    #[test]
    fn the_colour_with_the_most_pixels_wins() {
        let rgba = picture(|at| {
            if at % 8 == 0 {
                [200, 20, 20, 255]
            } else {
                [40, 160, 60, 255]
            }
        });
        let color = dominant(&rgba, Order::Rgba).unwrap();
        assert!((color >> 8) & 0xff > 120, "green covers most: {color:06x}");
    }

    #[test]
    fn nothing_for_see_through_or_white() {
        assert_eq!(dominant(&[0; 64], Order::Rgba), None, "see-through");
        assert_eq!(dominant(&[255; 4000], Order::Rgba), None, "all white");
    }
}
