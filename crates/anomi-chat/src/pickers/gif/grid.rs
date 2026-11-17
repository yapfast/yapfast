//! The grid: the tiles in their columns, built only near the view and
//! replaced by room of their size elsewhere, in a piece that scrolls; the
//! note that stands in for tiles, and the terms to try below a search that
//! found nothing (the `Flickable` of `GifPicker`, `ui/gifs.slint:140`-`:229`).

use iced::widget::{Id, button, column, container, row, space, text};
use iced::{Alignment, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::gif_picker::{GUTTER, GifPicker, Intent, Page, SHOWN};
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

use super::tile;

/// A note in place of tiles: 24 px in, 40 px down, in a place 96 px tall
/// (`:222`-`:226`, `:140`).
const NOTE_IN: f32 = 24.0;
const NOTE_DOWN: f32 = 40.0;
const NOTE_PLACE: f32 = 96.0;
/// The suggestions: 4 px below the grid, 10 px between the sentence and the
/// terms, 8 between terms, each 28 px tall with 4 px corners (`:196`-`:219`).
const FOOT_GAP: f32 = 4.0;
const FOOT_SPACING: f32 = 10.0;
const TERM_SPACING: f32 = 8.0;
const TERM_HEIGHT: f32 = 28.0;
const TERM_PAD: f32 = 10.0;
const TERM_RADIUS: f32 = 4.0;

/// The grid's scroll id: another page starts at its top (`changed view =>
/// grid.content-y = 0`).
fn id(page: Page) -> Id {
    Id::from(format!("gif-grid {page:?}"))
}

pub fn view<'a>(picker: &'a GifPicker, panel: f32, motion: Motion) -> Element<'a, Intent> {
    let mut content = column![];
    if !picker.tiles.is_empty() {
        content = content.push(tiles(picker, panel, motion).boxed());
    }
    if !picker.note.is_empty() {
        content = content.push(note(picker).boxed());
    }
    if !picker.suggestions.is_empty() {
        let gap = if picker.tiles.is_empty() {
            0.0
        } else {
            FOOT_GAP
        };
        content = content
            .push(space().height(gap).boxed())
            .push(foot(picker).boxed());
    }
    scroll_view(content, motion)
        .scrollbar(false)
        .id(id(picker.page))
        .report_pixels(true)
        .on_viewport(|seen| Intent::Shown {
            offset: seen.offset,
            height: seen.height,
        })
        .boxed()
}

/// The columns, each tile in its place; a tile a screen from the view is
/// room alone.
fn tiles<'a>(picker: &'a GifPicker, panel: f32, motion: Motion) -> Element<'a, Intent> {
    let placed = picker.placed(panel);
    let (top, seen) = picker.shown;
    let seen = if seen > 0.0 { seen } else { SHOWN };
    let (from, to) = (top - seen, top + 2.0 * seen);
    let columns = placed.columns.iter().map(|indices| {
        column(indices.iter().map(|&at| {
            let slot = &placed.slots[at];
            if slot.bottom() < from || slot.y > to {
                space().width(slot.width).height(slot.height).boxed()
            } else {
                tile::view(&picker.tiles[at], at, slot, picker.shift, motion)
            }
        }))
        .spacing(GUTTER)
        .boxed()
    });
    row(columns)
        .spacing(GUTTER)
        .padding(Padding::new(GUTTER))
        .boxed()
}

/// What the grid says with no tiles, centred in its place.
fn note(picker: &GifPicker) -> Element<'_, Intent> {
    let words = text(&picker.note)
        .size(typography::size::BODY)
        .style(catalog::text::muted)
        .align_x(Alignment::Center)
        .width(Fill);
    container(words)
        .padding(Padding {
            top: NOTE_DOWN,
            right: NOTE_IN,
            bottom: 0.0,
            left: NOTE_IN,
        })
        .width(Fill)
        .height(NOTE_PLACE)
        .boxed()
}

/// "Your perfect GIF is in another castle", and the terms to try.
fn foot(picker: &GifPicker) -> Element<'_, Intent> {
    let sentence = text(tr!(
        "Your perfect GIF is in another castle. Try the suggested keywords below!"
    ))
    .size(typography::size::SMALL)
    .style(catalog::text::secondary)
    .align_x(Alignment::Center)
    .width(Fill);
    let terms = row(picker.suggestions.iter().map(|term| {
        button(
            container(
                text(term)
                    .size(typography::size::LABEL)
                    .font(typography::MEDIUM),
            )
            .center_y(TERM_HEIGHT)
            .padding(Padding::new(0.0).left(TERM_PAD).right(TERM_PAD)),
        )
        .padding(0)
        .on_press(Intent::Category(term.clone()))
        .style(|theme: &Theme, status| {
            let palette = theme.palette();
            button::Style {
                background: Some(
                    match status {
                        button::Status::Hovered | button::Status::Pressed => palette.hover,
                        _ => palette.sunken,
                    }
                    .into(),
                ),
                text_color: palette.text,
                border: Border::default().rounded(TERM_RADIUS),
                ..button::Style::default()
            }
        })
        .boxed()
    }))
    .spacing(TERM_SPACING)
    .wrap()
    .vertical_spacing(TERM_SPACING)
    .align_x(Alignment::Center);
    container(
        column![sentence, container(terms).center_x(Fill)]
            .spacing(FOOT_SPACING)
            .width(Fill),
    )
    .padding(Padding::ZERO.left(GUTTER).right(GUTTER).bottom(GUTTER))
    .width(Fill)
    .boxed()
}
