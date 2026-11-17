//! Keys nothing in the window took, as the vendored Slint offers them to the
//! app (`vendor/i-slint-core/yapfast_typing.rs`, `window.rs:1553`-`:1598`):
//! with neither Ctrl, Alt nor Super held, Tab and Shift+Tab, Escape, and
//! keys that write: text, Backspace and Delete. Not Enter, nor keys that
//! write nothing.

use iced::Event;
use iced::keyboard::{self, Key, key::Named};

/// A key nothing took that the window acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unclaimed {
    /// Tab, or Shift+Tab when `back`.
    Tab {
        back: bool,
    },
    Escape,
    /// Text, Backspace or Delete; `plain` when it's only letters and
    /// digits, which alone leave the timeline's keys for the composer.
    Text {
        plain: bool,
    },
}

/// What `event` is of those, if it's one.
pub(super) fn of(event: &Event) -> Option<Unclaimed> {
    let Event::Keyboard(keyboard::Event::KeyPressed {
        key,
        modifiers,
        text,
        ..
    }) = event
    else {
        return None;
    };
    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return None;
    }
    match key.as_ref() {
        Key::Named(Named::Tab) => Some(Unclaimed::Tab {
            back: modifiers.shift(),
        }),
        Key::Named(Named::Escape) => Some(Unclaimed::Escape),
        Key::Named(Named::Enter) => None,
        Key::Named(Named::Backspace | Named::Delete) => Some(Unclaimed::Text { plain: false }),
        _ => {
            let text = text.as_deref().filter(|text| writes(text))?;
            Some(Unclaimed::Text {
                plain: text.chars().all(|c| c.is_ascii_alphanumeric()),
            })
        }
    }
}

/// The text writes something: no control characters, nor the private use
/// area that keys without text are named in.
fn writes(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| !c.is_control() && !('\u{E000}'..='\u{F8FF}').contains(&c))
}

#[cfg(test)]
mod tests {
    use iced::keyboard::key::{NativeCode, Physical};
    use iced::keyboard::{Location, Modifiers};

    use super::*;

    fn press(key: Key, modifiers: Modifiers, text: Option<&str>) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Unidentified(NativeCode::Unidentified),
            location: Location::Standard,
            modifiers,
            text: text.map(Into::into),
            repeat: false,
        })
    }

    fn typed(text: &str) -> Option<Unclaimed> {
        of(&press(
            Key::Character(text.into()),
            Modifiers::empty(),
            Some(text),
        ))
    }

    #[test]
    fn text_backspace_and_delete_write() {
        assert_eq!(typed("x"), Some(Unclaimed::Text { plain: true }));
        assert_eq!(typed("7"), Some(Unclaimed::Text { plain: true }));
        assert_eq!(typed("?"), Some(Unclaimed::Text { plain: false }));
        assert_eq!(typed("ж"), Some(Unclaimed::Text { plain: false }));
        let space = press(Key::Named(Named::Space), Modifiers::empty(), Some(" "));
        assert_eq!(of(&space), Some(Unclaimed::Text { plain: false }));
        for named in [Named::Backspace, Named::Delete] {
            let key = press(Key::Named(named), Modifiers::empty(), Some("\u{7f}"));
            assert_eq!(of(&key), Some(Unclaimed::Text { plain: false }));
        }
    }

    #[test]
    fn enter_held_keys_and_keys_that_write_nothing_dont() {
        let enter = press(Key::Named(Named::Enter), Modifiers::empty(), Some("\r"));
        assert_eq!(of(&enter), None);
        let down = press(Key::Named(Named::ArrowDown), Modifiers::empty(), None);
        assert_eq!(of(&down), None);
        for held in [Modifiers::CTRL, Modifiers::ALT, Modifiers::LOGO] {
            let x = press(Key::Character("x".into()), held, Some("x"));
            assert_eq!(of(&x), None, "{held:?}");
            let tab = press(Key::Named(Named::Tab), held, Some("\t"));
            assert_eq!(of(&tab), None, "{held:?}");
        }
        assert_eq!(typed("\u{b}"), None, "a control character");
        assert_eq!(
            typed("\u{F700}"),
            None,
            "a key named in the private use area"
        );
    }

    #[test]
    fn tab_shift_tab_and_escape() {
        let tab = |modifiers| press(Key::Named(Named::Tab), modifiers, Some("\t"));
        assert_eq!(
            of(&tab(Modifiers::empty())),
            Some(Unclaimed::Tab { back: false })
        );
        assert_eq!(
            of(&tab(Modifiers::SHIFT)),
            Some(Unclaimed::Tab { back: true })
        );
        let escape = press(Key::Named(Named::Escape), Modifiers::empty(), None);
        assert_eq!(of(&escape), Some(Unclaimed::Escape));
    }
}
