//! Twemoji in message text: each emoji in a text is rewritten to its glyph in the
//! colour bitmap font `build.rs` makes of Twemoji's pictures ([`crate::twemoji`]),
//! so it wraps and selects like a letter. One font holds every emoji, so reading
//! one costs what drawing a letter does: Skia's glyph cache, which has a limit,
//! and pages of the program's file that can be given back. A click on one opens
//! its card. An emoji Twemoji has no picture for stays the system font's.

use std::{cell::RefCell, collections::HashMap, sync::Once};

use slint::fontique_012::fontique;

use crate::{emoji_font, emoji_font_tables::TWEMOJI_LAST, twemoji};

/// Where Twemoji's glyphs end: they count down from the top of the Supplementary
/// Private Use Area B, and stay clear of custom emoji's, which count up from its
/// bottom ([`crate::custom_emoji`]).
const FIRST: u32 = 0x10_0000;
const LAST: u32 = TWEMOJI_LAST;

/// The link an emoji's glyph opens, then the emoji's code points in lower-case
/// hexadecimal, such as `1f62d`, or `c/` and a custom emoji's id.
pub const LINK: &str = "yapfast:emoji/";

thread_local! {
    /// The pictures the cards showed, by the emoji.
    static CARDS: RefCell<HashMap<String, slint::Image>> = RefCell::default();
}

/// The most pictures a card keeps; Discord's cards come one at a time.
const MOST_PICTURES: usize = 32;

/// The link that opens the card of the emoji with these code points.
pub fn unicode_link(emoji: &str) -> String {
    let points: Vec<String> = emoji
        .chars()
        .map(|character| format!("{:x}", character as u32))
        .collect();
    format!("{LINK}{}", points.join("-"))
}

/// The link that opens the card of the custom emoji with this id.
pub fn custom_link(id: &str) -> String {
    format!("{LINK}c/{id}")
}

/// What a link in a text points at, for the card it opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// An emoji of Unicode's own, by the emoji itself.
    Unicode(String),
    /// A Discord server's custom emoji, by its id.
    Custom(String),
}

impl Target {
    /// A custom emoji's id, or none for one of Unicode's own.
    pub fn custom(&self) -> Option<&str> {
        match self {
            Self::Custom(id) => Some(id),
            Self::Unicode(_) => None,
        }
    }
}

/// Reads `link` as [`unicode_link`] and [`custom_link`] write it.
pub fn target(link: &str) -> Option<Target> {
    let found = link.strip_prefix(LINK)?;
    if let Some(id) = found.strip_prefix("c/") {
        return (!id.is_empty()).then(|| Target::Custom(id.to_owned()));
    }
    let emoji: Option<String> = found
        .split('-')
        .map(|point| u32::from_str_radix(point, 16).ok().and_then(char::from_u32))
        .collect();
    emoji.filter(|emoji| !emoji.is_empty()).map(Target::Unicode)
}

/// Puts Twemoji's font in text's fallback chain. The font stays where the program
/// holds it: nothing copies it.
pub fn register() {
    static REGISTERED: Once = Once::new();
    REGISTERED.call_once(|| {
        let font = fontique::Blob::new(std::sync::Arc::new(twemoji::FONT));
        if !emoji_font::register_blob("Yapfast Twemoji", font) {
            tracing::warn!("could not register a font of Twemoji");
        }
    });
}

/// `text` with each emoji that has a glyph drawn by it.
pub fn text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    rewrite(text, &mut out, |emoji| {
        twemoji::glyph(emoji).map(String::from)
    });
    out
}

/// `text` with each emoji that has a glyph drawn by it, in a link that opens
/// the emoji's card.
pub fn linked(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    rewrite(text, &mut out, |emoji| {
        twemoji::glyph(emoji).map(|glyph| format!("[{glyph}](<{}>)", unicode_link(emoji)))
    });
    out
}

/// `text` into `out`, each emoji it shows replaced by what `each` gives it, or
/// kept as typed when it gives nothing. Anything in code, or in a link an
/// earlier pass made, is kept as typed.
fn rewrite(text: &str, out: &mut String, mut each: impl FnMut(&str) -> Option<String>) {
    let mut code = false;
    let mut rest = text;
    while let Some(next) = rest.chars().next() {
        if next == '`' {
            code = !code;
        }
        if !code {
            if next == '['
                && let Some(end) = link_end(rest)
            {
                out.push_str(&rest[..end]);
                rest = &rest[end..];
                continue;
            }
            if could_be_emoji(next) {
                let length = emoji_at(rest);
                if length > 0
                    && let Some(replacement) = each(&rest[..length])
                {
                    out.push_str(&replacement);
                    rest = &rest[length..];
                    continue;
                }
            }
            // A shortcode sent unexpanded, because its completion was ignored,
            // shows as its emoji, as Discord's client draws one.
            if next == ':'
                && let Some((length, emoji)) = shortcode_at(rest)
                && let Some(replacement) = each(emoji)
            {
                out.push_str(&replacement);
                rest = &rest[length..];
                continue;
            }
        }
        out.push(next);
        rest = &rest[next.len_utf8()..];
    }
}

/// Whether `character` is one of these glyphs, or a custom emoji's.
fn is_glyph(character: char) -> bool {
    (FIRST..=LAST).contains(&(character as u32))
}

/// Whether an emoji could start at `character`, so the walk looks for one:
/// emoji are beyond ASCII, but the digits 0 to 9, `#` and `*` begin keycaps.
fn could_be_emoji(character: char) -> bool {
    !is_glyph(character)
        && (!character.is_ascii() || character.is_ascii_digit() || "#*".contains(character))
}

/// The length of the longest emoji `text` starts with, or 0 when it starts
/// with none.
fn emoji_at(text: &str) -> usize {
    let ends: Vec<usize> = text
        .char_indices()
        .map(|(index, character)| index + character.len_utf8())
        .take(12)
        .collect();
    ends.iter()
        .rev()
        .find(|&&end| emojis::get(&text[..end]).is_some())
        .copied()
        .unwrap_or(0)
}

/// The emoji `text` names at its start between colons, such as `:sob:`, and how
/// many bytes that name takes.
fn shortcode_at(text: &str) -> Option<(usize, &'static str)> {
    let rest = text.strip_prefix(':')?;
    let end = rest.find(':')?;
    let name = &rest[..end];
    let shaped = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-+".contains(character));
    if !shaped {
        return None;
    }
    emojis::get_by_shortcode(name).map(|emoji| (end + 2, emoji.as_str()))
}

/// The length of the Markdown link `text` starts with, when `text` starts with
/// one, so the emoji in a link's text stay as its sender wrote them.
fn link_end(text: &str) -> Option<usize> {
    let target = text.find("](")? + 2;
    let mut in_brackets = false;
    for (index, character) in text[target..].char_indices() {
        match character {
            '<' => in_brackets = true,
            '>' => in_brackets = false,
            ')' if !in_brackets => return Some(target + index + 1),
            _ => {}
        }
    }
    None
}

/// `emoji` as a PNG scaled up from Twemoji's 72 pixels, for the card that shows
/// it larger than text does.
fn square_png(emoji: &str) -> Option<Vec<u8>> {
    use image::{ImageEncoder, imageops};
    let side = emoji_font::SIDE;
    let decoded = twemoji::published(emoji)?;
    // Scaled with its alpha premultiplied: mixed on straight colour, the
    // filter's overshoot where a picture fades out leaves a row of its colour
    // at an alpha too small to show alone and too wide to miss, which draws as
    // a line of it under the emoji at text's size.
    let scaled = imageops::resize(
        &premultiplied(&decoded),
        side,
        side,
        imageops::FilterType::CatmullRom,
    );
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            unpremultiplied(&scaled).as_raw(),
            side,
            side,
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(png)
}

/// `image` with each colour scaled by its alpha, as a filter may mix it: a
/// pixel it all but erased then stays a pixel's worth of nothing.
pub(super) fn premultiplied(image: &image::RgbaImage) -> image::RgbaImage {
    let mut out = image.clone();
    for pixel in out.pixels_mut() {
        let alpha = u32::from(pixel[3]);
        for channel in 0..3 {
            pixel[channel] = ((u32::from(pixel[channel]) * alpha + 127) / 255) as u8;
        }
    }
    out
}

/// `image` with its alpha divided out again, as a picture is stored.
pub(super) fn unpremultiplied(image: &image::RgbaImage) -> image::RgbaImage {
    let mut out = image.clone();
    for pixel in out.pixels_mut() {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 {
            pixel.0[..3].fill(0);
            continue;
        }
        for channel in 0..3 {
            let colour = (u32::from(pixel[channel]) * 255).div_ceil(alpha);
            pixel[channel] = u8::try_from(colour.min(255)).unwrap_or(u8::MAX);
        }
    }
    out
}

/// `emoji`'s picture at [`emoji_font::SIDE`] pixels, for a card that shows it
/// larger than text does: one upscale from Twemoji's 72 pixels.
pub fn picture(emoji: &str) -> Option<slint::Image> {
    if let Some(picture) = CARDS.with_borrow(|cards| cards.get(emoji).cloned()) {
        return Some(picture);
    }
    let side = emoji_font::SIDE;
    let png = square_png(emoji)?;
    let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    let picture = slint::Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(
        decoded.as_raw(),
        side,
        side,
    ));
    CARDS.with_borrow_mut(|cards| {
        if cards.len() >= MOST_PICTURES {
            cards.clear();
        }
        cards.insert(emoji.to_owned(), picture.clone());
    });
    Some(picture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_links_it_writes() {
        let joined = "👨‍👩‍👧";
        assert_eq!(
            target(&unicode_link("😭")),
            Some(Target::Unicode("😭".to_owned()))
        );
        assert_eq!(
            target(&unicode_link(joined)),
            Some(Target::Unicode(joined.to_owned()))
        );
        assert_eq!(
            target(&custom_link("42")),
            Some(Target::Custom("42".to_owned()))
        );
        assert_eq!(target("https://example.com"), None);
        assert_eq!(target(LINK), None);
        assert_eq!(target("yapfast:emoji/c/"), None);
    }

    #[test]
    fn finds_the_emoji_a_text_starts_with() {
        assert_eq!(emoji_at("😭 ok"), "😭".len());
        assert_eq!(emoji_at("👨‍👩‍👧!"), "👨‍👩‍👧".len());
        assert_eq!(emoji_at("❤️"), "❤️".len());
        assert_eq!(emoji_at("ok"), 0);
    }

    #[test]
    fn keeps_the_emoji_of_links_and_code_as_typed() {
        let text = "a [😭](<yapfast:emoji/1f62d>) `😭` 😭 [x](<https://example.com/a_(b)>) 😭";
        let mut found = Vec::new();
        let mut out = String::new();
        rewrite(text, &mut out, |emoji| {
            found.push(emoji.to_owned());
            Some("EMOJI".to_owned())
        });
        assert_eq!(found, ["😭", "😭"]);
        assert_eq!(
            out,
            "a [😭](<yapfast:emoji/1f62d>) `😭` EMOJI [x](<https://example.com/a_(b)>) EMOJI"
        );
    }

    #[test]
    fn gives_each_emoji_one_glyph() {
        let (first, second) = (twemoji::glyph("😭").unwrap(), twemoji::glyph("🎉").unwrap());
        assert_ne!(first, second);
        assert_eq!(twemoji::glyph("😭"), Some(first));
        assert!(is_glyph(first) && is_glyph(second));
        assert!(emoji_at("😭") > 0);
    }

    #[test]
    fn reads_the_shortcodes_a_text_names() {
        let mut out = String::new();
        let mut each = |emoji: &str| Some(format!("<{emoji}>", emoji = emoji));
        rewrite(
            "hi :sob: there :nope: 12:30: `:sob:` [:sob:](<https://a/b>) :wave:",
            &mut out,
            &mut each,
        );
        assert_eq!(
            out,
            "hi <😭> there :nope: 12:30: `:sob:` [:sob:](<https://a/b>) <👋>"
        );
    }

    /// A filter mixes a picture's colour times its alpha, so what fades out
    /// fades to nothing rather than to a darker shade of itself: mixed on
    /// straight colour, the overshoot leaves a line of it under an emoji at
    /// text's size.
    #[test]
    fn a_filter_mixes_a_pictures_alpha_with_its_colour() {
        use image::imageops;
        let mut image = image::RgbaImage::new(2, 1);
        image.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        let mixed = imageops::resize(&premultiplied(&image), 3, 1, imageops::FilterType::Triangle);
        let middle = unpremultiplied(&mixed).get_pixel(1, 0).0;
        assert!(
            (100..160).contains(&middle[3]),
            "halfway to nothing is half alpha: {middle:?}"
        );
        assert!(middle[0] > 240, "and still red, not dark red: {middle:?}");
    }

    /// The font `build.rs` makes, as Skia reads it where the program holds it.
    fn typeface() -> skia_safe::Typeface {
        // SAFETY: the font is in the program, and stays where it is.
        let data = unsafe { skia_safe::Data::new_bytes(twemoji::FONT) };
        skia_safe::FontMgr::new()
            .new_from_data(data, None)
            .expect("Skia loads the font")
    }

    #[test]
    fn a_glyph_draws_at_the_emoji_it_is_too() {
        let typeface = typeface();
        let glyph = twemoji::glyph("😭").unwrap();
        let drawn = typeface.unichar_to_glyph(glyph as i32);
        assert!(drawn > 0);
        assert_eq!(
            typeface.unichar_to_glyph(0x1F62D),
            drawn,
            "the emoji's own code point draws it, as the composer keeps it"
        );
        assert_eq!(typeface.unichar_to_glyph('x' as i32), 0, "and nothing else");
        // These join others, which this font can't do, so the system's font must
        // keep them: a thumb and a skin tone would draw as two pictures here.
        assert_eq!(typeface.unichar_to_glyph(0x1F3FD), 0, "a skin tone");
        assert_eq!(typeface.unichar_to_glyph(0x1F1E9), 0, "half a flag");
        assert_eq!(typeface.unichar_to_glyph(0x200D), 0, "a joiner");
        // What needs a selector to be an emoji stays text until it has one.
        assert_eq!(typeface.unichar_to_glyph(0x00A9), 0, "the copyright sign");
    }

    #[test]
    fn a_sequence_has_a_glyph_of_its_own() {
        let typeface = typeface();
        for emoji in ["👨‍👩‍👧", "🇩🇰", "1️⃣", "👍🏽", "❤️"] {
            let glyph = twemoji::glyph(emoji).expect(emoji);
            assert!(typeface.unichar_to_glyph(glyph as i32) > 0, "{emoji}");
        }
    }

    #[test]
    fn the_font_draws_its_pictures_in_colour_at_texts_size() {
        use skia_safe::{Color, Font, Paint, Point, image::CachingHint, surfaces};
        let typeface = typeface();
        let glyph = typeface.unichar_to_glyph(twemoji::glyph("🟥").unwrap() as i32);
        let mut surface = surfaces::raster_n32_premul((64, 64)).unwrap();
        surface.canvas().clear(Color::WHITE);
        surface.canvas().draw_glyphs_at(
            &[glyph],
            &[Point::new(10.0, 40.0)][..],
            Point::default(),
            &Font::from_typeface(typeface, 16.0),
            &Paint::default(),
        );
        let image = surface.image_snapshot();
        let mut pixels = vec![0u8; 64 * 64 * 4];
        assert!(image.read_pixels(
            image.image_info(),
            &mut pixels,
            64 * 4,
            (0, 0),
            CachingHint::Allow
        ));
        let (red, blue) = if image.image_info().color_type() == skia_safe::ColorType::BGRA8888 {
            (2, 0)
        } else {
            (0, 2)
        };
        let painted = pixels
            .chunks(4)
            .filter(|pixel| pixel[red] > 150 && pixel[1] < 90 && pixel[blue] < 90)
            .count();
        // 16 pixel text draws a picture 22 pixels on a side, the square most of it.
        assert!((200..=484).contains(&painted), "{painted} red pixels");
    }

    /// Skia scales a bitmap glyph without smoothing its edges. 😄 reaches all four of
    /// its picture's, so without a margin it came out flat where the cut fell, at some
    /// sizes and positions and not others.
    #[test]
    fn a_picture_keeps_its_round_edges_wherever_it_lands() {
        use skia_safe::{Font, Point, image::CachingHint, surfaces};
        let typeface = typeface();
        let glyph = typeface.unichar_to_glyph(twemoji::glyph("😄").unwrap() as i32);
        const SIZE: usize = 96;
        for text_size in [16.0, 17.6, 19.2, 20.0, 24.0, 32.0] {
            for shift in [0.0, 0.25, 0.5, 0.75] {
                let mut font = Font::from_typeface(typeface.clone(), text_size);
                font.set_subpixel(true);
                let mut surface = surfaces::raster_n32_premul((SIZE as i32, SIZE as i32)).unwrap();
                surface.canvas().draw_glyphs_at(
                    &[glyph],
                    &[Point::new(20.0 + shift, 60.0)][..],
                    Point::default(),
                    &font,
                    &skia_safe::Paint::default(),
                );
                let image = surface.image_snapshot();
                let mut pixels = vec![0u8; SIZE * SIZE * 4];
                assert!(image.read_pixels(
                    image.image_info(),
                    &mut pixels,
                    SIZE * 4,
                    (0, 0),
                    CachingHint::Allow
                ));
                let alpha = |x: usize, y: usize| pixels[(y * SIZE + x) * 4 + 3];
                let rows: Vec<usize> = (0..SIZE)
                    .filter(|&y| (0..SIZE).any(|x| alpha(x, y) > 0))
                    .collect();
                let columns: Vec<usize> = (0..SIZE)
                    .filter(|&x| (0..SIZE).any(|y| alpha(x, y) > 0))
                    .collect();
                let (height, width) = (rows.len(), columns.len());
                let at = format!("{text_size} pixel text, shifted {shift}");
                assert!(
                    width.abs_diff(height) <= 1,
                    "{width} wide, {height} tall at {at}"
                );
                // A circle's outermost pixels are few, and a cut one's many.
                let solid = |count: usize| count * 100 / width;
                let top = (0..SIZE).filter(|&x| alpha(x, rows[0]) > 200).count();
                let bottom = (0..SIZE)
                    .filter(|&x| alpha(x, *rows.last().unwrap()) > 200)
                    .count();
                let left = (0..SIZE).filter(|&y| alpha(columns[0], y) > 200).count();
                let right = (0..SIZE)
                    .filter(|&y| alpha(*columns.last().unwrap(), y) > 200)
                    .count();
                for (edge, count) in [
                    ("top", top),
                    ("bottom", bottom),
                    ("left", left),
                    ("right", right),
                ] {
                    assert!(
                        solid(count) < 40,
                        "the {edge} is flat, {count} of {width} pixels, at {at}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_cards_picture_is_the_size_asked_for() {
        let png = square_png("⬛").expect("a picture of the black square");
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (emoji_font::SIDE, emoji_font::SIDE));
        for pixel in image.pixels() {
            if pixel[3] == 0 {
                assert_eq!(pixel.0, [0, 0, 0, 0], "nothing shows where nothing is");
            }
        }
    }
}
