//! Twemoji's pictures, which Discord's client draws its emoji from, in the
//! font `build.rs` makes of them; see `vendor/twemoji/README.yapfast.md`.
//! Text in [`FAMILY`], or any text whose own font lacks an emoji (the fallback
//! `anomi_theme::fonts` sets up), draws each emoji as Twemoji's picture, a
//! sequence such as a flag or a skin tone joined into its one picture by the
//! font's ligatures. The font is in the program's file, and the font system
//! reads it where it is: what it holds costs memory only as far as it's read.
//!
//! Ports `src/twemoji.rs` and the `Twemoji` global of `ui/theme.slint`
//! ([`text`]). The pictures the Slint renderer shrank itself, swash's
//! rasterizer now scales; [`png`] hands a picture to whatever shows one
//! larger, such as an emoji's card.

use std::borrow::Cow;

use crate::tables::TWEMOJI_LAST;

/// The family the font is registered as.
pub const FAMILY: &str = "Yapfast Twemoji";

/// The font, each picture in it a glyph. A static, as a const could be a copy
/// in the program for each place that names it.
pub static FONT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/twemoji.ttf"));

/// What finds a picture in [`FONT`]: a count, then for each picture where its name
/// starts and how long it is, and where it starts in the font and how long it is,
/// then the names. The names sort.
static INDEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/twemoji.index"));

/// Where custom emoji's glyphs start, counting up; Twemoji's count down from
/// [`TWEMOJI_LAST`] and stay clear of them.
const FIRST: u32 = 0x10_0000;

/// The code point of the glyph in [`FONT`] that draws `emoji`, which text in
/// [`FAMILY`] draws as its picture whatever font would draw its code points.
pub fn glyph(emoji: &str) -> Option<char> {
    char::from_u32(TWEMOJI_LAST - find(emoji)? as u32)
}

/// Whether Twemoji has a picture for `emoji`, without reading one.
pub fn has_picture(emoji: &str) -> bool {
    find(emoji).is_some()
}

/// The picture of `emoji` as Twemoji publishes it: a PNG 72 pixels square.
pub fn png(emoji: &str) -> Option<&'static [u8]> {
    Some(entry(find(emoji)?).1)
}

/// `text` with each emoji that has a picture as its glyph, so a reaction, a
/// poll's answer or an emoji shown by itself draws Twemoji's picture whole, a
/// keycap included, which the font's ligatures can't join from digits text
/// draws in its own font. What `ui/theme.slint`'s `Twemoji.text` gives. Text
/// with no emoji comes back as it was, without a copy.
pub fn text(text: &str) -> Cow<'_, str> {
    let mut out = String::new();
    let mut copied = 0;
    let mut at = 0;
    while let Some(next) = text[at..].chars().next() {
        let length = emoji_at(&text[at..]);
        if length > 0
            && let Some(glyph) = glyph(&text[at..at + length])
        {
            out.push_str(&text[copied..at]);
            out.push(glyph);
            at += length;
            copied = at;
            continue;
        }
        at += next.len_utf8();
    }
    if copied == 0 {
        return Cow::Borrowed(text);
    }
    out.push_str(&text[copied..]);
    Cow::Owned(out)
}

/// The length in bytes of the longest emoji `text` starts with, or 0 when it
/// starts with none. It allocates nothing, and looks nothing up for text that
/// starts with a character no emoji does, such as any letter of Russian's;
/// otherwise as many starts of `text` as an emoji has code points at most, 12.
pub fn emoji_at(text: &str) -> usize {
    if !text.chars().next().is_some_and(could_be_emoji) {
        return 0;
    }
    let mut ends = [0usize; MOST_POINTS];
    let mut count = 0;
    for ((index, character), end) in text.char_indices().zip(&mut ends) {
        *end = index + character.len_utf8();
        count += 1;
    }
    ends[..count]
        .iter()
        .rev()
        .copied()
        .find(|&end| emojis::get(&text[..end]).is_some())
        .unwrap_or(0)
}

/// Whether `character` is one of the glyphs of [`FONT`] or of a custom
/// emoji's font, rather than text.
pub fn is_glyph(character: char) -> bool {
    (FIRST..=TWEMOJI_LAST).contains(&(character as u32))
}

/// Whether an emoji could start at `character`, so the walk looks for one.
/// No picture of Twemoji's starts below U+2000 but the keycaps' 0 to 9, `#`
/// and `*`, and © and ®, so the letters of Latin, Cyrillic and every script
/// up to there are never looked up, nor is a glyph already made.
fn could_be_emoji(character: char) -> bool {
    match character {
        '0'..='9' | '#' | '*' | '©' | '®' => true,
        _ => character >= '\u{2000}' && !is_glyph(character),
    }
}

/// The most code points a picture's name has (a test checks them all), as
/// many as [`emoji_at`] tries.
const MOST_POINTS: usize = 12;

/// The longest name: each code point six digits at most, a dash between
/// each two.
const LONGEST_NAME: usize = MOST_POINTS * 6 + MOST_POINTS - 1;

/// What [`name`] writes a name into, on the stack.
type NameBuffer = [u8; LONGEST_NAME];

/// The name Twemoji may have given an emoji's picture, written into `buffer`:
/// its code points in lower-case hexadecimal, a dash between each two, and
/// without the variation selectors (U+FE0F) unless `selectors`. Most names
/// drop them (`2764.png` for ❤️, `31-20e3.png` for 1️⃣), while the sequences
/// that need one keep it, as `2764-fe0f-200d-1f525.png` does. None when the
/// name would have more code points than any picture's.
fn name<'a>(emoji: &str, selectors: bool, buffer: &'a mut NameBuffer) -> Option<&'a str> {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut length = 0;
    let mut points = 0;
    for character in emoji.chars() {
        if !selectors && character == '\u{fe0f}' {
            continue;
        }
        points += 1;
        if points > MOST_POINTS {
            return None;
        }
        if length > 0 {
            buffer[length] = b'-';
            length += 1;
        }
        let value = character as u32;
        let digits = (u32::BITS - value.leading_zeros()).div_ceil(4).max(1);
        for digit in (0..digits).rev() {
            buffer[length] = DIGITS[((value >> (digit * 4)) & 0xf) as usize];
            length += 1;
        }
    }
    std::str::from_utf8(&buffer[..length]).ok()
}

/// The place of the picture of `emoji` in the sorted list: by its name with
/// the variation selectors, then, when it has one, without them. It
/// allocates nothing.
fn find(emoji: &str) -> Option<usize> {
    if emoji.is_empty() {
        return None;
    }
    let mut buffer: NameBuffer = [0; LONGEST_NAME];
    if let Some(place) = name(emoji, true, &mut buffer).and_then(place_of) {
        return Some(place);
    }
    if !emoji.contains('\u{fe0f}') {
        return None;
    }
    name(emoji, false, &mut buffer).and_then(place_of)
}

/// The place of the picture named `name` in the sorted list.
fn place_of(name: &str) -> Option<usize> {
    let (mut low, mut high) = (0, count());
    while low < high {
        let middle = low + (high - low) / 2;
        match entry(middle).0.cmp(name) {
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle,
            std::cmp::Ordering::Equal => return Some(middle),
        }
    }
    None
}

/// How many pictures the font holds.
pub fn count() -> usize {
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

    /// The two names of `emoji`, as [`name`] writes them.
    fn names(emoji: &str) -> [Option<String>; 2] {
        let mut buffer: NameBuffer = [0; LONGEST_NAME];
        [true, false].map(|selectors| name(emoji, selectors, &mut buffer).map(str::to_owned))
    }

    /// What `find` did when it joined a `String` per code point: the place
    /// of the name with the selectors, else of the name without.
    fn found_by_strings(emoji: &str) -> Option<usize> {
        let points: Vec<String> = emoji
            .chars()
            .map(|character| format!("{:x}", character as u32))
            .collect();
        let dropped: Vec<&str> = points
            .iter()
            .map(String::as_str)
            .filter(|point| *point != "fe0f")
            .collect();
        let names = [points.join("-"), dropped.join("-")];
        names
            .iter()
            .find_map(|name| (0..count()).find(|&place| entry(place).0 == name))
    }

    #[test]
    fn the_names_are_the_code_points_and_those_without_variation_selectors() {
        let some = |name: &str| Some(name.to_owned());
        assert_eq!(names("🍕"), [some("1f355"), some("1f355")]);
        assert_eq!(names("❤️"), [some("2764-fe0f"), some("2764")]);
        assert_eq!(names("1️⃣"), [some("31-fe0f-20e3"), some("31-20e3")]);
        assert_eq!(names("\0"), [some("0"), some("0")]);
        assert_eq!(names("\u{10ffff}"), [some("10ffff"), some("10ffff")]);
        // Thirteen code points are more than any picture's name has.
        assert_eq!(names(&"👍".repeat(13)), [None, None]);
        assert_eq!(
            names(&"\u{10ffff}".repeat(12))[0].as_ref().map(String::len),
            Some(LONGEST_NAME)
        );
        // A picture Twemoji named without the selector is found all the same.
        assert_eq!(entry(find("❤️").unwrap()).0, "2764");
        assert_eq!(entry(find("1️⃣").unwrap()).0, "31-20e3");
        // One that keeps the selector is found by it.
        assert_eq!(entry(find("🏳️‍🌈").unwrap()).0, "1f3f3-fe0f-200d-1f308");
        // One it has no picture for says so.
        assert!(!has_picture("not an emoji"));
        assert!(!has_picture(""));
        assert!(!has_picture("\u{fe0f}"));
    }

    #[test]
    fn every_emoji_is_found_where_the_joined_names_found_it() {
        for emoji in ["❤️", "1️⃣", "🏳️‍🌈", "🍕", "👨‍👩‍👧", "🇩🇰", "👍🏽", "#️⃣", "©️"]
        {
            assert_eq!(find(emoji), found_by_strings(emoji), "{emoji}");
            assert!(find(emoji).is_some(), "{emoji}");
        }
        let every = emojis::iter().flat_map(|emoji| {
            std::iter::once(emoji).chain(emoji.skin_tones().into_iter().flatten())
        });
        let mut checked = 0;
        for emoji in every.step_by(7) {
            assert_eq!(
                find(emoji.as_str()),
                found_by_strings(emoji.as_str()),
                "{}",
                emoji.as_str()
            );
            checked += 1;
        }
        assert!(checked > 400, "{checked} emoji");
    }

    #[test]
    fn no_name_is_longer_than_the_buffer() {
        for place in 0..count() {
            let name = entry(place).0;
            assert!(name.split('-').count() <= MOST_POINTS, "{name}");
            assert!(name.len() <= LONGEST_NAME, "{name}");
        }
    }

    #[test]
    fn a_sequence_has_a_picture_of_its_own() {
        for emoji in ["👨‍👩‍👧", "🇩🇰", "1️⃣", "👍🏽", "🏳️‍🌈"] {
            assert!(has_picture(emoji), "{emoji}");
        }
        // Each glyph is its own, counting down from the top of the area.
        let (sob, party) = (glyph("😭").unwrap(), glyph("🎉").unwrap());
        assert_ne!(sob, party);
        assert!(is_glyph(sob) && is_glyph(party));
        assert_eq!(glyph(&sob.to_string()), None, "a glyph is no emoji");
    }

    #[test]
    fn every_picture_is_a_png_of_72_pixels() {
        assert!(count() > 3_500, "{} pictures", count());
        for place in [0, count() / 2, count() - 1] {
            let png = entry(place).1;
            assert_eq!(png[..8], *b"\x89PNG\r\n\x1a\n");
            let side = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
            assert_eq!((side(16), side(20)), (72, 72), "{}", entry(place).0);
        }
        assert_eq!(png("🍕"), Some(entry(find("🍕").unwrap()).1));
    }

    #[test]
    fn text_turns_each_emoji_into_its_glyph() {
        let pizza = glyph("🍕").unwrap();
        let keycap = glyph("1️⃣").unwrap();
        let family = glyph("👨‍👩‍👧").unwrap();
        assert_eq!(text("a 🍕 b"), format!("a {pizza} b"));
        assert_eq!(text("1️⃣👨‍👩‍👧"), format!("{keycap}{family}"));
        // Digits alone are text, and text without emoji isn't copied.
        assert!(matches!(text("12 #3 ok"), Cow::Borrowed("12 #3 ok")));
        // A glyph already made stays as it is.
        assert_eq!(text(&pizza.to_string()), pizza.to_string());
    }

    #[test]
    fn the_longest_emoji_at_the_start_wins() {
        assert_eq!(emoji_at("👍🏽!"), "👍🏽".len());
        assert_eq!(emoji_at("👍!"), "👍".len());
        assert_eq!(emoji_at("🇩🇰🇩🇰"), "🇩🇰".len());
        assert_eq!(emoji_at("hello"), 0);
        assert_eq!(emoji_at(""), 0);
    }

    #[test]
    fn letters_below_u2000_are_never_looked_up_and_no_picture_starts_there() {
        // Every picture starts with a character the walk looks at.
        for place in 0..count() {
            let name = entry(place).0;
            let first = name.split('-').next().expect("a code point");
            let first = u32::from_str_radix(first, 16).expect("hexadecimal");
            let first = char::from_u32(first).expect("a character");
            assert!(could_be_emoji(first), "{name}");
        }
        for letter in ['П', 'é', 'ß', 'Ω', 'a', ' '] {
            assert!(!could_be_emoji(letter), "{letter}");
        }
        assert_eq!(emoji_at("Привет 👍"), 0);
        assert!(matches!(text("Привет, мир"), Cow::Borrowed(_)));
        let thumb = glyph("👍").unwrap();
        assert_eq!(text("Привет 👍"), format!("Привет {thumb}"));
    }
}
