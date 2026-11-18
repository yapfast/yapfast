//! A message's poll, and the dialog that makes one. Task C07
//! (docs/iced/tasks/chat.md). The Slint side is `PollView`
//! (`ui/messages.slint:690`) and `PollDialog` (`ui/dialogs.slint:932`); what
//! each asks for is `anomi_vm::chat::poll::Intent`.
//!
//! - [`view`] is the card on a message: the question, the answers to pick
//!   or their results, the votes and time left, the buttons and the
//!   confirmation of ending it. A message with no poll (no answers) draws
//!   nothing.
//! - [`dialog`] is the layer `Layer::Poll`: the shell gives it the backdrop,
//!   Escape and the way in; this draws the card.

mod answer;
mod card;
mod choices;
mod dialog;
mod drafts;
mod emoji;
mod ends;
mod link;
#[cfg(feature = "shots")]
mod shots;
mod widest;

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chat::poll::{Dialog, Intent, Poll};

pub use dialog::{CANCEL, POST, QUESTION, question_id};

pub fn view(poll: &Poll, motion: Motion) -> impl View<Intent> + '_ {
    if poll.answers.is_empty() {
        space().boxed()
    } else {
        card::view(poll, motion)
    }
}

/// The dialog that makes a poll. Drawn by the shell over the window.
pub fn dialog(poll: &Dialog, motion: Motion) -> impl View<Intent> + '_ {
    dialog::view(poll, motion)
}

/// Its shots for `anomi-shots`: the whole window from a fixture, the first
/// two the golden screenshots' (`chan/bot_embed_card_poll`,
/// `dialog/create_poll`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    shots::all()
}
