//! The choices, section by section, in the order of `choices`
//! (`ui/voice.slint:1456`-`:1560`): a muted heading, then its rows.

use iced::widget::{Column, column, container, row, space, text};
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
/// The first heading's `padding-top` (`:1458`), and a later section's (`:1466`).
const TOP_FIRST: f32 = 4.0;
const TOP_SECTION: f32 = 8.0;
/// A whole-pixel line for the 12 px heading, so the cards below sit on pixels.
const HEADING_LINE: f32 = 16.0;
/// The applications under "One application" start here (`:1520`).
const APPS_INDENT: f32 = 24.0;

/// A section's heading: 12 px, bold, muted, `top` above it.
fn heading<'a>(words: &'static str, top: f32) -> Element<'a, Intent> {
    container(
        text(words)
            .size(typography::size::CAPTION)
            .line_height(text::LineHeight::Absolute(HEADING_LINE.into()))
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

/// Rows of a menu, one under another: `spacing` 0 for windows, cameras and
/// applications (`:1492`, `:1511`, `:1538`), 2 px for resolutions and rates.
fn rows<'a>(rows: &'a [Row<Intent>], spacing: f32, motion: Motion) -> Element<'a, Intent> {
    column(rows.iter().map(|row| item(row, motion).boxed()))
        .spacing(spacing)
        .width(Fill)
        .boxed()
}

/// Everything above the button, `inner` wide.
pub fn choices(share: &Share, inner: f32, motion: Motion) -> impl View<Intent> + '_ {
    let mut all = column![].spacing(GAP).width(Fill);
    let rows_of = &share.rows;
    if !share.screens.is_empty() {
        all = all.push(heading(tr!("SCREENS"), TOP_FIRST));
        all = all.push(screens(share, inner));
    }
    if !share.windows.is_empty() {
        let top = if share.screens.is_empty() {
            TOP_FIRST
        } else {
            TOP_SECTION
        };
        all = all.push(heading(tr!("WINDOWS"), top));
        all = all.push(rows(&rows_of.windows, 0.0, motion));
    }
    if let Some(dialog) = &rows_of.dialog {
        all = all.push(heading(tr!("SCREEN"), TOP_FIRST));
        all = all.push(item(dialog, motion).boxed());
    }
    if !share.cameras.is_empty() {
        let top = if share.dialog_index() > 0 || share.asks_dialog() {
            TOP_SECTION
        } else {
            TOP_FIRST
        };
        all = all.push(heading(tr!("CAMERAS"), top));
        all = all.push(rows(&rows_of.cameras, 0.0, motion));
    }
    let top = if share.lists() || share.asks_dialog() {
        TOP_SECTION
    } else {
        TOP_FIRST
    };
    // While live the quality, which changes the stream at once, comes
    // above the sound; before, under it, as Slint has it.
    if share.switching {
        all = quality(all, share, top, motion);
        all = all.push(heading(tr!("SOUND"), TOP_SECTION));
        all.push(sound(share, motion))
    } else {
        all = all.push(heading(tr!("SOUND"), top));
        all = all.push(sound(share, motion));
        quality(all, share, TOP_SECTION, motion)
    }
}

/// Resolutions, frame rates and encoding under `all`, `top` above the first.
fn quality<'a>(
    all: Column<Element<'a, Intent>>,
    share: &'a Share,
    top: f32,
    motion: Motion,
) -> Column<Element<'a, Intent>> {
    let rows_of = &share.rows;
    let mut all = all
        .push(heading(tr!("RESOLUTION"), top))
        .push(rows(&rows_of.resolutions, GAP, motion))
        .push(heading(tr!("FRAME RATE"), TOP_SECTION))
        .push(rows(&rows_of.rates, GAP, motion))
        .push(heading(tr!("ENCODING"), TOP_SECTION));
    if let Some(quality) = &rows_of.quality {
        all = all.push(item(quality, motion).boxed());
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
            container(rows(&share.rows.apps, 0.0, motion))
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
