//! Scrolling the list while a drag holds the pointer near its top or bottom
//! edge, or past it, faster the farther in or past it is
//! (`scroll_near_edge` in `yapfast_selection.rs`): Slint stepped 16 ms
//! ticks of a timer; here each frame steps by the time since the last,
//! through the list's `scroll_by`, which any scrollable answers.

use iced::advanced::widget::Id;
use iced::advanced::widget::operation::scrollable::{AbsoluteOffset, Scrollable};
use iced::advanced::widget::operation::{Animation, Operation};
use iced::{Rectangle, Size, Vector};

use super::pass::Scroller;

/// How near the edge the pointer scrolls (`EDGE`, `yapfast_selection.rs:470`).
pub const EDGE: f32 = 24.0;
/// How far into the edge or past it still speeds the scroll up.
const REACH: f32 = 120.0;
/// A quarter of how far in or past it the list moves each tick of this
/// long, in seconds: 30 px at most (`yapfast_selection.rs:476`).
const TICK: f32 = 0.016;
const SPEED: f32 = 0.25;
/// The longest frame stepped whole: after a stall the list doesn't leap.
const LONGEST: f32 = 0.05;

/// How far the list moves in a tick with the pointer at `y`, towards its end
/// when positive; nothing away from the edges.
pub fn speed(bounds: Rectangle, y: f32) -> f32 {
    let above = bounds.y + EDGE - y;
    let below = y - (bounds.y + bounds.height - EDGE);
    if above > 0.0 {
        -above.min(REACH) * SPEED
    } else if below > 0.0 {
        below.min(REACH) * SPEED
    } else {
        0.0
    }
}

/// How far it moves in a frame `elapsed` seconds after the last.
pub fn step(speed: f32, elapsed: f32) -> f32 {
    speed * elapsed.clamp(0.0, LONGEST) / TICK
}

/// Asks the scrollable `target` to move `delta` pixels down.
pub struct Nudge {
    target: Scroller,
    delta: f32,
    done: bool,
    /// How far the scrollables around have scrolled their content, and
    /// how far the last one met has, for what's inside it.
    translation: Vector,
    pending: Vector,
}

impl Nudge {
    pub fn new(target: Scroller, delta: f32) -> Self {
        Self {
            target,
            delta,
            done: false,
            translation: Vector::ZERO,
            pending: Vector::ZERO,
        }
    }

    /// Whether the scrollable laid out at `bounds` is the target, which the
    /// walk found where it shows in the window.
    fn is_target(&self, id: Option<&Id>, bounds: Rectangle) -> bool {
        match &self.target.id {
            Some(target) => id == Some(target),
            None => id.is_none() && bounds - self.translation == self.target.bounds,
        }
    }
}

impl Operation for Nudge {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let translation = self.translation;
        self.translation += std::mem::take(&mut self.pending);
        if !self.done {
            operate(self);
        }
        self.translation = translation;
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Size,
        translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        self.pending = translation;
        if !self.done && self.is_target(id, bounds) {
            state.scroll_by(
                AbsoluteOffset {
                    x: 0.0,
                    y: self.delta,
                },
                Animation::Instant,
                bounds,
                content,
            );
            self.done = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Point;

    #[test]
    fn scrolls_faster_the_farther_past_an_edge() {
        let bounds = Rectangle::new(Point::new(0.0, 100.0), Size::new(400.0, 500.0));
        assert_eq!(speed(bounds, 300.0), 0.0, "away from the edges");
        assert!(speed(bounds, 110.0) < 0.0, "near the top it goes up");
        assert!(speed(bounds, 595.0) > 0.0, "near the bottom it goes down");
        assert!(
            speed(bounds, 40.0) < speed(bounds, 110.0),
            "past it, faster"
        );
        assert_eq!(speed(bounds, -500.0), -30.0, "30 px a tick at most");
        assert!((step(-30.0, TICK) + 30.0).abs() < 1e-3);
        assert_eq!(
            step(-30.0, 2.0),
            step(-30.0, LONGEST),
            "a stall doesn't leap"
        );
    }
}
