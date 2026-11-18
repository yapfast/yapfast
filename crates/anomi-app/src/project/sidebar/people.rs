//! Who is in a voice channel, under its row: `room.people` as
//! `show_sidebar` lists them (`src/ui/refresh.rs:1414`), and for the call
//! this account is in, its participants, as `show_voice` makes them
//! (`:4826`); and where each person in a server's voice channel is
//! (`voice_places`, `:5533`).

use std::collections::HashMap;

use yapfast_core::application::{AppState, Application};
use yapfast_core::chat::{ChatRoom, Permission, VoicePerson as Person};
use yapfast_core::voice::Participant;

use anomi_vm::kind::Avatar;
use anomi_vm::sidebar::channels::{ChannelRow, Channels, VoicePerson};

use crate::pictures;
use crate::project::rows;

/// A server voice channel someone is in, as a DM's row names it.
pub(crate) struct VoicePlace<'a> {
    pub room: &'a str,
    pub channel: &'a str,
    pub server: &'a str,
}

/// Who is in a voice channel of a server this account is in, by user id.
/// DM calls stay out: Discord's lists show only servers' channels.
pub(crate) fn voice_places<'a>(
    state: &'a AppState,
    rooms: &HashMap<&str, &'a ChatRoom>,
) -> HashMap<&'a str, VoicePlace<'a>> {
    let mut places = HashMap::new();
    for server in &state.servers {
        for channel in &server.channels {
            let Some(room) = rooms.get(channel.as_str()).filter(|room| room.voice) else {
                continue;
            };
            for person in &room.people {
                places.insert(
                    person.id.as_str(),
                    VoicePlace {
                        room: &room.id,
                        channel: &room.name,
                        server: &server.name,
                    },
                );
            }
        }
    }
    places
}

/// Whether this account may drag someone in the voice channel `room` onto
/// another: itself, or anyone with Move Members there. Where they may go is
/// asked as they're picked up (`Application::move_targets`).
fn movable(core: &Application, room: &str) -> impl Fn(&str) -> bool {
    let me = core.state.discord_user.clone();
    let moves = core.may(room, Permission::MoveMembers);
    move |user| !user.is_empty() && (moves || me.as_deref() == Some(user))
}

/// The people a voice channel lists under its row, as Discord tells who's
/// in it.
pub(super) fn of_room(core: &Application, room: &ChatRoom) -> Vec<VoicePerson> {
    let state = &core.state;
    let movable = movable(core, &room.id);
    room.people
        .iter()
        .map(|person: &Person| VoicePerson {
            id: person.id.clone(),
            user: person.id.clone(),
            name: person.name.clone(),
            avatar: rows::avatar(state, &person.name, &person.id, person.avatar.as_deref()),
            muted: person.muted,
            deafened: person.deafened,
            server_muted: person.server_muted,
            server_deafened: person.server_deafened,
            live: person.streaming,
            video: person.video,
            local: state.discord_user.as_ref() == Some(&person.id),
            elsewhere: person.elsewhere,
            connected: true,
            movable: movable(&person.id),
            ..VoicePerson::default()
        })
        .collect()
}

/// The call's participants, as its channel's row lists them: speaking and
/// connected as the call has them.
fn participants(core: &Application, room: &str) -> Vec<VoicePerson> {
    let state = &core.state;
    let account = state.account.as_deref().unwrap_or("Preview");
    let movable = movable(core, room);
    state
        .voice
        .participants
        .iter()
        .map(|person: &Participant| {
            let name = display_part(&person.user);
            let tinted = if person.local { account } else { &person.user };
            VoicePerson {
                id: person.id.clone(),
                user: person.person.clone(),
                name: name.to_owned(),
                avatar: Avatar {
                    initial: rows::initial(name),
                    tint: rows::tint(display_part(tinted)),
                    picture: pictures::find(state, person.avatar.as_deref()),
                },
                muted: person.muted,
                deafened: person.deafened,
                server_muted: person.server_muted,
                server_deafened: person.server_deafened,
                live: person.streaming,
                video: person.video.is_some(),
                local: person.local,
                speaking: person.speaking && !person.muted && person.connected,
                connected: person.connected,
                movable: movable(&person.person),
                ..VoicePerson::default()
            }
        })
        .collect()
}

/// A Matrix id's local part, as a name shows it; any other name whole.
fn display_part(name: &str) -> &str {
    name.strip_prefix('@')
        .and_then(|rest| rest.split_once(':'))
        .map_or(name, |(local, _)| local)
}

/// Lists the call's participants under its channel's row, and the room's
/// own people under the row that had them before: the call speaks several
/// times a second, and only that row changes. Whether it changed.
pub(super) fn follow(core: &Application, channels: &mut Channels) -> bool {
    let call = super::super::rail::call_room(&core.state);
    let mut changed = false;
    for row in channels
        .rows
        .iter_mut()
        .filter(|row| row.voice() && !row.hidden)
    {
        let here = call == Some(row.id.as_str());
        if here {
            let people = participants(core, &row.id);
            changed |= set(row, people, true);
        } else if row.in_call {
            let people = core
                .state
                .rooms
                .iter()
                .find(|room| room.id == row.id)
                .map_or_else(Vec::new, |room| of_room(core, room));
            changed |= set(row, people, false);
        }
    }
    changed
}

fn set(row: &mut ChannelRow, people: Vec<VoicePerson>, in_call: bool) -> bool {
    if row.people == people && row.in_call == in_call {
        return false;
    }
    row.people = people;
    row.in_call = in_call;
    true
}
