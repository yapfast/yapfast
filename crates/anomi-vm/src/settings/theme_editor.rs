//! The theme editor: what it shows and what it asks for. Task S07
//! (docs/iced/tasks/settings.md). The Slint side: the editor half of
//! `AppearanceSettings` and `ThemeColourRow` (`ui/settings.slint:1285`,
//! `:1561`), set from `Editor` in `src/ui/theme.rs:1155` and
//! `src/ui/appearance.rs:45`-`:330`.

use iced::Color;

/// One colour of the theme: its name, the colour, and the text of its field,
/// which keeps what was typed even when it is no colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Colour {
    pub name: String,
    /// The last colour the text was; what the window shows.
    pub colour: Color,
    pub text: String,
    /// The text is no colour: the row says so and Save is off.
    pub wrong: bool,
}

impl Colour {
    pub fn new(name: impl Into<String>, colour: Color) -> Self {
        Self {
            name: name.into(),
            colour,
            text: hex(colour),
            wrong: false,
        }
    }
}

/// The theme editor: what its screen draws. The window holds it as
/// `vm.settings.theme_editor`; the Appearance page shows it in its place
/// while `appearance.editing`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeEditor {
    /// It edits a saved theme rather than making one: no start to pick, and
    /// it can be deleted.
    pub saved: bool,
    pub name: String,
    pub dark: bool,
    /// What a new theme can start as, and which.
    pub starts: Vec<String>,
    pub start: usize,
    /// A row for each of the 37 colours.
    pub colours: Vec<Colour>,
    /// How the last export went, under the name.
    pub note: String,
    pub note_wrong: bool,
    /// Delete was pressed once and asks again.
    pub confirming: bool,
}

/// The theme editor: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The name's text, after each edit.
    Name(String),
    /// Keep the theme among the person's own and choose it.
    Save,
    /// Close and show the theme from before.
    Cancel,
    /// Choose where to write the theme as a file.
    Export,
    /// Start again from another theme, by its place in the list.
    Start(usize),
    Dark(bool),
    /// A colour's field's text, after each edit, by the colour's place.
    Colour(usize, String),
    /// Delete was pressed: the first time it asks again.
    Delete,
    /// Delete was confirmed.
    ConfirmDelete,
}

impl ThemeEditor {
    /// Every field holds a colour and the theme can be kept or shared.
    pub fn can_save(&self) -> bool {
        !self.colours.iter().any(|row| row.wrong)
    }

    /// What an intent of the editor changes in it, for the app's update and
    /// for the simulator's tests. Starting again, saving, exporting and
    /// deleting for good are the app's, which has the themes.
    pub fn apply(&mut self, intent: &Intent) {
        match intent {
            Intent::Name(name) => self.name.clone_from(name),
            Intent::Dark(dark) => self.dark = *dark,
            Intent::Start(index) if *index < self.starts.len() => self.start = *index,
            Intent::Colour(index, text) => {
                if let Some(row) = self.colours.get_mut(*index) {
                    row.text.clone_from(text);
                    match parse(text) {
                        Some(colour) => (row.colour, row.wrong) = (colour, false),
                        None => row.wrong = true,
                    }
                }
            }
            Intent::Delete => self.confirming = true,
            _ => {}
        }
    }

    /// Starts again from another theme's colours: every field gets its text
    /// anew.
    pub fn restart(&mut self, colours: impl IntoIterator<Item = (String, Color)>, dark: bool) {
        self.colours = colours
            .into_iter()
            .map(|(name, colour)| Colour::new(name, colour))
            .collect();
        self.dark = dark;
    }
}

/// A colour written `#rgb`, `#rrggbb` or `#rrggbbaa`. `anomi_theme::custom`
/// has the same, which this crate can't reach.
fn parse(text: &str) -> Option<Color> {
    let digits = text.trim().strip_prefix('#')?;
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    let [_, r, g, b] = value.to_be_bytes();
    match digits.len() {
        3 => {
            let nibble = |shift: u32| (((value >> shift) & 0xf) * 0x11) as u8;
            Some(Color::from_rgb8(nibble(8), nibble(4), nibble(0)))
        }
        6 => Some(Color::from_rgb8(r, g, b)),
        8 => Some(Color::from_rgba8(
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            f32::from(value as u8) / 255.0,
        )),
        _ => None,
    }
}

/// A colour as `#rrggbb`, or `#rrggbbaa` when it isn't opaque.
fn hex(colour: Color) -> String {
    let [r, g, b, a] = colour.into_rgba8();
    match a {
        0xff => format!("#{r:02x}{g:02x}{b:02x}"),
        a => format!("#{r:02x}{g:02x}{b:02x}{a:02x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> ThemeEditor {
        ThemeEditor {
            colours: vec![
                Colour::new("rail", Color::BLACK),
                Colour::new("text", Color::WHITE),
            ],
            starts: vec!["A".into(), "B".into()],
            ..ThemeEditor::default()
        }
    }

    #[test]
    fn a_wrong_draft_is_kept_and_stops_saving_until_mended() {
        let mut page = editor();
        page.apply(&Intent::Colour(0, "#12".into()));
        assert!(page.colours[0].wrong && !page.can_save());
        assert_eq!(page.colours[0].text, "#12");
        assert_eq!(page.colours[0].colour, Color::BLACK);
        page.apply(&Intent::Colour(0, "#abc".into()));
        assert!(page.can_save());
        assert_eq!(page.colours[0].colour, Color::from_rgb8(0xaa, 0xbb, 0xcc));
        page.apply(&Intent::Colour(1, "#11223344".into()));
        assert_eq!(hex(page.colours[1].colour), "#11223344");
    }

    #[test]
    fn delete_asks_twice_and_a_start_is_a_place_in_the_list() {
        let mut page = editor();
        page.apply(&Intent::Delete);
        assert!(page.confirming);
        page.apply(&Intent::Start(1));
        page.apply(&Intent::Start(7));
        assert_eq!(page.start, 1);
    }
}
