//! What heads the channels: a category's heading, which folds it and opens
//! its menu, with a "+" for a new channel under the pointer
//! (`CategoryHeading`, `ui/rail.slint:840`); the pinned channels' own
//! heading, which does neither, and the line under them (`:1361`-`:1372`);
//! and Browse Channels at the top of a Community server's list
//! (`BrowseLink`, `:1138`).

use iced::widget::{button, column, container, rule, space, text};
use iced::{Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::menu::Spot;
use anomi_vm::sidebar::channels::{ChannelRow, Intent};
use anomi_widgets::icon::{Icon, icon};
use yapfast_lang::tr;

use super::channel::glyph_tint;
use super::items::{self, Room};
use super::pointed::{Fill as Ground, Ink, Look, pointed};

/// Discord keeps 17 px above a category's name, then 21 for its line
/// (`:853`); its focus ring has 4 px corners (`:939`).
const ABOVE: f32 = 17.0;
const RING: f32 = 4.0;
/// The "+": 24 px, its icon 16 (`:923`, `:927`).
const ADD: f32 = 24.0;
const ADD_GLYPH: f32 = 16.0;
/// Browse Channels' room above and under it (`:1141`).
const BROWSE_PAD: f32 = 6.0;
/// The pinned heading's text is 8 px in (`:1367`).
const PINNED_IN: f32 = 8.0;

pub fn category<'a>(row_vm: &'a ChannelRow, room: Room, motion: Motion) -> Element<'a, Intent> {
    let name = container(
        text(row_vm.heading.as_str())
            .size(typography::size::SMALL)
            .font(typography::SEMIBOLD)
            .style(catalog::text::inherit)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    )
    .padding(Padding::ZERO.left(metrics::SPACE_S));
    let look = Look {
        fill: Ground::None,
        ink: Ink::Heading,
        ring: Some(RING),
        fade_trailing: true,
        ..Look::default()
    };
    let id = row_vm.id.as_str();
    let mut heading = pointed(name, items::HEADING, look, motion)
        .top(ABOVE)
        .chevron(row_vm.collapsed)
        .on_press(Intent::Fold(row_vm.category.clone()))
        .on_menu(move |at| Intent::CategoryMenu(id.to_owned(), Spot::pointer(at)));
    if row_vm.may_create {
        let add = button(container(icon(Icon::Plus, ADD_GLYPH)).center(ADD))
            .padding(0)
            .on_press(Intent::Create(id.to_owned()))
            .style(catalog::button::bare);
        heading = heading.trailing(add);
    }
    inset(heading, room)
}

/// The pinned channels' heading: 12 px bold muted text, 36 px tall.
pub fn pinned(row_vm: &ChannelRow, room: Room) -> Element<'_, Intent> {
    let heading = container(
        text(row_vm.heading.as_str())
            .size(typography::size::CAPTION)
            .font(typography::BOLD)
            .style(catalog::text::muted)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    )
    .padding(Padding::ZERO.left(PINNED_IN))
    .center_y(items::PINNED)
    .width(Fill);
    inset(heading, room)
}

/// The line that ends the pinned channels, in the middle of its 9 px.
pub fn divider<'a>(room: Room) -> Element<'a, Intent> {
    let line = container(rule::horizontal(1).style(catalog::rule::line))
        .center_y(items::DIVIDER)
        .width(Fill);
    inset(line, room)
}

/// Browse Channels: a row of its own, chosen while its screen shows.
pub fn browse<'a>(browsing: bool, room: Room, motion: Motion) -> Element<'a, Intent> {
    let name = text(tr!("Browse channels"))
        .size(typography::size::BODY)
        .font(typography::MEDIUM)
        .style(catalog::text::inherit)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Fill);
    let body = container(name)
        .padding(Padding::ZERO.right(metrics::SPACE_S))
        .center_y(metrics::CHANNEL_ROW_HEIGHT)
        .width(Fill);
    let look = Look {
        selected: browsing,
        ..Look::default()
    };
    let row = pointed(body, metrics::CHANNEL_ROW_HEIGHT, look, motion)
        .leading(Icon::Hash, glyph_tint)
        .on_press(Intent::Browse);
    let padded = column![space().height(BROWSE_PAD), row, space().height(BROWSE_PAD)];
    inset(padded, room)
}

/// An item in the list's row: 8 px in from each side, with its room
/// above and under it.
fn inset<'a>(content: impl View<Intent> + 'a, room: Room) -> Element<'a, Intent> {
    container(content)
        .padding(Padding {
            top: room.above,
            right: metrics::ROW_INSET,
            bottom: room.under,
            left: metrics::ROW_INSET,
        })
        .width(Fill)
        .boxed()
}
