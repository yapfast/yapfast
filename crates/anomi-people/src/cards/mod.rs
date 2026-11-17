//! The server, tag and role cards: a server a profile lists as mutual, a
//! server tag's server, and a role mentioned in a message. The Slint side:
//! `ServerCard` (`ui/people.slint:884`), `TagCardView` (`:974`) and
//! `RoleCard` (`:2327`); the window places them (`ui/yapfast.slint:4989`,
//! `:5099`, `:5150`). Task P12 (docs/iced/tasks/people.md).
//!
//! The shell draws two layers from here: `Layer::Card` calls [`view`] (the
//! server's card, or the tag's), `Layer::RoleCard` calls [`role`]. Each
//! card draws its own surface, its banner or header and its content; the
//! shell gives the backdrop, Escape and the way in and out.

mod head;
mod role;
mod server;
#[cfg(feature = "shots")]
mod shots;
mod tag;

use iced::widget::container;
use iced::{Background, Border, Color, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_vm::people::cards::{Cards, Intent};

#[cfg(feature = "shots")]
pub use shots::shots;

/// The cards' drop shadow (`drop-shadow-blur: 24px; drop-shadow-color:
/// #00000080; drop-shadow-offset-y: 8px`).
const SHADOW: Shadow = Shadow {
    color: Color::from_rgba(0.0, 0.0, 0.0, 128.0 / 255.0),
    offset: Vector::new(0.0, 8.0),
    blur_radius: 24.0,
};

/// A card's surface: the fill from the palette, a 1 px edge and the shadow,
/// as wide as `width`, as tall as its content. The content sits inside the
/// edge, so a banner's corners are one less than the card's.
fn surface<'a>(
    content: Element<'a, Intent>,
    width: f32,
    radius: f32,
    fill: fn(&Theme) -> Color,
) -> Element<'a, Intent> {
    container(content)
        .width(width)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(fill(theme))),
            border: Border {
                color: theme.palette().edge,
                width: 1.0,
                radius: radius.into(),
            },
            shadow: SHADOW,
            ..container::Style::default()
        })
        .boxed()
}

/// What `Layer::Card` shows: the server's card, else the tag's. With
/// neither open (the window opening or closing it) the role's, which
/// `Layer::RoleCard` draws through [`role`]; nothing when all are closed.
pub fn view(cards: &Cards, motion: Motion) -> impl View<Intent> + '_ {
    if let Some(server) = &cards.server {
        server::view(server, motion)
    } else if let Some(tag) = &cards.tag {
        tag::view(tag, motion)
    } else {
        role(cards, motion)
    }
}

/// What `Layer::RoleCard` shows: the role's card, or nothing while it is
/// closed.
pub fn role(cards: &Cards, motion: Motion) -> Element<'_, Intent> {
    match &cards.role {
        Some(role) => role::view(role, motion),
        None => iced::widget::space().boxed(),
    }
}
