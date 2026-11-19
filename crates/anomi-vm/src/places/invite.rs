//! A server's or channel's invite dialog: what it shows and what it asks
//! for. Discord's two pages in one card: whom to send the invite to, with
//! the link at its foot, and the link's settings. The Slint side:
//! `InvitePrompt` and `InviteDialog` (`ui/server.slint:12`, `:27`), filled
//! by `show_invite` (`src/ui/refresh.rs:2717`) and answered by
//! `on_make_invite` (`src/ui/callbacks.rs:1362`). The window holds it as
//! `vm.places.invite`.

use crate::icon::Icon;
use crate::kind::Avatar;

/// How long a link may last, in the order the dialog offers them
/// (`INVITE_AGES`): the choice is an index into these.
pub const AGES: usize = 7;
/// How often a link may be used (`INVITE_USES`).
pub const USES: usize = 7;

/// Which of its pages the dialog shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    /// Whom to invite, and the link to copy.
    #[default]
    People,
    /// How long the next link lasts and how often it works.
    Link,
}

/// How an invite to one person stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Sent {
    #[default]
    Not,
    /// On its way: the button waits.
    Sending,
    /// Discord took it: the button says so and takes no press.
    Sent,
    /// Discord refused it, for this reason, said by the row.
    Failed(String),
}

impl Sent {
    /// Its button may still send it.
    pub fn may_send(&self) -> bool {
        matches!(self, Self::Not)
    }
}

/// Someone the dialog offers to invite.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Invitee {
    /// Their Discord user id, which [`Intent::Send`] carries.
    pub id: String,
    /// Their display name, and their username under it, as `ann`.
    pub name: String,
    pub username: String,
    pub avatar: Avatar,
    pub sent: Sent,
}

/// The invite dialog: whom it invites where, the link once Discord made it,
/// and the choices for the next one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Invite {
    /// "Invite friends to {server}".
    pub title: String,
    /// The channel the invite lands people in, and its icon.
    pub channel: String,
    pub icon: Option<Icon>,
    /// A private channel: a notice says only some may see it.
    pub private: bool,
    pub page: Page,
    /// What the search field holds; the rows match it.
    pub query: String,
    /// The server's members who could join the voice channel invited to,
    /// under their foldable heading: none for a text channel.
    pub members: Vec<Invitee>,
    /// That heading is folded: its rows hide.
    pub folded: bool,
    /// Friends and DMs not in the server yet.
    pub friends: Vec<Invitee>,
    /// The link; empty while Discord makes it.
    pub link: String,
    /// How long the link lasts and how often it works, as said under it.
    pub note: String,
    /// Discord is making a link.
    pub making: bool,
    /// Why making one failed; empty for none.
    pub error: String,
    /// The choice for the next link's life: an index below [`AGES`]
    /// (30 min, 1 hr, 6 hr, 12 hr, 1 day, 7 days, Never).
    pub age: usize,
    /// The choice for its uses: an index below [`USES`] (No limit, 1, 5,
    /// 10, 25, 50, 100).
    pub uses: usize,
    /// The link was copied: Copy reads "Copied" until a new link comes. The
    /// app sets it on [`Intent::Copy`] and clears it with a new link.
    pub copied: bool,
}

impl Invite {
    /// What Generate a new link asks for, from the choices as they stand.
    pub fn remake(&self) -> Intent {
        Intent::Remake {
            age: self.age,
            uses: self.uses,
        }
    }

    /// Someone is listed to invite.
    pub fn listed(&self) -> bool {
        !self.members.is_empty() || !self.friends.is_empty()
    }
}

/// The invite dialog: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The X: close the dialog (Escape and a press outside are the
    /// shell's, which go back from the link's settings first).
    Close,
    /// Put the link on the clipboard.
    Copy(String),
    /// Choose how long the next link lasts.
    Age(usize),
    /// Choose how often the next link may be used.
    Uses(usize),
    /// Make a new link with these choices, and show it on the first page.
    Remake { age: usize, uses: usize },
    /// The search field changed.
    Search(String),
    /// Send this person, by user id, the invite.
    Send(String),
    /// Fold or unfold the server's members.
    Fold,
    /// The rows in view, by user id: the app asks for their pictures.
    Seen(Vec<String>),
    /// Show this page: the link's settings, or back to whom to invite.
    Show(Page),
}
