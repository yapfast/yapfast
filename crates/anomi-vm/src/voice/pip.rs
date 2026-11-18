//! A stream or a camera kept in a corner of the window while its call isn't
//! on screen, as Discord's picture in picture is: what it shows and what it
//! asks for. Task V06 (docs/iced/tasks/voice.md). The Slint side is
//! `StreamPip` and its `StreamWaiting` (`ui/voice.slint:676`, `:658`), filled
//! from `pip-kind` in `src/ui/refresh.rs:4576` and placed in `ui/yapfast.slint`
//! (`pip-left`, `pip-bottom`, `dropped`).
//!
//! The shell keeps the corner it sits in (`shell::Shell::pip`) and opens it
//! as the layer `Layer::Pip`; the card here draws what the window's
//! projection decided to show (the watched stream, this account's own screen,
//! or a camera) and where a drag has taken it from its corner.

use iced::Vector;

use crate::icon::Icon;
use crate::kind::Live;
use crate::voice::tiles::Viewers;

/// The button that stops what the card shows (`stop-icon`, `stop-label`): a
/// watched stream's is the cross, this account's own screen's the struck
/// screen. A camera has nothing to stop.
#[derive(Clone, Debug, PartialEq)]
pub struct Stop {
    pub icon: Icon,
    /// "Stop watching" or "Stop sharing", its tooltip.
    pub label: String,
}

/// A stream in picture in picture: what its screen draws. The window holds it
/// as `vm.voice.pip`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pip {
    /// What it shows, over its bottom edge under the pointer (`title`).
    pub title: String,
    /// What it waits for until its first picture: "Connecting to Bob's
    /// stream..." (`status`).
    pub status: String,
    /// Its frames as they come, drawn inside the card's rounded corners,
    /// which the renderer cuts, never copied; none until the first, when
    /// the status shows (`frame`).
    pub frame: Option<Live>,
    /// This account's own screen is live: the LIVE tag at its top left
    /// (`live`).
    pub live: bool,
    /// Who watches what it shows, at its top right (`viewers`,
    /// `viewer-names`).
    pub viewers: Viewers,
    /// What stops what it shows; none for a camera (`stop-label`).
    pub stop: Option<Stop>,
    /// How far a drag has taken it from its corner, in pixels (`drag-x`,
    /// `drag-y`); zero at rest.
    pub drag: Vector,
}

impl Pip {
    /// The card's size, `StreamPip`'s `width` and `height` (`:683`).
    pub const WIDTH: f32 = 320.0;
    pub const HEIGHT: f32 = 180.0;
}

/// A stream in picture in picture: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Back to the call: its tile shown large and its conversation opened
    /// (`open-call`, `ui/yapfast.slint`'s `StreamPip` handler).
    Open,
    /// Stop watching the stream, or sharing the screen (`stop`).
    Stop,
    /// A drag past the 4 px that make one has taken the card this far from
    /// its corner: the whole offset, not the step since the last, so events
    /// that arrive together don't count twice. The app keeps it as
    /// [`Pip::drag`] (`drag-x`, `drag-y`).
    Drag(Vector),
    /// The drag ended: the app sets the card in the corner nearest to where
    /// it was let go and clears [`Pip::drag`] (`dropped`).
    Dropped,
}
