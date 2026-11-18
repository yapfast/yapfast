//! The profile editor's preview and colours: what they show and what they
//! ask for. Task P15 (docs/iced/tasks/people.md), after `ProfileTabs`
//! (`ui/server.slint:1062`) and the `ColourPopover` (`:989`) beside it.
//!
//! The preview is the tabs beside the editor's card, laid out as the full
//! profile's are: Board, Activity and Wishlist, each with what the profile
//! wears or Yapfast's words where it has nothing yet. [`Colour`] is the
//! popover's state, which the editor's theme handles and the name style
//! window share; the app keeps it with the editor and clears it with it.

mod colour;

pub use colour::{Change, Colour, format_hex, parse_hex};

use crate::people::activity::card::Card as Activity;
use crate::people::full_profile::{Widget, Wish};
use crate::people::profile::Theme;

/// A tab of the preview (`ProfileTabs.page`, 0 to 2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Page {
    #[default]
    Board,
    Activity,
    Wishlist,
}

impl Page {
    /// In the order the tabs have them.
    pub const ALL: [Page; 3] = [Page::Board, Page::Activity, Page::Wishlist];
}

/// The profile editor's preview and colours: what its screen draws. The
/// editor holds it as `Editor::preview`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorPreview {
    /// The tab shown; the preview starts on the Board.
    pub page: Page,
    /// The profile's theme, which sets the tabs' text and tiles; `None`
    /// is the interface's own.
    pub theme: Option<Theme>,
    /// What the account is doing, a card each, counted from `now` (seconds
    /// as `activity::card::Card::start` counts them).
    pub activities: Vec<Activity>,
    pub now: i64,
    /// The board's widgets and what to say of them, and the wishlist.
    pub widgets: Vec<Widget>,
    pub board_note: String,
    pub wishlist: Vec<Wish>,
    pub wishlist_note: String,
    /// The colour popover's state, while a handle's picker is open.
    pub colour: Colour,
}

impl EditorPreview {
    /// The shown tab has nothing to list: its words show in its middle
    /// (`blank`, `ui/server.slint:1076`).
    pub fn blank(&self) -> bool {
        match self.page {
            Page::Board => self.widgets.is_empty(),
            Page::Activity => self.activities.is_empty(),
            Page::Wishlist => self.wishlist.is_empty(),
        }
    }
}

/// The profile editor's preview and colours: what a click or a key on them
/// asks for. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A tab was chosen: the app asks Discord for the wishlist the first
    /// time (`open-wishlist`, `src/ui/callbacks.rs:1008`).
    Choose(Page),
    /// An activity's title or button opened this address.
    Link(String),
    /// The popover's pad, hue bar or code changed: the app calls
    /// [`Colour::apply`] and, given a colour, gives it to the handle.
    Colour(Change),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_is_blank_while_its_list_is() {
        let mut preview = EditorPreview::default();
        assert!(preview.blank());
        preview.widgets.push(Widget::default());
        assert!(!preview.blank());
        preview.page = Page::Wishlist;
        assert!(preview.blank());
    }
}
