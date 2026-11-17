//! One row of the timeline: a message with what stands over it. The Slint
//! side: the row of the timeline's `ListView` (`ui/yapfast.slint:3725`-
//! `:3950`): its `col` with its padding, the history's edge over the first
//! row, a new day's date and the "New" divider in one line, the message
//! (`MessageContent`, C02's `message::view`), and under the last row of a
//! window of older messages the edge that loads newer ones. Its tints, a
//! jump's and the keyboard's, and the keyboard's edge lie under the message
//! (`body`, `:3802`-`:3826`): from the row's top edge, its space included,
//! when nothing stands over the message, and from just under the history's
//! edge or the divider otherwise, down to the row's foot with its 1 px.

use iced::advanced::widget::Id;
use iced::font::Weight;
use iced::widget::{Column, column, container, row, space, text};
use iced::{Alignment, Background, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::{self, timeline::Row, timeline::Timeline};
use yapfast_lang::tr;

use super::edge;
use super::glow::{Ground, Tint, glow};
use crate::message;

/// The keyboard's message, its tinted ground, which its menu and the
/// reaction picker open beside: the keys find it by this.
pub const ACTIVE: Id = Id::new("timeline-active");

/// Space over a row: one with a date or the "New" divider, the first of a
/// group (`ClientStyle.message-group-gap`, compact or not), any other
/// (`padding-top`, `:3855`, `ui/yapfast/style.slint:33`).
const DIVIDED: f32 = 12.0;
const GROUP_GAP: f32 = 16.0;
const COMPACT_GROUP_GAP: f32 = 8.0;
const GAP: f32 = 1.0;
/// The line of a date and the "New" divider: 26 px with 6 under it, 16 px
/// in from the sides, its parts 10 px apart (`:3871`-`:3888`).
const DIVIDER_HEIGHT: f32 = 20.0;
const DIVIDER_UNDER: f32 = 6.0;
const DIVIDER_INSET: f32 = 16.0;
const DIVIDER_GAP: f32 = 10.0;
const RULE_TOP: f32 = 10.0;
/// The "New" pill: 18 px, 4 px corners, its text 5 px in from each end,
/// 12 px extra bold; the dot between it and a date, 13 px bold.
const NEW_HEIGHT: f32 = 18.0;
const NEW_RADIUS: f32 = 4.0;
const NEW_PAD: f32 = 5.0;
const NEW_WEIGHT: Weight = Weight::ExtraBold;
const DOT_SIZE: f32 = 13.0;

/// The row at `index` of `timeline`.
pub fn view(timeline: &Timeline, index: usize, motion: Motion) -> Element<'_, chat::Intent> {
    let row = &timeline.rows[index];
    let last = index + 1 == timeline.rows.len();
    let divided = row.day.is_some() || row.new;
    let mut parts: Column<Element<'_, chat::Intent>> = column![].width(Fill);
    let mut headed = divided;
    if index == 0
        && timeline.ready
        && let Some(room) = &timeline.room
    {
        parts = parts.push(edge::earlier(timeline, room, motion));
        headed = true;
    }
    if divided {
        parts = parts.push(divider(row));
    }
    let top = if divided {
        DIVIDED
    } else if row.first && timeline.compact {
        COMPACT_GROUP_GAP
    } else if row.first {
        GROUP_GAP
    } else {
        GAP
    };
    // The last row leaves the room at the end to the list's own padding.
    let bottom = if last { 0.0 } else { GAP };
    // The space over the row lies on the message's ground, unless the
    // history's edge or the divider stands between them.
    let (over, under) = if headed { (top, 0.0) } else { (0.0, top) };
    parts = parts.push(body(
        timeline,
        row,
        Padding::ZERO.top(under).bottom(bottom),
        motion,
    ));
    if last
        && timeline.ready
        && let Some(later) = timeline.later
    {
        parts = parts.push(edge::later(later, timeline.locked, motion));
    }
    container(parts)
        .width(Fill)
        .padding(Padding::ZERO.top(over))
        .boxed()
}

/// The message itself, `padding` in on its ground: lit while the keyboard
/// is on it, with a marigold edge, and accent for a moment after a jump to
/// it.
fn body<'a>(
    timeline: &'a Timeline,
    row: &'a Row,
    padding: Padding,
    motion: Motion,
) -> Element<'a, chat::Intent> {
    let keys = &timeline.keys;
    let active = keys.active == Some(row.key);
    let keyed = active && keys.browsing;
    let jumped = timeline
        .jump
        .is_some_and(|jump| jump.key == row.key && jump.lit);
    let tint = if jumped {
        Tint::Jumped
    } else if keyed {
        Tint::Lit
    } else {
        Tint::None
    };
    let end = if timeline.aside {
        message::ASIDE_END
    } else {
        message::END
    };
    let mut message = container(message::ending(&row.message, end, motion))
        .width(Fill)
        .padding(padding);
    if active {
        message = message.id(ACTIVE);
    }
    glow(
        message,
        Ground {
            tint,
            edge: keyed,
            stepping: keys.browsing,
        },
        motion,
    )
    .boxed()
}

/// A new day's date and the "New" divider, in the middle of one line
/// between two rules, accent when it says "New".
fn divider(marks: &Row) -> Element<'_, chat::Intent> {
    let new = marks.new;
    let mut parts = row![rule(new)]
        .spacing(DIVIDER_GAP)
        .align_y(Alignment::Center);
    if let Some(day) = &marks.day {
        parts = parts.push(
            text(day.as_str())
                .size(typography::size::CAPTION)
                .font(typography::SEMIBOLD)
                .style(catalog::text::muted)
                .boxed(),
        );
        if new {
            parts = parts.push(
                text("·")
                    .size(DOT_SIZE)
                    .font(typography::BOLD)
                    .style(catalog::text::muted)
                    .boxed(),
            );
        }
    }
    if new {
        parts = parts.push(new_pill());
    }
    container(parts.push(rule(new)).height(DIVIDER_HEIGHT))
        .padding(
            Padding::ZERO
                .left(DIVIDER_INSET)
                .right(DIVIDER_INSET)
                .bottom(DIVIDER_UNDER),
        )
        .width(Fill)
        .boxed()
}

/// One of the divider's rules: the line colour, or the accent beside "New",
/// on a whole pixel 10 px down its line, where Slint's centring rounds it.
fn rule<'a>(new: bool) -> Element<'a, chat::Intent> {
    let line = container(space().width(Fill).height(1))
        .width(Fill)
        .height(1)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(if new {
                    palette.accent
                } else {
                    palette.line
                })),
                ..container::Style::default()
            }
        });
    container(line)
        .width(Fill)
        .height(DIVIDER_HEIGHT)
        .padding(Padding::ZERO.top(RULE_TOP))
        .boxed()
}

/// "New" on the accent, where the unread messages start
/// (`accessible-label: "New messages start here"`).
fn new_pill<'a>() -> Element<'a, chat::Intent> {
    container(
        text(tr!("New"))
            .size(typography::size::CAPTION)
            .font(typography::weight(NEW_WEIGHT))
            .style(|theme: &Theme| text::Style {
                color: Some(theme.palette().on_accent),
                ..text::Style::default()
            }),
    )
    .padding(Padding::ZERO.left(NEW_PAD).right(NEW_PAD))
    .center_y(NEW_HEIGHT)
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().accent)),
        border: Border::default().rounded(NEW_RADIUS),
        ..container::Style::default()
    })
    .boxed()
}
