//! Built-in shortcuts for the focused window. Custom global keybinds
//! remain the desktop watcher's responsibility.

use std::sync::LazyLock;

use iced::{
    Task,
    keyboard::{Key, Modifiers, key},
};
use serde::Deserialize;
use yapfast_core::application::Navigation;

use crate::{
    app::{App, Message},
    picker::{Tab, Target},
};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Shortcut {
    Help,
    Mute,
    Deafen,
    Pins,
    Inbox,
    Members,
    Emoji,
    Stickers,
    Upload,
    CopyLink,
    JoinServer,
    CurrentCall,
    Answer,
    StartCall,
    PreviousServer,
    NextServer,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keymap {
    bindings: Vec<Binding>,
    pub help: (String, String),
    pub groups: Vec<Group>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub heading: String,
    pub description: Option<String>,
    pub shortcuts: Vec<(String, String)>,
}

pub static DEFAULTS: LazyLock<Keymap> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../assets/keybinds/default.json"))
        .expect("assets/keybinds/default.json contains a valid built-in keymap")
});

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    #[serde(deserialize_with = "deserialize_modifiers")]
    modifiers: Modifiers,
    key: BoundKey,
    action: Shortcut,
}

fn deserialize_modifiers<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Modifiers, D::Error> {
    #[derive(Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum Chord {
        Ctrl,
        CtrlShift,
        CtrlAlt,
        CtrlAltShift,
    }
    let ctrl = Modifiers::CTRL;
    Ok(match Chord::deserialize(deserializer)? {
        Chord::Ctrl => ctrl,
        Chord::CtrlShift => ctrl | Modifiers::SHIFT,
        Chord::CtrlAlt => ctrl | Modifiers::ALT,
        Chord::CtrlAltShift => ctrl | Modifiers::ALT | Modifiers::SHIFT,
    })
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BoundKey {
    Character(char),
    Named(NamedKey),
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum NamedKey {
    Enter,
    ArrowUp,
    ArrowDown,
}

impl BoundKey {
    fn matches(&self, key: &Key, latin: Option<char>) -> bool {
        match *self {
            Self::Character(character) if character.is_ascii_alphabetic() => {
                latin == Some(character.to_ascii_lowercase())
            }
            Self::Character(character) => {
                matches!(key, Key::Character(text) if text.chars().eq([character]))
            }
            Self::Named(named) => {
                let expected = match named {
                    NamedKey::Enter => key::Named::Enter,
                    NamedKey::ArrowUp => key::Named::ArrowUp,
                    NamedKey::ArrowDown => key::Named::ArrowDown,
                };
                key == &Key::Named(expected)
            }
        }
    }
}

/// A configured desktop binding has priority, so its toggle cannot run
/// twice when the same chord also has a built-in action.
pub fn custom_owns(app: &App, physical: key::Physical, modifiers: Modifiers) -> bool {
    if app.keybinds.watched.is_empty() {
        return false;
    }
    let Some(code) = crate::keybinds::known_scancode(physical) else {
        return false;
    };
    app.keybinds.watched.iter().any(|watched| {
        if let Some(status) = &app.keybinds.status {
            use yapfast_core::keybinds::Reach;
            match status.reach {
                Reach::Asking => return false,
                Reach::Desktop
                    if !status
                        .given
                        .get(&watched.id)
                        .is_some_and(|keys| !keys.is_empty()) =>
                {
                    return false;
                }
                _ => {}
            }
        }
        let mut has_key = false;
        watched
            .keys
            .keys()
            .iter()
            .all(|key| match yapfast_core::keybinds::modifier(key.scancode) {
                Some((_, _, "CTRL")) => modifiers.control(),
                Some((_, _, "SHIFT")) => modifiers.shift(),
                Some((_, _, "ALT")) => modifiers.alt(),
                Some((_, _, "LOGO")) => modifiers.logo(),
                _ => {
                    has_key |= key.scancode == code;
                    key.scancode == code
                }
            })
            && has_key
    })
}

/// A press the window heard, wherever the keys are, as a built-in; but a
/// field's own Ctrl+E (the composer's or an edit's, which opens the picker
/// for it) isn't taken twice, which would open the picker and shut it.
pub fn heard(
    key: &Key,
    physical: key::Physical,
    modifiers: Modifiers,
    status: iced::event::Status,
) -> Option<Message> {
    pressed(key, physical, modifiers)
        .filter(|shortcut| *shortcut != Shortcut::Emoji || status == iced::event::Status::Ignored)
        .map(|shortcut| Message::Shortcut(shortcut, physical, modifiers))
}

/// Exact modifiers keep AltGr, text editing and larger chords separate.
pub fn pressed(key: &Key, physical: key::Physical, modifiers: Modifiers) -> Option<Shortcut> {
    if !modifiers.control() || modifiers.logo() {
        return None;
    }
    let latin = key
        .to_latin(physical)
        .map(|letter| letter.to_ascii_lowercase());
    DEFAULTS
        .bindings
        .iter()
        .find(|binding| binding.modifiers == modifiers && binding.key.matches(key, latin))
        .map(|binding| binding.action)
}

pub fn update(app: &mut App, shortcut: Shortcut) -> Task<Message> {
    use Shortcut::*;
    if !crate::signin::workspace(app) || app.recording_keybind() {
        return Task::none();
    }
    // Voice controls work while a dialog is open, like the account buttons.
    match shortcut {
        Mute => return crate::account::update(app, crate::account::Event::Mute),
        Deafen => return crate::account::update(app, crate::account::Event::Deafen),
        Help => {
            return app.settings_event(crate::settings::Event::Open(
                crate::settings::Section::Keybinds,
            ));
        }
        _ => {}
    }
    if app.keys_held() {
        return Task::none();
    }
    match shortcut {
        Pins if app.header.can_pin => {
            crate::header_popovers::popover_event(app, crate::header_popovers::Event::Pins)
        }
        Inbox => crate::window_bar::update(app, crate::window_bar::Event::Inbox),
        Members if app.header.voice_chat.is_some() => {
            let show = !app.header.voice_chat.unwrap_or(false);
            crate::sidebar::update(app, crate::sidebar::Event::VoiceChat(show))
        }
        Members if app.header.members_available => {
            crate::members::update(app, crate::members::Event::Toggle)
        }
        Emoji | Stickers if crate::composer::composer_look(app.ctx()).enabled => {
            if shortcut == Stickers && !app.client.state().can_pick_gifs() {
                return Task::none();
            }
            crate::picker::open_picker(
                app,
                Target::Composer,
                if shortcut == Emoji {
                    Tab::Emoji
                } else {
                    Tab::Stickers
                },
            )
        }
        Upload if crate::composer::composer_look(app.ctx()).attach => crate::composer::update(
            app,
            crate::composer::Event::Uploads(crate::uploads::Event::Attach),
        ),
        CopyLink => {
            let Some(room) = app.client.state().selected.clone() else {
                return Task::none();
            };
            crate::place_menus::choose_place(
                app,
                room,
                yapfast_core::application::MenuChoice::CopyLink,
            )
        }
        JoinServer => crate::rail::update(app, crate::rail::Event::Join),
        CurrentCall => crate::call::update(app, crate::call::Event::OpenCall),
        Answer => {
            let room = crate::call::ringing(app.client.state())
                .next()
                .map(|room| room.id.clone());
            match room {
                Some(room) => crate::call::update(app, crate::call::Event::Answer(room, true)),
                None => Task::none(),
            }
        }
        StartCall if app.header.can_call && app.header.can_join => {
            crate::call::update(app, crate::call::Event::Start(false))
        }
        PreviousServer | NextServer => {
            let forward = shortcut == NextServer;
            // Borrow the existing server order; allocate only the chosen id.
            let state = app.client.state();
            let current = match &state.navigation {
                Navigation::Server(id) => state.servers.iter().position(|server| server.id == *id),
                _ => None,
            };
            let count = state.servers.len();
            let next = match (current, forward) {
                (Some(index), true) if index + 1 < count => Some(index + 1),
                (Some(index), false) => index.checked_sub(1),
                (None, true) if count > 0 => Some(0),
                (None, false) if count > 0 => Some(count - 1),
                _ => None,
            };
            match next {
                Some(index) => {
                    let id = state.servers[index].id.clone();
                    crate::rail::update(app, crate::rail::Event::Server(id))
                }
                None => crate::rail::update(app, crate::rail::Event::Home),
            }
        }
        _ => Task::none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_require_exact_modifiers_and_follow_non_latin_layouts() {
        let m = Key::Character("m".into());
        let physical = key::Physical::Code(key::Code::KeyM);
        let chord = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(pressed(&m, physical, chord), Some(Shortcut::Mute));
        assert_eq!(pressed(&m, physical, Modifiers::CTRL), None);
        assert_eq!(pressed(&m, physical, chord | Modifiers::ALT), None);
        assert_eq!(
            pressed(&Key::Character("ь".into()), physical, chord),
            Some(Shortcut::Mute)
        );
        assert_eq!(pressed(&Key::Character("t".into()), physical, chord), None);
    }

    #[test]
    fn ctrl_slash_takes_shift_where_the_layout_needs_it() {
        let slash = Key::Character("/".into());
        let physical = key::Physical::Code(key::Code::Digit7);
        assert_eq!(
            pressed(&slash, physical, Modifiers::CTRL),
            Some(Shortcut::Help)
        );
        assert_eq!(
            pressed(&slash, physical, Modifiers::CTRL | Modifiers::SHIFT),
            Some(Shortcut::Help)
        );
    }

    #[test]
    fn a_fields_own_ctrl_e_is_not_taken_twice() {
        use iced::event::Status;
        let e = Key::Character("e".into());
        let physical = key::Physical::Code(key::Code::KeyE);
        let shortcut = |key: &Key, status| match heard(key, physical, Modifiers::CTRL, status) {
            Some(Message::Shortcut(shortcut, ..)) => Some(shortcut),
            _ => None,
        };
        assert_eq!(shortcut(&e, Status::Captured), None);
        assert_eq!(shortcut(&e, Status::Ignored), Some(Shortcut::Emoji));
        let p = Key::Character("p".into());
        assert_eq!(shortcut(&p, Status::Captured), Some(Shortcut::Pins));
    }
}
