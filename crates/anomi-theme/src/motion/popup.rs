//! The way in and out of what opens over the window: `PopupMotion` and
//! `PopupEntry` in `ui/theme.slint:216` and `:234`, with the `Popup`
//! global's durations (`:193`).
//!
//! An amount goes from 0 to 1 as a popup opens and back as it closes, on
//! [`ARRIVE`](super::ARRIVE): in at [`POPUP`](super::POPUP) for a menu, a
//! popover, a picker or a tooltip, at [`DIALOG`](super::DIALOG) for a
//! dialog, a sheet, a viewer or the settings, and out at
//! [`LEAVE`](super::LEAVE) for all of them. Turned round half way, it goes
//! back from where it is, as Slint's `animate` does. While the interface
//! holds still it lands at once, so nothing asks for frames.
//!
//! The card grows from 96% as it opens ([`super::popup_scale`]). It doesn't
//! fade: Slint fades it only on the GPU (`Popup.fade`), and tiny-skia, like
//! Skia Software, would draw a fading card into a layer as large as the
//! window. A dimmed backdrop does fade, by its own alpha.

use std::time::{Duration, Instant};

use super::{DIALOG, LEAVE, Motion, POPUP, arrive};

/// How fast a popup arrives: `PopupMotion`'s `popover`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pace {
    /// A menu, a popover, a picker or a tooltip: [`POPUP`].
    #[default]
    Popover,
    /// A dialog, a sheet, a viewer or the settings: [`DIALOG`].
    Dialog,
}

impl Pace {
    /// The duration of the way in.
    pub fn arriving(self) -> Duration {
        match self {
            Self::Popover => POPUP,
            Self::Dialog => DIALOG,
        }
    }
}

/// How far a popup is open, over time. Keep it in the state of what draws
/// the popup, and [`go`](Self::go) whenever whether it's open may have
/// changed; read [`amount`](Self::amount) when drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Popup {
    pace: Pace,
    /// Where the current move started, and where it goes.
    from: f32,
    to: f32,
    start: Instant,
    /// Zero once it has landed, or while still.
    duration: Duration,
}

impl Popup {
    /// A popup already open, as `PopupMotion`'s `init` sets one that is open
    /// when made: it shows whole at once.
    pub fn open(pace: Pace) -> Self {
        Self::at(pace, 1.0)
    }

    /// A popup that isn't open: nothing shows until it [`go`](Self::go)es.
    pub fn closed(pace: Pace) -> Self {
        Self::at(pace, 0.0)
    }

    /// `PopupEntry`: arriving from `now`, for what can't play a way out, or
    /// a popup made as it opens. Whole at once while still.
    pub fn entering(pace: Pace, motion: Motion, now: Instant) -> Self {
        let mut popup = Self::closed(pace);
        popup.go(true, motion, now);
        popup
    }

    fn at(pace: Pace, amount: f32) -> Self {
        Self {
            pace,
            from: amount,
            to: amount,
            start: Instant::now(),
            duration: Duration::ZERO,
        }
    }

    /// Heads for open or closed from `now`, from where it is: the way in at
    /// its pace, the way out at [`LEAVE`]. Nothing changes when it already
    /// heads there.
    pub fn go(&mut self, open: bool, motion: Motion, now: Instant) {
        let to = if open { 1.0 } else { 0.0 };
        if to == self.to {
            if motion.still() {
                self.land();
            }
            return;
        }
        let token = if open { self.pace.arriving() } else { LEAVE };
        self.from = self.amount(now);
        self.to = to;
        self.start = now;
        self.duration = motion.duration(token);
    }

    /// Lands where it heads at once, as when the interface holds still.
    pub fn land(&mut self) {
        self.from = self.to;
        self.duration = Duration::ZERO;
    }

    /// How far open it is at `now`, from 0 to 1.
    pub fn amount(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.to;
        }
        let elapsed = now.saturating_duration_since(self.start).as_secs_f32();
        let progress = (elapsed / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * arrive(progress)
    }

    /// Whether it heads for open.
    pub fn is_open(&self) -> bool {
        self.to > 0.0
    }

    /// `PopupMotion.shown`: it is open, or still on its way out.
    pub fn shown(&self, now: Instant) -> bool {
        self.is_open() || self.amount(now) > 0.0
    }

    /// Whether it still moves at `now`, and wants frames.
    pub fn is_animating(&self, now: Instant) -> bool {
        !self.duration.is_zero() && now < self.start + self.duration
    }

    /// The card's scale at `now`: 96% closed, whole open.
    pub fn scale(&self, now: Instant) -> f32 {
        super::popup_scale(self.amount(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front() -> Motion {
        Motion {
            front: true,
            ..Motion::default()
        }
    }

    #[test]
    fn a_popover_arrives_on_its_pace_and_leaves_quicker() {
        let start = Instant::now();
        let mut popup = Popup::entering(Pace::Popover, front(), start);
        assert_eq!(popup.amount(start), 0.0);
        assert!(popup.is_animating(start + POPUP / 2));
        assert!(popup.amount(start + POPUP / 2) > 0.5);
        assert_eq!(popup.amount(start + POPUP), 1.0);
        assert!(!popup.is_animating(start + POPUP));

        let leaving = start + POPUP;
        popup.go(false, front(), leaving);
        assert!(popup.shown(leaving + LEAVE / 2));
        assert!(!popup.shown(leaving + LEAVE));
        assert!(!popup.is_animating(leaving + LEAVE));
    }

    #[test]
    fn a_dialog_takes_longer_to_arrive() {
        let start = Instant::now();
        let popup = Popup::entering(Pace::Dialog, front(), start);
        assert!(popup.amount(start + POPUP) < 1.0);
        assert_eq!(popup.amount(start + DIALOG), 1.0);
    }

    #[test]
    fn turning_round_goes_back_from_where_it_is() {
        let start = Instant::now();
        let mut popup = Popup::entering(Pace::Popover, front(), start);
        let half = start + POPUP / 3;
        let reached = popup.amount(half);
        popup.go(false, front(), half);
        assert!((popup.amount(half) - reached).abs() < 1e-6);
        assert!(popup.amount(half + LEAVE / 2) < reached);
    }

    #[test]
    fn a_still_interface_lands_at_once() {
        let now = Instant::now();
        let still = Motion::default();
        let mut popup = Popup::entering(Pace::Dialog, still, now);
        assert_eq!(popup.amount(now), 1.0);
        assert!(!popup.is_animating(now));
        popup.go(false, still, now);
        assert!(!popup.shown(now));
        // A move begun in front lands when the window goes behind another.
        let mut moving = Popup::entering(Pace::Popover, front(), now);
        moving.go(true, still, now);
        assert_eq!(moving.amount(now), 1.0);
    }

    #[test]
    fn an_open_popup_shows_whole_and_grows_from_96_percent() {
        let now = Instant::now();
        assert_eq!(Popup::open(Pace::Popover).scale(now), 1.0);
        assert!((Popup::closed(Pace::Popover).scale(now) - 0.96).abs() < 1e-6);
        assert!(!Popup::closed(Pace::Popover).shown(now));
    }
}
