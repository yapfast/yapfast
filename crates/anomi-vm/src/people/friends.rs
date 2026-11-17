//! The Friends screen: what it shows and what it asks for. Task P01
//! (docs/iced/tasks/people.md). The Slint side: `FriendsScreen` and
//! `FriendRow` (`ui/friends.slint:247`, `:147`) and the friend half of
//! `show_sidebar` (`src/ui/refresh.rs:1059`).
//!
//! The screen is handed the rows it shows: those around when the Online tab
//! is open, every friend on All, both already narrowed by the search field.
//! The tabs themselves are the conversation header's (`header::Screen`).

use crate::kind::{Avatar, Presence, Spot, Status};
use crate::menu;

/// The Friends screen: what its screen draws. The window holds it as
/// `vm.people.friends`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Friends {
    /// The friends shown: those around, or all of them, narrowed by
    /// [`filter`](Self::filter).
    pub rows: Vec<FriendRow>,
    /// The All tab is open (`all`), not Online.
    pub all: bool,
    /// Friends in all, before the search (`total`): none shows the welcome.
    pub total: usize,
    /// The search field's text (`filter`).
    pub filter: String,
}

/// A friend in the list; clicking it opens the DM with them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FriendRow {
    /// Their user id (`id`).
    pub id: String,
    pub name: String,
    pub avatar: Avatar,
    /// `Offline` when Discord says nothing.
    pub presence: Presence,
    /// Their custom status or what they are doing (`activity`); empty when
    /// neither.
    pub status: Status,
    /// The server voice channel they are in (`voice-room`, `voice-channel`,
    /// `voice-server`).
    pub voice: Option<Voice>,
}

/// A friend's voice channel: its room, to go to it, and how it reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Voice {
    pub room: String,
    pub spot: Spot,
}

impl FriendRow {
    /// They are saying something about themselves: a status or an activity.
    pub fn has_status(&self) -> bool {
        self.status != Status::default()
    }
}

/// The Friends screen: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Open the DM with this friend (their user id): a click on the row or
    /// its Message button.
    Message(String),
    /// Go to the voice channel (its room id) a friend is in.
    OpenVoice(String),
    /// A right click on a friend: their menu (their user id) at the pointer.
    Menu(String, menu::Spot),
    /// The search field's text.
    Filter(String),
    /// "Add a friend" of the welcome.
    AddFriend,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_friend_with_nothing_said_has_no_status() {
        let mut row = FriendRow::default();
        assert!(!row.has_status());
        row.status.text = "Factorio".into();
        assert!(row.has_status());
    }
}
