//! Twemoji's pictures, which Discord's client draws its emoji from, in a font
//! `build.rs` makes of them for the program to carry; see
//! `vendor/twemoji/README.yapfast.md`. The font is mapped from the program's file,
//! so what it holds costs memory only as far as it's read, and it can be given
//! back: message text draws it as glyphs ([`crate::emoji_text`]).
//!
//! A cell asks for the picture of an emoji at the size the grid draws it, and
//! gets a decoded image back at that size, which it keeps while it shows it:
//! the pictures are 72px square, and each is shrunk once to the pixels the
//! display draws, so drawing it copies it rather than scaling it again.
//!
//! Text draws a glyph of the font the same way: the renderer asks
//! [`glyph_picture`] for the picture shrunk to the pixels the glyph covers and
//! draws that in its place, where Skia's own glyph was the 72 pixels scaled
//! without averaging them, jagged at a third of their size.

use std::{cell::RefCell, collections::HashMap};

/// The font, each picture in it a glyph. A static, as a const would be a copy in
/// the program for each place that names it.
pub static FONT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/twemoji.ttf"));

/// What finds a picture in [`FONT`]: a count, then for each picture where its name
/// starts and how long it is, and where it starts in the font and how long it is,
/// then the names. The names sort.
static INDEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/twemoji.index"));

/// The most bytes of pictures kept, which the cache lets go of at once: a filled
/// cell keeps its own, so letting them go costs a redraw of what comes next. A
/// display of twice the scale takes four times the bytes for each.
const MOST_BYTES: u32 = 4 << 20;

/// The most bytes of the pictures text draws in place of glyphs kept in each
/// half of [`GlyphPictures`], so twice this at most: a picture at text's 22
/// pixels takes 2 KB, and one at the 48 of a message of emoji alone, on a
/// display of twice the scale, 37 KB.
const GLYPH_BYTES: usize = 2 << 20;

thread_local! {
    static PICTURES: RefCell<HashMap<(String, u32), Option<slint::Image>>> = RefCell::default();
    static GLYPH_PICTURES: RefCell<GlyphPictures> = RefCell::new(GlyphPictures::new(GLYPH_BYTES));
}

/// A glyph of the font and the side, in pixels, of its picture.
type GlyphKey = (u16, u32);

/// The pictures text drew in place of glyphs, in two halves: those drawn since
/// the newer half last filled, and those drawn before. A picture drawn again
/// moves to the newer half, so when it fills and the older half goes, what
/// text still shows stays and only what scrolled away is let go, with no frame
/// that decodes every emoji on the screen again.
struct GlyphPictures {
    newer: HashMap<GlyphKey, skia_safe::Image>,
    newer_bytes: usize,
    older: HashMap<GlyphKey, skia_safe::Image>,
    most_bytes: usize,
}

impl GlyphPictures {
    fn new(most_bytes: usize) -> Self {
        Self {
            newer: HashMap::new(),
            newer_bytes: 0,
            older: HashMap::new(),
            most_bytes,
        }
    }

    /// The picture kept for `key`, or the one `make` makes of `bytes`, kept.
    /// None when `make` fails or one picture would fill a half alone.
    fn get_or_make(
        &mut self,
        key: GlyphKey,
        bytes: usize,
        make: impl FnOnce() -> Option<skia_safe::Image>,
    ) -> Option<skia_safe::Image> {
        if let Some(picture) = self.newer.get(&key) {
            return Some(picture.clone());
        }
        if bytes > self.most_bytes {
            return None;
        }
        let picture = match self.older.remove(&key) {
            Some(picture) => picture,
            None => make()?,
        };
        if self.newer_bytes + bytes > self.most_bytes {
            self.older = std::mem::take(&mut self.newer);
            self.newer_bytes = 0;
        }
        self.newer.insert(key, picture.clone());
        self.newer_bytes += bytes;
        Some(picture)
    }

    fn clear(&mut self) {
        *self = Self::new(self.most_bytes);
    }
}

/// What text draws in place of the font's glyph `glyph` in text of `size`
/// physical pixels: Twemoji's picture shrunk to the pixels the glyph covers,
/// and where its top left corner sits from the glyph's origin on the baseline.
/// None for a glyph with no picture, such as a joiner's, which Skia draws as
/// the nothing it is. The renderer calls it for each such glyph it draws, so a
/// picture is decoded once for its size and kept in [`GLYPH_PICTURES`].
pub fn glyph_picture(glyph: u16, size: f32) -> Option<(skia_safe::Image, skia_safe::Vector)> {
    use crate::emoji_font_tables::{TWEMOJI, TWEMOJI_MARGIN};
    // Pictures are glyphs 1 on, in the order of their names; the empty glyphs
    // that join them come after.
    let place = usize::from(glyph.checked_sub(1)?);
    if place >= count() {
        return None;
    }
    let scale = size / f32::from(TWEMOJI.ppem);
    let margin = TWEMOJI_MARGIN as f32;
    let side = (scale * (f32::from(TWEMOJI.side) - 2.0 * margin))
        .round()
        .max(1.0) as u32;
    let offset = skia_safe::Vector::new(
        scale * (f32::from(TWEMOJI.bearing_x) + margin),
        scale * (margin - f32::from(TWEMOJI.bearing_y)),
    );
    let bytes = (side as usize)
        .saturating_mul(side as usize)
        .saturating_mul(4);
    let picture = GLYPH_PICTURES.with_borrow_mut(|pictures| {
        pictures.get_or_make((glyph, side), bytes, || skia_picture(place, side))
    })?;
    Some((picture, offset))
}

/// The picture at `place` in the sorted list, `side` pixels square, as an
/// image Skia draws: its alpha premultiplied, its pixels handed over rather
/// than copied.
fn skia_picture(place: usize, side: u32) -> Option<skia_safe::Image> {
    let shrunk = resize_picture(decode_published(entry(place).1)?, side);
    let info = skia_safe::ImageInfo::new(
        (side as i32, side as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        skia_safe::ColorSpace::new_srgb(),
    );
    skia_safe::images::raster_from_data(
        &info,
        skia_safe::Data::new_with_owner(shrunk.into_raw()),
        side as usize * 4,
    )
}

/// The picture of `glyph` at `px` logical pixels, for a display of `scale`, or
/// none when Twemoji has no picture for it.
pub fn picture(glyph: &str, px: f32, scale: f32) -> Option<slint::Image> {
    let pixels = (px * scale).round().max(1.0) as u32;
    PICTURES.with_borrow_mut(|pictures| {
        if pictures.len() as u32 >= MOST_BYTES / (pixels * pixels * 4) {
            pictures.clear();
        }
        pictures
            .entry((glyph.to_owned(), pixels))
            .or_insert_with(|| decode(glyph, pixels))
            .clone()
    })
}

/// Lets go of the pictures kept, for when nothing shows them; a cell keeps its own.
pub fn forget() {
    PICTURES.with_borrow_mut(HashMap::clear);
}

/// Lets go of the pictures text drew in place of glyphs, for when no window
/// draws text; the next window's text decodes what it shows again.
pub fn forget_glyph_pictures() {
    GLYPH_PICTURES.with_borrow_mut(GlyphPictures::clear);
}

/// Whether Twemoji has a picture for `glyph`, without decoding one.
pub fn has_picture(glyph: &str) -> bool {
    picture_bytes(glyph).is_some()
}

/// The picture at `pixels` square, its alpha premultiplied, as Skia draws it.
/// Drawn at 72px into a smaller cell, Skia scaled every picture again on every
/// frame a scroll drew, a quarter of the scroll's time; shrunk here once, a
/// frame copies it.
fn decode(glyph: &str, pixels: u32) -> Option<slint::Image> {
    let shrunk = resize_picture(published(glyph)?, pixels);
    let buffer = slint::SharedPixelBuffer::clone_from_slice(shrunk.as_raw(), pixels, pixels);
    Some(slint::Image::from_rgba8_premultiplied(buffer))
}

/// `picture` at `pixels` square, its alpha premultiplied.
fn resize_picture(mut picture: image::RgbaImage, pixels: u32) -> image::RgbaImage {
    premultiply(&mut picture);
    let (width, height) = picture.dimensions();
    if (width, height) == (pixels, pixels) {
        picture
    } else if pixels < width.min(height) {
        shrunk(&picture, pixels)
    } else {
        // Larger than Twemoji drew it, on a display of over twice the scale.
        image::imageops::resize(
            &picture,
            pixels,
            pixels,
            image::imageops::FilterType::Triangle,
        )
    }
}

/// Multiplies each of `picture`'s colours by its alpha, so mixing pixels
/// doesn't darken the edge where a picture fades out.
fn premultiply(picture: &mut image::RgbaImage) {
    for pixel in picture.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
}

/// `picture` shrunk to `side` square, each pixel the mean of the part of the
/// picture it covers: a few microseconds, where `image`'s filters took
/// 70-120us a picture, which a scroll paid on every row that came into sight.
fn shrunk(picture: &image::RgbaImage, side: u32) -> image::RgbaImage {
    let (width, height) = picture.dimensions();
    let (across, down) = (spans(width, side), spans(height, side));
    let side = side as usize;
    let source = picture.as_raw();
    // Across first, into each source row's `side` sums.
    let mut rows = vec![0f32; height as usize * side * 4];
    for y in 0..height as usize {
        let row = &source[y * width as usize * 4..][..width as usize * 4];
        for (x, span) in across.iter().enumerate() {
            let out = &mut rows[(y * side + x) * 4..][..4];
            for &(at, weight) in span {
                for channel in 0..4 {
                    out[channel] += f32::from(row[at * 4 + channel]) * weight;
                }
            }
        }
    }
    let mut out = vec![0u8; side * side * 4];
    for (y, span) in down.iter().enumerate() {
        for x in 0..side {
            let mut sum = [0f32; 4];
            for &(at, weight) in span {
                let from = &rows[(at * side + x) * 4..][..4];
                for channel in 0..4 {
                    sum[channel] += from[channel] * weight;
                }
            }
            for channel in 0..4 {
                out[(y * side + x) * 4 + channel] = sum[channel].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    image::RgbaImage::from_raw(side as u32, side as u32, out).expect("a picture's pixels")
}

/// For each of `to` pixels shrunk from `from`, the source pixels it covers and
/// how much of each, which add up to one.
fn spans(from: u32, to: u32) -> Vec<Vec<(usize, f32)>> {
    let step = from as f32 / to as f32;
    (0..to)
        .map(|out| {
            let (start, end) = (out as f32 * step, (out + 1) as f32 * step);
            (start.floor() as usize..(end.ceil() as usize).min(from as usize))
                .map(|at| {
                    let covered = end.min(at as f32 + 1.0) - start.max(at as f32);
                    (at, covered / step)
                })
                .filter(|(_, weight)| *weight > 0.0)
                .collect()
        })
        .collect()
}

/// The names Twemoji may have given an emoji's picture: its code points in
/// lower-case hexadecimal, and the same with the variation selectors dropped,
/// as most of them are (`2764.png` for ❤️, `31-20e3.png` for 1️⃣, while the
/// sequences that need one keep it, as `2764-fe0f-200d-1f525.png` does).
fn names(glyph: &str) -> [String; 2] {
    let code_points: Vec<String> = glyph
        .chars()
        .map(|character| format!("{:x}", character as u32))
        .collect();
    let exact = code_points.join("-");
    let dropped = code_points
        .iter()
        .filter(|point| point.as_str() != "fe0f")
        .cloned()
        .collect::<Vec<_>>()
        .join("-");
    [exact, dropped]
}

/// The picture of `glyph` as Twemoji publishes it, without the margin it has in the
/// font.
pub(crate) fn published(glyph: &str) -> Option<image::RgbaImage> {
    decode_published(picture_bytes(glyph)?)
}

/// [`published`], of the PNG the font holds for a picture.
fn decode_published(png: &[u8]) -> Option<image::RgbaImage> {
    use crate::emoji_font_tables::TWEMOJI_MARGIN as MARGIN;
    let bitmap = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    let side = bitmap.width().checked_sub(2 * MARGIN)?;
    Some(image::imageops::crop_imm(&bitmap, MARGIN, MARGIN, side, side).to_image())
}

fn picture_bytes(glyph: &str) -> Option<&'static [u8]> {
    let (_, picture) = entry(find(glyph)?);
    Some(picture)
}

/// The code point of the glyph in [`FONT`] that draws `emoji`.
pub fn glyph(emoji: &str) -> Option<char> {
    char::from_u32(crate::emoji_font_tables::TWEMOJI_LAST - find(emoji)? as u32)
}

/// The place of the picture of `glyph` in the sorted list.
fn find(glyph: &str) -> Option<usize> {
    names(glyph).iter().find_map(|name| {
        let (mut low, mut high) = (0, count());
        while low < high {
            let middle = low + (high - low) / 2;
            match entry(middle).0.cmp(name.as_str()) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return Some(middle),
            }
        }
        None
    })
}

fn count() -> usize {
    u32::from_le_bytes(INDEX[0..4].try_into().expect("the index's count")) as usize
}

/// The name and the picture at `place` in the sorted list. An entry is where its
/// name starts, u32, and how long it is, u16, then where its picture starts and
/// how long it is, u32 each.
fn entry(place: usize) -> (&'static str, &'static [u8]) {
    const ENTRY: usize = 14;
    let at = 4 + place * ENTRY;
    let u32_at =
        |at: usize| u32::from_le_bytes(INDEX[at..at + 4].try_into().expect("a number")) as usize;
    let name_at = u32_at(at);
    let name_length =
        u16::from_le_bytes(INDEX[at + 4..at + 6].try_into().expect("a name's length")) as usize;
    let picture_at = u32_at(at + 6);
    let name = std::str::from_utf8(&INDEX[name_at..name_at + name_length]).expect("a name");
    (name, &FONT[picture_at..picture_at + u32_at(at + 10)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_emoji_has_twemojis_picture_at_the_size_it_draws() {
        // Shrunk once to the pixels a cell draws, so drawing it copies it
        // rather than scaling 72px down again every frame.
        for (px, scale, side) in [
            (36., 1., 36),
            (36., 2., 72),
            (32., 1.25, 40),
            (36., 3., 108),
        ] {
            let drawn = picture("🍕", px, scale).expect("Twemoji draws a pizza");
            assert_eq!(
                (drawn.size().width, drawn.size().height),
                (side, side),
                "{px} logical at {scale}"
            );
        }
    }

    #[test]
    fn a_shrunk_picture_keeps_its_colour_and_its_edge() {
        // A square red to just short of half, clear after: shrunk, the colour stays red
        // where it fades, as its alpha is premultiplied, and the clear half
        // stays clear.
        let mut square = image::RgbaImage::new(72, 72);
        for (x, _, pixel) in square.enumerate_pixels_mut() {
            *pixel = if x < 35 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([255, 255, 255, 0])
            };
        }
        premultiply(&mut square);
        let small = shrunk(&square, 32);
        assert_eq!(small.dimensions(), (32, 32));
        assert_eq!(small.get_pixel(0, 5).0, [255, 0, 0, 255]);
        assert_eq!(small.get_pixel(31, 5).0, [0, 0, 0, 0]);
        // The column the edge crosses is part of each.
        let edge = small.get_pixel(15, 5).0;
        assert!(
            edge[3] > 0 && edge[3] < 255 && edge[0] == edge[3] && edge[1] == 0,
            "{edge:?}"
        );
        // Each pixel's share of the source adds up to one.
        for span in spans(72, 32) {
            let sum: f32 = span.iter().map(|(_, weight)| weight).sum();
            assert!((sum - 1.0).abs() < 1e-4, "{span:?}");
        }
    }

    /// The glyph of `emoji`'s picture as Skia and the renderer number it.
    fn glyph_id(emoji: &str) -> u16 {
        picture_glyph(emoji) as u16
    }

    /// The pixels of what `draw` draws on a clear square of `side`, and the
    /// rows and columns where something shows, as (left, top, right, bottom).
    fn drawn(side: usize, draw: impl FnOnce(&skia_safe::Canvas)) -> (Vec<u8>, [usize; 4]) {
        use skia_safe::{image::CachingHint, surfaces};
        let mut surface = surfaces::raster_n32_premul((side as i32, side as i32)).unwrap();
        draw(surface.canvas());
        let image = surface.image_snapshot();
        let mut pixels = vec![0u8; side * side * 4];
        assert!(image.read_pixels(
            image.image_info(),
            &mut pixels,
            side * 4,
            (0, 0),
            CachingHint::Allow
        ));
        let alpha = |x: usize, y: usize| pixels[(y * side + x) * 4 + 3];
        let rows: Vec<usize> = (0..side)
            .filter(|&y| (0..side).any(|x| alpha(x, y) > 0))
            .collect();
        let columns: Vec<usize> = (0..side)
            .filter(|&x| (0..side).any(|y| alpha(x, y) > 0))
            .collect();
        let bounds = [
            columns[0],
            rows[0],
            *columns.last().unwrap(),
            *rows.last().unwrap(),
        ];
        (pixels, bounds)
    }

    /// The font as Skia reads it where the program holds it.
    fn typeface() -> skia_safe::Typeface {
        // SAFETY: the font is in the program, and stays where it is.
        let data = unsafe { skia_safe::Data::new_bytes(FONT) };
        skia_safe::FontMgr::new()
            .new_from_data(data, None)
            .expect("Skia loads the font")
    }

    #[test]
    fn a_glyphs_picture_covers_what_skias_glyph_did() {
        // The picture stands in for the glyph, so it lands where the glyph
        // drew, the same size, at text's sizes and a display's scales.
        let typeface = typeface();
        for emoji in ["😄", "🟥", "🇩🇰"] {
            let glyph = glyph_id(emoji);
            for size in [16.0, 17.6, 20.0, 22.0, 32.0, 48.0, 96.0] {
                let (_, by_skia) = drawn(256, |canvas| {
                    canvas.draw_glyphs_at(
                        &[glyph],
                        &[skia_safe::Point::new(20.0, 200.0)][..],
                        skia_safe::Point::default(),
                        &skia_safe::Font::from_typeface(typeface.clone(), size),
                        &skia_safe::Paint::default(),
                    );
                });
                let (picture, offset) = glyph_picture(glyph, size).expect("a picture");
                let (_, by_picture) = drawn(256, |canvas| {
                    canvas.draw_image(
                        &picture,
                        ((20.0 + offset.x).round(), (200.0 + offset.y).round()),
                        None,
                    );
                });
                // Skia scaling a picture up smears its edge a pixel further
                // into the margin.
                let near = if size > 48.0 { 2 } else { 1 };
                for (edge, (skia, ours)) in ["left", "top", "right", "bottom"]
                    .iter()
                    .zip(by_skia.iter().zip(&by_picture))
                {
                    assert!(
                        skia.abs_diff(*ours) <= near,
                        "{emoji} at {size}: the {edge} edge is at {ours}, Skia's at {skia}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_glyphs_picture_is_its_pixels_averaged_where_skias_glyph_was_not() {
        // 😄 at 16 pixel text, 22 pixels across: each of the picture's
        // pixels is the mean of the 72 pixel picture's it covers, the edge of
        // its circle as faint as the circle covers it. Skia's glyph samples
        // only a few of the source pixels each of its pixels covers, which
        // leaves its edges a staircase.
        let glyph = glyph_id("😄");
        let size = 16.0;
        let (picture, offset) = glyph_picture(glyph, size).unwrap();
        let at = ((40.0 + offset.x).round(), (40.0 + offset.y).round());
        let (ours, _) = drawn(64, |canvas| {
            canvas.draw_image(&picture, at, None);
        });
        let side = picture.width() as usize;
        let mean = resize_picture(published("😄").unwrap(), side as u32);
        let (skias, _) = drawn(64, |canvas| {
            canvas.draw_glyphs_at(
                &[glyph],
                &[skia_safe::Point::new(40.0, 40.0)][..],
                skia_safe::Point::default(),
                &skia_safe::Font::from_typeface(typeface(), size),
                &skia_safe::Paint::default(),
            );
        });
        // How far a drawing's alpha is, on average, from the mean's over the
        // square the picture covers, placed where it fits best.
        let off = |pixels: &[u8]| -> f32 {
            let mut best = u32::MAX;
            for (dx, dy) in [-1i32, 0, 1]
                .into_iter()
                .flat_map(|dx| [(dx, -1), (dx, 0), (dx, 1)])
            {
                let mut off = 0;
                for y in 0..side {
                    for x in 0..side {
                        let px = (at.0 as i32 + dx) as usize + x;
                        let py = (at.1 as i32 + dy) as usize + y;
                        let drawn = pixels[(py * 64 + px) * 4 + 3];
                        off += u32::from(drawn.abs_diff(mean.get_pixel(x as u32, y as u32)[3]));
                    }
                }
                best = best.min(off);
            }
            best as f32 / (side * side) as f32
        };
        // 0 and 10 of 255 when this was written.
        let (ours, skias) = (off(&ours), off(&skias));
        assert!(ours < 0.5, "ours is the mean, {ours} off");
        assert!(skias > 4.0, "Skia's glyph is {skias} off the mean");
    }

    #[test]
    fn a_glyphs_picture_is_kept_for_its_size() {
        let glyph = glyph_id("🍕");
        let (first, offset) = glyph_picture(glyph, 16.0).unwrap();
        // 72 pixels at 16 of the font's 53 per em, a pixel right of the
        // origin and its top 54 of the font's pixels above the baseline.
        assert_eq!((first.width(), first.height()), (22, 22));
        assert!((offset.x - 16.0 / 53.0).abs() < 1e-4, "{offset:?}");
        assert!((offset.y + 54.0 * 16.0 / 53.0).abs() < 1e-4, "{offset:?}");
        let (again, _) = glyph_picture(glyph, 16.0).unwrap();
        assert_eq!(again.unique_id(), first.unique_id(), "decoded once");
        let (larger, _) = glyph_picture(glyph, 32.0).unwrap();
        assert_eq!(larger.width(), 43);
        // `.notdef` and the empty glyphs after the pictures, a joiner's,
        // are left to Skia, which draws nothing for them.
        assert!(glyph_picture(0, 16.0).is_none());
        assert!(glyph_picture(count() as u16 + 1, 16.0).is_none());
        forget_glyph_pictures();
        let (anew, _) = glyph_picture(glyph, 16.0).unwrap();
        assert_ne!(
            anew.unique_id(),
            first.unique_id(),
            "let go of, and made again"
        );
    }

    #[test]
    fn glyph_pictures_keep_what_text_still_draws() {
        let mut made = 0;
        let mut pictures = GlyphPictures::new(3);
        let get = |pictures: &mut GlyphPictures, glyph: u16, made: &mut usize| {
            pictures
                .get_or_make((glyph, 1), 1, || {
                    *made += 1;
                    skia_safe::surfaces::raster_n32_premul((1, 1)).map(|mut s| s.image_snapshot())
                })
                .map(|picture| picture.unique_id())
        };
        let a = get(&mut pictures, 1, &mut made);
        for glyph in [2, 3, 4] {
            get(&mut pictures, glyph, &mut made);
        }
        // The newer half filled with 1 to 3, and 4 began it anew: 1 is in
        // the older half, and drawing it again moves it back without
        // decoding it.
        assert_eq!(made, 4);
        assert_eq!(get(&mut pictures, 1, &mut made), a);
        assert_eq!(made, 4);
        // 4, 1 and 5 fill the newer half, and 6 lets go of 2 and 3.
        get(&mut pictures, 5, &mut made);
        get(&mut pictures, 6, &mut made);
        assert_eq!(get(&mut pictures, 1, &mut made), a);
        get(&mut pictures, 2, &mut made);
        assert_eq!(made, 7, "2 was let go of and decoded again");
        // At most a half's worth in each half.
        assert!(pictures.newer.len() <= 3 && pictures.older.len() <= 3);
        // A picture larger than a half isn't kept, nor made.
        assert!(
            pictures
                .get_or_make((9, 1), 4, || panic!("too large to make"))
                .is_none()
        );
        pictures.clear();
        assert!(pictures.newer.is_empty() && pictures.older.is_empty());
    }

    #[test]
    fn a_sequence_has_a_picture_of_its_own() {
        // Twemoji draws these as pictures, where the system font's glyph would
        // have to be shaped.
        assert!(has_picture("👨‍👩‍👧"));
        assert!(has_picture("🇩🇰"));
        assert!(has_picture("1️⃣"));
        assert!(picture("👨‍👩‍👧", 36., 1.).is_some());
    }

    /// The glyphs and advances the font shapes `text` into, as Slint's text
    /// shapes it.
    fn shaped(text: &str) -> Vec<(u32, i32)> {
        let font = harfrust::FontRef::new(FONT).expect("the font reads");
        let data = harfrust::ShaperData::new(&font);
        let shaper = data.shaper(&font).build();
        let mut buffer = harfrust::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let shaped = shaper.shape(buffer, harfrust::ShapeOptions::new());
        shaped
            .glyph_infos()
            .iter()
            .zip(shaped.glyph_positions())
            .map(|(info, position)| (info.glyph_id, position.x_advance))
            .collect()
    }

    /// The glyph of `emoji`'s picture in the font.
    fn picture_glyph(emoji: &str) -> u32 {
        // Pictures are glyphs 1 on, in the order of their names; their code
        // points count down from the font's last.
        crate::emoji_font_tables::TWEMOJI_LAST - glyph(emoji).expect("a picture") as u32 + 1
    }

    #[test]
    fn text_typed_as_several_code_points_shapes_into_one_picture() {
        let advance = shaped("🍕")[0].1;
        assert_eq!(shaped("🍕"), [(picture_glyph("🍕"), advance)]);
        // A family, a flag and a skin tone, each its one picture.
        for emoji in ["👨‍👩‍👧", "🇩🇰", "👍🏽", "🏳️‍🌈", "🏴󠁧󠁢󠁳󠁣󠁴󠁿"]
        {
            assert_eq!(
                shaped(emoji),
                [(picture_glyph(emoji), advance)],
                "{emoji} is one picture"
            );
        }
        // A selector after a single emoji takes no room, and typed without
        // the selector a sequence joins all the same.
        let heart = shaped("❤️");
        assert_eq!(heart.iter().map(|(_, x)| x).sum::<i32>(), advance);
        assert_eq!(heart[0].0, picture_glyph("❤️"));
        assert_eq!(shaped("🏳‍🌈").len(), 1);
        // Digits, `#` and `*` are no glyphs of this font, so text in its
        // family draws them from the next: a keycap's are, and drew nothing.
        for text in ["2", "#", "*"] {
            assert_eq!(shaped(text)[0].0, 0, "{text} falls through");
        }
    }

    #[test]
    fn the_names_are_the_code_points_and_those_without_variation_selectors() {
        assert_eq!(names("🍕"), ["1f355".to_owned(), "1f355".to_owned()]);
        assert_eq!(names("❤️"), ["2764-fe0f".to_owned(), "2764".to_owned()]);
        // A picture Twemoji named without the selector is found all the same.
        assert!(has_picture("❤️"));
        assert!(has_picture("1️⃣"));
        // One it has no picture for says so, and the cell keeps its glyph.
        assert!(!has_picture("not an emoji"));
    }
}
