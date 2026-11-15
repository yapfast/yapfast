//! Yapfast port: copying premultiplied pixels onto the frame directly.
//!
//! tiny-skia draws a pixmap through its pattern shader in the high-precision
//! (f32) pipeline, several times the cost per pixel of a plain source-over
//! copy, and clips it with a window-sized mask. Pictures drawn at their own
//! size and glyphs, both on whole pixels, are copied in here instead,
//! clipped to a rectangle.

use crate::core::Rectangle;

/// Copies `source`, `width` by `height` premultiplied pixels in the frame's
/// byte order, onto `pixels` at (`x`, `y`), source over, within
/// `clip_bounds`, with its corners cut round to `radius` (top left, top
/// right, bottom right, bottom left) and scaled by `opacity` (256 is opaque).
/// `opaque` says every source pixel is, which spares looking at each row.
#[allow(clippy::too_many_arguments)]
pub fn blit(
    source: &[u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    clip_bounds: Rectangle,
    radius: [f32; 4],
    opacity: u32,
    opaque: bool,
    pixels: &mut tiny_skia::PixmapMut<'_>,
) {
    let (width, height) = (width as i32, height as i32);
    let (target_width, target_height) =
        (pixels.width() as i32, pixels.height() as i32);
    let x0 = x.max(clip_bounds.x.round() as i32).max(0);
    let y0 = y.max(clip_bounds.y.round() as i32).max(0);
    let x1 = (x + width)
        .min((clip_bounds.x + clip_bounds.width).round() as i32)
        .min(target_width);
    let y1 = (y + height)
        .min((clip_bounds.y + clip_bounds.height).round() as i32)
        .min(target_height);
    if x0 >= x1 || y0 >= y1 || opacity == 0 {
        return;
    }
    // Yapfast port (round 3): each corner its own radius, as a card's
    // banner has round top corners only.
    let radius = radius.map(|radius| {
        radius
            .min(width as f32 / 2.0)
            .min(height as f32 / 2.0)
            .max(0.0)
    });
    let [top_left, top_right, bottom_right, bottom_left] =
        radius.map(|radius| radius.ceil() as i32);
    let top = top_left.max(top_right);
    let bottom = bottom_left.max(bottom_right);
    let stride = pixels.width() as usize * 4;
    let target = pixels.data_mut();
    for row in y0..y1 {
        let local_y = row - y;
        // The corners this row passes through: (radius, cells from the
        // corner's edge row) for the left and the right one.
        let (left, right) = if local_y < top {
            (
                (local_y < top_left).then_some((radius[0], local_y)),
                (local_y < top_right).then_some((radius[1], local_y)),
            )
        } else if local_y >= height - bottom {
            let from_bottom = height - 1 - local_y;
            (
                (from_bottom < bottom_left).then_some((radius[3], from_bottom)),
                (from_bottom < bottom_right).then_some((radius[2], from_bottom)),
            )
        } else {
            (None, None)
        };
        let from = (local_y * width + (x0 - x)) as usize * 4;
        let to = row as usize * stride + x0 as usize * 4;
        let count = (x1 - x0) as usize;
        let source_row = &source[from..from + count * 4];
        let target_row = &mut target[to..to + count * 4];
        // Most rows of most pictures are opaque: one copy.
        if left.is_none()
            && right.is_none()
            && opacity == 256
            && (opaque || source_row.chunks_exact(4).all(|pixel| pixel[3] == 255))
        {
            target_row.copy_from_slice(source_row);
            continue;
        }
        for index in 0..count {
            let at = index * 4;
            let mut pixel = u32::from_le_bytes(
                source_row[at..at + 4].try_into().expect("four bytes"),
            );
            if pixel >> 24 == 0 {
                continue;
            }
            let mut scale = opacity;
            let local_x = x0 - x + index as i32;
            let corner = match (left, right) {
                (Some((radius, cell_y)), _) if local_x < radius.ceil() as i32 => {
                    Some((radius, local_x, cell_y))
                }
                (_, Some((radius, cell_y)))
                    if local_x >= width - radius.ceil() as i32 =>
                {
                    Some((radius, width - 1 - local_x, cell_y))
                }
                _ => None,
            };
            if let Some((radius, cell_x, cell_y)) = corner {
                scale = scale * coverage(radius, cell_x, cell_y) >> 8;
            }
            if scale < 256 {
                pixel = scaled(pixel, scale);
            }
            let alpha = pixel >> 24;
            if alpha == 0 {
                continue;
            }
            let target = &mut target_row[at..at + 4];
            if alpha == 255 {
                target.copy_from_slice(&pixel.to_le_bytes());
                continue;
            }
            let below = u32::from_le_bytes(target[..4].try_into().expect("four bytes"));
            let blended = pixel + times_255ths(below, 255 - alpha);
            target.copy_from_slice(&blended.to_le_bytes());
        }
    }
}

/// Yapfast port (patch 24): `source`, `width` by `height` premultiplied
/// pixels, scaled to `to_width` by `to_height`: each pixel's centre sampled
/// between the four source pixels round it (`nearest`: the one it falls
/// in), as tiny-skia's bilinear filter samples, in the same byte order.
pub fn resized(
    source: &[u8],
    width: u32,
    height: u32,
    to_width: u32,
    to_height: u32,
    nearest: bool,
) -> Vec<u8> {
    if width == 0 || height == 0 || to_width == 0 || to_height == 0 {
        return Vec::new();
    }
    // Where each target column or row samples, in 16.16 source pixels:
    // the target pixel's centre mapped back, less half a source pixel
    // between centres (the pixel it falls in, for `nearest`): the source
    // pixel before it, the one after and how far towards it, of 256.
    let samples = |from: u32, to: u32| -> Vec<(usize, usize, u32)> {
        let step = (i64::from(from) << 16) / i64::from(to);
        let start = step / 2 - if nearest { 0 } else { 0x8000 };
        let last = i64::from(from - 1) << 16;
        (0..i64::from(to))
            .map(|at| {
                let at = (start + at * step).clamp(0, last);
                let index = (at >> 16) as usize;
                let weight = if nearest { 0 } else { ((at >> 8) & 0xff) as u32 };
                (index, (index + 1).min(from as usize - 1), weight)
            })
            .collect()
    };
    let (columns, rows) = (samples(width, to_width), samples(height, to_height));
    let pixel = |row: usize, column: usize| {
        let at = (row * width as usize + column) * 4;
        u32::from_le_bytes([source[at], source[at + 1], source[at + 2], source[at + 3]])
    };
    let mut pixels = Vec::with_capacity(to_width as usize * to_height as usize * 4);
    for &(above, below, down) in &rows {
        for &(left, right, across) in &columns {
            let top = lerp(pixel(above, left), pixel(above, right), across);
            let mixed = if down == 0 {
                top
            } else {
                lerp(top, lerp(pixel(below, left), pixel(below, right), across), down)
            };
            pixels.extend_from_slice(&mixed.to_le_bytes());
        }
    }
    pixels
}

/// Two premultiplied pixels mixed, `weight` / 256 of the way to `to`.
#[inline(always)]
fn lerp(from: u32, to: u32, weight: u32) -> u32 {
    if weight == 0 || from == to {
        return from;
    }
    // Rounded: each lane's sum is at most 255 * 256 + 128, inside its 16
    // bits.
    let keep = 256 - weight;
    let mix = |from: u32, to: u32| (from * keep + to * weight + 0x0080_0080) >> 8 & 0x00ff_00ff;
    let even = mix(from & 0x00ff_00ff, to & 0x00ff_00ff);
    let odd = mix((from >> 8) & 0x00ff_00ff, (to >> 8) & 0x00ff_00ff);
    even | odd << 8
}

/// How much of the cell `cell_x`, `cell_y` from a corner's edges a round
/// corner of `radius` covers, 0 to 256.
fn coverage(radius: f32, cell_x: i32, cell_y: i32) -> u32 {
    let dx = radius - (cell_x as f32 + 0.5);
    let dy = radius - (cell_y as f32 + 0.5);
    let covered = if dx <= 0.0 || dy <= 0.0 {
        1.0
    } else {
        (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0)
    };
    (covered * 256.0) as u32
}

/// Each channel of a pixel times `scale` / 256.
fn scaled(pixel: u32, scale: u32) -> u32 {
    let even = ((pixel & 0x00ff_00ff) * scale >> 8) & 0x00ff_00ff;
    let odd = (((pixel >> 8) & 0x00ff_00ff) * scale >> 8) & 0x00ff_00ff;
    even | odd << 8
}

/// Yapfast port (patch 19): fills the whole pixels from (`left`, `top`) to
/// (`right`, `bottom`) of `pixels` with `color`, straight, laid source over,
/// or put in place of what's there (`replace`). tiny-skia filled a plain
/// rectangle through its pipeline, a span at a time.
pub fn fill(
    (left, top, right, bottom): (i32, i32, i32, i32),
    color: tiny_skia::Color,
    replace: bool,
    pixels: &mut tiny_skia::PixmapMut<'_>,
) {
    let (width, height) = (pixels.width() as i32, pixels.height() as i32);
    let (left, top) = (left.max(0), top.max(0));
    let (right, bottom) = (right.min(width), bottom.min(height));
    if left >= right || top >= bottom {
        return;
    }
    let color = color.premultiply().to_color_u8();
    let pixel = u32::from_le_bytes([color.red(), color.green(), color.blue(), color.alpha()]);
    if pixel >> 24 == 0 && !replace {
        return;
    }
    let stride = width as usize;
    let frame: &mut [u32] = bytemuck::cast_slice_mut(pixels.data_mut());
    for row in top..bottom {
        let row = &mut frame[row as usize * stride..][left as usize..right as usize];
        if replace || pixel >> 24 == 255 {
            row.fill(pixel);
        } else {
            for below in row {
                *below = over(pixel, *below);
            }
        }
    }
}

/// Yapfast port (patch 18): a premultiplied pixel laid source over another.
pub fn over(pixel: u32, below: u32) -> u32 {
    match pixel >> 24 {
        0 => below,
        255 => pixel,
        alpha => pixel + times_255ths(below, 255 - alpha),
    }
}

/// Each channel of a pixel times `factor` / 255, rounded.
fn times_255ths(pixel: u32, factor: u32) -> u32 {
    let divide = |value: u32| ((value + 0x0080_0080 + ((value >> 8) & 0x00ff_00ff)) >> 8) & 0x00ff_00ff;
    let even = divide((pixel & 0x00ff_00ff) * factor);
    let odd = divide(((pixel >> 8) & 0x00ff_00ff) * factor);
    even | odd << 8
}

/// Yapfast port: fills a linear gradient, `width` by `height` at (`x`, `y`)
/// on whole pixels, onto `pixels`, source over, within `clip_bounds`, with
/// its corners cut round to `radius` (top left, top right, bottom right,
/// bottom left). As tiny-skia's linear gradient with `SpreadMode::Pad`
/// colours each pixel's centre: the stops' straight colours interpolated
/// along `start` to `end`, then premultiplied. `stops` are (offset, colour
/// as the frame orders it, straight), offsets ascending.
///
/// Stock, a gradient quad was an anti-aliased path through tiny-skia's f32
/// pipeline, over its whole area for every damaged region it touched, and
/// masked: the nameplates' fades made a scrolled member list cost half a
/// core.
#[allow(clippy::too_many_arguments)]
pub fn gradient(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    clip_bounds: Rectangle,
    radius: [f32; 4],
    start: (f32, f32),
    end: (f32, f32),
    stops: &[(f32, [f32; 4])],
    pixels: &mut tiny_skia::PixmapMut<'_>,
) {
    let (target_width, target_height) =
        (pixels.width() as i32, pixels.height() as i32);
    let x0 = x.max(clip_bounds.x.round() as i32).max(0);
    let y0 = y.max(clip_bounds.y.round() as i32).max(0);
    let x1 = (x + width)
        .min((clip_bounds.x + clip_bounds.width).round() as i32)
        .min(target_width);
    let y1 = (y + height)
        .min((clip_bounds.y + clip_bounds.height).round() as i32)
        .min(target_height);
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return;
    };
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    // The colour at `t` along the gradient, premultiplied, in the frame's
    // byte order, as a pixel.
    let color = |t: f32| -> u32 {
        let t = t.clamp(0.0, 1.0);
        let [c0, c1, c2, a] = if t <= first.0 {
            first.1
        } else if t >= last.0 {
            last.1
        } else {
            let next = stops.iter().position(|stop| stop.0 > t).unwrap_or(stops.len() - 1);
            let (from, to) = (stops[next - 1], stops[next]);
            let f = (t - from.0) / (to.0 - from.0);
            std::array::from_fn(|channel| from.1[channel] + (to.1[channel] - from.1[channel]) * f)
        };
        let byte = |value: f32| (value * 255.0 + 0.5).clamp(0.0, 255.0) as u32;
        byte(c0 * a) | byte(c1 * a) << 8 | byte(c2 * a) << 16 | byte(a) << 24
    };
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let length = dx * dx + dy * dy;
    let at = |px: i32, py: i32| {
        if length <= 0.0 {
            return 0.0;
        }
        ((px as f32 + 0.5 - start.0) * dx + (py as f32 + 0.5 - start.1) * dy) / length
    };
    let radius = radius.map(|radius| {
        radius
            .min(width as f32 / 2.0)
            .min(height as f32 / 2.0)
            .max(0.0)
    });
    let [top_left, top_right, bottom_right, bottom_left] =
        radius.map(|radius| radius.ceil() as i32);
    let top = top_left.max(top_right);
    let bottom = bottom_left.max(bottom_right);
    let stride = pixels.width() as usize * 4;
    let target = pixels.data_mut();
    // A gradient along x has the same colours on every row: made once.
    let across = dy == 0.0;
    let mut colors: Vec<u32> = Vec::new();
    let mut opaque = false;
    for row in y0..y1 {
        if !across || colors.is_empty() {
            colors.clear();
            colors.extend((x0..x1).map(|column| color(at(column, row))));
            opaque = colors.iter().all(|pixel| pixel >> 24 == 255);
        }
        let local_y = row - y;
        let (left, right) = if local_y < top {
            (
                (local_y < top_left).then_some((radius[0], local_y)),
                (local_y < top_right).then_some((radius[1], local_y)),
            )
        } else if local_y >= height - bottom {
            let from_bottom = height - 1 - local_y;
            (
                (from_bottom < bottom_left).then_some((radius[3], from_bottom)),
                (from_bottom < bottom_right).then_some((radius[2], from_bottom)),
            )
        } else {
            (None, None)
        };
        let to = row as usize * stride + x0 as usize * 4;
        let target_row = &mut target[to..to + (x1 - x0) as usize * 4];
        // Yapfast port (patch 12): an opaque row clear of the corners, as
        // most of a panel's are, is one copy.
        if opaque && left.is_none() && right.is_none() && cfg!(target_endian = "little") {
            target_row.copy_from_slice(bytemuck::cast_slice(&colors));
            continue;
        }
        for (index, &pixel) in colors.iter().enumerate() {
            let mut pixel = pixel;
            let local_x = x0 - x + index as i32;
            let corner = match (left, right) {
                (Some((radius, cell_y)), _) if local_x < radius.ceil() as i32 => {
                    Some((radius, local_x, cell_y))
                }
                (_, Some((radius, cell_y)))
                    if local_x >= width - radius.ceil() as i32 =>
                {
                    Some((radius, width - 1 - local_x, cell_y))
                }
                _ => None,
            };
            if let Some((radius, cell_x, cell_y)) = corner {
                pixel = scaled(pixel, coverage(radius, cell_x, cell_y));
            }
            let alpha = pixel >> 24;
            if alpha == 0 {
                continue;
            }
            let target = &mut target_row[index * 4..index * 4 + 4];
            if alpha == 255 {
                target.copy_from_slice(&pixel.to_le_bytes());
                continue;
            }
            let below = u32::from_le_bytes(target[..4].try_into().expect("four bytes"));
            let blended = pixel + times_255ths(below, 255 - alpha);
            target.copy_from_slice(&blended.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(values: &[u32]) -> Vec<u8> {
        values.iter().flat_map(|value| value.to_le_bytes()).collect()
    }

    fn values(bytes: &[u8]) -> Vec<u32> {
        bytes
            .chunks_exact(4)
            .map(|pixel| u32::from_le_bytes(pixel.try_into().expect("four bytes")))
            .collect()
    }

    #[test]
    fn a_picture_resized_samples_between_the_pixels_round_each_centre() {
        let (black, white) = (0xff00_0000_u32, 0xffff_ffff_u32);
        let source = pixels(&[black, white, black, white]);
        // At its own size it's the same.
        assert_eq!(resized(&source, 2, 2, 2, 2, false), source);
        // Twice as wide: the outer pixels are the edges', the inner ones a
        // quarter and three quarters of the way across.
        let row = values(&resized(&source, 2, 2, 4, 2, false));
        assert_eq!(row[0], black);
        assert_eq!(row[3], white);
        assert_eq!(row[1], 0xff40_4040);
        assert_eq!(row[2], 0xffbf_bfbf);
        assert_eq!(row[..4], row[4..]);
        // Nearest takes the pixel each centre falls in.
        let nearest = values(&resized(&source, 2, 2, 4, 2, true));
        assert_eq!(nearest[..4], [black, black, white, white]);
        // One colour stays that colour, see-through or not.
        let half = 0x8040_2010_u32;
        let flat = values(&resized(&pixels(&[half; 6]), 3, 2, 7, 5, false));
        assert!(flat.iter().all(|&pixel| pixel == half));
    }

    #[test]
    fn a_fill_lays_its_colour_over_or_in_place_inside_the_frame() {
        let ground = 0xff10_2030_u32;
        let mut frame = vec![ground; 8 * 4];
        let mut pixels =
            tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(&mut frame), 8, 4).expect("pixmap");
        let half = tiny_skia::Color::from_rgba(1.0, 0.0, 0.0, 0.5).expect("colour");
        // Past the frame's edge, it stops at it.
        fill((-2, 1, 3, 9), half, false, &mut pixels);
        fill((6, 0, 20, 1), half, true, &mut pixels);
        let pixel = {
            let color = half.premultiply().to_color_u8();
            u32::from_le_bytes([color.red(), color.green(), color.blue(), color.alpha()])
        };
        for (at, &value) in frame.iter().enumerate() {
            let (x, y) = (at % 8, at / 8);
            let expected = if x < 3 && y >= 1 {
                over(pixel, ground)
            } else if x >= 6 && y == 0 {
                pixel
            } else {
                ground
            };
            assert_eq!(value, expected, "at {x}, {y}");
        }
    }
}
