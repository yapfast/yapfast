//! The server, tag and role cards: what they show and what they ask for.
//! The Slint side: `ServerCard` (`ui/people.slint:884`, a server a profile
//! lists as mutual, `ProfileServerRow` at `:355`), `TagCardView` (`:974`,
//! `TagCard` at `:937`, a server tag's server) and `RoleCard` (`:2327`, a
//! role mentioned in a message), filled by `tag_card` in `src/ui/profile.rs:1043`
//! and `show_role` (`src/ui/refresh.rs:2679`). Task P12
//! (docs/iced/tasks/people.md).
//!
//! The window holds one [`Cards`], and the shell draws it in two layers:
//! `Layer::Card` shows the server or the tag card (`cards::view`), and
//! `Layer::RoleCard` the role's (`cards::role`). The app fills the field
//! of the card that is open and clears it when its layer closes.

use iced::Color;

use crate::kind::{Avatar, Expected, Tag};
use crate::menu::Spot;

/// The card's width, as the window gives it (`width: 300px`); a role's is
/// narrower ([`ROLE_WIDTH`], `width: 260px`).
pub const WIDTH: f32 = 300.0;
pub const ROLE_WIDTH: f32 = 260.0;

/// What the cards show, one field for each. A field is `None` while its card
/// is closed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cards {
    pub server: Option<Server>,
    pub tag: Option<ServerTag>,
    pub role: Option<Role>,
}

/// A server a profile lists as mutual, as a card: what it is, and a way in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Server {
    pub id: String,
    pub name: String,
    /// The letter it shows without an icon.
    pub initial: String,
    /// Its icon, and its banner (`Expected::None` for a server without one).
    pub icon: Expected,
    pub banner: Expected,
    /// "12 members", when known, and "4 channels".
    pub members: String,
    pub channels: String,
    /// What they are called there: under "THEIR NICKNAME" when it isn't empty.
    pub nick: String,
}

/// A server tag's card: the server the tag is from, as far as Discord said.
/// What isn't known yet is empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerTag {
    /// The server's id, which Go to Server and the name open and Retry asks
    /// again about.
    pub server: String,
    pub name: String,
    /// The letter it shows without an icon.
    pub initial: String,
    pub icon: Expected,
    /// The tag's letters and its badge, drawn as the chip beside a name
    /// (`Tag::server` stays empty: the chip opens nothing here).
    pub tag: Tag,
    /// "1,200 Online" and "4,644 Members", when Discord counts them.
    pub online: String,
    pub members: String,
    /// "Est. Nov 2026".
    pub established: String,
    /// What the server says of itself, plain text.
    pub description: String,
    /// Why there's no more to show, when Discord didn't show the server.
    pub note: String,
    /// Discord hasn't answered yet.
    pub loading: bool,
    /// The server shows its profile only to its members.
    pub limited: bool,
    /// It failed to load, which a click on the note tries again.
    pub failed: bool,
    /// This account is in the server: its name and Go to Server open it.
    pub joined: bool,
}

/// A role mentioned in a message: who Yapfast knows has it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Role {
    pub name: String,
    /// Its colour, when it has one.
    pub color: Option<Color>,
    /// "Loading...", or that no one is known to have it, or empty.
    pub note: String,
    pub members: Vec<Member>,
    /// The tallest the card may be (`root.height - title - 16px`).
    pub limit: f32,
}

/// Someone known to have the role.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Member {
    pub id: String,
    pub name: String,
    pub avatar: Avatar,
    /// The name's colour, from a coloured role.
    pub color: Option<Color>,
}

/// What a click on a card asks for. Keep `#[non_exhaustive]`: the app
/// matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Go to Server, or a joined server's name: close the cards and the
    /// person's card, and go to this server.
    OpenServer(String),
    /// The failure's note: ask Discord about this server's tag again.
    Retry(String),
    /// Someone in a role card: their card opens beside the row.
    OpenMember(String, Spot),
}
