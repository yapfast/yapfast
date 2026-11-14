//! Leaving a private Discord call after everyone else has gone.
//!
//! The adapter owns the deadline, so closing the window to the tray does not
//! cancel it. Every firing checks the call again: someone arriving, a new call,
//! or disabling the setting makes an old deadline harmless.

use std::time::{Duration, Instant};

use slint::{Timer, TimerMode};
use yapfast_core::{
    application::{Action, AppState, Effect},
    chat::Command,
    network::Network,
    voice::{self, Phase},
};

use super::Adapter;

#[derive(Default)]
pub(super) struct AloneTimer {
    /// The private call and when it became empty, not merely when it started.
    pub(super) started: Option<(String, Instant)>,
    wait: Option<Duration>,
    timer: Timer,
}

impl AloneTimer {
    fn stop(&mut self) {
        self.started = None;
        self.wait = None;
        self.timer.stop();
    }
}

/// The connected private Discord call where only this device is left.
fn empty_private_call(state: &AppState) -> Option<&str> {
    if !matches!(state.voice.phase, Phase::Connected | Phase::Reconnecting) {
        return None;
    }
    let id = state.voice.room.as_deref()?;
    let room = state.rooms.iter().find(|room| room.id == id)?;
    if !room.direct || room.network != Network::Discord {
        return None;
    }
    let local_connected = state
        .voice
        .participants
        .iter()
        .any(|person| person.local && person.connected);
    let someone_else = state
        .voice
        .participants
        .iter()
        .any(|person| !person.local && person.connected);
    (local_connected && !someone_else).then_some(id)
}

impl Adapter {
    /// Starts or stops the deadline as the call, its people, or the setting
    /// changes. Changing the setting keeps the time already spent alone.
    pub(super) fn sync_alone_timer(&mut self) {
        let Some(wait) = self.layout.alone_timeout() else {
            self.alone.stop();
            return;
        };
        let Some(room) = empty_private_call(&self.app.state).map(str::to_owned) else {
            self.alone.stop();
            return;
        };
        let new_call = self
            .alone
            .started
            .as_ref()
            .is_none_or(|(current, _)| current != &room);
        if new_call {
            self.alone.started = Some((room, Instant::now()));
        }
        if !new_call && self.alone.wait == Some(wait) {
            return;
        }
        self.alone.wait = Some(wait);
        let started = self.alone.started.as_ref().unwrap().1;
        self.arm_alone_timer(wait.saturating_sub(started.elapsed()));
    }

    fn arm_alone_timer(&self, remaining: Duration) {
        let adapter = self.weak.clone();
        self.alone
            .timer
            .start(TimerMode::SingleShot, remaining, move || {
                if let Some(adapter) = adapter.upgrade() {
                    adapter.borrow_mut().fire_alone_timeout();
                }
            });
    }

    /// Leaves only if the same private call is still empty and its whole wait
    /// has passed. An early wake-up schedules the remainder instead.
    pub(super) fn fire_alone_timeout(&mut self) {
        let Some(wait) = self.layout.alone_timeout() else {
            self.alone.stop();
            return;
        };
        let Some(room) = empty_private_call(&self.app.state) else {
            self.alone.stop();
            return;
        };
        let Some((timed_room, started)) = self.alone.started.clone() else {
            return;
        };
        if timed_room != room {
            self.sync_alone_timer();
            return;
        }
        let elapsed = started.elapsed();
        if elapsed < wait {
            self.arm_alone_timer(wait - elapsed);
            return;
        }

        self.alone.stop();
        let effects = self.app.dispatch(Action::Voice(voice::Command::Leave));
        self.execute(effects);
        if let Some(window) = self.window.upgrade() {
            self.refresh(&window);
        }
        self.show_tray();
    }

    /// Cancels as soon as an action asks to leave, including the Leave + Join
    /// effects of a switch or accepted call. Waiting for the backend event
    /// would leave a due callback able to append a Leave in the new call.
    pub(super) fn sync_alone_after_effects(&mut self, effects: &[Effect]) {
        let leaving = effects.iter().any(|effect| {
            matches!(
                effect,
                Effect::Backend(Command::Voice(voice::Command::Leave))
            )
        });
        if leaving {
            self.alone.stop();
        } else {
            self.sync_alone_timer();
        }
    }
}
