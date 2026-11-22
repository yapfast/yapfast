//! Themes as packages: a `theme.json` and the files it names, which change how
//! Yapfast looks and never what it does. See `docs/themes.md`.
//!
//! A theme is data. [`package`] reads `theme.json` into named values, and
//! [`check`] decides whether a folder may be applied or listed: every colour
//! readable, every file bundled and of a kind Yapfast decodes, nothing reaching
//! outside the folder. [`motion`] works out a declared animation's value at a
//! moment, and a text effect's text, so the interface only draws what these
//! say. [`install`] files a theme into Yapfast's theme store, and the lock
//! file pins it; [`remote`] reads one from a commit of its GitHub repository,
//! and the marketplace's catalog of them.
//!
//! There are no selectors. A theme names a token, a surface, a slot or a part
//! from closed lists Yapfast publishes, and Yapfast applies the value where it
//! already draws that thing; a name it doesn't know is ignored with a note, so
//! a theme written for a later Yapfast still opens.

pub mod check;
pub mod chrome;
pub mod content;
pub mod creator;
pub mod install;
pub mod layout;
pub mod lists;
pub mod media;
pub mod motion;
pub mod options;
pub mod package;
pub mod remote;
pub mod scaffold;

pub use package::{Mode, Package, Resolved};

/// A character a theme's text may not hold: control characters, the marks
/// that turn text around, break lines or hide, and the fillers that draw as
/// nothing (a "blank" name), which would let a label reorder or disguise
/// what's beside it. The joiner emoji are made with (U+200D) and the emoji
/// variation selectors (U+FE00 to U+FE0F) are allowed, and so is the Braille
/// blank (U+2800), which takes a space's room as a space does (Phosphor pads
/// its prompt with one).
pub fn refused_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00ad}'
                | '\u{034f}'
                | '\u{061c}'
                | '\u{115f}'
                | '\u{1160}'
                | '\u{17b4}'
                | '\u{17b5}'
                | '\u{180b}'..='\u{180f}'
                | '\u{200b}'
                | '\u{200c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{2069}'
                | '\u{3164}'
                | '\u{feff}'
                | '\u{ffa0}'
                | '\u{fff9}'..='\u{fffb}'
                | '\u{1d173}'..='\u{1d17a}'
                | '\u{e0000}'..='\u{e007f}'
                | '\u{e0100}'..='\u{e01ef}'
        )
}

/// Text from a theme as Yapfast's own screens and `yapfast theme check`
/// show it: what [`refused_char`] names written out as `\u{…}`, and at most
/// 160 characters, so a name in a note can't pass for Yapfast's words or
/// push the rest off screen.
pub fn shown(text: &str) -> String {
    const MOST: usize = 160;
    let mut out = String::new();
    for (index, c) in text.chars().enumerate() {
        if index == MOST {
            out.push('…');
            break;
        }
        if refused_char(c) {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("\\u{{{:x}}}", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

/// The version of `theme.json` this Yapfast reads, which a theme names as
/// `theme_api`. Fields are only ever added within one version.
pub const THEME_API: u32 = 1;

/// What a theme's folder names its description.
pub const MANIFEST: &str = "theme.json";

/// Parts a theme may one day rebuild, and the contract version of each this
/// Yapfast offers: the fields a part receives. None are offered yet; a theme
/// that declares one draws the stock part and its page says so. A part's
/// minor version only adds fields, and a part built for another major draws
/// the stock part, so a theme never breaks on an update.
pub const PART_CONTRACTS: &[(&str, u32, u32)] = &[];

/// The parts a theme will be able to rebuild, by name, so a theme that
/// declares one is told it's for a later Yapfast rather than that the name is
/// wrong.
pub const PARTS: [&str; 5] = [
    "message-row",
    "member-row",
    "account-panel",
    "server-header",
    "profile-card",
];

/// How much of a theme runs, which Settings → Appearance sets and every
/// theme's cost is measured against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Performance {
    /// Everything, animations at the rate they ask for.
    Full,
    /// Animations marked `heavy` stay still, the rest run at most 30 times a
    /// second and pause while Yapfast isn't the focused window.
    #[default]
    Balanced,
    /// Colours, fonts, shapes and spacing; no declared animation, no text
    /// effect, and the stock hover and popup motion.
    Light,
}

impl Performance {
    pub const ALL: [Self; 3] = [Self::Full, Self::Balanced, Self::Light];

    pub fn name(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Balanced => "Balanced",
            Self::Light => "Light",
        }
    }

    pub fn index(self) -> i32 {
        match self {
            Self::Full => 0,
            Self::Balanced => 1,
            Self::Light => 2,
        }
    }

    pub fn from_index(index: i32) -> Self {
        match index {
            0 => Self::Full,
            2 => Self::Light,
            _ => Self::Balanced,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_themes_names_in_notes_stay_on_one_line_and_short() {
        let package = Package::parse(
            r#"{ "theme_api": 1, "id": "com.example.notes", "name": "Notes", "version": "1.0.0",
                "\n\n✔ Verified by Yapfast. Safe to install.\n": 1,
                "variants": { "dark": { "base": "yap" } } }"#,
        )
        .expect("read");
        assert!(!package.notes.is_empty());
        for note in &package.notes {
            assert!(!note.chars().any(refused_char), "{note:?}");
            assert!(note.chars().count() <= 161);
        }
        assert_eq!(shown("a\u{202e}b"), "a\\u{202e}b");
        assert_eq!(shown(&"x".repeat(200)).chars().count(), 161);
    }

    #[test]
    fn blank_fillers_are_refused_and_emoji_joiners_kept() {
        for blank in [
            '\u{3164}',
            '\u{115f}',
            '\u{ffa0}',
            '\u{00ad}',
            '\u{180e}',
            '\u{e0100}',
        ] {
            assert!(refused_char(blank), "{:x}", u32::from(blank));
        }
        for kept in ['\u{200d}', '\u{fe0f}', '\u{2800}', 'ก', 'ᠮ', '漢'] {
            assert!(!refused_char(kept), "{:x}", u32::from(kept));
        }
    }

    #[test]
    fn a_label_cant_turn_text_around_or_stand_without_its_data() {
        let theme = |chrome: &str| {
            Package::parse(&format!(
                r#"{{ "theme_api": 1, "id": "com.example.labels", "name": "Labels", "version": "1.0.0",
                    "chrome": {chrome},
                    "variants": {{ "dark": {{ "base": "yap" }} }} }}"#
            ))
        };
        assert!(theme(r#"{ "labels": { "typing": "{name} is typing" } }"#).is_ok());
        assert!(theme(r#"{ "labels": { "typing": "Trust & Safety is typing" } }"#).is_err());
        assert!(
            theme(r#"{ "labels": { "composer": "Session expired, type your password" } }"#)
                .is_err()
        );
        assert!(theme(r#"{ "labels": { "category": "\u2066{name}" } }"#).is_err());
    }
}
