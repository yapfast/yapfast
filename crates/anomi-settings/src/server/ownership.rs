//! Delete Server and Transfer Ownership: Slint's `OwnershipFrame`,
//! `DeleteServerDialog` and `TransferOwnershipDialog`
//! (`ui/server_ownership.slint:36`, `:69`, `:104`) over what
//! `ownership_prompt` fills (`src/ui/server_settings.rs:348`).
//!
//! Both are the app's dialog, a raised card with a title, Discord's words,
//! and Cancel and a red button at the end. Delete asks for the server's name
//! typed without it; Transfer shows both owners' avatars, a warning for a
//! Verified or Partnered server, the acknowledgement's box, and, when
//! two-factor authentication doesn't confirm it, the code Discord emailed.
//! Enter in the field confirms; Escape and a press beside the card are the
//! shell's layer, which asks [`Intent::Cancel`]. The app focuses [`NAME`] or
//! [`CODE`] when the field appears. Nothing may be asked again while it is
//! sent. The shell has no layer of its own for it yet (see the gap file).

mod card;
mod delete;
#[cfg(feature = "shots")]
mod shots;
mod transfer;

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::settings::ownership::{Intent, Kind, Ownership};

#[cfg(feature = "shots")]
pub use shots::shots;

/// The ids of the two fields, for the app's `field::focus`.
pub const NAME: &str = "ownership-name";
pub const CODE: &str = "ownership-code";
/// The ids of the buttons, so the keys and the tests can reach one.
pub const CANCEL: &str = "ownership-cancel";
pub const CONFIRM: &str = "ownership-confirm";

pub fn view(ownership: &Ownership, motion: Motion) -> Element<'_, Intent> {
    match ownership.kind {
        Kind::None => space().boxed(),
        Kind::Delete => delete::view(ownership, motion).boxed(),
        Kind::Transfer => transfer::view(ownership, motion).boxed(),
    }
}
