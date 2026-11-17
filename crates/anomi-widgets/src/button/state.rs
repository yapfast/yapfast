//! A [`Face`](super::face::Face)'s tree state: the pointer, the focus and the
//! four animations, the shaped text, and how they advance.

use std::time::{Duration, Instant};

use iced::advanced::widget::operation::Focusable;
use iced::animation::{Animation, Easing};

use anomi_theme::Motion;
use anomi_theme::motion::{ARRIVE, QUICK};

use super::paint::Line;

pub(super) struct State {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    /// The frame the animations are drawn at.
    pub now: Instant,
    /// How lit the face is under the pointer.
    pub lit: Animation<bool>,
    /// How far it gives.
    pub pressed_by: Animation<bool>,
    /// The face's flag (a menu open, a toggle on): the turned icon's half
    /// turn, and the blend of the looks it chooses between.
    pub turn: Animation<bool>,
    /// Whether it is enabled, for the fade between the two looks.
    pub enabled: Animation<bool>,
    /// The text and its caption, shaped in layout: they
    /// reshape only when their content, size or room changes.
    pub text: Line,
    pub caption: Line,
}

impl State {
    pub fn new(open: bool, enabled: bool) -> Self {
        Self {
            hovered: false,
            pressed: false,
            focused: false,
            now: Instant::now(),
            lit: Animation::new(false),
            pressed_by: Animation::new(false),
            turn: Animation::new(open),
            enabled: Animation::new(enabled),
            text: Line::default(),
            caption: Line::default(),
        }
    }

    /// Moves every animation toward what the state asks for at `now`; true
    /// while one still plays and a frame is due.
    pub fn advance(&mut self, now: Instant, open: bool, enabled: bool, motion: Motion) -> bool {
        self.now = now;
        let (hovered, pressed) = (self.hovered, self.pressed);
        self.lit = step(&self.lit, hovered, QUICK, ARRIVE, motion, now);
        self.pressed_by = step(&self.pressed_by, pressed, QUICK, ARRIVE, motion, now);
        self.turn = step(&self.turn, open, QUICK, ARRIVE, motion, now);
        self.enabled = step(&self.enabled, enabled, QUICK, ARRIVE, motion, now);
        [&self.lit, &self.pressed_by, &self.turn, &self.enabled]
            .into_iter()
            .any(|animation| animation.is_animating(now))
    }

    pub fn amount(animation: &Animation<bool>, now: Instant) -> f32 {
        animation.interpolate(0.0f32, 1.0, now)
    }
}

/// An animation that lands at once while still, and otherwise eases from
/// where it is to `target`.
fn step(
    animation: &Animation<bool>,
    target: bool,
    token: Duration,
    easing: Easing,
    motion: Motion,
    now: Instant,
) -> Animation<bool> {
    if motion.still() {
        Animation::new(target)
    } else if animation.value() != target {
        animation
            .clone()
            .duration(token)
            .easing(easing)
            .go(target, now)
    } else {
        animation.clone()
    }
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}
