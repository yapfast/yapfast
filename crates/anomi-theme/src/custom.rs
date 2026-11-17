//! The person's own themes: a palette made from a built-in one and the
//! colours changed, kept in `layout.json`, imported and exported as
//! `yapfast-theme` files the Slint app reads too (`src/ui/theme.rs:727`-
//! `:1125`). Task S07 (docs/iced/tasks/settings.md).

mod file;

use std::collections::BTreeMap;

use iced::Color;

use crate::builtin::{self, BUILT_IN};
use crate::{Palette, Theme, rgb, rgba};

pub use file::{FILE_LIMIT, ImportError, Imported, export, file_name, import, read};

/// A colour's name as `layout.json` has it: the token with `-` for `_`.
pub fn token(name: &str) -> String {
    name.replace('_', "-")
}

/// Where a colour named as `layout.json` has it is in [`Palette::NAMES`].
fn token_index(token: &str) -> Option<usize> {
    Palette::NAMES.iter().position(|name| {
        name.bytes()
            .map(|byte| if byte == b'_' { b'-' } else { byte })
            .eq(token.bytes())
    })
}

/// A colour written `#rgb`, `#rrggbb` or `#rrggbbaa`.
pub fn parse(text: &str) -> Option<Color> {
    let digits = text.trim().strip_prefix('#')?;
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    match digits.len() {
        3 => {
            let nibble = |shift: u32| ((value >> shift) & 0xf) * 0x11;
            Some(rgb((nibble(8) << 16) | (nibble(4) << 8) | nibble(0)))
        }
        6 => Some(rgb(value)),
        8 => Some(rgba(value)),
        _ => None,
    }
}

/// A colour as `#rrggbb`, or `#rrggbbaa` when it isn't opaque.
pub fn hex(colour: Color) -> String {
    let [r, g, b, a] = colour.into_rgba8();
    match a {
        0xff => format!("#{r:02x}{g:02x}{b:02x}"),
        alpha => format!("#{r:02x}{g:02x}{b:02x}{alpha:02x}"),
    }
}

/// One of the person's own themes, as a layout keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct Custom {
    /// `custom:` and a number: no built-in id has a colon. See [`tidy`].
    pub id: String,
    pub name: String,
    /// The built-in theme it began as a copy of, whose colours stand in for
    /// any it doesn't name, such as one added since it was saved.
    pub base: &'static str,
    pub palette: Palette,
}

impl Custom {
    /// The iced theme of it.
    pub fn theme(&self) -> Theme {
        Theme::new(self.id.clone(), self.name.clone(), self.palette)
    }
}

/// A custom theme in `layout.json`: each colour as text, by its name.
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Stored {
    id: String,
    name: String,
    base: String,
    dark: Option<bool>,
    colours: BTreeMap<String, String>,
}

/// A theme's base and colours as a stored or shared file names them: the
/// base's colours with `dark` and each colour that is one laid over them.
/// Also counts the colours Yapfast knows whose text isn't a colour, which
/// keep the base's.
fn assemble(
    base: &str,
    dark: Option<bool>,
    colours: &BTreeMap<String, String>,
) -> (&'static str, Palette, usize) {
    let base = builtin::find(base).unwrap_or(&BUILT_IN[0]);
    let mut palette = base.palette;
    palette.dark = dark.unwrap_or(palette.dark);
    let mut skipped = 0;
    for (token, text) in colours {
        let Some(index) = token_index(token) else {
            continue;
        };
        match (parse(text), palette.colour_mut(index)) {
            (Some(colour), Some(slot)) => *slot = colour,
            _ => skipped += 1,
        }
    }
    (base.id, palette, skipped)
}

/// Every colour of a palette as text, by its name.
fn written(palette: &Palette) -> BTreeMap<String, String> {
    Palette::NAMES
        .iter()
        .enumerate()
        .filter_map(|(index, name)| Some((token(name), hex(palette.colour(index)?))))
        .collect()
}

impl From<Stored> for Custom {
    fn from(stored: Stored) -> Self {
        let (base, palette, _) = assemble(&stored.base, stored.dark, &stored.colours);
        Self {
            id: stored.id,
            name: stored.name,
            base,
            palette,
        }
    }
}

impl From<&Custom> for Stored {
    fn from(custom: &Custom) -> Self {
        Self {
            id: custom.id.clone(),
            name: custom.name.clone(),
            base: custom.base.to_owned(),
            dark: Some(custom.palette.dark),
            colours: written(&custom.palette),
        }
    }
}

impl serde::Serialize for Custom {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Stored::from(self).serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Custom {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Stored::deserialize(deserializer).map(Self::from)
    }
}

/// The number in a custom theme's id, if it's one Yapfast gives.
fn custom_number(id: &str) -> Option<u32> {
    id.strip_prefix("custom:")?.parse().ok()
}

/// An id for a new custom theme, after those there are.
pub fn new_id(custom: &[Custom]) -> String {
    let last = custom
        .iter()
        .filter_map(|custom| custom_number(&custom.id))
        .max()
        .unwrap_or(0);
    format!("custom:{}", last + 1)
}

/// Gives a new id to each custom theme from `layout.json` whose id Yapfast
/// wouldn't have given, or that another has, so that ids stay apart from
/// the built-in ones and from each other.
pub fn tidy(custom: &mut [Custom]) {
    let mut seen = std::collections::HashSet::new();
    let unfit: Vec<usize> = (0..custom.len())
        .filter(|&index| {
            custom_number(&custom[index].id).is_none() || !seen.insert(custom[index].id.clone())
        })
        .collect();
    for index in unfit {
        custom[index].id = new_id(custom);
    }
}

/// A theme a new one can start as.
pub struct Start<'a> {
    pub id: &'a str,
    /// The built-in theme it is or began as.
    pub base: &'static str,
    pub name: &'a str,
    pub palette: Palette,
}

/// What a new theme can start as: every theme Appearance offers, in its
/// order.
pub fn starts(custom: &[Custom]) -> impl Iterator<Item = Start<'_>> {
    BUILT_IN
        .iter()
        .map(|built_in| Start {
            id: built_in.id,
            base: built_in.id,
            name: built_in.name,
            palette: built_in.palette,
        })
        .chain(custom.iter().map(|custom| Start {
            id: &custom.id,
            base: custom.base,
            name: &custom.name,
            palette: custom.palette,
        }))
}

/// A theme Appearance offers by id, built in or the person's own.
pub fn lookup(id: &str, custom: &[Custom]) -> Option<Theme> {
    builtin::find(id)
        .map(|built_in| Theme::new(built_in.id, built_in.name, built_in.palette))
        .or_else(|| {
            custom
                .iter()
                .find(|custom| custom.id == id)
                .map(Custom::theme)
        })
}

/// The theme a layout chose, Bonfire's default for none or an unknown one.
pub fn chosen(id: Option<&str>, custom: &[Custom]) -> Theme {
    id.and_then(|id| lookup(id, custom))
        .unwrap_or_else(|| Theme::built_in(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse_and_write_back() {
        assert_eq!(parse("#fff"), Some(rgb(0xffffff)));
        assert_eq!(parse(" #1a2b3c "), Some(rgb(0x1a2b3c)));
        assert_eq!(parse("#1a2b3c80").map(hex).as_deref(), Some("#1a2b3c80"));
        assert_eq!(parse("1a2b3c"), None);
        assert_eq!(parse("#12345"), None);
        assert_eq!(hex(rgb(0x0a0b0c)), "#0a0b0c");
    }

    #[test]
    fn a_stored_theme_round_trips_and_ids_are_tidied() {
        let (base, palette, _) = assemble("nord", Some(false), &BTreeMap::new());
        let one = Custom {
            id: "custom:1".into(),
            name: "One".into(),
            base,
            palette,
        };
        let text = serde_json::to_string(&one).unwrap();
        assert_eq!(serde_json::from_str::<Custom>(&text).unwrap(), one);
        let mut all = vec![
            one.clone(),
            Custom {
                id: "x".into(),
                ..one.clone()
            },
            one,
        ];
        tidy(&mut all);
        let ids: Vec<_> = all.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["custom:1", "custom:2", "custom:3"]);
    }

    #[test]
    fn an_unknown_colour_keeps_the_bases() {
        let mut colours = BTreeMap::new();
        colours.insert("accent".to_owned(), "nonsense".to_owned());
        colours.insert("rail".to_owned(), "#102030".to_owned());
        let (_, palette, skipped) = assemble("yap", None, &colours);
        assert_eq!(skipped, 1);
        assert_eq!(palette.rail, rgb(0x102030));
        assert_eq!(palette.accent, BUILT_IN[0].palette.accent);
    }
}
