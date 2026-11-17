//! The dialog's card, its title row and its buttons, as Slint's
//! `ChannelDialog` draws them (`ui/server.slint:516`).

use iced::advanced::widget::Id;
use iced::widget::{column, container, row, space, text};
use iced::{Border, Color, Fill, Length, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::create_channel::Intent;
use anomi_widgets::button::{Kind, action_button, icon_button};
use yapfast_lang::tr;

/// `max(460px, ..)`, and the title's `font-size: 18px`.
const WIDTH: f32 = 460.0;
const TITLE: f32 = 18.0;
/// `padding: 20px; spacing: 12px`.
const PADDING: f32 = 20.0;
const SPACING: f32 = 12.0;
/// The shadow under the card: `#00000066`, 12 px down, 32 px blur
/// (`ui/server.slint:534`).
const SHADOW_ALPHA: f32 = 0.4;
const SHADOW_OFFSET: f32 = 12.0;
const SHADOW_BLUR: f32 = 32.0;
/// The X: a 28 px button with an 18 px glyph (`ui/server.slint:549`).
const CLOSE_SIZE: f32 = 28.0;
const CLOSE_GLYPH: f32 = 18.0;
/// The ids of the two buttons, for focus and for the tests.
pub const CANCEL: &str = "dialog-cancel";
pub const CONFIRM: &str = "dialog-confirm";

/// The card around `parts`. The layer keeps it inside a narrow window.
pub fn card<'a>(parts: Vec<Element<'a, Intent>>) -> impl View<Intent> + 'a {
    container(column(parts).spacing(SPACING))
        .padding(PADDING)
        .width(Length::Fixed(WIDTH))
        .style(dialog)
}

/// Raised, edged, 12 px corners, the shadow under it (`drop-shadow-blur:
/// 32px`, `-offset-y: 12px`, `#00000066`).
fn dialog(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, SHADOW_ALPHA),
            offset: Vector::new(0.0, SHADOW_OFFSET),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}

/// The title, bold on one line that elides, and the X at its end.
pub fn title_row<'a>(words: &'a str, motion: Motion) -> Element<'a, Intent> {
    let x = icon_button(Icon::X, tr!("Close"), motion)
        .size(CLOSE_SIZE)
        .glyph(CLOSE_GLYPH)
        .on_press(Intent::Close)
        .view();
    row![
        text(words)
            .size(TITLE)
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .width(Fill)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
        x
    ]
    .align_y(iced::Alignment::Center)
    .boxed()
}

/// Cancel and the main button, at the end, a little below the rest.
pub fn buttons<'a>(
    confirm: &'a str,
    kind: Kind,
    enabled: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    container(
        row![
            space().width(Fill),
            action_button(tr!("Cancel"), Kind::Ghost, motion)
                .id(Id::new(CANCEL))
                .on_press(Intent::Close),
            action_button(confirm, kind, motion)
                .id(Id::new(CONFIRM))
                .on_press(Intent::Confirm)
                .enabled(enabled),
        ]
        .spacing(metrics::SPACE_S),
    )
    .padding(iced::Padding::ZERO.top(metrics::SPACE_XS))
    .boxed()
}
