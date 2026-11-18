//! The sidebar: the DM list or a server's header and channels, and a
//! Community server's Browse Channels; `show_sidebar` in
//! `src/ui/refresh.rs:1059`, `browse_rows` (`:124`), `show_moving_person`
//! and `show_arranging` (`:1575`, `:1605`).
//!
//! - The rows are built again only when what they show changed ([`Key`]).
//! - Who is in the call's own channel follows the call, several times a
//!   second as people speak: [`people::follow`] puts them on that one row,
//!   and nothing else is built again.
//! - What is dragged in the channel list ([`drags::Drags`]) is the app's,
//!   shown on top of the rows each time.
//! - The pictures the DM rows show are asked for as the list reports the
//!   rows in view ([`seen`]); the core leaves list pictures to the
//!   interface. The server's banner is asked for as the header shows it.

#![allow(clippy::needless_update)]

mod browse;
mod channels;
mod dms;
pub mod drags;
mod fold;
mod header;
mod people;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use yapfast_core::application::{AppState, Application, Cosmetics, DmScreen, Navigation};

use anomi_vm::browse::Browse;
use anomi_vm::sidebar::{Place, Server, Sidebar};

use crate::layout::Layout;
use crate::pictures::Requests;

pub use browse::Shown as BrowseShown;

/// What the sidebar was built from, the rows last in view, and what's
/// dragged.
#[derive(Default)]
pub struct Shown {
    key: Option<Key>,
    /// The DM list's rows in view, by room id.
    seen: Vec<String>,
    /// The room ids the channel list's places stand for, from the first.
    places: Vec<String>,
    pub drags: drags::Drags,
    rights: drags::Rights,
}

/// Everything the rows read but the call's people, which
/// [`people::follow`] keeps.
#[derive(PartialEq)]
struct Key {
    lists: u64,
    pictures: u64,
    categories: u64,
    navigation: Navigation,
    selected: Option<String>,
    screen: Option<DmScreen>,
    pinned: Vec<String>,
    /// The friend requests, which Requests' badge counts, by their hash.
    requests: u64,
    filter: String,
    online: bool,
    linked: bool,
    preview: bool,
    cosmetics: Cosmetics,
    hide_nameplates: bool,
    /// The server's folded categories.
    collapsed: Option<BTreeSet<String>>,
    /// The call's channel, which a folded category keeps showing.
    call: Option<String>,
}

impl Key {
    fn of(state: &AppState, layout: &Layout, filter: &str) -> Self {
        let collapsed = match &state.navigation {
            Navigation::Server(id) => layout.collapsed.get(id).cloned(),
            Navigation::DirectMessages => None,
        };
        Self {
            lists: state.list_changes,
            pictures: state.picture_changes,
            categories: state.category_changes,
            navigation: state.navigation.clone(),
            selected: state.selected.clone(),
            screen: state.dm_screen,
            pinned: state.pinned.clone(),
            requests: fingerprint(&state.friend_requests),
            filter: filter.to_owned(),
            online: state.discord_online,
            linked: linked(state),
            preview: state.preview,
            cosmetics: state.cosmetics,
            hide_nameplates: state.hide_nameplates,
            collapsed,
            call: super::rail::call_room(state).map(str::to_owned),
        }
    }

    /// Whether the rows would show the same, without copying anything: it
    /// runs on every refresh.
    fn matches(&self, state: &AppState, layout: &Layout, filter: &str) -> bool {
        let collapsed = match &state.navigation {
            Navigation::Server(id) => layout.collapsed.get(id),
            Navigation::DirectMessages => None,
        };
        self.lists == state.list_changes
            && self.pictures == state.picture_changes
            && self.categories == state.category_changes
            && self.navigation == state.navigation
            && self.selected == state.selected
            && self.screen == state.dm_screen
            && self.pinned == state.pinned
            && self.requests == fingerprint(&state.friend_requests)
            && self.filter == filter
            && self.online == state.discord_online
            && self.linked == linked(state)
            && self.preview == state.preview
            && self.cosmetics == state.cosmetics
            && self.hide_nameplates == state.hide_nameplates
            && self.collapsed.as_ref() == collapsed
            && self.call.as_deref() == super::rail::call_room(state)
    }

    /// The filter as the rows match it.
    fn query(&self) -> String {
        self.filter.trim().to_lowercase()
    }
}

/// A short list's hash, to tell it changed without keeping a copy.
fn fingerprint(list: &impl std::hash::Hash) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    BuildHasherDefault::<DefaultHasher>::default().hash_one(list)
}

/// A Discord account is linked, or a server of one listed.
fn linked(state: &AppState) -> bool {
    state.discord_linked || state.servers.iter().any(|server| server.discord)
}

pub(super) fn show(
    core: &Application,
    layout: &Layout,
    sidebar: &mut Sidebar,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let state = &core.state;
    let mut changed = false;
    if shown
        .key
        .as_ref()
        .is_none_or(|key| !key.matches(state, layout, &sidebar.filter))
    {
        let key = Key::of(state, layout, &sidebar.filter);
        build(core, &key, sidebar, shown, requests);
        shown.key = Some(key);
        changed = true;
    }
    if let Place::Server(server) = &mut sidebar.place {
        changed |= people::follow(core, &mut server.channels);
        changed |= drags::show(core, &shown.places, &mut shown.drags, &mut server.channels);
    }
    if changed {
        sidebar.version += 1;
    }
}

fn build(
    core: &Application,
    key: &Key,
    sidebar: &mut Sidebar,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let state = &core.state;
    shown.places.clear();
    sidebar.place = match &key.navigation {
        Navigation::DirectMessages => {
            let dms = dms::build(state, key);
            dms::seen(state, &shown.seen, requests);
            Place::Dms(dms)
        }
        Navigation::Server(id) => {
            let header = header::build(core, id);
            requests.want(state, header::banner(state, id));
            let channels = channels::build(core, id, key, &mut shown.places, &mut shown.rights);
            Place::Server(Server { header, channels })
        }
    };
}

/// The DM list's rows in view, by room id: their pictures are asked for,
/// now and as the rows are built again.
pub(super) fn seen(state: &AppState, shown: &mut Shown, ids: Vec<String>, requests: &mut Requests) {
    dms::seen(state, &ids, requests);
    shown.seen = ids;
}

/// Browse Channels' rows, while it shows.
pub(super) fn show_browse(state: &AppState, browse: &mut Browse, shown: &mut BrowseShown) {
    browse::show(state, browse, shown);
}

/// Whether a busy voice channel's row shows how long its call has gone on,
/// which a clock writes again each 15 s while the window is in front, as
/// `voice_timed` asked of ActivityClock (task I21).
pub fn timed(sidebar: &Sidebar) -> bool {
    match &sidebar.place {
        Place::Server(server) => server
            .channels
            .rows
            .iter()
            .any(|row| !row.hidden && !row.lasted.is_empty()),
        Place::Dms(_) => false,
    }
}

/// Writes the calls' lengths again for `now`, in Unix seconds; whether any
/// changed, and then the sidebar's version.
pub fn tick(state: &AppState, sidebar: &mut Sidebar, now: u64) -> bool {
    let Place::Server(server) = &mut sidebar.place else {
        return false;
    };
    let mut changed = false;
    for row in server
        .channels
        .rows
        .iter_mut()
        .filter(|row| !row.lasted.is_empty())
    {
        let started = state
            .rooms
            .iter()
            .find(|room| room.id == row.id)
            .and_then(|room| room.voice_started);
        let lasted = started.map_or_else(String::new, |started| channels::lasted(started, now));
        if row.lasted != lasted {
            row.lasted = lasted;
            changed = true;
        }
    }
    if changed {
        sidebar.version += 1;
    }
    changed
}

/// The room ids the channel list's places stand for, from the first.
pub fn places(shown: &Shown) -> &[String] {
    &shown.places
}
