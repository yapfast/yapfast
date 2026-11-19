//! A rail folder's settings, as Discord's dialog has them
//! (`discord-folder-settings-dialog.png` among the owner's references; the
//! Slint window has none): its name, in a field whose placeholder is the
//! name the rail gives it without one, and its colour, the default one, a
//! custom one picked with the profile editor's colour picker, or one of
//! Discord's twenty. Done applies both; Escape, the X and a press outside
//! leave them. The shell draws it as `Layer::FolderSettings`, and the
//! picker under the custom swatch as `Layer::FolderColour`.

use iced::Color;

use crate::menu::Spot;
use crate::people::editor_preview::{Change, Colour, format_hex};
use crate::rail::FOLDER_COLOR;

/// The dialog's view-model: what the window holds as `vm.rail.settings`
/// while it shows, the folder's as it opened and as it has been changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FolderSettings {
    /// The folder's id.
    pub id: String,
    /// The name as typed; empty for none.
    pub name: String,
    /// The name the rail gives it without one: its servers'.
    pub placeholder: String,
    /// The colour chosen as RGB: `None` for the default, [`FOLDER_COLOR`].
    pub colour: Option<u32>,
    /// The custom swatch's colour: the folder's own when none of the
    /// presets is it, or the one last picked; none before either.
    pub custom: Option<u32>,
    /// The colours offered under the two large swatches, a row of ten
    /// bright ones then ten dark: Discord's twenty, as a role's.
    pub presets: Vec<u32>,
    /// The picker's state while it shows: the custom colour as it's
    /// picked, its code as typed.
    pub pick: Colour,
}

/// What a click or a key in the dialog asks for. Keep `#[non_exhaustive]`:
/// the app matches it with a wildcard until it handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The name was edited, as typed.
    Name(String),
    /// The default swatch (`None`) or a preset was chosen.
    Colour(Option<u32>),
    /// The custom swatch was pressed, where it is in the window: its
    /// colour is chosen and the picker opens under it.
    Custom(Spot),
    /// The picker's pad, hue bar or code changed.
    Pick(Change),
    /// Done, or Enter: the name and colour are applied.
    Done,
    /// The X: nothing is applied.
    Close,
    /// What the picker's card sends besides its colour, which is nothing.
    Ignored,
}

impl FolderSettings {
    /// The dialog opening on folder `id`, whose name is `name` (empty for
    /// none) and colour `colour`, the rail naming it `placeholder` without
    /// one, with `presets` to choose from.
    pub fn opened(
        id: &str,
        name: &str,
        placeholder: &str,
        colour: Option<u32>,
        presets: &[u32],
    ) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            placeholder: placeholder.to_owned(),
            colour,
            custom: colour.filter(|rgb| !presets.contains(rgb)),
            presets: presets.to_vec(),
            pick: Colour::default(),
        }
    }

    /// The name as typed, kept to `limit` characters.
    pub fn rename(&mut self, text: &str, limit: usize) {
        self.name = text.chars().take(limit).collect();
    }

    /// The default swatch (`None`) or a preset chosen.
    pub fn choose(&mut self, colour: Option<u32>) {
        self.colour = colour;
    }

    /// The custom swatch pressed: its colour is chosen, if it has one, and
    /// the picker starts on it, else on the colour chosen.
    pub fn open_custom(&mut self) {
        if self.custom.is_some() {
            self.colour = self.custom;
        }
        let start = self
            .custom
            .or(self.colour)
            .unwrap_or_else(|| rgb_of(FOLDER_COLOR));
        self.pick = Colour::start(&format!("{start:06x}"));
    }

    /// The picker moved: a colour it gives is the custom one, chosen; a
    /// code that isn't one stays as typed and changes nothing.
    pub fn picked(&mut self, change: Change) {
        if let Some(colour) = self.pick.apply(change) {
            let rgb = rgb_of(colour);
            self.custom = Some(rgb);
            self.colour = Some(rgb);
        }
    }

    /// The custom swatch is the colour chosen: one picked, not a preset.
    pub fn custom_chosen(&self) -> bool {
        self.colour.is_some() && self.colour == self.custom && !self.preset_chosen()
    }

    /// One of the presets is the colour chosen.
    pub fn preset_chosen(&self) -> bool {
        self.colour.is_some_and(|rgb| self.presets.contains(&rgb))
    }
}

/// A colour as RGB in one number, `0xRRGGBB`.
pub fn rgb_of(colour: Color) -> u32 {
    u32::from_str_radix(&format_hex(colour), 16).unwrap_or_default()
}

/// The colour of `0xRRGGBB`.
pub fn rgb(value: u32) -> Color {
    Color::from_rgb8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRESETS: [u32; 2] = [0x1abc9c, 0x2ecc71];

    #[test]
    fn a_folder_opens_with_its_name_and_its_colour_custom_unless_a_preset() {
        let settings = FolderSettings::opened("f", "Games", "A, B", Some(0xed4245), &PRESETS);
        assert_eq!(settings.custom, Some(0xed4245));
        assert!(settings.custom_chosen() && !settings.preset_chosen());
        let preset = FolderSettings::opened("f", "", "A, B", Some(0x2ecc71), &PRESETS);
        assert_eq!(preset.custom, None);
        assert!(preset.preset_chosen() && !preset.custom_chosen());
        let plain = FolderSettings::opened("f", "", "A, B", None, &PRESETS);
        assert!(!plain.preset_chosen() && !plain.custom_chosen());
    }

    #[test]
    fn the_name_keeps_to_its_limit() {
        let mut settings = FolderSettings::default();
        settings.rename("Raids and dungeons", 5);
        assert_eq!(settings.name, "Raids");
        settings.rename("", 5);
        assert_eq!(settings.name, "");
    }

    #[test]
    fn the_picker_starts_on_the_custom_colour_and_what_it_picks_is_chosen() {
        let mut settings = FolderSettings::opened("f", "", "", Some(0x1abc9c), &PRESETS);
        settings.open_custom();
        assert_eq!(settings.colour, Some(0x1abc9c), "no custom colour yet");
        assert_eq!(settings.pick.text, "1abc9c");
        settings.picked(Change::Text("#336699".into()));
        assert_eq!(
            (settings.custom, settings.colour),
            (Some(0x336699), Some(0x336699))
        );
        assert!(settings.custom_chosen());
        // A preset, then the custom swatch again, which chooses its own.
        settings.choose(Some(0x2ecc71));
        settings.open_custom();
        assert_eq!(settings.colour, Some(0x336699));
        // A code that isn't a colour changes nothing.
        settings.picked(Change::Text("33".into()));
        assert_eq!(settings.colour, Some(0x336699));
        settings.choose(None);
        assert_eq!(settings.colour, None);
        assert_eq!(rgb_of(FOLDER_COLOR), 0x5865f2);
        assert_eq!(rgb_of(rgb(0x336699)), 0x336699);
    }
}
