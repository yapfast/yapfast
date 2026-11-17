//! The grid: each collection's heading and its stickers in the virtual list,
//! which builds only the rows near its view and tells which show, so the app
//! loads the pictures near them; or a note in place of the tiles
//! (`ui/stickers.slint`, the `ScrollView`). The Slint grid published the tiles
//! in sight from the adapter (`publish_sticker_tiles`); the list builds only
//! what shows, which comes to the same.

use std::borrow::Cow;
use std::rc::Rc;

use iced::widget::{Id, container, row, space, svg, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::sticker_picker::{
    GAP, Grid, HEADING, Intent, PADDING, Row, Section, SectionIcon, StickerPicker, TILE,
};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::list::list;
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

use super::tile;

/// The grid's id for a query: a new search's grid starts at its top
/// (`changed query => grid.content-y = 0`), and the rail's jump goes to the
/// current one.
pub fn grid_id(query: &str) -> Id {
    Id::from(format!("sticker-picker-grid {query}"))
}

/// A heading's words sit 2 px further in than the tiles, 6 px from its 16 px
/// icon, in 13 px bold (`x: 2px; spacing: 6px; height: 18px`).
const HEADING_IN: f32 = 2.0;
const HEADING_GAP: f32 = 6.0;
const HEADING_ICON: f32 = 16.0;
/// The note's place: 24 px in, 40 px down (`x: 24px; y: 40px`).
const NOTE_IN: f32 = 24.0;
const NOTE_TOP: f32 = 40.0;

pub fn view<'a>(picker: &'a StickerPicker, motion: Motion) -> Element<'a, Intent> {
    let grid = Rc::new(picker.grid());
    if grid.is_empty() {
        return note(picker);
    }
    let keys = Rc::clone(&grid);
    list(
        grid.len(),
        move |at| keys.key(at),
        move |at| line(picker, &grid, at, motion),
        motion,
    )
    .id(grid_id(&picker.query))
    .estimate(TILE + GAP)
    .on_viewport(|seen| Intent::Shown(seen.first, seen.last))
    .boxed()
}

/// Row `at` of the grid.
fn line<'a>(
    picker: &'a StickerPicker,
    grid: &Grid<'a>,
    at: usize,
    motion: Motion,
) -> Element<'a, Intent> {
    match grid.row(at) {
        Some(Row::Heading(_, section)) => heading(section, motion),
        Some(Row::Stickers { stickers, last, .. }) => {
            // Each row is a tile and the gap under it; a section's last has
            // the gap before the next heading too.
            let below = if last { 2.0 * GAP } else { GAP };
            let tiles = row(stickers
                .iter()
                .map(|sticker| tile::view(picker, sticker, motion)))
            .spacing(GAP);
            container(tiles)
                .padding(Padding {
                    left: PADDING,
                    ..Padding::ZERO
                })
                .height(TILE + below)
                .boxed()
        }
        None => space().boxed(),
    }
}

/// A collection's heading: Frequently Used's clock, or its server's or first
/// sticker's picture (once it is here), then its name in secondary text.
fn heading(section: &Section, motion: Motion) -> Element<'_, Intent> {
    let icon_of: Option<Element<'_, Intent>> = match &section.icon {
        SectionIcon::Frequent => Some(
            icon(Icon::Clock, HEADING_ICON)
                .style(|theme: &Theme, _| svg::Style {
                    color: Some(theme.palette().secondary),
                })
                .boxed(),
        ),
        other => other
            .picture()
            .and_then(|expected| expected.ready())
            .map(|handle| {
                picture(handle.clone())
                    .side(HEADING_ICON)
                    .fit(Fit::Contain)
                    .motion(motion)
                    .boxed()
            }),
    };
    let name = text(section.name.as_str())
        .size(typography::size::HEADING)
        .font(typography::BOLD)
        .style(catalog::text::secondary)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    let mut line = row![].spacing(HEADING_GAP).align_y(Alignment::Center);
    if let Some(icon_of) = icon_of {
        line = line.push(icon_of);
    }
    container(line.push(name.boxed()))
        .padding(Padding {
            left: PADDING + HEADING_IN,
            right: PADDING,
            ..Padding::ZERO
        })
        .width(Fill)
        .height(HEADING)
        .align_y(Alignment::Center)
        .boxed()
}

/// What the grid says in place of tiles, when it says anything: why the
/// packs didn't come, or that the search matched nothing.
fn words(picker: &StickerPicker) -> Option<Cow<'_, str>> {
    if let Some(error) = &picker.error {
        return Some(Cow::Borrowed(error));
    }
    picker
        .searching()
        .then(|| Cow::Borrowed(tr!("No stickers match your search.")))
}

fn note(picker: &StickerPicker) -> Element<'_, Intent> {
    let Some(words) = words(picker) else {
        return space().width(Fill).height(Fill).boxed();
    };
    let words = text(words)
        .size(typography::size::BODY)
        .style(catalog::text::muted)
        .align_x(Alignment::Center)
        .width(Fill);
    container(words)
        .padding(Padding {
            top: NOTE_TOP,
            left: NOTE_IN,
            right: NOTE_IN,
            bottom: 0.0,
        })
        .width(Fill)
        .height(Fill)
        .boxed()
}
