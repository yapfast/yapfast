//! macOS has no fontconfig, and an app started from Finder doesn't see one
//! a package manager installed. Core Text names the system's font for a
//! character instead: the font it would cascade to from Helvetica, as the
//! system's own text views do.

use std::path::Path;

use iced::advanced::graphics::text::cosmic_text::fontdb;
use objc2_core_foundation::{CFRange, CFString, CFURL};
use objc2_core_text::{CTFont, kCTFontURLAttribute};

/// The font Core Text cascades from. Any font with a full cascade list
/// does; Helvetica ships with every macOS.
const CASCADE_FROM: &str = "Helvetica";

/// The point size the lookup asks at; the choice doesn't depend on it.
const LOOKUP_SIZE: f64 = 12.0;

/// The font Core Text answers with when no font has a character: its boxes
/// are no better than the shaper's own.
const LAST_RESORT: &str = "LastResort";

/// The system's monospace faces, newest first.
const MONOSPACE_FILES: &[&str] = &[
    "/System/Library/Fonts/SFNSMono.ttf",
    "/System/Library/Fonts/Menlo.ttc",
];

/// The system's monospace face for code.
pub(super) fn monospace_file() -> Option<String> {
    MONOSPACE_FILES
        .iter()
        .find(|file| Path::new(file).exists())
        .map(|file| (*file).to_owned())
}

/// The system's font for a language. Core Text picks it per character,
/// which [`char_fonts`] asks for when the script's text lacks a glyph, so
/// there's nothing to load ahead.
pub(super) fn language_font(_language: &str) -> Option<(String, u32)> {
    None
}

/// The system's font for `chars`, as Core Text picks it: its file and face
/// index, or nothing when no font has them.
pub(super) fn char_fonts(chars: &[char]) -> Vec<(String, u32)> {
    let text: String = chars.iter().collect();
    let Some(path) = font_file_for(&text) else {
        return Vec::new();
    };
    let file = path.to_string_lossy().into_owned();
    face_indices(&path.0, &path.1)
        .into_iter()
        .map(|index| (file.clone(), index))
        .collect()
}

/// The file of the font Core Text picks for `text`, with that face's
/// PostScript name.
fn font_file_for(text: &str) -> Option<FontFile> {
    let string = CFString::from_str(text);
    let range = CFRange::new(0, string.length());
    // SAFETY: a null matrix means the identity transform.
    let base = unsafe {
        CTFont::with_name(
            &CFString::from_static_str(CASCADE_FROM),
            LOOKUP_SIZE,
            std::ptr::null(),
        )
    };
    // SAFETY: `range` lies within `string`.
    let font = unsafe { base.for_string(&string, range) };
    // SAFETY: plain Core Text getters on a live font.
    let name = unsafe { font.post_script_name() }.to_string();
    if name == LAST_RESORT {
        return None;
    }
    // SAFETY: the URL attribute key is a static Core Text constant.
    let url = unsafe { font.attribute(kCTFontURLAttribute) }?
        .downcast::<CFURL>()
        .ok()?;
    let path = url.to_file_path()?;
    Some(FontFile(path, name))
}

/// A font file and the PostScript name of the face Core Text chose in it.
struct FontFile(std::path::PathBuf, String);

impl FontFile {
    fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        self.0.to_string_lossy()
    }
}

/// The faces of the font file at `path` (a collection holds several), the
/// one named `post_script_name` first. The system's hidden UI fonts name
/// their faces differently from what Core Text reports, so the others
/// follow; the caller keeps the first that has the characters.
fn face_indices(path: &Path, post_script_name: &str) -> Vec<u32> {
    let mut db = fontdb::Database::new();
    if let Err(error) = db.load_font_file(path) {
        tracing::warn!(%error, path = %path.display(), "could not read a fallback font");
        return Vec::new();
    }
    let (mut named, rest): (Vec<_>, Vec<_>) = db
        .faces()
        .partition(|face| face.post_script_name == post_script_name);
    named.extend(rest);
    named.into_iter().map(|face| face.index).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_system_font_for_symbols_dm_sans_lacks() {
        // The separator Discord servers put in channel names.
        for c in ['・', '｜', '︱', '丨', '→'] {
            let fonts = char_fonts(&[c]);
            assert!(!fonts.is_empty(), "no font for {c}");
            assert!(Path::new(&fonts[0].0).exists());
        }
    }

    #[test]
    fn finds_nothing_for_unassigned_code_points() {
        assert!(char_fonts(&['\u{10ffff}']).is_empty());
    }

    #[test]
    fn has_a_monospace_face() {
        assert!(monospace_file().is_some());
    }
}
