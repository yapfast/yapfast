//! The quick switcher: what it shows and what it asks for. Task N10
//! (docs/iced/tasks/shell.md), porting `QuickSwitcher` (`ui/dialogs.slint:364`)
//! and its rows, `PlaceRow` (`ui/dialogs.slint:15`), as `src/ui/refresh.rs:3899`
//! fills them.
//!
//! Discord's quick switcher, which Ctrl+K opens: a search of the
//! conversations and servers to go to, those opened lately before a search.
//! The query, the chosen row and the places live here, so the window
//! projects them from the core (`AppState::switcher` and `switcher_list`)
//! and the screen only draws.

use crate::icon::Icon;
use crate::kind::Avatar;

/// What a place is marked with: a person's or a server's round picture, or
/// the icon of the kind of channel.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// A DM's person or group, or a server (also the server a channel is
    /// in, in Forward To).
    Avatar(Avatar),
    /// A channel: `Icon::Hash`, `Volume`, `MessageSquareText` for a forum,
    /// `Thread`, with their lock or warning variants.
    Icon(Icon),
}

impl Default for Mark {
    fn default() -> Self {
        Self::Icon(Icon::Hash)
    }
}

/// A place to go to: a conversation or a server (`PlaceRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Place {
    /// What `Intent::Go` carries back.
    pub id: String,
    pub name: String,
    /// Where it is: "Direct message", a server's name, "Server", "Channel".
    pub place: String,
    pub mark: Mark,
    /// Unread and not muted: its name in bold.
    pub unread: bool,
    /// Its mentions of this account as a count (`kind::count_badge`); empty
    /// for none.
    pub badge: String,
}

/// A section's heading, shown before a search only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading {
    Mentions,
    Previous,
}

/// One row of the list: a heading, or the place at an index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Heading(Heading),
    Place(usize),
}

impl Entry {
    /// The list's key for the entry: a place's is its index, the headings'
    /// are past any index.
    pub fn key(self) -> u64 {
        match self {
            Self::Place(index) => index as u64,
            Self::Heading(Heading::Mentions) => u64::MAX,
            Self::Heading(Heading::Previous) => u64::MAX - 1,
        }
    }
}

/// The quick switcher: what its screen draws. The window holds it as
/// `vm.switcher`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Switcher {
    /// What is searched for: the field's text.
    pub query: String,
    /// The places, in the order the core lists them.
    pub places: Vec<Place>,
    /// How many of the first places have mentions, and sit under their own
    /// heading before a search (`switcher_list().1`).
    pub mentions: usize,
    /// The place Enter goes to, moved by the arrow keys. A search sets it
    /// back to 0. It may be past the end while the list shrinks; see
    /// [`Self::current`].
    pub chosen: usize,
}

impl Switcher {
    /// The place Enter goes to: the chosen one, or the last when the list
    /// got shorter than that; none with no places (`current`).
    pub fn current(&self) -> Option<usize> {
        self.places
            .len()
            .checked_sub(1)
            .map(|last| self.chosen.min(last))
    }

    /// Whether the list has its headings: before a search.
    pub fn headed(&self) -> bool {
        self.query.is_empty()
    }

    /// The places with mentions, never more than there are places.
    fn mentioned(&self) -> usize {
        if self.headed() {
            self.mentions.min(self.places.len())
        } else {
            0
        }
    }

    /// How many rows the list has, headings included.
    pub fn rows(&self) -> usize {
        let places = self.places.len();
        if !self.headed() || places == 0 {
            return places;
        }
        let mentioned = self.mentioned();
        places + 1 + usize::from(mentioned > 0 && mentioned < places)
    }

    /// What row `row` of the list is: a heading starts the places with
    /// mentions and the others (`starts`, `ui/dialogs.slint:383`).
    pub fn entry(&self, row: usize) -> Entry {
        if !self.headed() {
            return Entry::Place(row);
        }
        let mentioned = self.mentioned();
        if mentioned == 0 {
            return match row {
                0 => Entry::Heading(Heading::Previous),
                _ => Entry::Place(row - 1),
            };
        }
        match row {
            0 => Entry::Heading(Heading::Mentions),
            row if row <= mentioned => Entry::Place(row - 1),
            row if row == mentioned + 1 => Entry::Heading(Heading::Previous),
            row => Entry::Place(row - 2),
        }
    }

    /// The place with `id`, if listed.
    pub fn find(&self, id: &str) -> Option<usize> {
        self.places.iter().position(|place| place.id == id)
    }
}

/// The quick switcher: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
///
/// Closing is the shell's (`shell::Intent::Close(Layer::Switcher)`: Escape,
/// a press outside). The app's I06 handles these.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The query was edited (`SearchSwitcher`). The app lists the places
    /// again, sets `chosen` back to 0 and scrolls the list to its top
    /// (`edited`, `ui/dialogs.slint:441`).
    Search(String),
    /// An arrow key or Tab chose the place at this index. The app sets
    /// `chosen` and scrolls the place into view (`list::operation::jump_to`
    /// with `Place::Reveal(0.0)` and the key `Entry::Place(i).key()`, by the
    /// list's id `anomi_screens::switcher::LIST`; the first place of a
    /// section shows its heading too, as Slint's `reveal` does).
    Choose(usize),
    /// Enter, or a click: go to the place with this id (`SwitchTo`), which
    /// closes the switcher.
    Go(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn switcher(places: usize, mentions: usize, query: &str) -> Switcher {
        Switcher {
            query: query.into(),
            places: (0..places)
                .map(|n| Place {
                    id: n.to_string(),
                    ..Place::default()
                })
                .collect(),
            mentions,
            chosen: 0,
        }
    }

    fn entries(switcher: &Switcher) -> Vec<Entry> {
        (0..switcher.rows())
            .map(|row| switcher.entry(row))
            .collect()
    }

    #[test]
    fn mentions_and_the_rest_each_have_a_heading() {
        use Entry::{Heading as H, Place as P};
        let list = entries(&switcher(3, 1, ""));
        assert_eq!(
            list,
            [H(Heading::Mentions), P(0), H(Heading::Previous), P(1), P(2)]
        );
        assert_eq!(
            entries(&switcher(2, 0, "")),
            [H(Heading::Previous), P(0), P(1)]
        );
        assert_eq!(
            entries(&switcher(2, 2, "")),
            [H(Heading::Mentions), P(0), P(1)]
        );
    }

    #[test]
    fn a_search_has_no_headings_and_nothing_has_no_rows() {
        use Entry::Place as P;
        assert_eq!(entries(&switcher(2, 1, "ob")), [P(0), P(1)]);
        assert!(entries(&switcher(0, 0, "")).is_empty());
    }

    #[test]
    fn a_chosen_index_past_the_end_is_clamped() {
        let mut list = switcher(3, 0, "");
        list.chosen = 9;
        assert_eq!(list.current(), Some(2));
    }
}
