//! A time that runs on its own, such as a call's length, which the widget
//! showing it reads as it draws (`anomi_widgets::clock`): a new second
//! changes no view-model and builds no view.

use std::time::{Duration, Instant};

/// How long something has gone on: from when it started, read as it draws,
/// or stopped at a length, as the fixtures show one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Elapsed {
    Since(Instant),
    Stopped(Duration),
}

impl Elapsed {
    /// How long it has gone on at `now`.
    pub fn at(self, now: Instant) -> Duration {
        match self {
            Elapsed::Since(start) => now.saturating_duration_since(start),
            Elapsed::Stopped(length) => length,
        }
    }

    /// When its whole seconds next turn, after `now`; never for one stopped.
    pub fn next_second(self, now: Instant) -> Option<Instant> {
        let Elapsed::Since(start) = self else {
            return None;
        };
        let gone = now.saturating_duration_since(start);
        let next = Duration::from_secs(gone.as_secs() + 1);
        Some(start + next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_time_turns_at_each_whole_second_and_a_stopped_one_never() {
        let start = Instant::now();
        let now = start + Duration::from_millis(2_300);
        let running = Elapsed::Since(start);
        assert_eq!(running.at(now).as_secs(), 2);
        assert_eq!(
            running.next_second(now),
            Some(start + Duration::from_secs(3))
        );
        let stopped = Elapsed::Stopped(Duration::from_secs(6));
        assert_eq!(stopped.at(now), Duration::from_secs(6));
        assert_eq!(stopped.next_second(now), None);
    }
}
