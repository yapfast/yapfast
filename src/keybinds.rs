//! Keybinds, as Discord's: keys held together that do something in any app,
//! such as push to talk or toggling mute. The watcher is
//! [`anomi_desktop::keybinds`], which the iced client shares; here are the
//! window's own key events, as winit gives them: the keys a recording takes
//! (`recorded_key`, `mouse_key`), and the keys that stand in for global ones
//! where nothing else hears keys (`window_event`).
use std::sync::atomic::{AtomicBool, Ordering};

pub use anomi_desktop::keybinds::*;
#[cfg(windows)]
use slint::winit_030::winit::event::MouseButton;
use slint::winit_030::winit::{
    event::{ElementState, KeyEvent, WindowEvent},
    keyboard::{Key as LogicalKey, KeyLocation, NamedKey, PhysicalKey},
    platform::scancode::PhysicalKeyExtScancode,
};

/// The key a key press names, unless it has no scan code.
pub fn recorded_key(event: &KeyEvent) -> Option<Key> {
    let scancode = event.physical_key.to_scancode()?;
    Some(Key {
        scancode,
        name: name(event),
        trigger: trigger(event),
    })
}

/// Mouse 4 or 5 on Windows, which keybinds may hold too.
#[cfg(windows)]
pub fn mouse_key(button: MouseButton) -> Option<Key> {
    let (scancode, name) = match button {
        MouseButton::Back => (MOUSE_4, "Mouse 4"),
        MouseButton::Forward => (MOUSE_5, "Mouse 5"),
        _ => return None,
    };
    Some(Key {
        scancode,
        name: name.into(),
        trigger: None,
    })
}

/// The key without Shift or Caps Lock, so a letter reads as itself.
fn unmodified(event: &KeyEvent) -> LogicalKey {
    #[cfg(any(target_os = "linux", windows))]
    {
        use slint::winit_030::winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
        event.key_without_modifiers()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        event.logical_key.clone()
    }
}

fn name(event: &KeyEvent) -> String {
    let side = match event.location {
        KeyLocation::Left => "Left ",
        KeyLocation::Right => "Right ",
        KeyLocation::Numpad => "Numpad ",
        KeyLocation::Standard => "",
    };
    match unmodified(event) {
        LogicalKey::Character(text) => format!("{side}{}", text.to_uppercase()),
        LogicalKey::Named(named) => {
            let named = match named {
                NamedKey::Control => "Ctrl".to_owned(),
                NamedKey::Super | NamedKey::Meta => "Super".to_owned(),
                NamedKey::ArrowUp => "Up".to_owned(),
                NamedKey::ArrowDown => "Down".to_owned(),
                NamedKey::ArrowLeft => "Left".to_owned(),
                NamedKey::ArrowRight => "Right".to_owned(),
                NamedKey::Space => "Space".to_owned(),
                named => format!("{named:?}"),
            };
            format!("{side}{named}")
        }
        _ => match event.physical_key {
            PhysicalKey::Code(code) => format!("{code:?}"),
            PhysicalKey::Unidentified(_) => "Unknown key".to_owned(),
        },
    }
}

/// The key's keysym name, as the XDG shortcuts specification writes a
/// trigger; on Linux, where only the portal needs it.
#[cfg(target_os = "linux")]
fn trigger(event: &KeyEvent) -> Option<String> {
    let right = event.location == KeyLocation::Right;
    let side =
        |left: &str, right_name: &str| Some(if right { right_name } else { left }.to_owned());
    match unmodified(event) {
        LogicalKey::Character(text) => {
            let mut chars = text.chars();
            let (Some(char), None) = (chars.next(), chars.next()) else {
                return None;
            };
            let name = xkeysym::Keysym::from_char(char).name()?;
            Some(name.strip_prefix("XK_").unwrap_or(name).to_owned())
        }
        LogicalKey::Named(named) => match named {
            NamedKey::Control => side("Control_L", "Control_R"),
            NamedKey::Shift => side("Shift_L", "Shift_R"),
            NamedKey::Alt => side("Alt_L", "Alt_R"),
            NamedKey::Super | NamedKey::Meta => side("Super_L", "Super_R"),
            NamedKey::AltGraph => Some("ISO_Level3_Shift".to_owned()),
            NamedKey::CapsLock => Some("Caps_Lock".to_owned()),
            NamedKey::ScrollLock => Some("Scroll_Lock".to_owned()),
            NamedKey::NumLock => Some("Num_Lock".to_owned()),
            NamedKey::PageUp => Some("Prior".to_owned()),
            NamedKey::PageDown => Some("Next".to_owned()),
            NamedKey::Space => Some("space".to_owned()),
            NamedKey::Enter => Some("Return".to_owned()),
            NamedKey::Backspace => Some("BackSpace".to_owned()),
            NamedKey::ArrowUp => Some("Up".to_owned()),
            NamedKey::ArrowDown => Some("Down".to_owned()),
            NamedKey::ArrowLeft => Some("Left".to_owned()),
            NamedKey::ArrowRight => Some("Right".to_owned()),
            NamedKey::Tab
            | NamedKey::Insert
            | NamedKey::Delete
            | NamedKey::Home
            | NamedKey::End
            | NamedKey::Pause => Some(format!("{named:?}")),
            NamedKey::ContextMenu => Some("Menu".to_owned()),
            named => {
                let name = format!("{named:?}");
                // F1 to F35 are their own keysyms.
                (name.starts_with('F') && name[1..].parse::<u8>().is_ok()).then_some(name)
            }
        },
        _ => None,
    }
}

#[cfg(not(target_os = "linux"))]
fn trigger(_event: &KeyEvent) -> Option<String> {
    None
}

/// The window's latest key press holds a push-to-talk keybind.
static PRESSED_TALK_KEY: AtomicBool = AtomicBool::new(false);

/// Whether the key press Slint is handling is a push-to-talk key, which
/// shouldn't also type into the composer.
pub fn pressed_talk_key() -> bool {
    PRESSED_TALK_KEY.load(Ordering::Relaxed)
}

/// Runs keybinds from the window's own events while no global keys work.
/// Losing the focus lets them go, as their keys' release won't come.
pub fn window_event(event: &WindowEvent) {
    match event {
        WindowEvent::KeyboardInput { event, .. } => {
            let code = event.physical_key.to_scancode();
            let down = event.state == ElementState::Pressed;
            if down {
                PRESSED_TALK_KEY.store(code.is_some_and(is_talk_key), Ordering::Relaxed);
            }
            if code.is_some() {
                window_key(code, down);
            }
        }
        #[cfg(windows)]
        WindowEvent::MouseInput { state, button, .. } => {
            if let Some(key) = mouse_key(*button) {
                window_key(Some(key.scancode), *state == ElementState::Pressed);
            }
        }
        WindowEvent::Focused(false) => window_key(None, false),
        _ => {}
    }
}

#[cfg(all(test, windows))]
mod tests {
    use slint::winit_030::winit::event::MouseButton;

    use super::*;

    #[test]
    fn the_window_records_mouse_4_and_5() {
        for (button, name, scancode) in [
            (MouseButton::Back, "Mouse 4", MOUSE_4),
            (MouseButton::Forward, "Mouse 5", MOUSE_5),
        ] {
            let key = mouse_key(button).expect("a side button should be recordable");
            assert_eq!(key.scancode, scancode);
            assert_eq!(key.name, name);
            assert_eq!(key.trigger, None);
        }
        assert!(
            mouse_key(MouseButton::Left).is_none(),
            "ordinary buttons keep their usual meaning"
        );
    }
}
