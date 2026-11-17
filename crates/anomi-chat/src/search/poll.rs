//! The poll a found message asks (`PollView` with `interactive` off,
//! `ui/messages.slint:690`): its question and answers, with the votes when
//! they show. A click on it goes to the card, which shows the message.

use iced::border::Radius;
use iced::widget::{Column, Stack, column, container, row, space, text};
use iced::{Alignment, Border, Fill, FillPortion, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, catalog, metrics, typography};
use anomi_vm::chat::search::{Answer, Poll};
use anomi_widgets::icon::{Icon, icon};

/// The card's padding and the gap between its parts. Slint caps its width
/// at 480 px, which the 420 px panel never reaches.
const PAD: f32 = 16.0;
const GAP: f32 = 8.0;
const RADIUS: f32 = 8.0;
/// An answer's least height, its inset and the gap inside.
const ANSWER: f32 = 44.0;
const INSET_X: f32 = 12.0;
const INSET_Y: f32 = 8.0;
const INNER: f32 = 10.0;
/// The answer's emoji, its tick and its percent's width.
const EMOJI: f32 = 20.0;
const TICK: f32 = 16.0;
const PERCENT: f32 = 44.0;
/// The radio button or the box, and what's in it.
const MARK: f32 = 20.0;
const MARK_RING: f32 = 2.0;
const MARK_DOT: f32 = 8.0;
const MARK_TICK: f32 = 14.0;
const BOX_RADIUS: f32 = 5.0;
/// The question's size (`messages.slint:702`), and the card's top padding
/// (`:694`, `padding-top: 4px`).
const QUESTION: f32 = 17.0;
const ABOVE: f32 = 4.0;
/// The share of an answer that isn't the leading one: `Theme.overlay` at
/// 0.06 (`messages.slint:723`).
const SHARE_ALPHA: f32 = 0.06;

pub fn view<'a, Message: 'a>(poll: &'a Poll) -> Element<'a, Message> {
    let mut parts: Vec<Element<'a, Message>> = vec![
        text(poll.question.as_str())
            .size(QUESTION)
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .boxed(),
    ];
    if !poll.prompt.is_empty() {
        parts.push(
            text(poll.prompt.as_str())
                .size(typography::size::LABEL)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    parts.extend(poll.answers.iter().map(|answer| answered(poll, answer)));
    parts.push(footer(poll));
    let card = container(Column::with_children(parts).spacing(GAP))
        .padding(PAD)
        .width(Fill)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.sidebar.into()),
                border: Border::default()
                    .rounded(RADIUS)
                    .width(1)
                    .color(palette.line),
                ..container::Style::default()
            }
        });
    container(card)
        .padding(Padding::default().top(ABOVE))
        .boxed()
}

fn answered<'a, Message: 'a>(poll: &'a Poll, answer: &'a Answer) -> Element<'a, Message> {
    let mut line: Vec<Element<'a, Message>> = Vec::new();
    if !answer.emoji.is_empty() {
        line.push(text(answer.emoji.as_str()).size(EMOJI).boxed());
    }
    line.push(
        text(answer.text.as_str())
            .size(typography::size::BODY)
            .font(typography::SEMIBOLD)
            .style(catalog::text::primary)
            .width(Fill)
            .boxed(),
    );
    if poll.results {
        if answer.picked {
            line.push(
                icon(Icon::Check, TICK)
                    .style(|theme: &Theme, _| iced::widget::svg::Style {
                        color: Some(theme.palette().accent),
                    })
                    .boxed(),
            );
        }
        line.push(
            text(answer.votes.as_str())
                .size(typography::size::LABEL)
                .style(catalog::text::secondary)
                .boxed(),
        );
        line.push(
            text(format!("{}%", answer.percent))
                .size(typography::size::BODY)
                .font(typography::BOLD)
                .style(catalog::text::primary)
                .width(PERCENT)
                .align_x(iced::alignment::Horizontal::Right)
                .boxed(),
        );
    } else {
        line.push(mark(poll.multiselect, answer.picked));
    }
    let inner = container(
        iced::widget::Row::with_children(line)
            .spacing(INNER)
            .align_y(Alignment::Center),
    )
    .padding([INSET_Y, INSET_X])
    .width(Fill);
    // A row of its own holds the answer's least height.
    let content = row![space().width(0).height(ANSWER), inner].align_y(Alignment::Center);

    let picked = answer.picked;
    let ground = container(space().width(Fill).height(Fill))
        .width(Fill)
        .height(Fill)
        .style(move |theme: &Theme| container::Style {
            background: Some(theme.palette().raised.into()),
            border: Border::default()
                .rounded(RADIUS)
                .width(if picked { MARK_RING } else { 0.0 })
                .color(theme.palette().accent),
            ..container::Style::default()
        });
    // Raised ground, then its share, then the text: the share sits over the
    // answer's background and under its text (`messages.slint:718-724`).
    let mut stack = Stack::with_children([content.boxed()]);
    if poll.results && answer.percent > 0 {
        let share = answer.percent.min(100) as u16;
        let leading = answer.leading;
        let radius = if share >= 100 {
            Radius::from(RADIUS)
        } else {
            Radius::default().top_left(RADIUS).bottom_left(RADIUS)
        };
        let bar = container(space())
            .height(Fill)
            .width(FillPortion(share))
            .style(move |theme: &Theme| container::Style {
                background: Some(bar_colour(theme.palette(), leading).into()),
                border: Border {
                    radius,
                    ..Border::default()
                },
                ..container::Style::default()
            });
        let rest = space().width(FillPortion((100 - share).max(1)));
        stack = stack.push_under(row![bar, rest].height(Fill).width(Fill).boxed());
    }
    stack.push_under(ground.boxed()).width(Fill).boxed()
}

fn bar_colour(palette: &Palette, leading: bool) -> iced::Color {
    if leading {
        palette.accent_soft
    } else {
        palette.overlay.scale_alpha(SHARE_ALPHA)
    }
}

/// A radio button, or for several answers a box.
fn mark<'a, Message: 'a>(multiselect: bool, picked: bool) -> Element<'a, Message> {
    let inside: Option<Element<'a, Message>> = match (picked, multiselect) {
        (true, true) => Some(
            icon(Icon::Check, MARK_TICK)
                .style(|theme: &Theme, _| iced::widget::svg::Style {
                    color: Some(theme.palette().on_accent),
                })
                .boxed(),
        ),
        (true, false) => Some(
            container(space())
                .width(MARK_DOT)
                .height(MARK_DOT)
                .style(|theme: &Theme| container::Style {
                    background: Some(theme.palette().on_accent.into()),
                    border: Border::default().rounded(MARK_DOT / 2.0),
                    ..container::Style::default()
                })
                .boxed(),
        ),
        (false, _) => None,
    };
    let radius = if multiselect { BOX_RADIUS } else { MARK / 2.0 };
    container(inside)
        .center(Length::Fixed(MARK))
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: picked.then(|| palette.accent.into()),
                border: Border::default()
                    .rounded(radius)
                    .width(MARK_RING)
                    .color(if picked {
                        palette.accent
                    } else {
                        palette.muted
                    }),
                ..container::Style::default()
            }
        })
        .boxed()
}

/// The votes so far, then the time left.
fn footer<'a, Message: 'a>(poll: &'a Poll) -> Element<'a, Message> {
    let mut line = row![
        text(poll.total.as_str())
            .size(typography::size::SMALL)
            .style(catalog::text::secondary)
            .boxed()
    ]
    .spacing(GAP);
    if !poll.left.is_empty() {
        line = line.push(
            text(format!("\u{00b7}  {}", poll.left))
                .size(typography::size::SMALL)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    column![line]
        .padding(Padding::default().top(metrics::SPACE_XS))
        .boxed()
}
