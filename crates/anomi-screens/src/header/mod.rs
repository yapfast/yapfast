//! The conversation's header: the open place's title and topic, then its
//! buttons. The Slint side: the header of `ui/yapfast.slint`
//! (`:3138`-`:3464`) and `ui/header.slint`. Task N07
//! (docs/iced/tasks/shell.md).
//!
//! - [`title`]: a DM's person, a group, a channel with its topic, a thread
//!   with the way back to its channel, and the titles of Friends, Requests
//!   and Browse Channels.
//! - [`tools`]: the buttons of each kind of place.
//! - [`topic()`]: the topic in full, the popover of the shell's
//!   `Layer::Topic`; its frame is [`popover`].
//!
//! The search field at the right (`HeaderSearch`) is C21's and is not drawn
//! here; [`Tools::search`](anomi_vm::header::Tools::search) says it has its
//! place.

mod popover;
mod title;
mod tools;
mod topic;

#[cfg(feature = "shots")]
mod shots;

use iced::widget::{container, row, space, svg, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, catalog, metrics, typography};
use anomi_vm::header::{Header, Intent, Title};
use anomi_vm::kind::Presence;
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::icon::{Icon, icon};

#[cfg(feature = "shots")]
pub use shots::shots;
pub use topic::topic;

/// Between the title and the buttons.
const GAP: f32 = 10.0;

pub fn view(header: &Header, motion: Motion) -> impl View<Intent> + '_ {
    // A one-to-one DM's person sits 4 px into their button, so the header
    // starts 4 px nearer the edge (`:3144`).
    let person = matches!(header, Header::Open(place)
        if matches!(&place.title, Title::Direct(direct) if direct.person));
    let line: Element<'_, Intent> = match header {
        Header::None => space().boxed(),
        Header::Open(place) => row![title::view(place, motion), tools::view(place, motion)]
            .spacing(GAP)
            .align_y(Alignment::Center)
            .boxed(),
        Header::Direct { name, avatar: face } => skeleton_direct(name, face).boxed(),
        Header::Channel { name, topic } => skeleton_channel(name, topic).boxed(),
    };
    container(line)
        .padding(Padding::from([0.0, metrics::SPACE_L]).left(if person {
            metrics::SPACE_M
        } else {
            metrics::SPACE_L
        }))
        .width(Fill)
        .center_y(metrics::HEADER_HEIGHT - 1.0)
}

/// An icon in one of the palette's inks.
fn tinted<'a>(glyph: Icon, size: f32, ink: fn(&Palette) -> iced::Color) -> impl View<Intent> + 'a {
    icon(glyph, size).style(move |theme: &Theme, _status| svg::Style {
        color: Some(ink(theme.palette())),
    })
}

/// The skeleton's DM header, which the app's projection still builds: the
/// avatar and name, no buttons (see [`Header`]).
fn skeleton_direct<'a>(name: &'a str, face: &'a anomi_vm::kind::Avatar) -> impl View<Intent> + 'a {
    row![
        avatar(face, 24.0, Presence::None, Ground::Chat),
        text(name)
            .size(typography::size::TITLE)
            .font(typography::SEMIBOLD),
    ]
    .spacing(metrics::SPACE_S)
    .align_y(Alignment::Center)
    .width(Fill)
}

/// The skeleton's channel header: the name and the topic's text.
fn skeleton_channel<'a>(name: &'a str, topic: &'a str) -> impl View<Intent> + 'a {
    row![
        icon(Icon::Hash, metrics::ICON_HEADER),
        text(name)
            .size(typography::size::TITLE)
            .font(typography::SEMIBOLD),
        text(topic)
            .size(typography::size::SMALL)
            .style(catalog::text::muted)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    ]
    .spacing(metrics::SPACE_S)
    .align_y(Alignment::Center)
    .width(Fill)
}
