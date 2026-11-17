//! A server's channel list: each category's heading above its channels, the
//! server's pinned channels first under a heading of their own, threads
//! hanging under their channel, and who is in each voice channel under it.
//! The Slint side: `RoomRow` and `RoomList` (`ui/rail.slint:38`, `:1150`, the
//! channel rows), `ChannelRow`, `CategoryHeading`, `BrowseLink`, `DropLine`,
//! `PersonTouch` and `VoiceCardView` (`:397`, `:840`, `:1138`, `:787`, `:799`,
//! `:741`); the channel half of `show_sidebar`, `show_moving_person` and
//! `show_arranging` (`src/ui/refresh.rs:1059`, `:1575`, `:1605`); folding,
//! arranging and moving people (`src/ui/callbacks.rs:2927`-`:3027`).
//!
//! A row is a channel as Slint's `RoomRow` is: it may start a category,
//! whose heading shows above it, and a row that is only that heading
//! (`hidden`) stands for a folded category with nothing left showing, or one
//! with no channels. Everything is already decided: which rows show, their
//! icons, the call's length as text. Dragging asks the app, which says what
//! may drop where in [`Channels::arranging`] and [`Channels::moving`].

mod people;

pub use people::VoicePerson;

use crate::icon::Icon;
use crate::menu::Spot;

/// The channels of the server shown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Channels {
    /// The server's id, so another server's list starts at its top.
    pub server: String,
    pub rows: Vec<ChannelRow>,
    /// Browse Channels at the top of a Community server's list: `Some`
    /// while offered, `true` while its screen shows (`browse-offered`,
    /// `browsing`). Discord's look leaves it to the events row, so the app
    /// offers it only without that look.
    pub browse: Option<bool>,
    /// Said where the rows would be, as when none match the filter
    /// (`sidebar-empty`); empty while there are rows.
    pub empty: String,
    /// A channel or a category being dragged, and where it may drop. Boxed,
    /// as is `moving`, so a server's list stays near the DM list's size in
    /// `sidebar::Place`.
    pub arranging: Option<Box<Arranging>>,
    /// Someone being dragged from a voice channel onto another.
    pub moving: Option<Box<Moving>>,
}

/// A channel, or a category's heading above its channels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChannelRow {
    /// The core's room id. A row that is only its heading names the
    /// category's first channel, or the category, for the heading's menu.
    pub id: String,
    pub name: String,
    /// The category this row starts, shown above it; empty when it starts
    /// none. Over the pinned channels it's "PINNED CHANNELS".
    pub heading: String,
    pub kind: ChannelKind,
    /// Its icon as the list shows it: its kind's, marked private or
    /// age-restricted, the rules', or a lock for one this account may not
    /// read (`Icons::room`, `src/ui/refresh.rs:349`). `None` draws its
    /// kind's own.
    pub icon: Option<Icon>,
    pub selected: bool,
    pub unread: bool,
    /// Mentions waiting, as a badge; empty for none.
    pub badge: String,
    /// Its notifications are muted: drawn faint until it's chosen or
    /// pointed at. The app leaves it off for a voice channel, which Discord
    /// doesn't dim.
    pub muted: bool,
    /// Listed but not open to join, as a channel the account's roles keep
    /// it out of: drawn faint.
    pub locked: bool,
    /// One of the server's pinned channels, or its thread, at the top: its
    /// heading neither folds nor has a menu, and a line ends them.
    pub pinned: bool,
    /// Its heading's category is folded.
    pub collapsed: bool,
    /// The row is only its heading: a folded category with nothing left
    /// showing, or one without channels.
    pub hidden: bool,
    /// What its heading folds by: a Discord category's room id, which tells
    /// apart categories of the same name, or else the heading.
    pub category: String,
    /// A thread whose channel has another thread after it, so its line runs
    /// on (`thread-continues`).
    pub thread_continues: bool,
    /// A voice channel with a chat, which a button on the row opens.
    pub chat: bool,
    /// How long a busy voice channel's call has gone on, as Discord's client
    /// counts it from its first person in ("47 min"); empty for none. The
    /// app's clock writes it while the window is in front.
    pub lasted: String,
    /// Who is in a voice channel, listed under it. For the call this
    /// account is in (`in_call`), its participants.
    pub people: Vec<VoicePerson>,
    /// This account's call is in this channel: its people speak, and show
    /// whether they are connected yet.
    pub in_call: bool,
    /// The line summing up its people on its card, as "3 people · 1 live".
    pub summary: String,
    /// This account may make a channel in its heading's category: a "+"
    /// on the heading under the pointer.
    pub may_create: bool,
    /// Where its channel, and its heading's category, come among the rows
    /// that drag and take a drop, from 1; 0 for neither, as a thread, a
    /// pinned channel or a filtered list.
    pub place: u32,
    pub heading_place: u32,
    /// This account may drag them (Manage Channels where they are:
    /// `Application::draggable`).
    pub draggable: bool,
    pub heading_draggable: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChannelKind {
    #[default]
    Text,
    Voice,
    Forum,
    Thread,
}

impl ChannelRow {
    pub fn thread(&self) -> bool {
        self.kind == ChannelKind::Thread
    }

    pub fn voice(&self) -> bool {
        self.kind == ChannelKind::Voice
    }

    /// The icon drawn: the row's, or its kind's.
    pub fn glyph(&self) -> Icon {
        self.icon.unwrap_or(match self.kind {
            ChannelKind::Text => Icon::Hash,
            ChannelKind::Voice => Icon::Volume,
            ChannelKind::Forum => Icon::MessageSquareText,
            ChannelKind::Thread => Icon::Thread,
        })
    }

    /// Its heading is the pinned channels', which neither folds nor drags.
    pub fn pinned_heading(&self) -> bool {
        !self.heading.is_empty() && self.pinned
    }
}

/// A channel or a category being dragged (`room-arranging`,
/// `room-droppable`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arranging {
    /// Its place, from 1.
    pub place: u32,
    /// Whether it may drop on each place, from the first.
    pub droppable: Vec<bool>,
}

impl Arranging {
    /// It may drop on `place`: another place the app allows.
    pub fn drops_on(&self, place: u32) -> bool {
        place > 0
            && place != self.place
            && self
                .droppable
                .get(place as usize - 1)
                .copied()
                .unwrap_or(false)
    }
}

/// Someone dragged from a voice channel, which moves them as their menu's
/// Move to does (`person-moving`, `person-droppable`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Moving {
    /// The voice channels they may drop on, by room id: those Move to
    /// offers.
    pub droppable: Vec<String>,
    /// The one the pointer is over, outlined while they may drop there.
    pub over: Option<String>,
}

impl Moving {
    pub fn drops_on(&self, room: &str) -> bool {
        self.droppable.iter().any(|known| known == room)
    }
}

/// What the channel list asks for. Keep `#[non_exhaustive]`: the app matches
/// it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Open a channel, by room id (`select-room`).
    Open(String),
    /// A channel's menu, by room id, at a right click or under the row from
    /// the keyboard (`open-room-menu`).
    Menu(String, Spot),
    /// A voice channel's chat, without joining it (`open-chat`).
    OpenChat(String),
    /// Browse Channels (`open-browse`).
    Browse,
    /// Fold a category, or open it again, by the row's `category`
    /// (`toggle-category`).
    Fold(String),
    /// A category's menu, by its heading's row id (`open-category-menu`).
    CategoryMenu(String, Spot),
    /// The heading's "+": make a channel in its category, by the heading's
    /// row id (`create-channel`).
    Create(String),
    /// Someone in a voice channel was clicked: their profile, by user id,
    /// beside their row (`open-voice-profile`).
    Profile {
        channel: String,
        user: String,
        at: Spot,
    },
    /// Someone's menu in a voice channel, by user id (`open-voice-menu`).
    PersonMenu {
        channel: String,
        user: String,
        at: Spot,
    },
    /// Their LIVE tag: watch their stream, by their id in the call
    /// (`show-voice-person`).
    Watch { channel: String, person: String },
    /// Dragging a channel or a category.
    Arrange(Arrange),
    /// Dragging someone onto another voice channel.
    Move(Move),
}

/// Dragging a channel or a category, by places: what `start-arranging`,
/// `arrange` and `stop-arranging` said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrange {
    /// It was picked up: the app says where it may drop, or lets it go.
    Start(u32),
    /// The pointer is over this place, or none.
    Over(Option<u32>),
    /// It was dropped on a place it may drop on.
    Drop(u32),
    /// It went back, dropped nowhere or with Escape.
    Stop,
}

/// Dragging someone from under a voice channel: `start-moving`,
/// `move-person` and `stop-moving`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Move {
    /// They were picked up from a channel, by its room id and their user id.
    Start {
        channel: String,
        user: String,
    },
    /// The pointer is over this voice channel, by room id, or none.
    Over(Option<String>),
    /// They were dropped on a voice channel they may move to.
    Drop(String),
    Stop,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_without_an_icon_draws_its_kinds() {
        let voice = ChannelRow {
            kind: ChannelKind::Voice,
            ..ChannelRow::default()
        };
        assert_eq!(voice.glyph(), Icon::Volume);
        let locked = ChannelRow {
            icon: Some(Icon::Lock),
            ..voice
        };
        assert_eq!(locked.glyph(), Icon::Lock);
    }

    #[test]
    fn a_drag_drops_only_on_other_places_it_may() {
        let arranging = Arranging {
            place: 2,
            droppable: vec![true, true, false],
        };
        assert!(arranging.drops_on(1));
        assert!(!arranging.drops_on(2), "its own place");
        assert!(!arranging.drops_on(3));
        assert!(!arranging.drops_on(0) && !arranging.drops_on(9));
    }

    #[test]
    fn a_servers_list_stays_small_beside_the_dm_lists() {
        // Clippy's `large_enum_variant` (200 bytes between `sidebar::Place`'s
        // variants) holds, with the server's header beside the channels.
        let server =
            size_of::<Channels>() + size_of::<crate::sidebar::server_header::ServerHeader>();
        assert!(
            server < size_of::<crate::sidebar::dms::Dms>() + 200,
            "{server}"
        );
    }

    #[test]
    fn the_pinned_heading_is_the_pinned_rows() {
        let row = ChannelRow {
            heading: "PINNED CHANNELS".into(),
            pinned: true,
            ..ChannelRow::default()
        };
        assert!(row.pinned_heading());
        assert!(!ChannelRow::default().pinned_heading());
    }
}
