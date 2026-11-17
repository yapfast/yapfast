//! The sticker picker: what it shows and what it asks for. Task C19
//! (docs/iced/tasks/chat.md), porting `StickerPicker` (`ui/stickers.slint:31`)
//! as `show_stickers` (`src/ui/stickers.rs`) fills it.
//!
//! Discord's picker: a search, the collections down the left (Favorites,
//! Frequently Used, each server's own stickers, Discord's default packs), each
//! one's stickers under its name, 96 px square, and the sticker under the
//! pointer named at the foot. The core lists the collections with the search
//! already applied (`StickerPicker::listed`), so the sections here are what
//! shows; the grid cuts them into rows of [`StickerPicker::columns`]
//! ([`Grid`]) and the screen builds only the rows near its view. The app
//! loads the pictures of the stickers near the rows in sight and lets go of
//! those far from them ([`StickerPicker::wanted`],
//! [`StickerPicker::let_go_far`]), and of all of them when the picker closes.
//! A sticker with no picture yet draws nothing, as in Slint.

mod grid;
mod sight;

use crate::kind::Expected;

pub use grid::{Grid, Row};
pub use sight::{FETCH_ROWS, SHOWN_ROWS};

/// A tile's side, the gap between tiles and the grid's edge, a heading's
/// height: Discord's picker's (`TILE`, `GAP`, `PADDING`, `HEADING`,
/// `src/ui/stickers.rs:9`).
pub const TILE: f32 = 96.0;
pub const GAP: f32 = 8.0;
pub const PADDING: f32 = 12.0;
pub const HEADING: f32 = 28.0;

/// The picker's width by default (`picker-width`, `ui/yapfast.slint:2201`)
/// and its height where the window leaves room (`:2538`).
pub const WIDTH: f32 = 498.0;
pub const HEIGHT: f32 = 486.0;

/// The rail and the grid's padding either side take this much of the width
/// the picker shows at (`sticker-grid-width: picker-shown - 72px`,
/// `ui/yapfast.slint:682`).
const BESIDE: f32 = 72.0;

/// The stickers per row of a grid in a picker `width` wide: as many 96 px
/// tiles 8 px apart as the grid holds, hanging from the heading's left, one
/// even when it is narrower (`columns`, `src/ui/stickers.rs:20`).
pub fn columns(width: f32) -> usize {
    ((((width - BESIDE) + GAP) / (TILE + GAP)).floor().max(1.0)) as usize
}

/// A sticker of a section.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sticker {
    /// What a press sends and what the app asks the picture by.
    pub id: String,
    /// Named at the picker's foot while the pointer is over it.
    pub name: String,
    /// Its picture, moving if it is a GIF or an APNG: the app registers its
    /// frames (`picture::register`).
    pub picture: Expected,
    /// This account may use it where it is looking; one it may not shows
    /// greyed, with a lock over its corner, and a press offers Nitro.
    pub usable: bool,
}

/// What shows a section in the rail and before its heading.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SectionIcon {
    /// Frequently Used: the icon set's clock (`SectionGlyph.clock`, the
    /// heading of kind 2).
    #[default]
    Frequent,
    /// A server's own icon; `Waiting` while it comes. `id` is the server's.
    Server { id: String, picture: Expected },
    /// A section with no icon of its own (Favorites, a default pack, a
    /// server without an icon) is shown by its first sticker's picture. `id`
    /// is that sticker's.
    Sticker { id: String, picture: Expected },
}

impl SectionIcon {
    /// The picture, whatever it is of.
    pub fn picture(&self) -> Option<&Expected> {
        match self {
            Self::Frequent => None,
            Self::Server { picture, .. } | Self::Sticker { picture, .. } => Some(picture),
        }
    }
}

/// A collection: a heading and its stickers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    /// The core's id for it: `favorites`, `frequent`, a server's or a pack's.
    /// A row of the grid keeps its key by it.
    pub id: String,
    /// Its name: the heading shows it as it is, the foot names it while its
    /// rail button is pointed at.
    pub name: String,
    pub icon: SectionIcon,
    /// At least one: a collection the search leaves nothing isn't listed.
    pub stickers: Vec<Sticker>,
}

/// What the pointer is over, which the foot names (`hovered`).
#[derive(Clone, Debug, PartialEq)]
pub enum Hovered {
    /// A sticker, by its id.
    Sticker(String),
    /// A section's button in the rail, by its index in
    /// [`StickerPicker::sections`].
    Section(usize),
}

/// The sticker picker: what its screen draws. The window holds it as
/// `vm.chat.pickers.stickers`, which the picker above the composer shows as
/// its third tab.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StickerPicker {
    /// The search field's text. The sections are what it leaves.
    pub query: String,
    /// Every collection that lists, in order (`sections`).
    pub sections: Vec<Section>,
    /// The ids of this device's favourite stickers, which carry a gold star.
    pub favorites: Vec<String>,
    pub hovered: Option<Hovered>,
    /// The stickers a row of the grid holds, from the width it shows at
    /// ([`columns`]); more as the picker above the composer is dragged wider.
    pub columns: usize,
    /// The rows in sight, first and last, as the grid last reported
    /// ([`Intent::Shown`]).
    pub shown: (usize, usize),
    /// Why Discord's packs couldn't be fetched (`StickerPicker::error`):
    /// said in place of the grid while nothing lists.
    pub error: Option<String>,
}

impl StickerPicker {
    /// The grid's rows, as its columns cut the sections.
    pub fn grid(&self) -> Grid<'_> {
        Grid::new(self)
    }

    /// Whether the search is looking for something.
    pub fn searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// Whether `sticker` is one of this device's favourites.
    pub fn favorite(&self, sticker: &Sticker) -> bool {
        self.favorites.contains(&sticker.id)
    }

    /// The section the rail marks: the one the grid's top is in.
    pub fn marked(&self) -> Option<usize> {
        self.grid().section_of(self.shown.0)
    }

    /// The first sticker the grid lists, which Enter picks (`first-id`).
    pub fn first(&self) -> Option<&Sticker> {
        self.sections
            .iter()
            .find_map(|section| section.stickers.first())
    }

    /// The name the foot shows, none while nothing is pointed at.
    pub fn hovered_name(&self) -> Option<&str> {
        match self.hovered.as_ref()? {
            Hovered::Sticker(id) => self
                .sections
                .iter()
                .flat_map(|section| &section.stickers)
                .find(|sticker| sticker.id == *id)
                .map(|sticker| sticker.name.as_str()),
            Hovered::Section(index) => self.sections.get(*index).map(|s| s.name.as_str()),
        }
    }
}

/// The sticker picker: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The search field's text after each edit (`sticker-search`): the app
    /// lists the collections again and the grid starts at its top (its id
    /// follows the query).
    Search(String),
    /// A sticker was pressed, or Enter with a first one (`pick`): the app
    /// sends it and closes the picker.
    Pick(String),
    /// A locked sticker was pressed: offer Nitro, which unlocks it
    /// (`unlock-nitro`).
    Locked,
    /// A sticker's star: keep it among the favourites, or let it go
    /// (`favorite`), by its id. The foot stops naming the sticker.
    Favorite(String),
    /// The pointer came over a sticker or a rail's button, which the foot
    /// names until another (`hovered`). Only [`StickerPicker::hovered`]
    /// changes: the sections needn't be built again.
    Hover(Hovered),
    /// The pointer left what it was over: [`StickerPicker::hovered`] clears
    /// if it still names it (`else if root.hovered == tile.label`).
    Leave(Hovered),
    /// A rail's button was pressed: the grid goes to its section's heading,
    /// `list::operation::jump_to(grid_id(query), key, Top)` with the
    /// section's [`Grid::heading_key`] (`grid_id` is the screen's).
    Jump(usize),
    /// The rows in sight, first and last, as the grid scrolled or was laid
    /// out again (`scrolled-to`): [`StickerPicker::shown`] takes them, so the
    /// rail marks the section at the top, and the app asks for the pictures
    /// [`StickerPicker::wanted`] lists and lets go of those
    /// [`StickerPicker::let_go_far`] finds.
    Shown(usize, usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_columns_hang_from_the_heading() {
        // The picker's 426 px grid holds four 96 px tiles 8 px apart.
        assert_eq!(columns(WIDTH), 4);
        assert_eq!(columns(72.0 + 700.0), 6);
        assert_eq!(columns(60.0), 1, "one even when too narrow");
    }

    #[test]
    fn hovered_names_a_sticker_or_a_section() {
        let mut picker = StickerPicker {
            sections: vec![Section {
                id: "1".into(),
                name: "Server".into(),
                stickers: vec![Sticker {
                    id: "9".into(),
                    name: "Wave".into(),
                    ..Sticker::default()
                }],
                ..Section::default()
            }],
            ..StickerPicker::default()
        };
        assert_eq!(picker.hovered_name(), None);
        picker.hovered = Some(Hovered::Sticker("9".into()));
        assert_eq!(picker.hovered_name(), Some("Wave"));
        picker.hovered = Some(Hovered::Section(0));
        assert_eq!(picker.hovered_name(), Some("Server"));
        assert_eq!(picker.first().map(|s| s.id.as_str()), Some("9"));
    }
}
