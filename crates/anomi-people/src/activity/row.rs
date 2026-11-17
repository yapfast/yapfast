//! One card of the panel (`ui/people.slint:197`-`:240`): 68 px high with 8 px
//! beneath it, 8 px round with a 1 px border on a wash of the overlay, 8 px
//! padding. Its words are the person (a 16 px face and the name), the game,
//! artist or show in bold, and the kind's icon with the time, green while it
//! goes on; its picture, 48 px, is at the end. A click opens the person's
//! profile and a right click their member menu.
//!
//! The card draws twice, as iced's `hover` lays one over the other: at rest
//! and under the pointer (the wash twice as strong), the second the one that
//! answers.

use iced::widget::svg;
use iced::widget::{column, container, hover, text};
use iced::{Alignment, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, catalog, typography};
use anomi_vm::kind::{Doing, Expected};
use anomi_vm::menu::Spot;
use anomi_vm::people::activity::{Intent, Row};
use anomi_widgets::avatar::circle;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::menu::opener;

use super::picture;

const HEIGHT: f32 = 68.0;
/// What each card keeps beneath it.
const GAP: f32 = 8.0;
const RADIUS: f32 = 8.0;
const PADDING: f32 = 8.0;
const FACE: f32 = 16.0;
const ICON: f32 = 16.0;
const THUMB: f32 = 48.0;
/// Spacings of the card (`ui/people.slint:211`-`:222`): the face and the
/// name, the icon and the time, the three lines, and the words and picture.
const WHO_GAP: f32 = 6.0;
const SINCE_GAP: f32 = 4.0;
const WORDS_GAP: f32 = 2.0;
const LINE_GAP: f32 = 8.0;

pub fn view<'a>(row: &'a Row, motion: Motion) -> Element<'a, Intent> {
    let face = opener(hover(
        card(row, false, Motion::default()),
        card(row, true, motion),
    ))
    .on_press(move |card| Intent::Profile(row.id.clone(), card))
    .on_right_press(move |at| Intent::Menu(row.id.clone(), Spot::pointer(at)));
    container(face)
        .padding(iced::Padding::ZERO.bottom(GAP))
        .boxed()
}

/// What an activity's icon is: a game, a song, a show, a stream or a top.
fn glyph(doing: Doing) -> Icon {
    match doing {
        Doing::Listening => Icon::Music,
        Doing::Watching => Icon::Tv,
        Doing::Streaming => Icon::Video,
        Doing::Competing => Icon::Trophy,
        Doing::Playing | Doing::Nothing => Icon::Gamepad,
    }
}

fn card<'a>(row: &'a Row, hovered: bool, motion: Motion) -> Element<'a, Intent> {
    let who = iced::widget::row![
        circle(&row.avatar, FACE),
        text(row.name.as_str())
            .size(typography::size::SMALL)
            .font(typography::MEDIUM)
            .style(catalog::text::secondary)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    ]
    .spacing(WHO_GAP)
    .align_y(Alignment::Center);
    let live = row.live;
    let ink = move |palette: &Palette| if live { palette.live } else { palette.muted };
    let since = iced::widget::row![
        icon(glyph(row.doing), ICON).style(move |theme: &Theme, _| svg::Style {
            color: Some(ink(theme.palette())),
        }),
        text(row.since.as_str())
            .size(typography::size::CAPTION)
            .style(move |theme: &Theme| text::Style {
                color: Some(ink(theme.palette())),
                ..text::Style::default()
            })
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    ]
    .spacing(SINCE_GAP)
    .align_y(Alignment::Center);
    let words = column![
        who,
        text(row.game.as_str())
            .size(typography::size::SMALL)
            .font(typography::SEMIBOLD)
            .style(catalog::text::primary)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
        since,
    ]
    .spacing(WORDS_GAP)
    .width(Fill)
    .clip(true);
    // The words start 8 px below the card's top at their own height, as
    // Slint's column does, and may run into the bottom padding.
    let mut line = iced::widget::row![
        container(words)
            .padding(iced::Padding::ZERO.top(PADDING))
            .width(Fill)
    ]
    .spacing(LINE_GAP)
    .align_y(Alignment::Center);
    if !matches!(row.thumb, Expected::None) {
        line = line.push(picture::view(&row.thumb, THUMB, THUMB / 6.0, motion));
    }
    container(line)
        .padding([0.0, PADDING])
        .width(Fill)
        .height(Length::Fixed(HEIGHT))
        .style(move |theme: &Theme| wash(theme.palette(), hovered))
        .boxed()
}

/// A light theme's line is too faint on the card's wash; its edge is the
/// step darker that keeps the card's shape.
fn wash(palette: &Palette, hovered: bool) -> container::Style {
    container::Style {
        background: Some(palette.lift(if hovered { 0.12 } else { 0.06 }).into()),
        border: iced::Border {
            color: if palette.dark {
                palette.line
            } else {
                palette.edge
            },
            width: 1.0,
            radius: RADIUS.into(),
        },
        ..container::Style::default()
    }
}
