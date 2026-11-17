//! The Activity panel above the member list: a header ("Activity — 5", an
//! eye to hide the cards, a chevron to open past the first three) and a card
//! for each thing someone is doing or did lately, with its time. The Slint
//! side: the panel at the top of `MemberPanel` (`ui/people.slint:130`-`:196`)
//! with `ActivityPicture` (`:64`), filled by `activity_panel` in
//! `src/ui/profile.rs:53`. Task P06 (docs/iced/tasks/people.md).
//!
//! - [`view`] is the panel: nothing without cards, the header alone while
//!   they are hidden.
//! - [`card`] is one activity as a profile shows it (`ActivityCard`,
//!   `:780`), drawn by the profile card, the full profile's tab and the
//!   Active Now column's owners.
//!
//! The panel's times are words the projection makes and brings up to date
//! once a minute while the window is in front (`Activity::wants_clock`);
//! a card's are counted here from the `now` it is given, which the app moves
//! each second under the same rule (`card::wants_clock`). Nothing here
//! ticks.

pub mod card;
mod header;
mod picture;
mod row;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::{Column, container};
use iced::{Fill, Padding};

use anomi_theme::prelude::*;
use anomi_vm::people::activity::{Activity, Intent};

#[cfg(feature = "shots")]
pub use shots::shots;

/// The panel's padding, around its header and cards.
const PADDING: Padding = Padding {
    top: 8.0,
    right: 8.0,
    bottom: 4.0,
    left: 8.0,
};

/// The panel, as wide as it is given and as tall as its cards.
pub fn view(activity: &Activity, motion: Motion) -> impl View<Intent> + '_ {
    let mut panel = Column::<Element<'_, Intent>>::new();
    if !activity.rows.is_empty() {
        panel = panel.push(header::view(activity, motion));
        // Each card keeps 8 px beneath it; the ones not showing are not
        // made, and their pictures not asked for.
        for row in activity.rows.iter().take(activity.showing()) {
            panel = panel.push(row::view(row, motion));
        }
    }
    container(panel)
        .width(Fill)
        .padding(if activity.rows.is_empty() {
            Padding::ZERO
        } else {
            PADDING
        })
}
