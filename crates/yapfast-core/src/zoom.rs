//! Zooming the whole window, as browsers do: Ctrl with + or -, or with the
//! scroll wheel, steps through [`LEVELS`], and Ctrl+0 returns to 100%.
//!
//! The zoom multiplies the display's own scale factor: text, spacing and
//! pictures all grow together, and the layout gets fewer logical pixels to
//! fill. What a key or the wheel asks for and where the levels stop are here;
//! each window toolkit reads its own events into them and draws at the level.

/// The zoom levels, smallest first: 5% apart up to 150%, where small changes
/// matter most, then 10% apart.
pub const LEVELS: [f32; 26] = [
    0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9, 0.95, 1.0, 1.05, 1.1, 1.15, 1.2, 1.25, 1.3,
    1.35, 1.4, 1.45, 1.5, 1.6, 1.7, 1.8, 1.9, 2.0,
];

/// The least logical size Yapfast's window lays out in, as `YapfastWindow`'s
/// `min-width` and `min-height` in `ui/yapfast.slint` say. Zooming in stops
/// before the window would have to grow to fit it.
const MIN_WIDTH: f32 = 800.;
const MIN_HEIGHT: f32 = 560.;

/// Touchpads scroll in pixels, many events to a swipe; this much scrolling
/// makes one step.
const PIXELS_PER_STEP: f64 = 60.;

/// What Ctrl and a key or the wheel asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    In,
    Out,
    Reset,
}

/// Where a key is, for the keys that zoom whatever they type: the keypad's
/// + and -, and the 0 key's place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyPlace {
    NumpadAdd,
    NumpadSubtract,
    Zero,
    Other,
}

/// The step a key asks for with Ctrl held. The typed character decides, so
/// any keyboard layout's + and - work; the keypad's keys and the 0 key's
/// place work too, for layouts whose 0 key types something else.
pub fn key_step(text: Option<&str>, place: KeyPlace) -> Option<Step> {
    match text {
        Some("+" | "=") => return Some(Step::In),
        Some("-" | "_") => return Some(Step::Out),
        Some("0") => return Some(Step::Reset),
        _ => {}
    }
    match place {
        KeyPlace::NumpadAdd => Some(Step::In),
        KeyPlace::NumpadSubtract => Some(Step::Out),
        KeyPlace::Zero => Some(Step::Reset),
        KeyPlace::Other => None,
    }
}

/// The scroll wheel with Ctrl held: a mouse's notch is a step, a
/// touchpad's many small scrolls one step for each [`PIXELS_PER_STEP`].
#[derive(Default)]
pub struct Wheel {
    /// Scrolling not yet worth a step, in pixels; positive scrolls up.
    scrolled: f64,
}

impl Wheel {
    /// A wheel's notches, positive up: a step, unless it's none.
    pub fn lines(&mut self, up: f64) -> Option<Step> {
        self.scrolled = 0.;
        direction(up)
    }

    /// A touchpad's pixels, positive up: a step once there are enough.
    pub fn pixels(&mut self, up: f64) -> Option<Step> {
        self.scrolled += up;
        if self.scrolled.abs() < PIXELS_PER_STEP {
            return None;
        }
        direction(std::mem::take(&mut self.scrolled))
    }

    /// Ctrl went up, or the window lost the focus.
    pub fn reset(&mut self) {
        self.scrolled = 0.;
    }
}

fn direction(up: f64) -> Option<Step> {
    if up > 0. {
        Some(Step::In)
    } else if up < 0. {
        Some(Step::Out)
    } else {
        None
    }
}

/// The level a step leads to from `level`, no further in than `most`.
pub fn stepped(level: f32, step: Step, most: f32) -> f32 {
    let next = match step {
        Step::Reset => 1.,
        Step::In => LEVELS
            .into_iter()
            .find(|candidate| *candidate > level + 0.001)
            .unwrap_or(level),
        Step::Out => LEVELS
            .into_iter()
            .rev()
            .find(|candidate| *candidate < level - 0.001)
            .unwrap_or(level),
    };
    // Zooming in stops where the window would have to grow; zooming out
    // and resetting always may, even in a window too small to zoom in.
    if next > level && next > most {
        level
    } else {
        next
    }
}

/// A saved level, if it's one of [`LEVELS`]; else 100%.
pub fn saved(level: Option<f32>) -> f32 {
    level
        .and_then(|level| {
            LEVELS
                .into_iter()
                .find(|known| (known - level).abs() < 0.001)
        })
        .unwrap_or(1.)
}

/// The furthest the window can zoom in at its physical size and its
/// display's scale factor before its layout would need more room.
pub fn most(width: u32, height: u32, base: f32) -> f32 {
    (width as f32 / (MIN_WIDTH * base)).min(height as f32 / (MIN_HEIGHT * base))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_through_the_levels() {
        assert_eq!(stepped(1., Step::In, 3.), 1.05);
        assert_eq!(stepped(1., Step::Out, 3.), 0.95);
        assert_eq!(stepped(1.5, Step::In, 3.), 1.6, "10% apart above 150%");
        assert_eq!(stepped(2., Step::In, 3.), 2., "the largest stays");
        assert_eq!(stepped(0.5, Step::Out, 3.), 0.5, "the smallest stays");
        assert_eq!(stepped(1.5, Step::Reset, 3.), 1.);
        // A level between two steps goes to the next one either way.
        assert_eq!(stepped(1.52, Step::In, 3.), 1.6);
        assert_eq!(stepped(1.52, Step::Out, 3.), 1.5);
    }

    #[test]
    fn zooming_in_stops_before_the_window_must_grow() {
        let most = most(1200, 800, 1.);
        assert!((most - 1.4285).abs() < 0.001, "{most}");
        assert_eq!(stepped(1.4, Step::In, most), 1.4);
        assert_eq!(stepped(1.4, Step::Out, most), 1.35);
        // Too small to zoom in at all: out and back to 100% still work.
        assert_eq!(stepped(1.4, Step::In, 0.9), 1.4);
        assert_eq!(stepped(1.4, Step::Reset, 0.9), 1.);
        // A high-DPI display needs twice the pixels.
        assert_eq!(super::most(2400, 1600, 2.), most);
    }

    #[test]
    fn saved_levels_are_known_ones() {
        assert_eq!(saved(Some(1.25)), 1.25);
        assert_eq!(saved(Some(1.33)), 1.);
        assert_eq!(saved(None), 1.);
    }

    #[test]
    fn touchpads_step_once_per_swipe_worth() {
        let mut wheel = Wheel::default();
        assert_eq!(wheel.pixels(30.), None);
        assert_eq!(wheel.pixels(40.), Some(Step::In));
        assert_eq!(wheel.pixels(-20.), None);
        assert_eq!(wheel.lines(-1.), Some(Step::Out));
    }

    #[test]
    fn keys_zoom_by_what_they_type_or_where_they_are() {
        assert_eq!(key_step(Some("="), KeyPlace::Other), Some(Step::In));
        assert_eq!(key_step(Some("_"), KeyPlace::Other), Some(Step::Out));
        assert_eq!(key_step(Some("à"), KeyPlace::Zero), Some(Step::Reset));
        assert_eq!(key_step(None, KeyPlace::NumpadAdd), Some(Step::In));
        assert_eq!(key_step(Some("k"), KeyPlace::Other), None);
    }
}
