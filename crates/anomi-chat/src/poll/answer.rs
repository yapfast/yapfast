//! One answer of a poll's card (`PollView`'s `for answer`,
//! `ui/messages.slint:716`): its emoji and words, then a radio or a check
//! while it takes votes, or its votes and share, over a bar of that share,
//! once results show.

use iced::widget::{button, container, row, space, stack, text};
use iced::{Alignment, Background, Border, Fill, Length, border};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::chat::poll::{Answer, Intent, Poll};
use anomi_vm::icon::Icon;
use anomi_widgets::icon::icon;

use super::emoji::face;

/// The row's least height and its side padding (`min-height: 44px`,
/// `padding: 8px 12px`, `spacing: 10px`).
const HEIGHT: f32 = 44.0;
const SIDE: f32 = 12.0;
const GAP: f32 = 10.0;
const VERTICAL: f32 = 8.0;
const EMOJI: f32 = 20.0;
/// A server's emoji, which Slint draws as a glyph of the emoji font: as
/// large as one, and in the room one takes, so the words line up.
const PICTURE: f32 = 24.0;
const PICTURE_ROOM: f32 = 27.0;
const NAME: f32 = typography::size::BODY;
/// The ring of a picked answer.
const RING: f32 = 2.0;
/// The check of a voted one, the percent's room (`min-width: 44px`).
const CHECK: f32 = 16.0;
const PERCENT: f32 = 44.0;
/// The radio and the checkbox (20 px, 2 px rim, 8 px dot, 14 px check).
const MARK: f32 = 20.0;
const MARK_RIM: f32 = 2.0;
const DOT: f32 = 8.0;
const MARK_CHECK: f32 = 14.0;
/// The faint fill of a share that doesn't lead (`overlay` at 6 %).
const SHARE_ALPHA: f32 = 0.06;

pub(super) fn view<'a>(poll: &'a Poll, answer: &'a Answer) -> Element<'a, Intent> {
    let clickable = poll.interactive && (poll.results || poll.can_pick);
    let mut line: Vec<Element<'a, Intent>> = Vec::with_capacity(5);
    if let Some(emoji) = &answer.emoji {
        line.push(if emoji.glyph.is_empty() {
            container(face(emoji, PICTURE)).width(PICTURE_ROOM).boxed()
        } else {
            face(emoji, EMOJI)
        });
    }
    line.push(
        container(
            text(answer.text.as_str())
                .size(NAME)
                .font(typography::SEMIBOLD)
                .wrapping(text::Wrapping::Word)
                .style(anomi_theme::catalog::text::primary),
        )
        .padding([VERTICAL, 0.0])
        .width(Fill)
        .boxed(),
    );
    if poll.results {
        if answer.picked {
            line.push(
                icon(Icon::Check, CHECK)
                    .style(|theme: &Theme, _| iced::widget::svg::Style {
                        color: Some(theme.palette().accent),
                    })
                    .boxed(),
            );
        }
        line.push(
            text(answer.votes.as_str())
                .size(typography::size::LABEL)
                .wrapping(text::Wrapping::None)
                .style(anomi_theme::catalog::text::secondary)
                .boxed(),
        );
        line.push(
            text(format!("{}%", answer.percent))
                .size(NAME)
                .font(typography::BOLD)
                .width(PERCENT)
                .align_x(iced::alignment::Horizontal::Right)
                .wrapping(text::Wrapping::None)
                .style(anomi_theme::catalog::text::primary)
                .boxed(),
        );
    } else {
        line.push(mark(answer.picked, poll.multiselect));
    }
    // The least height beside the line, outside its spacing.
    let line = row![
        space().height(HEIGHT),
        row(line)
            .spacing(GAP)
            .align_y(Alignment::Center)
            .width(Fill)
    ]
    .align_y(Alignment::Center)
    .padding([0.0, SIDE]);
    let body: Element<'a, Intent> = if poll.results && answer.percent > 0 {
        // Under the line and as tall as it: a bar of `Fill` height on its
        // own would take all the height a list offers.
        stack![line.width(Fill)]
            .push_under(bar(answer.percent, answer.leading))
            .width(Fill)
            .boxed()
    } else {
        line.width(Fill).boxed()
    };
    let picked = answer.picked;
    let id = answer.id;
    let key = poll.key;
    let results = poll.results;
    button(body)
        .width(Fill)
        .padding(0)
        .on_press_maybe(clickable.then_some(if results {
            Intent::Voters(key, Some(id))
        } else {
            Intent::Pick(key, id)
        }))
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let lit =
                clickable && matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(if lit {
                    palette.active
                } else {
                    palette.raised
                })),
                text_color: palette.text,
                border: Border::default()
                    .rounded(metrics::RADIUS_CARD)
                    .width(if picked { RING } else { 0.0 })
                    .color(palette.accent),
                ..button::Style::default()
            }
        })
        .boxed()
}

/// The share's bar from the left edge: its width the percent of the row's.
fn bar<'a>(percent: u8, leading: bool) -> Element<'a, Intent> {
    let percent = percent.min(100);
    let right = if percent == 100 {
        metrics::RADIUS_CARD
    } else {
        0.0
    };
    let fill = container(space())
        .width(Length::FillPortion(u16::from(percent)))
        .height(Fill)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(if leading {
                    palette.accent_soft
                } else {
                    palette.lift(SHARE_ALPHA)
                })),
                border: Border {
                    radius: border::Radius {
                        top_left: metrics::RADIUS_CARD,
                        top_right: right,
                        bottom_right: right,
                        bottom_left: metrics::RADIUS_CARD,
                    },
                    ..Border::default()
                },
                ..container::Style::default()
            }
        });
    if percent == 100 {
        fill.boxed()
    } else {
        row![
            fill,
            space().width(Length::FillPortion(u16::from(100 - percent)))
        ]
        .height(Fill)
        .boxed()
    }
}

/// A radio button, or for several answers a checkbox.
fn mark<'a>(picked: bool, multiselect: bool) -> Element<'a, Intent> {
    let radius = if multiselect {
        metrics::RADIUS_MARK
    } else {
        MARK / 2.0
    };
    let inside: Element<'a, Intent> = match (picked, multiselect) {
        (false, _) => space().boxed(),
        (true, true) => icon(Icon::Check, MARK_CHECK)
            .style(|theme: &Theme, _| iced::widget::svg::Style {
                color: Some(theme.palette().on_accent),
            })
            .boxed(),
        (true, false) => container(space())
            .width(DOT)
            .height(DOT)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().on_accent)),
                border: Border::default().rounded(DOT / 2.0),
                ..container::Style::default()
            })
            .boxed(),
    };
    container(inside)
        .center(MARK)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: picked.then_some(Background::Color(palette.accent)),
                border: Border::default()
                    .rounded(radius)
                    .width(MARK_RIM)
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
