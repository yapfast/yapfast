//! Built-in shortcuts for the focused window. Custom global keybinds
//! remain the desktop watcher's responsibility.

use iced::{
    Task,
    keyboard::{Key, Modifiers, key},
};
use yapfast_core::application::Navigation;

use crate::{
    app::{App, Message},
    picker::{Tab, Target},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    Server(bool),
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

/// Exact modifiers keep AltGr, text editing and larger chords separate.
pub fn pressed(key: &Key, physical: key::Physical, modifiers: Modifiers) -> Option<Shortcut> {
    use Shortcut::*;
    let ctrl = Modifiers::CTRL;
    let shift = ctrl | Modifiers::SHIFT;
    let alt = ctrl | Modifiers::ALT;
    if modifiers == ctrl {
        if key.as_ref() == Key::Character("/") {
            return Some(Help);
        }
        if key == &Key::Named(key::Named::Enter) {
            return Some(Answer);
        }
        if key.as_ref() == Key::Character("'") {
            return Some(StartCall);
        }
    }
    if modifiers == alt {
        match key {
            Key::Named(key::Named::ArrowUp) => return Some(Server(false)),
            Key::Named(key::Named::ArrowDown) => return Some(Server(true)),
            _ => {}
        }
    }
    let letter = key.to_latin(physical)?.to_ascii_lowercase();
    match (modifiers, letter) {
        (m, 'p') if m == ctrl => Some(Pins),
        (m, 'i') if m == ctrl => Some(Inbox),
        (m, 'u') if m == ctrl => Some(Members),
        (m, 'e') if m == ctrl => Some(Emoji),
        (m, 's') if m == ctrl => Some(Stickers),
        (m, 'm') if m == shift => Some(Mute),
        (m, 'd') if m == shift => Some(Deafen),
        (m, 'u') if m == shift => Some(Upload),
        (m, 'l') if m == shift => Some(CopyLink),
        (m, 'n') if m == shift => Some(JoinServer),
        (m, 'v') if m == (shift | Modifiers::ALT) => Some(CurrentCall),
        _ => None,
    }
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
        Server(forward) => {
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

/// Also lists existing shortcuts; these allocate no persistent app state.
pub const GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "General",
        &[
            ("Show keyboard shortcuts", "Ctrl + /"),
            ("Quick switcher", "Ctrl + K"),
            ("Search this channel", "Ctrl + F"),
            ("Search all channels", "Ctrl + Shift + F"),
        ],
    ),
    (
        "Navigation",
        &[
            ("Previous / next server", "Ctrl + Alt + Up / Down"),
            ("Back / forward", "Alt + Left / Right"),
            ("Go to current call", "Ctrl + Alt + Shift + V"),
            ("Create or join a server", "Ctrl + Shift + N"),
        ],
    ),
    (
        "Chat",
        &[
            ("Toggle pins", "Ctrl + P"),
            ("Toggle inbox", "Ctrl + I"),
            ("Toggle members or voice chat", "Ctrl + U"),
            ("Toggle emoji picker", "Ctrl + E"),
            ("Toggle GIF picker (in composer)", "Ctrl + G"),
            ("Toggle sticker picker", "Ctrl + S"),
            ("Upload a file", "Ctrl + Shift + U"),
            ("Copy channel link", "Ctrl + Shift + L"),
            ("Focus composer", "Type anywhere"),
            ("Close popup / cancel", "Esc"),
        ],
    ),
    (
        "Messages (Alt + Up from composer)",
        &[
            ("Move between messages", "Up / Down"),
            ("Edit own message", "E"),
            ("Pin / unpin message", "P"),
            ("Delete message", "Backspace / Delete"),
            ("Add reaction", "+"),
            ("Reply", "R"),
            ("Forward", "F"),
            ("Copy text", "Ctrl + C"),
            ("Mark unread", "Alt + Enter"),
            ("Return to composer", "Esc"),
        ],
    ),
    (
        "Voice and video",
        &[
            ("Toggle mute", "Ctrl + Shift + M"),
            ("Toggle deafen", "Ctrl + Shift + D"),
            ("Answer incoming call", "Ctrl + Enter"),
            ("Start call in DM or group", "Ctrl + '"),
        ],
    ),
];

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
}
