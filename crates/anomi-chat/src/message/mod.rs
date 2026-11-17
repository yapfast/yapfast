//! A message. Task C02 (docs/iced/tasks/chat.md) fills it; F01's stub draws
//! nothing until then.

mod author;
mod hover_bar;
mod notice;
mod reply;

use iced::widget::column;

use anomi_theme::prelude::*;
use anomi_vm::chat::{self, message::Message};

use crate::{attachments, components, embeds, inline_edit, poll, reactions};

/// A message, with the parts other tasks draw. The calls to them are F01's
/// wiring: keep them as the view grows.
pub fn view(message: &Message, motion: Motion) -> impl View<chat::Intent> + '_ {
    column![
        inline_edit::view(&message.editing, motion).map(chat::Intent::from),
        attachments::view(&message.attachments, motion).map(chat::Intent::from),
        embeds::view(&message.embeds, motion).map(chat::Intent::from),
        components::view(&message.components, motion).map(chat::Intent::from),
        poll::view(&message.poll, motion).map(chat::Intent::from),
        reactions::view(&message.reactions, motion).map(chat::Intent::from),
    ]
}

/// Its shots for `anomi-shots`: none until C02 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
