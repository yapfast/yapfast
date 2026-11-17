//! The grid: each section's heading and its emoji, or a search's matches,
//! in the virtual list, which builds only the rows near its view and tells
//! which show, so the app loads the custom emoji's pictures near them; or a
//! note that nothing matches (`EmojiPicker`'s `ScrollView`,
//! `ui/messages.slint:1341`-`:1393`). The Slint grid moved a few kept
//! buttons as it scrolled (`src/ui/emoji_grid.rs`); the list builds only
//! what shows, which comes to the same.

use std::rc::Rc;

use iced::widget::{Id, column, container, row, space, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::emoji_picker::{
    CELL, Emoji, EmojiPicker, Grid, Hovered, Intent, Row, Section, SectionIcon,
};
use anomi_widgets::list::list;
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

use super::cell::emoji_button;
use super::face;

/// The grid's id for a query: a new search's grid starts at its top
/// (`grid.content-y = 0`, `:1314`), and the rail's jump goes to the one
/// without a query, `grid_id("")`.
pub fn grid_id(query: &str) -> Id {
    Id::from(format!("emoji-picker-grid {query}"))
}

/// An emoji's side in its button (`glyph: 36px`, `:1376`; `GLYPH`,
/// `src/emoji.rs:68`).
const GLYPH: f32 = 36.0;
/// A row's padding either side (`padding-left: 8px; padding-right: 8px`,
/// `:1350`; `PADDING`, `src/ui/emoji_grid.rs:30`).
const PAD: f32 = 8.0;
/// A heading's row: 28 px (`HEADING_HEIGHT`, `src/emoji.rs:75`), its words
/// 2 px further in and 2 px off its foot, 6 px from its 16 px icon, a
/// server's with 4 px corners (`:1352`-`:1366`).
const HEADING: f32 = 28.0;
const HEADING_IN: f32 = 2.0;
const HEADING_FOOT: f32 = 2.0;
const HEADING_GAP: f32 = 6.0;
const HEADING_ICON: f32 = 16.0;
const ICON_RADIUS: f32 = 4.0;
/// The note's padding (`:1389`).
const NOTE_PAD: f32 = 16.0;

pub fn view<'a>(picker: &'a EmojiPicker, motion: Motion) -> Element<'a, Intent> {
    let grid = Rc::new(picker.grid());
    if grid.is_empty() {
        return note(&picker.query);
    }
    let keys = Rc::clone(&grid);
    list(
        grid.len(),
        move |at| keys.key(at),
        move |at| line(picker, &grid, at, motion),
        motion,
    )
    .id(grid_id(&picker.query))
    .estimate(CELL)
    .on_viewport(|seen| Intent::Shown(seen.first, seen.last))
    .boxed()
}

/// Row `at` of the grid.
fn line<'a>(
    picker: &'a EmojiPicker,
    grid: &Grid<'a>,
    at: usize,
    motion: Motion,
) -> Element<'a, Intent> {
    match grid.row(at) {
        Some(Row::Heading(_, section)) => heading(section),
        Some(Row::Emoji(emoji)) => row(emoji.iter().map(|emoji| cell(picker, emoji, motion)))
            .padding(Padding {
                left: PAD,
                right: PAD,
                ..Padding::ZERO
            })
            .height(CELL)
            .boxed(),
        None => space().boxed(),
    }
}

/// An emoji of the grid: 56 px with the emoji 36 px across, its star
/// keeping it among the favourites, and no tooltip, the foot naming it
/// instead.
fn cell<'a>(picker: &'a EmojiPicker, emoji: &'a Emoji, motion: Motion) -> Element<'a, Intent> {
    emoji_button(emoji, motion)
        .size(CELL)
        .glyph(GLYPH)
        .star(
            picker.favorite(emoji),
            Intent::Favorite(emoji.key().to_owned()),
        )
        .on_press(Intent::Pick(emoji.clone()))
        .on_locked(Intent::Locked)
        .on_hover(Intent::Hover(Hovered::Emoji(emoji.clone())))
        .view()
}

/// A section's heading: its glyph or its server's picture, then its name in
/// capitals. With an icon its line is the icon's 16 px at the row's top, as
/// the 16 px icon is the most Slint's layout lets the line grow to; without
/// one it's in the middle of the row above its foot.
fn heading(section: &Section) -> Element<'_, Intent> {
    let icon: Option<Element<'_, Intent>> = match &section.icon {
        SectionIcon::Server {
            picture: expected, ..
        } => expected.ready().map(|handle| {
            picture(handle.clone())
                .side(HEADING_ICON)
                .radius(ICON_RADIUS)
                .fit(Fit::Contain)
                .boxed()
        }),
        icon_of => face::glyph(icon_of, HEADING_ICON),
    };
    let name = text(section.name.to_uppercase())
        .size(typography::size::HEADING)
        .font(typography::BOLD)
        .style(catalog::text::secondary);
    let mut line = row![].spacing(HEADING_GAP).align_y(Alignment::Center);
    let mut foot = HEADING_FOOT;
    if let Some(icon) = icon {
        line = line.push(icon);
        foot = HEADING - HEADING_ICON;
    }
    container(line.push(name.boxed()))
        .padding(Padding {
            top: 0.0,
            right: PAD,
            bottom: foot,
            left: PAD + HEADING_IN,
        })
        .width(Fill)
        .height(HEADING)
        .align_y(Alignment::Center)
        .boxed()
}

/// No emoji match the query: said under the empty grid's upper half
/// (`:1387`-`:1391`).
fn note(query: &str) -> Element<'_, Intent> {
    let words = text(tr!("No emoji match \"{}\".", query))
        .size(typography::size::LABEL)
        .style(catalog::text::muted)
        .align_x(Alignment::Center)
        .width(Fill);
    column![
        space().height(Fill),
        container(words).padding(NOTE_PAD).height(Fill)
    ]
    .width(Fill)
    .height(Fill)
    .boxed()
}
