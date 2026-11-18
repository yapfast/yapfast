//! Forwarding a message, from the core's `forwarding` (the forward dialog's
//! part of `show_dialogs`, `src/ui/refresh.rs:3614`): its layer is open
//! while the core forwards, its search takes the keys as it opens, and its
//! places, as the quick switcher lists them, are built again only when the
//! search, the choices, the lists or the pictures changed.

use yapfast_core::application::{AppState, FORWARD_LIMIT, Place as Found};
use yapfast_core::network::Network;
use yapfast_lang::tr;

use anomi_vm::chat::forward::{Forward, Target};
use anomi_vm::shell::{Layer, Shell};

use crate::pictures::Requests;
use crate::project::places::switcher;

/// What the dialog listed last.
#[derive(Default)]
pub(crate) struct Shown {
    key: Option<Key>,
}

#[derive(PartialEq)]
struct Key {
    query: String,
    chosen: Vec<String>,
    pinned: Vec<String>,
    lists: u64,
    used: u64,
    pictures: u64,
}

pub(crate) fn show(
    state: &AppState,
    shell: &mut Shell,
    forward: &mut Forward,
    shown: &mut Shown,
    requests: &mut Requests,
    focus: &mut Option<&'static str>,
) {
    let forwarding = state.forwarding.as_ref();
    match (forwarding, shell.is_open(Layer::Forward)) {
        (Some(forwarding), false) => {
            shell.open(Layer::Forward);
            // Its note starts as the core's: empty, or the one a failed try
            // sent; the search takes the keys (`init`).
            forwarding.note.clone_into(&mut forward.note);
            *focus = Some("forward-query");
        }
        (None, true) => shell.close(Layer::Forward),
        _ => {}
    }
    let Some(forwarding) = forwarding else {
        shown.key = None;
        return;
    };
    // Choosing a place clears the search.
    set(&mut forward.query, &forwarding.query);
    set(&mut forward.quote, &forwarding.quote);
    set(
        &mut forward.error,
        forwarding.error.as_deref().unwrap_or_default(),
    );
    forward.limit = FORWARD_LIMIT;
    forward.chosen = forwarding.chosen.len();
    forward.send = if forward.chosen > 1 {
        tr!("Send ({})", forward.chosen)
    } else {
        tr!("Send").to_owned()
    };
    let empty = if Network::of(&forwarding.room) == Network::Discord {
        tr!("We searched far and wide. Unfortunately, no results were found.")
    } else if !forwarding.query.is_empty() {
        tr!("No conversations match.")
    } else {
        tr!(
            "There's nowhere to forward to yet. Yapfast forwards Matrix messages only into encrypted conversations."
        )
    };
    set(&mut forward.empty, empty);
    let key = Key {
        query: forwarding.query.clone(),
        chosen: forwarding.chosen.clone(),
        pinned: forwarding.pinned.clone(),
        lists: state.list_changes,
        used: state.places_used.changes(),
        pictures: state.picture_changes,
    };
    if shown.key.as_ref() == Some(&key) {
        return;
    }
    forward.slowmode = slowmode(state, &forwarding.chosen);
    forward.targets = targets(state, &forwarding.chosen, requests);
    shown.key = Some(key);
}

/// The dialog played its way out: its places go.
pub(crate) fn gone(forward: &mut Forward, shown: &mut Shown) {
    forward.targets = Vec::new();
    shown.key = None;
}

/// The places the message may go, those chosen ticked, their pictures
/// wanted.
fn targets(state: &AppState, chosen: &[String], requests: &mut Requests) -> Vec<Target> {
    let places = state.forward_places();
    requests.want(
        state,
        places
            .iter()
            .filter(|room| room.direct)
            .filter_map(|room| room.avatar.as_deref()),
    );
    let found: Vec<Found<'_>> = places.iter().map(|&room| Found::Room(room)).collect();
    let servers = switcher::servers_of(state, &found);
    places
        .into_iter()
        .map(|room| {
            let place = switcher::room(state, room, servers.get(room.id.as_str()).copied());
            Target {
                chosen: chosen.contains(&place.id),
                id: place.id,
                name: place.name,
                place: place.place,
                mark: place.mark,
            }
        })
        .collect()
}

/// As Discord's client warns, the message added doesn't go where slowmode
/// would hold it.
fn slowmode(state: &AppState, chosen: &[String]) -> String {
    let slowed: Vec<String> = chosen
        .iter()
        .filter_map(|id| state.rooms.iter().find(|room| room.id == *id))
        .filter(|room| room.slowed)
        .map(|room| state.destination(room))
        .collect();
    match slowed.as_slice() {
        [] => String::new(),
        [one] => tr!(
            "The message you added will not be sent to slowmode channel: {}.",
            one
        ),
        many => tr!(
            "The message you added will not be sent to slowmode channels: {}.",
            many.join(", ")
        ),
    }
}

/// Sets `field` to `value` when they differ, keeping its allocation.
fn set(field: &mut String, value: &str) {
    if field != value {
        value.clone_into(field);
    }
}
