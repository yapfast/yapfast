//! Font preferences and custom font configuration kept in `layout.json`.

/// Built-in text fonts bundled or standard on the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinTextFont {
    /// DM Sans, the default UI font.
    #[default]
    DmSans,
    /// Inter, an alternative clean sans-serif font.
    Inter,
    /// The desktop system's default sans-serif font.
    SystemDefault,
}

impl BuiltinTextFont {
    /// The font family name used by the font database, or `None` to query the
    /// system default sans-serif family.
    pub fn family_name(&self) -> Option<&'static str> {
        match self {
            Self::DmSans => Some("DM Sans"),
            Self::Inter => Some("Inter"),
            Self::SystemDefault => None,
        }
    }

    /// The human-readable name shown in settings.
    pub fn label(&self) -> &'static str {
        match self {
            Self::DmSans => "DM Sans",
            Self::Inter => "Inter",
            Self::SystemDefault => "System Default",
        }
    }
}

/// A choice of text font: either a built-in font or an imported custom font.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextFontChoice {
    /// A built-in font.
    Builtin { builtin: BuiltinTextFont },
    /// A custom font identified by its UUID.
    Custom { id: String },
}

impl Default for TextFontChoice {
    fn default() -> Self {
        Self::Builtin {
            builtin: BuiltinTextFont::DmSans,
        }
    }
}

/// Built-in emoji fonts bundled with Yapfast.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinEmojiFont {
    /// Twemoji, the default Discord-compatible emoji set.
    #[default]
    Twemoji,
}

impl BuiltinEmojiFont {
    /// The font family name used by the font database.
    pub fn family_name(&self) -> &'static str {
        match self {
            Self::Twemoji => "Yapfast Twemoji",
        }
    }

    /// The human-readable name shown in settings.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Twemoji => "Twemoji",
        }
    }
}

/// A choice of emoji font: either a built-in emoji font or an imported custom
/// emoji font.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EmojiFontChoice {
    /// A built-in emoji font.
    Builtin { builtin: BuiltinEmojiFont },
    /// A custom emoji font identified by its UUID.
    Custom { id: String },
}

impl Default for EmojiFontChoice {
    fn default() -> Self {
        Self::Builtin {
            builtin: BuiltinEmojiFont::Twemoji,
        }
    }
}

/// Whether an imported font provides text glyphs or emoji graphics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomFontKind {
    /// A standard text font (`.ttf` or `.otf`).
    Text,
    /// An emoji font with color or monochrome emoji glyphs.
    Emoji,
}

/// An imported custom font stored under `<data-dir>/fonts/`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CustomFontEntry {
    /// The unique identifier (e.g. UUID v4) for this font.
    pub id: String,
    /// The display name shown in settings.
    pub name: String,
    /// The font family name extracted from the font file metadata.
    pub family: String,
    /// Whether this font is for UI text or emoji rendering.
    pub kind: CustomFontKind,
    /// The file name within `<data-dir>/fonts/` (typically `<id>.<ext>`).
    pub filename: String,
}

/// Font settings persisted in `layout.json` under `"fonts"`.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FontPrefs {
    /// The selected UI text font.
    pub text: TextFontChoice,
    /// The selected emoji font.
    pub emoji: EmojiFontChoice,
    /// The list of imported custom fonts available for selection.
    pub custom: Vec<CustomFontEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_prefs_defaults_to_dm_sans_and_twemoji() {
        let prefs = FontPrefs::default();
        assert_eq!(
            prefs.text,
            TextFontChoice::Builtin {
                builtin: BuiltinTextFont::DmSans,
            }
        );
        assert_eq!(
            prefs.emoji,
            EmojiFontChoice::Builtin {
                builtin: BuiltinEmojiFont::Twemoji,
            }
        );
        assert!(prefs.custom.is_empty());
    }

    #[test]
    fn font_prefs_serializes_and_deserializes() {
        let mut prefs = FontPrefs::default();
        prefs.custom.push(CustomFontEntry {
            id: "uuid-123".into(),
            name: "Fira Code".into(),
            family: "Fira Code".into(),
            kind: CustomFontKind::Text,
            filename: "uuid-123.ttf".into(),
        });
        prefs.text = TextFontChoice::Custom {
            id: "uuid-123".into(),
        };
        let json = serde_json::to_string(&prefs).expect("serialize");
        let parsed: FontPrefs = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(prefs, parsed);
    }

    #[test]
    fn font_prefs_deserializes_empty_object_as_defaults() {
        let parsed: FontPrefs = serde_json::from_str("{}").expect("deserialize empty");
        assert_eq!(parsed, FontPrefs::default());
    }

    #[test]
    fn font_prefs_deserializes_spec_json() {
        let json = r#"{
            "text": {
                "type": "builtin",
                "builtin": "inter"
            },
            "emoji": {
                "type": "custom",
                "id": "7a3f1201-4c22-48a9-9231-314159265358"
            },
            "custom": [
                {
                    "id": "e81d8c06-1b0b-47e2-89e4-c5ec090bb6f1",
                    "name": "Comic Neue",
                    "family": "Comic Neue",
                    "kind": "text",
                    "filename": "e81d8c06-1b0b-47e2-89e4-c5ec090bb6f1.ttf"
                },
                {
                    "id": "7a3f1201-4c22-48a9-9231-314159265358",
                    "name": "JoyPixels",
                    "family": "JoyPixels",
                    "kind": "emoji",
                    "filename": "7a3f1201-4c22-48a9-9231-314159265358.ttf"
                }
            ]
        }"#;
        let parsed: FontPrefs = serde_json::from_str(json).expect("deserialize spec json");
        assert_eq!(
            parsed.text,
            TextFontChoice::Builtin {
                builtin: BuiltinTextFont::Inter,
            }
        );
        assert_eq!(
            parsed.emoji,
            EmojiFontChoice::Custom {
                id: "7a3f1201-4c22-48a9-9231-314159265358".into(),
            }
        );
        assert_eq!(parsed.custom.len(), 2);
        assert_eq!(parsed.custom[0].name, "Comic Neue");
        assert_eq!(parsed.custom[0].kind, CustomFontKind::Text);
        assert_eq!(parsed.custom[1].name, "JoyPixels");
        assert_eq!(parsed.custom[1].kind, CustomFontKind::Emoji);
    }

    #[test]
    fn builtin_labels_and_family_names() {
        assert_eq!(BuiltinTextFont::DmSans.family_name(), Some("DM Sans"));
        assert_eq!(BuiltinTextFont::Inter.family_name(), Some("Inter"));
        assert_eq!(BuiltinTextFont::SystemDefault.family_name(), None);

        assert_eq!(BuiltinTextFont::DmSans.label(), "DM Sans");
        assert_eq!(BuiltinTextFont::Inter.label(), "Inter");
        assert_eq!(BuiltinTextFont::SystemDefault.label(), "System Default");

        assert_eq!(BuiltinEmojiFont::Twemoji.family_name(), "Yapfast Twemoji");
        assert_eq!(BuiltinEmojiFont::Twemoji.label(), "Twemoji");
    }
}
