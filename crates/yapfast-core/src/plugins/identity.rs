//! The answers about how people look, kept once per (person, server).
//!
//! A channel shows the same author twenty times and a member list shows a
//! hundred people, so asking a plugin per message would cost a call per row and
//! a tree per frame. §17.2 of the plan asks for one answer per (person, server)
//! instead: this is where the parent keeps it.
//!
//! An entry is asked for when the person comes on screen and the cache has
//! nothing fresh for them, which is a miss and not a timer. The cache is capped,
//! so a long session with a big server keeps the people it has shown lately and
//! forgets the rest, and [`FRESH`] keeps an answer from outliving a person's
//! name for a whole session.
//!
//! Nothing here runs a plugin: the bridge asks, and the answer arrives as
//! [`crate::plugins::state::Notice::Identity`].

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use yapfast_plugin_api::identity::Identity;

/// How many people's answers are kept. A person asked about again moves to the
/// back, so the front is the people shown least lately.
pub const KEPT: usize = 256;

/// How long an answer is used before the person is asked about again: long
/// enough that scrolling a channel costs nothing, short enough that a name a
/// plugin changed shows up within a few minutes.
pub const FRESH: Duration = Duration::from_secs(300);

/// One person's answer, or that nobody changed them.
struct Entry {
    person: String,
    /// The server the answer is for; empty for a DM and for a room no server
    /// holds, which is one answer for that person everywhere.
    server: String,
    /// Where the plugin that answered sits in the chain: a later plugin does
    /// not overwrite an earlier one's answer while it is fresh, so which
    /// plugin wins doesn't depend on which answer arrived first.
    rank: usize,
    /// `None` when nobody changed this person, which is an answer worth
    /// keeping: without it every row would ask again.
    answer: Option<Arc<Identity>>,
    at: Instant,
}

/// Every answer kept, and a count of the ones that changed.
#[derive(Default)]
pub struct Identities {
    kept: VecDeque<Entry>,
    changes: u64,
}

impl Identities {
    /// Whether this person has to be asked about: nothing is kept, or what is
    /// is older than [`FRESH`].
    pub fn needs(&self, person: &str, server: Option<&str>, now: Instant) -> bool {
        self.position(person, server).is_none_or(|at| {
            let entry = &self.kept[at];
            now.saturating_duration_since(entry.at) >= FRESH
        })
    }

    /// How this person looks, when a plugin changed anything about them. A
    /// fresh entry that says nobody did answers `None` like an absent one: the
    /// caller draws Yapfast's own view either way.
    pub fn get(
        &mut self,
        person: &str,
        server: Option<&str>,
        now: Instant,
    ) -> Option<Arc<Identity>> {
        let at = self.position(person, server)?;
        let entry = &self.kept[at];
        if now.saturating_duration_since(entry.at) >= FRESH {
            return None;
        }
        let answer = entry.answer.clone();
        // Shown again, so it is the last to go.
        let entry = self.kept.remove(at)?;
        self.kept.push_back(entry);
        answer
    }

    /// Keeps what a plugin answered about one person.
    ///
    /// An answer from a plugin later in the chain doesn't replace a fresh one
    /// from earlier: the order the answers arrive in must not decide which
    /// plugin's person is shown. Answers the same as what is kept change
    /// nothing, so a surface that draws from here redraws only on a real change.
    /// Returns whether what a surface draws changed.
    pub fn note(
        &mut self,
        person: &str,
        server: Option<&str>,
        rank: usize,
        answer: Option<Identity>,
        now: Instant,
    ) -> bool {
        let answer = answer.map(|identity| Arc::new(identity.sanitized()));
        let server = server.unwrap_or_default();
        match self.position(person, Some(server)) {
            Some(at) => {
                let entry = &self.kept[at];
                if rank > entry.rank && now.saturating_duration_since(entry.at) < FRESH {
                    return false;
                }
                let changed = match (&entry.answer, &answer) {
                    (Some(kept), Some(new)) => **kept != **new,
                    (None, None) => false,
                    _ => true,
                };
                let Some(_) = self.kept.remove(at) else {
                    return false;
                };
                self.kept.push_back(Entry {
                    person: person.to_owned(),
                    server: server.to_owned(),
                    rank,
                    answer,
                    at: now,
                });
                if changed {
                    self.changes += 1;
                }
                changed
            }
            None => {
                let changed = answer.is_some();
                self.kept.push_back(Entry {
                    person: person.to_owned(),
                    server: server.to_owned(),
                    rank,
                    answer,
                    at: now,
                });
                while self.kept.len() > KEPT {
                    self.kept.pop_front();
                }
                if changed {
                    self.changes += 1;
                }
                changed
            }
        }
    }

    /// The count of answers that changed what a surface draws, so the interface
    /// can tell a real change from the same answer again.
    pub fn changes(&self) -> u64 {
        self.changes
    }

    /// Forgets every answer, which is what the plugins that answered changing
    /// asks for: what a plugin that is no longer enabled said must not stay.
    /// Returns whether there was anything to forget.
    pub fn forget(&mut self) -> bool {
        let had = !self.kept.is_empty();
        self.kept.clear();
        if had {
            self.changes += 1;
        }
        had
    }

    fn position(&self, person: &str, server: Option<&str>) -> Option<usize> {
        let server = server.unwrap_or_default();
        self.kept
            .iter()
            .position(|entry| entry.person == person && entry.server == server)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(name: &str) -> Identity {
        Identity {
            display_name: Some(name.to_owned()),
            ..Identity::default()
        }
    }

    #[test]
    fn a_person_nobody_changed_is_asked_about_once() {
        let now = Instant::now();
        let mut kept = Identities::default();
        assert!(kept.needs("person-1", None, now));
        assert!(!kept.note("person-1", None, 0, None, now));
        assert!(!kept.needs("person-1", None, now));
        assert_eq!(kept.get("person-1", None, now), None);
    }

    #[test]
    fn an_answer_is_kept_per_server_and_forgets_the_oldest() {
        let now = Instant::now();
        let mut kept = Identities::default();
        assert!(kept.note("person-1", Some("server-1"), 0, Some(identity("Ada")), now));
        assert!(kept.needs("person-1", Some("server-2"), now));
        assert!(kept.needs("person-1", None, now));
        assert_eq!(
            kept.get("person-1", Some("server-1"), now)
                .and_then(|found| found.display_name.clone())
                .as_deref(),
            Some("Ada")
        );
        for index in 0..KEPT + 10 {
            kept.note(&format!("person-{index}"), None, 0, None, now);
        }
        assert!(kept.kept.len() <= KEPT);
        assert!(kept.needs("person-1", Some("server-1"), now));
    }

    #[test]
    fn an_answer_goes_stale_and_is_asked_for_again() {
        let now = Instant::now();
        let mut kept = Identities::default();
        kept.note("person-1", None, 0, Some(identity("Ada")), now);
        assert!(!kept.needs("person-1", None, now + FRESH - Duration::from_secs(1)));
        assert!(kept.needs("person-1", None, now + FRESH));
        assert_eq!(kept.get("person-1", None, now + FRESH), None);
    }

    #[test]
    fn the_earlier_plugin_in_the_chain_wins_whatever_the_order_of_answers() {
        let now = Instant::now();
        // The earlier plugin answered first, and the later one's answer is
        // dropped: what is kept is still fresh, and it isn't the later one's to
        // replace.
        let mut kept = Identities::default();
        assert!(kept.note("person-1", None, 0, Some(identity("first")), now));
        assert!(!kept.note("person-1", None, 1, Some(identity("second")), now));
        assert_eq!(
            kept.get("person-1", None, now)
                .and_then(|found| found.display_name.clone())
                .as_deref(),
            Some("first")
        );

        // The later one answered first, and the earlier one still wins: which
        // plugin decides cannot depend on which answer arrived first.
        let mut ordered = Identities::default();
        assert!(ordered.note("person-1", None, 1, Some(identity("second")), now));
        assert!(ordered.note("person-1", None, 0, Some(identity("first")), now));
        assert_eq!(
            ordered
                .get("person-1", None, now)
                .and_then(|found| found.display_name.clone())
                .as_deref(),
            Some("first")
        );
    }

    #[test]
    fn the_same_answer_again_changes_nothing() {
        let now = Instant::now();
        let mut kept = Identities::default();
        assert!(kept.note("person-1", None, 0, Some(identity("Ada")), now));
        let changes = kept.changes();
        assert!(!kept.note("person-1", None, 0, Some(identity("Ada")), now));
        assert_eq!(kept.changes(), changes);
    }

    #[test]
    fn a_bad_field_is_dropped_and_the_rest_kept() {
        let now = Instant::now();
        let mut kept = Identities::default();
        kept.note(
            "person-1",
            None,
            0,
            Some(Identity {
                display_name: Some("Ada".into()),
                avatar: Some("http://example.test/a.png".into()),
                ..Identity::default()
            }),
            now,
        );
        let kept = kept.get("person-1", None, now).unwrap();
        assert_eq!(kept.display_name.as_deref(), Some("Ada"));
        assert_eq!(kept.avatar, None);
    }
}
