//! A gift, such as Nitro, that a clicked gift link opened: what it grants
//! shows before anyone redeems it, as Discord's gift dialog does. The Slint
//! side: `GiftDialog` (`ui/server.slint:311`).

use anomi_theme::prelude::*;
use anomi_vm::places::gift::{Gift, Intent};

use super::invite::frame::{buttons, confirm, dialog, failure, found, leave, note};

/// The card's width at least (`:322`).
const WIDTH: f32 = 440.0;

pub fn view(gift: &Gift, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts: Vec<Element<'_, Intent>> = Vec::new();
    if let Some(shown) = &gift.found {
        parts.push(found(None, &shown.name, &[&shown.detail]));
    }
    if !gift.status.is_empty() {
        parts.push(note(&gift.status));
    }
    if !gift.error.is_empty() {
        parts.push(failure(&gift.error));
    }
    let mut ends = vec![leave(gift.confirm.is_empty(), Intent::Close, motion)];
    if !gift.confirm.is_empty() {
        ends.push(confirm(&gift.confirm, gift.busy, gift.confirmed(), motion));
    }
    parts.push(buttons(ends));
    dialog(&gift.title, WIDTH, Intent::Close, motion, parts)
}

#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
