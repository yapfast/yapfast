//! A profile's badges (`BadgeRows`, `ui/people.slint:1201`): twelve to a
//! row, 20px icons 2px apart, each row a pill of its own (3px by 1px inside
//! a 1px border, 8px corners, a grey wash that brightens under the pointer),
//! more going on to the next row 4px below. Six and a "+n" count is
//! Discord's badge-management experiment, off by default, and not ported.
//! One badge is [`one`], the card of a levelled one [`card`].

mod card;
/// A single badge and the pill that holds a row of them, for the full
/// profile's inline line (P07) and the name style sample (P16).
pub mod one;
pub mod pill;

use iced::widget::{column, container, row};
use iced::{Length, Padding};

use anomi_theme::prelude::*;
use anomi_vm::people::badges::{Badges, Intent};

/// Badges to a row, the space between rows and between badges.
const PER_ROW: usize = 12;
const ROWS: f32 = 4.0;
const GAP: f32 = 2.0;
/// Inside the pill's border: 1px above, 3px below (the 20px badges sit at
/// y 1 of 24, `ui/people.slint:1208`), 3px at the sides.
const PAD: Padding = Padding {
    top: 1.0,
    bottom: 3.0,
    left: 3.0,
    right: 3.0,
};

pub fn view(badges: &Badges, motion: Motion) -> impl View<Intent> + '_ {
    let mut rows = column![].spacing(ROWS).width(Length::Shrink);
    for chunk in badges.badges.chunks(PER_ROW) {
        let line = row(chunk.iter().map(|badge| one::view(badge, motion).boxed()))
            .spacing(GAP)
            .width(Length::Shrink);
        // The Slint pill is 22px a badge and 6px over: a badge's 2px gap
        // trails the last.
        let inside = container(line).padding(Padding {
            right: PAD.right + GAP,
            ..PAD
        });
        rows = rows.push(pill::pill(inside, motion));
    }
    rows
}

/// Its shots for `anomi-shots`: none. The badges have no window of their
/// own: they show in the profile card's shots (P07's `profile/popout_member`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
