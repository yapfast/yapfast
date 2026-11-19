//! Font file validation and management for custom fonts.

use std::path::{Path, PathBuf};
use yapfast_core::fonts::{
    BuiltinEmojiFont, CustomFontEntry, CustomFontKind, EmojiFontChoice, FontPrefs, TextFontChoice,
};

/// Returns the path to the fonts directory under `root` (`<root>/fonts`).
pub fn fonts_directory(root: &Path) -> PathBuf {
    root.join("fonts")
}

/// Reads the byte content of a font file, falling back to workspace root if
/// running under tests where cwd is a subcrate directory.
fn read_font_bytes(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path)
        .or_else(|orig_err| {
            if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
                let ws_root = Path::new(&manifest_dir)
                    .parent()
                    .and_then(|p| p.parent())
                    .unwrap_or_else(|| Path::new(""));
                let fallback = ws_root.join(path);
                if fallback.exists() {
                    return std::fs::read(&fallback);
                }
            }
            Err(orig_err)
        })
        .map_err(|e| format!("failed to read font file {}: {e}", path.display()))
}

/// Extracts the family name from the font face, preferring English records and
/// typographic family names.
fn extract_family_name(face: &ttf_parser::Face<'_>) -> Option<String> {
    let decode_name = |name: &ttf_parser::name::Name<'_>| -> Option<String> {
        name.to_string()
            .or_else(|| std::str::from_utf8(name.name).ok().map(|s| s.to_string()))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };

    let is_english = |name: &ttf_parser::name::Name<'_>| -> bool {
        let lang = name.language();
        lang.primary_language() == "English"
            || (name.platform_id == ttf_parser::PlatformId::Windows
                && (name.language_id & 0xFF == 0x09))
            || (name.platform_id == ttf_parser::PlatformId::Macintosh && name.language_id == 0)
    };

    // 1. Preferred Typographic Family (name_id 16) in English
    if let Some(name) = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY && is_english(n))
        .find_map(|ref n| decode_name(n))
    {
        return Some(name);
    }

    // 2. Standard Family (name_id 1) in English
    if let Some(name) = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == ttf_parser::name_id::FAMILY && is_english(n))
        .find_map(|ref n| decode_name(n))
    {
        return Some(name);
    }

    // 3. Typographic Family in any language
    if let Some(name) = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
        .find_map(|ref n| decode_name(n))
    {
        return Some(name);
    }

    // 4. Standard Family in any language
    if let Some(name) = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == ttf_parser::name_id::FAMILY)
        .find_map(|ref n| decode_name(n))
    {
        return Some(name);
    }

    // 5. Full font name (name_id 4)
    face.names()
        .into_iter()
        .filter(|n| n.name_id == ttf_parser::name_id::FULL_NAME)
        .find_map(|ref n| decode_name(n))
}

/// Inspects a font file at `path`, returning its family name and whether it contains
/// color emoji tables (e.g. CBDT/CBLC, sbix, COLR, SVG).
pub fn inspect_font_file(path: &Path) -> Result<(String, bool), String> {
    let data = read_font_bytes(path)?;
    let face =
        ttf_parser::Face::parse(&data, 0).map_err(|e| format!("failed to parse font: {e}"))?;

    let family = extract_family_name(&face)
        .ok_or_else(|| "could not find family name in font file".to_string())?;

    let has_color = face.tables().cbdt.is_some()
        || face.tables().sbix.is_some()
        || face.tables().colr.is_some()
        || face.tables().svg.is_some()
        || face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"CBDT"))
            .is_some()
        || face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"CBLC"))
            .is_some()
        || face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"sbix"))
            .is_some()
        || face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"COLR"))
            .is_some()
        || face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"SVG "))
            .is_some();

    Ok((family, has_color))
}

/// Imports a font file from `source_path` into `<root>/fonts/<id>.<ext>`.
///
/// Validates that the font is valid, creates `<root>/fonts/` if needed, generates
/// a UUID v4 identifier, copies the font, and returns a [`CustomFontEntry`].
pub fn import_font(
    root: &Path,
    source_path: &Path,
    kind: CustomFontKind,
) -> Result<CustomFontEntry, String> {
    let (family, _) = inspect_font_file(source_path)?;

    let dir = fonts_directory(root);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create fonts directory {}: {e}", dir.display()))?;

    let id = uuid::Uuid::new_v4().to_string();
    let ext = source_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("ttf");
    let filename = format!("{id}.{ext}");
    let target_path = dir.join(&filename);

    let bytes = read_font_bytes(source_path)?;
    std::fs::write(&target_path, bytes)
        .map_err(|e| format!("failed to write font file to {}: {e}", target_path.display()))?;

    Ok(CustomFontEntry {
        id,
        name: family.clone(),
        family,
        kind,
        filename,
    })
}

/// Deletes an imported custom font from `<root>/fonts/<entry.filename>`.
pub fn delete_font(root: &Path, entry: &CustomFontEntry) -> Result<(), std::io::Error> {
    let filename = Path::new(&entry.filename)
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid font filename"))?;
    let path = fonts_directory(root).join(filename);
    std::fs::remove_file(path)
}

/// Applies saved font preferences to the font system, resolving custom font files
/// in `<root>/fonts/` and falling back to defaults if any custom font file is missing.
pub fn apply_font_prefs(root: &Path, prefs: &FontPrefs) {
    let fonts_dir = fonts_directory(root);
    let mut custom_files = Vec::new();

    for entry in &prefs.custom {
        let path = fonts_dir.join(&entry.filename);
        if path.exists() {
            custom_files.push((entry.family.clone(), path));
        } else {
            tracing::warn!(
                id = %entry.id,
                family = %entry.family,
                filename = %entry.filename,
                path = %path.display(),
                "custom font file is missing"
            );
        }
    }

    let active_text = match &prefs.text {
        TextFontChoice::Builtin { builtin } => builtin.family_name(),
        TextFontChoice::Custom { id } => {
            if let Some(entry) = prefs.custom.iter().find(|c| c.id == *id) {
                let path = fonts_dir.join(&entry.filename);
                if path.exists() {
                    Some(entry.family.as_str())
                } else {
                    tracing::warn!(
                        id = %id,
                        family = %entry.family,
                        "active custom text font file missing; falling back to DM Sans"
                    );
                    Some("DM Sans")
                }
            } else {
                tracing::warn!(id = %id, "active custom text font not found; falling back to DM Sans");
                Some("DM Sans")
            }
        }
    };

    let active_emoji = match &prefs.emoji {
        EmojiFontChoice::Builtin { builtin } => Some(builtin.family_name()),
        EmojiFontChoice::Custom { id } => {
            if let Some(entry) = prefs.custom.iter().find(|c| c.id == *id) {
                let path = fonts_dir.join(&entry.filename);
                if path.exists() {
                    Some(entry.family.as_str())
                } else {
                    tracing::warn!(
                        id = %id,
                        family = %entry.family,
                        "active custom emoji font file missing; falling back to Twemoji"
                    );
                    Some(BuiltinEmojiFont::Twemoji.family_name())
                }
            } else {
                tracing::warn!(id = %id, "active custom emoji font not found; falling back to Twemoji");
                Some(BuiltinEmojiFont::Twemoji.family_name())
            }
        }
    };

    yapfast_iced_kit::fonts::apply_fonts(active_text, active_emoji, &custom_files);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_valid_dm_sans_font() {
        let path = Path::new("crates/yapfast-iced-kit/fonts/DMSans[opsz,wght].ttf");
        let (family, has_color) = inspect_font_file(path).expect("valid font");
        assert_eq!(family, "DM Sans");
        assert!(!has_color);
    }

    #[test]
    fn inspect_valid_inter_font() {
        let path = Path::new("crates/yapfast-iced-kit/fonts/Inter[opsz,wght].ttf");
        let (family, has_color) = inspect_font_file(path).expect("valid font");
        assert_eq!(family, "Inter");
        assert!(!has_color);
    }

    #[test]
    fn reject_corrupted_font() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), b"not a font at all").expect("write");
        let result = inspect_font_file(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn reject_nonexistent_font() {
        let path = Path::new("nonexistent/path/to/font.ttf");
        let result = inspect_font_file(path);
        assert!(result.is_err());
    }

    #[test]
    fn inspect_detects_color_emoji_font() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let font_path = temp_dir.path().join("emoji_test.ttf");
        let glyphs = [('\u{100000}', vec![1, 2, 3])];
        let font_bytes = crate::emoji_font_tables::build_joined(
            &crate::emoji_font_tables::TWEMOJI,
            "Test Twemoji",
            &glyphs,
            &[],
            &crate::emoji_font_tables::Joins::default(),
        );
        std::fs::write(&font_path, font_bytes).expect("write font");

        let (family, has_color) = inspect_font_file(&font_path).expect("valid emoji font");
        assert_eq!(family, "Test Twemoji");
        assert!(has_color);
    }

    #[test]
    fn fonts_directory_returns_expected_path() {
        let root = Path::new("/var/data/yapfast");
        assert_eq!(fonts_directory(root), PathBuf::from("/var/data/yapfast/fonts"));
    }

    #[test]
    fn import_and_delete_font() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let source_path = Path::new("crates/yapfast-iced-kit/fonts/DMSans[opsz,wght].ttf");

        let entry = import_font(temp_dir.path(), source_path, CustomFontKind::Text)
            .expect("import succeeds");

        assert_eq!(entry.family, "DM Sans");
        assert_eq!(entry.name, "DM Sans");
        assert_eq!(entry.kind, CustomFontKind::Text);
        assert!(entry.filename.ends_with(".ttf"));
        assert_eq!(entry.filename, format!("{}.ttf", entry.id));

        // Verify file was copied to fonts/ directory
        let copied_path = fonts_directory(temp_dir.path()).join(&entry.filename);
        assert!(copied_path.exists());
        let orig_bytes = read_font_bytes(source_path).expect("read orig");
        let copied_bytes = std::fs::read(&copied_path).expect("read copied");
        assert_eq!(orig_bytes, copied_bytes);

        // Delete the font
        delete_font(temp_dir.path(), &entry).expect("delete succeeds");
        assert!(!copied_path.exists());

        // Deleting again returns an IO error (not found)
        let err = delete_font(temp_dir.path(), &entry).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn import_emoji_font_preserves_kind() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let source_path = Path::new("crates/yapfast-iced-kit/fonts/DMSans[opsz,wght].ttf");

        let entry = import_font(temp_dir.path(), source_path, CustomFontKind::Emoji)
            .expect("import succeeds");

        assert_eq!(entry.kind, CustomFontKind::Emoji);
    }

    #[test]
    fn import_corrupted_font_fails_without_creating_file() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let corrupted_file = temp_dir.path().join("broken.ttf");
        std::fs::write(&corrupted_file, b"corrupted bytes").expect("write");

        let result = import_font(temp_dir.path(), &corrupted_file, CustomFontKind::Text);
        assert!(result.is_err());

        let fonts_dir = fonts_directory(temp_dir.path());
        if fonts_dir.exists() {
            let files: Vec<_> = std::fs::read_dir(&fonts_dir)
                .expect("read_dir")
                .collect();
            assert!(files.is_empty());
        }
    }

    #[test]
    fn apply_font_prefs_falls_back_when_file_missing() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let prefs = FontPrefs {
            text: TextFontChoice::Custom {
                id: "nonexistent-id".into(),
            },
            emoji: EmojiFontChoice::Custom {
                id: "nonexistent-emoji-id".into(),
            },
            custom: vec![
                CustomFontEntry {
                    id: "nonexistent-id".into(),
                    name: "Nonexistent".into(),
                    family: "Nonexistent".into(),
                    kind: CustomFontKind::Text,
                    filename: "nonexistent.ttf".into(),
                },
                CustomFontEntry {
                    id: "nonexistent-emoji-id".into(),
                    name: "Nonexistent Emoji".into(),
                    family: "Nonexistent Emoji".into(),
                    kind: CustomFontKind::Emoji,
                    filename: "nonexistent_emoji.ttf".into(),
                },
            ],
        };

        apply_font_prefs(temp_dir.path(), &prefs);
        assert_eq!(yapfast_iced_kit::fonts::active_fallbacks()[0], "DM Sans");
        assert_eq!(
            yapfast_iced_kit::fonts::active_emoji_family(),
            "Yapfast Twemoji"
        );
    }
}
