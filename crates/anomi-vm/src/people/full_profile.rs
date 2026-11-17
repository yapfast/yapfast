//! The full profile: what it shows and what it asks for. Task P09
//! (docs/iced/tasks/people.md).
//!
//! A [`FullProfile`] is `ProfileModal`'s data (`ui/people.slint:2067`): the
//! person's [`Card`] on the left, drawn in [`Place::Side`], and on the right
//! tabs for what they are doing and what they share with this account
//! (`ProfileData`, `:439`). The tab shown is settled as the profile opens,
//! as in Discord's client: a Board that comes with the answer doesn't take
//! over, and a tab that goes falls back to the first for good
//! ([`FullProfile::page`], [`FullProfile::choose`]). The app tells Discord of
//! a tab the first time it shows ([`Page::asks`], `src/ui/callbacks.rs:1008`).

use iced::Rectangle;
use iced::widget::image;

use crate::kind::{Avatar, Expected, Presence};
use crate::people::profile::{Card, Tab};

/// A tab of the full profile, by what it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Activity,
    Friends,
    Servers,
    Board,
    Wishlist,
}

/// What Discord is asked for when a page opens: its answer fills the lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asks {
    Activity,
    Board,
    Wishlist,
}

impl Page {
    /// What opening it asks of Discord; the mutual tabs ask for nothing.
    pub fn asks(self) -> Option<Asks> {
        match self {
            Self::Activity => Some(Asks::Activity),
            Self::Board => Some(Asks::Board),
            Self::Wishlist => Some(Asks::Wishlist),
            Self::Friends | Self::Servers => None,
        }
    }
}

impl From<Tab> for Page {
    fn from(tab: Tab) -> Self {
        match tab {
            Tab::Friends => Self::Friends,
            Tab::Servers => Self::Servers,
        }
    }
}

/// A mutual friend: a row with their face and status (`ProfileFriendRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Friend {
    pub id: String,
    pub name: String,
    pub avatar: Avatar,
    pub presence: Presence,
}

/// A mutual server: a row with its icon, and the name they go by there
/// (`ProfileServerRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub initial: String,
    pub icon: Expected,
    pub nick: String,
}

/// Something they did lately, on the Activity tab (`ProfileRecentRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recent {
    pub title: String,
    pub subtitle: String,
    pub initial: String,
    pub picture: Expected,
    pub music: bool,
    pub when: String,
}

/// A game on a board's widget (`ProfileGameRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Game {
    pub name: String,
    pub initial: String,
    pub icon: Expected,
    pub comment: String,
}

/// A widget on their Discord board (`ProfileWidgetRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Widget {
    pub title: String,
    pub games: Vec<Game>,
    /// It holds something shown only in Discord.
    pub elsewhere: bool,
}

/// An item on their wishlist (`ProfileWishlistRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wish {
    pub name: String,
    pub initial: String,
    pub picture: Expected,
}

/// The full profile: what its screen draws. The window holds it as
/// `vm.people.full_profile`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FullProfile {
    /// The left column: shown in `Place::Side`. Its frame shows only when
    /// the window has room above the modal for it (`show-frame`): the app
    /// leaves `card.frame` empty when there's none.
    pub card: Card,
    /// The tab the person chose, `None` before; the tab to open on, as a
    /// card's mutual line asks, is the app's to put here as it opens.
    pub chosen: Option<Page>,
    /// Which tabs exist, as Discord's client decides: neither mutual one on
    /// one's own or on Matrix, and a bot's only has servers.
    pub board_tab: bool,
    pub wishlist_tab: bool,
    pub friends_tab: bool,
    pub servers_tab: bool,
    /// The mutual tabs' names: "Mutual Friends" till Discord has said, then
    /// "No Mutual Friends" or "3 Mutual Friends".
    pub friends_label: String,
    pub servers_label: String,
    /// Discord has said who's mutual, so an empty list means none.
    pub mutuals_known: bool,
    pub friends: Vec<Friend>,
    pub servers: Vec<Server>,
    /// Discord's answer to what they did lately came, or none is waited for.
    pub recent_known: bool,
    pub recent: Vec<Recent>,
    pub widgets: Vec<Widget>,
    /// Why the board shows nothing, or that it's private.
    pub board_note: String,
    pub wishlist: Vec<Wish>,
    pub wishlist_note: String,
}

impl FullProfile {
    /// The first tab: the Board when there is one, else Activity.
    pub fn first(&self) -> Page {
        if self.board_tab {
            Page::Board
        } else {
            Page::Activity
        }
    }

    /// Whether `page` has its tab.
    pub fn has(&self, page: Page) -> bool {
        match page {
            Page::Activity => true,
            Page::Friends => self.friends_tab,
            Page::Servers => self.servers_tab,
            Page::Board => self.board_tab,
            Page::Wishlist => self.wishlist_tab,
        }
    }

    /// The tab shown: the one chosen while it exists, else the first.
    pub fn page(&self) -> Page {
        self.chosen
            .filter(|page| self.has(*page))
            .unwrap_or(self.first())
    }

    /// Choose a tab: a tab that isn't there is the first, for good.
    pub fn choose(&mut self, page: Page) {
        self.chosen = Some(if self.has(page) { page } else { self.first() });
    }

    /// The tabs in Discord's order, each with whether it shows; the mutual
    /// ones are named by `friends_label` and `servers_label`.
    pub fn tabs(&self) -> [(Page, bool); 5] {
        [
            (Page::Board, self.board_tab),
            (Page::Activity, true),
            (Page::Wishlist, self.wishlist_tab),
            (Page::Friends, self.friends_tab),
            (Page::Servers, self.servers_tab),
        ]
    }

    /// The shown tab has nothing to list: rows the shape of what's coming
    /// while Discord hasn't said, else its words in its middle.
    pub fn blank(&self) -> bool {
        match self.page() {
            Page::Activity => self.card.activities.is_empty() && self.recent.is_empty(),
            Page::Friends => self.friends.is_empty(),
            Page::Servers => self.servers.is_empty(),
            Page::Board => !self.board_note.is_empty() || self.widgets.is_empty(),
            Page::Wishlist => self.wishlist.is_empty(),
        }
    }

    /// A blank Activity or mutual tab Discord hasn't answered for.
    pub fn loading(&self) -> bool {
        self.blank()
            && match self.page() {
                Page::Activity => !self.recent_known,
                Page::Friends | Page::Servers => !self.mutuals_known,
                Page::Board | Page::Wishlist => false,
            }
    }

    /// A blank tab that Discord has answered: it says so in its middle.
    pub fn empty(&self) -> bool {
        self.blank() && !self.loading()
    }

    /// Its pictures can be wanted: the handles the lists hold.
    pub fn pictures(&self) -> impl Iterator<Item = &image::Handle> {
        let servers = self.servers.iter().map(|server| &server.icon);
        let recent = self.recent.iter().map(|entry| &entry.picture);
        let games = self
            .widgets
            .iter()
            .flat_map(|widget| widget.games.iter().map(|game| &game.icon));
        let wishes = self.wishlist.iter().map(|wish| &wish.picture);
        servers
            .chain(recent)
            .chain(games)
            .chain(wishes)
            .filter_map(Expected::ready)
    }
}

/// The full profile: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant. What the card on its left asks is
/// `people::profile::Intent`, mapped as it is.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The close button.
    Close,
    /// A tab: the app asks Discord for it the first time ([`Page::asks`]).
    Choose(Page),
    /// A mutual server's card, beside its row.
    OpenServer(String, Rectangle),
    /// A mutual friend's full profile.
    OpenFriend(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_tabs() -> FullProfile {
        FullProfile {
            board_tab: true,
            wishlist_tab: true,
            friends_tab: true,
            servers_tab: true,
            ..FullProfile::default()
        }
    }

    #[test]
    fn the_first_tab_is_the_board_when_there_is_one() {
        assert_eq!(with_tabs().page(), Page::Board);
        assert_eq!(FullProfile::default().page(), Page::Activity);
    }

    #[test]
    fn a_tab_that_goes_falls_back_to_the_first() {
        let mut full = with_tabs();
        full.choose(Page::Servers);
        assert_eq!(full.page(), Page::Servers);
        full.servers_tab = false;
        assert_eq!(full.page(), Page::Board);
        full.choose(Page::Servers);
        assert_eq!(full.chosen, Some(Page::Board));
    }

    #[test]
    fn an_empty_tab_loads_until_discord_has_said() {
        let mut full = with_tabs();
        full.choose(Page::Friends);
        assert!(full.loading() && !full.empty());
        full.mutuals_known = true;
        assert!(full.empty() && !full.loading());
        full.friends.push(Friend::default());
        assert!(!full.blank());
    }
}
