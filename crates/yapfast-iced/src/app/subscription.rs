//! What the window listens to besides its widgets: keys nothing took,
//! fonts the shaper wants, the window's size, timers, the test display.

use std::time::Duration;

use iced::{
    Subscription,
    keyboard::{self, key},
};

use crate::{control, fonts};

use super::{App, Message};

impl App {
    /// Keys nothing in the window took, fonts the shaper wants, and the
    /// test display's commands.
    pub fn subscription(&self) -> Subscription<Message> {
        let keys = keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed { key, modifiers, .. } => {
                Some(Message::Key(key, modifiers))
            }
            keyboard::Event::ModifiersChanged(modifiers) => Some(Message::Modifiers(modifiers)),
            _ => None,
        });
        let fonts = Subscription::run(fonts::wakes).map(|()| Message::FontsWanted);
        let drops = iced::event::listen_with(|event, _, _| match event {
            iced::Event::Window(event) => crate::uploads::drops(&event),
            _ => None,
        });
        // What the overlays are placed by: the window's size as it is,
        // not as it was asked for.
        let sizes = iced::event::listen_with(|event, _, _| match event {
            iced::Event::Window(
                iced::window::Event::Resized(size) | iced::window::Event::Opened { size, .. },
            ) => Some(Message::Resized(size)),
            _ => None,
        });
        // Ctrl with + or - zooms wherever the keys are.
        let zooms = iced::event::listen_with(|event, _, _| match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                ..
            }) if modifiers.control() && !modifiers.alt() => Some(Message::Zoom(key, physical_key)),
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
            subscriptions.push(iced::time::every(crate::settings::updates::EVERY).map(|_| {
                Message::Settings(crate::settings::Event::Updates(
                    crate::settings::updates::Event::Due,
                ))
            }));
        }
        if self.updates_installing() {
            subscriptions.push(iced::time::every(Duration::from_millis(200)).map(|_| {
                Message::Shell(crate::shell::Event::Heard(crate::desktop::Heard::Updates))
            }));
        }
        // Ctrl+K opens the quick switcher wherever the keys are, as the
        // Slint client's does.
        if self.workspace() {
            subscriptions.push(iced::event::listen_with(|event, _, _| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Character(c),
                    modifiers,
                    repeat: false,
                    ..
                }) if c.as_str() == "k"
                    && modifiers.control()
                    && !modifiers.alt()
                    && !modifiers.shift() =>
                {
                    Some(Message::Switcher(crate::switcher::Event::Toggle))
                }
                _ => None,
            }));
        }
        // A dialog's field keeps Escape to itself, only letting go of the
        // keys: while a dialog with one is open, Escape closes the dialog,
        // as the Slint client's do.
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
        // A shown activity's time, each second.
        if self.profile_ticks() || self.active_now_ticks() || self.invites_ticking() {
            subscriptions.push(iced::time::every(Duration::from_secs(1)).map(|_| Message::Second));
        }
        // The microphone's level while the input menu shows it, the call's
        // clock, and the corner picture's drag.
        if self.metering() {
            subscriptions.push(
                iced::time::every(Duration::from_millis(50))
                    .map(|_| Message::Call(crate::call::Event::Meter)),
            );
        }
        // Settings: the gate's meter while it shows, the push-to-talk key
        // while it's recorded.
        if self.gate_metered() {
            subscriptions.push(
                iced::time::every(crate::settings::voice::METER_EVERY).map(|_| {
                    Message::Settings(crate::settings::Event::Voice(
                        crate::settings::voice::Event::Meter,
                    ))
                }),
            );
        }
        if self.recording_talk_key() {
            subscriptions.push(crate::settings::voice::record_keys());
        }
        if self.call.since.is_some() {
            subscriptions.push(
                iced::time::every(Duration::from_secs(1))
                    .map(|_| Message::Call(crate::call::Event::Second)),
            );
        }
        subscriptions.extend(self.arrange_subscription());
        subscriptions.extend(self.follow_subscription());
        subscriptions.push(self.away_subscription());
        subscriptions.extend(crate::settings::voice::window_talk_keys());
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
        if let Some(path) = control::path() {
            subscriptions.push(
                Subscription::run_with(path, |path| control::commands(path.clone()))
                    .map(Message::Control),
            );
        }
        Subscription::batch(subscriptions)
    }
}
