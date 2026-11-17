//! The soundboard's card: its title and note, and its list of rows.

use iced::widget::text::LineHeight;
use iced::widget::{column, container, row, text};
use iced::{Border, Color, Length, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::voice::soundboard::{Intent, Row, Soundboard};
use anomi_widgets::list::list;
use yapfast_lang::tr;

use super::chip::{self, chip};

pub const WIDTH: f32 = 372.0;
pub const HEIGHT_MAX: f32 = 440.0;
/// A row's height, a heading's or a line of three sounds'.
const ROW: f32 = 42.0;
const INSET: f32 = 12.0;
const GAP: f32 = 6.0;
const RADIUS: f32 = 10.0;
/// The card's shadow (`drop-shadow-blur: 24px; offset-y: 8px; #00000080`).
const SHADOW_DROP: f32 = 8.0;
const SHADOW_BLUR: f32 = 24.0;
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.5);
/// What sits under the last row.
const FOOT: f32 = 10.0;
/// The title block's padding and spacing (`ui/voice.slint:1735`).
const TITLE_TOP: f32 = 14.0;
const TITLE_BOTTOM: f32 = 4.0;
const TITLE_GAP: f32 = 4.0;
/// The title's line, at 16 px, and the note's, at 13 px.
const TITLE_LINE: f32 = 21.0;
const NOTE_LINE: f32 = 17.0;
/// What a heading leaves under its text (`row-height - 8px`, `:1749`).
const HEADING_FOOT: f32 = 8.0;
/// A sound row's padding above and below (`:1752`).
const SOUNDS_PAD: f32 = 3.0;

/// The card, as tall as its title, note and rows, up to [`HEIGHT_MAX`].
pub fn card(soundboard: &Soundboard, motion: Motion) -> impl View<Intent> + '_ {
    let title: &str = if soundboard.title.is_empty() {
        tr!("Soundboard")
    } else {
        &soundboard.title
    };
    let mut top = column![
        text(title)
            .size(typography::size::TITLE)
            .font(typography::BOLD)
            .line_height(LineHeight::Absolute(TITLE_LINE.into()))
            .wrapping(iced::widget::text::Wrapping::None)
            .ellipsis(iced::widget::text::Ellipsis::End)
            .style(catalog::text::primary)
            .boxed()
    ]
    .spacing(TITLE_GAP);
    // What sits above the list, at least: the title's line and its padding.
    let mut above = TITLE_TOP + TITLE_LINE + TITLE_BOTTOM;
    if !soundboard.note.is_empty() {
        top = top.push(
            text(soundboard.note.as_str())
                .size(typography::size::LABEL)
                .line_height(LineHeight::Absolute(NOTE_LINE.into()))
                .style(catalog::text::muted)
                .boxed(),
        );
        above += TITLE_GAP + NOTE_LINE;
    }
    if soundboard.loading() {
        top = top.push(
            text(tr!("Loading sounds..."))
                .size(typography::size::LABEL)
                .line_height(LineHeight::Absolute(NOTE_LINE.into()))
                .style(catalog::text::muted)
                .boxed(),
        );
        above += TITLE_GAP + NOTE_LINE;
    }
    let top = container(top).padding(Padding {
        top: TITLE_TOP,
        right: INSET,
        bottom: TITLE_BOTTOM,
        left: INSET,
    });

    let room = soundboard.rows.len().max(1) as f32 * ROW + FOOT;
    let overflows = room + above > HEIGHT_MAX;
    let playable = soundboard.playable();
    let extras = !soundboard.chooser;
    let rows = &soundboard.rows;
    let keys = &soundboard.keys;
    let body = list(
        rows.len(),
        move |index| keys[index],
        move |index| line(&rows[index], playable, extras, motion),
        motion,
    )
    .height(if overflows {
        Length::Fill
    } else {
        Length::Fixed(room)
    });

    let card = container(column![top, body]).width(Length::Fixed(WIDTH));
    let card = if overflows {
        card.height(Length::Fixed(HEIGHT_MAX))
    } else {
        card
    };
    card.clip(true).style(|theme: &Theme| {
        let mut style = catalog::container::raised(theme);
        style.border = Border {
            radius: RADIUS.into(),
            ..style.border
        };
        style.shadow = Shadow {
            color: SHADOW,
            offset: Vector::new(0.0, SHADOW_DROP),
            blur_radius: SHADOW_BLUR,
        };
        style
    })
}

/// A row of the list: a heading at the foot of its room, or its sounds.
fn line<'a>(row_: &'a Row, playable: bool, extras: bool, motion: Motion) -> impl View<Intent> + 'a {
    match row_ {
        Row::Heading(heading) => container(
            text(heading.as_str())
                .size(typography::size::CAPTION)
                .font(typography::BOLD)
                .wrapping(iced::widget::text::Wrapping::None)
                .ellipsis(iced::widget::text::Ellipsis::End)
                .style(catalog::text::muted),
        )
        .padding(Padding {
            left: INSET,
            right: INSET,
            bottom: HEADING_FOOT,
            ..Padding::ZERO
        })
        .height(ROW)
        .width(Length::Fill)
        .align_y(iced::alignment::Vertical::Bottom)
        .boxed(),
        Row::Sounds(sounds) => container(
            row(sounds
                .iter()
                .map(|sound| chip(sound, playable, extras, motion)))
            .spacing(GAP),
        )
        .padding(Padding {
            left: INSET,
            right: INSET,
            top: SOUNDS_PAD,
            bottom: SOUNDS_PAD,
        })
        .height(ROW)
        .width(Length::Fill)
        .boxed(),
    }
}

const _: () = assert!(chip::HEIGHT + 2.0 * SOUNDS_PAD <= ROW);
