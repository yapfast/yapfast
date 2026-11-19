//! What the window listens to besides its widgets: keys nothing took,
//! fonts the shaper wants, the window's size, timers, the test display.

use std::time::Duration;

use iced::{
    Subscription,
    keyboard::{self, key},
};

use crate::{control_file, fonts};

use super::{App, Message};

impl App {
    /// Keys nothing in the window took, fonts the shaper wants, and the
    /// test display's commands.
    pub fn subscription(&self) -> Subscription<Message> {
        let keys = keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed {
                key,
                modifiers,
                physical_key,
                ..
            } => Some(Message::Pressed(key, modifiers, physical_key)),
            keyboard::Event::ModifiersChanged(modifiers) => Some(Message::Modifiers(modifiers)),
            _ => None,
        });
        let fonts = Subscription::run(fonts::wakes).map(|()| Message::FontsLoaded);
        let drops = iced::event::listen_with(|event, _, _| match event {
            iced::Event::Window(event) => crate::uploads::drops(&event),
            _ => None,
        });
        // What the overlays are placed by: the window's size as it is,
        // not as it was asked for.
        let sizes = iced::event::listen_with(|event, _, _| match event {
            iced::Event::Window(
                iced::window::Event::Resized(size) | iced::window::Event::Opened { size, .. },
            ) => Some(Message::Shell(crate::shell::Event::Resized(size))),
            _ => None,
        });
        // Ctrl with + or - zooms wherever the keys are.
        let zooms = iced::event::listen_with(|event, _, _| match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                ..
            }) if modifiers.control() && !modifiers.alt() => {
                Some(Message::Shell(crate::shell::Event::Zoom(key, physical_key)))
            }
            _ => None,
        });
        let mut subscriptions = vec![
            keys,
            fonts,
            drops,
            sizes,
            zooms,
            crate::window_bar::history_keys(),
            Self::shell_subscription(),
        ];
        // A look at the releases every six hours while the check is on.
        if self.layout.prefs.check_for_updates {
            subscriptions.push(
                iced::time::every(crate::updates::EVERY)
                    .map(|_| Message::Updates(crate::updates::Event::Due)),
            );
        }
        if crate::updates::updates_installing(self) {
            subscriptions.push(iced::time::every(Duration::from_millis(200)).map(|_| {
                Message::Shell(crate::shell::Event::Heard(crate::desktop::Heard::Updates))
            }));
        }
        // Ctrl+K opens the quick switcher wherever the keys are, and Ctrl+F
        // or Ctrl+Shift+F goes to the conversation's search, as Discord's
        // binds are: Ctrl+F fills in a server channel's `in:` first.
        if crate::signin::workspace(self) {
            subscriptions.push(iced::event::listen_with(|event, _, _| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key,
                    physical_key,
                    modifiers,
                    repeat: false,
                    ..
                }) if modifiers.control() && !modifiers.alt() => {
                    if is_letter(&key, physical_key, "k", key::Code::KeyK) && !modifiers.shift() {
                        Some(Message::Switcher(crate::switcher::Event::Toggle))
                    } else if is_letter(&key, physical_key, "f", key::Code::KeyF) {
                        Some(Message::Search(crate::search::Event::Focus {
                            channel: !modifiers.shift(),
                        }))
                    } else {
                        None
                    }
                }
                _ => None,
            }));
        }
        // A dialog's field keeps Escape to itself, only letting go of the
        // keys: while a dialog with one is open, Escape closes the dialog.
        if self.field_keeps_escape() {
            subscriptions.push(iced::event::listen_with(|event, status, _| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: key @ keyboard::Key::Named(key::Named::Escape),
                    modifiers,
                    ..
                }) if status == iced::event::Status::Captured => Some(Message::Key(key, modifiers)),
                _ => None,
            }));
        }
        // With the window closed to the tray, nothing shows.
        let shown = self.shell.window.is_some();
        // Settings: the push-to-talk key while it's recorded.
        if self.recording_keybind() {
            subscriptions.push(crate::settings::keybinds::record_keys());
        }
        // The connection's details, copied out each second while they or
        // the signal's tooltip show; the call's clock moves by itself.
        if shown && self.call.since.is_some() && crate::call::details::link_shown(self) {
            subscriptions.push(
                iced::time::every(Duration::from_secs(1))
                    .map(|_| Message::Call(crate::call::Event::Second)),
            );
        }
        subscriptions.extend(crate::arrange::arrange_subscription(self));
        subscriptions.extend(crate::uploads::follow_subscription(self));
        subscriptions.push(crate::away::away_subscription(self));
        subscriptions.push(crate::keybinds::window_talk_keys());
        if self.call.grab.is_some() {
            subscriptions.push(iced::event::listen_with(|event, _, _| match event {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::Call(crate::call::Event::DragTo(position)))
                }
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )) => Some(Message::Call(crate::call::Event::Drop)),
                _ => None,
            }));
        }
        if let Some(path) = control_file::path() {
            subscriptions.push(
                Subscription::run_with(path, |path| control_file::commands(path.clone()))
                    .map(Message::Control),
            );
        }
        Subscription::batch(subscriptions)
    }
}

/// The key of a shortcut's letter, `letter` in lower case: that letter in
/// the keyboard's layout, or in a layout without Latin letters (Cyrillic,
/// Greek, Thai), the key in its place, `code`.
fn is_letter(key: &keyboard::Key, physical: key::Physical, letter: &str, code: key::Code) -> bool {
    match key {
        keyboard::Key::Character(text)
            if text.chars().all(|letter| letter.is_ascii_alphabetic()) =>
        {
            text.eq_ignore_ascii_case(letter)
        }
        _ => physical == key::Physical::Code(code),
    }
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key, key};

    use super::is_letter;

    #[test]
    fn a_shortcuts_letter_is_the_key_in_its_place_where_the_layout_has_no_latin_letters() {
        let is_k = |key: &Key, physical| is_letter(key, physical, "k", key::Code::KeyK);
        let k = key::Physical::Code(key::Code::KeyK);
        let r = key::Physical::Code(key::Code::KeyR);
        assert!(is_k(&Key::Character("k".into()), k));
        assert!(is_k(&Key::Character("K".into()), k), "with Shift");
        // Dvorak's K is where QWERTY's V is.
        assert!(is_k(
            &Key::Character("k".into()),
            key::Physical::Code(key::Code::KeyV)
        ));
        assert!(!is_k(&Key::Character("t".into()), k), "Dvorak's T, on K");
        assert!(is_k(&Key::Character("л".into()), k), "a Russian layout");
        assert!(
            !is_k(&Key::Character("к".into()), r),
            "its own K isn't K's key"
        );
        let f = key::Physical::Code(key::Code::KeyF);
        assert!(is_letter(
            &Key::Character("а".into()),
            f,
            "f",
            key::Code::KeyF
        ));
        assert!(!is_letter(
            &Key::Character("k".into()),
            k,
            "f",
            key::Code::KeyF
        ));
    }
}
