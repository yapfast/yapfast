//! Registering a colour bitmap font, so emoji show inline in message text:
//! Slint's `StyledText` draws only glyphs. See [`crate::emoji_font_tables`] for
//! the font itself.

use std::sync::Arc;

use slint::fontique_012::{fontique, shared_collection};

pub use crate::emoji_font_tables::{SIDE, build};

/// Registers a font with each PNG of `glyphs`, which must be [`SIDE`] pixels
/// square, at `also`'s code points mapped to a glyph's, as `family`, and puts
/// it first in text's fallback chain after the UI typeface. True when the font
/// loaded.
pub fn register(family: &str, glyphs: &[(char, Vec<u8>)], also: &[(u32, char)]) -> bool {
    register_blob(
        family,
        fontique::Blob::new(Arc::new(build(family, glyphs, also))),
    )
}

/// Registers `font` as [`register_blob`] does, and makes it the one emoji
/// font: the generic emoji family, which text with an emoji asks after its own
/// typeface, is it alone, and the system's emoji fonts leave the families text
/// falls back through, so none of their emoji show. True when the font loaded.
pub fn register_emoji(family: &str, font: fontique::Blob<u8>) -> bool {
    if !register_blob(family, font) {
        return false;
    }
    let mut collection = shared_collection();
    let Some(emoji) = collection.family_by_name(family).map(|found| found.id()) else {
        return false;
    };
    for generic in [
        fontique::GenericFamily::SansSerif,
        fontique::GenericFamily::SystemUi,
    ] {
        let families: Vec<_> = collection.generic_families(generic).collect();
        let kept: Vec<_> = families
            .into_iter()
            .filter(|id| {
                *id == emoji
                    || !collection
                        .family_name(*id)
                        .is_some_and(|name| name.to_lowercase().contains("emoji"))
            })
            .collect();
        collection.set_generic_families(generic, kept.into_iter());
    }
    collection.set_generic_families(fontique::GenericFamily::Emoji, std::iter::once(emoji));
    true
}

/// Registers `font`, as [`register`] does one it builds.
pub fn register_blob(family: &str, font: fontique::Blob<u8>) -> bool {
    let mut collection = shared_collection();
    let registered = collection.register_fonts(
        font,
        Some(fontique::FontInfoOverride {
            family_name: Some(family),
            ..Default::default()
        }),
    );
    let Some(&(family_id, _)) = registered.first() else {
        return false;
    };
    for generic in [
        fontique::GenericFamily::SansSerif,
        fontique::GenericFamily::SystemUi,
    ] {
        let families: Vec<_> = collection.generic_families(generic).collect();
        collection.set_generic_families(generic, std::iter::once(family_id).chain(families));
    }
    true
}

#[cfg(test)]
mod tests {
    /// Fedora installs only the COLRv1 build of Noto Color Emoji. Skia draws
    /// COLRv1 through FreeType, and the prebuilt Skia made for the system's
    /// FreeType drew nothing; Linux uses the one with Skia's own FreeType.
    #[test]
    fn skia_draws_colrv1_glyphs_in_colour() {
        use skia_safe::{Color, Data, Font, FontMgr, Paint, Point, image::CachingHint, surfaces};
        let typeface = FontMgr::new()
            .new_from_data(
                Data::new_copy(&crate::emoji_font_tables::tests::colrv1_square()),
                None,
            )
            .expect("Skia loads the font");
        assert_eq!(typeface.unichar_to_glyph(0x10_0000), 1);
        let mut surface = surfaces::raster_n32_premul((100, 100)).unwrap();
        surface.canvas().clear(Color::WHITE);
        surface.canvas().draw_glyphs_at(
            &[1],
            &[Point::new(10.0, 60.0)][..],
            Point::default(),
            &Font::from_typeface(typeface, 40.0),
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
        let (red, blue) = if image.image_info().color_type() == skia_safe::ColorType::BGRA8888 {
            (2, 0)
        } else {
            (0, 2)
        };
        let painted = pixels
            .chunks(4)
            .filter(|pixel| pixel[red] > 200 && pixel[1] < 80 && pixel[blue] < 80)
            .count();
        // 40 pixel text draws the square 32 pixels on a side.
        assert!((900..=1100).contains(&painted), "{painted} red pixels");
    }
}
