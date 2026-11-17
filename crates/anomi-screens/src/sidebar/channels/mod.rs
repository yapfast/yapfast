//! A server's channel list: the category headings above their channels, the
//! pinned channels first under their own, threads hanging under their
//! channel on a line, and who is in each voice channel under it, with LIVE
//! and mute marks and a card of them beside the row. Categories fold;
//! channels and categories drag to new places, and people onto other voice
//! channels. The Slint side: `RoomList`'s channel rows, `ChannelRow`,
//! `CategoryHeading`, `BrowseLink`, `DropLine`, `PersonTouch` and
//! `VoiceCardView` (`ui/rail.slint:1150`, `:397`, `:840`, `:1138`, `:787`,
//! `:799`, `:741`); what they show comes from `show_sidebar`
//! (`src/ui/refresh.rs:1059`), dragging from `:1575` and `:1605` and
//! `src/ui/callbacks.rs:2927`-`:3027`. Task N04.
//!
//! The list is F07's virtual list: only the items near the view are built
//! ([`items`]), and it draws the drop line and the dragged copy.

mod card;
mod channel;
mod drag;
mod heading;
mod items;
mod live;
mod person;
pub mod pointed;
#[cfg(feature = "shots")]
mod shots;

#[cfg(feature = "shots")]
pub use shots::shots;

use std::rc::Rc;

use iced::advanced::widget::Id;
use iced::widget::{Column, column, container};
use iced::{Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::sidebar::channels::{Channels, Intent};
use anomi_widgets::list::list;

use items::Item;

/// The height a row is taken to be before any was measured.
const ESTIMATE: f32 = 32.0;

pub fn view(channels: &Channels, motion: Motion) -> impl View<Intent> + '_ {
    let all = Rc::new(items::items(channels));
    let rows = &channels.rows;
    let (keys, built) = (all.clone(), all.clone());
    let (picked, dropped, told) = (all.clone(), all.clone(), all.clone());
    let (grounds, copies) = (all.clone(), all.clone());
    let list = list(
        all.len(),
        move |at| items::key(rows, keys[at]),
        move |at| item(channels, &built, at, motion),
        motion,
    )
    .id(Id::from(format!("channels-{}", channels.server)))
    .estimate(ESTIMATE)
    .reorder(
        move |at| drag::may_drag(channels, picked[at]),
        move |from, to| drag::may_drop(channels, dropped[from], dropped[to]),
        move |what| drag::intent(channels, &told, what),
    )
    .drop_line(channels.moving.is_none())
    .drag_ground(move |at| drag::ground(grounds[at]))
    .drag_copy(move |at| drag::copy(channels, copies[at]));
    if !channels.rows.is_empty() || (channels.browse.is_none() && channels.empty.is_empty()) {
        return list.boxed();
    }
    // With no rows what the list would start with shows above it
    // (`ui/rail.slint:1319`).
    let mut above = Column::new();
    if let Some(browsing) = channels.browse {
        let room = items::Room {
            above: 0.0,
            under: items::GAP,
        };
        above = above.push(heading::browse(browsing, room, motion));
    }
    if !channels.empty.is_empty() {
        // `Label` 8 px in, in the list's 8 (`:1335`).
        let inset = 2.0 * metrics::SPACE_S;
        let padded = container(typography::label(channels.empty.as_str()))
            .padding(Padding::new(metrics::SPACE_S).left(inset).right(inset))
            .width(Fill);
        above = above.push(padded.boxed());
    }
    column![above, list].width(Fill).height(Fill).boxed()
}

/// Item `at` of the list.
fn item<'a>(
    channels: &'a Channels,
    all: &[Item],
    at: usize,
    motion: Motion,
) -> Element<'a, Intent> {
    let rows = &channels.rows;
    let room = items::room(rows, all, at);
    match all[at] {
        Item::Browse => heading::browse(channels.browse.unwrap_or(false), room, motion),
        Item::Divider(_) => heading::divider(room),
        Item::Pinned(r) => heading::pinned(&rows[r], room),
        Item::Heading(r) => heading::category(&rows[r], room, motion),
        Item::Channel(r) => channel::view(channels, r, room, motion),
        Item::Person(r, p) => person::view(channels, r, p, room, motion),
    }
}
