//! How long the call has lasted, under its status in the voice dock. Its
//! one timer runs only during a call, ticking once a second while the
//! window is in front: behind another, as behind a game, the time holds and
//! the window draws nothing for it, and it's right again as the window is
//! back.

use std::time::{Duration, Instant};

use slint::{ComponentHandle, Timer, TimerMode};

use super::{Adapter, YapfastWindow};

/// The call it's timing and when it connected.
#[derive(Default)]
pub(super) struct CallTimer {
    since: Option<(String, Instant)>,
    timer: Timer,
}

impl Adapter {
    /// Starts timing the call in `room` once it connects, and stops when it
    /// ends; a call that moves to another room starts again. Called again
    /// as the window gains or loses the focus.
    pub(super) fn time_call(&mut self, window: &YapfastWindow, room: Option<&str>) {
        let same = match (&self.call_timer.since, room) {
            (Some((timed, _)), Some(room)) => timed == room,
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.call_timer.timer.stop();
            let Some(room) = room else {
                self.call_timer.since = None;
                window.set_voice_time(Default::default());
                return;
            };
            self.call_timer.since = Some((room.to_owned(), Instant::now()));
            window.set_voice_time(elapsed(Duration::ZERO).into());
        }
        let Some(&(_, started)) = self.call_timer.since.as_ref() else {
            return;
        };
        if !super::FOCUSED.get() {
            self.call_timer.timer.stop();
            // A window that opens behind another shows the time once.
            if window.get_voice_time().is_empty() {
                window.set_voice_time(elapsed(started.elapsed()).into());
            }
            return;
        }
        if self.call_timer.timer.running() {
            return;
        }
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

impl CallTimer {
    /// Stops ticking, as the window closes; the next one's refresh starts
    /// it again.
    pub(super) fn stop(&mut self) {
        self.timer.stop();
    }

    #[cfg(test)]
    pub(super) fn ticking(&self) -> bool {
        self.timer.running()
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
