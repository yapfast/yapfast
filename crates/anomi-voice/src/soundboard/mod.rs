//! The soundboard: `Soundboard` (`ui/voice.slint:1699`), the card that opens
//! over the call from its button, or under Settings' entrance-sound picker.
//! The shell draws it as a popover (the way in and out, the backdrop, Escape,
//! placing it at its button); this is the card.
//!
//! A title, the note when no sound can be played (deafened, server muted),
//! "Loading sounds..." while there are none, then the sounds under their
//! headings three to a row, in a list that scrolls; the card is as tall as
//! its rows, up to 440. Each sound is a [`chip`]: a click plays it, and
//! hovering offers hearing it and starring it (a favourite's star stays).
//!
//! Known gaps: the two small buttons have no tooltip (the Slint ones only
//! name themselves to accessibility), and the soundboard's custom emoji are
//! asked for by the app, which knows the sounds it holds (Slint's
//! `soundboard-opened`).

mod card;
pub mod chip;
mod paint;
#[cfg(feature = "shots")]
mod shots;

use anomi_theme::prelude::*;
use anomi_vm::voice::soundboard::{Intent, Soundboard};

pub use card::{HEIGHT_MAX, WIDTH};
#[cfg(feature = "shots")]
pub use shots::shots;

pub fn view(soundboard: &Soundboard, motion: Motion) -> impl View<Intent> + '_ {
    card::card(soundboard, motion)
}
