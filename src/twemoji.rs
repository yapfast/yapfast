//! Twemoji's pictures, which Discord's client draws its emoji from, in a font
//! `build.rs` makes of them for the program to carry; see
//! `vendor/twemoji/README.yapfast.md`. The font is mapped from the program's file,
//! so what it holds costs memory only as far as it's read, and it can be given
//! back: message text draws it as glyphs ([`crate::emoji_text`]).
//!
//! A cell asks for the picture of an emoji at the size the grid draws it, and
//! gets a decoded image back, which it keeps while it shows it: the pictures are
//! 72px square, and drawn from there at whatever the display needs.

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

thread_local! {
    static PICTURES: RefCell<HashMap<(String, u32), Option<slint::Image>>> = RefCell::default();
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

/// Whether Twemoji has a picture for `glyph`, without decoding one.
#[cfg(test)]
pub fn has_picture(glyph: &str) -> bool {
    picture_bytes(glyph).is_some()
}

fn decode(glyph: &str, pixels: u32) -> Option<slint::Image> {
    let decoded = image::DynamicImage::ImageRgba8(published(glyph)?);
    let scaled = if (decoded.width(), decoded.height()) == (pixels, pixels) {
        decoded
    } else {
        decoded.resize_exact(pixels, pixels, image::imageops::FilterType::Triangle)
    };
    let pixels_buffer = scaled.into_rgba8();
    let buffer = slint::SharedPixelBuffer::clone_from_slice(pixels_buffer.as_raw(), pixels, pixels);
    Some(slint::Image::from_rgba8(buffer))
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
    use crate::emoji_font_tables::TWEMOJI_MARGIN as MARGIN;
    let bitmap =
        image::load_from_memory_with_format(picture_bytes(glyph)?, image::ImageFormat::Png)
            .ok()?
            .into_rgba8();
    let side = bitmap.width() - 2 * MARGIN;
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
    fn an_emoji_has_twemojis_picture_at_the_size_the_grid_draws() {
        let drawn = picture("🍕", 36., 1.).expect("Twemoji draws a pizza");
        assert_eq!((drawn.size().width, drawn.size().height), (36, 36));
        // A display at twice the scale gets twice the pixels for the same size.
        let scaled = picture("🍕", 36., 2.).expect("kept for this size");
        assert_eq!((scaled.size().width, scaled.size().height), (72, 72));
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
