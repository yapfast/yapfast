//! The call's tiles from the core's call: this account's shared screen, the
//! streams of those who film too, then everyone (`ParticipantGrid`'s three
//! loops, `ui/voice.slint:988`-`:1116`, over `show_voice`'s rows,
//! `src/ui/refresh.rs:4650`).
//!
//! A tile is built again only when what it reads changed: its participant
//! (compared with the list shown before, which the core replaces whole), or
//! what it reads beside them (the stream watched, a preview, its picture,
//! whether its camera's first frame came), kept as a hash per tile. So
//! someone muting or starting to speak builds their tile alone. A camera's
//! or a stream's frames reach its tile through one `Live` per feed, kept
//! while the feed lasts; the tile holds it once its first frame came.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use yapfast_core::application::AppState;
use yapfast_core::voice::{Participant, SharePhase};
use yapfast_lang::tr;

use anomi_vm::kind::Live;
use anomi_vm::voice::tiles::{Chip, Fill, Key, Mark, Outline, Tile, Viewers};

use super::focus::Stage;
use super::lives::Lives;
use crate::pictures::Requests;
use crate::project::rows;

/// What the tiles showed: each with the hash of what it read beside its
/// participant, and the participants they were built from.
#[derive(Default)]
pub struct Shown {
    built: HashMap<Key, (u64, Tile)>,
    participants: Arc<[Participant]>,
    /// How many tiles were built, for the tests.
    #[cfg(test)]
    pub builds: usize,
}

/// The tiles of the call, in their slots; built again only where changed.
pub fn tiles(
    state: &AppState,
    stage: Stage<'_>,
    lives: &Lives,
    shown: &mut Shown,
    requests: &mut Requests,
) -> Vec<Tile> {
    let voice = &state.voice;
    let watching = voice.watching.as_deref();
    let mut built = std::mem::take(&mut shown.built);
    let mut builds = 0;
    let mut take =
        |key: Key, context: u64, same: bool, build: &dyn Fn() -> Tile| match built.remove(&key) {
            Some((kept, tile)) if same && kept == context => (key, context, tile),
            _ => {
                builds += 1;
                (key, context, build())
            }
        };
    let mut made = Vec::with_capacity(stage.before() + stage.participants.len());
    if let Some(sharing) = &voice.sharing {
        let live = lives.own.as_ref().map(|own| &own.1);
        let context = hash((
            sharing.phase as u8,
            live.is_some_and(Live::has_frame),
            &sharing.viewers,
            state.picture_changes,
        ));
        made.push(take(Key::OwnScreen, context, true, &|| {
            own_screen(state, stage.participants, live)
        }));
    }
    for index in stage.streams() {
        let person = &stage.participants[index];
        let watched = watching.filter(|watching| watching.user == person.id);
        // The preview comes again after watching stops.
        let context = hash((
            watch_context(state, watched, lives),
            &name(person),
            state.stream_previews.get(&person.id).map(Arc::as_ptr),
        ));
        let key = Key::Stream(person.id.clone());
        made.push(take(key, context, true, &|| {
            stream(state, person, watched, lives)
        }));
    }
    for person in stage.participants {
        let before = shown.participants.iter().find(|old| old.id == person.id);
        let same = before == Some(person);
        let watched = watching.filter(|watching| watching.user == person.id);
        let context = hash((
            watch_context(state, watched, lives),
            state.stream_previews.get(&person.id).map(Arc::as_ptr),
            picture_at(state, person.avatar.as_deref()),
            lives.camera(&person.id).is_some_and(Live::has_frame),
            person.local && voice.camera_phase == yapfast_core::voice::CameraPhase::Starting,
        ));
        let key = Key::Person(person.id.clone());
        made.push(take(key, context, same, &|| {
            participant(state, person, watched, lives)
        }));
    }
    for person in stage.participants {
        if let Some(avatar) = person.avatar.as_deref() {
            requests.want(state, [avatar]);
        }
    }
    #[cfg(test)]
    {
        shown.builds += builds;
    }
    let _ = builds;
    // The core's list itself, shared: no participant is copied.
    shown.participants = if voice.active() {
        voice.participants.clone()
    } else {
        Arc::default()
    };
    let mut tiles = Vec::with_capacity(made.len());
    for (key, context, tile) in made {
        tiles.push(tile.clone());
        shown.built.insert(key, (context, tile));
    }
    tiles
}

/// Forgets every tile, as the call ends or the window goes.
pub fn forget(shown: &mut Shown) {
    *shown = Shown::default();
}

/// What a tile showing `watched` reads of it: whether it's watched, its
/// first frame came, who watches it.
fn watch_context(
    state: &AppState,
    watched: Option<&yapfast_core::voice::Watching>,
    lives: &Lives,
) -> u64 {
    let Some(watched) = watched else {
        return 0;
    };
    hash((
        &watched.user,
        watched.frames.is_some(),
        lives
            .watched
            .as_ref()
            .is_some_and(|live| live.1.has_frame()),
        &watched.viewers,
        state.picture_changes,
    ))
}

fn hash(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// The core's picture at `address`, as a number that changes when it comes.
fn picture_at(state: &AppState, address: Option<&str>) -> usize {
    address
        .and_then(|address| state.pictures.get(address))
        .map_or(0, |picture| Arc::as_ptr(picture).addr())
}

/// A participant's name as their row shows it.
fn name(person: &Participant) -> &str {
    crate::project::account::display_part(&person.user)
}

/// This account's shared screen (`:988`).
fn own_screen(state: &AppState, participants: &[Participant], live: Option<&Live>) -> Tile {
    let sharing = state.voice.sharing.as_ref();
    let phase = sharing.map(|sharing| sharing.phase);
    let is_live = phase == Some(SharePhase::Live);
    let frame = live.filter(|live| live.has_frame()).cloned();
    let status = share_status(phase).to_owned();
    let note = if !is_live && frame.is_some() {
        status.clone()
    } else {
        String::new()
    };
    let ids = sharing.map_or(&[][..], |sharing| &sharing.viewers);
    Tile {
        key: Key::OwnScreen,
        fill: Fill::OwnScreen { frame, status },
        chip: Chip {
            name: tr!("Your screen").into(),
            live: is_live,
            note,
            ..Chip::default()
        },
        viewers: viewers(state, participants, ids, None),
        ..Tile::default()
    }
}

/// How this account's stream goes, as the dock and its tile say it.
pub fn share_status(phase: Option<SharePhase>) -> &'static str {
    match phase {
        Some(SharePhase::Choosing) => tr!("Choose what to share"),
        Some(SharePhase::Starting) => tr!("Starting your stream..."),
        Some(SharePhase::Live) => tr!("Live"),
        None => "",
    }
}

/// What a stream watched says until its first picture.
pub fn watch_status(watched: &yapfast_core::voice::Watching) -> String {
    if watched.frames.is_none() {
        tr!("Connecting to {}'s stream...", watched.name)
    } else {
        tr!("Waiting for {}'s picture...", watched.name)
    }
}

/// The stream watched, in a tile of its own or in theirs.
fn watched_fill(watched: Option<&yapfast_core::voice::Watching>, lives: &Lives) -> Option<Fill> {
    let watched = watched?;
    let frame = lives.watched.as_ref().map(|live| &live.1);
    Some(Fill::Watched {
        frame: frame.filter(|live| live.has_frame()).cloned(),
        status: watch_status(watched),
    })
}

/// Someone's stream in a tile of its own, beside their camera's (`:1008`).
fn stream(
    state: &AppState,
    person: &Participant,
    watched: Option<&yapfast_core::voice::Watching>,
    lives: &Lives,
) -> Tile {
    let preview = state
        .stream_previews
        .get(&person.id)
        .and_then(crate::pictures::shown);
    let fill = watched_fill(watched, lives).unwrap_or(Fill::Offer { preview });
    Tile {
        key: Key::Stream(person.id.clone()),
        watch: watched.is_none(),
        viewers: watched.map_or_else(Viewers::default, |watched| {
            viewers(
                state,
                &state.voice.participants,
                &watched.viewers,
                Some(&person.id),
            )
        }),
        fill,
        chip: Chip {
            name: name(person).to_owned(),
            mark: Some(Mark::Screen),
            live: true,
            note: String::new(),
        },
        ..Tile::default()
    }
}

/// Someone's own tile (`:1051`): their camera, the stream they share
/// without one, its preview, or their picture.
fn participant(
    state: &AppState,
    person: &Participant,
    watched: Option<&yapfast_core::voice::Watching>,
    lives: &Lives,
) -> Tile {
    let streams_here = person.streaming && !person.local && person.video.is_none();
    let watched = watched.filter(|_| streams_here);
    let camera = lives
        .camera(&person.id)
        .filter(|live| person.video.is_some() && live.has_frame());
    let preview = state
        .stream_previews
        .get(&person.id)
        .filter(|_| streams_here && watched.is_none())
        .and_then(crate::pictures::shown);
    let fill = if let Some(live) = camera {
        Fill::Camera(live.clone())
    } else if let Some(fill) = watched_fill(watched, lives) {
        fill
    } else if let Some(preview) = preview {
        Fill::Preview(preview)
    } else {
        let key = if person.local {
            state.account.as_deref().unwrap_or("Preview")
        } else {
            name(person)
        };
        Fill::Avatar {
            avatar: rows::avatar(state, name(person), key, person.avatar.as_deref()),
            connected: person.connected,
        }
    };
    let speaking = person.speaking && !person.muted && person.connected;
    let mark = if person.deafened {
        Some(Mark::Deafened)
    } else if person.muted {
        Some(Mark::Muted)
    } else {
        None
    };
    let note = match &person.problem {
        Some(problem) => problem.clone(),
        None if !person.connected => tr!("Connection lost").into(),
        None => String::new(),
    };
    let starting =
        person.local && state.voice.camera_phase == yapfast_core::voice::CameraPhase::Starting;
    Tile {
        key: Key::Person(person.id.clone()),
        fill,
        outline: if speaking {
            Outline::Speaking
        } else {
            Outline::Line
        },
        chip: Chip {
            name: name(person).to_owned(),
            mark,
            live: person.streaming && (person.local || person.video.is_none()),
            note,
        },
        viewers: watched.map_or_else(Viewers::default, |watched| {
            viewers(
                state,
                &state.voice.participants,
                &watched.viewers,
                Some(&person.id),
            )
        }),
        watch: streams_here && watched.is_none(),
        starting,
        ..Tile::default()
    }
}

/// Who watches a stream, in the call's order, its streamer left out: the
/// first three's pictures, how many, and their names for the tooltip.
pub fn viewers(
    state: &AppState,
    participants: &[Participant],
    ids: &[String],
    streamer: Option<&str>,
) -> Viewers {
    let watching: Vec<&Participant> = participants
        .iter()
        .filter(|person| Some(person.id.as_str()) != streamer && ids.contains(&person.id))
        .collect();
    if watching.is_empty() {
        return Viewers::default();
    }
    let names: Vec<&str> = watching.iter().map(|person| name(person)).collect();
    Viewers {
        faces: watching
            .iter()
            .take(Viewers::FACES)
            .map(|person| rows::avatar(state, name(person), name(person), person.avatar.as_deref()))
            .collect(),
        count: watching.len(),
        names: tr!("Watching: {}", names.join(", ")),
    }
}
