//! The list over the composer: `CommandFrame`, `CommandHeading` and
//! `CommandList` (`ui/app_commands.slint:22`, `:31`, `:40`; `show_app_commands`
//! at `src/ui/app_commands.rs`). A heading, then the rows, eight showing at
//! once; the list scrolls past that and the app keeps the chosen row in view
//! ([`reveal`](super::reveal)). A row is 44 px: the app's icon (or a "/" on
//! the accent where a command has none), the name with its `/`, the
//! description under it, and the app's name at the end.
//!
//! The Slint frame grows from the composer as it shows (a scale and fade
//! over `Popup`). iced at this pin has no layer opacity and a scaled
//! overlay would show its text blurred and its pictures unscaled, so the
//! frame appears at once.

use iced::widget::{Column, Row, button, column, container, text};
use iced::{Border, Color, Length, Shadow, Vector, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::chat::commands::{Choice, Commands, Intent};
use anomi_widgets::list::list;
use anomi_widgets::picture::{self, Fit};

/// `row-height: 44px`.
pub const ROW: f32 = 44.0;
/// Eight rows show at once.
pub const SHOWN: usize = 8;
/// `padding: 6px` of the list's column, and `24px` of the heading.
const PAD: f32 = 6.0;
const HEADING: f32 = 24.0;
/// The heading's and a row's insets (`padding-left: 6px`; `padding-left: 8px;
/// padding-right: 10px; spacing: 10px`).
const HEADING_INSET: f32 = 6.0;
const ROW_LEFT: f32 = 8.0;
const ROW_RIGHT: f32 = 10.0;
const ROW_GAP: f32 = 10.0;
/// The app's icon and the "/" disc: `24px`, round.
const ICON: f32 = 24.0;
/// The app's name at most `160px`.
const APP_WIDTH: f32 = 160.0;
/// `font-size: 11px` of the heading.
const HEADING_SIZE: f32 = 11.0;
/// The frame's shadow: `24px` blur, `#00000080`, 8 px down.
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0x80 as f32 / 255.0);

/// The frame, the heading and the rows.
pub fn frame<'a>(commands: &'a Commands, motion: Motion) -> Element<'a, Intent> {
    let rows = commands.choices.len();
    let shown = list(
        rows,
        |index| index as u64,
        move |index| entry(index, &commands.choices[index], commands, motion),
        motion,
    )
    .id(super::id())
    .reveal(Some(commands.chosen as u64), 0.0)
    .width(Length::Fill)
    .height(rows.min(SHOWN) as f32 * ROW);
    let heading = container(
        text(commands.heading.as_str())
            .size(HEADING_SIZE)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .align_y(alignment::Vertical::Center)
            .height(HEADING)
            .style(anomi_theme::catalog::text::muted),
    )
    .padding(iced::Padding::ZERO.left(HEADING_INSET).right(HEADING_INSET))
    .width(Length::Fill);
    container(column![heading, shown])
        .padding(PAD)
        .width(Length::Fill)
        .style(ground)
        .boxed()
}

/// `background: Theme.raised; border: 1px Theme.edge; radius: 8px` and the
/// drop shadow.
fn ground(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border {
            color: palette.edge,
            width: 1.0,
            radius: metrics::RADIUS_CARD.into(),
        },
        shadow: Shadow {
            color: SHADOW,
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        ..container::Style::default()
    }
}

/// One row: lit when chosen, or under the pointer.
fn entry<'a>(
    index: usize,
    choice: &'a Choice,
    commands: &'a Commands,
    motion: Motion,
) -> Element<'a, Intent> {
    let chosen = index == commands.chosen;
    let name = if commands.commands {
        format!("/{}", choice.name)
    } else {
        choice.name.clone()
    };
    let mut parts: Vec<Element<'a, Intent>> = Vec::new();
    match &choice.icon {
        Some(handle) => parts.push(
            picture::picture(handle.clone())
                .side(ICON)
                .round()
                .fit(Fit::Cover)
                .motion(motion)
                .boxed(),
        ),
        None if commands.commands => parts.push(slash()),
        None => {}
    }
    let mut words: Vec<Element<'a, Intent>> = vec![
        text(name)
            .size(typography::size::SMALL)
            .font(typography::SEMIBOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(anomi_theme::catalog::text::primary)
            .boxed(),
    ];
    if !choice.description.is_empty() {
        words.push(
            text(choice.description.as_str())
                .size(typography::size::CAPTION)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(anomi_theme::catalog::text::muted)
                .boxed(),
        );
    }
    parts.push(Column::with_children(words).width(Length::Fill).boxed());
    if !choice.app.is_empty() {
        parts.push(
            text(choice.app.as_str())
                .size(typography::size::CAPTION)
                .font(typography::SEMIBOLD)
                .width(Length::Shrink.max(APP_WIDTH))
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(anomi_theme::catalog::text::muted)
                .boxed(),
        );
    }
    let line = Row::with_children(parts)
        .spacing(ROW_GAP)
        .align_y(alignment::Vertical::Center);
    let inset = iced::Padding::ZERO.left(ROW_LEFT).right(ROW_RIGHT);
    button(container(line).padding(inset).center_y(ROW))
        .padding(0)
        .width(Length::Fill)
        .height(ROW)
        .on_press(Intent::Pick(index))
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let ground = if chosen {
                Some(palette.active)
            } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                Some(palette.hover)
            } else {
                None
            };
            button::Style {
                background: ground.map(Into::into),
                text_color: palette.text,
                border: Border::default().rounded(metrics::RADIUS_ROW),
                ..button::Style::default()
            }
        })
        .boxed()
}

/// A command with no picture: a "/" in the accent on its soft ground.
fn slash<'a>() -> Element<'a, Intent> {
    container(
        text("/")
            .size(13)
            .font(typography::BOLD)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.palette().accent),
                ..text::Style::default()
            }),
    )
    .center(ICON)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().accent_soft.into()),
        border: Border::default().rounded(ICON / 2.0),
        ..container::Style::default()
    })
    .boxed()
}
