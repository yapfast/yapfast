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
