//! One line of the list: a section's heading, or a thread as a card
//! (`ThreadEntry`, `ui/header.slint:319`).

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Color, Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::threads::{Intent, Person, Row, Thread};
use anomi_vm::icon::Icon;
use anomi_widgets::avatar::circle;
use anomi_widgets::icon::icon;

use super::card::card;
use super::ends::ends;
use super::ring::{Lit, ring};

/// A heading is 24 px of text 8 px under the top of the first section's
/// 32 px and 16 px under the later ones' 40.
const HEADING_TEXT: f32 = 24.0;
const FIRST_HEADING: f32 = 32.0;
const LATER_HEADING: f32 = 40.0;
/// A card sits 16 px in from each side, 8 px above the next, and 16 px
/// under the header when nothing heads it.
const SIDE: f32 = metrics::SPACE_L;
const GAP: f32 = metrics::SPACE_S;
const FIRST_CARD: f32 = metrics::SPACE_L;
/// A heading's text sits 8 px under the top of the first, 16 px under a
/// later one's.
const FIRST_TOP: f32 = metrics::SPACE_S;
const LATER_TOP: f32 = metrics::SPACE_L;
/// The gap in a card's rows (name over line, and the line's parts), and
/// between its left and its members (`ui/header.slint:319-397`).
const LINE_GAP: f32 = 6.0;
const MEMBERS_GAP: f32 = metrics::SPACE_L;
/// A person's picture on the line, with the gap after it.
const PERSON_GAP: f32 = 6.0;
/// The ring round a member's avatar, and the room past the last one.
const RING: f32 = 1.0;
const STACK_END: f32 = 8.0;
/// DM Sans' natural line height, as Slint lays its text out; iced's default
/// of 1.375 puts the name 2 px high.
const LINE: f32 = 1.3;
/// What a card's line has: its avatars and glyphs.
const PERSON: f32 = 16.0;
const GLYPH: f32 = 14.0;
const MEMBER: f32 = 20.0;
const OVERLAP: f32 = 12.0;

/// Row `index` of the list.
pub fn row_at<'a>(rows: &'a [Row], index: usize, motion: Motion) -> Element<'a, Intent> {
    match &rows[index] {
        Row::Heading(heading) => heading_row(heading, index == 0),
        Row::Thread(thread) => thread_row(thread, index == 0, motion),
    }
}

fn heading_row(heading: &str, first: bool) -> Element<'_, Intent> {
    let (height, top) = if first {
        (FIRST_HEADING, FIRST_TOP)
    } else {
        (LATER_HEADING, LATER_TOP)
    };
    container(
        text(heading)
            .size(typography::size::CAPTION)
            .font(typography::BOLD)
            .style(catalog::text::muted)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .height(HEADING_TEXT)
            .align_y(Alignment::Center),
    )
    .padding(Padding::from([top, SIDE]).bottom(0.0))
    .width(Fill)
    .height(height)
    .boxed()
}

fn thread_row(thread: &Thread, first: bool, motion: Motion) -> Element<'_, Intent> {
    let room = &thread.room;
    let lit = Lit::default();
    let card = card(
        container(content(thread, &lit))
            .padding([0.0, SIDE])
            .width(Fill)
            .height(Fill),
        move |beside| Intent::Open {
            room: room.clone(),
            beside,
        },
        motion,
        lit.clone(),
    );
    let top = if first { FIRST_CARD } else { 0.0 };
    container(card)
        .padding(Padding {
            top,
            right: SIDE,
            bottom: GAP,
            left: SIDE,
        })
        .width(Fill)
        .boxed()
}

/// The card's inside: the name over the line on the left, who is in it on
/// the right.
fn content<'a>(thread: &'a Thread, lit: &Lit) -> Element<'a, Intent> {
    let name = row![
        thread.private.then(|| muted_icon(Icon::Lock)),
        text(&thread.name)
            .size(typography::size::BODY)
            .font(typography::SEMIBOLD)
            .style(catalog::text::primary)
            .line_height(LINE)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .width(Fill),
    ]
    .spacing(LINE_GAP)
    .align_y(Alignment::Center);
    let left = column![name, line(thread)]
        .spacing(LINE_GAP)
        .width(Fill)
        .align_x(Alignment::Start);
    let left = container(left).center_y(Fill).width(Fill);
    let mut parts = vec![left.boxed()];
    parts.extend(members(thread, lit));
    row(parts)
        .spacing(MEMBERS_GAP)
        .align_y(Alignment::Center)
        .boxed()
}

/// The second line: who, what they said and when; the time never cut.
fn line(thread: &Thread) -> Element<'_, Intent> {
    let mut parts: Vec<Element<'_, Intent>> = Vec::new();
    match &thread.person {
        Some(person) => parts.push(
            row![circle(&person.avatar, PERSON), space().width(PERSON_GAP)]
                .align_y(Alignment::Center)
                .boxed(),
        ),
        None if thread.archived => parts.push(
            row![muted_icon(Icon::Thread), space().width(PERSON_GAP)]
                .align_y(Alignment::Center)
                .boxed(),
        ),
        None => {}
    }
    if !thread.lead.is_empty() {
        parts.push(small(&thread.lead).boxed());
    }
    if let Some(person) = &thread.person {
        parts.push(named(person));
    }
    if !thread.text.is_empty() {
        if thread.person.is_some() {
            parts.push(small(": ").boxed());
        }
        parts.push(small(&thread.text).boxed());
    }
    let body = row(parts).align_y(Alignment::Center);
    let spaced = thread.person.is_some() || !thread.text.is_empty();
    let time: Element<'_, Intent> = if spaced {
        row![small(" • "), small(&thread.time)].boxed()
    } else {
        small(&thread.time).boxed()
    };
    container(ends(body, time)).width(Fill).boxed()
}

fn small<'a>(words: impl text::IntoFragment<'a>) -> text::Text<'a, Theme> {
    text(words)
        .size(typography::size::LABEL)
        .style(catalog::text::muted)
        .line_height(LINE)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
}

/// The person's name in their role's colour, or the secondary ink.
fn named(person: &Person) -> Element<'_, Intent> {
    let color: Option<Color> = person.color;
    text(&person.name)
        .size(typography::size::LABEL)
        .font(typography::SEMIBOLD)
        .line_height(LINE)
        .style(move |theme: &Theme| text::Style {
            color: Some(color.unwrap_or(theme.palette().secondary)),
            ..text::Style::default()
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .boxed()
}

fn muted_icon<'a>(glyph: Icon) -> Element<'a, Intent> {
    icon(glyph, GLYPH)
        .style(|theme: &Theme, _status| iced::widget::svg::Style {
            color: Some(theme.palette().muted),
        })
        .boxed()
}

/// A few of those in it, their pictures overlapping, and how many.
fn members<'a>(thread: &'a Thread, lit: &Lit) -> Option<Element<'a, Intent>> {
    if thread.members == 0 {
        return None;
    }
    let count = text(thread.members.to_string())
        .size(typography::size::LABEL)
        .font(typography::SEMIBOLD)
        .style(catalog::text::muted);
    if thread.member_avatars.is_empty() {
        return Some(count.boxed());
    }
    let stack = iced::widget::stack(thread.member_avatars.iter().enumerate().map(
        |(at, avatar)| {
            let ring = iced::widget::stack![
                ring(MEMBER, lit.clone()),
                container(circle(avatar, MEMBER - 2.0 * RING)).padding(RING),
            ];
            container(ring)
                .padding(Padding::ZERO.left(at as f32 * OVERLAP))
                .boxed()
        },
    ))
    .width(Length::Fixed(
        OVERLAP * thread.member_avatars.len() as f32 + STACK_END,
    ))
    .height(MEMBER);
    Some(
        row![stack, count]
            .spacing(LINE_GAP)
            .align_y(Alignment::Center)
            .boxed(),
    )
}
