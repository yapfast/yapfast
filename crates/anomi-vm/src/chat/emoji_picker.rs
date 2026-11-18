//! The emoji picker: what it shows and what it asks for. Task C17
//! (docs/iced/tasks/chat.md), porting what `EmojiPicker`, `PickerRail`,
//! `EmojiButton` and `EmojiFace` (`ui/messages.slint:1223`, `:116`, `:179`,
//! `:64`) draw, as `show_emoji` (`src/ui/callbacks.rs:426`), `rows` in
//! `src/emoji.rs:450` and `src/ui/emoji_grid.rs` fill them.
//!
//! Discord's picker: a search, the emoji in sections under their headings
//! (the account's favourites, those it uses most, each server's, then
//! Unicode's categories), a rail down the left that jumps to each section,
//! and the emoji under the pointer named at the foot. The sections live here
//! whole; the grid cuts them into rows of [`EmojiPicker::columns`]
//! ([`Grid`]), and the screen builds only the rows near its view. The app
//! loads the pictures of the custom emoji near the rows in sight and lets go
//! of those far from them ([`EmojiPicker::wanted`],
//! [`EmojiPicker::let_go_far`]).

mod build;
mod emoji;
mod grid;
mod sight;

use crate::kind::Expected;

pub use build::{categories, found};
pub use emoji::{Custom, Emoji};
pub use grid::{Grid, Row};
pub use sight::{FETCH_ROWS, SHOWN_ROWS};

/// What a picked emoji goes to, which the picker's foot names before an
/// emoji is pointed at (`react-item`, `ui/messages.slint:1243`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Purpose {
    /// A reaction to a message; the app keeps which.
    #[default]
    Reaction,
    /// The message being written (`yapfast:composer`).
    Composer,
    /// The message being edited (`yapfast:edit`).
    Edit,
    /// The custom status (`yapfast:status`).
    Status,
    /// A poll's answer (`yapfast:poll`), named as a reaction is.
    Poll,
}

/// What shows a section in the rail, and before its heading.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SectionIcon {
    /// The account's favourites: the icon set's star (`SectionGlyph.star`).
    #[default]
    Favorites,
    /// What it uses most: the icon set's clock (`SectionGlyph.clock`).
    Frequent,
    /// One of Unicode's categories, shown by one of its emoji.
    Category(&'static emojis::Emoji),
    /// A server's emoji, shown by its icon, or by its first emoji's picture
    /// when it has none; `Waiting` while that comes.
    Server { id: String, picture: Expected },
}

/// A section of the grid: a heading and its emoji (`EmojiSection` and the
/// rows of `EmojiRow` under it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    /// Its name in the chosen language: the heading shows it in capitals,
    /// the foot as it is when its rail button is pointed at.
    pub name: String,
    pub icon: SectionIcon,
    /// At least one: a section with none isn't listed.
    pub emoji: Vec<Emoji>,
}

/// What the pointer is over, which the foot names (`emoji-hover`).
#[derive(Clone, Debug, PartialEq)]
pub enum Hovered {
    /// An emoji of the grid.
    Emoji(Emoji),
    /// A section's button in the rail, by its index in
    /// [`EmojiPicker::sections`].
    Section(usize),
}

/// The emoji picker: what its screen draws. The window holds it as
/// `vm.chat.emoji_picker`; the picker above the composer shows it too.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmojiPicker {
    pub purpose: Purpose,
    /// What is searched for: the field's text. With anything in it but
    /// colons and spaces the grid shows [`Self::found`].
    pub query: String,
    /// Every section, which the rail lists, and the grid too unless a
    /// search shows its matches (`emoji-sections`, `rail-sections`).
    pub sections: Vec<Section>,
    /// The search's matches, under no heading: the server's emoji whose
    /// name holds the query, then Unicode's ([`found`]).
    pub found: Vec<Emoji>,
    /// The keys of this account's favourites, which carry a star
    /// (`EmojiFavorites`).
    pub favorites: Vec<String>,
    pub hovered: Option<Hovered>,
    /// The emoji a row of the grid holds, from the width it shows at
    /// ([`columns`]): six in the popup, more in the picker above the
    /// composer as it's dragged wider.
    pub columns: usize,
    /// The rows in sight, first and last, as the grid last reported
    /// ([`Intent::Shown`]).
    pub shown: (usize, usize),
}

impl EmojiPicker {
    /// Whether the grid shows a search's matches: its query holds more than
    /// colons and spaces, as `:smile` searches for "smile".
    pub fn searching(&self) -> bool {
        !self.query.trim().trim_matches(':').is_empty()
    }

    /// The grid's rows, as its columns cut the sections or the matches.
    pub fn grid(&self) -> Grid<'_> {
        Grid::new(self)
    }

    /// Whether `emoji` is one of this account's favourites.
    pub fn favorite(&self, emoji: &Emoji) -> bool {
        self.favorites.iter().any(|key| key == emoji.key())
    }

    /// The section the rail marks: the one the grid's top is in, none while
    /// a search shows its matches.
    pub fn marked(&self) -> Option<usize> {
        self.grid().section_of(self.shown.0)
    }

    /// The first match of a search, which Enter picks.
    pub fn first_match(&self) -> Option<&Emoji> {
        self.searching().then(|| self.found.first()).flatten()
    }
}

/// The popup's width, which its grid's columns fit (`emoji-popup-width`,
/// `ui/yapfast.slint:2245`), and its height (`:2650`).
pub const POPUP_WIDTH: f32 = 408.0;
pub const POPUP_HEIGHT: f32 = 432.0;

/// A button of the grid is 56 px, as Discord's (`src/emoji.rs:65`).
pub const CELL: f32 = 56.0;
/// The rail beside the grid and the grid's padding take this much of the
/// width it shows at (`BESIDE`, `src/emoji.rs:79`).
const BESIDE: f32 = 64.0;

/// The emoji per row of a grid showing at `width`: as many as it holds, one
/// even when it is narrower (`emoji::columns`, `src/emoji.rs:86`).
pub fn columns(width: f32) -> usize {
    (((width - BESIDE) / CELL).floor().max(0.0) as usize).max(1)
}

/// The emoji picker: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The search field's text after each edit (`emoji-search`): the grid
    /// shows its matches, from their top (the grid's id follows the query).
    Search(String),
    /// An emoji was clicked, or Enter pressed with matches: the first
    /// (`pick`). The app gives it to what the picker is for and closes it.
    Pick(Emoji),
    /// A locked emoji was pressed: offer Nitro, which unlocks it
    /// (`unlock-nitro`).
    Locked,
    /// An emoji's star: keep it among the favourites, or let it go
    /// (`favorite`), by its key.
    Favorite(String),
    /// The pointer came over an emoji or a rail's section, which the foot
    /// names until another (`emoji-hover`). Only [`EmojiPicker::hovered`]
    /// changes: the grid's sections needn't be built again.
    Hover(Hovered),
    /// A rail's section was pressed: the grid goes to its heading, after
    /// clearing a search: `list::operation::jump_to(grid_id(""), key, Top)`
    /// with the section's [`Grid::heading_key`] (`grid_id` is the screen's,
    /// `anomi_chat::pickers::emoji`).
    Jump(usize),
    /// The rows in sight, first and last, as the grid scrolled or was laid
    /// out again (`scrolled-to`): [`EmojiPicker::shown`] takes them, so the
    /// rail marks the section at the top, and the app asks for the pictures
    /// [`EmojiPicker::wanted`] lists and lets go of those
    /// [`EmojiPicker::let_go_far`] finds.
    Shown(usize, usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_columns_follow_the_width() {
        assert_eq!(columns(POPUP_WIDTH), 6);
        assert_eq!(columns(498.0), 7);
        assert_eq!(columns(10.0), 1, "one even when too narrow");
    }

    #[test]
    fn colons_and_spaces_search_for_nothing() {
        let mut picker = EmojiPicker {
            query: " :: ".into(),
            ..EmojiPicker::default()
        };
        assert!(!picker.searching());
        picker.query = ":smile".into();
        assert!(picker.searching());
    }
}
