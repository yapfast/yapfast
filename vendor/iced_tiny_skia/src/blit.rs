//! Anomi port (patch 5): glyphs copied onto the frame directly, clipped to
//! a rectangle.
//!
//! Stock drew each glyph with `draw_pixmap`: a pattern shader through
//! tiny-skia's f32 pipeline, and, for text not wholly inside its clip, the
//! window-sized clip mask set to the clip first (cleared and filled). Here a
//! glyph is clipped by its rectangle and laid source over pixel by pixel,
//! with the arithmetic of that pipeline, so the frame comes out the same.

use crate::core::Rectangle;

/// What `1 / 255` is to tiny-skia when it loads a channel.
const FACTOR: f32 = 1.0 / 255.0;

/// The rectangle text is clipped by. Stock drew text
/// wholly inside its clip with no mask at all, so a glyph reaching past the
/// text's bounds (a descender, an emoji's edge) showed past the clip; it
/// still does, but only within the damaged region being drawn, which each
/// region is cleared for. Past it, stock laid those pixels again over
/// themselves on every frame the text was drawn. Yapfast's patch clips such
/// text by its clip, which cut those glyphs where a clip is tight to them.
pub fn text_clip(
    physical_bounds: Rectangle,
    clip_bounds: Rectangle,
    damage_bounds: Rectangle,
) -> Rectangle {
    if physical_bounds.is_within(&clip_bounds) {
        damage_bounds
    } else {
        clip_bounds
    }
}

/// Lays `source`, `width` by `height` premultiplied pixels, onto `pixels` at
/// (`x`, `y`), source over, within `clip_bounds` and scaled by `opacity`, as
/// tiny-skia's `draw_pixmap` would with no transform and the pixels of
/// `clip_bounds` as its mask.
pub fn glyph(
    source: &[u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    clip_bounds: Rectangle,
    opacity: f32,
    pixels: &mut tiny_skia::PixmapMut<'_>,
) {
    let (width, height) = (width as i32, height as i32);
    let (target_width, target_height) = (pixels.width() as i32, pixels.height() as i32);
    // The pixels whose centres are inside, as the mask was filled.
    let left = x.max(clip_bounds.x.round() as i32).max(0);
    let top = y.max(clip_bounds.y.round() as i32).max(0);
    let right = (x + width)
        .min((clip_bounds.x + clip_bounds.width).round() as i32)
        .min(target_width);
    let bottom = (y + height)
        .min((clip_bounds.y + clip_bounds.height).round() as i32)
        .min(target_height);
    // tiny-skia clamps the opacity of a pattern; at nothing, the pipeline
    // leaves every pixel as it was.
    let opacity = opacity.clamp(0.0, 1.0);
    if left >= right || top >= bottom || opacity == 0.0 {
        return;
    }

    let stride = target_width as usize;
    let frame: &mut [u32] = bytemuck::cast_slice_mut(pixels.data_mut());
    let count = (right - left) as usize;

    for row in top..bottom {
        let from = ((row - y) * width + (left - x)) as usize * 4;
        let source_row = &source[from..from + count * 4];
        let at = row as usize * stride + left as usize;
        let target_row = &mut frame[at..at + count];

        for (pixel, below) in source_row.chunks_exact(4).zip(target_row) {
            let pixel: [u8; 4] = pixel.try_into().expect("four bytes");
            if pixel == [0; 4] {
                continue;
            }
            if pixel[3] == u8::MAX && opacity == 1.0 {
                *below = u32::from_ne_bytes(pixel);
                continue;
            }
            *below = u32::from_ne_bytes(over(pixel, below.to_ne_bytes(), opacity));
        }
    }
}

/// `pixel`, scaled by `opacity`, laid over `below`: tiny-skia's high
/// precision source over, one lane of it (`gather`, `scale_1_float`,
/// `load_dst`, `source_over`, `store`).
fn over(pixel: [u8; 4], below: [u8; 4], opacity: f32) -> [u8; 4] {
    let alpha = f32::from(pixel[3]) * FACTOR * opacity;
    let keep = 1.0 - alpha;
    std::array::from_fn(|channel| {
        let source = f32::from(pixel[channel]) * FACTOR * opacity;
        let destination = f32::from(below[channel]) * FACTOR;
        let mixed = destination * keep + source;
        (mixed.clamp(0.0, 1.0) * 255.0).round_ties_even() as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small xorshift, so the test needs no crate.
    struct Noise(u32);

    impl Noise {
        fn next(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0
        }
    }

    #[test]
    fn text_inside_its_clip_is_clipped_by_the_damaged_region_only() {
        let rectangle = |x, y, width, height| Rectangle {
            x,
            y,
            width,
            height,
        };
        let damage = rectangle(0.0, 0.0, 400.0, 300.0);
        let clip = rectangle(10.0, 10.0, 100.0, 20.0);
        // A line inside its clip: its descenders may reach below it.
        assert_eq!(
            text_clip(rectangle(10.0, 10.0, 80.0, 20.0), clip, damage),
            damage
        );
        // Text that runs past its clip is cut at it.
        assert_eq!(
            text_clip(rectangle(10.0, 10.0, 180.0, 20.0), clip, damage),
            clip
        );
    }

    #[test]
    fn a_glyph_comes_out_as_tiny_skias_draw_pixmap_drew_it() {
        let (width, height) = (64, 40);
        let mut noise = Noise(0x9e37_79b9);
        let frame: Vec<u32> = (0..width * height)
            .map(|_| noise.next() | 0xff00_0000)
            .collect();

        // A glyph's pixels: mostly empty or opaque, the rest partly
        // covered, premultiplied as the glyph cache makes them.
        let (glyph_width, glyph_height) = (23, 17);
        let source: Vec<u8> = (0..glyph_width * glyph_height)
            .flat_map(|_| {
                let alpha = match noise.next() % 4 {
                    0 => 0,
                    1 => 255,
                    _ => (noise.next() % 256) as u8,
                };
                let [red, green, blue, _] = noise.next().to_le_bytes();
                let pixel = tiny_skia::ColorU8::from_rgba(red, green, blue, alpha).premultiply();
                [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
            })
            .collect();
        let pixmap = tiny_skia::PixmapRef::from_bytes(&source, glyph_width, glyph_height)
            .expect("glyph pixmap");

        let clips = [
            Rectangle::new(
                crate::core::Point::ORIGIN,
                crate::core::Size::new(64.0, 40.0),
            ),
            Rectangle::new(
                crate::core::Point::new(10.5, 7.49),
                crate::core::Size::new(20.2, 18.6),
            ),
            Rectangle::new(
                crate::core::Point::new(-3.0, 30.0),
                crate::core::Size::new(90.0, 4.0),
            ),
        ];
        for opacity in [1.0, 0.5, 0.731, 0.0] {
            for (x, y) in [(5, 3), (-7, -4), (50, 30), (20, 12)] {
                for clip in clips {
                    let mut ours = frame.clone();
                    let mut theirs = frame.clone();

                    glyph(
                        &source,
                        glyph_width,
                        glyph_height,
                        x,
                        y,
                        clip,
                        opacity,
                        &mut tiny_skia::PixmapMut::from_bytes(
                            bytemuck::cast_slice_mut(&mut ours),
                            width,
                            height,
                        )
                        .expect("pixmap"),
                    );

                    // Stock drew text wholly inside its clip with no mask
                    // (the first clip is the whole frame), the rest masked.
                    let whole = clip == clips[0];
                    let mut mask = tiny_skia::Mask::new(width, height).expect("mask");
                    mask.fill_path(
                        &tiny_skia::PathBuilder::from_rect(
                            tiny_skia::Rect::from_xywh(clip.x, clip.y, clip.width, clip.height)
                                .expect("clip"),
                        ),
                        tiny_skia::FillRule::EvenOdd,
                        false,
                        tiny_skia::Transform::default(),
                    );
                    tiny_skia::PixmapMut::from_bytes(
                        bytemuck::cast_slice_mut(&mut theirs),
                        width,
                        height,
                    )
                    .expect("pixmap")
                    .draw_pixmap(
                        x,
                        y,
                        pixmap,
                        &tiny_skia::PixmapPaint {
                            opacity,
                            ..tiny_skia::PixmapPaint::default()
                        },
                        tiny_skia::Transform::identity(),
                        (!whole).then_some(&mask),
                    );

                    assert!(ours == theirs, "opacity {opacity}, at {x}, {y}, {clip:?}");
                }
            }
        }
    }
}
