//! Leaving a private Discord call no one else is in, as Discord's own apps
//! do (`src/ui/alone.rs`): Settings › Voice & video chooses how long it
//! waits, or Never. One wait runs while such a call is empty, set for the
//! moment it ends; nothing ticks meanwhile. It runs behind other windows
//! and in the tray too, and leaving tells of it in a notification while
//! the window isn't in front. A wait that outlived its call (someone came,
//! the call ended, the wait chosen changed) was aborted, and one that comes
//! anyway finds another number and does nothing.

use std::time::Duration;

use iced::Task;
use iced::task::Handle;
use yapfast_core::application::{Action, AppState};
use yapfast_core::chat::Alert;
use yapfast_core::voice::{Command, Phase};
use yapfast_lang::trn;

use super::Due;
use crate::app::{App, later};
use crate::message::Message;
use crate::project::settings::pages::{WAITS, alone_choice};

/// The empty private call, and the wait that leaves it.
#[derive(Default)]
pub struct Alone {
    /// The call's room, from when no one else was in it. It stays while the
    /// Leave goes out, so that the call isn't waited on again as it ends.
    room: Option<String>,
    wait: Option<Handle>,
    /// The number of the last wait set: an older one that comes is stale.
    number: u64,
}

impl Alone {
    fn stop(&mut self) {
        if let Some(wait) = self.wait.take() {
            wait.abort();
        }
    }
}

/// The private Discord call this account is connected to with no one else
/// connected: those it still rings haven't come.
pub(crate) fn empty_private_call(state: &AppState) -> Option<&str> {
    let voice = &state.voice;
    if !matches!(voice.phase, Phase::Connected | Phase::Reconnecting) {
        return None;
    }
    let room = voice.room.as_deref()?;
    // A DM's or group DM's room is under Discord's `@me`, as its call is.
    let private = yapfast_core::discord::parse_room(room)
        .is_some_and(|(server, _)| server == yapfast_core::discord::DM_GUILD);
    let (mut here, mut others) = (false, false);
    for person in voice.participants.iter().filter(|person| person.connected) {
        if person.local {
            here = true;
        } else {
            others = true;
        }
    }
    (private && here && !others).then_some(room)
}

/// The minutes the chosen wait lasts; none to stay in the call.
fn minutes(app: &App) -> Option<u32> {
    Some(WAITS[alone_choice(&app.layout)]).filter(|&minutes| minutes > 0)
}

/// After each update: starts the wait as the call this account is in
/// empties, and stops it as someone comes or the call ends.
pub fn after(app: &mut App) -> Task<Message> {
    let minutes = minutes(app);
    let room = empty_private_call(&app.core.state);
    let alone = &mut app.shown.voice.alone;
    let Some(room) = room else {
        if alone.room.take().is_some() {
            alone.stop();
        }
        return Task::none();
    };
    if alone.room.as_deref() == Some(room) {
        return Task::none();
    }
    alone.room = Some(room.to_owned());
    alone.stop();
    let Some(minutes) = minutes else {
        return Task::none();
    };
    alone.number += 1;
    let wait = Duration::from_secs(u64::from(minutes) * 60);
    let (task, handle) = later(wait, Message::Voice(Due::Alone(alone.number))).abortable();
    alone.wait = Some(handle);
    task
}

/// The wait chosen changed: an empty call waits it again from now, so a
/// shorter one never leaves at once.
pub fn wait_changed(app: &mut App) {
    let alone = &mut app.shown.voice.alone;
    alone.room = None;
    alone.stop();
}

/// The wait ended: leaves the call, as its Leave button does, if it's still
/// the one that emptied.
pub fn ended(app: &mut App, number: u64) -> Task<Message> {
    let minutes = minutes(app);
    let alone = &mut app.shown.voice.alone;
    if alone.number != number || alone.wait.take().is_none() {
        return Task::none();
    }
    let still =
        empty_private_call(&app.core.state).is_some_and(|room| alone.room.as_deref() == Some(room));
    let (Some(room), Some(minutes)) = (alone.room.clone(), minutes) else {
        return Task::none();
    };
    if !still {
        // `after` waits again for the call as it is now.
        alone.room = None;
        return Task::none();
    }
    tracing::info!(minutes, "leaving a DM call no one else is in");
    let name = app.core.state.voice.name.clone();
    let task = app.dispatch(Action::Voice(Command::Leave));
    tell(app, room, name, minutes);
    task
}

/// Says why the call ended while the window isn't in front, where its sound
/// may go unheard; in front, the voice dock closing shows it.
fn tell(app: &mut App, room: String, name: String, minutes: u32) {
    let state = &app.core.state;
    let picture = (state.rooms.iter())
        .find(|found| found.id == room)
        .and_then(|found| found.avatar.as_ref())
        .and_then(|avatar| state.pictures.get(avatar).cloned());
    let body = trn!(
        "You left: no one else was in the call for {n} minute",
        "You left: no one else was in the call for {n} minutes",
        u64::from(minutes)
    );
    let alert = Alert {
        room,
        title: name,
        body,
        picture,
        call: false,
        mention: false,
    };
    crate::effects::notify::behind(app, alert);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use yapfast_core::voice::{Phase, State};

    use super::*;
    use crate::project::voice::tests::person;

    const DM: &str = "discord:@me:20";

    fn in_dm_call(name: &str) -> (App, std::path::PathBuf) {
        let folder =
            std::env::temp_dir().join(format!("anomi-alone-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let mut app = App::for_tests(&folder);
        app.core.state.voice = State {
            phase: Phase::Connected,
            room: Some(DM.into()),
            name: "Bob".into(),
            participants: Arc::from([person("alice", "Alice"), person("bob", "Bob")]),
            ..State::default()
        };
        (app, folder)
    }

    fn alone(app: &mut App) {
        let mut people = app.core.state.voice.participants.to_vec();
        people.retain(|person| person.local);
        app.core.state.voice.participants = people.into();
    }

    #[test]
    fn only_an_empty_private_call_is_waited_on() {
        let (mut app, folder) = in_dm_call("empty");
        drop(after(&mut app));
        assert!(app.shown.voice.alone.wait.is_none(), "Bob is there");
        alone(&mut app);
        drop(after(&mut app));
        assert!(app.shown.voice.alone.wait.is_some());
        let number = app.shown.voice.alone.number;
        drop(after(&mut app));
        assert_eq!(
            app.shown.voice.alone.number, number,
            "one wait, not one per update"
        );
        app.core.state.voice.room = Some("discord:1400:1403".into());
        drop(after(&mut app));
        assert!(
            app.shown.voice.alone.wait.is_none(),
            "a server's call stays"
        );
        drop(app);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_stale_wait_leaves_nothing_and_a_current_one_leaves_the_call() {
        let (mut app, folder) = in_dm_call("leave");
        alone(&mut app);
        drop(after(&mut app));
        let first = app.shown.voice.alone.number;
        wait_changed(&mut app);
        drop(after(&mut app));
        let second = app.shown.voice.alone.number;
        assert_ne!(first, second, "the new wait counts from now");
        drop(ended(&mut app, first));
        assert!(
            app.shown.voice.alone.wait.is_some(),
            "the stale one changed nothing"
        );
        assert!(app.asked.notified.is_empty());
        drop(ended(&mut app, second));
        assert!(app.shown.voice.alone.wait.is_none());
        // Behind other windows, leaving says why.
        assert_eq!(app.asked.notified.len(), 1, "the call is left");
        drop(after(&mut app));
        assert!(
            app.shown.voice.alone.wait.is_none(),
            "not waited on again while the Leave goes out"
        );
        drop(app);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn never_waits_for_nothing() {
        let (mut app, folder) = in_dm_call("never");
        app.layout.edit(|saved| saved.alone_timeout = Some(0));
        alone(&mut app);
        drop(after(&mut app));
        assert!(app.shown.voice.alone.wait.is_none());
        drop(app);
        let _ = std::fs::remove_dir_all(folder);
    }
}
