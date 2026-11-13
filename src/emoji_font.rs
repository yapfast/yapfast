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
