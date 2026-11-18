//! Types several areas share: a person's presence, an avatar, a count badge,
//! a server tag, a status, the voice spot someone is in; how a name is
//! dressed (`name`: its style and nameplate); what a profile carries of its
//! effect and frame (`profile`); a moving picture's newest frame (`live`).
//! Each is plain data a view-model holds and a widget of `anomi-widgets`
//! draws. Task F14 owns it; others ask for an addition in their review.

mod live;
mod name;
mod profile;

pub use live::{Live, on_read};
pub use name::{NameEffect, NameStyle, Nameplate};
pub use profile::{Effect, EffectLayer, FrameAnchor, FrameKind, FrameLayer};

use iced::Color;
use iced::widget::image;

/// A person's presence, as Discord shows it by their avatar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Presence {
    /// Unknown: no dot.
    #[default]
    None,
    Online,
    Mobile,
    Idle,
    DoNotDisturb,
    Offline,
}

/// A round picture of a person, a group or a server: its picture once it
/// arrived, and until then its initial on its tint.
#[derive(Clone, Debug, PartialEq)]
pub struct Avatar {
    pub initial: String,
    /// The theme's avatar colour when transparent (the default).
    pub tint: Color,
    /// The decoded picture. Its pixels are the core's, shared, not copied.
    pub picture: Option<image::Handle>,
}

impl Default for Avatar {
    fn default() -> Self {
        Self {
            initial: "?".into(),
            tint: Color::TRANSPARENT,
            picture: None,
        }
    }
}

/// A count as a badge shows it: nothing for none, and at most "99+".
pub fn count_badge(count: u64) -> String {
    match count {
        0 => String::new(),
        count if count > 99 => "99+".into(),
        count => count.to_string(),
    }
}

/// A picture that may be on its way: Discord's client keeps the room of a
/// badge or an emoji from the start, so that a row doesn't move when the
/// picture arrives.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Expected {
    /// There is none.
    #[default]
    None,
    /// There is one, not here yet: its room stays empty.
    Waiting,
    Ready(image::Handle),
}

impl Expected {
    /// The handle, once it is here.
    pub fn ready(&self) -> Option<&image::Handle> {
        match self {
            Self::Ready(handle) => Some(handle),
            _ => None,
        }
    }
}

/// A server tag someone wears by their name: up to four letters and, when
/// the server has one, its badge's picture. `server` is the id a click
/// opens the card of; empty leaves it a plain chip.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tag {
    pub text: String,
    pub badge: Expected,
    pub server: String,
}

/// What someone is doing, as a small icon before the words of their status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Doing {
    #[default]
    Nothing,
    Playing,
    Listening,
    Streaming,
    Watching,
    Competing,
}

/// A custom status or what someone is doing: its emoji's picture when it
/// has one (room kept while it is on its way), then its words.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub text: String,
    pub emoji: Expected,
    pub doing: Doing,
}

/// The voice channel someone is in, and its server.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spot {
    /// "channel \u{2014} server", as the spot and its tooltip write it.
    pub place: String,
}

impl Spot {
    pub fn new(channel: &str, server: &str) -> Self {
        Self {
            place: format!("{channel} \u{2014} {server}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::count_badge;

    #[test]
    fn a_badge_stops_at_99() {
        assert_eq!(count_badge(0), "");
        assert_eq!(count_badge(7), "7");
        assert_eq!(count_badge(100), "99+");
    }

    #[test]
    fn a_spot_names_channel_and_server() {
        let spot = super::Spot::new("Lounge", "Home");
        assert_eq!(spot.place, "Lounge \u{2014} Home");
    }
}
