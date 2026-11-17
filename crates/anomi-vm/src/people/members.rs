//! The member list: what it shows and what it asks for. The Slint side:
//! `MemberPanel` (the list half) and `MemberRow` in `ui/people.slint:11` and
//! `:97`; `show_members` and `show_roster` in `src/ui/refresh.rs:2429`,
//! `:2507` fill it, `src/ui/callbacks.rs:1964`, `:2005` act on it.
//!
//! The rows are one flat list of group headings and people, in Discord's
//! order, so a server of thousands is drawn virtually and a heading is a row
//! of its own, as `show_roster` makes it. A row keeps what the screen draws,
//! formatted: counts in the heading's words, the activity as a [`Status`],
//! the pictures as handles the app made once. The activity cards above the
//! list are `people::activity`'s.

use iced::Color;
use iced::widget::image;

use crate::kind::{Avatar, Expected, NameStyle, Nameplate, Presence, Status, Tag};
use crate::menu::Spot;

/// The member list: what its screen draws. The window holds it as
/// `vm.people.members`.
#[derive(Clone, Debug, PartialEq)]
pub struct Members {
    /// Group headings and people, in order.
    pub rows: Vec<Row>,
    /// It lists a group DM, whose crown marks who made it.
    pub group: bool,
    /// Shown above the list while it loads or when it can't.
    pub note: String,
    /// The id of the member whose profile card is open.
    pub selected: String,
    /// A gem by each server booster's name; on unless turned off.
    pub gems: bool,
}

impl Default for Members {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            group: false,
            note: String::new(),
            selected: String::new(),
            gems: true,
        }
    }
}

/// A row of the list.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// The heading that starts a group: "Mods \u{2014} 5".
    Heading(Heading),
    Member(Box<Member>),
}

/// A group's heading, with the count in its words.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Heading {
    pub text: String,
    /// The picture of the role the group is, which Discord draws by its
    /// heading; its room is kept while it is on its way.
    pub icon: Expected,
}

/// A person in the list (`MemberRow` with an id).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Member {
    /// The person's id, which a click and a right click name.
    pub id: String,
    pub name: String,
    pub avatar: Avatar,
    /// The decoration around their avatar.
    pub decoration: Option<image::Handle>,
    /// Discord's presence; none for Matrix. An offline member is faded and
    /// has no dot.
    pub presence: Presence,
    /// The name's colour, from a coloured Discord role.
    pub color: Option<Color>,
    /// How their name is drawn; none for the usual.
    pub name_style: Option<NameStyle>,
    /// Their nameplate, behind the row.
    pub plate: Nameplate,
    /// A custom status or what they are doing, under the name; without
    /// words or emoji the line is not drawn.
    pub status: Status,
    /// A bullet follows the status's icon, before a custom status.
    pub bullet: bool,
    /// The server tag they wear by their name. It opens no card here, as in
    /// Discord's client.
    pub tag: Option<Tag>,
    /// The picture of their highest role that has one.
    pub role_icon: Expected,
    /// An application's account, marked APP.
    pub bot: bool,
    /// The server's owner, or a group's creator, marked with a crown.
    pub owner: bool,
    /// "Server boosting since 3 March 2024" for someone who boosts their
    /// Discord server, which a gem by the name says on hover; empty for
    /// anyone else.
    pub boosting: String,
    /// Timed out in their Discord server, marked with a clock.
    pub timed_out: bool,
}

impl Member {
    /// Faded, as Discord fades its offline members.
    pub fn offline(&self) -> bool {
        self.presence == Presence::Offline
    }
}

/// What the member list asks for. Keep `#[non_exhaustive]`: the app matches
/// it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A click on a member, by id: their profile card, beside the row
    /// (`open-profile`).
    Open(String, Spot),
    /// A right click on a member, by id: their menu, at the pointer
    /// (`open-member-menu`).
    Menu(String, Spot),
    /// The rows in view, first and last by index in [`Members::rows`], as
    /// they change: for the app to load the pictures they hold.
    Shown(usize, usize),
}
