//! Theme files: `yapfast-theme` JSON, read and written as the Slint app does
//! (`src/ui/theme.rs:880`-`:1030`).

use std::collections::BTreeMap;

use yapfast_lang::tr;

use super::{assemble, token_index, written};
use crate::Palette;

/// What a theme file says in its `format`, so that a file of anything else
/// is turned away.
const FILE_FORMAT: &str = "yapfast-theme";

/// The version of the file, which grows only for a change an older Yapfast
/// would read wrongly; a colour added later needs none, as a missing one
/// takes the base's.
const FILE_VERSION: u32 = 1;

/// The most a theme file may hold. One holds about 2 KiB.
pub const FILE_LIMIT: u64 = 64 * 1024;

/// The longest name a theme taken from a file keeps, in characters.
const NAME_LIMIT: usize = 48;

/// A theme as shared in a file: the same fields as `layout.json` keeps, but
/// no id, which is the reader's to give, and the format and version first.
#[derive(serde::Serialize, serde::Deserialize)]
struct Shared {
    format: String,
    version: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    base: String,
    #[serde(default)]
    dark: Option<bool>,
    #[serde(default)]
    colours: BTreeMap<String, String>,
}

/// A theme read from a file, ready to be kept among the person's own.
#[derive(Debug, PartialEq)]
pub struct Imported {
    pub name: String,
    pub base: &'static str,
    pub palette: Palette,
    /// Colours of the file whose text wasn't a colour, which keep the base's.
    pub skipped: usize,
}

/// Why a file isn't taken as a theme.
#[derive(Debug, PartialEq, Eq)]
pub enum ImportError {
    /// The file couldn't be read, and why, as the system says.
    Unreadable(String),
    TooLarge,
    NotATheme,
    Newer,
    Empty,
}

impl ImportError {
    /// What Appearance says of it, in the interface's language.
    pub fn text(&self) -> String {
        match self {
            Self::Unreadable(error) => tr!("Couldn't read the file: {}", error),
            Self::TooLarge => tr!("That file is too large to be a theme.").to_owned(),
            Self::NotATheme => tr!("That file is not a theme for Yapfast.").to_owned(),
            Self::Newer => {
                tr!("That theme was made by a newer Yapfast. Update to use it.").to_owned()
            }
            Self::Empty => tr!("That theme names no colours Yapfast knows.").to_owned(),
        }
    }
}

/// A theme as the text of a file to share, indented so that people can read
/// and edit it.
pub fn export(name: &str, base: &str, palette: &Palette) -> String {
    let shared = Shared {
        format: FILE_FORMAT.into(),
        version: FILE_VERSION,
        name: name.into(),
        base: base.into(),
        dark: Some(palette.dark),
        colours: written(palette),
    };
    let mut text = serde_json::to_string_pretty(&shared).expect("a theme serializes");
    text.push('\n');
    text
}

/// Reads the theme file at `path`, no more of it than a theme file may
/// hold. It waits on the disk, so it runs off the window's thread.
pub fn read(path: &std::path::Path) -> Result<Imported, ImportError> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(FILE_LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|error| ImportError::Unreadable(error.to_string()))?;
    import(&bytes)
}

/// Reads a theme file's text. A name or colour it doesn't know is left out,
/// and a colour it lacks takes the base's, as in `layout.json`, so that a
/// file from another version of Yapfast, or edited by hand, loads.
pub fn import(bytes: &[u8]) -> Result<Imported, ImportError> {
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(ImportError::TooLarge);
    }
    // A text editor, such as Windows' Notepad, can start the file with a
    // byte order mark, which JSON doesn't allow.
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let shared: Shared = serde_json::from_slice(bytes).map_err(|_| ImportError::NotATheme)?;
    if shared.format != FILE_FORMAT {
        return Err(ImportError::NotATheme);
    }
    if shared.version > FILE_VERSION {
        return Err(ImportError::Newer);
    }
    if !shared.colours.keys().any(|key| token_index(key).is_some()) {
        return Err(ImportError::Empty);
    }
    let (base, palette, skipped) = assemble(&shared.base, shared.dark, &shared.colours);
    Ok(Imported {
        name: clean_name(&shared.name),
        base,
        palette,
        skipped,
    })
}

/// Unicode's format characters that only disguise text: zero-width ones and
/// those that change the direction text reads in.
fn hidden(c: char) -> bool {
    matches!(
        c,
        '\u{00ad}'
            | '\u{061c}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}'
    )
}

/// A character a name from a file may not hold. The joiner of emoji
/// sequences (U+200D) is allowed.
fn refused(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{2028}' | '\u{2029}' | '\u{e0000}'..='\u{e007f}')
        || (c != '\u{200d}' && hidden(c))
}

/// A name from a file, without those characters or more than a card can
/// show; "Imported theme" when nothing is left.
fn clean_name(name: &str) -> String {
    let name: String = name
        .chars()
        .filter(|&c| !refused(c))
        .take(NAME_LIMIT)
        .collect();
    match name.trim() {
        "" => tr!("Imported theme").into(),
        name => name.into(),
    }
}

/// A name for a theme file to save as: the theme's name, as long as one
/// from a file may be, with what a file name shouldn't hold swapped for
/// `-`, then `.yapfast-theme.json`.
pub fn file_name(name: &str) -> String {
    let stem: String = name
        .chars()
        .take(NAME_LIMIT)
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    match stem.trim().trim_matches('-') {
        "" => "theme.yapfast-theme.json".into(),
        stem => format!("{stem}.yapfast-theme.json"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::token;
    use super::*;
    use crate::builtin::BUILT_IN;

    #[test]
    fn an_exported_theme_imports_back() {
        let palette = BUILT_IN[4].palette;
        let text = export("Mine", BUILT_IN[4].id, &palette);
        let back = import(text.as_bytes()).unwrap();
        assert_eq!((back.name.as_str(), back.base), ("Mine", BUILT_IN[4].id));
        assert_eq!((back.palette, back.skipped), (palette, 0));
        assert_eq!(import(b"{}"), Err(ImportError::NotATheme));
        let newer = br#"{"format":"yapfast-theme","version":9}"#;
        assert_eq!(import(newer), Err(ImportError::Newer));
        let none = br##"{"format":"yapfast-theme","version":1,"colours":{"zzz":"#000"}}"##;
        assert_eq!(import(none), Err(ImportError::Empty));
    }

    #[test]
    fn a_file_the_slint_app_wrote_imports() {
        // As `export` in `src/ui/theme.rs` writes it, cut to a few colours,
        // with the byte order mark a text editor may add.
        let file = "\u{feff}".to_owned()
            + r##"{
  "format": "yapfast-theme",
  "version": 1,
  "name": "Evening",
  "base": "nord",
  "dark": false,
  "colours": {
    "accent": "#ff8800",
    "accent-soft": "#5865f226",
    "glow": "#123456",
    "on-accent": "#000",
    "row-hover": "#1f1f23"
  }
}
"##;
        let theme = import(file.as_bytes()).unwrap();
        assert_eq!(
            (theme.name.as_str(), theme.base, theme.skipped),
            ("Evening", "nord", 0)
        );
        let nord = crate::builtin::find("nord").unwrap().palette;
        assert!(!theme.palette.dark && nord.dark);
        assert_eq!(theme.palette.accent, crate::rgb(0xff8800));
        assert_eq!(theme.palette.on_accent, crate::rgb(0x000000));
        assert_eq!(theme.palette.accent_soft, crate::rgba(0x5865f226));
        assert_eq!(theme.palette.row_hover, crate::rgb(0x1f1f23));
        assert_eq!(theme.palette.rail, nord.rail);
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(file_name("a/b: c"), "a-b- c.yapfast-theme.json");
        assert_eq!(file_name("///"), "theme.yapfast-theme.json");
        assert_eq!(clean_name("Ni\u{202e}ce\n"), "Nice");
        assert_eq!(token("on_accent"), "on-accent");
    }
}
