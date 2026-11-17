//! The full profile (`ProfileModal`, `ui/people.slint:2067`): a person's
//! card on the left, in its theme and effect, and on the right tabs for what
//! they are doing and what they share with this account, as Discord's modal
//! shows it. The shell gives the sheet's backdrop, Escape and the way in and
//! out; this draws the modal on its own ground, which a profile's theme
//! tints, 960 px wide and 800 high at most.
//!
//! The card is `card::card` in `Place::Side`; its intents reach the app as
//! they are. [`tabs`] is the bar, [`pages`] the lists, [`rows`] the mutual
//! friends and servers, [`states`] a tab still loading or empty, and [`look`]
//! the colours a theme sets.

mod look;
mod pages;
mod rows;
#[cfg(feature = "shots")]
mod shots;
mod states;
mod tabs;

use iced::widget::{Column, Row, Stack, button, container, responsive};
use iced::{Background, Border, Fill, Length, Padding, Shadow, Size, Vector};

use anomi_theme::prelude::*;
use anomi_theme::rgba;
use anomi_vm::icon::Icon;
use anomi_vm::people::full_profile::{FullProfile, Intent, Page};
use anomi_vm::people::profile::Place;
use anomi_vm::people::{self};
use anomi_widgets::icon::icon;
use anomi_widgets::list::{list, scroll_view};
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

use super::card::card;
use look::Look;

#[cfg(feature = "shots")]
pub use shots::shots;

/// The modal's widest and highest, in a window with room.
const WIDTH: f32 = 960.0;
const HEIGHT: f32 = 800.0;
/// The card's width, and the tabs' column's least room.
const CARD: f32 = 400.0;
const TABS_LEAST: f32 = 320.0;
/// The card's inset from the modal's top and left, and from the tabs, where
/// there's room and where there's not.
const EDGE_MAX: f32 = 48.0;
const EDGE_MIN: f32 = 24.0;
const TABS_GAP: f32 = 32.0;
const RADIUS: f32 = 16.0;
const CLOSE: f32 = 32.0;
const SHADOW: Shadow = Shadow {
    color: rgba(0x00000080),
    offset: Vector::new(0.0, 8.0),
    blur_radius: 24.0,
};

/// The modal: `Layer::FullProfile`.
pub fn view(full: &FullProfile, motion: Motion) -> impl View<people::Intent> + '_ {
    responsive(move |size| modal(full, size, motion))
        .width(Length::Fill.max(WIDTH))
        .height(Length::Fill.max(HEIGHT))
}

/// As far in from the top and left as the tabs from the card and the right
/// edge: 48 px where there's room, down to 24 in a narrow window.
fn edge(width: f32) -> f32 {
    ((width - CARD - TABS_GAP - TABS_LEAST) / 2.0).clamp(EDGE_MIN, EDGE_MAX)
}

fn modal(full: &FullProfile, size: Size, motion: Motion) -> Element<'_, people::Intent> {
    let look = Look::new(full);
    let edge = edge(size.width);
    let gap = TABS_GAP.min(edge);
    let tabs_width = size.width - edge * 2.0 - CARD - gap;
    let right = Column::<Element<'_, people::Intent>>::new()
        .spacing(16)
        .width(Fill)
        .height(Fill)
        .padding(Padding::ZERO.bottom(edge / 2.0))
        .push(
            tabs::bar(full, look)
                .map(people::Intent::FullProfile)
                .boxed(),
        )
        .push(content(full, tabs_width, look, motion));
    let body = Row::<Element<'_, people::Intent>>::new()
        .spacing(gap)
        .padding(Padding::ZERO.top(edge).left(edge).right(edge))
        .push(
            container(card(&full.card, Place::Side, motion))
                .width(CARD)
                .height(Fill)
                .boxed(),
        )
        .push(right.boxed());
    Stack::with_children([ground(look), body.boxed(), close(look, motion)])
        .width(Fill)
        .height(Fill)
        .boxed()
}

/// The modal's own ground, themed or not, with its corners and shadow.
fn ground<'a, M: 'a>(look: Look) -> Element<'a, M> {
    container(iced::widget::space())
        .width(Fill)
        .height(Fill)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(look.ground(palette))),
                border: Border {
                    color: palette.edge,
                    width: if look.themed() { 0.0 } else { 1.0 },
                    radius: RADIUS.into(),
                },
                shadow: SHADOW,
                ..container::Style::default()
            }
        })
        .boxed()
}

/// The close button, on a tile of its own in the corner, as Discord's.
fn close<'a>(look: Look, motion: Motion) -> Element<'a, people::Intent> {
    let glyph = icon(Icon::X, 18.0).style(move |theme: &Theme, status| {
        let palette = theme.palette();
        iced::widget::svg::Style {
            color: Some(match status {
                iced::widget::svg::Status::Hovered => look.ink(palette),
                iced::widget::svg::Status::Idle => look.ink2(palette),
            }),
        }
    });
    let face = button(container(glyph).center(CLOSE))
        .padding(0)
        .on_press(people::Intent::FullProfile(Intent::Close))
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let tile = look.tile(palette);
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(if hovered {
                    look::mix(tile, look.ink(palette), 0.85)
                } else {
                    tile
                })),
                border: Border::default().rounded(8),
                ..button::Style::default()
            }
        });
    container(tip(face, tr!("Close profile"), Side::Below, motion))
        .padding(8)
        .align_right(Fill)
        .align_top(Fill)
        .boxed()
}

/// What the shown tab holds: rows the shape of what's coming, Discord's
/// words for an empty tab, or its list.
fn content<'a>(
    full: &'a FullProfile,
    width: f32,
    look: Look,
    motion: Motion,
) -> Element<'a, people::Intent> {
    let page = full.page();
    if full.loading() {
        return container(states::loading(page, motion))
            .height(Fill)
            .boxed();
    }
    if full.empty() {
        return states::empty(full, look, motion);
    }
    match page {
        Page::Activity => scroll_view(pages::activity(full, look, motion), motion).boxed(),
        Page::Board => scroll_view(pages::board(full, look, motion), motion).boxed(),
        Page::Wishlist => scroll_view(pages::wishlist(full, width, look, motion), motion).boxed(),
        Page::Friends => list(
            full.friends.len(),
            move |at| rows::key(&full.friends[at].id),
            move |at| rows::friend(&full.friends[at], look, motion),
            motion,
        )
        .estimate(rows::PLAIN)
        .boxed()
        .map(people::Intent::FullProfile)
        .boxed(),
        Page::Servers => list(
            full.servers.len(),
            move |at| rows::key(&full.servers[at].id),
            move |at| rows::server(&full.servers[at], look, motion),
            motion,
        )
        .estimate(rows::DETAILED)
        .boxed()
        .map(people::Intent::FullProfile)
        .boxed(),
    }
}
