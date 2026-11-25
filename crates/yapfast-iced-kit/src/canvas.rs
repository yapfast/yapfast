//! Pixels the kit makes itself, in the renderer's own format, so the
//! renderer takes them as they are: a [`Canvas`] drawn in premultiplied
//! BGRA, the icons rasterised from SVG, and the handle iced draws.
//!
//! The format: on the CPU, tiny-skia's premultiplied BGRA, which the
//! patched renderer copies onto the frame; on the GPU, the straight
//! RGBA iced's `Handle::from_rgba` means, which iced_wgpu uploads to its
//! atlas as it is. A [`Canvas`] is drawn in premultiplied BGRA either way
//! and turned into the format once, by [`finish`].

use iced::advanced::image::Handle;
use resvg::{tiny_skia, usvg};

/// A colour as 0xRRGGBB and an alpha from 0 to 1.
#[derive(Clone, Copy)]
pub struct Ink(pub u32, pub f32);

/// A premultiplied BGRA picture being made.
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        }
    }

    /// Lays `ink` over the pixel at `coverage` (0 to 1), source over.
    pub fn over(&mut self, x: u32, y: u32, Ink(rgb, alpha): Ink, coverage: f32) {
        let alpha = (alpha * coverage).clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let at = ((y * self.width + x) * 4) as usize;
        let [r, g, b] =
            [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8].map(|c| f32::from(c) * alpha);
        let pixel = &mut self.pixels[at..at + 4];
        let keep = 1.0 - alpha;
        for (channel, value) in pixel.iter_mut().zip([b, g, r, alpha * 255.0]) {
            *channel = (value + f32::from(*channel) * keep).round().min(255.0) as u8;
        }
    }

    /// A rectangle with round corners, edges smoothed.
    pub fn rounded(&mut self, x: f32, y: f32, width: f32, height: f32, radius: f32, ink: Ink) {
        let radius = radius.min(width / 2.0).min(height / 2.0);
        let (x0, y0) = (x.floor().max(0.0) as u32, y.floor().max(0.0) as u32);
        let x1 = ((x + width).ceil() as u32).min(self.width);
        let y1 = ((y + height).ceil() as u32).min(self.height);
        let (cx, cy) = (x + width / 2.0, y + height / 2.0);
        let (hw, hh) = (width / 2.0 - radius, height / 2.0 - radius);
        for py in y0..y1 {
            for px in x0..x1 {
                // Signed distance from the rounded rectangle's edge.
                let dx = ((px as f32 + 0.5 - cx).abs() - hw).max(0.0);
                let dy = ((py as f32 + 0.5 - cy).abs() - hh).max(0.0);
                let inside_x = (px as f32 + 0.5 - cx).abs() - hw;
                let inside_y = (py as f32 + 0.5 - cy).abs() - hh;
                let distance =
                    (dx * dx + dy * dy).sqrt() + inside_x.max(inside_y).min(0.0) - radius;
                self.over(px, py, ink, (0.5 - distance).clamp(0.0, 1.0));
            }
        }
    }

    pub fn circle(&mut self, x: f32, y: f32, size: f32, ink: Ink) {
        self.rounded(x, y, size, size, size / 2.0, ink);
    }

    /// Clears pixels inside a circle of `radius` centred at `(cx, cy)`,
    /// smoothing the edge.
    pub fn erase_circle(&mut self, cx: f32, cy: f32, radius: f32) {
        let x0 = (cx - radius - 1.0).floor().max(0.0) as u32;
        let y0 = (cy - radius - 1.0).floor().max(0.0) as u32;
        let x1 = ((cx + radius + 1.0).ceil() as u32).min(self.width);
        let y1 = ((cy + radius + 1.0).ceil() as u32).min(self.height);
        for py in y0..y1 {
            for px in x0..x1 {
                let d = (px as f32 + 0.5 - cx).hypot(py as f32 + 0.5 - cy) - radius;
                let keep = (d + 0.5).clamp(0.0, 1.0);
                if keep < 1.0 {
                    let at = ((py * self.width + px) * 4) as usize;
                    for channel in &mut self.pixels[at..at + 4] {
                        *channel = (f32::from(*channel) * keep).round().min(255.0) as u8;
                    }
                }
            }
        }
    }
}

/// A canvas's premultiplied BGRA to the renderer's format, in place: on the
/// GPU, straight RGBA.
pub fn finish(pixels: &mut [u8]) {
    if !crate::renderer::gpu() {
        return;
    }
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let [b, g, r, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
        let straight = |c: u8| match a {
            0 => 0,
            255 => c,
            _ => ((u16::from(c) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8,
        };
        pixel.copy_from_slice(&[straight(r), straight(g), straight(b), a]);
    }
}

/// Decoded pixels (straight RGBA) to the renderer's format, in place: on
/// the CPU, premultiplied BGRA; on the GPU they already are.
pub fn to_native(pixels: &mut [u8]) {
    if crate::renderer::gpu() {
        return;
    }
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
        if a == 255 {
            pixel.copy_from_slice(&[b, g, r, a]);
        } else {
            let scale = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
            pixel.copy_from_slice(&[scale(b), scale(g), scale(r), a]);
        }
    }
}

/// An SVG `size` square in its own colours, premultiplied BGRA.
pub fn svg_drawn(svg: &str, size: u32) -> Canvas {
    let mut canvas = Canvas::new(size, size);
    let Ok(tree) = usvg::Tree::from_str(svg, &usvg::Options::default()) else {
        return canvas;
    };
    let Some(mut pixmap) = tiny_skia::Pixmap::new(size, size) else {
        return canvas;
    };
    let scale = size as f32 / tree.size().width().max(1.0);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    canvas.pixels = pixmap.take();
    for pixel in canvas.pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    canvas
}

/// One of the kit's icons (`icons/`, a 24 by 24 view box) drawn whole by
/// resvg, strokes, circles, groups and transforms included, in `rgb` at
/// `size` pixels: premultiplied BGRA, as a `Canvas` is. resvg here has no
/// text, so it makes no font database.
pub fn svg_icon(svg: &str, size: u32, rgb: u32) -> Canvas {
    let mut canvas = Canvas::new(size, size);
    let color = format!("#{rgb:06x}");
    let svg = svg.replace("currentColor", &color);
    let Ok(tree) = usvg::Tree::from_str(&svg, &usvg::Options::default()) else {
        return canvas;
    };
    let Some(mut pixmap) = tiny_skia::Pixmap::new(size, size) else {
        return canvas;
    };
    let scale = size as f32 / tree.size().width().max(1.0);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    canvas.pixels = pixmap.take();
    // Every pixel in the one colour, by its coverage: an icon drawn in
    // white (the logo's mask) comes out the same as one in `currentColor`.
    let [r, g, b] = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
    let scale = |c: u8, a: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
    for pixel in canvas.pixels.as_chunks_mut::<4>().0 {
        let a = pixel[3];
        pixel.copy_from_slice(&[scale(b, a), scale(g, a), scale(r, a), a]);
    }
    canvas
}

/// A handle on pixels the client made, in the renderer's format.
pub fn handle(mut canvas: Canvas) -> Handle {
    finish(&mut canvas.pixels);
    Handle::from_rgba(canvas.width, canvas.height, canvas.pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drawn_icon_keeps_its_own_colours() {
        let svg = include_str!("../icons/verified.svg");
        let canvas = svg_drawn(svg, 16);
        let at = |x: usize, y: usize| {
            let i = (y * 16 + x) * 4;
            let [b, g, r, a]: [u8; 4] = canvas.pixels[i..i + 4].try_into().unwrap();
            (r, g, b, a)
        };
        // Discord's green round the edge, the white tick through the middle.
        assert_eq!(at(8, 2), (0x23, 0xa5, 0x5a, 255));
        let (r, g, b, _) = at(7, 10);
        assert!(r > 200 && g > 200 && b > 200, "{:?}", at(7, 10));
    }
}
