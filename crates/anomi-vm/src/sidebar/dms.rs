//! The DM home's list: Friends and Requests, then the DMs. The Slint side:
//! `RoomRow` in `ui/rail.slint` (the DM rows), `DmHeading` and the DM half of
//! `show_sidebar` in `src/ui/refresh.rs`, `dm_detail` in `src/ui.rs`.
//!
//! A row carries what the Slint `RoomRow` carried for a DM: the person's
//! avatar with its presence and decoration, their name in its style, their
//! nameplate and server tag, the line under the name (their status, a
//! group's size, or the voice channel they are in), and the marks at its
//! end (a call going on, pinned, mentions, a Wave button). The rows come
//! already filtered by the sidebar's search field and ordered, pinned DMs
//! first; a long list is drawn virtually by the screen.

use iced::widget::image;

use crate::kind::{Avatar, NameStyle, Nameplate, Presence, Status, Tag};
use crate::menu;

/// The DM home: Friends and Requests, then the DMs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dms {
    pub friends_selected: bool,
    pub requests_selected: bool,
    /// Requests waiting, as a badge; empty for none.
    pub requests_badge: String,
    pub rows: Vec<DmRow>,
    /// No Discord account is linked: Friends and Requests, which are
    /// Discord's, and the add-a-friend button are not offered.
    pub no_discord: bool,
    /// The app's preview, with sample rooms and no account (`AppState.preview`), which has neither the links nor the heading.
    pub preview: bool,
    /// What the list says when it has no rows.
    pub empty: Empty,
}

/// Why a list with no rows is empty.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Empty {
    /// It isn't, or says nothing.
    #[default]
    None,
    /// The search field's text matches none of the DMs.
    NoMatch,
    /// There are no DMs yet, such as before the first sync.
    NoDms,
}

/// A DM in the list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DmRow {
    /// The core's room id.
    pub id: String,
    pub name: String,
    pub avatar: Avatar,
    /// A one-to-one DM's person's presence; none for a group.
    pub presence: Presence,
    /// The line under the name when there is no [`status`](Self::status):
    /// a group's size, or a status's words.
    pub detail: String,
    pub selected: bool,
    pub unread: bool,
    /// Mentions waiting; empty for none.
    pub badge: String,
    /// Notifications are muted: the row is drawn faint until it is chosen or
    /// pointed at.
    pub muted: bool,
    /// The decoration around the person's avatar.
    pub decoration: Option<image::Handle>,
    /// How their name is drawn; none for the usual.
    pub name_style: Option<NameStyle>,
    /// Their nameplate, shown behind the row while it's chosen or pointed at.
    pub plate: Nameplate,
    /// The server tag they wear.
    pub tag: Option<Tag>,
    /// Their custom status, words and emoji, with `doing` left empty (a DM
    /// row draws no activity icon); without them the row says its `detail`.
    pub status: Status,
    /// The voice channel they are in, which a click goes to.
    pub voice: Option<Voice>,
    /// Discord's own account: marked OFFICIAL.
    pub official: bool,
    /// The person's id when nobody has written in this DM yet, so that a
    /// Wave button is offered; empty otherwise.
    pub wave: String,
    /// A call is going on in it.
    pub call: bool,
    /// Pinned to the top of the list.
    pub pinned: bool,
    /// A Discord DM, marked by its logo at the avatar's corner; the app
    /// leaves it off where marks are turned off.
    pub discord: bool,
}

/// The voice channel a DM's person is in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Voice {
    /// The channel's room id.
    pub room: String,
    /// Its name and its server's.
    pub spot: crate::kind::Spot,
}

/// What the DM list asks for. Keep `#[non_exhaustive]`: the app matches it
/// with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Open a DM, by room id.
    Open(String),
    Friends,
    Requests,
    /// The heading's add-a-friend button.
    AddFriend,
    /// The heading's new-message button, which starts a DM.
    NewMessage,
    /// Wave at a person, by their id, from an empty DM's button.
    Wave(String),
    /// Go to the voice channel a DM's person is in, by its room id.
    OpenVoice(String),
    /// A right click on a DM, by room id: its menu, at the pointer.
    Menu(String, menu::Spot),
    /// The DMs in view and a few beyond, as the list reports them, by room
    /// id: the app asks for their pictures.
    Seen(Vec<String>),
    /// The pointer came onto a DM, by room id, or left it: its nameplate and
    /// decoration move under it, as the member list's do.
    Pointed(Option<String>),
}
