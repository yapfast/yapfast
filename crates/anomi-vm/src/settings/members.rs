//! A server's Members settings: what it shows and what it asks for. Task X03
//! (docs/iced/tasks/settings.md). The Slint side: `MembersSection` and
//! `RoleChip` (`ui/server_people.slint:117`, `:98`) with the glue of
//! `src/ui/server_settings.rs:506` (`members_prompt`).
//!
//! The server's members, newest first or in an order chosen, a page at a
//! time, found by name or id. The app formats every time and role chip; the
//! page only draws.

use iced::Color;

use crate::kind::Avatar;
use crate::menu::Spot;

/// How many members a page may hold, in the order the footer's menu lists them.
pub const PAGE_SIZES: [usize; 4] = [12, 25, 50, 100];

/// Under this many members the footer has no "Showing" menu: they fit a page.
pub const ONE_PAGE: usize = 12;

/// A member's row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Member {
    /// The user's id: what a row, its menu and its profile name.
    pub id: String,
    /// The name Discord shows in the server, and the username under it
    /// (empty when it is the same).
    pub name: String,
    pub username: String,
    pub avatar: Avatar,
    /// The colour of their highest coloured role; the name takes it.
    pub name_color: Option<Color>,
    /// "1 month ago": since they joined the server, and since they made
    /// their Discord account.
    pub since: String,
    pub age: String,
    /// How they joined, and that it isn't known (drawn muted).
    pub join: String,
    pub join_unknown: bool,
    /// Their first role's chip: its name (empty without roles), its colour
    /// and "+2" for more.
    pub role: String,
    pub role_color: Color,
    pub more_roles: String,
    /// "Timed out for 3 hours", or empty.
    pub timed_out: String,
    /// The signals: excessive DMs, suspected spam, a name to change.
    pub unusual_dms: bool,
    pub spammer: bool,
    pub quarantined: bool,
}

/// A server's Members settings: what its screen draws. The window holds it as
/// `vm.settings.members`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Members {
    /// The page's rows.
    pub rows: Vec<Member>,
    /// What the search field holds. The app sends the search to Discord once
    /// the typing stops for 300 ms, as Discord's client does.
    pub search: String,
    /// The rows are search results.
    pub searched: bool,
    /// The order chosen: the place in "Member Since (Newest first)", "Member
    /// Since (Oldest first)", "Joined Discord (Newest first)", "Joined Discord
    /// (Oldest first)".
    pub sort: usize,
    /// The page showing and how many there are; the page size's place in
    /// [`PAGE_SIZES`].
    pub page: usize,
    pub pages: usize,
    pub page_size: usize,
    /// How many members there are in all, and as the footer says it ("..."
    /// before the first page is in).
    pub total: usize,
    pub count: String,
    /// A page is on its way; the server is being indexed for a search.
    pub fetching: bool,
    pub indexing: bool,
    /// The column of how they joined shows (the account may see it).
    pub join_method: bool,
    /// The page is narrow: Joined Discord goes, as the least needed.
    pub narrow: bool,
}

impl Members {
    /// How many rows a page holds.
    pub fn page_length(&self) -> usize {
        PAGE_SIZES
            .get(self.page_size)
            .copied()
            .unwrap_or(PAGE_SIZES[0])
    }
}

/// A server's Members settings: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The search field changed (`act(0, text)`).
    Search(String),
    /// The search was sent with Enter: no waiting.
    Find(String),
    /// An order was chosen, by its place in `sort` (`act(1)`).
    Sort(usize),
    /// A page was asked for (`act(2)`).
    Page(usize),
    /// A page size was chosen, by its place in [`PAGE_SIZES`] (`act(3)`).
    PageSize(usize),
    /// A row was pressed: open the member's profile by id (`act(4)`).
    Open(String),
    /// A member's menu, by id: More Options, or a right click.
    Menu(String, Spot),
}
