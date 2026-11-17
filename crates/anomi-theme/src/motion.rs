//! How the interface moves: the durations and easings of `Theme` and `Popup`
//! in `ui/theme.slint`, and the switch that holds everything still.
//!
//! Nothing moves while the window isn't in front, whatever the setting: a
//! window behind a game draws a change once rather than for every frame of
//! its transition. [`Motion`] says whether it may; every animation takes its
//! duration from it, so a still interface gets zero-length ones.
//!
//! What opens over the window arrives and leaves by [`Popup`]
//! (`PopupMotion` and `PopupEntry` in `ui/theme.slint:216`, `:234`).

mod popup;

pub use popup::{Pace, Popup};

use std::time::Duration;

use iced::animation::{Animation, Easing};

/// A hover fill, a pressed button, a small toggle, a colour that tells a state.
pub const QUICK: Duration = Duration::from_millis(120);
/// What stays on screen and moves or turns: a tab's underline, a selection
/// pill, a chevron, a small bar changing size. With [`GLIDE`].
pub const SLIDE: Duration = Duration::from_millis(180);
/// A highlight that fades once seen, such as the message a jump lands on.
pub const LINGER: Duration = Duration::from_millis(400);
/// A menu, a popover, a picker or a tooltip arriving.
pub const POPUP: Duration = Duration::from_millis(150);
/// A dialog, a sheet, a viewer or the settings arriving.
pub const DIALOG: Duration = Duration::from_millis(220);
/// Any of them leaving.
pub const LEAVE: Duration = Duration::from_millis(120);

/// Arriving, and answering a click: quick at first, then settling;
/// `cubic-bezier(0.22, 1, 0.36, 1)`.
pub const ARRIVE: Easing = Easing::Custom(arrive);
/// Moving while staying on screen: starts and stops gently; CSS `ease-in-out`.
pub const GLIDE: Easing = Easing::Custom(glide);

/// Whether things may move, from the window's focus, the setting and how
/// lately the window was used. Copy it into what animates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Motion {
    /// The window has the focus. A clock on screen ticks only while it does.
    pub front: bool,
    /// Animations are turned off in the settings.
    pub never: bool,
    /// Nobody used the window in the last 15 seconds.
    pub idle: bool,
}

impl Motion {
    /// Animations play: the window has the focus and they aren't turned off.
    pub fn playing(self) -> bool {
        self.front && !self.never
    }

    /// Someone may be looking: what only decorates, such as a skeleton's
    /// sweep, moves only then.
    pub fn watched(self) -> bool {
        self.playing() && !self.idle
    }

    /// Nothing moves: every duration is zero.
    pub fn still(self) -> bool {
        !self.playing()
    }

    /// A token's duration, or zero while still.
    pub fn duration(self, token: Duration) -> Duration {
        if self.still() { Duration::ZERO } else { token }
    }

    /// An animation of a token's length with an easing, which lands at once
    /// while still.
    pub fn animation<T>(self, initial: T, token: Duration, easing: Easing) -> Animation<T>
    where
        T: Clone + Copy + PartialEq + iced::animation::Float,
    {
        Animation::new(initial)
            .duration(self.duration(token))
            .easing(easing)
    }

    /// A popup's way in or out as an iced [`Animation`], for one kept in the
    /// app's state: [`POPUP`] or [`DIALOG`] in, [`LEAVE`] out. Its duration
    /// is the one the move starts with; a popup that may turn round half way
    /// keeps a [`Popup`], which changes pace as it turns.
    pub fn popup(self, open: bool, dialog: bool) -> Animation<bool> {
        let token = match (open, dialog) {
            (false, _) => LEAVE,
            (true, true) => DIALOG,
            (true, false) => POPUP,
        };
        Animation::new(!open)
            .duration(self.duration(token))
            .easing(ARRIVE)
    }
}

/// A popup's card scale for how far it's open, from 0 to 1: it grows from 96%.
pub fn popup_scale(amount: f32) -> f32 {
    0.96 + 0.04 * amount
}

/// How far from its place what slides in starts, in logical pixels.
pub fn popup_shift(amount: f32) -> f32 {
    (1.0 - amount) * 6.0
}

fn arrive(x: f32) -> f32 {
    cubic_bezier(0.22, 1.0, 0.36, 1.0, x)
}

fn glide(x: f32) -> f32 {
    cubic_bezier(0.42, 0.0, 0.58, 1.0, x)
}

/// CSS `cubic-bezier`: solves x(t) = x by Newton's method and returns y(t).
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let curve = |a: f32, b: f32, t: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
    };
    let slope = |a: f32, b: f32, t: f32| {
        let u = 1.0 - t;
        3.0 * u * u * a + 6.0 * u * t * (b - a) + 3.0 * t * t * (1.0 - b)
    };
    let mut t = x;
    for _ in 0..8 {
        let error = curve(x1, x2, t) - x;
        let rate = slope(x1, x2, t);
        if error.abs() < 1e-5 || rate.abs() < 1e-6 {
            break;
        }
        t = (t - error / rate).clamp(0.0, 1.0);
    }
    curve(y1, y2, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_start_and_end_in_place() {
        for curve in [arrive, glide] {
            assert!(curve(0.0).abs() < 1e-4);
            assert!((curve(1.0) - 1.0).abs() < 1e-4);
        }
        // Arriving is mostly done by half way; gliding is half done.
        assert!(arrive(0.5) > 0.85);
        assert!((glide(0.5) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn a_window_behind_another_holds_still() {
        let behind = Motion {
            front: false,
            never: false,
            idle: false,
        };
        assert!(behind.still());
        assert_eq!(behind.duration(SLIDE), Duration::ZERO);
        let front = Motion {
            front: true,
            ..behind
        };
        assert_eq!(front.duration(SLIDE), SLIDE);
        assert!(
            !Motion {
                never: true,
                ..front
            }
            .playing()
        );
        assert!(
            !Motion {
                idle: true,
                ..front
            }
            .watched()
        );
    }
}
