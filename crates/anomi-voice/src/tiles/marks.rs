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
        top = top.push(pulse(starting_pill(), PILL / 2.0, motion).boxed());
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
    pill(
        text_in("ParticipantGrid", "Watch stream"),
        |theme: &Theme| theme.palette().on_danger,
        |theme: &Theme| theme.palette().danger,
    )
}

/// "Starting camera...", white on the scrim.
fn starting_pill<'a>() -> Element<'a, Intent> {
    pill(tr!("Starting camera..."), |_| Color::WHITE, |_| SCRIM)
}

fn pill<'a>(
    label: &'a str,
    ink: fn(&Theme) -> Color,
    ground: fn(&Theme) -> Color,
) -> Element<'a, Intent> {
    container(
        text(label)
            .size(PILL_TEXT)
            .font(typography::BOLD)
            .style(move |theme: &Theme| text::Style {
                color: Some(ink(theme)),
                ..text::Style::default()
            }),
    )
    .padding(Padding::ZERO.left(PILL_SIDE).right(PILL_SIDE))
    .center_y(PILL)
    .style(move |theme: &Theme| container::Style {
        background: Some(Background::Color(ground(theme))),
        border: Border::default().rounded(PILL / 2.0),
        ..container::Style::default()
    })
    .boxed()
}
