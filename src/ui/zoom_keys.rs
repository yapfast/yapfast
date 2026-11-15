//! Zooming the Slint window (`yapfast_core::zoom`): winit's events read into
//! zoom's steps, and the level applied to Slint's window. Slint draws
//! everything in logical pixels, so the zoom multiplies the display's own
//! scale factor.

use slint::winit_030::winit::{
    event::{ElementState, MouseScrollDelta, WindowEvent},
    keyboard::{Key, KeyCode, PhysicalKey},
};

use yapfast_core::zoom::{self, KeyPlace, Step, Wheel};

/// Follows Ctrl and the keys and wheel that zoom.
#[derive(Default)]
pub struct Keys {
    control: bool,
    alt: bool,
    shift: bool,
    wheel: Wheel,
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

    /// Alt is held, without Ctrl or Shift.
    pub fn alt_only(&self) -> bool {
        self.alt && !self.control && !self.shift
    }

    /// Shift is held, as far as the window knows: losing the focus lets go
    /// of it, since it may be let go elsewhere.
    pub fn shift(&self) -> bool {
        self.shift
    }

    pub fn event(&mut self, event: &WindowEvent) -> Outcome {
        match event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.control = modifiers.state().control_key();
                self.alt = modifiers.state().alt_key();
                self.shift = modifiers.state().shift_key();
                if !self.control {
                    self.wheel.reset();
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
                let step = match delta {
                    MouseScrollDelta::LineDelta(_, y) => self.wheel.lines(f64::from(*y)),
                    MouseScrollDelta::PixelDelta(position) => self.wheel.pixels(position.y),
                };
                step.map_or(Outcome::Taken, Outcome::Step)
            }
            WindowEvent::Focused(false) => {
                // Ctrl may be let go elsewhere.
                self.control = false;
                self.alt = false;
                self.shift = false;
                self.wheel.reset();
                Outcome::Pass
            }
            _ => Outcome::Pass,
        }
    }
}

/// The step a key asks for with Ctrl held, by [`zoom::key_step`].
fn key_step(key: &Key, physical: PhysicalKey) -> Option<Step> {
    let text = match key {
        Key::Character(text) => Some(text.as_str()),
        _ => None,
    };
    let place = match physical {
        PhysicalKey::Code(KeyCode::NumpadAdd) => KeyPlace::NumpadAdd,
        PhysicalKey::Code(KeyCode::NumpadSubtract) => KeyPlace::NumpadSubtract,
        PhysicalKey::Code(KeyCode::Digit0 | KeyCode::Numpad0) => KeyPlace::Zero,
        _ => KeyPlace::Other,
    };
    zoom::key_step(text, place)
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
    fn ctrl_counts_alone_without_alt_or_shift_and_both_let_go_with_the_focus() {
        use slint::winit_030::winit::keyboard::ModifiersState;
        let mut keys = Keys::default();
        let modifiers = |state: ModifiersState| WindowEvent::ModifiersChanged(state.into());
        keys.event(&modifiers(ModifiersState::CONTROL));
        assert!(keys.control_only());
        keys.event(&modifiers(ModifiersState::CONTROL | ModifiersState::SHIFT));
        assert!(!keys.control_only() && keys.shift());
        keys.event(&modifiers(ModifiersState::CONTROL | ModifiersState::ALT));
        assert!(!keys.control_only() && !keys.shift());
        keys.event(&modifiers(ModifiersState::CONTROL | ModifiersState::SHIFT));
        keys.event(&WindowEvent::Focused(false));
        assert!(!keys.control_only());
        assert!(!keys.shift(), "Shift may be let go elsewhere too");
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
