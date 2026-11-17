//! A card's words, beside its picture (`ui/people.slint:828`-`:884`): the
//! name in bold (a song's opens its page, in the link colour under the
//! pointer), the details and the state in the second ink, then how long it
//! has run or how long is left, or for a song with its start and end a bar
//! with how far it has got and how long it is either side.

use iced::widget::{Column, Row, button, container, row, space, stack, text};
use iced::{Alignment, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::people::activity::card::{Card, Intent, Look, clock};
use anomi_widgets::presence::status_text;
use yapfast_lang::tr;

use super::Inks;

/// Between the name, the details, the state and the time
/// (`ui/people.slint:830`).
const WORDS_GAP: f32 = 2.0;
/// Between the played time, the bar and the length (`:861`).
const BAR_GAP: f32 = 8.0;
const BAR: f32 = 3.0;
const LINE: f32 = 16.0;
/// What the bar's fill is cut into, so its portions stay whole numbers.
const PARTS: u16 = 1000;

pub fn view<'a>(card: &'a Card, now: i64, look: &Look, motion: Motion) -> Element<'a, Intent> {
    let look = *look;
    let mut words = Column::<Element<'a, Intent>>::new()
        .spacing(WORDS_GAP)
        .width(Fill);
    words = words.push(name(card, look, motion));
    for line in [&card.details, &card.state] {
        if !line.is_empty() {
            words = words.push(
                text(line.as_str())
                    .size(typography::size::LABEL)
                    .style(move |theme: &Theme| ink(&look, theme, |inks| inks.ink2))
                    .wrapping(text::Wrapping::None)
                    .ellipsis(text::Ellipsis::End)
                    .boxed(),
            );
        }
    }
    if card.bar() {
        words = words.push(bar(card, now, look));
    } else if card.timed() {
        let seconds = if card.start > 0 {
            tr!("{} elapsed", clock(now - card.start))
        } else {
            tr!("{} left", clock(card.end - now))
        };
        words = words.push(
            container(
                text(seconds)
                    .size(typography::size::CAPTION)
                    .style(move |theme: &Theme| ink(&look, theme, |inks| inks.ink3))
                    .wrapping(text::Wrapping::None)
                    .ellipsis(text::Ellipsis::End),
            )
            .height(LINE)
            .center_y(LINE)
            .boxed(),
        );
    }
    container(words).width(Fill).clip(true).boxed()
}

fn ink(look: &Look, theme: &Theme, pick: fn(Inks) -> iced::Color) -> text::Style {
    text::Style {
        color: Some(pick(Inks::new(look, theme.palette()))),
        ..text::Style::default()
    }
}

/// The name: 14 px bold in the first ink; a card with a link opens it.
fn name<'a>(card: &'a Card, look: Look, motion: Motion) -> Element<'a, Intent> {
    if card.link.is_empty() {
        let words = status_text(&card.name, motion)
            .size(typography::size::SMALL)
            .font(typography::BOLD)
            .color(|palette| palette.text);
        return match look.ink {
            Some(color) => words.ink(color),
            None => words,
        }
        .view()
        .boxed();
    }
    let title = move |theme: &Theme, status: button::Status| {
        let palette = theme.palette();
        let lit = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            text_color: if lit {
                palette.link
            } else {
                Inks::new(&look, palette).ink
            },
            ..button::Style::default()
        }
    };
    let link = card.link.clone();
    row![
        button(
            text(card.name.text.as_str())
                .size(typography::size::SMALL)
                .font(typography::BOLD)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
        )
        .padding(0)
        .style(title)
        .on_press(Intent::Link(link))
    ]
    .boxed()
}

/// How far a song has got, its length either side of the bar.
fn bar<'a>(card: &Card, now: i64, look: Look) -> Element<'a, Intent> {
    let length = clock(card.length());
    let side = |value: String, pick: fn(Inks) -> iced::Color| {
        text(value)
            .size(typography::size::CAPTION)
            .style(move |theme: &Theme| ink(&look, theme, pick))
            .wrapping(text::Wrapping::None)
    };
    let done = (card.progress(now) * f32::from(PARTS)).round() as u16;
    let filled = container(space().height(BAR)).style(move |theme: &Theme| container::Style {
        background: Some(Inks::new(&look, theme.palette()).ink.into()),
        border: iced::Border::default().rounded(BAR / 2.0),
        ..container::Style::default()
    });
    let mut fill = Row::<Element<'a, Intent>>::new();
    if done > 0 {
        fill = fill.push(filled.width(Length::FillPortion(done)).boxed());
    }
    if done < PARTS {
        fill = fill.push(space().width(Length::FillPortion(PARTS - done)).boxed());
    }
    let track = container(fill.height(BAR))
        .width(Fill)
        .style(move |theme: &Theme| container::Style {
            background: Some(Inks::new(&look, theme.palette()).track.into()),
            border: iced::Border::default().rounded(BAR / 2.0),
            ..container::Style::default()
        });
    row![
        // As wide as the length's text, as Slint's `song-length` is, so the
        // bar doesn't move as the digits change.
        stack![
            side(length.clone(), |_| iced::Color::TRANSPARENT),
            side(clock(card.played(now)), |inks| inks.ink2),
        ],
        container(track).center_y(LINE).width(Fill),
        side(length, |inks| inks.ink2),
    ]
    .spacing(BAR_GAP)
    .height(LINE)
    .align_y(Alignment::Center)
    .boxed()
}
