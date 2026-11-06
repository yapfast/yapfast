//! Discord's custom emoji as text: each emoji whose picture has arrived gets a
//! code point in the Supplementary Private Use Area B, drawn by a font made for
//! it; see [`crate::emoji_font`]. Each batch of new emoji is a font of its own,
//! which text falls back to first after the UI typeface. Icon fonts such as
//! Nerd Fonts take code points in area A, so these stay clear of them.
//!
//! Only emoji in message text need glyphs. The picker, suggestions and
//! reaction buttons show an emoji's picture as an image, so the hundreds a
//! picker lists don't each add a font for text to fall back through.

use std::{cell::RefCell, collections::HashMap};
use yapfast_core::chat::{CustomEmoji, Picture};

use crate::emoji_font;

/// The Supplementary Private Use Area B.
const FIRST: u32 = 0x10_0000;
const LAST: u32 = 0x10_FFFD;

#[derive(Default)]
struct Registry {
    /// Each emoji with a glyph, by id.
    glyphs: HashMap<String, char>,
    /// Each emoji's picture, by id, as shown on its own.
    images: HashMap<String, slint::Image>,
    /// The fonts registered so far.
    fonts: usize,
}

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::default();
}

/// The character that draws the emoji with this id, once its picture is in a font.
pub fn glyph(id: &str) -> Option<char> {
    REGISTRY.with_borrow(|registry| registry.glyphs.get(id).copied())
}

/// The emoji's picture, once it has arrived.
pub fn image(id: &str) -> Option<slint::Image> {
    REGISTRY.with_borrow(|registry| registry.images.get(id).cloned())
}

/// Lets go of the pictures kept of emoji shown on their own, which the picker's
/// cells hold while they show and the disk cache holds after; what a look shows
/// next is put back by [`add_images`] from the pictures still in memory.
pub fn forget_images() {
    REGISTRY.with_borrow_mut(|registry| registry.images.clear());
}

/// Keeps the pictures that arrived of emoji shown on their own. True when
/// there are new ones.
pub fn add_images<'a>(arrived: impl IntoIterator<Item = (&'a str, &'a Picture)>) -> bool {
    REGISTRY.with_borrow_mut(|registry| {
        let mut added = false;
        for (id, picture) in arrived {
            if registry.images.contains_key(id) {
                continue;
            }
            registry
                .images
                .insert(id.to_owned(), crate::ui::slint_image(picture));
            added = true;
        }
        added
    })
}

/// Gives the emoji whose pictures arrived glyphs. True when there are new ones,
/// which text that showed their names can now show.
pub fn add<'a>(arrived: impl IntoIterator<Item = (&'a str, &'a Picture)>) -> bool {
    REGISTRY.with_borrow_mut(|registry| {
        let mut glyphs: Vec<(char, Vec<u8>)> = Vec::new();
        let mut ids: Vec<(String, char)> = Vec::new();
        for (id, picture) in arrived {
            if registry.glyphs.contains_key(id) || ids.iter().any(|(known, _)| known == id) {
                continue;
            }
            let next = FIRST + (registry.glyphs.len() + ids.len()) as u32;
            let Some(glyph) = char::from_u32(next).filter(|_| next <= LAST) else {
                break;
            };
            let Some(png) = square_png(picture) else {
                continue;
            };
            glyphs.push((glyph, png));
            ids.push((id.to_owned(), glyph));
        }
        if glyphs.is_empty() {
            return false;
        }
        let family = format!("Yapfast Custom Emoji {}", registry.fonts);
        if !emoji_font::register(&family, &glyphs, &[]) {
            tracing::warn!("could not register a font of custom emoji");
            return false;
        }
        registry.fonts += 1;
        registry.glyphs.extend(ids);
        true
    })
}

/// `markdown` with the name of each of `emoji` that has a glyph, such as
/// `:pog:`, replaced by it.
pub fn text(markdown: &str, emoji: &[CustomEmoji]) -> String {
    replace(markdown, emoji, false)
}

/// [`text`], each glyph in a link that opens the emoji's card: what a message's
/// body shows.
pub fn linked(markdown: &str, emoji: &[CustomEmoji]) -> String {
    replace(markdown, emoji, true)
}

fn replace(markdown: &str, emoji: &[CustomEmoji], link: bool) -> String {
    let mut out = markdown.to_owned();
    for each in emoji {
        if let Some(glyph) = glyph(&each.id)
            && out.contains(&each.text)
        {
            let shown = if link {
                format!(
                    "[{}](<{}>)",
                    glyph,
                    crate::emoji_text::custom_link(&each.id)
                )
            } else {
                glyph.to_string()
            };
            out = out.replace(&each.text, &shown);
        }
    }
    out
}

/// Whether `text` is nothing but emoji, at most 30, as Discord shows large.
pub fn only_emoji(text: &str) -> bool {
    let mut count = 0;
    for word in text.split_whitespace() {
        let mut rest = word;
        while let Some(first) = rest.chars().next() {
            let length = if (FIRST..=LAST).contains(&(first as u32)) {
                first.len_utf8()
            } else {
                // The longest emoji the rest starts with, such as a family.
                let ends: Vec<usize> = rest
                    .char_indices()
                    .map(|(index, c)| index + c.len_utf8())
                    .take(12)
                    .collect();
                let Some(&end) = ends
                    .iter()
                    .rev()
                    .find(|&&end| emojis::get(&rest[..end]).is_some())
                else {
                    return false;
                };
                end
            };
            rest = &rest[length..];
            count += 1;
        }
    }
    (1..=30).contains(&count)
}

/// A picture fitted into the font's square, centred, as a PNG, with the margin the
/// font's bitmaps have.
fn square_png(picture: &Picture) -> Option<Vec<u8>> {
    use image::{ImageEncoder, RgbaImage, imageops};
    let side = emoji_font::SIDE;
    let source = RgbaImage::from_raw(picture.width, picture.height, picture.rgba.to_vec())?;
    let scale = side as f32 / picture.width.max(picture.height).max(1) as f32;
    let (width, height) = (
        ((picture.width as f32 * scale).round() as u32).clamp(1, side),
        ((picture.height as f32 * scale).round() as u32).clamp(1, side),
    );
    let fitted = imageops::resize(&source, width, height, imageops::FilterType::CatmullRom);
    let margin = crate::emoji_font_tables::MARGIN;
    let bitmap = side + 2 * margin;
    let mut square = RgbaImage::new(bitmap, bitmap);
    imageops::overlay(
        &mut square,
        &fitted,
        i64::from(margin + (side - width) / 2),
        i64::from(margin + (side - height) / 2),
    );
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            square.as_raw(),
            bitmap,
            bitmap,
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::fontique_012::fontique;
    use std::sync::Arc;

    #[test]
    fn makes_fonts_that_load_with_their_glyphs() {
        let picture = Picture {
            width: 4,
            height: 2,
            rgba: yapfast_core::chat::Pixels::new(4, 2, [255, 0, 0, 255].repeat(8)),
            ..Default::default()
        };
        let png = square_png(&picture).expect("encodes");
        let font = emoji_font::build(
            "Test Emoji",
            &[('\u{100000}', png.clone()), ('\u{100001}', png)],
            &[],
        );
        let mut collection = fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let registered = collection.register_fonts(fontique::Blob::new(Arc::new(font)), None);
        let (family, fonts) = registered.first().expect("a family");
        assert_eq!(fonts.len(), 1);
        assert_eq!(collection.family_name(*family), Some("Test Emoji"));
    }

    #[test]
    fn skia_draws_the_glyphs_in_colour() {
        use skia_safe::{Color, Data, Font, FontMgr, Paint, Point, image::CachingHint, surfaces};
        let red = Picture {
            width: 4,
            height: 4,
            rgba: yapfast_core::chat::Pixels::new(4, 4, [255, 0, 0, 255].repeat(16)),
            ..Default::default()
        };
        let font = emoji_font::build(
            "Probe Emoji",
            &[('\u{100000}', square_png(&red).unwrap())],
            &[],
        );
        let typeface = FontMgr::new()
            .new_from_data(Data::new_copy(&font), None)
            .expect("Skia loads the font");
        let mut surface = surfaces::raster_n32_premul((100, 100)).unwrap();
        surface.canvas().clear(Color::WHITE);
        let font = Font::from_typeface(typeface, 40.0);
        surface.canvas().draw_glyphs_at(
            &[1],
            &[Point::new(10.0, 60.0)][..],
            Point::default(),
            &font,
            &Paint::default(),
        );
        let image = surface.image_snapshot();
        let mut pixels = vec![0u8; 100 * 100 * 4];
        assert!(image.read_pixels(
            image.image_info(),
            &mut pixels,
            400,
            (0, 0),
            CachingHint::Allow
        ));
        let painted = pixels
            .chunks(4)
            .filter(|pixel| {
                let color = image.image_info().color_type();
                let (r, b) = if color == skia_safe::ColorType::BGRA8888 {
                    (pixel[2], pixel[0])
                } else {
                    (pixel[0], pixel[2])
                };
                r > 200 && pixel[1] < 80 && b < 80
            })
            .count();
        // 40 pixel text draws the emoji 1.36 em square.
        assert!((2500..=3300).contains(&painted), "{painted} red pixels");
    }

    #[test]
    fn knows_messages_of_only_emoji() {
        assert!(only_emoji("👍"));
        assert!(only_emoji(" 👨‍👩‍👧 \u{100001}\u{100002} ❤️ "));
        assert!(!only_emoji("👍 ok"));
        assert!(!only_emoji(""));
        assert!(!only_emoji(&"👍".repeat(31)));
    }
}
