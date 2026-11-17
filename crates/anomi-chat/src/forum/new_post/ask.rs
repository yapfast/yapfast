//! The question when closing what was written, as Discord's client asks
//! (`ui/forum.slint:421`-`:446`): a card of 440 px.

use iced::widget::{column, container, row, space, text};
use iced::{Border, Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::new_post::Intent;
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

const WIDTH: f32 = 440.0;
const RADIUS: f32 = 10.0;
const PAD: f32 = 16.0;
const GAP: f32 = 10.0;
const TITLE: f32 = 17.0;
/// The buttons' row: `spacing: 8px; padding-top: 6px` (`ui/forum.slint:464`).
const BUTTON_GAP: f32 = 8.0;
const BUTTON_TOP: f32 = 6.0;

pub(super) fn view<'a>(motion: Motion) -> Element<'a, Intent> {
    let buttons = row![
        space().width(Fill),
        action_button(tr!("Back to editing"), Kind::Ghost, motion).on_press(Intent::Stay),
        action_button(tr!("Exit"), Kind::Danger, motion).on_press(Intent::Leave),
    ]
    .spacing(BUTTON_GAP)
    .padding(Padding::default().top(BUTTON_TOP));
    container(
        column![
            text(tr!("Are you sure you want to go?"))
                .size(TITLE)
                .font(typography::BOLD)
                .style(catalog::text::primary),
            text(tr!("Any changes you made will be lost if you exit now."))
                .size(typography::size::SMALL)
                .style(catalog::text::secondary),
            buttons,
        ]
        .spacing(GAP)
        .padding(PAD),
    )
    .width(Length::Fixed(WIDTH))
    .style(|theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            background: Some(palette.sidebar.into()),
            border: Border::default()
                .rounded(RADIUS)
                .width(1)
                .color(palette.edge),
            ..container::Style::default()
        }
    })
    .boxed()
}
