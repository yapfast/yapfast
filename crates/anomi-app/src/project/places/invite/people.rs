//! Whom the invite dialog offers, as Discord's client gathers them, from
//! what the core knows: for a voice channel, the server's members the
//! member list holds who aren't in the channel already (a private one's
//! only while it is the channel open, whose list is those who may see it);
//! then the one-to-one DMs, in the DM list's order, and the friends without
//! one, leaving out whoever the core knows is in the server already. Never
//! this account, a bot, nor someone blocked or ignored. Gathered again only
//! when what they come from changes.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use yapfast_core::application::{AppState, InviteDialog, Relation};
use yapfast_core::chat::ChatMember;
use yapfast_core::network::Network;

use anomi_vm::places::invite::{Invitee, Sent};

use crate::project::rows;

/// Someone it may offer, before the search.
struct Candidate {
    id: String,
    name: String,
    username: String,
    avatar: Option<String>,
}

/// Everyone it may offer, and what they were gathered from.
#[derive(Default)]
pub(super) struct People {
    key: Option<u64>,
    /// Counts the gatherings, which the rows key on.
    pub version: u64,
    members: Vec<Candidate>,
    friends: Vec<Candidate>,
}

impl People {
    /// Gathers them again when what they come from changed. Those listed
    /// already keep their places, as an invite sent moves its DM to the
    /// top of the DM list: rows don't move under the pointer.
    pub fn gather(&mut self, state: &AppState, dialog: &InviteDialog) {
        let key = key(state, dialog);
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        self.version += 1;
        let (mut members, mut friends) = gather(state, dialog);
        steady(&mut members, &self.members);
        steady(&mut friends, &self.friends);
        (self.members, self.friends) = (members, friends);
    }

    /// Someone it offers, by user id.
    pub fn offers(&self, user: &str) -> bool {
        self.members
            .iter()
            .chain(&self.friends)
            .any(|who| who.id == user)
    }

    /// The avatars of these people, by user id.
    pub fn avatars<'a>(&'a self, ids: &'a [String]) -> impl Iterator<Item = &'a str> + 'a {
        let all = self.members.iter().chain(&self.friends);
        all.filter(move |who| ids.contains(&who.id))
            .filter_map(|who| who.avatar.as_deref())
    }

    /// The rows the search finds, the members' and the friends', each with
    /// how its invite stands.
    pub fn rows(
        &self,
        state: &AppState,
        query: &str,
        sent: &impl Fn(&str) -> Sent,
    ) -> (Vec<Invitee>, Vec<Invitee>) {
        let query = query.trim().to_lowercase();
        let found = |who: &&Candidate| {
            query.is_empty()
                || who.name.to_lowercase().contains(&query)
                || who.username.to_lowercase().contains(&query)
        };
        let row = |who: &Candidate| Invitee {
            id: who.id.clone(),
            name: who.name.clone(),
            username: who.username.clone(),
            avatar: rows::avatar(state, &who.name, &who.id, who.avatar.as_deref()),
            sent: sent(&who.id),
        };
        let pick = |list: &[Candidate]| list.iter().filter(found).map(row).collect();
        (pick(&self.members), pick(&self.friends))
    }
}

/// Puts those `before` listed in their places there, the new after them
/// in their own order.
fn steady(list: &mut [Candidate], before: &[Candidate]) {
    if before.is_empty() {
        return;
    }
    let places: HashMap<&str, usize> = (before.iter().enumerate())
        .map(|(at, who)| (who.id.as_str(), at))
        .collect();
    list.sort_by_key(|who| places.get(who.id.as_str()).copied().unwrap_or(usize::MAX));
}

/// What they come from: the dialog's channel, the people lists, the
/// rooms, the member list and who is in the voice channel.
fn key(state: &AppState, dialog: &InviteDialog) -> u64 {
    let mut hasher = DefaultHasher::new();
    (&dialog.room, state.people_changes, state.list_changes).hash(&mut hasher);
    (&state.selected, listed(state).as_ptr() as usize).hash(&mut hasher);
    for room in state.rooms.iter().filter(|room| room.id == dialog.room) {
        for person in &room.people {
            person.id.hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// The open channel's member list, as the core holds it.
fn listed(state: &AppState) -> &[ChatMember] {
    match &state.members {
        Some(Ok(members)) => members.as_slice(),
        _ => &[],
    }
}

fn gather(state: &AppState, dialog: &InviteDialog) -> (Vec<Candidate>, Vec<Candidate>) {
    let server = state
        .servers
        .iter()
        .find(|server| server.channels.contains(&dialog.room));
    let in_server = |room: &String| server.is_some_and(|server| server.channels.contains(room));
    // The open channel's list, when it is of this server.
    let listed = match &state.selected {
        Some(open) if in_server(open) => listed(state),
        _ => &[],
    };
    let channel = state.rooms.iter().find(|room| room.id == dialog.room);
    let invitable = |user: &str| {
        !matches!(state.relation(user), Relation::Me | Relation::Bot)
            && !state.is_blocked(user)
            && !state.discord_ignored.iter().any(|ignored| ignored == user)
    };
    let guild = server.and_then(|server| yapfast_core::discord::parse_server(&server.id));
    // Who is who, as the Discord service knows them: usernames, and who it
    // knows is a member of the server.
    #[cfg(feature = "discord")]
    let names = state.discord_names.as_ref().map(|names| names.read());
    let username = |user: &str| -> String {
        #[cfg(feature = "discord")]
        if let Some(names) = &names {
            return names.username(user).unwrap_or_default().to_owned();
        }
        let _ = user;
        String::new()
    };
    let known_member = |user: &str| -> bool {
        #[cfg(feature = "discord")]
        if let (Some(names), Some(guild)) = (&names, guild) {
            return names.knows_member(guild, user);
        }
        let _ = (user, guild);
        false
    };
    let mut seen: HashSet<&str> = HashSet::new();
    let mut members = Vec::new();
    let private = channel.is_some_and(|room| room.mark.private);
    let open = state.selected.as_ref() == Some(&dialog.room);
    if dialog.voice && (!private || open) {
        if let Some(channel) = channel {
            seen.extend(channel.people.iter().map(|person| person.id.as_str()));
        }
        for member in listed {
            if !member.bot && invitable(&member.id) && seen.insert(&member.id) {
                members.push(Candidate {
                    id: member.id.clone(),
                    name: member.name.clone(),
                    username: member.username.clone().unwrap_or_default(),
                    avatar: member.avatar.clone(),
                });
            }
        }
    }
    let here: HashSet<&str> = listed.iter().map(|member| member.id.as_str()).collect();
    let dms = state.rooms.iter().filter(|room| {
        room.direct && !room.request && room.network == Network::Discord && !room.official
    });
    let dms = dms.filter_map(|room| Some((room.person.as_deref()?, &room.name, &room.avatar)));
    let friends_too = state
        .friends
        .iter()
        .map(|friend| (friend.id.as_str(), &friend.name, &friend.avatar));
    let mut friends = Vec::new();
    for (id, name, avatar) in dms.chain(friends_too) {
        if seen.insert(id) && invitable(id) && !here.contains(id) && !known_member(id) {
            friends.push(Candidate {
                id: id.to_owned(),
                name: name.clone(),
                username: username(id),
                avatar: avatar.clone(),
            });
        }
    }
    (members, friends)
}
