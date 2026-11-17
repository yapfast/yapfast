//! The clock every moving picture draws its frame from: it runs only while
//! something plays, so a window behind another loses no frame and draws
//! none, and one that comes back goes on from where it stopped. The Slint
//! side: the `Timer` of `Picture` (`ui/controls.slint:423`), which ran while
//! `Motion.playing`, and the shared clock of `src/ui/emoji_animation.rs`
//! (`catch_up`).
//!
//! Sharing it means two pictures of one animation show the same frame,
//! and pictures whose frames change together wake the window once.
//!
//! A picture held still by its own [`Motion`](anomi_theme::Motion), as an
//! avatar drawn without one, stops the clock only in a frame where nothing
//! plays: a frame's pictures pause and tick in any order, and one that
//! played keeps the clock going for the rest.

use std::cell::Cell;
use std::time::{Duration, Instant};

/// The shortest a frame shows, as Slint's `max(20, delay)`: a file that
/// asks for none still moves at a pace a screen shows.
pub(crate) const SHORTEST: u32 = 20;

/// The most the clock moves at once: a frame the widgets lost track of,
/// such as the first after a long while with nothing moving on screen,
/// moves it no further.
const LONGEST_STEP: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
struct Clock {
    elapsed: Duration,
    /// The frame time it last moved at; none before it first does.
    last: Option<Instant>,
    /// The first frame time since then that a picture was held still at.
    /// A tick at that same time undoes it, as something played in that
    /// frame; a tick at a later one goes on from there, as the clock
    /// stood still between.
    paused: Option<Instant>,
}

thread_local! {
    static CLOCK: Cell<Clock> = const {
        Cell::new(Clock {
            elapsed: Duration::ZERO,
            last: None,
            paused: None,
        })
    };
}

/// Moves the clock to `now`, a frame's time, and says how long it has run.
/// Every picture drawn in one frame reads the same time.
pub(crate) fn tick(now: Instant) -> Duration {
    CLOCK.with(|cell| {
        let mut clock = cell.get();
        let ran = clock.paused.is_none_or(|paused| paused == now);
        if let Some(last) = clock.last
            && ran
        {
            clock.elapsed += now.saturating_duration_since(last).min(LONGEST_STEP);
        }
        clock.last = Some(clock.last.map_or(now, |last| last.max(now)));
        clock.paused = None;
        cell.set(clock);
        clock.elapsed
    })
}

/// Stops the clock at `now`, a frame's time, until a [`tick`] at a later
/// one, from which it goes on: nothing plays while the window is behind
/// another or animations are off. A frame something played in already, or
/// plays in later, isn't stopped.
pub(crate) fn pause(now: Instant) {
    CLOCK.with(|cell| {
        let mut clock = cell.get();
        if clock.last != Some(now) && clock.paused.is_none() {
            clock.paused = Some(now);
        }
        cell.set(clock);
    });
}

/// Which frame shows and for how much longer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct At {
    pub index: usize,
    /// Until the next frame; none once it holds its last frame.
    pub wait: Option<Duration>,
}

/// The frame an animation of `delays` (milliseconds) shows `at` into its
/// play, going round again, or holding its last frame after `plays` rounds
/// (0 for ever), as a browser plays an APNG.
pub(crate) fn frame_at(delays: &[u32], plays: u32, at: Duration) -> At {
    let shown = |delay: u32| Duration::from_millis(u64::from(delay.max(SHORTEST)));
    let round: Duration = delays.iter().map(|&delay| shown(delay)).sum();
    if delays.len() < 2 || round.is_zero() {
        return At {
            index: 0,
            wait: None,
        };
    }
    if plays > 0 && at >= round * plays {
        return At {
            index: delays.len() - 1,
            wait: None,
        };
    }
    let round_nanos = round.as_nanos();
    let mut into = Duration::from_nanos((at.as_nanos() % round_nanos) as u64);
    for (index, &delay) in delays.iter().enumerate() {
        let lasts = shown(delay);
        if into < lasts {
            return At {
                index,
                wait: Some(lasts - into),
            };
        }
        into -= lasts;
    }
    At {
        index: delays.len() - 1,
        wait: Some(Duration::from_millis(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn frames_follow_their_delays_round_and_round() {
        let delays = [100, 200];
        let at = |millis| frame_at(&delays, 0, ms(millis));
        assert_eq!(
            at(0),
            At {
                index: 0,
                wait: Some(ms(100))
            }
        );
        assert_eq!(
            at(150),
            At {
                index: 1,
                wait: Some(ms(150))
            }
        );
        assert_eq!(
            at(300),
            At {
                index: 0,
                wait: Some(ms(100))
            },
            "round again"
        );
        assert_eq!(
            at(3_250),
            At {
                index: 1,
                wait: Some(ms(50))
            }
        );
    }

    #[test]
    fn a_counted_animation_holds_its_last_frame() {
        let held = frame_at(&[100, 200], 2, ms(600));
        assert_eq!(
            held,
            At {
                index: 1,
                wait: None
            }
        );
        assert_eq!(frame_at(&[100, 200], 2, ms(350)).index, 0, "second round");
    }

    #[test]
    fn a_frame_shows_at_least_twenty_milliseconds() {
        assert_eq!(
            frame_at(&[0, 5], 0, ms(10)),
            At {
                index: 0,
                wait: Some(ms(10))
            }
        );
        assert_eq!(frame_at(&[0, 5], 0, ms(25)).index, 1);
    }

    #[test]
    fn a_still_never_waits() {
        assert_eq!(
            frame_at(&[], 0, ms(500)),
            At {
                index: 0,
                wait: None
            }
        );
        assert_eq!(
            frame_at(&[80], 0, ms(500)),
            At {
                index: 0,
                wait: None
            }
        );
    }

    #[test]
    fn the_clock_runs_only_between_pauses() {
        let start = Instant::now();
        let from = tick(start);
        assert_eq!(tick(start + ms(40)) - from, ms(40));
        // Every picture of one frame reads one time.
        assert_eq!(tick(start + ms(40)) - from, ms(40));
        pause(start + ms(50));
        // Behind another window for a minute: no time passed.
        assert_eq!(tick(start + ms(60_000)) - from, ms(40));
        assert_eq!(tick(start + ms(60_016)) - from, ms(56));
        // A frame lost for long moves it a step at most.
        assert_eq!(tick(start + ms(200_000)) - from, ms(56) + LONGEST_STEP);
    }

    #[test]
    fn a_picture_held_still_stops_no_frame_another_plays_in() {
        let start = Instant::now();
        let from = tick(start);
        // Held still before the one playing, then after it: the clock runs.
        pause(start + ms(100));
        assert_eq!(tick(start + ms(100)) - from, ms(100));
        pause(start + ms(100));
        pause(start + ms(200));
        assert_eq!(tick(start + ms(200)) - from, ms(200));
        // A frame nothing played in stops it, whatever comes later.
        pause(start + ms(300));
        pause(start + ms(400));
        assert_eq!(tick(start + ms(400)) - from, ms(200));
        assert_eq!(tick(start + ms(450)) - from, ms(250));
    }
}
