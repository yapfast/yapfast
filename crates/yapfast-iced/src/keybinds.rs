//! Keybinds: push to talk's key (Voice's) and the Keybinds page's, watched
//! by the root crate's watcher where the desktop lets it watch keys in
//! every app, else in this window only (`window_talk_keys`). They run with
//! Settings closed; Settings records them and shows where they work. What
//! a keybind is, and how it's kept, is the core's
//! (`yapfast_core::keybinds`).

use std::sync::Mutex;

use iced::{
    Task,
    keyboard::{self, key},
};
use yapfast_core::keybinds::{Action, Status, TALK_SHORTCUT, Watched};

use crate::{
    app::{App, Message},
    ctx::{Pref, Request},
    layout::{InputMode, Layout},
};

/// The keybinds' watcher, what it watches and what it said of where they
/// work, and how many watches there have been, so a word from an old one
/// is let go.
#[derive(Default)]
pub struct Keybinds {
    pub watcher: Option<Box<dyn std::any::Any>>,
    pub watched: Vec<Watched>,
    pub status: Option<Status>,
    pub count: u64,
    /// They rest while one is recorded.
    pub paused: bool,
}

impl Keybinds {
    /// What Voice says under push to talk's key, while it's watched.
    pub fn talk_note(&self) -> String {
        self.status
            .as_ref()
            .filter(|_| {
                self.watched
                    .iter()
                    .any(|watched| watched.id == TALK_SHORTCUT)
            })
            .map(Status::talk_note)
            .unwrap_or_default()
    }

    /// What the Keybinds page says above the keybinds.
    pub fn page_note(&self) -> Option<String> {
        self.status.as_ref().map(Status::page_note)
    }

    /// What the desktop did with keybind `id`, while it's watched and the
    /// desktop runs them.
    pub fn given_note(&self, id: &str) -> Option<String> {
        self.status
            .as_ref()
            .filter(|_| self.watched.iter().any(|watched| watched.id == id))
            .and_then(|status| status.given_note(id))
    }
}

/// The keybinds to watch: push to talk's key and push to talk keybinds only
/// in push to talk, and none unassigned or without keys.
pub fn watched(layout: &Layout) -> Vec<Watched> {
    let talking = layout.voice.input_mode == InputMode::PushToTalk;
    let talk = layout
        .voice
        .push_to_talk_key
        .iter()
        .filter(|_| talking)
        .map(|keys| Watched {
            id: TALK_SHORTCUT.to_owned(),
            action: Action::PushToTalk,
            keys: keys.clone(),
        });
    let others = layout.keybinds.iter().filter_map(|binding| {
        let keys = binding.keys.clone()?;
        match binding.action {
            Action::Unassigned => None,
            Action::PushToTalk if !talking => None,
            action => Some(Watched {
                id: binding.shortcut_id(),
                action,
                keys,
            }),
        }
    });
    talk.chain(others).collect()
}

/// What the watcher tells, from its own threads.
#[derive(Clone, Debug)]
pub enum Told {
    /// Where the keybinds work, and what the desktop gave each.
    Status(Status),
    /// A toggling keybind was pressed.
    Toggled(Action),
}

/// Whether a key, by where it is on the keyboard, ends a push to talk
/// keybind watched, so it doesn't also type.
pub fn is_talk_key(app: &App, physical: key::Physical) -> bool {
    let Some(code) = known_scancode(physical) else {
        return false;
    };
    app.keybinds.watched.iter().any(|watched| {
        watched.action == Action::PushToTalk
            && watched
                .keys
                .keys()
                .last()
                .is_some_and(|key| key.scancode == code)
    })
}

/// Watches the keybinds as they are now, if they changed: the old
/// watcher goes first, ending its portal session. What it tells comes
/// as a message (`Message::Keybinds`).
pub fn watch_keybinds(app: &mut App) -> Task<Message> {
    let wanted = watched(&app.layout);
    if wanted == app.keybinds.watched {
        return Task::none();
    }
    app.keybinds.watcher = None;
    app.keybinds.status = None;
    app.keybinds.count += 1;
    app.keybinds.watched.clone_from(&wanted);
    let (sender, told) = tokio::sync::mpsc::unbounded_channel();
    let toggles = sender.clone();
    let toggled: crate::Toggled = std::sync::Arc::new(move |action| {
        toggles.send(Told::Toggled(action)).ok();
    });
    let tell = Box::new(move |status| {
        sender.send(Told::Status(status)).ok();
    });
    app.keybinds.watcher =
        crate::desktop::desktop().watch_keybinds(wanted, app.client.runtime(), toggled, tell);
    if app.keybinds.watcher.is_none() {
        return Task::none();
    }
    // Ends as the watcher, which holds the senders, goes.
    let count = app.keybinds.count;
    Task::run(
        iced::futures::stream::unfold(told, |mut receiver| async move {
            let told = receiver.recv().await?;
            Some((told, receiver))
        }),
        move |told| Message::Keybinds(count, told),
    )
}

/// The watcher told something, by the watch's count: an old watch's
/// word is let go.
pub fn keybinds_told(app: &mut App, count: u64, told: Told) -> Task<Message> {
    if count != app.keybinds.count {
        return Task::none();
    }
    match told {
        Told::Status(status) => {
            app.keybinds.status = Some(status);
            Task::none()
        }
        Told::Toggled(action) => toggled(app, action),
    }
}

/// A toggling keybind went down.
fn toggled(app: &mut App, action: Action) -> Task<Message> {
    tracing::debug!(?action, "keybind toggled");
    match action {
        // As the account card's buttons: in a call they toggle; out of
        // one they say how the next starts.
        Action::ToggleMute => crate::account::update(app, crate::account::Event::Mute),
        Action::ToggleDeafen => crate::account::update(app, crate::account::Event::Deafen),
        Action::ToggleVoiceMode => {
            let voice = &mut app.layout.voice;
            voice.input_mode = match voice.input_mode {
                InputMode::PushToTalk => InputMode::VoiceActivity,
                InputMode::VoiceActivity => InputMode::PushToTalk,
            };
            yapfast_core::push_to_talk::set_on(voice.input_mode == InputMode::PushToTalk);
            crate::layout::store(app.client.root(), "input_mode", voice.input_mode);
            app.requests.push(Request::Changed(Pref::Keybinds));
            Task::none()
        }
        Action::Unassigned | Action::PushToTalk | Action::PushToMute => Task::none(),
    }
}

/// The scan code winit gives the key: iced's codes are winit's by name.
pub(crate) fn scancode(physical: key::Physical) -> Option<u32> {
    use winit::platform::scancode::PhysicalKeyExtScancode;
    let key::Physical::Code(code) = physical else {
        return None;
    };
    let wanted = format!("{code:?}");
    let codes = if cfg!(windows) {
        (0..0x100).chain(0xE000..0xE100).collect::<Vec<u32>>()
    } else {
        (0..0x300).collect()
    };
    codes.into_iter().find(|&scancode| {
        matches!(
            winit::keyboard::PhysicalKey::from_scancode(scancode),
            winit::keyboard::PhysicalKey::Code(found) if format!("{found:?}") == wanted
        )
    })
}

/// The window's own keys for keybinds: each key's scan code and each side
/// button going down or up, and the focus going, to the root crate, which
/// runs keybinds by them only while nothing else hears the key. No message:
/// the filter calls the root crate and returns nothing.
pub fn window_talk_keys() -> iced::Subscription<Message> {
    iced::event::listen_with(|event, _, _| {
        let desktop = crate::desktop::desktop();
        let talk = |scancode, pressed| desktop.window_key(scancode, pressed);
        match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                physical_key,
                repeat: false,
                ..
            }) => {
                if let Some(code) = known_scancode(physical_key) {
                    talk(Some(code), true);
                }
            }
            iced::Event::Keyboard(keyboard::Event::KeyReleased { physical_key, .. }) => {
                if let Some(code) = known_scancode(physical_key) {
                    talk(Some(code), false);
                }
            }
            // Mouse 4 and 5, which keybinds hold on Windows.
            #[cfg(windows)]
            iced::Event::Mouse(
                event @ (iced::mouse::Event::ButtonPressed(button)
                | iced::mouse::Event::ButtonReleased(button)),
            ) => {
                let code = match button {
                    iced::mouse::Button::Back => yapfast_core::keybinds::MOUSE_4,
                    iced::mouse::Button::Forward => yapfast_core::keybinds::MOUSE_5,
                    _ => return None,
                };
                let pressed = matches!(event, iced::mouse::Event::ButtonPressed(_));
                talk(Some(code), pressed);
            }
            iced::Event::Window(iced::window::Event::Unfocused) => talk(None, false),
            _ => {}
        }
        None
    })
}

/// [`scancode`], the last key's kept, as it searches winit's table.
fn known_scancode(physical: key::Physical) -> Option<u32> {
    static LAST: Mutex<Option<(key::Physical, Option<u32>)>> = Mutex::new(None);
    let mut last = LAST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((known, code)) = *last
        && known == physical
    {
        return code;
    }
    let code = scancode(physical);
    *last = Some((physical, code));
    code
}
