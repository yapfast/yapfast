//! The marks and rows of a radio group, a checkbox group and a checkbox:
//! Slint's `Mark`, `MarkRow`, `RadioRow` and `CheckRow`
//! (`ui/interactions.slint:116`, `:128`, `:152`, `:153`). A round or a
//! square 20 px mark with a 2 px ring that fills with the accent when on (a
//! dot in the round, a check in the square), beside a label and a muted
//! description, the whole row a press with the hover fill.

use iced::widget::text::IntoFragment;
use iced::widget::{button, column, container, row, space, svg, text};
use iced::{Background, Border, Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_widgets::icon::{Icon, icon};

/// The check in a square mark (`width: 14px`).
const CHECK: f32 = 14.0;
/// The dot in a round mark (`width: 10px`).
const DOT: f32 = 10.0;
/// A row's padding and the gap between mark and words (`padding: 6px`,
/// `spacing: 10px`); the mark sits a pixel below the row's top.
const PADDING: f32 = 6.0;
/// The mark's drop below the first line (`padding-top: 1px`,
/// `ui/interactions.slint:142`).
const MARK_DROP: f32 = 1.0;
const GAP: f32 = 10.0;
/// The rows of a group (`spacing: 2px`, `interactions.slint:233`, `:243`).
const ROWS_GAP: f32 = 2.0;
const LABEL: f32 = typography::size::BODY;
const DETAIL: f32 = typography::size::LABEL;

/// The mark: `on` fills it, `round` makes it a radio's.
pub fn mark<'a, M: 'a>(on: bool, round: bool) -> Element<'a, M> {
    let inner: Element<'a, M> = match (on, round) {
        (true, true) => container(space())
            .width(DOT)
            .height(DOT)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().accent)),
                border: Border::default().rounded(DOT / 2.0),
                ..container::Style::default()
            })
            .boxed(),
        (true, false) => icon(Icon::Check, CHECK)
            .style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().on_accent),
            })
            .boxed(),
        (false, _) => space().boxed(),
    };
    let radius = if round {
        metrics::MARK_SIZE / 2.0
    } else {
        metrics::RADIUS_MARK
    };
    container(inner)
        .center(Length::Fixed(metrics::MARK_SIZE))
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: (on && !round).then_some(Background::Color(palette.accent)),
                border: Border::default()
                    .rounded(radius)
                    .width(metrics::MARK_RING)
                    .color(if on {
                        palette.accent
                    } else {
                        palette.secondary
                    }),
                ..container::Style::default()
            }
        })
        .boxed()
}

/// A row: the mark, the label and its description under it, sending
/// `message` when pressed; without one (`enabled` false) it is inert.
pub fn mark_row<'a, M: Clone + 'a>(
    label: impl IntoFragment<'a>,
    description: &'a str,
    on: bool,
    round: bool,
    message: Option<M>,
) -> Element<'a, M> {
    let mut words = column![
        text(label)
            .size(LABEL)
            .style(catalog::text::primary)
            .width(Fill)
            .boxed()
    ];
    if !description.is_empty() {
        words = words.push(
            text(description)
                .size(DETAIL)
                .style(catalog::text::secondary)
                .width(Fill)
                .boxed(),
        );
    }
    let content = row![
        container(mark(on, round))
            .padding(Padding::ZERO.top(MARK_DROP))
            .boxed(),
        words.boxed()
    ]
    .spacing(GAP);
    button(content)
        .padding(PADDING)
        .width(Fill)
        .style(catalog::button::plain)
        .on_press_maybe(message)
        .boxed()
}

/// A radio group's or a checkbox group's options, as rows.
pub fn group<'a, M: Clone + 'a>(
    choices: &'a [anomi_vm::chat::bot_forms::Choice],
    round: bool,
    enabled: bool,
    pick: impl Fn(&str) -> M,
) -> Element<'a, M> {
    column(choices.iter().map(|choice| {
        mark_row(
            &choice.label,
            &choice.description,
            choice.selected,
            round,
            enabled.then(|| pick(&choice.value)),
        )
    }))
    .spacing(ROWS_GAP)
    .boxed()
}
