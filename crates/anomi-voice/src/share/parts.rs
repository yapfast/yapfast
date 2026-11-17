//! The choices, section by section, in the order of `choices`
//! (`ui/voice.slint:1456`-`:1560`): a muted heading, then its rows.

use iced::widget::{column, container, row, space, text};
use iced::{Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::menu::Row;
use anomi_vm::voice::share::{Intent, Share};
use anomi_widgets::menu::item;
use yapfast_lang::tr;

use super::card;

/// Between the choices (`spacing: 2px`, `:1453`) and between the cards.
const GAP: f32 = 2.0;
const CARD_GAP: f32 = 4.0;
/// A heading's left padding and its bottom (`:1458`).
const HEADING_LEFT: f32 = 8.0;
const HEADING_BOTTOM: f32 = 4.0;
/// The applications under "One application" start here (`:1520`).
const APPS_INDENT: f32 = 24.0;

/// A section's heading: 12 px, bold, muted, `top` above it.
fn heading<'a>(words: &'static str, top: f32) -> Element<'a, Intent> {
    container(
        text(words)
            .size(typography::size::CAPTION)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .style(catalog::text::muted),
    )
    .padding(Padding {
        top,
        right: 0.0,
        bottom: HEADING_BOTTOM,
        left: HEADING_LEFT,
    })
    .width(Fill)
    .boxed()
}

/// Rows of a menu, one under another.
fn rows<'a>(rows: &'a [Row<Intent>], motion: Motion) -> Element<'a, Intent> {
    column(rows.iter().map(|row| item(row, motion).boxed()))
        .spacing(GAP)
        .width(Fill)
        .boxed()
}

/// Everything above the button, `inner` wide.
pub fn choices(share: &Share, inner: f32, motion: Motion) -> impl View<Intent> + '_ {
    let mut all = column![].spacing(GAP).width(Fill);
    let rows_of = &share.rows;
    if !share.screens.is_empty() {
        all = all.push(heading(tr!("SCREENS"), 4.0));
        all = all.push(screens(share, inner));
    }
    if !share.windows.is_empty() {
        let top = if share.screens.is_empty() { 4.0 } else { 8.0 };
        all = all.push(heading(tr!("WINDOWS"), top));
        all = all.push(rows(&rows_of.windows, motion));
    }
    if let Some(dialog) = &rows_of.dialog {
        all = all.push(heading(tr!("SCREEN"), 4.0));
        all = all.push(item(dialog, motion).boxed());
    }
    if !share.cameras.is_empty() {
        let top = if share.dialog_index() > 0 || share.asks_dialog() {
            8.0
        } else {
            4.0
        };
        all = all.push(heading(tr!("CAMERAS"), top));
        all = all.push(rows(&rows_of.cameras, motion));
    }
    let top = if share.lists() || share.asks_dialog() {
        8.0
    } else {
        4.0
    };
    all = all.push(heading(tr!("SOUND"), top));
    all = all.push(sound(share, motion));
    if !share.switching {
        all = all.push(heading(tr!("RESOLUTION"), 8.0));
        all = all.push(rows(&rows_of.resolutions, motion));
        all = all.push(heading(tr!("FRAME RATE"), 8.0));
        all = all.push(rows(&rows_of.rates, motion));
        all = all.push(heading(tr!("ENCODING"), 8.0));
        if let Some(quality) = &rows_of.quality {
            all = all.push(item(quality, motion).boxed());
        }
    }
    all
}

/// All applications, No sound and One application, whose list opens below
/// it with its checks under its text.
fn sound(share: &Share, motion: Motion) -> Element<'_, Intent> {
    let mut all = column![].spacing(GAP).width(Fill);
    for row in &share.rows.sound {
        let mut entry = item(row, motion);
        // The last is the one that opens the list: an arrow, down while open.
        if row.intent == Some(Intent::Apps(!share.apps_open)) {
            entry = entry.arrow(true).expanded(share.apps_open);
        }
        all = all.push(entry.boxed());
    }
    if share.apps_open {
        all = all.push(
            container(rows(&share.rows.apps, motion))
                .padding(Padding {
                    left: APPS_INDENT,
                    ..Padding::ZERO
                })
                .width(Fill)
                .boxed(),
        );
    }
    all.boxed()
}

/// The screens, two to a row; one alone sits in the middle.
fn screens(share: &Share, inner: f32) -> Element<'_, Intent> {
    let width = (inner - CARD_GAP) / 2.0;
    let make = |index: usize| {
        card::card(
            &share.screens[index],
            index,
            share.source == Some(index),
            width,
        )
        .boxed()
    };
    let mut all = column![].spacing(CARD_GAP).width(Fill);
    let count = share.screens.len();
    for first in (0..count).step_by(2) {
        let line = if count == 1 {
            row![
                space().width(Length::Fill),
                make(first),
                space().width(Length::Fill)
            ]
        } else {
            let mut line = row![make(first)].spacing(CARD_GAP);
            if first + 1 < count {
                line = line.push(make(first + 1));
            }
            line
        };
        all = all.push(line);
    }
    all.boxed()
}
