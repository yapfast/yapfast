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

/// Anomi port (patches 2, 7): the whole pixels (left, top, right, bottom) a
/// picture at `bounds`, physical, is put on: its corner on the nearest
/// pixel and its size rounded, as `anomi_widgets::picture` makes its copies
/// at the rounded pixels drawn, so the size doesn't change with where it
/// lies. The clip mask rounds each edge the same way.
pub fn whole(bounds: Rectangle) -> (i32, i32, i32, i32) {
    let (left, top) = (bounds.x.round() as i32, bounds.y.round() as i32);
    (
        left,
        top,
        left + bounds.width.round() as i32,
        top + bounds.height.round() as i32,
    )
}

/// Anomi port (patches 2, 7): the rounded rectangle on whole pixels a
/// picture is cut to, its own clip as iced's wgpu renderer rounds it: each
/// corner its own radius, at most half the shorter side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rounded {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    /// Top left, top right, bottom right, bottom left, in pixels.
    radius: [f32; 4],
}

impl Rounded {
    /// The pixels `edges` (left, top, right, bottom) with corners of
    /// `radius` pixels.
    pub fn new(edges: (i32, i32, i32, i32), radius: [f32; 4]) -> Self {
        let (left, top, right, bottom) = edges;
        let most = (right - left).min(bottom - top).max(0) as f32 / 2.0;
        Self {
            left,
            top,
            right,
            bottom,
            radius: radius.map(|radius| radius.min(most).max(0.0)),
        }
    }

    /// The corners row `y` passes through on its left and its right: each
    /// its radius, how many columns it reaches in and the row's place from
    /// the corner's edge.
    fn row(&self, y: i32) -> [Option<(f32, i32, i32)>; 2] {
        let [top_left, top_right, bottom_right, bottom_left] = self.radius;
        let (from_top, from_bottom) = (y - self.top, self.bottom - 1 - y);
        let corner = |radius: f32, from: i32| {
            let reach = radius.ceil() as i32;
            (from < reach).then_some((radius, reach, from))
        };
        let pick = |upper: f32, lower: f32| corner(upper, from_top).or(corner(lower, from_bottom));
        [pick(top_left, bottom_left), pick(top_right, bottom_right)]
    }

    /// How much of the pixel at column `x` of a row passing through
    /// `corners` ([`Rounded::row`]) lies inside, 0 to 1.
    fn cover(&self, x: i32, corners: [Option<(f32, i32, i32)>; 2]) -> f32 {
        match corners {
            [Some((radius, reach, from)), _] if x - self.left < reach => {
                coverage(radius, x - self.left, from)
            }
            [_, Some((radius, reach, from))] if self.right - 1 - x < reach => {
                coverage(radius, self.right - 1 - x, from)
            }
            _ => 1.0,
        }
    }
}

/// How much of the pixel `cell_x`, `cell_y` from a corner's edges a round
/// corner of `radius` covers: its centre's distance inside the arc, plus
/// half a pixel, as `anomi_widgets::picture` cleared its copies' corners.
fn coverage(radius: f32, cell_x: i32, cell_y: i32) -> f32 {
    let dx = radius - (cell_x as f32 + 0.5);
    let dy = radius - (cell_y as f32 + 0.5);
    if dx <= 0.0 || dy <= 0.0 {
        return 1.0;
    }
    (radius - dx.hypot(dy) + 0.5).clamp(0.0, 1.0)
}

/// Anomi port (patches 2, 6, 7, 19): lays `source`, a `width` by `height`
/// premultiplied picture, onto `pixels` at (`x`, `y`), source over, within
/// `clip_bounds` and `rounded`, scaled by `opacity`: as tiny-skia's
/// `draw_pixmap` draws it at its own size on whole pixels with the clip as
/// its mask, and each pixel where a corner passes scaled by how much of it
/// the corner covers. `opaque` says every pixel of `source` is, so a row
/// clear of the corners is one copy.
#[allow(clippy::too_many_arguments)]
pub fn picture(
    source: &[u8],
    width: u32,
    height: u32,
    (x, y): (i32, i32),
    clip_bounds: Rectangle,
    rounded: &Rounded,
    opacity: f32,
    opaque: bool,
    pixels: &mut tiny_skia::PixmapMut<'_>,
) {
    let (width, height) = (width as i32, height as i32);
    let (target_width, target_height) = (pixels.width() as i32, pixels.height() as i32);
    let left = x.max(clip_bounds.x.round() as i32).max(rounded.left).max(0);
    let top = y.max(clip_bounds.y.round() as i32).max(rounded.top).max(0);
    let right = (x + width)
        .min((clip_bounds.x + clip_bounds.width).round() as i32)
        .min(rounded.right)
        .min(target_width);
    let bottom = (y + height)
        .min((clip_bounds.y + clip_bounds.height).round() as i32)
        .min(rounded.bottom)
        .min(target_height);
    let opacity = opacity.clamp(0.0, 1.0);
    if left >= right
        || top >= bottom
        || opacity == 0.0
        || source.len() < (width * height * 4) as usize
    {
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
        let corners = rounded.row(row);

        if opaque && opacity == 1.0 && corners == [None, None] {
            bytemuck::cast_slice_mut::<u32, u8>(target_row).copy_from_slice(source_row);
            continue;
        }
        for (column, (pixel, below)) in (left..).zip(source_row.chunks_exact(4).zip(target_row)) {
            let pixel: [u8; 4] = pixel.try_into().expect("four bytes");
            if pixel == [0; 4] {
                continue;
            }
            let scale = if corners == [None, None] {
                opacity
            } else {
                opacity * rounded.cover(column, corners)
            };
            if scale == 0.0 {
                continue;
            }
            *below = u32::from_ne_bytes(if pixel[3] == u8::MAX && scale == 1.0 {
                pixel
            } else {
                over(pixel, below.to_ne_bytes(), scale)
            });
        }
    }
}

/// Anomi port (patch 18): `pixel` laid over `below` as `draw_pixmap` lays
/// it at full opacity: an empty one leaves it, an opaque one replaces it.
pub(crate) fn laid(pixel: [u8; 4], below: [u8; 4]) -> [u8; 4] {
    match pixel[3] {
        0 if pixel == [0; 4] => below,
        u8::MAX => pixel,
        _ => over(pixel, below, 1.0),
    }
}

/// Anomi port (patches 3, 19): fills the whole pixels from (`left`, `top`)
/// to (`right`, `bottom`) of `pixels` with `color`, source over, as
/// tiny-skia's fill of a rectangle on whole pixels leaves them: an opaque
/// colour written in, a see-through one laid over with its 8-bit pipeline's
/// arithmetic, a clear one leaving them as they were.
pub fn fill(
    (left, top, right, bottom): (i32, i32, i32, i32),
    color: tiny_skia::Color,
    pixels: &mut tiny_skia::PixmapMut<'_>,
) {
    let (width, height) = (pixels.width() as i32, pixels.height() as i32);
    let (left, top) = (left.max(0), top.max(0));
    let (right, bottom) = (right.min(width), bottom.min(height));
    let color = color.premultiply().to_color_u8();
    let pixel = [color.red(), color.green(), color.blue(), color.alpha()];
    if left >= right || top >= bottom || pixel[3] == 0 && pixel == [0; 4] {
        return;
    }
    let stride = width as usize;
    let frame: &mut [u32] = bytemuck::cast_slice_mut(pixels.data_mut());
    let keep = 255 - u16::from(pixel[3]);
    for row in top..bottom {
        let line = &mut frame[row as usize * stride..][left as usize..right as usize];
        if keep == 0 {
            line.fill(u32::from_ne_bytes(pixel));
            continue;
        }
        for below in line {
            let under = below.to_ne_bytes();
            *below = u32::from_ne_bytes(std::array::from_fn(|channel| {
                let mixed =
                    u16::from(pixel[channel]) + ((u16::from(under[channel]) * keep + 255) >> 8);
                mixed.min(255) as u8
            }));
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

    /// A `width` by `height` premultiplied picture: opaque, or with empty
    /// and partly covered pixels among them.
    fn picture_of(noise: &mut Noise, width: u32, height: u32, opaque: bool) -> Vec<u8> {
        (0..width * height)
            .flat_map(|_| {
                let alpha = match (opaque, noise.next() % 4) {
                    (true, _) | (false, 1) => 255,
                    (false, 0) => 0,
                    _ => (noise.next() % 256) as u8,
                };
                let [red, green, blue, _] = noise.next().to_le_bytes();
                let pixel = tiny_skia::ColorU8::from_rgba(red, green, blue, alpha).premultiply();
                [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
            })
            .collect()
    }

    fn frame_of(noise: &mut Noise, width: u32, height: u32) -> Vec<u32> {
        (0..width * height)
            .map(|_| noise.next() | 0xff00_0000)
            .collect()
    }

    fn pixmap(frame: &mut [u32], width: u32, height: u32) -> tiny_skia::PixmapMut<'_> {
        tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(frame), width, height)
            .expect("pixmap")
    }

    #[test]
    fn a_square_picture_at_its_own_size_comes_out_as_draw_pixmap_drew_it() {
        let (width, height) = (64, 40);
        let mut noise = Noise(0x2545_f491);
        let frame = frame_of(&mut noise, width, height);
        let clips = [
            Rectangle::new(
                crate::core::Point::ORIGIN,
                crate::core::Size::new(64.0, 40.0),
            ),
            Rectangle::new(
                crate::core::Point::new(10.5, 7.49),
                crate::core::Size::new(20.2, 18.6),
            ),
        ];
        for opaque in [true, false] {
            let (picture_width, picture_height) = (23, 17);
            let source = picture_of(&mut noise, picture_width, picture_height, opaque);
            let image = tiny_skia::PixmapRef::from_bytes(&source, picture_width, picture_height)
                .expect("picture");
            for opacity in [1.0, 0.5, 0.731] {
                for (x, y) in [(5, 3), (-7, -4), (50, 30)] {
                    for clip in clips {
                        let mut ours = frame.clone();
                        let mut theirs = frame.clone();
                        // Its own clip is the frame: no corners.
                        let rounded = Rounded::new((-100, -100, 200, 200), [8.0; 4]);
                        picture(
                            &source,
                            picture_width,
                            picture_height,
                            (x, y),
                            clip,
                            &rounded,
                            opacity,
                            opaque,
                            &mut pixmap(&mut ours, width, height),
                        );
                        let mut mask = tiny_skia::Mask::new(width, height).expect("mask");
                        crate::clip::Clip::default().set(&mut mask, clip);
                        pixmap(&mut theirs, width, height).draw_pixmap(
                            x,
                            y,
                            image,
                            &tiny_skia::PixmapPaint {
                                opacity,
                                ..tiny_skia::PixmapPaint::default()
                            },
                            tiny_skia::Transform::identity(),
                            Some(&mask),
                        );
                        assert!(
                            ours == theirs,
                            "opaque {opaque}, {opacity}, at {x}, {y}, {clip:?}"
                        );
                    }
                }
            }
        }
    }

    /// An opaque white `side` square drawn on black at (`x`, `y`), cut to
    /// `rounded`: each pixel's coverage, 0 to 255.
    fn cut(side: u32, (x, y): (i32, i32), rounded: &Rounded, opacity: f32) -> Vec<u8> {
        let (width, height) = (24, 24);
        let mut frame = vec![0xff00_0000_u32; (width * height) as usize];
        let source = vec![255; (side * side * 4) as usize];
        let whole = Rectangle::new(
            crate::core::Point::ORIGIN,
            crate::core::Size::new(width as f32, height as f32),
        );
        picture(
            &source,
            side,
            side,
            (x, y),
            whole,
            rounded,
            opacity,
            true,
            &mut pixmap(&mut frame, width, height),
        );
        frame.iter().map(|pixel| pixel.to_ne_bytes()[0]).collect()
    }

    #[test]
    fn a_round_picture_covers_its_circle() {
        let rounded = Rounded::new((2, 2, 18, 18), [8.0; 4]);
        let covered = cut(16, (2, 2), &rounded, 1.0);
        let at = |x: usize, y: usize| covered[y * 24 + x];
        // Outside each corner nothing; along the middle and inside, all.
        for (x, y) in [(2, 2), (17, 2), (17, 17), (2, 17)] {
            assert_eq!(at(x, y), 0, "corner pixel {x}, {y}");
        }
        for (x, y) in [(10, 10), (5, 5), (14, 14)] {
            assert_eq!(at(x, y), 255, "{x}, {y}");
        }
        // The middle of each edge, all but the arc's sliver past its centre.
        for (x, y) in [(10, 2), (2, 10), (17, 9), (9, 17)] {
            assert!((245..255).contains(&at(x, y)), "{x}, {y}: {}", at(x, y));
        }
        // Its area is the circle's, within a pixel.
        let area: f32 = covered.iter().map(|&value| f32::from(value) / 255.0).sum();
        let circle = std::f32::consts::PI * 64.0;
        assert!((area - circle).abs() < 1.0, "{area} against {circle}");
        // Symmetric: each corner cut as the others.
        for y in 0..16 {
            for x in 0..16 {
                let pixel = at(2 + x, 2 + y);
                assert_eq!(pixel, at(17 - x, 2 + y), "mirrored across, {x}, {y}");
                assert_eq!(pixel, at(2 + x, 17 - y), "mirrored down, {x}, {y}");
            }
        }
        // Half see-through, each pixel half of that.
        let half = cut(16, (2, 2), &rounded, 0.5);
        for (full, half) in covered.iter().zip(&half) {
            assert!((f32::from(*full) / 2.0 - f32::from(*half)).abs() <= 1.0);
        }
    }

    #[test]
    fn each_corner_has_its_own_radius_and_they_are_its_clips() {
        // Round at the top left and bottom right only.
        let rounded = Rounded::new((2, 2, 18, 18), [6.0, 0.0, 3.0, 0.0]);
        let covered = cut(16, (2, 2), &rounded, 1.0);
        let at = |x: usize, y: usize| covered[y * 24 + x];
        assert_eq!((at(2, 2), at(17, 17)), (0, 0));
        assert_eq!((at(17, 2), at(2, 17)), (255, 255), "square corners");
        assert_eq!(at(4, 4), 255, "inside the six-pixel corner");
        // A picture larger than its clip is cut at the clip's corners.
        let clip = Rounded::new((6, 6, 14, 14), [3.0; 4]);
        let covered = cut(16, (2, 2), &clip, 1.0);
        let at = |x: usize, y: usize| covered[y * 24 + x];
        assert_eq!((at(5, 10), at(10, 14)), (0, 0), "outside the clip");
        assert_eq!((at(6, 6), at(13, 13)), (0, 0), "its corners");
        assert_eq!((at(6, 9), at(9, 6), at(10, 10)), (255, 255, 255));
        // Radii past half the shorter side are held to it.
        assert_eq!(
            Rounded::new((0, 0, 10, 4), [9.0; 4]),
            Rounded::new((0, 0, 10, 4), [2.0; 4])
        );
    }
}
