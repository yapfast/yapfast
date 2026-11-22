//! Keybinds: compact custom rows with an action, inline recorder, delete
//! and enable controls, and the built-in shortcuts in grouped cards.
//! What records a keybind's keys, push to talk's on
//! Voice's page among them. What a keybind is, how it's kept and how its
//! keys are recorded is the core's (`yapfast_core::keybinds`); the root
//! watches them (`crate::keybinds`).

use iced::{
    Alignment, Border, Element, Length, Padding,
    keyboard::{self, Key as Pressed, Location, key},
    widget::{Space, button, column, container, row, text},
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
    Configure,
    Enabled(usize, bool),
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
            Event::Enabled(row, enabled) => {
                let Some(binding) = keybinds.get_mut(row) else {
                    return;
                };
                binding.enabled = enabled;
                self.record = None;
            }
            Event::Configure => {
                self.configure_keybinds = !self.configure_keybinds;
                self.record = None;
                self.open = None;
                return;
            }
            Event::Add => {
                let id = keybinds.iter().map(|binding| binding.id).max().unwrap_or(0) + 1;
                keybinds.push(Binding {
                    id,
                    action: Action::Unassigned,
                    enabled: true,
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
        let mut custom = column![
            section_title(theme, "Custom Keybinds"),
            text("Custom keyboard shortcuts let you mute, deafen or talk from any app.")
                .size(13)
                .color(theme.secondary),
            Space::new().height(8),
            action(
                theme,
                if self.s.configure_keybinds {
                    "Close Global Keybinds"
                } else {
                    "Configure Global Keybinds"
                },
                Kind::Primary,
                Some(keybind_message(Event::Configure))
            )
            .wide(38.0),
        ]
        .spacing(8);
        if self.s.configure_keybinds {
            let note = self.cx.keybinds.page_note().unwrap_or_else(|| {
                "Add a keybind, choose what it does, and record its keys.".to_owned()
            });
            custom = custom.push(form::note(theme, note));
            for (row, binding) in self.cx.layout.keybinds.iter().enumerate() {
                custom = custom.push(self.keybind_row(row, binding));
            }
            custom = custom.push(
                action(
                    theme,
                    "Add a Keybind",
                    Kind::Primary,
                    Some(keybind_message(Event::Add)),
                )
                .wide(38.0),
            );
        }
        let defaults = column![
            section_title(theme, "Default Keybinds"),
            shortcut_card(theme, &[("Show Keyboard Shortcuts List", "Ctrl + /")]),
        ]
        .spacing(24);
        let mut parts = column![custom, defaults].spacing(40);
        // The help row stands alone, followed by the message actions as
        // in Discord's list. Keep the labels shared with the actual list.
        for index in [3, 1, 2, 4, 0] {
            let (heading, shortcuts) = crate::shortcuts::GROUPS[index];
            let mut group = column![
                text(if index == 3 { "Messages" } else { heading })
                    .size(18)
                    .font(crate::fonts::SEMIBOLD)
                    .color(theme.text)
            ]
            .spacing(8);
            if index == 3 {
                group = group.push(text("These shortcuts work when focused on a message. Alt + Up from the composer focuses messages.")
                    .size(13).color(theme.secondary));
            }
            let shortcuts = if index == 0 {
                &shortcuts[1..]
            } else {
                shortcuts
            };
            group = group
                .push(Space::new().height(8))
                .push(shortcut_card(theme, shortcuts));
            parts = parts.push(group);
        }
        parts.width(Length::Fill).into()
    }

    /// Compact action and recorder fields, delete and enable controls.
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
        let actions = form::dropdown(
            names,
            chosen,
            settings.open == Some(Drop::Keybind(row)),
            Message::Settings(SettingsEvent::Drop(Drop::Keybind(row))),
            move |index| keybind_message(Event::Action(row, index)),
        )
        .height(38.0)
        .width(Length::FillPortion(1))
        .view(self.cx.look);
        let target = Target::Row(row);
        let recorded = settings.recorded(target);
        let recording = recorded.is_some();
        let keys = binding
            .keys
            .as_ref()
            .map(Shortcut::name)
            .unwrap_or_default();
        let shown = match recorded {
            Some(keys) if keys.is_empty() => "Press a key...".to_owned(),
            Some(keys) => keys,
            None if keys.is_empty() => "No Keybind Set".to_owned(),
            None => keys,
        };
        let (ground, edge, ink) = (
            theme.field,
            if recording {
                theme.accent
            } else {
                crate::theme::mix(theme.field, theme.muted, 0.35)
            },
            if binding.keys.is_none() && !recording {
                theme.muted
            } else {
                theme.text
            },
        );
        let (rest, hover, button_ink) = if recording {
            (theme.danger, theme.danger_hover, theme.on_danger)
        } else {
            (theme.button_default, theme.button_default_hover, theme.text)
        };
        let record = button(
            container(
                text(if recording { "Stop" } else { "Record" })
                    .size(13)
                    .font(crate::fonts::MEDIUM),
            )
            .height(Length::Fill)
            .align_y(Alignment::Center),
        )
        .height(28)
        .padding([0, 12])
        .on_press(Message::Settings(SettingsEvent::Record(target, !recording)))
        .style(move |_, status| button::Style {
            background: Some(
                if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    hover
                } else {
                    rest
                }
                .into(),
            ),
            text_color: button_ink,
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        let keys = container(
            row![
                container(
                    text(shown)
                        .size(14)
                        .color(ink)
                        .wrapping(text::Wrapping::None)
                )
                .width(Length::Fill)
                .clip(true),
                record,
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 4.0,
            right: 4.0,
            bottom: 4.0,
            left: 12.0,
        })
        .height(38)
        .align_y(Alignment::Center)
        .width(Length::FillPortion(1))
        .style(move |_| container::Style {
            background: Some(ground.into()),
            border: Border {
                color: edge,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });
        let remove = self
            .cx
            .look
            .icon_button(
                Icon::Trash,
                "Remove Keybind",
                keybind_message(Event::Remove(row)),
            )
            .size(32.0, 16);
        let toggle = form::toggle(
            theme,
            binding.enabled,
            Some(keybind_message(Event::Enabled(row, !binding.enabled))),
        );
        let mut body = column![
            row![actions, keys, remove, toggle]
                .spacing(8)
                .align_y(Alignment::Center)
        ]
        .spacing(4);
        if let Some(given) = self.cx.keybinds.given_note(&binding.shortcut_id()) {
            body = body.push(form::note(theme, given));
        }
        body.into()
    }
}

fn section_title<'a>(theme: &Theme, label: &'a str) -> Element<'a, Message> {
    text(label)
        .size(24)
        .font(crate::fonts::SEMIBOLD)
        .color(theme.text)
        .into()
}

fn shortcut_card<'a>(theme: &Theme, shortcuts: &[(&'a str, &'a str)]) -> Element<'a, Message> {
    let mut rows = column![];
    for (index, &(label, keys)) in shortcuts.iter().enumerate() {
        if index > 0 {
            rows = rows.push(crate::controls::bar(theme.line, Length::Fill, 1));
        }
        let mut chords = column![keycaps(theme, keys.split(" / ").next().unwrap_or(keys))]
            .spacing(5)
            .align_x(Alignment::End);
        if let Some((first, second)) = keys.split_once(" / ") {
            // "Ctrl + Alt + Up / Down" shows the modifiers on both lines.
            let alternative = first.rsplit_once(" + ").map_or_else(
                || second.to_owned(),
                |(prefix, _)| format!("{prefix} + {second}"),
            );
            chords = chords.push(keycaps(theme, &alternative));
        }
        rows = rows.push(
            container(
                row![
                    text(label)
                        .size(15)
                        .color(theme.secondary)
                        .width(Length::Fill),
                    chords,
                ]
                .spacing(16)
                .align_y(Alignment::Center),
            )
            .padding([16, 16])
            .width(Length::Fill),
        );
    }
    let (ground, line) = (theme.raised, theme.line);
    container(rows)
        .width(Length::Fill)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(ground.into()),
            border: Border {
                color: line,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn keycaps<'a>(theme: &Theme, chord: &str) -> Element<'a, Message> {
    let (ground, line, ink) = (theme.panel, theme.line, theme.secondary);
    row(chord.split(" + ").map(|key| {
        let label = match key {
            "Up" => "↑".to_owned(),
            "Down" => "↓".to_owned(),
            "Left" => "←".to_owned(),
            "Right" => "→".to_owned(),
            _ => key.to_uppercase(),
        };
        container(text(label).size(11).font(crate::fonts::SEMIBOLD).color(ink))
            .padding([3, 6])
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border: Border {
                    color: line,
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..Default::default()
            })
            .into()
    }))
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
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
            "Stop recording"
        } else if empty {
            "Record keybind"
        } else {
            "Edit keybind"
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
            key::Physical::Unidentified(_) => "Unknown key".to_owned(),
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
            key::Named::PageDown => Some("Next".to_owned()),
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
