//! Searching messages from the core's state: the header's field, which
//! names where it looks (`search-placeholder`, `show_dm_tools`,
//! `src/ui/refresh.rs:2329`), and the panel of what it found (`show_hits`,
//! `:5563`): how many, why none, the results newest first with the
//! channel's name above a run in one channel of a server, "Show more".
//!
//! The field's text is the app's: typing changes it alone and shows
//! nothing again (`intent::chat::places::search`); Enter hands it to the
//! core, which drops the pages of an older search. The results hold
//! nothing once the search ends.

pub(crate) mod found;

use yapfast_core::application::AppState;
use yapfast_core::discord;
use yapfast_lang::{tr, trn};

use anomi_vm::chat::search::Search;

use crate::layout::Layout;
use crate::pictures::Requests;

/// What the panel showed last.
#[derive(Default)]
pub(crate) struct Shown {
    results: found::Built,
    /// A search showed, and the core's count of rooms opened then: the
    /// field's words go with the search, and with the place it looked in.
    open: bool,
    selection: u64,
    /// What the placeholder was made from: the room opened and the list.
    named: Option<(u64, u64)>,
}

pub(crate) fn show(
    state: &AppState,
    layout: &Layout,
    search: &mut Search,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let named = (state.selection_version, state.list_changes);
    if shown.named != Some(named) {
        shown.named = Some(named);
        set(&mut search.placeholder, placeholder(state));
    }
    let open = state.search_shown();
    // Ended here, or left with the place: its words go too. A server's
    // search stays, with its words, in the server's channels.
    let moved = shown.selection != state.selection_version;
    if open.is_none() && (shown.open || moved) && !search.focused && !search.text.is_empty() {
        search.text.clear();
    }
    shown.open = open.is_some();
    shown.selection = state.selection_version;
    let Some(open) = open else {
        if !search.results.is_empty() || !search.heading.is_empty() {
            let kept = (std::mem::take(&mut search.text), search.focused);
            *search = Search {
                placeholder: std::mem::take(&mut search.placeholder),
                ..Search::default()
            };
            (search.text, search.focused) = kept;
            shown.results = found::Built::default();
        }
        return;
    };
    let server = discord::parse_server(&open.place).is_some();
    search.loading = open.loading;
    search.more = open.more();
    let heading = if open.results.is_empty() && open.loading {
        tr!("Searching...").to_owned()
    } else if open.results.is_empty() && open.error.is_some() {
        tr!("Search").to_owned()
    } else {
        trn!("{n} Result", "{n} Results", open.total)
    };
    set(&mut search.heading, heading);
    let note = match &open.error {
        Some(error) => error.as_str(),
        None if !open.loading && open.results.is_empty() => {
            if server {
                tr!("Nothing in this server matches. Try other words.")
            } else {
                tr!("Nothing in this conversation matches. Try other words.")
            }
        }
        None => "",
    };
    if search.note != note {
        note.clone_into(&mut search.note);
    }
    let hits = &open.results;
    let entries = hits.iter().enumerate().map(|(index, hit)| {
        let starts = index == 0 || hits[index - 1].room != hit.room;
        let heading = (state.rooms.iter())
            .find(|room| room.id == hit.room)
            .filter(|_| server && starts)
            .map_or("", |room| room.name.as_str());
        found::Entry {
            message: &hit.message,
            room: &hit.room,
            heading,
        }
    });
    let twelve_hour = layout.twelve_hour_clock;
    (shown.results).show(state, twelve_hour, entries, &mut search.results, requests);
}

/// What the field says while empty: where it looks, the DM or the
/// channel's server (`Search {}`).
fn placeholder(state: &AppState) -> String {
    let place = state.search_place();
    let looks_in = place
        .as_deref()
        .and_then(|place| state.servers.iter().find(|server| server.id == place))
        .map(|server| server.name.as_str())
        .or(state.selected_room().map(|room| room.name.as_str()))
        .unwrap_or_default();
    tr!("Search {}", looks_in)
}

/// Sets `field` to `value` unless it holds it already.
fn set(field: &mut String, value: String) {
    if *field != value {
        *field = value;
    }
}
