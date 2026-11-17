//! Zooming the whole window, as browsers do: Ctrl with + or -, or with the
//! scroll wheel, steps through [`LEVELS`], and Ctrl+0 returns to 100%. A
//! port of `src/zoom.rs` without winit: what a key or the wheel asks, the
//! level it leads to, and the note the window shows for a moment after
//! (`zoom-note`, `ui/yapfast.slint:5425`). The app applies the level as the
//! window's scale factor (task I01).

use iced::keyboard::{self, Key, key};
use iced::mouse::ScrollDelta;

/// The zoom levels, smallest first: 5% apart up to 150%, where small changes
/// matter most, then 10% apart.
pub const LEVELS: [f32; 26] = [
    0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9, 0.95, 1.0, 1.05, 1.1, 1.15, 1.2, 1.25, 1.3,
    1.35, 1.4, 1.45, 1.5, 1.6, 1.7, 1.8, 1.9, 2.0,
];

/// The least logical size the window lays out in, `YapfastWindow`'s
/// `min-width` and `min-height`: zooming in stops before the window would
/// have to grow to fit it.
const MIN_WIDTH: f32 = 640.0;
const MIN_HEIGHT: f32 = 560.0;

/// Touchpads scroll in pixels, many events to a swipe; this much scrolling
/// makes one step.
const PIXELS_PER_STEP: f32 = 60.0;

/// What Ctrl and a key or the wheel asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    In,
    Out,
    Reset,
}

/// The step a key pressed with Ctrl, and without Alt, asks for (Shift may
/// be held: + takes it on most layouts). The typed character decides, so
/// any keyboard layout's + and - work; the keypad's keys and the 0 key's
/// place work too, for layouts whose 0 key types something else.
pub fn key_step(
    key: &Key,
    physical: key::Physical,
    modifiers: keyboard::Modifiers,
) -> Option<Step> {
    if !modifiers.control() || modifiers.alt() {
        return None;
    }
    if let Key::Character(text) = key {
        match text.as_str() {
            "+" | "=" => return Some(Step::In),
            "-" | "_" => return Some(Step::Out),
            "0" => return Some(Step::Reset),
            _ => {}
        }
    }
    match physical {
        key::Physical::Code(key::Code::NumpadAdd) => Some(Step::In),
        key::Physical::Code(key::Code::NumpadSubtract) => Some(Step::Out),
        key::Physical::Code(key::Code::Digit0 | key::Code::Numpad0) => Some(Step::Reset),
        _ => None,
    }
}

/// The wheel with Ctrl held: whole lines step at once, a touchpad's pixels
/// once they add up to a step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wheel {
    /// Scrolling not yet worth a step, in pixels; positive scrolls up.
    scrolled: f32,
}

impl Wheel {
    /// The step a scroll asks for, if any.
    pub fn scroll(&mut self, delta: ScrollDelta) -> Option<Step> {
        let up = match delta {
            ScrollDelta::Lines { y, .. } => {
                self.scrolled = 0.0;
                y
            }
            ScrollDelta::Pixels { y, .. } => {
                self.scrolled += y;
                if self.scrolled.abs() < PIXELS_PER_STEP {
                    return None;
                }
                std::mem::take(&mut self.scrolled)
            }
        };
        if up > 0.0 {
            Some(Step::In)
        } else if up < 0.0 {
            Some(Step::Out)
        } else {
            None
        }
    }

    /// Ctrl let go, or the window lost the focus: what was scrolled goes.
    pub fn forget(&mut self) {
        self.scrolled = 0.0;
    }
}

/// The level a step leads to from `level`, no further in than `most`.
pub fn stepped(level: f32, step: Step, most: f32) -> f32 {
    let next = match step {
        Step::Reset => 1.0,
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
        .unwrap_or(1.0)
}

/// The furthest the window can zoom in at its physical size and its
/// display's scale factor before its layout would need more room.
pub fn most(physical_width: f32, physical_height: f32, base: f32) -> f32 {
    (physical_width / (MIN_WIDTH * base)).min(physical_height / (MIN_HEIGHT * base))
}

/// The note the window shows for a level: "110%".
pub fn note(level: f32) -> String {
    format!("{}%", (level * 100.0).round())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_through_the_levels() {
        assert_eq!(stepped(1.0, Step::In, 3.0), 1.05);
        assert_eq!(stepped(1.0, Step::Out, 3.0), 0.95);
        assert_eq!(stepped(1.5, Step::In, 3.0), 1.6, "10% apart above 150%");
        assert_eq!(stepped(2.0, Step::In, 3.0), 2.0, "the largest stays");
        assert_eq!(stepped(0.5, Step::Out, 3.0), 0.5, "the smallest stays");
        assert_eq!(stepped(1.5, Step::Reset, 3.0), 1.0);
        // A level between two steps goes to the next one either way.
        assert_eq!(stepped(1.52, Step::In, 3.0), 1.6);
        assert_eq!(stepped(1.52, Step::Out, 3.0), 1.5);
    }

    #[test]
    fn zooming_in_stops_before_the_window_must_grow() {
        let most = most(1200.0, 800.0, 1.0);
        assert!((most - 1.4285).abs() < 0.001, "{most}");
        assert_eq!(stepped(1.4, Step::In, most), 1.4);
        assert_eq!(stepped(1.4, Step::Out, most), 1.35);
        // Too small to zoom in at all: out and back to 100% still work.
        assert_eq!(stepped(1.4, Step::In, 0.9), 1.4);
        assert_eq!(stepped(1.4, Step::Reset, 0.9), 1.0);
        // A high-DPI display needs twice the pixels.
        assert_eq!(super::most(2400.0, 1600.0, 2.0), most);
    }

    #[test]
    fn a_narrow_window_limits_zooming_by_its_width() {
        // Half of a 1366 pixel screen, 560 high: 683 / 640 is the most.
        let most = most(683.0, 700.0, 1.0);
        assert!((most - 1.0672).abs() < 0.001, "{most}");
        assert_eq!(stepped(1.0, Step::In, most), 1.05);
        assert_eq!(stepped(1.05, Step::In, most), 1.05);
    }

    #[test]
    fn only_known_levels_are_kept() {
        assert_eq!(saved(Some(1.1)), 1.1);
        assert_eq!(saved(Some(1.12)), 1.0);
        assert_eq!(saved(None), 1.0);
    }

    #[test]
    fn keys_with_control_step() {
        let control = keyboard::Modifiers::CTRL;
        let unknown = key::Physical::Unidentified(key::NativeCode::Unidentified);
        let plus = Key::Character("+".into());
        let minus = Key::Character("-".into());
        assert_eq!(key_step(&plus, unknown, control), Some(Step::In));
        assert_eq!(
            key_step(&plus, unknown, control | keyboard::Modifiers::SHIFT),
            Some(Step::In),
            "+ is typed with Shift"
        );
        assert_eq!(key_step(&minus, unknown, control), Some(Step::Out));
        assert_eq!(
            key_step(&minus, unknown, keyboard::Modifiers::empty()),
            None
        );
        assert_eq!(
            key_step(&minus, unknown, control | keyboard::Modifiers::ALT),
            None
        );
        let zero = key::Physical::Code(key::Code::Digit0);
        assert_eq!(
            key_step(&Key::Character("à".into()), zero, control),
            Some(Step::Reset),
            "the 0 key's place on a layout that types something else"
        );
    }

    #[test]
    fn a_touchpad_steps_once_its_pixels_add_up() {
        let mut wheel = Wheel::default();
        assert_eq!(wheel.scroll(ScrollDelta::Pixels { x: 0.0, y: 30.0 }), None);
        assert_eq!(
            wheel.scroll(ScrollDelta::Pixels { x: 0.0, y: 31.0 }),
            Some(Step::In)
        );
        assert_eq!(wheel.scroll(ScrollDelta::Pixels { x: 0.0, y: -30.0 }), None);
        assert_eq!(
            wheel.scroll(ScrollDelta::Lines { x: 0.0, y: -1.0 }),
            Some(Step::Out)
        );
    }

    #[test]
    fn the_note_names_the_level() {
        assert_eq!(note(1.1), "110%");
        assert_eq!(note(0.55), "55%");
    }
}
