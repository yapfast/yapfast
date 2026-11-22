//! `presence`: who came online, went idle or left, told once per change.
//!
//! §10.4 of the plan makes `presence` opt-in and coalesced, like `typing` and
//! `speaking`: a member list updates as a whole when Discord sends it, so the
//! tap compares it with what it last said and reports only the people whose
//! presence actually moved. A burst of them is one event batch, which the bridge
//! already does for every coalesced kind.
//!
//! Nothing here is kept while no plugin subscribed: the caller asks
//! [`PluginState`](crate::plugins::state::PluginState) whether anyone wants
//! `presence`, and only then builds an event.

use std::collections::BTreeMap;

use crate::chat::{ChatMember, Presence};

/// The presence a plugin is told about, as the plugin API names it.
pub fn name(presence: Presence) -> &'static str {
    match presence {
        Presence::Online | Presence::Mobile => "online",
        Presence::Idle => "idle",
        Presence::DoNotDisturb => "dnd",
        Presence::Offline => "offline",
    }
}

/// The presences a surface last said, so a member list that arrives again costs
/// one comparison per member and no event at all.
#[derive(Default)]
pub struct Presences {
    known: BTreeMap<String, &'static str>,
}

impl Presences {
    /// The people whose presence moved since the last call, and the ones who
    /// left the list are forgotten rather than reported: a member list that
    /// arrives without somebody is a conversation whose members changed, not a
    /// presence change.
    ///
    /// The first call reports nobody: a tap that spoke about everybody the
    /// first time a room opened would say nothing a plugin couldn't read.
    ///
    /// The map is updated where it stands rather than rebuilt: a list that
    /// arrives again with the same presences allocates nothing at all, and one
    /// that moved copies only the people it moved.
    pub fn changes(&mut self, people: &[ChatMember], first: bool) -> Vec<(String, &'static str)> {
        let mut changed = Vec::new();
        for person in people {
            let presence = name(person.presence.unwrap_or(Presence::Offline));
            let was = self.known.get(person.id.as_str()).copied();
            if was == Some(presence) {
                continue;
            }
            if !first {
                changed.push((person.id.clone(), presence));
            }
            if was.is_none() {
                self.known.insert(person.id.clone(), presence);
            } else {
                // Somebody who moved is already a key: only the value changes.
                if let Some(held) = self.known.get_mut(person.id.as_str()) {
                    *held = presence;
                }
            }
        }
        if self.known.len() != people.len() {
            // Somebody left the list. One set of borrowed ids answers for all
            // of them, rather than a copy of every id that is still there.
            let present: std::collections::HashSet<&str> =
                people.iter().map(|person| person.id.as_str()).collect();
            self.known.retain(|id, _| present.contains(id.as_str()));
        }
        changed
    }

    /// Whether this is the first list of a conversation, which is reported as
    /// nothing.
    pub fn empty(&self) -> bool {
        self.known.is_empty()
    }

    /// Forgets everything, which is what turning the last plugin off asks for.
    pub fn forget(&mut self) {
        self.known.clear();
    }
}

/// Who the open room's member list held when it was last compared, so a list
/// that arrives again tells `member_join` and `member_leave` who came and who
/// went, and nothing about the people who stayed.
#[derive(Default)]
pub struct Roster {
    room: String,
    known: std::collections::HashSet<String>,
}

impl Roster {
    /// The people who appeared, with the name the list shows, and the ones
    /// who left, since the last list of the same room. A list of another room
    /// is a first list, which reports nobody: a plugin is told about a
    /// conversation's comings and goings, not about opening it.
    pub fn changes(
        &mut self,
        room: &str,
        people: &[ChatMember],
    ) -> (Vec<(String, String)>, Vec<String>) {
        if self.room != room {
            room.clone_into(&mut self.room);
            self.known = people.iter().map(|person| person.id.clone()).collect();
            return (Vec::new(), Vec::new());
        }
        let joined: Vec<(String, String)> = people
            .iter()
            .filter(|person| !self.known.contains(&person.id))
            .map(|person| (person.id.clone(), person.name.clone()))
            .collect();
        let present: std::collections::HashSet<&str> =
            people.iter().map(|person| person.id.as_str()).collect();
        let mut left: Vec<String> = self
            .known
            .iter()
            .filter(|id| !present.contains(id.as_str()))
            .cloned()
            .collect();
        left.sort_unstable();
        if joined.is_empty() && left.is_empty() {
            return (joined, left);
        }
        for gone in &left {
            self.known.remove(gone);
        }
        self.known
            .extend(joined.iter().map(|(person, _)| person.clone()));
        (joined, left)
    }

    /// Forgets the list, which is what turning the last plugin off asks for.
    pub fn forget(&mut self) {
        self.room.clear();
        self.known.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, presence: Presence) -> ChatMember {
        ChatMember {
            id: id.to_owned(),
            bot: false,
            name: id.to_owned(),
            username: None,
            role: crate::chat::Role::Member,
            avatar: None,
            decoration: None,
            nameplate: None,
            style: None,
            group: None,
            group_icon: None,
            role_icon: None,
            presence: Some(presence),
            color: None,
            activity: None,
            activities: crate::chat::Activities::default(),
            roles: std::sync::Arc::from(Vec::new()),
            timed_out_until: None,
            tag: None,
            boosting_since: None,
        }
    }

    #[test]
    fn only_the_people_whose_presence_moved_are_reported() {
        let mut known = Presences::default();
        let people = vec![member("a", Presence::Online), member("b", Presence::Idle)];
        assert!(
            known.changes(&people, true).is_empty(),
            "the first list says nothing"
        );
        assert!(known.changes(&people, false).is_empty(), "nothing moved");

        let moved = vec![
            member("a", Presence::DoNotDisturb),
            member("b", Presence::Idle),
        ];
        assert_eq!(known.changes(&moved, false), vec![("a".to_owned(), "dnd")]);
    }

    #[test]
    fn a_person_who_left_is_forgotten_and_not_reported() {
        let mut known = Presences::default();
        known.changes(
            &[member("a", Presence::Online), member("b", Presence::Online)],
            true,
        );
        let left = vec![member("a", Presence::Online)];
        assert!(known.changes(&left, false).is_empty());
        // Back again, and it is a change worth reporting: they were not here.
        let back = vec![member("a", Presence::Online), member("b", Presence::Idle)];
        assert_eq!(known.changes(&back, false), vec![("b".to_owned(), "idle")]);
    }

    #[test]
    fn a_mobile_account_reads_as_online() {
        assert_eq!(name(Presence::Mobile), "online");
        assert_eq!(name(Presence::Offline), "offline");
    }

    #[test]
    fn a_roster_reports_who_came_and_went_and_not_the_first_list() {
        let mut roster = Roster::default();
        let first = vec![member("a", Presence::Online), member("b", Presence::Idle)];
        assert_eq!(
            roster.changes("room-1", &first),
            (Vec::new(), Vec::new()),
            "opening a room says nothing"
        );
        let next = vec![member("b", Presence::Idle), member("c", Presence::Online)];
        let (joined, left) = roster.changes("room-1", &next);
        assert_eq!(joined, [("c".to_owned(), "c".to_owned())]);
        assert_eq!(left, ["a".to_owned()]);
        assert_eq!(
            roster.changes("room-1", &next),
            (Vec::new(), Vec::new()),
            "the same list again says nothing"
        );
        assert_eq!(
            roster.changes("room-2", &first),
            (Vec::new(), Vec::new()),
            "another room's first list says nothing either"
        );
    }
}

/// The friends and friend requests the plugins were last told of, kept only
/// while a plugin listens for relationships.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Relationships {
    /// The `people_changes` they were read at, so a sync that changed none of
    /// the people lists compares nothing.
    pub at: u64,
    pub friends: std::collections::BTreeSet<String>,
    /// Requests by person, and whether each came to this account.
    pub requests: std::collections::BTreeMap<String, bool>,
}

/// One change in who is a friend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelationshipChange {
    /// They are a friend now.
    Added(String),
    /// A friend, or a request, went.
    Removed { person: String, was: &'static str },
    /// A request arrived, or this account sent one.
    Requested { person: String, incoming: bool },
}

impl Relationships {
    /// The lists now.
    pub fn read(at: u64, friends: &[String], requests: &[crate::chat::FriendRequest]) -> Self {
        Relationships {
            at,
            friends: friends.iter().cloned().collect(),
            requests: requests
                .iter()
                .map(|request| (request.id.clone(), request.incoming))
                .collect(),
        }
    }

    /// What changed from `self` to `now`: a request that became a friend is
    /// an addition, and nothing else.
    pub fn changes(&self, now: &Relationships) -> Vec<RelationshipChange> {
        let mut changes = Vec::new();
        for person in now.friends.difference(&self.friends) {
            changes.push(RelationshipChange::Added(person.clone()));
        }
        for (person, incoming) in &now.requests {
            if !self.requests.contains_key(person) && !now.friends.contains(person) {
                changes.push(RelationshipChange::Requested {
                    person: person.clone(),
                    incoming: *incoming,
                });
            }
        }
        for person in self.friends.difference(&now.friends) {
            changes.push(RelationshipChange::Removed {
                person: person.clone(),
                was: "friend",
            });
        }
        for person in self.requests.keys() {
            if !now.requests.contains_key(person) && !now.friends.contains(person) {
                changes.push(RelationshipChange::Removed {
                    person: person.clone(),
                    was: "request",
                });
            }
        }
        changes
    }
}

#[cfg(test)]
mod relationship_tests {
    use super::*;

    fn request(id: &str, incoming: bool) -> crate::chat::FriendRequest {
        crate::chat::FriendRequest {
            id: id.into(),
            name: id.into(),
            avatar: None,
            incoming,
            spam: false,
        }
    }

    #[test]
    fn a_request_then_a_friend_then_gone_is_told_as_it_happened() {
        let start = Relationships::read(1, &["ann".into()], &[]);
        let asked = Relationships::read(2, &["ann".into()], &[request("bo", true)]);
        assert_eq!(
            start.changes(&asked),
            [RelationshipChange::Requested {
                person: "bo".into(),
                incoming: true
            }]
        );
        let accepted = Relationships::read(3, &["ann".into(), "bo".into()], &[]);
        assert_eq!(
            asked.changes(&accepted),
            [RelationshipChange::Added("bo".into())],
            "an accepted request is a friend added, not a request removed"
        );
        let unfriended = Relationships::read(4, &["bo".into()], &[]);
        assert_eq!(
            accepted.changes(&unfriended),
            [RelationshipChange::Removed {
                person: "ann".into(),
                was: "friend"
            }]
        );
    }
}
