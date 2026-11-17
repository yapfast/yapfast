//! What went wrong in the call, or what to watch (`DockNotice`,
//! `ui/voice.slint:362`): a card tinted by its tone, a sign, the text
//! wrapped in the dock's width and, where it can be put away, a cross.

use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography::size;
use anomi_theme::{Palette, catalog};
use anomi_vm::icon::Icon;
use anomi_vm::voice::dock::Intent;
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

/// `padding: 8px; spacing: 8px; border-radius: 8px; with-alpha(0.12)`.
const PAD: f32 = 8.0;
const RADIUS: f32 = 8.0;
const WASH: f32 = 0.12;
const SIGN: f32 = 16.0;
/// The cross: a 20 px button, its picture 14, corners 4.
const CROSS: f32 = 20.0;
const CROSS_GLYPH: f32 = 14.0;
const CROSS_RADIUS: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// `Theme.loud`: something to watch.
    Warning,
    /// `Theme.danger`: something failed.
    Error,
}

impl Tone {
    fn colour(self, palette: &Palette) -> iced::Color {
        match self {
            Self::Warning => palette.loud,
            Self::Error => palette.danger,
        }
    }
}

pub fn view<'a>(
    message: &'a str,
    tone: Tone,
    closable: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let sign =
        icon(Icon::TriangleAlert, SIGN).style(move |theme: &Theme, _| iced::widget::svg::Style {
            color: Some(tone.colour(theme.palette())),
        });
    let mut card = row![
        column![sign].padding(Padding::ZERO.top(1.0)),
        text(message)
            .size(size::LABEL)
            .width(Fill)
            .style(catalog::text::primary),
    ]
    .spacing(PAD);
    if closable {
        card = card.push(
            column![
                icon_button(Icon::X, tr!("Dismiss"), motion)
                    .size(CROSS)
                    .glyph(CROSS_GLYPH)
                    .radius(CROSS_RADIUS)
                    .on_press(Intent::DismissError)
                    .view()
            ]
            .boxed(),
        );
    }
    container(
        container(card.align_y(Alignment::Start).padding(PAD))
            .width(Fill)
            .style(move |theme: &Theme| container::Style {
                background: Some(tone.colour(theme.palette()).scale_alpha(WASH).into()),
                border: Border::default().rounded(RADIUS),
                ..container::Style::default()
            }),
    )
    .padding(Padding::ZERO.left(super::INSET))
    .boxed()
}
