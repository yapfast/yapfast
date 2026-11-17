//! A stream on its way: a ring turning once a second over a note that says
//! what it waits for (`StreamWaiting`, `ui/voice.slint:658`). It may wait
//! for ever, so the ring turns only while someone may be looking
//! (`Motion::watched`). A stream's ground is black in every theme, so both
//! are a fixed grey. The stage's tiles draw it, and picture in picture may.

use iced::widget::{column, container, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_widgets::icon::Icon;

use super::GREY;
use super::spinner::spinner;

/// The ring, 28 px, 14 px above the note, which is 14 px and kept 16 px
/// from the sides (`:660`, `:664`, `:669`).
const RING: f32 = 28.0;
const GAP: f32 = 14.0;
const SIDE: f32 = 16.0;
const NOTE: f32 = typography::size::SMALL;

/// The ring over `note`, in the middle of what it fills.
pub fn waiting<'a, M: 'a>(note: &'a str, motion: Motion) -> impl View<M> + 'a {
    let note = text(note)
        .size(NOTE)
        .color(GREY)
        .width(Fill)
        .align_x(Alignment::Center);
    container(
        column![spinner(Icon::Loader, RING, GREY, motion), note]
            .spacing(GAP)
            .align_x(Alignment::Center),
    )
    .padding(Padding::ZERO.left(SIDE).right(SIDE))
    .center_y(Fill)
    .width(Fill)
}
