//! Joining and answering: the way into a voice channel or a DM's call, the
//! line over the stage, a call ringing in another conversation and the
//! question before switching calls (`show_voice`,
//! `src/ui/refresh.rs:4523`-`:4600`, `:4749`-`:4790`; `show_incoming`,
//! `src/ui.rs:3334`).

use yapfast_core::application::AppState;
use yapfast_core::chat::{ChatRoom, VoicePerson};
use yapfast_lang::tr;

use anomi_vm::conversation::{Call, Conversation};
use anomi_vm::voice::incoming::{Incoming, Switch};
use anomi_vm::voice::join::{self, Join};

use crate::layout::Layout;
use crate::project::rows;

/// This account is in the call of the open room.
pub fn here(state: &AppState) -> bool {
    let voice = &state.voice;
    state
        .selected_room()
        .is_some_and(|room| voice.room.as_ref() == Some(&room.id))
        && voice.active()
}

/// The open room as a Discord DM that can call.
fn dm_call(state: &AppState) -> Option<&ChatRoom> {
    state
        .selected_room()
        .filter(|room| room.direct && room.capabilities.dm_calls)
}

/// The call in the conversation, the line over its stage and the way in.
pub fn show(state: &AppState, layout: &Layout, focused: bool, conversation: &mut Conversation) {
    let voice_here = here(state);
    let room = state.selected_room();
    let panel = room.is_some_and(|room| room.voice && !state.voice_chat);
    let dm = dm_call(state);
    let shown = dm.is_some_and(|room| voice_here || !room.people.is_empty() || room.incoming);
    conversation.call = if voice_here && (panel || shown) {
        Call::Here { focused }
    } else if panel || shown {
        Call::Elsewhere
    } else {
        Call::None
    };
    let description = description(state, layout, voice_here);
    if conversation.description != description {
        conversation.description = description;
    }
}

/// The line over the stage, or what joining says (`voice-description`).
fn description(state: &AppState, layout: &Layout, voice_here: bool) -> String {
    let voice = &state.voice;
    let room = state.selected_room();
    let present = room.map_or(&[][..], |room| room.people.as_slice());
    if room.is_some_and(|room| room.locked) {
        return tr!("Your roles on this Discord server don't let you join this channel.").into();
    }
    if let Some(room) = dm_call(state).filter(|_| voice_here) {
        let ringing: Vec<&str> = room
            .ringing
            .iter()
            .map(|person| person.name.as_str())
            .collect();
        return if !ringing.is_empty() {
            tr!("Calling {}...", ringing.join(", "))
        } else if voice.participants.len() == 1 {
            tr!("Nobody else is in the call yet.").into()
        } else {
            String::new()
        };
    }
    if voice_here {
        return if voice.participants.len() == 1 {
            tr!("You're here alone. Stay as long as you like; others can join later.").into()
        } else {
            String::new()
        };
    }
    let join = if joins_muted(layout) {
        tr!("Join muted and hang out.")
    } else {
        tr!("Join and hang out.")
    };
    if present.is_empty() {
        tr!("{} The channel stays here when empty.", join)
    } else {
        tr!("{} here. {}", who_is_here(present), join)
    }
}

/// The next call is joined muted.
fn joins_muted(layout: &Layout) -> bool {
    !layout.unmute_on_join() || layout.deafen_on_join()
}

/// "Bob is", "Bob and Carol are", ... (`who_is_here`).
fn who_is_here(people: &[VoicePerson]) -> String {
    let names: Vec<&str> = people.iter().map(|person| person.name.as_str()).collect();
    match names.as_slice() {
        [one] => tr!("{} is", one),
        [first, second] => tr!("{} and {} are", first, second),
        [first, second, third] => tr!("{}, {} and {} are", first, second, third),
        [first, second, rest @ ..] => tr!("{}, {} and {} others are", first, second, rest.len()),
        [] => String::new(),
    }
}

/// The way into the open room's call, while not in it.
pub fn way_in(state: &AppState, conversation: &Conversation, made: &mut Join) {
    let room = state.selected_room();
    let can_join = room.is_some_and(|room| state.can_join_voice(&room.id));
    let join = match (room, conversation.call) {
        (Some(room), Call::Elsewhere) if room.voice => Join::Channel(join::Channel {
            name: room.name.clone(),
            description: conversation.description.clone(),
            preview: state.preview,
            encrypted: room.call_encrypted(),
            switching: state.voice.active(),
            can_join,
        }),
        (Some(room), Call::Elsewhere) => {
            let present = room.people.as_slice();
            let incoming = room.incoming;
            Join::Call(join::Call {
                people: present
                    .iter()
                    .map(|person| {
                        rows::avatar(state, &person.name, &person.id, person.avatar.as_deref())
                    })
                    .collect(),
                note: match (incoming, present.is_empty()) {
                    (true, true) => tr!("Incoming call.").into(),
                    (true, false) => tr!("{} calling you.", who_is_here(present)),
                    (false, _) => tr!("{} in the call.", who_is_here(present)),
                },
                incoming,
                room: room.id.clone(),
                can_join,
            })
        }
        _ => Join::None,
    };
    if *made != join {
        *made = join;
    }
}

/// The DM calls ringing this account, but the one it's in.
pub fn ringing(state: &AppState) -> impl Iterator<Item = &ChatRoom> {
    let voice = &state.voice;
    state
        .rooms
        .iter()
        .map(std::sync::Arc::as_ref)
        .filter(|room| room.incoming && !(voice.active() && voice.room.as_ref() == Some(&room.id)))
}

/// A call ringing in another conversation, and the question before
/// switching calls; says whether each shows.
pub fn incoming(state: &AppState, layout: &Layout, made: &mut Incoming) -> (bool, bool) {
    let shown = ringing(state).find(|room| state.selected.as_ref() != Some(&room.id));
    let mut incoming = Incoming::default();
    if let Some(room) = shown {
        incoming.room = room.id.clone();
        incoming.name = room.name.clone();
        incoming.detail = if room.person.is_some() || room.people.is_empty() {
            tr!("Incoming call").into()
        } else {
            tr!("{} calling", who_is_here(&room.people))
        };
        incoming.avatar = rows::avatar(state, &room.name, &room.id, room.avatar.as_deref());
    }
    let switch = switch(state, layout);
    let asks = switch.is_some();
    incoming.switch = switch.unwrap_or_default();
    if *made != incoming {
        *made = incoming;
    }
    (shown.is_some(), asks)
}

/// What switching calls asks (`switch_prompt`).
fn switch(state: &AppState, layout: &Layout) -> Option<Switch> {
    let id = state.switch_voice.as_ref()?;
    let voice = &state.voice;
    let room = state.rooms.iter().find(|room| &room.id == id);
    let target = room.map_or(tr!("the other channel"), |room| room.name.as_str());
    let muted = if joins_muted(layout) {
        tr!(" You'll join muted.")
    } else {
        ""
    };
    let watch = (state.pending_watch.as_ref())
        .filter(|(watched, _)| watched == id)
        .map(|(_, user)| {
            room.and_then(|room| room.people.iter().find(|person| &person.id == user))
                .map_or(tr!("someone"), |person| person.name.as_str())
        });
    Some(match watch {
        Some(name) if !voice.active() => Switch {
            text: tr!("Join {} to watch {}'s stream?", target, name),
            joins: true,
        },
        Some(name) => Switch {
            text: tr!(
                "You're already in {}. Leave it and join {} to watch {}'s stream?{}",
                voice.name,
                target,
                name,
                muted
            ),
            joins: false,
        },
        None => Switch {
            text: tr!(
                "You're already in {}. Leave it and join {}?{}",
                voice.name,
                target,
                muted
            ),
            joins: false,
        },
    })
}
