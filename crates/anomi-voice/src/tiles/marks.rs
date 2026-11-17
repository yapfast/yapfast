//! What a tile says over its picture, 10 px in from its edges: at its top
//! left "Starting camera..." while this account's camera is on its way
//! (`ui/voice.slint:1094`), at its top right the pill offering a stream
//! that a click watches (`:1100`), and at its bottom left its chip
//! (`TileChip`, `:819`).

use iced::widget::{Row, Space, column, container, text};
use iced::{Background, Border, Color, Fill, Padding};
use yapfast_lang::{text_in, tr};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::voice::tiles::{Fill as Filling, Intent, Tile};

use super::pulse::pulse;
use super::{INSET, PILL, PILL_SIDE, PILL_TEXT, SCRIM, chip};

pub fn view(tile: &Tile, motion: Motion) -> Element<'_, Intent> {
    let mut top: Row<Element<'_, Intent>> = Row::new();
    if tile.starting {
        top = top.push(starting_pill(motion));
    }
    top = top.push(Space::new().width(Fill).boxed());
    if tile.watch && !matches!(tile.fill, Filling::Offer { .. }) {
        top = top.push(watch_pill());
    }
    column![top, Space::new().height(Fill), chip::view(&tile.chip)]
        .padding(INSET)
        .width(Fill)
        .height(Fill)
        .boxed()
}

/// "Watch stream" on the danger colour, as `ParticipantGrid` words it
/// (`:1042`, `:1105`).
pub fn watch_pill<'a>() -> Element<'a, Intent> {
    words(
        text_in("ParticipantGrid", "Watch stream"),
        |theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().danger)),
            text_color: Some(theme.palette().on_danger),
            border: Border::default().rounded(PILL / 2.0),
            ..container::Style::default()
        },
    )
}

/// "Starting camera...", white on the scrim, beating while `motion` plays.
fn starting_pill<'a>(motion: Motion) -> Element<'a, Intent> {
    let bare = |_: &Theme| container::Style::default();
    pulse(
        words(tr!("Starting camera..."), bare),
        SCRIM,
        Color::WHITE,
        PILL / 2.0,
        motion,
    )
    .boxed()
}

/// A pill's words on its `ground`, 10 px from its ends, in the colour of
/// what holds them.
fn words<'a>(label: &'a str, ground: fn(&Theme) -> container::Style) -> Element<'a, Intent> {
    container(text(label).size(PILL_TEXT).font(typography::BOLD))
        .padding(Padding::ZERO.left(PILL_SIDE).right(PILL_SIDE))
        .center_y(PILL)
        .style(ground)
        .boxed()
}
