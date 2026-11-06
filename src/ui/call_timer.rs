//! How long the call has lasted, under its status in the voice dock. Its
//! one timer runs only during a call, ticking once a second.

use std::time::{Duration, Instant};

use slint::{ComponentHandle, Timer, TimerMode};

use super::{Adapter, YapfastWindow};

/// The call it's timing and when it connected.
#[derive(Default)]
pub(super) struct CallTimer {
    since: Option<(String, Instant)>,
    timer: Timer,
}

impl CallTimer {
    /// Stops the clock with the window it counts in; the call's time stays, for
    /// the window that opens next.
    pub(super) fn pause(&mut self) {
        self.timer.stop();
    }
}

impl Adapter {
    /// Starts timing the call in `room` once it connects, and stops when it
    /// ends; a call that moves to another room starts again.
    pub(super) fn time_call(&mut self, window: &YapfastWindow, room: Option<&str>) {
        let same = match (&self.call_timer.since, room) {
            // A paused one goes on in the window that opened, from when it began.
            (Some((timed, _)), Some(room)) => timed == room && self.call_timer.timer.running(),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        let Some(room) = room else {
            self.call_timer.since = None;
            self.call_timer.timer.stop();
            window.set_voice_time(Default::default());
            return;
        };
        let started = match &self.call_timer.since {
            Some((timed, started)) if timed == room => *started,
            _ => Instant::now(),
        };
        self.call_timer.since = Some((room.to_owned(), started));
        window.set_voice_time(elapsed(started.elapsed()).into());
        let window = window.as_weak();
        self.call_timer
            .timer
            .start(TimerMode::Repeated, Duration::from_secs(1), move || {
                if let Some(window) = window.upgrade() {
                    window.set_voice_time(elapsed(started.elapsed()).into());
                }
            });
    }
}

/// `0:42`, `12:05`, or `1:02:03`.
fn elapsed(time: Duration) -> String {
    let seconds = time.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_minutes_then_hours() {
        assert_eq!(elapsed(Duration::from_secs(42)), "0:42");
        assert_eq!(elapsed(Duration::from_secs(12 * 60 + 5)), "12:05");
        assert_eq!(elapsed(Duration::from_secs(3723)), "1:02:03");
    }
}
