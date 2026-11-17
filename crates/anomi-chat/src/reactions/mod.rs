//! A message's reactions, and the Reactions window. Task C03
//! (docs/iced/tasks/chat.md): `ReactionToggle` (`ui/messages.slint:272`),
//! the `ReactionHover` tooltip, `ReactionsDialog` (`ui/dialogs.slint:797`).

mod chips;
mod dialog;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::{text, text::Wrapping};

use anomi_theme::prelude::*;
use anomi_vm::chat::reactions::{Dialog, Face, Intent, Reactions};
use anomi_widgets::picture::picture;

/// A message's reactions under its text, wrapping onto more lines.
pub fn view(reactions: &Reactions, motion: Motion) -> impl View<Intent> + '_ {
    chips::view(reactions, motion)
}

/// The Reactions window: who reacted with what. Drawn by the shell over the
/// window.
pub fn dialog(reactions: &Dialog, motion: Motion) -> impl View<Intent> + '_ {
    dialog::view(reactions, motion)
}

/// The side of the Twemoji glyph of text `size` big: it runs to 72/53 of
/// the size, as in the paragraph and Slint's emoji font.
fn glyph(size: f32) -> f32 {
    size * 72.0 / 53.0
}

/// An emoji as text `size` big draws it, as Slint's `Twemoji.text` at that
/// font size: glyphs as text, a custom one as its picture the glyph's side,
/// and the room of one on its way kept empty.
fn emoji<'a>(face: &'a Face, size: f32, motion: Motion) -> Element<'a, Intent> {
    use iced::widget::space;
    match face {
        Face::Glyphs(glyphs) => text(glyphs.as_str())
            .size(size)
            .wrapping(Wrapping::None)
            .boxed(),
        Face::Picture(expected) => match expected.ready() {
            Some(handle) => picture(handle.clone())
                .side(glyph(size))
                .motion(motion)
                .boxed(),
            None => space().width(glyph(size)).height(glyph(size)).boxed(),
        },
    }
}

/// Its shots for `anomi-shots`: the whole window from a fixture.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    shots::shots()
}
