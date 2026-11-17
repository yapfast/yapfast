//! The DM home's list: Friends and Requests, the "Direct messages" heading
//! with its buttons, then the DMs, pinned first. The Slint side: `DmHeading`
//! in `ui/yapfast/dm-heading.slint`, `DmScreenLinks` and the DM rows of
//! `RoomList` in `ui/rail.slint`.
//!
//! It is one virtual list, so only the rows near the view are built however
//! many DMs there are; the links and the heading scroll away with it, as
//! they do in the Slint list. The list ends 12 px under the last row
//! (`ui/rail.slint:1344`).

mod dm_row;
mod gutter;
mod lines;
mod look;
mod nav;
#[cfg(feature = "shots")]
mod shots;

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::Padding;
use iced::widget::container;

use anomi_theme::prelude::*;
use anomi_vm::sidebar::dms::{Dms, Intent};
use anomi_widgets::icon::Icon;
use anomi_widgets::list::list;
use yapfast_lang::tr;

#[cfg(feature = "shots")]
pub use shots::shots;

/// The space between two rows, and the room the links keep above and below
/// them (`ui/rail.slint:1122`).
const SPACING: f32 = 2.0;
const LINKS: f32 = 6.0;

/// What comes before the DMs.
#[derive(Clone, Copy)]
enum Lead {
    Friends,
    Requests,
    Heading,
    Empty,
}

/// The items before the first DM, and how many.
fn leading(dms: &Dms) -> ([Option<Lead>; 4], usize) {
    let screens = !dms.no_discord && !dms.preview;
    let mut items = [None; 4];
    let mut count = 0;
    let mut push = |item| {
        items[count] = Some(item);
        count += 1;
    };
    if screens {
        push(Lead::Friends);
        push(Lead::Requests);
    }
    if !dms.preview {
        push(Lead::Heading);
    }
    if dms.rows.is_empty() && dms.empty != anomi_vm::sidebar::dms::Empty::None {
        push(Lead::Empty);
    }
    (items, count)
}

/// An item's identity, which stays with it as others come and go.
fn key(id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

/// Room under the last row (ui/rail.slint:1344, `padding-bottom` of the last).
const FOOT: f32 = 12.0;

pub fn view(dms: &Dms, motion: Motion) -> impl View<Intent> + '_ {
    let count = leading(dms).1 + dms.rows.len();
    list(
        count,
        move |at| {
            let (items, lead) = leading(dms);
            match items.get(at).copied().flatten() {
                Some(Lead::Friends) => 1,
                Some(Lead::Requests) => 2,
                Some(Lead::Heading) => 3,
                Some(Lead::Empty) => 4,
                None => key(&dms.rows[at - lead].id),
            }
        },
        move |at| item(dms, at, motion),
        motion,
    )
    .spacing(SPACING)
    .padding(Padding::ZERO.bottom(FOOT))
}

/// Item `at` of the list.
fn item<'a>(dms: &'a Dms, at: usize, motion: Motion) -> Element<'a, Intent> {
    let (items, lead) = leading(dms);
    match items.get(at).copied().flatten() {
        Some(Lead::Friends) => container(nav::link(
            Icon::Users,
            tr!("Friends"),
            "",
            dms.friends_selected,
            Intent::Friends,
        ))
        .padding(Padding::ZERO.top(LINKS))
        .boxed(),
        Some(Lead::Requests) => container(nav::link(
            Icon::Inbox,
            tr!("Requests"),
            &dms.requests_badge,
            dms.requests_selected,
            Intent::Requests,
        ))
        .padding(Padding::ZERO.bottom(LINKS))
        .boxed(),
        Some(Lead::Heading) => nav::heading(!dms.no_discord, motion),
        Some(Lead::Empty) => nav::empty(dms.empty).unwrap_or_else(|| container("").boxed()),
        None => dm_row::view(&dms.rows[at - lead], motion),
    }
}
