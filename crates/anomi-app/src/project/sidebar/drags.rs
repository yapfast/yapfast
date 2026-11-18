//! What's dragged in a server's channel list: a channel or a category to a
//! new place (`show_arranging`, `src/ui/refresh.rs:1605`), or someone in a
//! voice channel onto another (`show_moving_person`, `:1575`). The intents
//! start and end it (`intent::sidebar`); each refresh shows it on top of the
//! rows, so it follows them as they're built again, and stops once what's
//! dragged left the list.

use std::collections::HashMap;

use yapfast_core::application::{AppState, Application};

use anomi_vm::sidebar::channels::{Arranging, Channels, Moving};

/// What's being dragged, by room id.
#[derive(Debug, Default, PartialEq)]
pub struct Drags {
    /// The channel or category being arranged.
    pub arranging: Option<String>,
    /// Someone dragged from a voice channel: its room id and their user id.
    pub moving: Option<(String, String)>,
    /// The voice channel the pointer is over while they are.
    pub over: Option<String>,
}

impl Drags {
    /// Ends dragging someone.
    pub fn stop_moving(&mut self) {
        self.moving = None;
        self.over = None;
    }
}

/// Shows what's dragged on the rows, stopping what left the list; whether
/// that changed them.
pub(super) fn show(
    core: &Application,
    places: &[String],
    drags: &mut Drags,
    channels: &mut Channels,
) -> bool {
    let arranging = arranging(core, places, drags);
    let moving = moving(core, drags, channels);
    let changed = channels.arranging != arranging || channels.moving != moving;
    channels.arranging = arranging;
    channels.moving = moving;
    changed
}

fn arranging(core: &Application, places: &[String], drags: &mut Drags) -> Option<Box<Arranging>> {
    let room = drags.arranging.as_deref()?;
    let dragged = places
        .iter()
        .position(|place| place == room)
        .filter(|_| core.draggable(room));
    let Some(dragged) = dragged else {
        drags.arranging = None;
        return None;
    };
    Some(Box::new(Arranging {
        place: dragged as u32 + 1,
        droppable: core.droppable(places, dragged),
    }))
}

/// The voice channels the person dragged may drop on: those their menu's
/// Move to offers, among the rows shown. It stops once there are none, as
/// when they left the channel or another server's list shows.
fn moving(core: &Application, drags: &mut Drags, channels: &Channels) -> Option<Box<Moving>> {
    let (place, user) = drags.moving.as_ref()?;
    let targets = core.move_targets(place, user);
    let droppable: Vec<String> = channels
        .rows
        .iter()
        .filter(|row| !row.hidden && targets.iter().any(|(room, _)| *room == row.id))
        .map(|row| row.id.clone())
        .collect();
    if droppable.is_empty() {
        drags.stop_moving();
        return None;
    }
    Some(Box::new(Moving {
        over: drags.over.clone(),
        droppable,
    }))
}

/// Whether this account may drag each channel and category, kept while the
/// server and its roles are the same: the core works each out from the
/// whole server, and the list is built again on every message.
#[derive(Default)]
pub(super) struct Rights {
    of: Option<(String, u64, bool)>,
    known: HashMap<String, bool>,
}

impl Rights {
    /// Forgets what it knew once the server, its categories and roles, or
    /// Discord's reach changed.
    pub fn follow(&mut self, server: &str, state: &AppState) {
        let of = (
            server.to_owned(),
            state.category_changes,
            state.discord_online,
        );
        if self.of.as_ref() != Some(&of) {
            self.of = Some(of);
            self.known.clear();
        }
    }

    pub fn may_drag(&mut self, core: &Application, room: &str) -> bool {
        if let Some(&known) = self.known.get(room) {
            return known;
        }
        let may = core.draggable(room);
        self.known.insert(room.to_owned(), may);
        may
    }
}
