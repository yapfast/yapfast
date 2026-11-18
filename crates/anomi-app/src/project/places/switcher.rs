//! The quick switcher from the core's `switcher` and `switcher_list`
//! (`src/ui/refresh.rs:3694`): its places are listed again only when the
//! query, the lists, the places' uses, the open conversation or the
//! pictures changed (`SwitcherShown`), so the view, the arrows and Enter
//! all read the one list.

use std::collections::{HashMap, HashSet};

use yapfast_core::application::{AppState, Place as Found};
use yapfast_core::chat::{ChatRoom, ChatServer};
use yapfast_lang::tr;

use anomi_vm::icon::Icon;
use anomi_vm::kind::count_badge;
use anomi_vm::shell::{Layer, Shell};
use anomi_vm::switcher::{Mark, Place, Switcher};

use super::{follow, set};
use crate::pictures::Requests;
use crate::project::rows;

/// What the switcher listed last.
#[derive(Default)]
pub(super) struct Shown {
    key: Option<Key>,
}

#[derive(PartialEq)]
struct Key {
    query: String,
    lists: u64,
    used: u64,
    selected: Option<String>,
    pictures: u64,
}

pub(super) fn show(
    state: &AppState,
    shell: &mut Shell,
    switcher: &mut Switcher,
    shown: &mut Shown,
    requests: &mut Requests,
    focus: &mut Option<&'static str>,
) {
    if follow(shell, Layer::Switcher, state.switcher.is_some()) {
        // It opens on the first place, its field taking the keys
        // (`init => self.focus-input()`).
        switcher.chosen = 0;
        *focus = Some(anomi_screens::switcher::FIELD);
    }
    let Some(query) = &state.switcher else {
        shown.key = None;
        return;
    };
    let key = Key {
        query: query.clone(),
        lists: state.list_changes,
        used: state.places_used.changes(),
        selected: state.selected.clone(),
        pictures: state.picture_changes,
    };
    if shown.key.as_ref() == Some(&key) {
        return;
    }
    set(&mut switcher.query, query);
    let (found, mentions) = state.switcher_list();
    switcher.places = places(state, &found, requests);
    switcher.mentions = mentions;
    shown.key = Some(key);
}

/// The places as rows, their pictures wanted.
fn places(state: &AppState, found: &[Found<'_>], requests: &mut Requests) -> Vec<Place> {
    let servers = servers_of(state, found);
    requests.want(
        state,
        found.iter().filter_map(|place| match place {
            Found::Room(room) if room.direct => room.avatar.as_deref(),
            Found::Room(_) => None,
            Found::Server(server) => server.icon.as_deref(),
        }),
    );
    found
        .iter()
        .map(|place| match place {
            Found::Room(room) => self::room(state, room, servers.get(room.id.as_str()).copied()),
            Found::Server(server) => Place {
                id: server.id.clone(),
                name: server.name.clone(),
                place: tr!("Server").to_owned(),
                mark: Mark::Avatar(rows::avatar(
                    state,
                    &server.name,
                    &server.id,
                    server.icon.as_deref(),
                )),
                ..Place::default()
            },
        })
        .collect()
}

/// The server each listed channel is in, found in one pass over the
/// servers' channels.
fn servers_of<'a>(state: &'a AppState, found: &[Found<'a>]) -> HashMap<&'a str, &'a ChatServer> {
    let mut wanted: HashMap<&str, &ChatServer> = HashMap::new();
    let channels: HashSet<&str> = found
        .iter()
        .filter_map(|place| match place {
            Found::Room(room) if !room.direct => Some(room.id.as_str()),
            _ => None,
        })
        .collect();
    if channels.is_empty() {
        return wanted;
    }
    for server in &state.servers {
        for channel in &server.channels {
            if let Some(&listed) = channels.get(channel.as_str()) {
                wanted.insert(listed, server);
            }
        }
    }
    wanted
}

/// A conversation as the switcher lists it (`place_row`): a DM by its
/// picture, a channel by its kind's icon, with where it is.
fn room(state: &AppState, room: &ChatRoom, server: Option<&ChatServer>) -> Place {
    let place = match server {
        _ if room.direct => tr!("Direct message").to_owned(),
        Some(server) => server.name.clone(),
        None => tr!("Channel").to_owned(),
    };
    let mark = if room.direct {
        Mark::Avatar(rows::avatar(
            state,
            &room.name,
            &room.id,
            room.avatar.as_deref(),
        ))
    } else if room.locked {
        Mark::Icon(Icon::Lock)
    } else {
        Mark::Icon(rows::channel_icon(room))
    };
    Place {
        id: room.id.clone(),
        name: room.name.clone(),
        place,
        mark,
        unread: room.unread > 0 && !room.muted,
        badge: count_badge(room.mentions),
    }
}
