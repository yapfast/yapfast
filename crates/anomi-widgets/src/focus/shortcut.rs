//! The keys and buttons the window takes before anything else has them,
//! wherever the focus is, as `watch_window` does (`src/ui.rs:624`-`:768`):
//! Ctrl+K, Ctrl+F, Alt+Left and Alt+Right, the mouse's back and forward
//! buttons, and Escape for what waits to be let go.

use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Modifiers};
use iced::{Event, mouse};

/// What the window takes first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shortcut {
    /// Ctrl+K.
    Switcher,
    /// Ctrl+F.
    Search,
    /// Alt+Left, the mouse's back button.
    Back,
    /// Alt+Right, the mouse's forward button.
    Forward,
    /// Escape, with or without anything held.
    Escape,
}

/// A shortcut as it came: `act` is false for a held key's repeats and a side
/// button's release, which are kept from the window without acting again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Taken {
    pub shortcut: Shortcut,
    pub act: bool,
}

/// The shortcut `event` is, if any.
pub(super) fn of(event: &Event) -> Option<Taken> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            repeat,
            ..
        }) => Some(Taken {
            shortcut: key_shortcut(key, *physical_key, *modifiers)?,
            act: !repeat,
        }),
        Event::Mouse(mouse::Event::ButtonPressed(button)) => Some(Taken {
            shortcut: side(*button)?,
            act: true,
        }),
        Event::Mouse(mouse::Event::ButtonReleased(button)) => Some(Taken {
            shortcut: side(*button)?,
            act: false,
        }),
        _ => None,
    }
}

fn key_shortcut(key: &Key, physical: Physical, modifiers: Modifiers) -> Option<Shortcut> {
    if matches!(key, Key::Named(Named::Escape)) {
        return Some(Shortcut::Escape);
    }
    if control_only(modifiers) {
        if is_letter(key, physical, "k", Code::KeyK) {
            return Some(Shortcut::Switcher);
        }
        if is_letter(key, physical, "f", Code::KeyF) {
            return Some(Shortcut::Search);
        }
        return None;
    }
    if alt_only(modifiers) {
        return match key {
            Key::Named(Named::ArrowLeft) => Some(Shortcut::Back),
            Key::Named(Named::ArrowRight) => Some(Shortcut::Forward),
            _ => None,
        };
    }
    None
}

/// What the mouse's side buttons do, as Alt+Left and Alt+Right
/// (`side_button_action`, `src/ui.rs:328`).
fn side(button: mouse::Button) -> Option<Shortcut> {
    match button {
        mouse::Button::Back => Some(Shortcut::Back),
        mouse::Button::Forward => Some(Shortcut::Forward),
        _ => None,
    }
}

/// Ctrl is held, without Alt or Shift (`zoom::Keys::control_only`).
fn control_only(modifiers: Modifiers) -> bool {
    modifiers.control() && !modifiers.alt() && !modifiers.shift()
}

/// Alt is held, without Ctrl or Shift.
fn alt_only(modifiers: Modifiers) -> bool {
    modifiers.alt() && !modifiers.control() && !modifiers.shift()
}

/// Whether a key is `letter`'s: that letter in the keyboard's layout, or in
/// a layout without Latin letters, such as a Russian one, the key in its
/// place, `code` (`is_letter`, `src/ui.rs:376`).
fn is_letter(key: &Key, physical: Physical, letter: &str, code: Code) -> bool {
    match key {
        Key::Character(text) if text.chars().all(|ch| ch.is_ascii_alphabetic()) => {
            text.eq_ignore_ascii_case(letter)
        }
        _ => physical == code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letter(logical: &str, pressed: Code, letter: &str, code: Code) -> bool {
        is_letter(
            &Key::Character(logical.into()),
            Physical::Code(pressed),
            letter,
            code,
        )
    }

    /// Ctrl+K and Ctrl+F go by the letter in the keyboard's layout, and in
    /// a layout without Latin letters, by the key in the letter's place.
    #[test]
    fn letter_shortcuts_follow_the_layout_or_the_keys_place() {
        let f = |logical, pressed| letter(logical, pressed, "f", Code::KeyF);
        let k = |logical, pressed| letter(logical, pressed, "k", Code::KeyK);
        assert!(f("f", Code::KeyF) && f("F", Code::KeyF));
        assert!(k("k", Code::KeyK) && k("K", Code::KeyK));
        // A Russian layout: А is on F's key, and Л on K's.
        assert!(f("а", Code::KeyF) && f("А", Code::KeyF));
        assert!(k("л", Code::KeyK));
        assert!(!f("л", Code::KeyK) && !k("а", Code::KeyF));
        // A Latin layout that moves the letters: the letter counts, not the key.
        assert!(f("f", Code::KeyY) && !f("u", Code::KeyF));
        assert!(!f("k", Code::KeyK) && !k("f", Code::KeyF));
        // Other keys.
        assert!(!is_letter(
            &Key::Named(Named::ArrowLeft),
            Physical::Code(Code::ArrowLeft),
            "f",
            Code::KeyF
        ));
    }

    #[test]
    fn ctrl_and_alt_count_alone() {
        let shortcut =
            |key: Key, modifiers| key_shortcut(&key, Physical::Code(Code::KeyK), modifiers);
        let k = || Key::Character("k".into());
        assert_eq!(shortcut(k(), Modifiers::CTRL), Some(Shortcut::Switcher));
        assert_eq!(shortcut(k(), Modifiers::CTRL | Modifiers::SHIFT), None);
        assert_eq!(shortcut(k(), Modifiers::CTRL | Modifiers::ALT), None);
        assert_eq!(shortcut(k(), Modifiers::empty()), None);
        let left = || Key::Named(Named::ArrowLeft);
        assert_eq!(shortcut(left(), Modifiers::ALT), Some(Shortcut::Back));
        assert_eq!(shortcut(left(), Modifiers::ALT | Modifiers::SHIFT), None);
        assert_eq!(shortcut(left(), Modifiers::empty()), None);
        let right = Key::Named(Named::ArrowRight);
        assert_eq!(shortcut(right, Modifiers::ALT), Some(Shortcut::Forward));
        let escape = || Key::Named(Named::Escape);
        assert_eq!(
            shortcut(escape(), Modifiers::empty()),
            Some(Shortcut::Escape)
        );
        assert_eq!(shortcut(escape(), Modifiers::SHIFT), Some(Shortcut::Escape));
    }

    #[test]
    fn the_side_buttons_act_on_the_press_and_keep_the_release() {
        let pressed = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Back));
        let released = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Forward));
        let left = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        assert_eq!(
            of(&pressed),
            Some(Taken {
                shortcut: Shortcut::Back,
                act: true
            })
        );
        assert_eq!(
            of(&released),
            Some(Taken {
                shortcut: Shortcut::Forward,
                act: false
            })
        );
        assert_eq!(of(&left), None);
        assert_eq!(side(mouse::Button::Middle), None);
        assert_eq!(side(mouse::Button::Other(7)), None);
    }
}
