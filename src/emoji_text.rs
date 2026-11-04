//! Twemoji in message text: each emoji in a text gets a glyph in a colour
//! bitmap font made from Twemoji's picture of it, as Discord's custom emoji do
//! ([`crate::custom_emoji`]), so it wraps and selects like a letter; see
//! [`crate::emoji_font`]. A click on one opens its card.
//!
//! The glyphs are batched: a pass of rows says which emoji its texts show with
//! [`prepare`], which makes one font of the new ones, and the texts built after
//! it draw them. An emoji Twemoji has no picture for, and one found in a text
//! before its glyph arrived, stays the system font's.

use std::{cell::RefCell, collections::HashMap};

use crate::emoji_font;

/// Where Twemoji's glyphs start: the top of the Supplementary Private Use Area
/// B, counting down, so they stay clear of custom emoji's, which count up from
/// its bottom ([`crate::custom_emoji`]).
const FIRST: u32 = 0x10_0000;
const LAST: u32 = 0x10_FFFD;

/// The link an emoji's glyph opens, then the emoji's code points in lower-case
/// hexadecimal, such as `1f62d`, or `c/` and a custom emoji's id.
pub const LINK: &str = "yapfast:emoji/";

#[derive(Default)]
struct Registry {
    /// Each emoji's glyph, by the emoji itself.
    glyphs: HashMap<String, char>,
    /// Those found since the last font, with the glyph each gets.
    queued: Vec<(String, char)>,
    /// The fonts registered so far.
    fonts: usize,
}

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::default();
}

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

/// Gives the emoji `bodies` show glyphs, in one font of those new, and says
/// whether there are any: texts built after this draw them, and those built
/// before show the system font's until they are built again.
pub fn prepare<'a>(bodies: impl IntoIterator<Item = &'a str>) -> bool {
    let queued = REGISTRY.with_borrow_mut(|registry| {
        let mut found = String::new();
        for body in bodies {
            found.clear();
            rewrite(body, &mut found, |emoji| {
                queue(registry, emoji);
                None
            });
        }
        std::mem::take(&mut registry.queued)
    });
    let mut glyphs: Vec<(char, Vec<u8>)> = Vec::with_capacity(queued.len());
    let mut made: Vec<(String, char)> = Vec::with_capacity(queued.len());
    for (emoji, glyph) in queued {
        let Some(png) = square_png(&emoji) else {
            continue;
        };
        glyphs.push((glyph, png));
        made.push((emoji, glyph));
    }
    if glyphs.is_empty() {
        return false;
    }
    let family = REGISTRY.with_borrow(|registry| format!("Yapfast Twemoji {}", registry.fonts));
    if !emoji_font::register(&family, &glyphs) {
        tracing::warn!("could not register a font of Twemoji");
        return false;
    }
    REGISTRY.with_borrow_mut(|registry| {
        registry.fonts += 1;
        registry.glyphs.extend(made);
    });
    true
}

/// `text` with each emoji that has a glyph drawn by it.
pub fn text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    rewrite(text, &mut out, |emoji| glyph(emoji).map(String::from));
    out
}

/// `text` with each emoji that has a glyph drawn by it, in a link that opens
/// the emoji's card.
pub fn linked(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    rewrite(text, &mut out, |emoji| {
        glyph(emoji).map(|glyph| format!("[{glyph}](<{}>)", unicode_link(emoji)))
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
        }
        out.push(next);
        rest = &rest[next.len_utf8()..];
    }
}

/// Gives `emoji` a glyph, for the next font.
fn queue(registry: &mut Registry, emoji: &str) {
    if registry.glyphs.contains_key(emoji)
        || registry.queued.iter().any(|(known, _)| known == emoji)
        || !crate::twemoji::has_picture(emoji)
    {
        return;
    }
    let next = LAST.saturating_sub((registry.glyphs.len() + registry.queued.len()) as u32);
    let Some(glyph) = char::from_u32(next).filter(|_| (FIRST..=LAST).contains(&next)) else {
        return;
    };
    registry.queued.push((emoji.to_owned(), glyph));
}

/// The glyph of `emoji`, once a font draws it.
fn glyph(emoji: &str) -> Option<char> {
    REGISTRY.with_borrow(|registry| registry.glyphs.get(emoji).copied())
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

/// `emoji` as a PNG of the font's square, scaled up from Twemoji's 72 pixels.
fn square_png(emoji: &str) -> Option<Vec<u8>> {
    use image::{ImageEncoder, imageops};
    let side = emoji_font::SIDE;
    let bytes = crate::twemoji::picture_bytes(emoji)?;
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    let scaled = imageops::resize(&decoded, side, side, imageops::FilterType::CatmullRom);
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(scaled.as_raw(), side, side, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(png)
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
        let (first, second) = REGISTRY.with_borrow_mut(|registry| {
            queue(registry, "😭");
            queue(registry, "😭");
            queue(registry, "🎉");
            let queued = registry.queued.clone();
            registry.queued.clear();
            (queued[0].1, queued[1].1)
        });
        assert_ne!(first, second);
        assert!(is_glyph(first) && is_glyph(second));
        assert!(emoji_at("😭") > 0);
    }
}
