//! Zooming the whole window, as browsers do: Ctrl with + or -, or with the
//! scroll wheel, steps through [`LEVELS`], and Ctrl+0 returns to 100%.
//!
//! Slint draws everything in logical pixels, so the zoom multiplies the
//! display's own scale factor: text, spacing and pictures all grow together,
//! and the layout gets fewer logical pixels to fill.

use slint::winit_030::winit::{
    event::{ElementState, MouseScrollDelta, WindowEvent},
    keyboard::{Key, KeyCode, PhysicalKey},
};

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

/// Follows Ctrl and the keys and wheel that zoom.
#[derive(Default)]
pub struct Keys {
    control: bool,
    alt: bool,
    shift: bool,
    /// Scrolling not yet worth a step, in pixels; positive scrolls up.
    scrolled: f64,
    /// The keys whose press zoomed, so their release is kept from Slint too.
    pressed: Vec<PhysicalKey>,
}

/// What a window event means for zooming.
pub enum Outcome {
    /// Not about zooming: Slint handles it.
    Pass,
    /// Zooming took it, without a step yet.
    Taken,
    /// Zooming took it, and asks for a step.
    Step(Step),
}

impl Keys {
    /// Ctrl is held, without Alt or Shift.
    pub fn control_only(&self) -> bool {
        self.control && !self.alt && !self.shift
    }

    pub fn event(&mut self, event: &WindowEvent) -> Outcome {
        match event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.control = modifiers.state().control_key();
                self.alt = modifiers.state().alt_key();
                self.shift = modifiers.state().shift_key();
                if !self.control {
                    self.scrolled = 0.;
                }
                Outcome::Pass
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Released {
                    let index = self
                        .pressed
                        .iter()
                        .position(|key| *key == event.physical_key);
                    return match index {
                        Some(index) => {
                            self.pressed.swap_remove(index);
                            Outcome::Taken
                        }
                        None => Outcome::Pass,
                    };
                }
                if !self.control || self.alt {
                    return Outcome::Pass;
                }
                let Some(step) = key_step(&event.logical_key, event.physical_key) else {
                    return Outcome::Pass;
                };
                if !self.pressed.contains(&event.physical_key) {
                    self.pressed.push(event.physical_key);
                }
                Outcome::Step(step)
            }
            WindowEvent::MouseWheel { delta, .. } if self.control => {
                let up = match delta {
                    MouseScrollDelta::LineDelta(_, y) => {
                        self.scrolled = 0.;
                        f64::from(*y)
                    }
                    MouseScrollDelta::PixelDelta(position) => {
                        self.scrolled += position.y;
                        if self.scrolled.abs() < PIXELS_PER_STEP {
                            return Outcome::Taken;
                        }
                        std::mem::take(&mut self.scrolled)
                    }
                };
                if up > 0. {
                    Outcome::Step(Step::In)
                } else if up < 0. {
                    Outcome::Step(Step::Out)
                } else {
                    Outcome::Taken
                }
            }
            WindowEvent::Focused(false) => {
                // Ctrl may be let go elsewhere.
                self.control = false;
                self.alt = false;
                self.shift = false;
                self.scrolled = 0.;
                Outcome::Pass
            }
            _ => Outcome::Pass,
        }
    }
}

/// The step a key asks for with Ctrl held. The typed character decides, so
/// any keyboard layout's + and - work; the keypad's keys and the 0 key's
/// place work too, for layouts whose 0 key types something else.
fn key_step(key: &Key, physical: PhysicalKey) -> Option<Step> {
    if let Key::Character(text) = key {
        match text.as_str() {
            "+" | "=" => return Some(Step::In),
            "-" | "_" => return Some(Step::Out),
            "0" => return Some(Step::Reset),
            _ => {}
        }
    }
    match physical {
        PhysicalKey::Code(KeyCode::NumpadAdd) => Some(Step::In),
        PhysicalKey::Code(KeyCode::NumpadSubtract) => Some(Step::Out),
        PhysicalKey::Code(KeyCode::Digit0 | KeyCode::Numpad0) => Some(Step::Reset),
        _ => None,
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
pub fn most(physical: slint::PhysicalSize, base: f32) -> f32 {
    (physical.width as f32 / (MIN_WIDTH * base)).min(physical.height as f32 / (MIN_HEIGHT * base))
}

/// Draws the window at `level` of its display's scale factor `base`: Slint
/// takes the new scale, then lays the window out in its new logical size.
pub fn apply(window: &slint::Window, base: f32, level: f32) {
    let scale_factor = base * level;
    if (window.scale_factor() - scale_factor).abs() < f32::EPSILON {
        return;
    }
    let physical = window.size();
    let _ = window.dispatch_event_with_result(slint::platform::WindowEvent::ScaleFactorChanged {
        scale_factor,
    });
    let _ = window.dispatch_event_with_result(slint::platform::WindowEvent::Resized {
        size: physical.to_logical(scale_factor),
    });
    window.request_redraw();
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
        let most = most(slint::PhysicalSize::new(1200, 800), 1.);
        assert!((most - 1.4285).abs() < 0.001, "{most}");
        assert_eq!(stepped(1.4, Step::In, most), 1.4);
        assert_eq!(stepped(1.4, Step::Out, most), 1.35);
        // Too small to zoom in at all: out and back to 100% still work.
        assert_eq!(stepped(1.4, Step::In, 0.9), 1.4);
        assert_eq!(stepped(1.4, Step::Reset, 0.9), 1.);
        // A high-DPI display needs twice the pixels.
        assert_eq!(super::most(slint::PhysicalSize::new(2400, 1600), 2.), most);
    }

    #[test]
    fn saved_levels_are_known_ones() {
        assert_eq!(saved(Some(1.25)), 1.25);
        assert_eq!(saved(Some(1.33)), 1.);
        assert_eq!(saved(None), 1.);
    }

    #[test]
    fn ctrl_counts_alone_without_alt_or_shift() {
        use slint::winit_030::winit::keyboard::ModifiersState;
        let mut keys = Keys::default();
        let modifiers = |state: ModifiersState| WindowEvent::ModifiersChanged(state.into());
        keys.event(&modifiers(ModifiersState::CONTROL));
        assert!(keys.control_only());
        keys.event(&modifiers(ModifiersState::CONTROL | ModifiersState::SHIFT));
        assert!(!keys.control_only());
        keys.event(&modifiers(ModifiersState::CONTROL | ModifiersState::ALT));
        assert!(!keys.control_only());
        keys.event(&WindowEvent::Focused(false));
        assert!(!keys.control_only());
    }

    #[test]
    fn touchpads_step_once_per_swipe_worth() {
        use slint::winit_030::winit::{
            dpi::PhysicalPosition,
            event::{DeviceId, TouchPhase},
            keyboard::ModifiersState,
        };
        let mut keys = Keys::default();
        let wheel = |delta| WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta,
            phase: TouchPhase::Moved,
        };
        let pixels = |y| wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(0., y)));
        assert!(
            matches!(keys.event(&pixels(30.)), Outcome::Pass),
            "without Ctrl it scrolls"
        );
        keys.event(&WindowEvent::ModifiersChanged(
            ModifiersState::CONTROL.into(),
        ));
        assert!(matches!(keys.event(&pixels(30.)), Outcome::Taken));
        assert!(matches!(keys.event(&pixels(40.)), Outcome::Step(Step::In)));
        assert!(matches!(keys.event(&pixels(-20.)), Outcome::Taken));
        assert!(matches!(
            keys.event(&wheel(MouseScrollDelta::LineDelta(0., -1.))),
            Outcome::Step(Step::Out)
        ));
    }
}
