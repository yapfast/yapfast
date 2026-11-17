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
//! The search field at the right (`HeaderSearch`) is C21's: [`with_search`]
//! places it in the row.

mod limit;
mod person;
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

/// Between the title and the buttons, and between the buttons; 4 px in a
/// narrow conversation (`:3146`).
const GAP: f32 = 10.0;
const GAP_COMPACT: f32 = 4.0;

pub fn view(header: &Header, motion: Motion) -> impl View<Intent> + '_ {
    assemble::<Intent>(header, None, motion)
}

/// [`view`] with the search field `search` placed in the row where Slint
/// has it (`ui/yapfast.slint:3440`-`:3457`): after the profile button and
/// before the members' count, spaced as the buttons are, where the place has
/// [`Tools::search`](anomi_vm::header::Tools::search). N01's conversation
/// passes C21's `anomi_chat::search::field::view`.
pub fn with_search<'a, M: From<Intent> + Clone + 'static>(
    header: &'a Header,
    search: Element<'a, M>,
    motion: Motion,
) -> Element<'a, M> {
    assemble(header, Some(search), motion).boxed()
}

/// An element of the header's own intents, as the caller's messages.
fn lift<'a, M: From<Intent> + 'static>(element: Element<'a, Intent>) -> Element<'a, M> {
    element.map(M::from).boxed()
}

fn assemble<'a, M: From<Intent> + Clone + 'static>(
    header: &'a Header,
    search: Option<Element<'a, M>>,
    motion: Motion,
) -> iced::widget::Container<'a, Element<'a, M>, Theme> {
    // A one-to-one DM's person sits 4 px into their button, so the header
    // starts 4 px nearer the edge (`:3144`).
    let person = matches!(header, Header::Open(place)
        if matches!(&place.title, Title::Direct(direct) if direct.person));
    let line: Element<'a, M> = match header {
        Header::None => space().boxed(),
        Header::Open(place) => {
            let tools = &place.tools;
            let (before, after) = tools::view(place, motion);
            let mut items: Vec<Element<'a, M>> = vec![lift(title::view(place, motion))];
            items.extend(before.into_iter().map(lift));
            if tools.search {
                items.extend(search);
            }
            items.extend(after.into_iter().map(lift));
            row(items)
                .spacing(if tools.compact { GAP_COMPACT } else { GAP })
                .align_y(Alignment::Center)
                .boxed()
        }
        Header::Direct { name, avatar: face } => lift(skeleton_direct(name, face).boxed()),
        Header::Channel { name, topic } => lift(skeleton_channel(name, topic).boxed()),
    };
    // 49 px and a 1 px pad above: the content centres at y=25 in the bar's
    // 50 px, the line below covering its last row.
    container(line)
        .padding(
            Padding::from([0.0, metrics::SPACE_L])
                .top(1.0)
                .left(if person {
                    metrics::SPACE_M
                } else {
                    metrics::SPACE_L
                }),
        )
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
