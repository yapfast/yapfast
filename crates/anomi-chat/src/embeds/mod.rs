//! A message's embeds: link previews and bot cards, each behind a bar of its
//! colour (`EmbedView`, `ui/messages.slint:2108`). Task C05.
//!
//! - `card`: the bar, the texts, the fields, the thumbnail.
//! - `gallery`: the card's pictures, a video's poster with its play button.

mod card;
mod gallery;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::column;

use anomi_theme::prelude::*;
use anomi_vm::chat::embeds::{Embeds, Intent};

#[cfg(feature = "shots")]
pub use shots::shots;

/// The cards of a message, one under another, `4` px below what is above.
pub fn view(embeds: &Embeds, motion: Motion) -> impl View<Intent> + '_ {
    column(
        embeds
            .cards
            .iter()
            .map(|one| card::view(embeds.item, one, motion)),
    )
}
