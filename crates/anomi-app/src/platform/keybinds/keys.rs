//! The window's keys as keybinds know them: by the scan code winit gives a
//! key, which `layout.json` keeps and both clients share, with its name and,
//! for the portal, its keysym (`recorded_key`, `name`, `trigger` in the Slint
//! app's `src/keybinds.rs`). iced gives a key's place as winit's code,
//! converted: the scan code is found from winit's own table, read once.

use std::collections::HashMap;
use std::sync::OnceLock;

use iced::keyboard::key::{Named, Physical};
use iced::keyboard::{self, Key as Logical, Location};
use iced::{Event, window};
use iced_winit::conversion;
use iced_winit::winit::keyboard::PhysicalKey;
use iced_winit::winit::platform::scancode::PhysicalKeyExtScancode;
use yapfast_core::keybinds::Key;

use crate::message::Message;

/// The scan codes winit knows, as Linux's evdev or Windows' set 1 has them.
fn codes() -> impl Iterator<Item = u32> {
    if cfg!(windows) {
        (0..0x100)
            .chain(0xE000..0xE100)
            .collect::<Vec<_>>()
            .into_iter()
    } else {
        (0..0x300).collect::<Vec<_>>().into_iter()
    }
}

/// iced's key at `scancode`, as winit converts it.
pub fn physical(scancode: u32) -> Physical {
    conversion::physical_key(PhysicalKey::from_scancode(scancode))
}

/// The scan code winit gives the key at `physical`; none for a key it has
/// none for.
pub fn scancode(physical: Physical) -> Option<u32> {
    static TABLE: OnceLock<HashMap<Physical, u32>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = HashMap::new();
        for code in codes() {
            let found = PhysicalKey::from_scancode(code);
            if found.to_scancode() == Some(code) {
                table.entry(conversion::physical_key(found)).or_insert(code);
            }
        }
        table
    });
    table.get(&physical).copied()
}

/// A key of the window's, from its event filter (`crate::window`), which
/// runs on iced's executor, not the window's thread: it runs keybinds while
/// no global keys work, and comes back as a message while a keybind records.
/// The focus going lets every key go, as their release won't come.
pub fn heard(event: &Event) -> Option<Message> {
    let (key, physical, location, pressed, repeat) = match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            location,
            repeat,
            ..
        }) => (key, *physical_key, *location, true, *repeat),
        Event::Keyboard(keyboard::Event::KeyReleased {
            key,
            physical_key,
            location,
            ..
        }) => (key, *physical_key, *location, false, false),
        Event::Window(window::Event::Unfocused) => {
            anomi_desktop::keybinds::window_key(None, false);
            return None;
        }
        _ => return None,
    };
    if let Some(code) = scancode(physical) {
        anomi_desktop::keybinds::window_key(Some(code), pressed);
    }
    super::record::recording().then(|| {
        Message::Recorded(super::record::Input {
            key: key.clone(),
            physical,
            location,
            pressed,
            repeat,
        })
    })
}

/// The key a key press names, unless it has no scan code: `key` is the key
/// without Shift or Caps Lock, as iced gives it, so a letter reads as itself.
pub fn recorded(key: &Logical, physical_key: Physical, location: Location) -> Option<Key> {
    Some(Key {
        scancode: scancode(physical_key)?,
        name: name(key, physical_key, location),
        trigger: trigger(key, location),
    })
}

fn name(key: &Logical, physical_key: Physical, location: Location) -> String {
    let side = match location {
        Location::Left => "Left ",
        Location::Right => "Right ",
        Location::Numpad => "Numpad ",
        Location::Standard => "",
    };
    match key {
        Logical::Character(text) => format!("{side}{}", text.to_uppercase()),
        Logical::Named(named) => {
            let named = match named {
                Named::Control => "Ctrl".to_owned(),
                Named::Super | Named::Meta => "Super".to_owned(),
                Named::ArrowUp => "Up".to_owned(),
                Named::ArrowDown => "Down".to_owned(),
                Named::ArrowLeft => "Left".to_owned(),
                Named::ArrowRight => "Right".to_owned(),
                Named::Space => "Space".to_owned(),
                named => format!("{named:?}"),
            };
            format!("{side}{named}")
        }
        Logical::Unidentified => match physical_key {
            Physical::Code(code) => format!("{code:?}"),
            Physical::Unidentified(_) => "Unknown key".to_owned(),
        },
    }
}

/// The key's keysym name, as the XDG shortcuts specification writes a
/// trigger; on Linux, where only the portal needs it.
#[cfg(target_os = "linux")]
fn trigger(key: &Logical, location: Location) -> Option<String> {
    let right = location == Location::Right;
    let side =
        |left: &str, right_name: &str| Some(if right { right_name } else { left }.to_owned());
    let simple = |name: &str| Some(name.to_owned());
    match key {
        Logical::Character(text) => {
            let mut chars = text.chars();
            let (Some(char), None) = (chars.next(), chars.next()) else {
                return None;
            };
            let name = xkeysym::Keysym::from_char(char).name()?;
            Some(name.strip_prefix("XK_").unwrap_or(name).to_owned())
        }
        Logical::Named(named) => match named {
            Named::Control => side("Control_L", "Control_R"),
            Named::Shift => side("Shift_L", "Shift_R"),
            Named::Alt => side("Alt_L", "Alt_R"),
            Named::Super | Named::Meta => side("Super_L", "Super_R"),
            Named::AltGraph => simple("ISO_Level3_Shift"),
            Named::CapsLock => simple("Caps_Lock"),
            Named::ScrollLock => simple("Scroll_Lock"),
            Named::NumLock => simple("Num_Lock"),
            Named::PageUp => simple("Prior"),
            Named::PageDown => simple("Next"),
            Named::Space => simple("space"),
            Named::Enter => simple("Return"),
            Named::Backspace => simple("BackSpace"),
            Named::ArrowUp => simple("Up"),
            Named::ArrowDown => simple("Down"),
            Named::ArrowLeft => simple("Left"),
            Named::ArrowRight => simple("Right"),
            Named::Tab
            | Named::Insert
            | Named::Delete
            | Named::Home
            | Named::End
            | Named::Pause => Some(format!("{named:?}")),
            Named::ContextMenu => simple("Menu"),
            named => {
                let name = format!("{named:?}");
                // F1 to F35 are their own keysyms.
                (name.starts_with('F') && name[1..].parse::<u8>().is_ok()).then_some(name)
            }
        },
        Logical::Unidentified => None,
    }
}

#[cfg(not(target_os = "linux"))]
fn trigger(_key: &Logical, _location: Location) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use iced::keyboard::key::Code;

    use super::*;

    #[test]
    fn a_key_is_known_by_winits_scan_code_both_ways() {
        // evdev's codes on Linux, set 1's on Windows.
        let (v, left_ctrl, right_ctrl) = if cfg!(windows) {
            (0x2F, 0x1D, 0xE01D)
        } else {
            (47, 29, 97)
        };
        assert_eq!(scancode(Physical::Code(Code::KeyV)), Some(v));
        assert_eq!(scancode(Physical::Code(Code::ControlLeft)), Some(left_ctrl));
        assert_eq!(
            scancode(Physical::Code(Code::ControlRight)),
            Some(right_ctrl)
        );
        assert_eq!(physical(v), Physical::Code(Code::KeyV));
        assert_eq!(scancode(physical(right_ctrl)), Some(right_ctrl));
    }

    #[test]
    fn a_recorded_key_has_its_name_and_keysym() {
        let v = recorded(
            &Logical::Character("v".into()),
            Physical::Code(Code::KeyV),
            Location::Standard,
        )
        .unwrap();
        assert_eq!(v.name, "V");
        let ctrl = recorded(
            &Logical::Named(Named::Control),
            Physical::Code(Code::ControlRight),
            Location::Right,
        )
        .unwrap();
        assert_eq!(ctrl.name, "Right Ctrl");
        if cfg!(target_os = "linux") {
            assert_eq!(v.trigger.as_deref(), Some("v"));
            assert_eq!(ctrl.trigger.as_deref(), Some("Control_R"));
        }
    }
}
