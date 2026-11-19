//! Keybinds, as Discord's Keybinds page: "Add a Keybind", and each
//! keybind's action, its keys with Record keybind, a remove button and what
//! the action does; and what records a keybind's keys, push to talk's on
//! Voice's page among them. What a keybind is, how it's kept and how its
//! keys are recorded is the core's (`yapfast_core::keybinds`); the root
//! watches them (`crate::keybinds`).

use iced::{
    Alignment, Element, Length,
    keyboard::{self, Key as Pressed, Location, key},
    widget::{column, container, row},
};
use yapfast_core::keybinds::{Action, Binding, Key, Recorder, Shortcut};

use super::{Drop, Event as SettingsEvent, Page, Settings};
use crate::{
    app::Message,
    controls::{Kind, action},
    ctx::{Cx, Pref as Shown, Request},
    form,
    pictures::Icon,
    theme::Theme,
};
use yapfast_core::i18n::tr;

/// The actions' column, as wide as the longest, so rows line up; and a
/// row's keys' box.
const ACTION_WIDTH: f32 = 220.0;
const KEYS_WIDTH: f32 = 180.0;

/// Which keybind records: push to talk's (Voice's), or a row's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Talk,
    Row(usize),
}

/// A keybind being recorded, and the keys pressed so far.
pub struct Recording {
    target: Target,
    recorder: Recorder,
}

/// What reaches a recording.
#[derive(Clone, Debug)]
pub enum Heard {
    /// A key went down or up; `escape` if it's Escape.
    Key {
        key: Key,
        pressed: bool,
        escape: bool,
    },
    /// The window lost the focus, so the keys' release won't come.
    Unfocused,
}

#[derive(Clone, Debug)]
pub enum Event {
    Add,
    Remove(usize),
    /// A row's action chosen, by its place in [`Action::OFFERED`].
    Action(usize, usize),
}

fn keybind_message(event: Event) -> Message {
    Message::Settings(SettingsEvent::Keybind(event))
}

impl Settings {
    /// While `target` records, the keys pressed so far.
    pub(super) fn recorded(&self, target: Target) -> Option<String> {
        self.record
            .as_ref()
            .filter(|recording| recording.target == target)
            .map(|recording| recording.recorder.shown())
    }

    /// Whether a keybind is being recorded, while keybinds rest.
    pub fn recording(&self) -> bool {
        self.record.is_some()
    }

    pub(super) fn record(&mut self, target: Target, on: bool) {
        self.record = on.then(|| Recording {
            target,
            recorder: Recorder::default(),
        });
    }

    /// A key while recording: each key pressed joins the keybind, which is
    /// recorded once they're all up again; Escape, pressed first, stops
    /// without changing it; the focus going records what was pressed.
    pub(super) fn heard(&mut self, heard: Heard, cx: &mut Cx<'_>) {
        let Some(recording) = &mut self.record else {
            return;
        };
        let done = match heard {
            Heard::Key {
                escape: true,
                pressed: true,
                ..
            } if recording.recorder.is_empty() => None,
            Heard::Key { key, pressed, .. } => match recording.recorder.key(key, pressed) {
                Some(done) => Some(done),
                None => return,
            },
            Heard::Unfocused => recording.recorder.finish(),
        };
        let target = recording.target;
        self.record = None;
        let Some(keys) = done else {
            return;
        };
        match target {
            Target::Talk => {
                tracing::info!(keys = keys.name(), "push-to-talk keybind recorded");
                cx.keep("push_to_talk_key", &keys);
                cx.layout.voice.push_to_talk_key = Some(keys);
            }
            Target::Row(row) => {
                let Some(binding) = cx.layout.keybinds.get_mut(row) else {
                    return;
                };
                tracing::info!(keys = keys.name(), action = ?binding.action, "keybind recorded");
                binding.keys = Some(keys);
                cx.keep("keybinds", &cx.layout.keybinds);
            }
        }
        cx.request(Request::Changed(Shown::Keybinds));
    }

    pub(super) fn keybind_event(&mut self, event: Event, cx: &mut Cx<'_>) {
        let keybinds = &mut cx.layout.keybinds;
        match event {
            Event::Add => {
                let id = keybinds.iter().map(|binding| binding.id).max().unwrap_or(0) + 1;
                keybinds.push(Binding {
                    id,
                    action: Action::Unassigned,
                    keys: None,
                });
            }
            Event::Remove(row) => {
                if row >= keybinds.len() {
                    return;
                }
                keybinds.remove(row);
                self.record = None;
                self.open = None;
            }
            Event::Action(row, index) => {
                self.open = None;
                let (Some(binding), Some(&action)) =
                    (keybinds.get_mut(row), Action::OFFERED.get(index))
                else {
                    return;
                };
                if binding.action == action {
                    return;
                }
                binding.action = action;
            }
        }
        cx.keep("keybinds", &cx.layout.keybinds);
        cx.request(Request::Changed(Shown::Keybinds));
    }
}

impl<'a> Page<'a> {
    pub(super) fn keybinds_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let note = self.cx.keybinds.page_note().unwrap_or_else(|| {
            tr("Keybinds mute, deafen or talk from any app. Add one, choose what it does, and record its keys.").to_owned()
        });
        let mut parts = vec![
            form::beside(
                form::title(theme, tr("Keybinds")),
                action(
                    theme,
                    tr("Add a Keybind"),
                    Kind::Plain,
                    Some(keybind_message(Event::Add)),
                ),
            ),
            form::note(theme, note),
        ];
        for (row, binding) in self.cx.layout.keybinds.iter().enumerate() {
            parts.push(self.keybind_row(row, binding));
        }
        form::page(parts)
    }

    /// A keybind's row, on the sunken ground: its action, its keys and
    /// their recorder, the remove button, what the action does and what
    /// the desktop did with it.
    fn keybind_row(self, row: usize, binding: &'a Binding) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let settings = self.s;
        let names = Action::OFFERED
            .iter()
            .map(|action| action.label().to_owned())
            .collect();
        let chosen = Action::OFFERED
            .iter()
            .position(|&action| action == binding.action);
        let actions = column![
            form::heading(theme, tr("ACTION")),
            form::dropdown(
                names,
                chosen,
                settings.open == Some(Drop::Keybind(row)),
                Message::Settings(SettingsEvent::Drop(Drop::Keybind(row))),
                move |index| keybind_message(Event::Action(row, index)),
            )
            .view(self.cx.look),
        ]
        .spacing(8)
        .width(ACTION_WIDTH);
        let target = Target::Row(row);
        let recorded = settings.recorded(target);
        let recording = recorded.is_some();
        let keys = binding
            .keys
            .as_ref()
            .map(Shortcut::name)
            .unwrap_or_default();
        let empty = keys.is_empty();
        let keys = column![
            form::heading(theme, tr("KEYBIND")),
            form::buttons([
                form::capture(theme, keys, recorded, KEYS_WIDTH),
                record_button(theme, target, recording, empty),
            ]),
        ]
        .spacing(8)
        .width(Length::Fill);
        let remove = self
            .cx
            .look
            .icon_button(
                Icon::X,
                tr("Remove Keybind"),
                keybind_message(Event::Remove(row)),
            )
            .size(32.0, 16);
        let mut body = column![
            row![actions, keys, remove]
                .spacing(12)
                .align_y(Alignment::Start),
            form::note(theme, binding.action.description()),
        ]
        .spacing(12);
        if let Some(given) = self.cx.keybinds.given_note(&binding.shortcut_id()) {
            body = body.push(form::note(theme, given));
        }
        let ground = theme.sunken;
        container(body)
            .padding(16)
            .width(Length::Fill)
            .style(move |_| crate::style::rounded(ground, 8))
            .into()
    }
}

/// A keybind's recorder's button: Record keybind, Edit keybind, or Stop
/// recording while it records.
pub(super) fn record_button<'a>(
    theme: &Theme,
    target: Target,
    recording: bool,
    empty: bool,
) -> Element<'a, Message> {
    action(
        theme,
        if recording {
            tr("Stop recording")
        } else if empty {
            tr("Record keybind")
        } else {
            tr("Edit keybind")
        },
        if recording { Kind::Danger } else { Kind::Plain },
        Some(Message::Settings(SettingsEvent::Record(target, !recording))),
    )
    .into()
}

/// A key as a keybind keeps it, from a key pressed or let go: its scan
/// code (evdev's on Linux, set 1's on Windows, as winit gives it), its
/// name as Settings shows it, and on Linux its keysym, which the desktop
/// portal takes; none for a key with no scan code.
pub fn recorded(key: &Pressed, physical: key::Physical, location: Location) -> Option<Key> {
    Some(Key {
        scancode: crate::keybinds::scancode(physical)?,
        name: key_name(key, physical, location),
        trigger: trigger(key, location),
    })
}

/// What a keyboard event says to a recording, if anything.
fn heard(
    key: &Pressed,
    physical: key::Physical,
    location: Location,
    pressed: bool,
) -> Option<Message> {
    let escape = *key == Pressed::Named(key::Named::Escape);
    let key = recorded(key, physical, location)?;
    Some(Message::Settings(SettingsEvent::Heard(Heard::Key {
        key,
        pressed,
        escape,
    })))
}

/// While a keybind is recorded, every key goes to it, whatever has the
/// keys, and on Windows Mouse 4 and 5; and the focus going.
pub fn record_keys() -> iced::Subscription<Message> {
    iced::event::listen_with(|event, _, _| match event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            location,
            repeat: false,
            ..
        }) => heard(&key, physical_key, location, true),
        iced::Event::Keyboard(keyboard::Event::KeyReleased {
            key,
            physical_key,
            location,
            ..
        }) => heard(&key, physical_key, location, false),
        #[cfg(windows)]
        iced::Event::Mouse(
            iced::mouse::Event::ButtonPressed(button) | iced::mouse::Event::ButtonReleased(button),
        ) => mouse(
            button,
            matches!(
                event,
                iced::Event::Mouse(iced::mouse::Event::ButtonPressed(_))
            ),
        ),
        iced::Event::Window(iced::window::Event::Unfocused) => {
            Some(Message::Settings(SettingsEvent::Heard(Heard::Unfocused)))
        }
        _ => None,
    })
}

/// Mouse 4 (Back) or 5 (Forward) on Windows, which keybinds may hold too.
#[cfg(windows)]
fn mouse(button: iced::mouse::Button, pressed: bool) -> Option<Message> {
    let (scancode, name) = match button {
        iced::mouse::Button::Back => (yapfast_core::keybinds::MOUSE_4, "Mouse 4"),
        iced::mouse::Button::Forward => (yapfast_core::keybinds::MOUSE_5, "Mouse 5"),
        _ => return None,
    };
    let key = Key {
        scancode,
        name: name.to_owned(),
        trigger: None,
    };
    Some(Message::Settings(SettingsEvent::Heard(Heard::Key {
        key,
        pressed,
        escape: false,
    })))
}

/// A key's name as Settings shows it, as the root crate's push to talk
/// names it: "V", "Right Ctrl", "Numpad 1".
fn key_name(key: &Pressed, physical: key::Physical, location: Location) -> String {
    let side = match location {
        Location::Left => "Left ",
        Location::Right => "Right ",
        Location::Numpad => "Numpad ",
        Location::Standard => "",
    };
    match key {
        Pressed::Character(text) => format!("{side}{}", text.to_uppercase()),
        Pressed::Named(named) => {
            let named = match named {
                key::Named::Control => "Ctrl".to_owned(),
                key::Named::Super | key::Named::Meta => "Super".to_owned(),
                key::Named::ArrowUp => "Up".to_owned(),
                key::Named::ArrowDown => "Down".to_owned(),
                key::Named::ArrowLeft => "Left".to_owned(),
                key::Named::ArrowRight => "Right".to_owned(),
                key::Named::Space => "Space".to_owned(),
                named => format!("{named:?}"),
            };
            format!("{side}{named}")
        }
        Pressed::Unidentified => match physical {
            key::Physical::Code(code) => format!("{code:?}"),
            key::Physical::Unidentified(_) => tr("Unknown key").to_owned(),
        },
    }
}

/// The key's keysym name, as the XDG shortcuts specification writes a
/// trigger; on Linux, where only the portal needs it.
fn trigger(key: &Pressed, location: Location) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let right = location == Location::Right;
    let side =
        |left: &str, right_name: &str| Some(if right { right_name } else { left }.to_owned());
    match key {
        Pressed::Character(text) => {
            let mut chars = text.chars();
            let (Some(char), None) = (chars.next(), chars.next()) else {
                return None;
            };
            let name = xkeysym::Keysym::from_char(char).name()?;
            Some(name.strip_prefix("XK_").unwrap_or(name).to_owned())
        }
        Pressed::Named(named) => match named {
            key::Named::Control => side("Control_L", "Control_R"),
            key::Named::Shift => side("Shift_L", "Shift_R"),
            key::Named::Alt => side("Alt_L", "Alt_R"),
            key::Named::Super | key::Named::Meta => side("Super_L", "Super_R"),
            key::Named::AltGraph => Some("ISO_Level3_Shift".to_owned()),
            key::Named::CapsLock => Some("Caps_Lock".to_owned()),
            key::Named::ScrollLock => Some("Scroll_Lock".to_owned()),
            key::Named::NumLock => Some("Num_Lock".to_owned()),
            key::Named::PageUp => Some("Prior".to_owned()),
            key::Named::PageDown => Some(tr("Next").to_owned()),
            key::Named::Space => Some("space".to_owned()),
            key::Named::Enter => Some("Return".to_owned()),
            key::Named::Backspace => Some("BackSpace".to_owned()),
            key::Named::ArrowUp => Some("Up".to_owned()),
            key::Named::ArrowDown => Some("Down".to_owned()),
            key::Named::ArrowLeft => Some("Left".to_owned()),
            key::Named::ArrowRight => Some("Right".to_owned()),
            key::Named::Tab
            | key::Named::Insert
            | key::Named::Delete
            | key::Named::Home
            | key::Named::End
            | key::Named::Pause => Some(format!("{named:?}")),
            key::Named::ContextMenu => Some("Menu".to_owned()),
            named => {
                let name = format!("{named:?}");
                (name.starts_with('F') && name[1..].parse::<u8>().is_ok()).then_some(name)
            }
        },
        Pressed::Unidentified => None,
    }
}
