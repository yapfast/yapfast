//! A video in a message, and full screen (`VideoCell`, `VideoControls`,
//! `VideoViewer`, `ui/messages.slint:524`, `:412`, `:2731`), drawing
//! `anomi_vm::chat::video`.
//!
//! - [`view`]: the layer a gallery lays over a video's poster ([`cell`]):
//!   the play disc, then the frame, "Loading...", why it can't play, and
//!   its controls while the pointer is on it or it's paused ([`hovered`]).
//! - [`full_screen`]: the same video large over the window ([`full`]),
//!   drawn by the shell as `Layer::Video`, with Space and the arrows
//!   ([`keys`]).
//! - [`controls`]: play, time, the position bar ([`scrub`]), mute with the
//!   volume over it ([`sound`]) and full screen, white over the video in
//!   every theme ([`look`]).
//!
//! Frames are the app's handles drawn as they are (no copy per frame);
//! nothing here asks for a frame of its own but the volume panel's fade,
//! which lands at once while still.

mod cell;
pub mod controls;
pub mod full;
mod hovered;
pub mod keys;
pub mod look;
pub mod scrub;
pub mod sound;

#[cfg(feature = "shots")]
mod shots;

use anomi_theme::prelude::*;
use anomi_vm::chat::video::{FullScreen, Intent, Video};

/// A video's layer over its poster in a gallery's cell, as large as the
/// cell.
pub fn view(video: &Video, motion: Motion) -> impl View<Intent> + '_ {
    cell::cell(video, motion)
}

/// A video played full screen. Drawn by the shell over the window.
pub fn full_screen(video: &FullScreen, motion: Motion) -> impl View<Intent> + '_ {
    full::full_screen(video, motion)
}

/// Its shots for `anomi-shots`: the whole window from a fixture (see
/// `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    shots::all()
}
