//! Searching messages. Task C21 (docs/iced/tasks/chat.md). The Slint side:
//! `HeaderSearch`, `SearchPanel` and `FoundCard` (`ui/header.slint:65`,
//! `:647`, `:158`).
//!
//! - [`view`]: the panel docked beside the conversation (420 px, placed by
//!   the conversation's `beside`): a heading with its cross, a note, the
//!   results newest first with the channel's name above a run in one
//!   channel of a server, and "Show more".
//! - [`field()`]: the header's search field, which the header's row places at
//!   its right end. It is not part of [`view`].
//! - [`found::card`]: one message found, which a DM's pinned messages (C22)
//!   draw alike.

mod field;
mod found;
mod gallery;
pub(crate) mod grow;
mod poll;
#[cfg(feature = "shots")]
mod shots;

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::{Column, Id, container, row, rule, stack, svg, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::search::{Found, Intent, Search};
use anomi_widgets::button::{Kind, action_button, icon_button};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::list::list;
use anomi_widgets::tip::Side;
use yapfast_lang::tr;

pub use field::{COLLAPSED, OPEN, REST, natural, view as field};
pub use found::{JUMP, card};
#[cfg(feature = "shots")]
pub use shots::shots;

/// The header's height, its inset and the cross.
const HEADER: f32 = 48.0;
const HEADER_LEFT: f32 = 16.0;
const HEADER_RIGHT: f32 = 10.0;
const CROSS: f32 = 28.0;
const CROSS_GLYPH: f32 = 18.0;
const HEADING: f32 = 15.0;
/// The list's inset, the gap between results, and under a channel's name.
const LIST: f32 = 12.0;
const BETWEEN: f32 = 8.0;
const UNDER_CHANNEL: f32 = 6.0;
/// Above a channel's name, and between its picture and the name.
const ABOVE_CHANNEL: f32 = 6.0;
const CHANNEL_GAP: f32 = 4.0;
const CHANNEL_ICON: f32 = 14.0;
/// "Show more"'s id, which tests find it by.
pub const MORE: Id = Id::new("search-more");
/// The note's inset.
const NOTE: f32 = 16.0;
/// Above "Show more".
const ABOVE_MORE: f32 = 4.0;

/// The panel: what a search found, beside the conversation.
pub fn view(search: &Search, motion: Motion) -> impl View<Intent> + '_ {
    let heading = row![
        text(search.heading.as_str())
            .size(HEADING)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary)
            .width(Fill),
        container(
            icon_button(Icon::X, tr!("Close search"), motion)
                .size(CROSS)
                .glyph(CROSS_GLYPH)
                .tip_side(Side::Below)
                .on_press(Intent::Close)
                .view()
        )
        .center_y(Fill),
    ]
    .spacing(HEADER_RIGHT)
    .padding(Padding {
        left: HEADER_LEFT,
        right: HEADER_RIGHT,
        ..Padding::ZERO
    })
    .align_y(Alignment::Center)
    .height(HEADER);

    // The heading's line is the header's bottom pixel row (`y: 47px`,
    // `ui/header.slint:670`).
    let line = container(rule::horizontal(1).style(catalog::rule::line))
        .align_bottom(Fill)
        .width(Fill)
        .height(HEADER);
    let mut parts: Vec<Element<'_, Intent>> =
        vec![stack![heading, line].width(Fill).height(HEADER).boxed()];
    if !search.note.is_empty() {
        parts.push(
            container(
                text(search.note.as_str())
                    .size(typography::size::LABEL)
                    .style(catalog::text::muted),
            )
            .padding(NOTE)
            .width(Fill)
            .boxed(),
        );
    }
    parts.push(results(search, motion));
    let panel = Column::with_children(parts).width(Fill).height(Fill);
    // The edge is drawn over the panel's first column
    // (`x: 0; width: 1px`, `ui/header.slint:661`).
    let edge = container(rule::vertical(1).style(edge))
        .align_left(Fill)
        .width(Fill)
        .height(Fill);
    container(stack![panel, edge])
        .width(Fill)
        .height(Fill)
        .style(catalog::container::sidebar)
}

/// The panel's left edge, `Theme.edge`.
fn edge(theme: &Theme) -> rule::Style {
    rule::Style {
        color: theme.palette().edge,
        ..catalog::rule::line(theme)
    }
}

/// The results in a list that builds only those in view, then "Show more"
/// when a next page can be asked for.
fn results(search: &Search, motion: Motion) -> Element<'_, Intent> {
    let found = &search.results;
    let rows = found.len() + usize::from(search.more);
    list(
        rows,
        move |at| match found.get(at) {
            Some(found) => key(&found.item),
            None => u64::MAX,
        },
        move |at| match found.get(at) {
            Some(found) => one(found, motion),
            None => more(search, motion),
        },
        motion,
    )
    .padding(LIST)
    .spacing(BETWEEN)
    .height(Fill)
    .boxed()
}

/// One result, under its channel's name when it starts a run.
fn one(found: &Found, motion: Motion) -> Element<'_, Intent> {
    let card = found::card(found, motion);
    if found.heading.is_empty() {
        return card;
    }
    let name = row![
        icon(Icon::Hash, CHANNEL_ICON).style(|theme: &Theme, _| svg::Style {
            color: Some(theme.palette().muted),
        }),
        text(found.heading.as_str())
            .size(typography::size::HEADING)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::secondary)
            .width(Fill),
    ]
    .spacing(CHANNEL_GAP)
    .padding(Padding::default().top(ABOVE_CHANNEL))
    .align_y(Alignment::Center);
    Column::with_children([name.boxed(), card])
        .spacing(UNDER_CHANNEL)
        .boxed()
}

fn more(search: &Search, motion: Motion) -> Element<'_, Intent> {
    let label = if search.loading {
        tr!("Loading...")
    } else {
        tr!("Show more")
    };
    container(
        action_button(label, Kind::Secondary, motion)
            .small()
            .enabled(!search.loading)
            .id(MORE)
            .on_press(Intent::More),
    )
    .center_x(Fill)
    .padding(Padding::default().top(ABOVE_MORE))
    .boxed()
}

/// A result's identity, which keeps its row through a next page.
fn key(item: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    hasher.finish()
}
