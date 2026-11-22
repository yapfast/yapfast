//! Pixels the client makes itself, once, in the renderer's own format, so the
//! renderer takes them as they are: status dots, icons, pictures fitted to
//! the size they show at, Twemoji's pictures, and display names with their
//! style's effects (gradients and strokes, which iced's text can't draw).
//!
//! The format: on the CPU, tiny-skia's premultiplied BGRA (see
//! `pictures::install`), which the patched renderer copies onto the frame;
//! on the GPU, the straight RGBA iced's `Handle::from_rgba` means,
//! which iced_wgpu uploads to its atlas as it is. A [`Canvas`] is drawn in
//! premultiplied BGRA either way and turned into the format once, by
//! [`finish`].

use iced::advanced::graphics::text::cosmic_text;
use yapfast_core::chat::Presence;

pub use crate::canvas::{Canvas, Ink, finish, to_native};
use crate::{fonts::TWEMOJI, looks::NameLook};

/// Discord's presence dot at 16 px, on a ring of `ring`: online, idle's
/// moon, do not disturb's bar, and the phone of someone online on mobile,
/// which stands 5 px taller (its picture is 16 by 21, drawn 5 px higher),
/// and offline's `faint` dot with a hole, which invisible shows too. Where
/// offline shows no dot (lists of members), the caller leaves it off.
pub fn status_dot(presence: Presence, ring: u32, live: u32, faint: u32) -> Option<Canvas> {
    let ring = Ink(ring, 1.0);
    if presence == Presence::Mobile {
        let mut canvas = Canvas::new(16, 21);
        canvas.rounded(0.0, 0.0, 16.0, 21.0, 4.0, ring);
        canvas.rounded(3.0, 3.0, 10.0, 15.0, 2.0, Ink(live, 1.0));
        canvas.rounded(5.0, 5.0, 6.0, 9.0, 1.0, ring);
        canvas.circle(7.0, 15.0, 2.0, ring);
        return Some(canvas);
    }
    let mut canvas = Canvas::new(16, 16);
    canvas.circle(0.0, 0.0, 16.0, ring);
    match presence {
        Presence::Online => canvas.circle(3.0, 3.0, 10.0, Ink(live, 1.0)),
        Presence::Idle => {
            canvas.circle(2.0, 2.0, 12.0, Ink(0xf0b232, 1.0));
            canvas.circle(1.0, 1.0, 7.0, ring);
        }
        Presence::DoNotDisturb => {
            canvas.circle(3.0, 3.0, 10.0, Ink(0xf23f43, 1.0));
            canvas.rounded(5.0, 7.0, 6.0, 2.0, 1.0, ring);
        }
        Presence::Offline => {
            canvas.circle(3.0, 3.0, 10.0, Ink(faint, 1.0));
            canvas.circle(6.0, 6.0, 4.0, ring);
        }
        Presence::Mobile => return None,
    }
    Some(canvas)
}

/// A picture in the renderer's format fitted to `width` by `height` by area
/// averaging: scaled to fit inside it about its middle, keeping its shape,
/// with the rest see-through (a tall custom emoji in a square stays tall),
/// or, with `cover`, scaled to fill it and cut to it about its middle.
/// Premultiplied pixels (the CPU's) average as they are; straight ones (the
/// GPU's) are weighted by their alpha, so see-through pixels lend no colour
/// to the edges (a decoration's).
pub fn fitted(source: &[u8], from: (u32, u32), to: (u32, u32), cover: bool) -> Vec<u8> {
    let ((source_width, source_height), (width, height)) = (from, to);
    if !cover {
        let scale = f32::min(
            width as f32 / source_width as f32,
            height as f32 / source_height as f32,
        );
        let inner = (
            ((source_width as f32 * scale).round() as u32).clamp(1, width),
            ((source_height as f32 * scale).round() as u32).clamp(1, height),
        );
        if inner != to {
            let scaled = fitted(source, from, inner, true);
            let (left, top) = ((width - inner.0) / 2, (height - inner.1) / 2);
            let mut out = vec![0u8; (width * height * 4) as usize];
            let row = (inner.0 * 4) as usize;
            for (y, line) in scaled.chunks_exact(row).enumerate() {
                let at = (((top + y as u32) * width + left) * 4) as usize;
                out[at..at + row].copy_from_slice(line);
            }
            return out;
        }
    }
    let (mut crop_x, mut crop_y) = (0.0, 0.0);
    let (mut crop_width, mut crop_height) = (source_width as f32, source_height as f32);
    if cover {
        let scale = f32::max(
            width as f32 / source_width as f32,
            height as f32 / source_height as f32,
        );
        crop_width = width as f32 / scale;
        crop_height = height as f32 / scale;
        crop_x = (source_width as f32 - crop_width) / 2.0;
        crop_y = (source_height as f32 - crop_height) / 2.0;
    }
    let (sx, sy) = (crop_width / width as f32, crop_height / height as f32);
    // Straight pixels (the GPU's) are weighted by their alpha.
    let straight = crate::renderer::gpu();
    let mut out = vec![0u8; (width * height * 4) as usize];
    let span = |start: f32, step: f32, index: u32, limit: u32| {
        let from = ((start + index as f32 * step) as u32).min(limit - 1);
        let to = ((start + (index + 1) as f32 * step).ceil() as u32).clamp(from + 1, limit);
        (from, to)
    };
    for y in 0..height {
        let (y0, y1) = span(crop_y, sy, y, source_height);
        for x in 0..width {
            let (x0, x1) = span(crop_x, sx, x, source_width);
            let mut sum = [0u32; 4];
            for row in y0..y1 {
                let start = ((row * source_width + x0) * 4) as usize;
                for pixel in source[start..start + ((x1 - x0) * 4) as usize]
                    .as_chunks::<4>()
                    .0
                {
                    let weight = if straight { u32::from(pixel[3]) } else { 1 };
                    for channel in 0..3 {
                        sum[channel] += u32::from(pixel[channel]) * weight;
                    }
                    sum[3] += u32::from(pixel[3]);
                }
            }
            let count = (y1 - y0) * (x1 - x0);
            let at = ((y * width + x) * 4) as usize;
            let colour_count = if straight { sum[3] } else { count };
            for channel in 0..3 {
                out[at + channel] = (sum[channel] + colour_count / 2)
                    .checked_div(colour_count)
                    .unwrap_or(0) as u8;
            }
            out[at + 3] = ((sum[3] + count / 2) / count) as u8;
        }
    }
    out
}

/// A display name drawn whole with its style, as on a profile card: the
/// colours, gradients, strokes and stacked copies of each effect. The text
/// box (the line's height, the text's width) starts `margin` pixels in from
/// the picture's top left, so strokes and halos have room around it.
pub struct StyledPicture {
    pub canvas: Canvas,
    pub margin: u32,
    /// The text box's size.
    pub width: f32,
    pub height: f32,
}

pub fn styled_name(
    font_system: &mut cosmic_text::FontSystem,
    text: &str,
    look: &NameLook,
    size: f32,
    weight: u16,
) -> StyledPicture {
    use cosmic_text::{Attrs, Buffer, Family, Metrics, Shaping, SwashCache, SwashContent, Weight};
    let em = size;
    let line_height = (size * 1.3).ceil();
    let family = look.family.map_or(Family::Name("DM Sans"), Family::Name);
    let weight = if look.family.is_some() {
        look.weight
    } else {
        weight
    };
    let attrs = Attrs::new().family(family).weight(Weight(weight));
    let mut buffer = Buffer::new(font_system, Metrics::new(size, line_height));
    buffer.set_size(font_system, None, None);
    buffer.set_text(font_system, text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    // Room for the widest stroke or halo, and pop's lowered copy.
    let margin = (em * 0.12 + 6.0).ceil() as u32 + 1;
    let text_width = buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0, f32::max)
        .ceil();
    let (width, height) = (
        text_width as u32 + 2 * margin,
        line_height as u32 + 2 * margin,
    );
    // Each glyph's coverage, and which letter it is, for gummy's colours.
    let mut mask = vec![0f32; (width * height) as usize];
    let mut letter = vec![u16::MAX; (width * height) as usize];
    let mut cache = SwashCache::new();
    for run in buffer.layout_runs() {
        for glyph in run.glyphs {
            let physical = glyph.physical((0.0, 0.0), 1.0);
            let Some(image) = cache.get_image_uncached(font_system, physical.cache_key) else {
                continue;
            };
            let index = text[..glyph.start].chars().count() as u16;
            let left = margin as i32 + physical.x + image.placement.left;
            let top = margin as i32 + run.line_y as i32 + physical.y - image.placement.top;
            let (glyph_width, glyph_height) =
                (image.placement.width as i32, image.placement.height as i32);
            for gy in 0..glyph_height {
                for gx in 0..glyph_width {
                    let (x, y) = (left + gx, top + gy);
                    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                        continue;
                    }
                    let at = (gy * glyph_width + gx) as usize;
                    let coverage = match image.content {
                        SwashContent::Mask => image.data[at],
                        SwashContent::Color => image.data[at * 4 + 3],
                        SwashContent::SubpixelMask => image.data[at * 4],
                    };
                    if coverage == 0 {
                        continue;
                    }
                    let to = (y as u32 * width + x as u32) as usize;
                    mask[to] = mask[to].max(f32::from(coverage) / 255.0);
                    letter[to] = index;
                }
            }
        }
    }
    let mut canvas = Canvas::new(width, height);
    let box_x = margin as f32;
    let box_y = margin as f32;
    let colors = &look.colors;
    let solid = |rgb: u32| move |_: f32, _: f32, _: u16| Ink(rgb, 1.0);
    // Colours across the text box, x from 0 to 1, at `stops`.
    let across = |stops: Vec<(f32, u32)>| {
        move |x: f32, _: f32, _: u16| {
            Ink(
                gradient(&stops, ((x - box_x) / text_width.max(1.0)).clamp(0.0, 1.0)),
                1.0,
            )
        }
    };
    let main = look.main;
    match look.effect {
        // Gradient and glow: the colours from 10% to 90%.
        2 | 6 if colors.len() > 1 => {
            let last = (colors.len() - 1) as f32;
            let stops = colors
                .iter()
                .enumerate()
                .map(|(i, c)| (0.1 + 0.8 * i as f32 / last, *c))
                .collect();
            fill(&mut canvas, &mask, &letter, 0.0, 0.0, across(stops));
        }
        // Prism: round once, back to the first.
        7 if colors.len() > 1 => {
            let count = colors.len() as f32;
            let mut stops: Vec<(f32, u32)> = colors
                .iter()
                .enumerate()
                .map(|(i, c)| (i as f32 / count, *c))
                .collect();
            stops.push((1.0, colors[0]));
            fill(&mut canvas, &mask, &letter, 0.0, 0.0, across(stops));
        }
        // Neon: a halo in the colour, then white edged in a lighter one.
        3 => {
            let halo = dilate(&mask, width, height, (em * 0.24 + 6.0) / 2.0);
            fill(&mut canvas, &halo, &letter, 0.0, 0.0, |_, _, _| {
                Ink(main, 0.28)
            });
            let edge = dilate(&mask, width, height, em * 0.04 + 1.0);
            fill(&mut canvas, &edge, &letter, 0.0, 0.0, solid(look.neon));
            fill(&mut canvas, &mask, &letter, 0.0, 0.0, solid(0xffffff));
        }
        // Toon: light shades in bands down the line, edged in the dark one.
        4 => {
            let edge = dilate(&mask, width, height, em * 0.04 + 1.6);
            fill(&mut canvas, &edge, &letter, 0.0, 0.0, solid(look.toon));
            let stops = vec![
                (0.0, look.light2),
                (0.45, look.light1),
                (0.55, main),
                (1.0, look.dark1),
            ];
            fill(&mut canvas, &mask, &letter, 0.0, 0.0, move |_, y, _| {
                Ink(
                    gradient(&stops, ((y - box_y) / line_height).clamp(0.0, 1.0)),
                    1.0,
                )
            });
        }
        // Pop: white edged in the darkest shade, over a lowered copy in the colour.
        5 => {
            let edge = dilate(&mask, width, height, em * 0.04 + 1.2);
            let drop = em * 0.08 + em * 0.04 + 1.2;
            fill(&mut canvas, &edge, &letter, 0.0, drop, solid(look.dark2));
            fill(&mut canvas, &mask, &letter, 0.0, drop, solid(main));
            fill(&mut canvas, &edge, &letter, 0.0, 0.0, solid(look.dark2));
            fill(&mut canvas, &mask, &letter, 0.0, 0.0, solid(0xffffff));
        }
        // Gummy: each letter in its colour.
        8 if !look.letters.is_empty() => {
            let letters: Vec<u32> = look.letters.iter().map(|(_, color)| *color).collect();
            fill(&mut canvas, &mask, &letter, 0.0, 0.0, move |_, _, index| {
                Ink(
                    letters.get(usize::from(index)).copied().unwrap_or(main),
                    1.0,
                )
            });
        }
        _ => fill(
            &mut canvas,
            &mask,
            &letter,
            0.0,
            0.0,
            solid(colors.first().copied().unwrap_or(main)),
        ),
    }
    StyledPicture {
        canvas,
        margin,
        width: text_width,
        height: line_height,
    }
}

/// Lays `coverage` onto the canvas, moved down by `dy` (whole pixels), in
/// the colour `ink` gives each pixel.
fn fill(
    canvas: &mut Canvas,
    coverage: &[f32],
    letter: &[u16],
    _dx: f32,
    dy: f32,
    ink: impl Fn(f32, f32, u16) -> Ink,
) {
    let shift = dy.round() as u32;
    let (width, height) = (canvas.width, canvas.height);
    for y in 0..height.saturating_sub(shift) {
        for x in 0..width {
            let at = (y * width + x) as usize;
            if coverage[at] > 0.0 {
                let color = ink(x as f32 + 0.5, y as f32 + 0.5, letter[at]);
                canvas.over(x, y + shift, color, coverage[at]);
            }
        }
    }
}

/// The coverage grown outward by `radius` pixels: an outside stroke.
fn dilate(mask: &[f32], width: u32, height: u32, radius: f32) -> Vec<f32> {
    let reach = radius.ceil() as i32 + 1;
    let mut offsets = Vec::new();
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let weight = (radius + 0.5 - ((dx * dx + dy * dy) as f32).sqrt()).clamp(0.0, 1.0);
            if weight > 0.0 {
                offsets.push((dx, dy, weight));
            }
        }
    }
    let mut out = vec![0f32; mask.len()];
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let mut best = 0f32;
            for &(dx, dy, weight) in &offsets {
                let (sx, sy) = (x + dx, y + dy);
                if sx >= 0 && sy >= 0 && sx < width as i32 && sy < height as i32 {
                    best = best.max(mask[(sy as u32 * width + sx as u32) as usize] * weight);
                }
            }
            out[(y as u32 * width + x as u32) as usize] = best;
        }
    }
    out
}

/// The colour at `t` along stops of (offset, 0xRRGGBB).
fn gradient(stops: &[(f32, u32)], t: f32) -> u32 {
    let Some(first) = stops.first() else { return 0 };
    if t <= first.0 {
        return first.1;
    }
    for pair in stops.windows(2) {
        let ((a, from), (b, to)) = (pair[0], pair[1]);
        if t <= b {
            let f = if b > a { (t - a) / (b - a) } else { 1.0 };
            let mix = |shift: u32| {
                let (x, y) = (
                    f32::from((from >> shift) as u8),
                    f32::from((to >> shift) as u8),
                );
                (((x + (y - x) * f).round() as u32) & 0xff) << shift
            };
            return mix(16) | mix(8) | mix(0);
        }
    }
    stops.last().map_or(0, |stop| stop.1)
}

/// Twemoji's picture of `emoji` at `size` pixels square: shaped with the
/// client's font system so a sequence takes Twemoji's one glyph for it, and
/// drawn by swash from the font's PNG. None when Twemoji has no one glyph
/// for it, which the picker leaves out.
pub fn emoji(system: &mut cosmic_text::FontSystem, emoji: &str, size: u32) -> Option<Canvas> {
    // Taken at the font's own strike (53 ppem: 72 px pictures with 4 px of
    // room, `emoji_font_tables.rs` `TWEMOJI`), which swash copies out
    // without scaling, then shrunk by area once to the size shown. Asking
    // swash for another size had it resample each picture first: a fifth of
    // the picker's scrolling CPU.
    let glyph = twemoji_glyph(system, emoji, 53.0)?;
    let mut cache = cosmic_text::SwashCache::new();
    let image = cache.get_image_uncached(system, glyph.physical((0.0, 0.0), 1.0).cache_key)?;
    let (width, height) = (image.placement.width, image.placement.height);
    if width == 0 || height == 0 || !matches!(image.content, cosmic_text::SwashContent::Color) {
        return None;
    }
    // Premultiplied BGRA, as a canvas is.
    let mut source = image.data;
    for pixel in source.as_chunks_mut::<4>().0 {
        let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
        let scale = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
        pixel.copy_from_slice(&[scale(b), scale(g), scale(r), a]);
    }
    // Each picture in the font has 4 px of room round its 72
    // (`emoji_font_tables.rs`).
    let margin = width as f32 * 4.0 / 80.0;
    let side = (width as f32 - 2.0 * margin)
        .min(height as f32 - 2.0 * margin)
        .max(1.0);
    let mut canvas = Canvas::new(size, size);
    let step = side / size as f32;
    for y in 0..size {
        for x in 0..size {
            let (x0, y0) = (margin + x as f32 * step, margin + y as f32 * step);
            let (x1, y1) = (x0 + step, y0 + step);
            let mut sum = [0.0f32; 4];
            let mut weight = 0.0;
            for sy in y0.floor() as u32..(y1.ceil() as u32).min(height) {
                let cover_y = (y1.min(sy as f32 + 1.0) - y0.max(sy as f32)).max(0.0);
                for sx in x0.floor() as u32..(x1.ceil() as u32).min(width) {
                    let cover = cover_y * (x1.min(sx as f32 + 1.0) - x0.max(sx as f32)).max(0.0);
                    let at = ((sy * width + sx) * 4) as usize;
                    for (sum, &value) in sum.iter_mut().zip(&source[at..at + 4]) {
                        *sum += f32::from(value) * cover;
                    }
                    weight += cover;
                }
            }
            if weight > 0.0 {
                let at = ((y * size + x) * 4) as usize;
                for (pixel, sum) in canvas.pixels[at..at + 4].iter_mut().zip(sum) {
                    *pixel = (sum / weight).round().min(255.0) as u8;
                }
            }
        }
    }
    Some(canvas)
}

/// The one glyph of the Twemoji font that draws `emoji`, if it has one.
fn twemoji_glyph(
    system: &mut cosmic_text::FontSystem,
    emoji: &str,
    size: f32,
) -> Option<cosmic_text::LayoutGlyph> {
    let mut buffer = cosmic_text::Buffer::new(system, cosmic_text::Metrics::new(size, size));
    let attrs = cosmic_text::Attrs::new().family(cosmic_text::Family::Name(TWEMOJI));
    buffer.set_text(system, emoji, &attrs, cosmic_text::Shaping::Advanced, None);
    buffer.shape_until_scroll(system, false);
    let run = buffer.layout_runs().next()?;
    // Variation selectors and joiners shape to nothing; what's left is one
    // glyph of Twemoji's.
    let mut drawn = run.glyphs.iter().filter(|glyph| glyph.w > 0.0);
    let glyph = drawn.next()?.clone();
    if drawn.next().is_some() {
        return None;
    }
    let twemoji = system
        .db()
        .face(glyph.font_id)
        .is_some_and(|face| face.families.iter().any(|(family, _)| family == TWEMOJI));
    twemoji.then_some(glyph)
}

/// Whether Twemoji draws `emoji` as one picture, without drawing it.
pub fn has_emoji(system: &mut cosmic_text::FontSystem, emoji: &str) -> bool {
    twemoji_glyph(system, emoji, 16.0).is_some()
}

/// A display name painted whole in its style and handed to the renderer,
/// ready to show wherever a styled name is drawn: the profile's card, the
/// Display Name Style window's tiles and previews.
pub struct NamePicture {
    image: iced::advanced::image::Handle,
    margin: f32,
    /// The text box's size, and the picture's with its margins.
    pub width: f32,
    pub height: f32,
    full: iced::Size,
}

impl NamePicture {
    /// `text` in `look` at `size` and `weight` (the usual font's; a
    /// style's own font keeps its weight).
    pub fn new(look: &NameLook, text: &str, size: f32, weight: u16) -> Self {
        use iced::advanced::graphics::text::font_system;
        // Its font first, so the font system has it.
        let _ = crate::fonts::style_font(look.font(), weight);
        let mut system = font_system().write().expect("font system");
        let shown = crate::fonts::shown_name(text, look.font());
        let mut picture = styled_name(system.raw(), &shown, look, size, weight);
        let mut pixels = std::mem::take(&mut picture.canvas.pixels);
        finish(&mut pixels);
        let (width, height) = (picture.canvas.width, picture.canvas.height);
        Self {
            image: iced::advanced::image::Handle::from_rgba(width, height, pixels),
            margin: picture.margin as f32,
            width: picture.width,
            height: picture.height,
            full: iced::Size::new(width as f32, height as f32),
        }
    }

    /// The name in its text box, its strokes and halos drawn past it.
    pub fn view<'a>(&self) -> iced::Element<'a, crate::app::Message> {
        crate::widgets::Pic::new(self.image.clone(), iced::Size::new(self.width, self.height))
            .at(iced::Rectangle::new(
                iced::Point::new(-self.margin, -self.margin),
                self.full,
            ))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_cut_to_cover_keeps_its_middle() {
        // Four columns, red, green, green, blue, made two wide: the cover
        // cut keeps the middle two.
        let mut source = Vec::new();
        for rgb in [[255, 0, 0], [0, 255, 0], [0, 255, 0], [0, 0, 255]] {
            source.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        let out = fitted(&source, (4, 1), (2, 2), true);
        assert_eq!(out.len(), 2 * 2 * 4);
        for pixel in out.as_chunks::<4>().0 {
            assert_eq!(*pixel, [0, 255, 0, 255]);
        }
    }

    #[test]
    fn a_picture_of_another_shape_fits_inside_keeping_it() {
        // Two tall pixels, red over blue, made 4 by 4: two columns of
        // them in the middle, see-through either side.
        let source = [255, 0, 0, 255, 0, 0, 255, 255];
        let out = fitted(&source, (1, 2), (4, 4), false);
        assert_eq!(out.len(), 4 * 4 * 4);
        let pixel = |x: usize, y: usize| &out[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
        for y in 0..4 {
            assert_eq!(pixel(0, y), [0, 0, 0, 0], "see-through on the left");
            assert_eq!(pixel(3, y), [0, 0, 0, 0], "see-through on the right");
        }
        assert_eq!(pixel(1, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(2, 1), [255, 0, 0, 255]);
        assert_eq!(pixel(1, 3), [0, 0, 255, 255]);
        // One of its own shape is scaled to it whole.
        let same = fitted(&[9, 9, 9, 255].repeat(4), (2, 2), (4, 4), false);
        assert!(
            same.as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [9, 9, 9, 255])
        );
    }

    #[test]
    fn a_presence_dot_is_drawn_for_each_presence() {
        let mobile = status_dot(Presence::Mobile, 0, LIVE_TEST, 0).expect("drawn");
        assert_eq!((mobile.width, mobile.height), (16, 21));
        for presence in [
            Presence::Online,
            Presence::Idle,
            Presence::DoNotDisturb,
            Presence::Offline,
        ] {
            let dot = status_dot(presence, 0, LIVE_TEST, 0).expect("drawn");
            assert_eq!((dot.width, dot.height), (16, 16));
        }
    }

    const LIVE_TEST: u32 = 0x3ecf7e;
}

/// The vendored tiny-skia renderer (vendor/iced_tiny_skia) keeps a round
/// quad inside its layer's clip, which its patches 2, 3 and 11 draw
/// without a mask. The tests draw with it, as Yapfast does unless its GPU
/// setting is on; the GPU's renderer has no such patches to test.
#[cfg(test)]
mod renderer_tests {
    use iced::{
        Border, Color, Font, Pixels, Rectangle, Size,
        advanced::{
            Renderer as _,
            renderer::{Headless as _, Quad},
        },
    };

    /// The pixel at `x`, `y` of a screenshot `width` wide.
    fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * width + x) * 4) as usize;
        [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
    }

    #[test]
    fn a_round_quad_stays_inside_its_layers_clip() {
        let clip = Rectangle::new(iced::Point::new(10.0, 10.0), Size::new(40.0, 20.0));
        // Rounded, bordered and both; at whole and fractional scales; and
        // as wide as a line of text that doesn't wrap.
        let cases = [(4.0, 0.0), (4.0, 1.0), (0.0, 1.0)]
            .into_iter()
            .flat_map(|look| [(look, 1.0, 100.0), (look, 1.5, 100.0), (look, 1.25, 5000.0)]);
        for ((radius, border), scale, width) in cases {
            let mut renderer = iced_tiny_skia::Renderer::new(Font::default(), Pixels(16.0));
            renderer.with_layer(clip, |renderer| {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(
                            iced::Point::new(20.0, 12.0),
                            Size::new(width, 16.0),
                        ),
                        border: Border {
                            color: Color::from_rgb(1.0, 0.0, 0.0),
                            width: border,
                            radius: radius.into(),
                        },
                        ..Quad::default()
                    },
                    Color::WHITE,
                );
            });
            let rgba = renderer.screenshot(Size::new(200, 60), scale, Color::BLACK);
            let at = |x: f32| ((x * scale) as u32, (20.0 * scale) as u32);
            let (x, y) = at(30.0);
            assert_eq!(pixel(&rgba, 200, x, y)[..3], [255, 255, 255], "inside");
            for outside in [52.0, 80.0, 130.0] {
                let (x, y) = at(outside);
                assert_eq!(
                    pixel(&rgba, 200, x, y)[..3],
                    [0, 0, 0],
                    "at {outside}, outside the clip: radius {radius}, border {border}, scale {scale}, width {width}"
                );
            }
        }
    }
}
