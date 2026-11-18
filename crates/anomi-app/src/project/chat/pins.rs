//! The open conversation's pinned messages from the core's state, the
//! latest pinned first, as the pins popover shows them (`show_dm_tools`,
//! `src/ui/refresh.rs:2302`-`:2326`, over `show_found`, `:5530`): what to
//! say while they load, why they didn't, or that there are none, and each
//! as a found message's card (`search::found`). A pin edited since shows
//! again: a row's key is what its message shows, not only its id.
//!
//! The popover has no unpin of its own, as Slint's; pinning a message
//! elsewhere shows here as the core lists the pins again. Its rows go once
//! the popover played its way out ([`gone`]).

use yapfast_core::application::AppState;
use yapfast_lang::tr;

use anomi_vm::chat::pins::Pins;

use super::search::found::{Built, Entry};
use crate::layout::Layout;
use crate::pictures::Requests;

/// What the popover showed last.
#[derive(Default)]
pub(crate) struct Shown {
    rows: Built,
}

pub(crate) fn show(
    state: &AppState,
    layout: &Layout,
    pins: &mut Pins,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let selected = state.selected.as_deref();
    // Closed: what it showed stays while it plays its way out.
    let Some(open) = (state.pins.as_ref()).filter(|pins| Some(pins.room.as_str()) == selected)
    else {
        return;
    };
    let note = match &open.messages {
        None => tr!("Loading pinned messages..."),
        Some(Err(error)) => error.as_str(),
        Some(Ok(pinned)) if pinned.is_empty() => {
            if state.selected_room().is_some_and(|room| room.direct) {
                tr!("This conversation doesn't have any pinned messages yet.")
            } else {
                tr!("This channel doesn't have any pinned messages yet.")
            }
        }
        Some(Ok(_)) => "",
    };
    if pins.note != note {
        note.clone_into(&mut pins.note);
    }
    let pinned = match &open.messages {
        Some(Ok(pinned)) => pinned.as_slice(),
        _ => &[],
    };
    let entries = pinned.iter().map(|message| Entry {
        message,
        room: &open.room,
        heading: "",
    });
    let twelve_hour = layout.twelve_hour_clock;
    (shown.rows).show(state, twelve_hour, entries, &mut pins.pins, requests);
}

/// The popover played its way out: its rows go.
pub(crate) fn gone(pins: &mut Pins, shown: &mut Shown) {
    *pins = Pins::default();
    *shown = Shown::default();
}
