//! The Active Now column: friends together in a server's voice channel, or
//! at the same game, a card each, beside the friends list. The Slint side is
//! `ActiveNowColumn` (`ui/active_now.slint:186`); the cards are made by
//! `src/ui/active_now.rs`. Task P04.

use crate::kind::{Avatar, Expected};
use crate::menu::Spot;

/// The Active Now column: what its screen draws. The window holds it as
/// `vm.people.active_now`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActiveNow {
    /// A card per party or person, in Discord's order. None draws the quiet
    /// message.
    pub cards: Vec<Card>,
    /// The time now, in seconds, the clock the cards count from: the app's
    /// clock writes it while the window is in front, so a card's times
    /// advance only then (`ActivityClock.now`, `ui/controls.slint:268`).
    pub now: i64,
}

/// A card: friends doing one thing, or together in a voice channel
/// (`ActiveNowCard`, `ui/active_now.slint:48`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub id: String,
    /// "Ann and Bob", "Ann, Bob, and 1 other".
    pub title: String,
    /// What they are doing, as "Dota 2" or "In a Voice Channel"; the time
    /// since `start` follows it.
    pub doing: String,
    /// When it started, in seconds as `ActiveNow::now`'s; 0 when Discord
    /// didn't say.
    pub start: i64,
    /// The first of them, for the card's avatar.
    pub face: Avatar,
    /// The game's or app's picture, at the card's right.
    pub icon: Expected,
    pub rows: Vec<Row>,
    /// One of them, whose profile a click on the card's head opens.
    pub person: String,
}

/// What a row shows (`ActiveRowKind`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// A server's voice channel: the server, the channel, who is in it.
    #[default]
    Voice,
    /// A game several friends play: the game, how many.
    Game,
    /// What one of them does in full: Rich Presence's details and state, and
    /// how long since it started.
    Detail,
}

/// One row of a card (`ActiveNowRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub kind: Kind,
    pub title: String,
    pub detail: String,
    /// Its picture, a server's icon or a game's, else its initial on its
    /// tint; a voice row's is round, the others' a rounded square.
    pub icon: Avatar,
    /// When it started, as `ActiveNow::now`'s seconds; 0 for none.
    pub start: i64,
    /// The stack of faces at its end, at most three.
    pub faces: Vec<Avatar>,
    /// Those beyond the faces shown, as "+4".
    pub more: u32,
    /// A voice channel's room, which a click on the row opens; empty for none.
    pub room: String,
}

/// The Active Now column: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A voice row was clicked: open that room.
    OpenRoom(String),
    /// A card's head was clicked: open this person's menu at the head's bottom left.
    OpenPerson(String, Spot),
}
