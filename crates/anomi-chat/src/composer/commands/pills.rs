//! The command being written, over the composer's text: its name, then a
//! pill for each option (`CommandPills`, `ui/app_commands.slint:117`). A
//! pill is as wide as its name, value and error, up to the row's width, past
//! which its value and error are cut short; pills that don't fit on a row go
//! on to the next. The focused one, which the text below writes, is
//! outlined; one that's wrong says why in red. A click takes the composer to
//! a pill, or picks the file of a file's. An optional pill with nothing
//! entered, not focused, is faint (`opacity: 0.6`, which here fades its
//! colours alone: iced at this pin has no layer opacity).

use iced::widget::{Row, button, container, text};
use iced::{Border, Color, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::chat::commands::{Commands, Intent, Pill, Written};
use anomi_vm::icon::Icon;
use anomi_widgets::icon::icon;

/// `height: 26px; border-radius: 4px`, and the room inside (`padding: 6px`,
/// `spacing: 4px`) and between pills (`spacing: 6px`).
const HEIGHT: f32 = 26.0;
const RADIUS: f32 = 4.0;
const INSET: f32 = 6.0;
const GAP: f32 = 4.0;
const SPACING: f32 = 6.0;
/// The file icon, 14 px.
const FILE: f32 = 14.0;
/// `opacity: 0.6`.
const FAINT: f32 = 0.6;

/// The pills of the command written, or nothing when none is.
pub fn pills(commands: &Commands, _motion: Motion) -> Element<'_, Intent> {
    let Some(written) = &commands.written else {
        return iced::widget::space().boxed();
    };
    let mut items = vec![name(written)];
    for (index, pill) in written.options.iter().enumerate() {
        items.push(option(index, pill, index == written.focused));
    }
    Row::with_children(items)
        .spacing(SPACING)
        .wrap()
        .vertical_spacing(SPACING)
        .boxed()
}

/// `/name` on the accent's soft ground.
fn name(written: &Written) -> Element<'_, Intent> {
    container(
        text(format!("/{}", written.name))
            .size(typography::size::SMALL)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.palette().accent),
                ..text::Style::default()
            }),
    )
    .padding(iced::Padding::ZERO.left(INSET).right(INSET))
    .center_y(HEIGHT)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().accent_soft.into()),
        border: Border::default().rounded(RADIUS),
        ..container::Style::default()
    })
    .boxed()
}

/// One option's pill.
fn option(index: usize, pill: &Pill, focused: bool) -> Element<'_, Intent> {
    let faint = !pill.required && pill.value.is_empty() && !focused;
    let fade = move |color: Color| {
        if faint {
            Color {
                a: color.a * FAINT,
                ..color
            }
        } else {
            color
        }
    };
    let mut parts: Vec<Element<'_, Intent>> = Vec::new();
    if pill.file {
        parts.push(
            icon(Icon::File, FILE)
                .style(move |theme: &Theme, _| iced::widget::svg::Style {
                    color: Some(fade(theme.palette().muted)),
                })
                .boxed(),
        );
    }
    parts.push(
        text(format!("{}:", pill.name))
            .size(13)
            .font(typography::SEMIBOLD)
            .wrapping(text::Wrapping::None)
            .style(move |theme: &Theme| text::Style {
                color: Some(fade(theme.palette().muted)),
                ..text::Style::default()
            })
            .boxed(),
    );
    if !pill.value.is_empty() {
        parts.push(
            text(pill.value.as_str())
                .size(13)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(move |theme: &Theme| text::Style {
                    color: Some(fade(theme.palette().text)),
                    ..text::Style::default()
                })
                .boxed(),
        );
    }
    if !pill.error.is_empty() {
        parts.push(
            text(pill.error.as_str())
                .size(typography::size::CAPTION)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.palette().danger_text()),
                    ..text::Style::default()
                })
                .boxed(),
        );
    }
    let line = Row::with_children(parts)
        .spacing(GAP)
        .align_y(alignment::Vertical::Center);
    let wrong = !pill.error.is_empty();
    let intent = if pill.file {
        Intent::File(index)
    } else {
        Intent::Option(index)
    };
    button(
        container(line)
            .padding(iced::Padding::ZERO.left(INSET).right(INSET))
            .center_y(HEIGHT),
    )
    .padding(0)
    .height(HEIGHT)
    .width(Length::Shrink)
    .on_press(intent)
    .style(move |theme: &Theme, _| {
        let palette = theme.palette();
        let edge = if wrong {
            palette.danger
        } else if focused {
            palette.accent
        } else {
            palette.line
        };
        let ground = if focused {
            palette.active
        } else {
            palette.raised
        };
        button::Style {
            background: Some(fade(ground).into()),
            text_color: palette.text,
            border: Border {
                color: fade(edge),
                width: 1.0,
                radius: RADIUS.into(),
            },
            ..button::Style::default()
        }
    })
    .boxed()
}
