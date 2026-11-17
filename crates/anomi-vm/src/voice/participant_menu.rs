//! A participant's menu: what it shows and what it asks for. The Slint side
//! is `VolumeMenu` (`ui/voice.slint:1594`) and the window's `voice-menu`
//! popup (`ui/yapfast.slint:2686`); the rows under it are the window's
//! context menu (`vm.menu`), built by `src/ui/menus.rs` for a target named
//! `in-voice:<room>/<user>` (Profile, Mention, Message, Start call, notes,
//! the mute boxes, Invite, Move to, server mute, Disconnect), whose choices
//! send `menu::Intent::Choose`.
//!
//! This module is the card on top of those rows: the person's name and their
//! volumes. The app opens `Layer::ParticipantMenu` at the pointer and the
//! context menu `HEAD_OVERLAP` px above the card's bottom edge, so the two
//! read as one: a card [`ParticipantMenu::joined`] to rows has square
//! bottom corners.

/// The card's width unless the rows under it are wider: the context menu's
/// narrowest there (`narrowest: 260px`, `ui/yapfast.slint:2713`).
pub const WIDTH: f32 = 260.0;

/// The card's padding and the space between its lines (`padding: 12px;
/// spacing: 12px`).
pub const PADDING: f32 = 12.0;
pub const SPACING: f32 = 12.0;
/// The person's name line (Slint's 15 px line is 19.5, kept whole), and one
/// volume slider with its header (13 px x 1.3 + 6 + 18 = 40.9, kept whole).
pub const NAME_HEIGHT: f32 = 20.0;
pub const SLIDER_HEIGHT: f32 = 41.0;

/// How far the rows start above the card's bottom edge, so the two share
/// one line.
pub const HEAD_OVERLAP: f32 = 1.0;

/// Where a volume runs to, in percent.
pub const MAXIMUM: i32 = 200;

/// A participant's menu: what its screen draws. The window holds it as
/// `vm.voice.participant_menu`.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticipantMenu {
    /// Who across calls (a Discord or Matrix user id), which their volumes
    /// are remembered by; every [`Intent`] names them.
    pub person: String,
    pub name: String,
    /// They share their screen: a second slider sets their stream's volume.
    pub live: bool,
    /// Their voice's volume and their stream's, in percent.
    pub voice_volume: i32,
    pub stream_volume: i32,
    /// The menu's rows follow right under, as one card.
    pub joined: bool,
    /// The card's width: the rows' under it when they are wider than
    /// [`WIDTH`].
    pub width: f32,
}

impl Default for ParticipantMenu {
    fn default() -> Self {
        Self {
            person: String::new(),
            name: String::new(),
            live: false,
            voice_volume: 100,
            stream_volume: 100,
            joined: false,
            width: WIDTH,
        }
    }
}

impl ParticipantMenu {
    /// The card's height: the app places the rows under it with this.
    pub fn height(&self) -> f32 {
        let sliders = if self.live { 2.0 } else { 1.0 };
        2.0 * PADDING + NAME_HEIGHT + sliders * (SPACING + SLIDER_HEIGHT)
    }

    /// Where the rows start under a card whose top is at `top`.
    pub fn rows_top(&self, top: f32) -> f32 {
        if self.joined {
            top + self.height() - HEAD_OVERLAP
        } else {
            top
        }
    }
}

/// A participant's menu: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A slider moved (`volume-changed(person, stream, volume, last)`,
    /// `src/ui/callbacks.rs:2438`): `last` is the end of a drag or a key,
    /// which keeps the value; every step is heard at once, only the last is
    /// remembered.
    Volume {
        person: String,
        /// Their stream's volume, not their voice's.
        stream: bool,
        volume: i32,
        last: bool,
    },
}
