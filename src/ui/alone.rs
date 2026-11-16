//! Leaving a private Discord call no one else is in, as Discord's own apps
//! do: Settings › Voice & video chooses how long it waits, or Never.
//!
//! One single-shot timer runs while such a call is empty. Each action and
//! backend event checks the call in a few comparisons, without a lookup, and
//! allocates only as a call empties. The timer runs with the window closed to
//! the tray, and leaving tells of it in a notification while Yapfast isn't in
//! front.

use std::time::Duration;

use slint::{Timer, TimerMode};
use yapfast_core::{
    application::{Action, AppState, Effect},
    chat::{Alert, Command},
    voice::{self, Phase},
};
use yapfast_lang::trn;

use super::{Adapter, FOCUSED, Layout};

/// The waits Settings offers, in minutes, in its order: Never, then three
/// minutes, as long as Discord's apps wait, and longer.
const WAITS: [u32; 5] = [0, 3, 10, 30, 60];
/// The choice of [`WAITS`] until another is made.
const DEFAULT: usize = 1;

/// The minutes a choice of Settings waits.
pub(super) fn minutes(choice: i32) -> u32 {
    usize::try_from(choice)
        .ok()
        .and_then(|choice| WAITS.get(choice))
        .copied()
        .unwrap_or(WAITS[DEFAULT])
}

impl Layout {
    /// The choice of [`WAITS`] the saved wait is, or the one nearest it.
    pub(super) fn alone_choice(&self) -> usize {
        match self.alone_timeout {
            None => DEFAULT,
            Some(0) => 0,
            Some(saved) => (1..WAITS.len())
                .min_by_key(|&choice| WAITS[choice].abs_diff(saved))
                .unwrap_or(DEFAULT),
        }
    }

    /// How long a private Discord call waits with no one else in it; none
    /// to stay in it.
    fn alone_wait(&self) -> Option<Duration> {
        let minutes = WAITS[self.alone_choice()];
        (minutes > 0).then(|| Duration::from_secs(u64::from(minutes) * 60))
    }
}

/// The empty private call, and the timer that leaves it.
#[derive(Default)]
pub(super) struct AloneTimer {
    /// The call's room, from when no one else was in it. It stays while a
    /// Leave goes out, so that the call isn't waited on again as it ends.
    room: Option<String>,
    timer: Timer,
}

/// The private Discord call this account is connected to with no one else
/// connected: those it still rings haven't come.
fn empty_private_call(state: &AppState) -> Option<&str> {
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

impl Adapter {
    /// Starts the wait as the call this account is in empties, and stops it
    /// as someone comes or the call ends. `effects` are what the action or
    /// event set off: a Leave in them, alone or before joining another call,
    /// stops the wait before the call shows it ending, so that the wait
    /// can't end meanwhile and leave the call joined next.
    pub(super) fn watch_alone(&mut self, effects: &[Effect]) {
        let leaving = effects.iter().any(|effect| {
            matches!(
                effect,
                Effect::Backend(Command::Voice(voice::Command::Leave))
            )
        });
        if leaving {
            self.alone.timer.stop();
            return;
        }
        let Some(room) = empty_private_call(&self.app.state) else {
            if self.alone.room.take().is_some() {
                self.alone.timer.stop();
            }
            return;
        };
        if self.alone.room.as_deref() == Some(room) {
            return;
        }
        self.alone.room = Some(room.to_owned());
        let Some(wait) = self.layout.alone_wait() else {
            self.alone.timer.stop();
            return;
        };
        let adapter = self.weak.clone();
        self.alone
            .timer
            .start(TimerMode::SingleShot, wait, move || {
                if let Some(adapter) = adapter.upgrade() {
                    adapter.borrow_mut().leave_alone();
                }
            });
    }

    /// The wait chosen changed: an empty call waits it again from now, so a
    /// shorter one never leaves at once.
    pub(super) fn alone_wait_changed(&mut self) {
        if self.alone.room.take().is_some() {
            self.alone.timer.stop();
        }
        self.watch_alone(&[]);
    }

    /// The wait ended: leaves the call, as its Leave button does, if it's
    /// still the one that emptied.
    fn leave_alone(&mut self) {
        let still = empty_private_call(&self.app.state)
            .is_some_and(|room| self.alone.room.as_deref() == Some(room));
        let (Some(room), Some(wait)) = (self.alone.room.clone(), self.layout.alone_wait()) else {
            return;
        };
        if !still {
            self.alone.room = None;
            self.alone.timer.stop();
            self.watch_alone(&[]);
            return;
        }
        let minutes = wait.as_secs() / 60;
        tracing::info!(minutes, "leaving a DM call no one else is in");
        let name = self.app.state.voice.name.clone();
        let leave = Action::Voice(voice::Command::Leave);
        match self.window.upgrade() {
            Some(window) => self.action(&window, leave),
            None => {
                let effects = self.app.dispatch(leave);
                self.watch_alone(&effects);
                self.execute(effects);
                self.show_tray();
            }
        }
        self.tell_left_alone(room, name, minutes);
    }

    /// Says why the call ended while Yapfast isn't in front, where its sound
    /// may go unheard; in front, the voice dock closing shows it.
    fn tell_left_alone(&self, room: String, name: String, minutes: u64) {
        let state = &self.app.state;
        if FOCUSED.get() || !self.layout.notifications.unwrap_or(true) || state.do_not_disturb() {
            return;
        }
        let picture = state
            .rooms
            .iter()
            .find(|found| found.id == room)
            .and_then(|found| found.avatar.as_ref())
            .and_then(|avatar| state.pictures.get(avatar).cloned());
        let body = trn!(
            "You left: no one else was in the call for {n} minute",
            "You left: no one else was in the call for {n} minutes",
            minutes
        );
        self.notifier.show(
            Alert {
                room,
                title: name,
                body,
                picture,
                call: false,
                mention: false,
            },
            crate::notify::Style { text: true },
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc, sync::Arc};

    use super::super::*;
    use super::*;

    const DM: &str = "discord:@me:20";

    #[test]
    fn the_wait_is_three_minutes_until_another_is_chosen() {
        let saved = |json: serde_json::Value| serde_json::from_value::<Layout>(json).unwrap();
        let fresh = saved(serde_json::json!({}));
        assert_eq!(fresh.alone_choice(), 1);
        assert_eq!(fresh.alone_wait(), Some(Duration::from_secs(3 * 60)));
        assert_eq!(
            saved(serde_json::json!({ "alone_timeout": 0 })).alone_wait(),
            None
        );
        // A wait Settings doesn't offer goes by the nearest it does.
        assert_eq!(
            saved(serde_json::json!({ "alone_timeout": 1 })).alone_choice(),
            1
        );
        assert_eq!(
            saved(serde_json::json!({ "alone_timeout": 25 })).alone_choice(),
            3
        );
        assert_eq!(
            saved(serde_json::json!({ "alone_timeout": 1000 })).alone_choice(),
            4
        );
        assert_eq!(minutes(4), 60);
        assert_eq!(minutes(-1), 3);
    }

    #[test]
    fn settings_show_and_save_the_wait() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let (directory, adapter) = adapter(&window);
        bind_callbacks(&window, &adapter);
        Layout::default().apply(&window);
        assert_eq!(window.get_alone_wait(), 1);
        window.set_alone_wait(2);
        window.invoke_preferences_changed();
        assert_eq!(Layout::read(directory.path()).alone_timeout, Some(10));
    }

    fn adapter(window: &YapfastWindow) -> (tempfile::TempDir, Rc<RefCell<Adapter>>) {
        let directory = tempfile::tempdir().unwrap();
        let adapter = Adapter::new(
            Arc::new(BackgroundRuntime::new().unwrap()),
            directory.path().into(),
            window,
        )
        .unwrap();
        (directory, adapter)
    }

    fn person(id: &str, local: bool, connected: bool) -> voice::Participant {
        voice::Participant {
            id: id.into(),
            user: id.into(),
            person: id.into(),
            local,
            muted: false,
            deafened: false,
            server_muted: false,
            server_deafened: false,
            speaking: false,
            connected,
            problem: None,
            streaming: false,
            video: None,
            avatar: None,
        }
    }

    /// This account connected to `room`'s call, with `others`.
    fn call(adapter: &mut Adapter, room: &str, others: Vec<voice::Participant>) {
        adapter.app.state.voice = voice::State {
            phase: Phase::Connected,
            room: Some(room.into()),
            participants: std::iter::once(person("me", true, true))
                .chain(others)
                .collect(),
            ..Default::default()
        };
        adapter.watch_alone(&[]);
    }

    fn waiting(adapter: &Adapter) -> Option<Duration> {
        let alone = &adapter.alone;
        alone.timer.running().then(|| alone.timer.interval())
    }

    #[test]
    fn only_a_private_discord_call_with_no_one_else_waits() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let (_directory, adapter) = adapter(&window);
        let mut adapter = adapter.borrow_mut();
        let three = Some(Duration::from_secs(3 * 60));

        call(&mut adapter, DM, vec![person("sam", false, true)]);
        assert_eq!(waiting(&adapter), None);
        // Someone it still rings hasn't come.
        call(&mut adapter, DM, vec![person("sam", false, false)]);
        assert_eq!(waiting(&adapter), three);
        call(&mut adapter, DM, vec![person("sam", false, true)]);
        assert_eq!(waiting(&adapter), None);
        call(&mut adapter, "discord:10:30", vec![]);
        assert_eq!(waiting(&adapter), None, "a server's voice channel");
        call(&mut adapter, "!room:example.org", vec![]);
        assert_eq!(waiting(&adapter), None, "a Matrix call");

        call(&mut adapter, DM, vec![]);
        assert_eq!(waiting(&adapter), three);
        adapter.app.state.voice.phase = Phase::Leaving;
        adapter.watch_alone(&[]);
        assert_eq!(waiting(&adapter), None);
    }

    #[test]
    fn a_changed_wait_starts_again_and_never_stays() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let (_directory, adapter) = adapter(&window);
        let mut adapter = adapter.borrow_mut();
        call(&mut adapter, DM, vec![]);

        adapter.layout.alone_timeout = Some(0);
        adapter.alone_wait_changed();
        assert_eq!(waiting(&adapter), None);
        // Events in the call meanwhile change nothing.
        adapter.watch_alone(&[]);
        assert_eq!(waiting(&adapter), None);
        adapter.layout.alone_timeout = Some(30);
        adapter.alone_wait_changed();
        assert_eq!(waiting(&adapter), Some(Duration::from_secs(30 * 60)));
    }

    #[test]
    fn leaving_stops_the_wait_until_the_call_ends() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let (_directory, adapter) = adapter(&window);
        let mut adapter = adapter.borrow_mut();
        call(&mut adapter, DM, vec![]);
        assert!(waiting(&adapter).is_some());

        // As Leave, or accepting another call, sends it: the call still
        // shows until the backend says it ended.
        adapter.action(&window, Action::Voice(voice::Command::Leave));
        assert_eq!(waiting(&adapter), None);
        adapter.watch_alone(&[]);
        assert_eq!(waiting(&adapter), None, "not waited on again as it ends");
        adapter.app.state.voice = voice::State::default();
        adapter.watch_alone(&[]);
        call(&mut adapter, DM, vec![]);
        assert!(waiting(&adapter).is_some(), "the next empty call waits");
    }

    #[test]
    fn the_wait_ending_leaves_the_call_it_waited_on() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let (_directory, adapter) = adapter(&window);
        let mut adapter = adapter.borrow_mut();
        // The preview leaves a call at once, as the backend reports later.
        adapter.app.dispatch(Action::Preview);

        call(&mut adapter, DM, vec![]);
        adapter
            .app
            .state
            .voice
            .participants
            .push(person("sam", false, true));
        // A call that changed without a check meanwhile stays.
        adapter.leave_alone();
        assert!(adapter.app.state.voice.active());
        assert_eq!(waiting(&adapter), None);

        call(&mut adapter, DM, vec![]);
        adapter.leave_alone();
        assert!(!adapter.app.state.voice.active());
        adapter.watch_alone(&[]);
        assert_eq!(waiting(&adapter), None);
        assert!(adapter.alone.room.is_none());
    }
}
