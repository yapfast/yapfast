//! Whether a theme's motion may run now, and how fast: the one place every
//! theme clock asks. See `docs/themes.md`, Performance.
//!
//! Nobody sees a window that's minimised, closed to the tray, hidden or (as
//! the platform reports it) covered, so nothing of a theme's runs then: no
//! timer, no redraw. Shown but without the focus, the theme's
//! `motion.unfocused` says: `full`, `slow` (the default, at most
//! [`SLOW_FPS`] frames a second) or `still`. Balanced holds still there, as
//! it always has; Light and reduced motion run nothing at all, which each
//! clock knows already.
//!
//! Each clock keeps its own [`Time`], which stands still while its timer
//! does, so motion goes on from where it stopped instead of jumping.

use std::cell::Cell;
use std::time::{Duration, Instant};

use yapfast_core::themes::Performance;
use yapfast_core::themes::package::Unfocused;
use yapfast_ui::YapfastWindow;

/// The most frames a second motion takes while the window isn't focused
/// and the theme asks for `slow`.
pub const SLOW_FPS: u32 = 15;

/// How fast theme motion may run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// At the rate it asks for.
    Full,
    /// At most [`SLOW_FPS`] frames a second.
    Slow,
    /// Not at all: held where it is.
    Still,
}

/// What the window's people can see of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Seen {
    focused: bool,
    /// Shown: made, not hidden and not closed to the tray.
    shown: bool,
    /// Yapfast minimised it and it hasn't been seen since.
    minimized: bool,
    /// The platform says it's minimised.
    platform_minimized: bool,
    /// The platform says nothing of it shows (winit's `Occluded`).
    occluded: bool,
}

impl Default for Seen {
    fn default() -> Self {
        Self {
            focused: false,
            shown: true,
            minimized: false,
            platform_minimized: false,
            occluded: false,
        }
    }
}

impl Seen {
    fn hidden(self) -> bool {
        !self.shown || self.minimized || self.platform_minimized || self.occluded
    }
}

thread_local! {
    static SEEN: Cell<Seen> = Cell::new(Seen::default());
    static UNFOCUSED: Cell<Unfocused> = const { Cell::new(Unfocused::Slow) };
    static PERFORMANCE: Cell<Performance> = const { Cell::new(Performance::Balanced) };
}

/// The pace for what's seen, the setting and the theme's wish.
fn pace_of(seen: Seen, performance: Performance, unfocused: Unfocused) -> Pace {
    if seen.hidden() {
        return Pace::Still;
    }
    if seen.focused {
        return Pace::Full;
    }
    match (performance, unfocused) {
        // Balanced pauses while another window has the focus.
        (Performance::Balanced | Performance::Light, _) | (_, Unfocused::Still) => Pace::Still,
        (Performance::Full, Unfocused::Slow) => Pace::Slow,
        (Performance::Full, Unfocused::Full) => Pace::Full,
    }
}

/// The pace now.
pub fn pace() -> Pace {
    pace_of(SEEN.get(), PERFORMANCE.get(), UNFOCUSED.get())
}

/// Whether nobody can see the window.
pub fn hidden() -> bool {
    SEEN.get().hidden()
}

/// A clock's frames a second at this pace: `rate` at full pace, at most
/// [`SLOW_FPS`] slow, none still.
pub fn cap(rate: u32) -> u32 {
    cap_at(pace(), rate)
}

fn cap_at(pace: Pace, rate: u32) -> u32 {
    match pace {
        Pace::Full => rate,
        Pace::Slow => rate.min(SLOW_FPS),
        Pace::Still => 0,
    }
}

/// How long a step of something that runs every `every` takes at this pace:
/// the same at full pace, at least a [`SLOW_FPS`] frame slow. `None` still.
pub fn stretch(every: Duration) -> Option<Duration> {
    match pace() {
        Pace::Full => Some(every),
        Pace::Slow => Some(every.max(Duration::from_millis(u64::from(1000 / SLOW_FPS)))),
        Pace::Still => None,
    }
}

/// The applied theme's `motion.unfocused`, and the Performance setting.
pub fn set_theme(window: &YapfastWindow, performance: Performance, unfocused: Unfocused) {
    let before = (pace(), hidden());
    PERFORMANCE.set(performance);
    UNFOCUSED.set(unfocused);
    if (pace(), hidden()) != before {
        changed(window);
    }
}

fn update(window: Option<&YapfastWindow>, change: impl FnOnce(&mut Seen)) {
    let before = (pace(), hidden());
    let mut seen = SEEN.get();
    change(&mut seen);
    SEEN.set(seen);
    if (pace(), hidden()) != before
        && let Some(window) = window
    {
        changed(window);
    }
}

/// The window took or lost the focus. A focused window shows, whatever
/// Yapfast thought before.
pub fn focused(window: &YapfastWindow, focused: bool) {
    update(Some(window), |seen| {
        seen.focused = focused;
        if focused {
            seen.minimized = false;
        }
    });
}

/// The platform says the window is or isn't covered.
pub fn occluded(window: &YapfastWindow, occluded: bool) {
    update(Some(window), |seen| {
        seen.occluded = occluded;
        if !occluded {
            seen.minimized = false;
        }
    });
}

/// Yapfast asked for the window to be minimised. Not every platform says
/// when it's back, so anything that shows it is (the focus, the pointer,
/// a resize or a redraw: see [`seen_again`]).
pub fn minimized(window: &YapfastWindow) {
    update(Some(window), |seen| seen.minimized = true);
}

/// Something only a window on screen gets, after Yapfast minimised it.
pub fn seen_again(window: &YapfastWindow) {
    if SEEN.get().minimized {
        update(Some(window), |seen| seen.minimized = false);
    }
}

/// Whether the platform says it's minimised; asked after it resizes.
pub fn platform_minimized(window: &YapfastWindow, minimized: bool) {
    if minimized != SEEN.get().platform_minimized {
        update(Some(window), |seen| seen.platform_minimized = minimized);
    }
}

/// The window was shown or hidden; closing to the tray drops it, which
/// counts as hidden until a new one shows.
pub fn shown(window: Option<&YapfastWindow>, shown: bool) {
    update(window, |seen| {
        seen.shown = shown;
        if !shown {
            seen.focused = false;
        }
        seen.minimized = false;
        seen.platform_minimized = false;
        seen.occluded = false;
    });
}

/// Every theme clock looks at the pace again.
fn changed(window: &YapfastWindow) {
    tracing::debug!(pace = ?pace(), "theme motion pace");
    super::theme_look::repace(window);
    super::theme_art::repace(window);
    super::theme_cursor::repace();
    super::theme_chrome::repace();
}

/// A clock's time: how long it has run, not counting while it was held.
#[derive(Clone, Copy, Debug)]
pub struct Time {
    kept: Duration,
    since: Option<Instant>,
}

impl Time {
    /// Running, from nothing.
    pub fn start() -> Self {
        Self {
            kept: Duration::ZERO,
            since: Some(Instant::now()),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.kept + self.since.map_or(Duration::ZERO, |since| since.elapsed())
    }

    /// Stops it where it is.
    pub fn hold(&mut self) {
        if let Some(since) = self.since.take() {
            self.kept += since.elapsed();
        }
    }

    /// Goes on from where it was held.
    pub fn go(&mut self) {
        if self.since.is_none() {
            self.since = Some(Instant::now());
        }
    }
}

/// A theme clock: its time and the timer that ticks it, at the pace.
pub struct Ticker {
    time: Time,
    timer: slint::Timer,
    every: Duration,
}

impl Default for Ticker {
    fn default() -> Self {
        Self::new()
    }
}

impl Ticker {
    pub fn new() -> Self {
        Self {
            time: Time::start(),
            timer: slint::Timer::default(),
            every: Duration::ZERO,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.time.elapsed()
    }

    /// Ticks `tick` `rate` times a second as the pace allows, or holds
    /// still. Returns whether it (newly or already) runs; a timer already
    /// running at that rate is left as it is.
    pub fn run(&mut self, rate: u32, tick: impl FnMut() + 'static) -> bool {
        let rate = cap(rate);
        if rate == 0 {
            self.timer.stop();
            self.time.hold();
            self.every = Duration::ZERO;
            return false;
        }
        self.time.go();
        let every = Duration::from_millis(u64::from(1000 / rate.max(1)));
        if self.timer.running() && self.every == every {
            return true;
        }
        self.every = every;
        self.timer.start(slint::TimerMode::Repeated, every, tick);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(focused: bool) -> Seen {
        Seen {
            focused,
            ..Seen::default()
        }
    }

    #[test]
    fn nothing_moves_where_nobody_sees() {
        for hide in [
            Seen {
                shown: false,
                ..seen(true)
            },
            Seen {
                minimized: true,
                ..seen(true)
            },
            Seen {
                platform_minimized: true,
                ..seen(true)
            },
            Seen {
                occluded: true,
                ..seen(true)
            },
        ] {
            for performance in Performance::ALL {
                for unfocused in Unfocused::ALL {
                    assert_eq!(pace_of(hide, performance, unfocused), Pace::Still);
                }
            }
        }
    }

    #[test]
    fn unfocused_follows_the_theme_in_full_and_holds_in_balanced() {
        let away = seen(false);
        assert_eq!(
            pace_of(away, Performance::Full, Unfocused::default()),
            Pace::Slow
        );
        assert_eq!(
            pace_of(away, Performance::Full, Unfocused::Full),
            Pace::Full
        );
        assert_eq!(
            pace_of(away, Performance::Full, Unfocused::Still),
            Pace::Still
        );
        assert_eq!(
            pace_of(away, Performance::Balanced, Unfocused::Full),
            Pace::Still
        );
        assert_eq!(
            pace_of(seen(true), Performance::Balanced, Unfocused::Still),
            Pace::Full
        );
    }

    #[test]
    fn slow_caps_at_fifteen_frames() {
        assert_eq!(cap_at(Pace::Full, 60), 60);
        assert_eq!(cap_at(Pace::Slow, 60), 15);
        assert_eq!(cap_at(Pace::Slow, 4), 4);
        assert_eq!(cap_at(Pace::Still, 60), 0);
    }

    #[test]
    fn held_time_goes_on_from_where_it_stopped() {
        let mut time = Time::start();
        std::thread::sleep(Duration::from_millis(20));
        time.hold();
        let held = time.elapsed();
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(time.elapsed(), held);
        time.go();
        std::thread::sleep(Duration::from_millis(5));
        let after = time.elapsed();
        assert!(after >= held && after < held + Duration::from_millis(30));
    }
}
