//! Who's typing and the member list come several times a second in a busy
//! server, and change nothing while what shows them doesn't show: the
//! member list hidden, typing in another conversation, the window behind
//! another or in the tray. The events' stream sifts each batch here before
//! it becomes a message (`backend::start`): what the window doesn't show
//! waits in the gate, each room's latest alone, and no message reaches
//! `update`, so iced builds no view and lays nothing out for it. The core
//! takes what waits before the next message of any kind is handled
//! (`backend::drain`), and what shows it is shown again the moment it shows
//! more (`backend::follow`): the member list opened, the window in front
//! again, as the clocks are (`crate::clocks::front`).
//!
//! Only events that ask nothing of the core but to keep them are held: both
//! replace what came before for their room, so the latest stands for all.
//! Anything else (messages and the account's snapshots, which mark read
//! and notify; the call, whose changes play sounds) passes at once.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use yapfast_core::chat::Event;

/// What the window shows of the events the gate can hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shows {
    /// The member list with its Activity panel, or the composer's draft
    /// and suggestions, which read the members.
    pub members: bool,
    /// The rooms whose typing shows above a composer: the open
    /// conversation's and the thread's beside it.
    pub typing: [Option<String>; 2],
}

impl Shows {
    fn types_in(&self, room: &str) -> bool {
        self.typing.iter().flatten().any(|shown| shown == room)
    }

    /// Whether this shows something `before` didn't.
    fn more_than(&self, before: &Self) -> bool {
        (self.members && !before.members)
            || self
                .typing
                .iter()
                .flatten()
                .any(|room| !before.types_in(room))
    }
}

#[derive(Default)]
struct Held {
    shows: Shows,
    events: Vec<Event>,
}

impl Held {
    /// Keeps `event` if the window doesn't show it, in place of what came
    /// before for its room; gives it back otherwise.
    fn wait(&mut self, event: Event) -> Option<Event> {
        let waits = match &event {
            Event::Members { room, members } if !self.shows.members => {
                // A list that failed to come doesn't replace one that came
                // (`Application::apply`): it waits behind it.
                if members.is_ok() {
                    self.events.retain(
                        |held| !matches!(held, Event::Members { room: other, .. } if other == room),
                    );
                }
                true
            }
            Event::Typing { room, .. } if !self.shows.types_in(room) => {
                self.events.retain(
                    |held| !matches!(held, Event::Typing { room: other, .. } if other == room),
                );
                true
            }
            _ => false,
        };
        if waits {
            self.events.push(event);
            return None;
        }
        Some(event)
    }
}

/// The gate of one backend run, shared by its events' stream and the app.
#[derive(Clone, Default)]
pub struct Gate(Arc<Mutex<Held>>);

impl Gate {
    fn lock(&self) -> MutexGuard<'_, Held> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The events of `batch` the window shows, or that ask more than to be
    /// kept; the rest waits here.
    pub fn sift(&self, batch: Vec<Event>) -> Vec<Event> {
        let mut held = self.lock();
        batch
            .into_iter()
            .filter_map(|event| held.wait(event))
            .collect()
    }

    /// What waits, for the core to take now.
    pub fn take(&self) -> Vec<Event> {
        std::mem::take(&mut self.lock().events)
    }

    /// Whether the window shows the member list now.
    pub fn shows_members(&self) -> bool {
        self.lock().shows.members
    }

    /// Says what the window shows now: the member list or what reads it,
    /// and the rooms whose typing shows. When that's more than before, what
    /// waits comes back, for the core to take and the window to show.
    pub fn follow(&self, members: bool, typing: [Option<&str>; 2]) -> Option<Vec<Event>> {
        let mut held = self.lock();
        let shows = &held.shows;
        let same_typing = (shows.typing.iter())
            .zip(typing)
            .all(|(shown, room)| shown.as_deref() == room);
        if shows.members == members && same_typing {
            return None;
        }
        let now = Shows {
            members,
            typing: typing.map(|room| room.map(str::to_owned)),
        };
        let more = now.more_than(&held.shows);
        held.shows = now;
        more.then(|| std::mem::take(&mut held.events))
    }
}

#[cfg(test)]
mod tests {
    use yapfast_core::chat::{Event, Presence};

    use super::Gate;
    use crate::project::people::tests::member;

    fn members(room: &str, count: usize) -> Event {
        let list = (0..count)
            .map(|n| member(&n.to_string(), "Ann", Presence::Online, "Online"))
            .collect();
        Event::Members {
            room: room.into(),
            members: Ok(list),
        }
    }

    fn typing(room: &str, who: &str) -> Event {
        Event::Typing {
            room: room.into(),
            names: vec![who.into()],
        }
    }

    /// Each event as its kind, its room and its first name or its count.
    fn kinds(events: &[Event]) -> Vec<(&'static str, &str, String)> {
        events
            .iter()
            .map(|event| match event {
                Event::Members { room, members } => {
                    let count = members.as_ref().map_or(0, Vec::len);
                    ("members", room.as_str(), count.to_string())
                }
                Event::Typing { room, names } => ("typing", room.as_str(), names.join(",")),
                _ => ("other", "", String::new()),
            })
            .collect()
    }

    #[test]
    fn what_the_window_does_not_show_waits_and_each_room_keeps_its_latest() {
        let gate = Gate::default();
        drop(gate.follow(false, [Some("general"), None]));
        let batch = vec![
            members("general", 3),
            typing("dm", "Bob"),
            typing("general", "Cat"),
            members("general", 4),
            typing("dm", "Dan"),
            Event::Error("passes".into()),
        ];
        // Typing where it shows, and anything else, go on as a message.
        let passing = gate.sift(batch);
        let expected = [
            ("typing", "general", "Cat".into()),
            ("other", "", String::new()),
        ];
        assert_eq!(kinds(&passing), expected);
        let expected = [
            ("members", "general", "4".into()),
            ("typing", "dm", "Dan".into()),
        ];
        assert_eq!(kinds(&gate.take()), expected);
        assert!(gate.take().is_empty());
        // A batch the window shows nothing of makes no message.
        assert!(gate.sift(vec![typing("dm", "Eve")]).is_empty());
    }

    #[test]
    fn showing_more_gives_back_what_waited_and_the_same_gives_nothing() {
        let gate = Gate::default();
        assert!(gate.sift(vec![members("general", 2)]).is_empty());
        // Nothing more shows: it keeps waiting.
        assert!(gate.follow(false, [None, None]).is_none());
        let back = gate
            .follow(true, [Some("general"), None])
            .expect("shows more");
        assert_eq!(kinds(&back), [("members", "general", "2".into())]);
        assert!(gate.follow(true, [Some("general"), None]).is_none());
        // Shown, it passes.
        assert_eq!(gate.sift(vec![members("general", 2)]).len(), 1);
        // Less shows: nothing comes back, and the list waits again.
        assert!(gate.follow(false, [Some("general"), None]).is_none());
        assert!(gate.sift(vec![members("general", 2)]).is_empty());
        assert!(!gate.shows_members());
    }
}
