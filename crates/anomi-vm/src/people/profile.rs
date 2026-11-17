//! The profile card: what it shows and what it asks for. Task P07
//! (docs/iced/tasks/people.md).
//!
//! A [`Card`] is one person's profile as `ProfileData` holds it
//! (`ui/people.slint:439`) for `ProfileCard` (`:1326`), filled by
//! `profile_data` (`src/ui/profile.rs:588`). The window holds two
//! (`show_profile`, `src/ui/refresh.rs:5129`): the popout over the window
//! ([`Profile::card`], `Layer::Profile`), opened from a name, a member row or
//! one's own account, and the card docked beside a one-to-one DM
//! ([`Profile::panel`]). The full profile's side (P09) and the profile
//! editor's preview (P15) hold a [`Card`] of their own and draw it through
//! the card's view in their [`Place`]; every intent names its place, so the
//! app knows whose card it was.
//!
//! What the person does on a card that only the card shows (the pointer on
//! it, the status bubble opened, a note or a message being typed) is its
//! [`Touch`]: the app keeps it across refreshes of the same person and
//! clears it for another, as Slint's `changed person` handlers did.

mod connection;
mod theme;

use iced::widget::image;
use iced::{Color, Rectangle};

use crate::kind::{Avatar, Effect, Expected, FrameLayer, NameStyle, Presence, Status, Tag};
use crate::menu::Spot;
use crate::people::activity::card::{self as activity, Card as Activity};
use crate::people::badges::Badges;
use crate::text::{Link, Text};

pub use connection::{Connection, connection_icon};
pub use theme::Theme;

/// Where a card shows, which sets its size and what it offers
/// (`large`, `side`, `docked`, `preview`, `ui/people.slint:1328`-`:1335`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Place {
    /// Over the window, 300 px wide: someone's card from a name or a member
    /// row, or one's own from the account.
    #[default]
    Popout,
    /// Docked beside a one-to-one DM (`docked`): everything shown, no
    /// frame, no message box, "View Full Profile" at its foot.
    Panel,
    /// The full profile's left column (`large` and `side`): larger, with
    /// Discord's actions under the name, its connections and the note.
    Side,
    /// The profile editor's preview of one's own (`preview`): it opens
    /// nothing.
    Preview,
}

impl Place {
    /// The full profile's, larger in everything (`large`).
    pub fn large(self) -> bool {
        self == Self::Side
    }

    /// It shows everything: the bio whole and the dates (`full`).
    pub fn full(self) -> bool {
        matches!(self, Self::Side | Self::Panel)
    }
}

/// What the card offers for a Discord friendship (`ProfileData.friend`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Friendship {
    /// Nothing: one's own, a Matrix person, someone blocked.
    #[default]
    None,
    /// Send them a request.
    Offer,
    /// One was sent: shown, but it does nothing.
    Sent,
    /// Accept theirs.
    Theirs,
    /// They are a friend: the button opens their menu.
    Friends,
}

/// One of their roles in the server the card was opened in, as a chip with
/// its colour's dot; `None` draws the faint dot (`ProfileRole`, `:351`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Role {
    pub name: String,
    pub color: Option<Color>,
}

/// The server the card was opened in, and when they joined it, beside the
/// account's date under "Member Since" (`MemberSince`, `:613`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Joined {
    pub date: String,
    pub server: String,
    /// Its icon, or its initial without one.
    pub initial: String,
    pub icon: Expected,
}

/// What they share with this account, as the card's line says it: the
/// first three mutual friends' faces, and how many friends and servers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mutual {
    pub faces: Vec<Avatar>,
    pub friends: usize,
    pub servers: usize,
}

/// The full profile's tab a part of the mutual line opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Friends,
    Servers,
}

/// What the person is doing on a card, which only the card shows; see the
/// module's doc.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Touch {
    /// The pointer is on the card: an animated avatar plays.
    pub pointed: bool,
    /// The pointer is on the status bubble, which shows all its lines.
    pub bubble: bool,
    /// "+n" on the full profile's badge line was clicked: all of them show
    /// under the name.
    pub all_badges: bool,
    /// The note as typed, until it is saved; `None` shows the saved one.
    pub note: Option<String>,
    /// The note's box has the keys: it is filled.
    pub note_focused: bool,
    /// What the popout's message box holds.
    pub message: String,
}

/// One person's card. Pictures are the app's handles; empty strings and
/// `None` show nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    /// Their id; empty for a card with no person behind it (a preview),
    /// which offers no menu.
    pub id: String,
    pub name: String,
    pub avatar: Avatar,
    pub decoration: Option<image::Handle>,
    pub name_style: Option<NameStyle>,
    pub presence: Presence,
    /// Their Discord username or Matrix ID, and the pronouns they give.
    pub handle: String,
    pub pronouns: String,
    pub tag: Option<Tag>,
    /// The banner, else its colour, else the theme's first, else raised.
    pub banner: Expected,
    pub accent: Option<Color>,
    /// The profile's theme, a Nitro setting, which tints the card's
    /// gradient and border: [`Theme::new`] of its two colours.
    pub theme: Option<Theme>,
    pub effect: Option<Effect>,
    /// The frame's layers, the back ones first; none without one, or with
    /// frames off.
    pub frame: Vec<FrameLayer>,
    /// Their custom status, in a thought bubble beside the picture.
    pub thought: Option<Status>,
    /// What they're doing in full, on cards; the clock they count from is
    /// `now` (seconds as `activity::card::Card::start` counts them).
    pub activities: Vec<Activity>,
    pub now: i64,
    pub badges: Badges,
    pub bio: Option<Text>,
    pub roles: Vec<Role>,
    /// On Discord since, as the day is written; and in this server since.
    pub created: String,
    pub joined: Option<Joined>,
    /// When the friendship began, for a friend.
    pub friends_since: String,
    pub mutual: Mutual,
    pub connections: Vec<Connection>,
    /// The note this account keeps on them; `None` where none can be
    /// written (one's own, a Matrix person).
    pub note: Option<String>,
    /// "Loading the profile...", or why there's no more.
    pub remark: String,
    pub friend: Friendship,
    /// The full profile offers Message.
    pub can_message: bool,
    /// The placeholder of the popout's box that messages them in their DM,
    /// "Message @name" as the app translates it; empty for none.
    pub message_hint: String,
    pub own: bool,
    pub bot: bool,
    pub touch: Touch,
}

impl Card {
    /// The status the bubble shows, when there are words or an emoji.
    pub fn thought(&self) -> Option<&Status> {
        self.thought
            .as_ref()
            .filter(|status| !status.text.is_empty() || !matches!(status.emoji, Expected::None))
    }

    /// The note as the box shows it: as typed, else as saved.
    pub fn note_shown(&self) -> &str {
        self.touch
            .note
            .as_deref()
            .or(self.note.as_deref())
            .unwrap_or_default()
    }

    /// An activity's clock must tick: a card counts its time.
    pub fn wants_clock(&self, place: Place) -> bool {
        !place.large() && activity::wants_clock(&self.activities, self.now)
    }
}

/// The profile cards the window shows. The window holds it as
/// `vm.people.profile`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    /// The popout, drawn while `Layer::Profile` is open.
    pub card: Card,
    /// The card beside a one-to-one DM, while the DM has one.
    pub panel: Option<Card>,
}

impl Profile {
    /// The card of `place`: the panel's, else the popout's.
    pub fn at(&self, place: Place) -> Option<&Card> {
        match place {
            Place::Panel => self.panel.as_ref(),
            _ => Some(&self.card),
        }
    }

    /// A card's activity counts its time: the app ticks `now` once a second
    /// while it does, and only while the window is in front.
    pub fn wants_clock(&self, popout_open: bool) -> bool {
        (popout_open && self.card.wants_clock(Place::Popout))
            || self
                .panel
                .as_ref()
                .is_some_and(|panel| panel.wants_clock(Place::Panel))
    }
}

/// The profile card: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Their full profile: the picture, the styled name, "View full bio",
    /// or the panel's "View Full Profile".
    Enlarge(Place),
    /// Their full profile on a mutual tab, from the mutual line.
    EnlargeOn(Place, Tab),
    /// Send them a friend request, or accept theirs.
    Befriend(String),
    /// Their menu, under the "..." or the Friends button.
    More(String, Spot),
    /// Open their DM: the full profile's Message.
    Message(String),
    /// What the popout's box holds, as typed.
    Draft(Place, String),
    /// Enter in the popout's box: send what it holds to their DM, and
    /// empty it.
    Send(Place),
    /// The note as typed.
    Note(Place, String),
    /// Enter in the note: save it when it changed.
    SaveNote(Place),
    /// The note's box got the keys, or lost them: then save it when it
    /// changed.
    NoteFocus(Place, bool),
    /// Edit Profile, on one's own.
    EditProfile,
    /// A web address: a connection's, a bio's, an activity's.
    OpenLink(String),
    /// Something else the bio links: a mention, a channel.
    Link(Link),
    /// The server tag's card, beside where the chip sits.
    Tag(String, Rectangle),
    /// An activity card's link or more menu; `privacy` when it is one's own
    /// game, whose menu offers Manage Privacy.
    Activity {
        intent: activity::Intent,
        privacy: bool,
    },
    /// The pointer came onto the card, or left it.
    Pointed(Place, bool),
    /// The pointer came onto the status bubble, or left it.
    Bubble(Place, bool),
    /// "+n": all the full profile's badges, under the name.
    AllBadges(Place),
    /// The profile effect is about to show: its pictures are wanted.
    EffectWanted(Place),
}

#[cfg(test)]
mod tests;
