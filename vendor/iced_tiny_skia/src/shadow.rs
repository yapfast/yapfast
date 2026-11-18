//! Anomi port (patches 11, 17 and 18): quads' shadows worked out once and
//! laid only where they show.
//!
//! Stock worked out every pixel of a shadow's reach each time its quad was
//! drawn, for every damaged region it touched, into a pixmap of its own,
//! then laid it through tiny-skia's pattern pipeline and the clip mask: a
//! modal's shadow was most of the frame's cost. Here a shadow's pixels are
//! kept as its nine slices (patch 18): the corners whole, each straight
//! stretch of its edges one pixel across, which the pixels along it repeat
//! exactly, and its middle one pixel; the last [`KEPT`] shadows by their
//! shape (patch 17). Only the pixels inside the clip are laid (patch 11),
//! and none under the quad's opaque body, which its fill covers whole. Each
//! pixel is stock's (its formula, at the top-left corner of the pixel, on
//! the grid stock's truncation puts it), laid source over with the
//! arithmetic `draw_pixmap` uses, so the frame comes out byte for byte.

use crate::blit;
use crate::core::{Rectangle, Shadow, Transformation, Vector};
use crate::engine::{into_color, rounded_box_sdf, smoothstep};

/// How many shadows are kept: the hover bar's, a menu's, a popup's, a
/// modal's, with room for the sizes they take as they change.
const KEPT: usize = 8;

/// The shadows worked out lately.
#[derive(Debug, Default)]
pub struct Shadows {
    kept: Vec<(Key, Sliced)>,
}

/// What a shadow's pixels follow from wholly: the quad's place from the
/// grid's corner and its size, the grid's size, the corners, the blur, the
/// offset and the colour, all physical.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Key {
    within: (f32, f32),
    size: (f32, f32),
    pixels: (u32, u32),
    radii: [f32; 4],
    blur: f32,
    offset: (f32, f32),
    color: [f32; 4],
}

/// Where a quad's shadow reaches, logical: the quad moved by the offset
/// and grown by the blur, as stock works it out.
pub fn reach(bounds: Rectangle, shadow: Shadow) -> Rectangle {
    Rectangle {
        x: bounds.x + shadow.offset.x - shadow.blur_radius,
        y: bounds.y + shadow.offset.y - shadow.blur_radius,
        width: bounds.width + shadow.blur_radius * 2.0,
        height: bounds.height + shadow.blur_radius * 2.0,
    }
}

impl Shadows {
    /// Lays the shadow of a quad at `bounds` (logical, `radii` its corners
    /// physical) over the pixels of `clip_bounds` outside `body` (whole
    /// pixels left, top, right, bottom that the quad's fill covers whole).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        bounds: Rectangle,
        radii: [f32; 4],
        shadow: Shadow,
        transformation: Transformation,
        clip_bounds: Rectangle,
        body: Option<(i32, i32, i32, i32)>,
        pixels: &mut tiny_skia::PixmapMut<'_>,
    ) {
        let physical_bounds = bounds * transformation;
        let reach = reach(bounds, shadow) * transformation;
        // Stock's grid: its corner and size truncated.
        let origin = (reach.x as u32, reach.y as u32);
        let size = (reach.width as u32, reach.height as u32);
        let scale = transformation.scale_factor();
        let key = Key {
            within: (
                physical_bounds.x - origin.0 as f32,
                physical_bounds.y - origin.1 as f32,
            ),
            size: (physical_bounds.width, physical_bounds.height),
            pixels: size,
            radii,
            blur: shadow.blur_radius * scale,
            offset: (shadow.offset.x * scale, shadow.offset.y * scale),
            color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a,
            ],
        };
        let at = match self.kept.iter().position(|(known, _)| *known == key) {
            Some(at) => at,
            None => {
                let Some(sliced) = Sliced::new(physical_bounds, origin, size, radii, shadow, scale)
                else {
                    return;
                };
                if self.kept.len() == KEPT {
                    let _ = self.kept.remove(0);
                }
                self.kept.push((key, sliced));
                self.kept.len() - 1
            }
        };
        let (origin, size) = (
            (origin.0 as i32, origin.1 as i32),
            (size.0 as i32, size.1 as i32),
        );
        self.kept[at]
            .1
            .draw(origin, size, clip_bounds, body, pixels);
    }
}

/// One axis of a shadow's grid as its slices keep it: the pixels from
/// `start` to `end`, both in, lie along a straight stretch, where each is
/// the same across, and are kept as the one at `start`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Slices {
    start: u32,
    end: u32,
}

impl Slices {
    /// Along an axis of `pixels` from `origin`, of a box from `near` to
    /// `far` whose corners reach `corner` in. A pixel short of the stretch
    /// on each side is kept whole, so no rounding of its edge counts.
    fn new(origin: u32, pixels: u32, near: f32, far: f32, corner: f32) -> Self {
        let start = ((near + corner - origin as f32).ceil() + 1.0).max(0.0);
        let end = ((far - corner - origin as f32).floor() - 1.0).min(pixels as f32 - 1.0);
        if end > start {
            Self {
                start: start as u32,
                end: end as u32,
            }
        } else {
            Self { start: 0, end: 0 }
        }
    }

    /// How many of `pixels` are kept.
    fn kept(self, pixels: u32) -> u32 {
        pixels - (self.end - self.start)
    }

    /// The pixel of the grid a kept one stands for.
    fn whole(self, kept: u32) -> u32 {
        if kept <= self.start {
            kept
        } else {
            kept + (self.end - self.start)
        }
    }

    /// The kept pixel that stands for a pixel of the grid.
    fn kept_at(self, whole: u32) -> u32 {
        if whole <= self.start {
            whole
        } else if whole <= self.end {
            self.start
        } else {
            whole - (self.end - self.start)
        }
    }
}

/// A shadow's pixels, premultiplied in the frame's byte order, as kept.
#[derive(Debug)]
struct Sliced {
    pixels: Vec<[u8; 4]>,
    width: u32,
    x: Slices,
    y: Slices,
}

impl Sliced {
    /// Works out the shadow over its grid, `size` pixels from `origin`, as
    /// stock does, keeping one pixel of each straight stretch.
    fn new(
        physical_bounds: Rectangle,
        origin: (u32, u32),
        size: (u32, u32),
        radii: [f32; 4],
        shadow: Shadow,
        scale: f32,
    ) -> Option<Self> {
        let half_width = physical_bounds.width / 2.0;
        let half_height = physical_bounds.height / 2.0;
        let half = tiny_skia::Size::from_wh(half_width, half_height)?;
        let corner = radii.iter().fold(0.0f32, |most, radius| most.max(*radius));
        let (offset_x, offset_y) = (shadow.offset.x * scale, shadow.offset.y * scale);
        let near = (physical_bounds.x + offset_x, physical_bounds.y + offset_y);
        let x = Slices::new(
            origin.0,
            size.0,
            near.0,
            near.0 + physical_bounds.width,
            corner,
        );
        let y = Slices::new(
            origin.1,
            size.1,
            near.1,
            near.1 + physical_bounds.height,
            corner,
        );
        let (width, height) = (x.kept(size.0), y.kept(size.1));
        if width == 0 || height == 0 {
            return None;
        }
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for row in 0..height {
            let at_y = (origin.1 + y.whole(row)) as f32;
            for column in 0..width {
                let at_x = (origin.0 + x.whole(column)) as f32;
                // Stock's very expression, so each pixel is its own.
                let distance = rounded_box_sdf(
                    Vector::new(
                        at_x - physical_bounds.position().x - offset_x - half_width,
                        at_y - physical_bounds.position().y - offset_y - half_height,
                    ),
                    half,
                    &radii,
                )
                .max(0.0);
                let alpha = 1.0
                    - smoothstep(
                        -shadow.blur_radius * scale,
                        shadow.blur_radius * scale,
                        distance,
                    );
                let mut color = into_color(shadow.color);
                color.apply_opacity(alpha);
                let pixel = color.to_color_u8().premultiply();
                pixels.push([pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]);
            }
        }
        Some(Self {
            pixels,
            width,
            x,
            y,
        })
    }

    /// Lays the shadow, its grid `size` pixels from `origin`, over the
    /// pixels of `clip_bounds` (each edge rounded, as the mask holds it)
    /// outside `body`.
    fn draw(
        &self,
        origin: (i32, i32),
        size: (i32, i32),
        clip_bounds: Rectangle,
        body: Option<(i32, i32, i32, i32)>,
        pixels: &mut tiny_skia::PixmapMut<'_>,
    ) {
        let (frame_width, frame_height) = (pixels.width() as i32, pixels.height() as i32);
        let left = origin.0.max(clip_bounds.x.round() as i32).max(0);
        let top = origin.1.max(clip_bounds.y.round() as i32).max(0);
        let right = (origin.0 + size.0)
            .min((clip_bounds.x + clip_bounds.width).round() as i32)
            .min(frame_width);
        let bottom = (origin.1 + size.1)
            .min((clip_bounds.y + clip_bounds.height).round() as i32)
            .min(frame_height);
        if left >= right || top >= bottom {
            return;
        }
        let stride = frame_width as usize;
        let frame: &mut [u32] = bytemuck::cast_slice_mut(pixels.data_mut());
        for row in top..bottom {
            let kept = self.y.kept_at((row - origin.1) as u32) as usize;
            let source = &self.pixels[kept * self.width as usize..][..self.width as usize];
            // The columns the body hides in this row.
            let hidden = body
                .filter(|&(_, body_top, _, body_bottom)| (body_top..body_bottom).contains(&row))
                .map_or((right, right), |(body_left, _, body_right, _)| {
                    (body_left.clamp(left, right), body_right.clamp(left, right))
                });
            let line = &mut frame[row as usize * stride..][..stride];
            for column in (left..hidden.0).chain(hidden.1..right) {
                let pixel = source[self.x.kept_at((column - origin.0) as u32) as usize];
                let below = &mut line[column as usize];
                *below = u32::from_ne_bytes(blit::laid(pixel, below.to_ne_bytes()));
            }
        }
    }
}

#[cfg(test)]
mod tests;
