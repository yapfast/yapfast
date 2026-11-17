//! A scheduled event a clicked event link opened, as Discord's event dialog
//! shows it, with its Interested button. The Slint side: `EventDialog`
//! (`ui/server.slint:237`).

use anomi_theme::prelude::*;
use anomi_vm::places::event::{Event, Intent};

use super::invite::frame::{body, buttons, confirm, dialog, failure, found, leave, note};

/// The card's width at least (`:247`).
const WIDTH: f32 = 440.0;

pub fn view(event: &Event, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts: Vec<Element<'_, Intent>> = Vec::new();
    if let Some(shown) = &event.found {
        parts.push(found(None, &shown.name, &[&shown.when, &shown.detail]));
    }
    if !event.description.is_empty() {
        parts.push(body(&event.description));
    }
    if !event.status.is_empty() {
        parts.push(note(&event.status));
    }
    if !event.error.is_empty() {
        parts.push(failure(&event.error));
    }
    let mut ends = vec![leave(event.confirm.is_empty(), Intent::Close, motion)];
    if !event.confirm.is_empty() {
        ends.push(confirm(
            &event.confirm,
            event.busy,
            Intent::ToggleInterest,
            motion,
        ));
    }
    parts.push(buttons(ends));
    dialog(&event.title, WIDTH, Intent::Close, motion, parts)
}

#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
