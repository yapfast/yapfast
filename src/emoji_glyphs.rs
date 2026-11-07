//! The picker's emoji as pictures, each drawn once rather than through the
//! colour font on every frame.
//!
//! The system emoji font is a colour font (COLRv1 on Windows 11, as Segoe UI
//! Emoji is there): each emoji is a small paint graph of gradient-filled paths,
//! and Skia walks it again for every draw. A picker stretched wide shows several
//! hundred of them, so scrolling re-drew hundreds of paint graphs a frame.
//! Drawn once into a small image, a cell costs one textured quad instead.
//!
//! Only one-code-point emoji come through here; the sequences that the font
//! joins by shaping (families, flags, keycaps) keep their glyphs, and are a few
//! groups of the list.

use std::{cell::RefCell, collections::HashMap};

use skia_safe::{AlphaType, ColorType, Font, FontHinting, FontStyle, ImageInfo, surfaces};
use slint::fontique_012::shared_collection;

use crate::emoji;

/// The most pictures kept, which the cache lets go of at once: the picker's
/// cells hold their own once filled, so a redraw is what it costs, and a
/// stretched picker that had shown the whole list would otherwise hold tens of
/// megabytes of them.
const MOST: usize = 1024;

// A picture per emoji and size, in physical pixels.
thread_local! {
    static PICTURES: RefCell<HashMap<(String, u32), Option<slint::Image>>> = RefCell::default();
    static TYPEFACE: RefCell<Option<Option<skia_safe::Typeface>>> = RefCell::default();
}

/// Lets go of the pictures kept, for when nothing shows them; a cell keeps its own.
pub fn forget() {
    PICTURES.with_borrow_mut(HashMap::clear);
}

/// The emoji drawn at `px` logical pixels, for a display of `scale`; the picture
/// holds the pixels that size needs, so it stays sharp on a scaled display.
pub fn picture(glyph: &str, px: f32, scale: f32) -> Option<slint::Image> {
    let pixels = (px * scale).round().max(1.0) as u32;
    PICTURES.with_borrow_mut(|pictures| {
        if pictures.len() >= MOST {
            pictures.clear();
        }
        pictures
            .entry((glyph.to_owned(), pixels))
            .or_insert_with(|| rasterize(glyph, pixels))
            .clone()
    })
}

/// The system emoji font: as Skia's own manager finds it, or from text's font
/// collection, which is the only one that knows the font Linux registers.
fn emoji_typeface() -> Option<skia_safe::Typeface> {
    let family = emoji::family();
    let manager = skia_safe::FontMgr::new();
    manager
        .legacy_make_typeface(Some(family), FontStyle::normal())
        // It answers a name it doesn't know with a font of its own choosing.
        .filter(|typeface| typeface.family_name() == family)
        .or_else(|| {
            let font = shared_collection()
                .family_by_name(family)?
                .fonts()
                .first()?
                .load(None)?;
            // The collection shares this blob's bytes for as long as the process
            // runs, and this keeps a handle on them for the typeface, so they
            // neither move nor go. Nothing copies them.
            let font: &'static _ = Box::leak(Box::new(font));
            let bytes: &'static [u8] = font.data();
            // SAFETY: `bytes` stay in place until the process ends.
            let data = unsafe { skia_safe::Data::new_bytes(bytes) };
            manager.new_from_data(data, None)
        })
}

/// The picture of an emoji drawn at `pixels` square, or none for a sequence the
/// font would have to shape, and for a glyph the font doesn't draw.
fn rasterize(glyph: &str, pixels: u32) -> Option<slint::Image> {
    let mut characters = glyph.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    let typeface =
        TYPEFACE.with_borrow_mut(|cached| cached.get_or_insert_with(emoji_typeface).clone())?;
    let mut font = Font::from_typeface(typeface, pixels as f32);
    // As the renderer draws text, so the picture matches the glyph it replaces.
    font.set_subpixel(true);
    font.set_hinting(FontHinting::Slight);
    let id = font.unichar_to_glyph(character as _);
    if id == 0 {
        return None;
    }
    let paint = skia_safe::Paint::default();
    // Where the glyph draws, so it sits centred in the square as the grid
    // centres it: the font's own ascent is deeper than its size, so a baseline
    // taken from the metrics would push the emoji out of the picture.
    let mut bounds = [skia_safe::Rect::default()];
    font.get_bounds(&[id], &mut bounds, Some(&paint));
    if bounds[0].is_empty() {
        return None;
    }
    let centre = pixels as f32 / 2.0;
    let origin =
        skia_safe::Point::new(centre - bounds[0].center_x(), centre - bounds[0].center_y());
    let size = pixels as i32;
    let mut surface = surfaces::raster_n32_premul((size, size))?;
    surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    surface.canvas().draw_glyphs_at(
        &[id],
        skia_safe::canvas::GlyphPositions::Points(&[origin]),
        skia_safe::Point::default(),
        &font,
        &paint,
    );
    let info = ImageInfo::new((size, size), ColorType::RGBA8888, AlphaType::Premul, None);
    let mut pixels_rgba = vec![0u8; (size * size * 4) as usize];
    surface.read_pixels(&info, &mut pixels_rgba, (size * 4) as usize, (0, 0));
    // Nothing drawn, so the glyph stays a glyph.
    if pixels_rgba.iter().all(|byte| *byte == 0) {
        return None;
    }
    let buffer = slint::SharedPixelBuffer::clone_from_slice(&pixels_rgba, size as u32, size as u32);
    Some(slint::Image::from_rgba8(buffer))
}

/// Whether an emoji has a picture already, without drawing one.
#[cfg(test)]
pub fn is_cached(glyph: &str, px: f32, scale: f32) -> bool {
    let pixels = (px * scale).round().max(1.0) as u32;
    PICTURES.with_borrow(|pictures| {
        pictures
            .get(&(glyph.to_owned(), pixels))
            .is_some_and(Option::is_some)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_emoji_becomes_a_picture_of_the_size_the_display_needs() {
        let drawn = picture("🍕", 36., 1.).expect("the system emoji font draws a pizza");
        assert_eq!(
            (drawn.size().width, drawn.size().height),
            (36, 36),
            "the picture holds the pixels the size needs"
        );
        // A display at twice the scale gets twice the pixels, for the same 36.
        let scaled = picture("🍕", 36., 2.).expect("cached and drawn again");
        assert_eq!((scaled.size().width, scaled.size().height), (72, 72));
        assert!(is_cached("🍕", 36., 1.));
    }

    #[test]
    fn a_sequence_the_font_shapes_keeps_its_glyphs() {
        // A family is joined by shaping, which this picture path does not do.
        assert!(picture("👨‍👩‍👧", 36., 1.).is_none());
        // So is a flag, made of two letters the font joins.
        assert!(picture("🇩🇰", 36., 1.).is_none());
    }
}
