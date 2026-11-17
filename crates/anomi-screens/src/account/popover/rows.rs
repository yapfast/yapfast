//! The popover's rows: 40 px high, 4 px round, lit under the pointer
//! (`MenuItem`, `ui/controls.slint:1156`), the status row that unfolds the
//! four statuses, and each of those (`StatusOption`,
//! `ui/dialogs.slint:567`).

use iced::widget::{button, column, container, hover, row, text};
use iced::{Alignment, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, Theme, catalog, metrics, typography};
use anomi_vm::account::{Choice, Intent, Popover};
use anomi_widgets::avatar::Ground;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::presence::{Ring, status_dot};
use yapfast_lang::tr;

const HEIGHT: f32 = 40.0;
const RADIUS: f32 = 4.0;
const DOT: f32 = 16.0;
const GLYPH: f32 = 16.0;
/// An icon's gap to its words in a `MenuItem` (`spacing: 8px`), and the
/// overlay's alpha under the pointer (`Theme.overlay` at 0.09).
const ITEM_GAP: f32 = 8.0;
const ITEM_WASH: f32 = 0.09;
/// A status with no note (`height: max(36px, ..)`, `dialogs.slint:575`).
const OPTION_HEIGHT: f32 = 36.0;

/// The status row's fill under the pointer is the chosen row's.
fn lit(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.palette();
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hovered.then(|| palette.active.into()),
        text_color: palette.text,
        border: Border::default().rounded(RADIUS),
        ..button::Style::default()
    }
}

fn label<'a>(words: &'a str) -> iced::widget::Text<'a, Theme> {
    text(words)
        .size(typography::size::SMALL)
        .font(typography::MEDIUM)
        .style(catalog::text::primary)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
}

/// An icon, then its words: a `MenuItem` (`ui/controls.slint:1156`) is the
/// row at rest with the lit one laid over it under the pointer.
pub fn plain<'a>(glyph: Icon, words: &'a str, intent: Intent) -> impl View<Intent> + 'a {
    hover(
        item(glyph, words, intent.clone(), false),
        item(glyph, words, intent, true),
    )
}

fn item<'a>(glyph: Icon, words: &'a str, intent: Intent, lit: bool) -> Element<'a, Intent> {
    button(
        row![
            icon(glyph, GLYPH).style(move |theme: &Theme, _| iced::widget::svg::Style {
                color: Some(if lit {
                    theme.palette().text
                } else {
                    theme.palette().secondary
                }),
            }),
            label(words)
        ]
        .spacing(ITEM_GAP)
        .align_y(Alignment::Center),
    )
    .padding(
        Padding::default()
            .left(metrics::SPACE_S)
            .right(metrics::SPACE_S),
    )
    .width(Fill)
    .height(HEIGHT)
    .on_press(intent)
    .style(move |theme: &Theme, _| {
        let palette = theme.palette();
        button::Style {
            background: lit.then(|| palette.lift(ITEM_WASH).into()),
            text_color: palette.text,
            border: Border::default().rounded(RADIUS),
            ..button::Style::default()
        }
    })
    .boxed()
}

/// The line between the groups of a card (`height: 1px; Theme.line`).
pub fn rule<'a>() -> impl View<Intent> + 'a {
    container(iced::widget::space())
        .width(Fill)
        .height(1)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().line.into()),
            ..container::Style::default()
        })
}

/// What the status row says of a choice.
pub fn name(choice: Choice) -> &'static str {
    match choice {
        Choice::Online => tr!("Online"),
        Choice::Idle => tr!("Idle"),
        Choice::DoNotDisturb => tr!("Do Not Disturb"),
        Choice::Invisible => tr!("Invisible"),
    }
}

/// The row that shows the chosen status and unfolds the others; its arrow
/// points down while they show. (A turned svg is drawn at no width by
/// tiny-skia, so it is a glyph of its own.)
pub fn status<'a>(popover: &'a Popover) -> impl View<Intent> + 'a {
    let arrow = if popover.expanded {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    button(
        row![
            container(status_dot(
                popover.choice.presence(),
                DOT,
                Ring::Ground(Ground::Active)
            )),
            text(name(popover.choice))
                .size(typography::size::BODY)
                .style(catalog::text::secondary)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .width(Fill),
            icon(arrow, GLYPH),
        ]
        .spacing(metrics::SPACE_S)
        .align_y(Alignment::Center),
    )
    .padding(
        Padding::default()
            .left(metrics::SPACE_S)
            .right(metrics::SPACE_S),
    )
    .width(Fill)
    .height(HEIGHT)
    .on_press(Intent::ToggleStatuses)
    .style(lit)
}

/// The four statuses, the chosen one ticked.
pub fn options<'a>(popover: &'a Popover, _motion: Motion) -> impl View<Intent> + 'a {
    let rows = Choice::ALL.map(|choice| {
        let note = match choice {
            Choice::DoNotDisturb => tr!("You won't get message notifications."),
            Choice::Invisible => tr!("You'll appear offline."),
            _ => "",
        };
        let chosen = popover.choice == choice;
        hover(
            option(choice, note, chosen, false),
            option(choice, note, chosen, true),
        )
        .boxed()
    });
    column(rows).spacing(2)
}

/// One status (`StatusOption`, `ui/dialogs.slint:567`): its dot, its name
/// and its note, a check on the chosen one; the name turns `text` when
/// chosen or under the pointer (`:594`).
fn option<'a>(choice: Choice, note: &'a str, chosen: bool, lit: bool) -> Element<'a, Intent> {
    let mut words = column![
        text(name(choice))
            .size(typography::size::BODY)
            .font(if chosen {
                typography::BOLD
            } else {
                typography::UI
            })
    ]
    .spacing(2)
    .width(Fill);
    if !note.is_empty() {
        words = words.push(
            text(note)
                .size(typography::size::CAPTION)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    let mut line = row![
        container(status_dot(
            choice.presence(),
            DOT,
            Ring::Ground(Ground::Active)
        )),
        words
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if chosen {
        line = line.push(
            icon(Icon::Check, GLYPH)
                .style(|theme: &Theme, _| iced::widget::svg::Style {
                    color: Some(theme.palette().accent),
                })
                .boxed(),
        );
    }
    // At least 36 px: a row with no note is exactly that, centred; one
    // with a note is as tall as its two lines.
    let height = if note.is_empty() {
        iced::Length::Fixed(OPTION_HEIGHT)
    } else {
        iced::Length::Shrink
    };
    button(
        container(line)
            .padding([6.0, metrics::SPACE_S])
            .center_y(Fill),
    )
    .padding(0)
    .width(Fill)
    .height(height)
    .on_press(Intent::Choose(choice))
    .style(move |theme: &Theme, _| {
        let palette = theme.palette();
        button::Style {
            background: lit.then(|| palette.active.into()),
            text_color: if chosen || lit {
                palette.text
            } else {
                palette.secondary
            },
            border: Border::default().rounded(RADIUS),
            ..button::Style::default()
        }
    })
    .boxed()
}
