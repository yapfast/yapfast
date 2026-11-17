//! The soundboard's card: its title and note, and its list of rows.

use std::hash::{Hash, Hasher};

use iced::widget::{column, container, row, text};
use iced::{Border, Length, Padding};

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
/// What sits under the last row.
const FOOT: f32 = 10.0;

/// The card, as tall as its title, note and rows, up to [`HEIGHT_MAX`].
pub fn card(soundboard: &Soundboard, motion: Motion) -> impl View<Intent> + '_ {
    let title = if soundboard.title.is_empty() {
        tr!("Soundboard").to_string()
    } else {
        soundboard.title.clone()
    };
    let mut top = column![
        text(title)
            .size(typography::size::TITLE)
            .font(typography::BOLD)
            .wrapping(iced::widget::text::Wrapping::None)
            .ellipsis(iced::widget::text::Ellipsis::End)
            .style(catalog::text::primary)
            .boxed()
    ]
    .spacing(4);
    let mut above = 14.0 + 21.0 + 4.0;
    if !soundboard.note.is_empty() {
        top = top.push(
            text(soundboard.note.as_str())
                .size(typography::size::LABEL)
                .style(catalog::text::muted)
                .boxed(),
        );
        above += 4.0 + note_height(&soundboard.note);
    }
    if soundboard.loading() {
        top = top.push(
            text(tr!("Loading sounds..."))
                .size(typography::size::LABEL)
                .style(catalog::text::muted)
                .boxed(),
        );
        above += 4.0 + 17.0;
    }
    let top = container(top).padding(Padding {
        top: 14.0,
        right: INSET,
        bottom: 4.0,
        left: INSET,
    });

    let rows = soundboard.rows.len().max(1) as f32 * ROW + FOOT;
    let room = rows.min(HEIGHT_MAX - above);
    let playable = soundboard.playable();
    let extras = !soundboard.chooser;
    let rows = &soundboard.rows;
    let body = list(
        rows.len(),
        move |index| key(&rows[index], index),
        move |index| line(&rows[index], playable, extras, motion),
        motion,
    )
    .height(Length::Fixed(room));

    container(column![top, body])
        .width(Length::Fixed(WIDTH))
        .clip(true)
        .style(|theme: &Theme| {
            let mut style = catalog::container::raised(theme);
            style.border = Border {
                radius: RADIUS.into(),
                ..style.border
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
                .style(catalog::text::muted),
        )
        .padding(Padding {
            left: INSET,
            right: INSET,
            bottom: 8.0,
            ..Padding::ZERO
        })
        .height(ROW)
        .width(Length::Fill)
        .align_y(iced::alignment::Vertical::Bottom)
        .boxed(),
        Row::Sounds(sounds) => container(
            row(sounds
                .iter()
                .map(|sound| chip(sound, playable, extras, motion).boxed()))
            .spacing(GAP),
        )
        .padding(Padding {
            left: INSET,
            right: INSET,
            top: 3.0,
            bottom: 3.0,
        })
        .height(ROW)
        .width(Length::Fill)
        .boxed(),
    }
}

/// A row's identity, which stays with it as the others come and go.
fn key(row: &Row, index: usize) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match row {
        Row::Heading(heading) => heading.hash(&mut hasher),
        Row::Sounds(sounds) => sounds.first().map(|sound| &sound.id).hash(&mut hasher),
    }
    index.hash(&mut hasher);
    hasher.finish()
}

/// The note wraps within the card's inset; a rough count of its lines.
fn note_height(note: &str) -> f32 {
    let per_line = ((WIDTH - 2.0 * INSET) / 6.5) as usize;
    note.chars().count().div_ceil(per_line.max(1)).max(1) as f32 * 17.0
}

const _: () = assert!(chip::HEIGHT + 6.0 <= ROW);
