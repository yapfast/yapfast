//! Twemoji's pictures, which Discord's client draws its emoji from, packed into
//! the program at build time; see `vendor/twemoji/README.yapfast.md`.
//!
//! A cell asks for the picture of an emoji at the size the grid draws it, and
//! gets a decoded image back, which it keeps: the pictures are 72px square, and
//! drawn from there at whatever the display needs.

use std::{cell::RefCell, collections::HashMap, sync::OnceLock};

/// What `build.rs` packs: a count, then for each picture the length of its name
/// and where its bytes start and end, then the names and the bytes.
const PACKED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/twemoji.bin"));

/// The most pictures kept, which the cache lets go of at once: a filled cell
/// keeps its own, so letting them go costs a redraw of what comes next.
const MOST: usize = 1024;

thread_local! {
    static PICTURES: RefCell<HashMap<(String, u32), Option<slint::Image>>> = RefCell::default();
}

/// The picture of `glyph` at `px` logical pixels, for a display of `scale`, or
/// none when Twemoji has no picture for it.
pub fn picture(glyph: &str, px: f32, scale: f32) -> Option<slint::Image> {
    let pixels = (px * scale).round().max(1.0) as u32;
    PICTURES.with_borrow_mut(|pictures| {
        if pictures.len() >= MOST {
            pictures.clear();
        }
        pictures
            .entry((glyph.to_owned(), pixels))
            .or_insert_with(|| decode(glyph, pixels))
            .clone()
    })
}

/// Whether Twemoji has a picture for `glyph`, without decoding one.
pub fn has_picture(glyph: &str) -> bool {
    picture_bytes(glyph).is_some()
}

fn decode(glyph: &str, pixels: u32) -> Option<slint::Image> {
    let bytes = picture_bytes(glyph)?;
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?;
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

pub(crate) fn picture_bytes(glyph: &str) -> Option<&'static [u8]> {
    let pictures = index();
    for name in names(glyph) {
        if let Ok(found) = pictures.binary_search_by(|(candidate, _)| candidate.cmp(&name.as_str()))
        {
            return Some(pictures[found].1);
        }
    }
    None
}

/// What the pack holds, by name, read once.
fn index() -> &'static [(&'static str, &'static [u8])] {
    static PICTURES: OnceLock<Vec<(&'static str, &'static [u8])>> = OnceLock::new();
    PICTURES.get_or_init(|| {
        let count = u32::from_le_bytes(PACKED[0..4].try_into().expect("the pack's count")) as usize;
        // An entry is its name's length, u16, then where its picture starts and
        // how long it is, u32 each.
        const ENTRY: usize = 10;
        let names_start = 4 + count * ENTRY;
        let mut name_at = names_start;
        let mut read = Vec::with_capacity(count);
        for entry in 0..count {
            let at = 4 + entry * ENTRY;
            let name_length =
                u16::from_le_bytes(PACKED[at..at + 2].try_into().expect("a name's length"))
                    as usize;
            let picture_at = u32::from_le_bytes(
                PACKED[at + 2..at + 6]
                    .try_into()
                    .expect("a picture's start"),
            ) as usize;
            let picture_length = u32::from_le_bytes(
                PACKED[at + 6..at + 10]
                    .try_into()
                    .expect("a picture's length"),
            ) as usize;
            let name = std::str::from_utf8(&PACKED[name_at..name_at + name_length])
                .expect("a picture's name");
            read.push((name, &PACKED[picture_at..picture_at + picture_length]));
            name_at += name_length;
        }
        read
    })
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
