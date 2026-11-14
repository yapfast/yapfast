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
}
