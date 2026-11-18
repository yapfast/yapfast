//! Keybinds in any app: push to talk's key (Voice & video's) and the
//! Keybinds page's, watched by `anomi_desktop::keybinds`, which the Slint app
//! shares, wherever the desktop lets it hear keys in every app, and else in
//! this window only, from the keys `crate::window`'s event filter hands on
//! (`keys::heard`). They run with the window behind another or closed to the
//! tray: holding keybinds set the microphone's atomics from the thread that
//! hears the key, and toggles come back here through the desktop's channel
//! ([`Heard::Keybind`]), numbered by watch so an old watcher's word is let
//! go. What a keybind is, and how it's kept, is the core's
//! (`yapfast_core::keybinds`); `docs/keybinds.md` says how they work.

pub mod keys;
pub mod record;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use anomi_desktop::keybinds::{Action, Status, TALK_SHORTCUT, Toggled, Watched, Watcher};
use anomi_widgets::focus::Scope;
use iced::Task;
use iced::keyboard::key::Physical;
use yapfast_core::application::Action as Act;
use yapfast_core::voice::Command;

use super::Heard;
use crate::app::App;
use crate::layout::{InputMode, Saved};
use crate::message::Message;

pub use record::Target;

/// The watcher, what it said of where keybinds work, the watch's number,
/// the keys push to talk holds and a recording.
#[derive(Default)]
pub struct Keybinds {
    watcher: Option<Watcher>,
    /// What is watched, the watcher's or, before the desktop's channel is
    /// up (and in tests), what would be.
    watched: Vec<Watched>,
    pub(crate) status: Option<Status>,
    count: u64,
    /// The keys push-to-talk keybinds hold, in their places, which don't
    /// also type into the composer while a call hears the microphone.
    talk: Vec<Physical>,
    pub(crate) recording: Option<record::Recording>,
}

/// What the watcher tells, from its own threads.
#[derive(Clone, Debug)]
pub enum Told {
    /// Where the keybinds work, and what the desktop gave each.
    Status(Status),
    /// A toggling keybind went down.
    Toggled(Action),
}

/// The keybinds to watch: push to talk's key and push-to-talk keybinds only
/// in push to talk, where they do something, as Discord's; none unassigned
/// or without keys (`watched_keybinds`, `src/ui.rs`).
pub fn watched(saved: &Saved) -> Vec<Watched> {
    let talking = saved.input_mode == InputMode::PushToTalk;
    let talk = (saved.push_to_talk_key.iter())
        .filter(|_| talking)
        .map(|keys| Watched {
            id: TALK_SHORTCUT.to_owned(),
            action: Action::PushToTalk,
            keys: keys.clone(),
        });
    let others = saved.keybinds.iter().filter_map(|binding| {
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

/// Hands the microphone push to talk's mode and release delay, as the
/// layout has them; at the start and whenever either changes.
pub fn carry(saved: &Saved) {
    yapfast_core::push_to_talk::set_on(saved.input_mode == InputMode::PushToTalk);
    yapfast_core::push_to_talk::set_release_delay(saved.release_delay());
}

/// Watches the keybinds as the layout has them now, if they changed: the
/// old watcher goes first, ending its portal session, and none runs with
/// none to watch. Call it after any change to `push_to_talk_key`,
/// `keybinds` or `input_mode`.
pub fn watch(app: &mut App) {
    let wanted = watched(&app.layout);
    let keybinds = &mut app.desktop.keybinds;
    if wanted == keybinds.watched {
        return;
    }
    keybinds.watcher = None;
    keybinds.status = None;
    keybinds.count += 1;
    keybinds.talk = anomi_desktop::keybinds::talk_codes(&wanted)
        .map(keys::physical)
        .collect();
    keybinds.watched = wanted;
    if let Some(tell) = app.desktop.tell.clone()
        && !keybinds.watched.is_empty()
    {
        let count = keybinds.count;
        let toggles = tell.clone();
        let toggled: Toggled = Arc::new(move |action| {
            let _ = toggles.send(Heard::Keybind(count, Told::Toggled(action)));
        });
        let told = move |status| {
            let _ = tell.send(Heard::Keybind(count, Told::Status(status)));
        };
        let watched = keybinds.watched.clone();
        keybinds.watcher = Some(Watcher::start(watched, &app.runtime, toggled, told));
    }
    record::show(app);
}

/// The watcher told something, by its watch's number.
pub fn told(app: &mut App, count: u64, told: Told) -> Task<Message> {
    if count != app.desktop.keybinds.count {
        return Task::none();
    }
    match told {
        Told::Status(status) => {
            app.desktop.keybinds.status = Some(status);
            record::show(app);
            Task::none()
        }
        Told::Toggled(action) => toggled(app, action),
    }
}

/// A toggling keybind went down: it does what its button does in the
/// window (`toggled`, `src/ui/keybinds.rs`), and with no window, what needs
/// none (`keybind`, `src/ui/shell.rs`).
pub fn toggled(app: &mut App, action: Action) -> Task<Message> {
    tracing::debug!(?action, hidden = app.hidden, "keybind toggled");
    if app.hidden {
        return hidden(app, action);
    }
    use anomi_vm::account::Intent as Card;
    match action {
        Action::ToggleMute => crate::intent::account::handle(app, Card::ToggleMute),
        Action::ToggleDeafen => crate::intent::account::handle(app, Card::ToggleDeafen),
        Action::ToggleVoiceMode => {
            app.layout.edit(|saved| {
                saved.input_mode = match saved.input_mode {
                    InputMode::PushToTalk => InputMode::VoiceActivity,
                    InputMode::VoiceActivity => InputMode::PushToTalk,
                };
            });
            carry(&app.layout);
            watch(app);
            Task::none()
        }
        Action::ToggleGoLive => go_live(app),
        Action::LeaveCall => command(app, Command::Leave),
        Action::Unassigned | Action::PushToTalk | Action::PushToMute => Task::none(),
    }
}

/// With the window closed to the tray: leaving, stopping Go Live, muting
/// and deafening in a call. Starting Go Live needs a source chosen, so it
/// starts where the desktop has its own dialog, and elsewhere the window
/// opens first.
fn hidden(app: &mut App, action: Action) -> Task<Message> {
    let voice = &app.core.state.voice;
    let active = voice.active();
    let chosen = match action {
        Action::LeaveCall => Command::Leave,
        Action::ToggleMute if active => Command::Mute(!voice.muted),
        Action::ToggleDeafen if active => Command::Deafen(!voice.deafened),
        Action::ToggleGoLive if voice.sharing.is_some() => Command::StopLive,
        #[cfg(feature = "discord")]
        Action::ToggleGoLive if active && yapfast_core::stream_worker::portal_picks() => {
            Command::GoLive(voice.quality)
        }
        Action::ToggleGoLive if active => {
            let shown = super::window::open(app, None);
            return Task::batch([shown, go_live(app)]);
        }
        _ => return Task::none(),
    };
    command(app, chosen)
}

/// Go Live's keybind, as the window's `toggle-share`: stops a stream, or
/// starts one at the quality last chosen.
fn go_live(app: &mut App) -> Task<Message> {
    let voice = &app.core.state.voice;
    let chosen = if voice.sharing.is_some() {
        Command::StopLive
    } else {
        Command::GoLive(voice.quality)
    };
    command(app, chosen)
}

fn command(app: &mut App, command: Command) -> Task<Message> {
    app.dispatch(Act::Voice(command))
}

/// The keys that talk while held, which the focus scope keeps from typing
/// into the composer while a call hears the microphone (`type_anywhere`,
/// `src/ui.rs:883`).
pub fn talk_keys(app: &App) -> &[Physical] {
    let voice = &app.core.state.voice;
    if voice.active() && !voice.muted {
        &app.desktop.keybinds.talk
    } else {
        &[]
    }
}

/// The focus scope with the keybinds' keys: those that talk don't type,
/// and while one records the window gets no key.
pub fn scope<'a, M>(app: &'a App, scope: Scope<'a, M>) -> Scope<'a, M> {
    scope
        .talk_keys(talk_keys(app))
        .take_keys(app.desktop.keybinds.recording.is_some())
}

/// What Voice & video says under push to talk's key, while it's watched.
pub fn talk_note(app: &App) -> String {
    let keybinds = &app.desktop.keybinds;
    let talk = (keybinds.watched.iter()).any(|watched| watched.id == TALK_SHORTCUT);
    (keybinds.status.as_ref())
        .filter(|_| talk)
        .map(Status::talk_note)
        .unwrap_or_default()
}

/// What the desktop did with keybind `id`, while it's watched and the
/// desktop runs keybinds.
pub fn given_note(app: &App, id: &str) -> Option<String> {
    let keybinds = &app.desktop.keybinds;
    (keybinds.status.as_ref())
        .filter(|_| keybinds.watched.iter().any(|watched| watched.id == id))
        .and_then(|status| status.given_note(id))
}
