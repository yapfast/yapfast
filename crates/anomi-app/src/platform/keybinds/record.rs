//! Settings' side of keybinds (`src/ui/keybinds.rs`): recording a keybind's
//! keys from the window's own key events, adding, removing and choosing the
//! Keybinds page's rows, and what Voice & video and the Keybinds page show
//! of them ([`show`]). While one records, keybinds rest
//! (`anomi_desktop::keybinds::pause`), the window's event filter hands every
//! key on as [`Message::Recorded`] and the focus scope keeps the keys from
//! the window (`Scope::take_keys`): each key pressed joins the keybind, and
//! it's recorded once they're all up again; Escape, pressed first, stops
//! without changing it; the window losing the focus records what was
//! pressed.

use std::sync::atomic::{AtomicBool, Ordering};

use anomi_desktop::keybinds::{Action, Binding, Recorder, Shortcut};
use anomi_vm::settings::keybinds::{Action as Shown, Bind};
use iced::keyboard::key::{Named, Physical};
use iced::keyboard::{Key as Logical, Location};

use crate::app::App;

/// Which recorder records: Voice & video's push to talk, or a row of the
/// Keybinds page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Talk,
    Row(usize),
}

/// A recording: its target and the keys pressed so far.
pub struct Recording {
    target: Target,
    recorder: Recorder,
}

/// The window's event filter hands keys on while one records.
static RECORDING: AtomicBool = AtomicBool::new(false);

/// Whether a keybind records now, as the window's event filter asks.
pub fn recording() -> bool {
    RECORDING.load(Ordering::Relaxed)
}

/// A key of the window's while one records.
#[derive(Clone, Debug)]
pub struct Input {
    pub key: Logical,
    pub physical: Physical,
    pub location: Location,
    pub pressed: bool,
    pub repeat: bool,
}

/// Starts recording `target`'s keys, with none pressed yet, or stops
/// (none); one recording stops as another starts.
pub fn record(app: &mut App, target: Option<Target>) {
    app.desktop.keybinds.recording = target.map(|target| Recording {
        target,
        recorder: Recorder::default(),
    });
    RECORDING.store(target.is_some(), Ordering::Relaxed);
    anomi_desktop::keybinds::pause(target.is_some());
    show(app);
}

/// A key while recording.
pub fn key(app: &mut App, input: Input) {
    if input.pressed && input.repeat {
        return;
    }
    let Some(recording) = &mut app.desktop.keybinds.recording else {
        return;
    };
    let escape = input.key == Logical::Named(Named::Escape);
    if input.pressed && escape && recording.recorder.is_empty() {
        return record(app, None);
    }
    let Some(key) = super::keys::recorded(&input.key, input.physical, input.location) else {
        return;
    };
    let (done, target) = (recording.recorder.key(key, input.pressed), recording.target);
    match done {
        Some(keys) => recorded(app, target, Some(keys)),
        None => show(app),
    }
}

/// The window lost the focus: its keys' release won't come, so what was
/// pressed is the keybind.
pub fn unfocused(app: &mut App) {
    if let Some(recording) = &mut app.desktop.keybinds.recording {
        let (target, keys) = (recording.target, recording.recorder.finish());
        recorded(app, target, keys);
    }
}

/// Recording ended, with these keys or none: they're kept and watched.
fn recorded(app: &mut App, target: Target, keys: Option<Shortcut>) {
    record(app, None);
    let Some(keys) = keys else {
        return;
    };
    tracing::info!(keys = keys.name(), ?target, "keybind recorded");
    app.layout.edit(|saved| match target {
        Target::Talk => saved.push_to_talk_key = Some(keys),
        Target::Row(row) => {
            if let Some(binding) = saved.keybinds.get_mut(row) {
                binding.keys = Some(keys);
            }
        }
    });
    super::watch(app);
    show(app);
}

/// "Add a Keybind": a row doing nothing, with no keys, numbered after the
/// others so the desktop keeps what it allowed for them.
pub fn add(app: &mut App) {
    app.layout.edit(|saved| {
        let id = saved.keybinds.iter().map(|binding| binding.id).max();
        saved.keybinds.push(Binding {
            id: id.unwrap_or(0) + 1,
            action: Action::Unassigned,
            keys: None,
        });
    });
    show(app);
}

/// A row's remove button; a recording stops with it.
pub fn remove(app: &mut App, row: usize) {
    if row >= app.layout.keybinds.len() {
        return;
    }
    app.layout.edit(|saved| {
        saved.keybinds.remove(row);
    });
    record(app, None);
    super::watch(app);
    show(app);
}

/// An action chosen for a row, by its place in `Action::OFFERED`.
pub fn choose(app: &mut App, row: usize, action: usize) {
    let Some(&action) = Action::OFFERED.get(action) else {
        return;
    };
    if app
        .layout
        .keybinds
        .get(row)
        .is_none_or(|binding| binding.action == action)
    {
        return;
    }
    app.layout.edit(|saved| saved.keybinds[row].action = action);
    super::watch(app);
    show(app);
}

/// What Voice & video and the Keybinds page show of the keybinds: the keys,
/// a recording, where keybinds work and what the desktop gave each.
pub fn show(app: &mut App) {
    let (target, recorded) = match &app.desktop.keybinds.recording {
        Some(recording) => (Some(recording.target), recording.recorder.shown()),
        None => (None, String::new()),
    };
    let talk_note = super::talk_note(app);
    let note = (app.desktop.keybinds.status.as_ref())
        .map(anomi_desktop::keybinds::Status::page_note)
        .unwrap_or_default();
    let rows = (app.layout.keybinds.iter())
        .map(|binding| Bind {
            action: Shown::ALL
                .get(
                    Action::OFFERED
                        .iter()
                        .position(|&a| a == binding.action)
                        .unwrap_or(0),
                )
                .copied()
                .unwrap_or_default(),
            keys: binding
                .keys
                .as_ref()
                .map(Shortcut::name)
                .unwrap_or_default(),
            desktop: super::given_note(app, &binding.shortcut_id()).unwrap_or_default(),
        })
        .collect();
    let talk_key = (app.layout.push_to_talk_key.as_ref())
        .map(Shortcut::name)
        .unwrap_or_default();
    let settings = &mut app.vm.settings;
    let voice = &mut settings.voice;
    voice.talk_key = talk_key;
    voice.talk_recording = target == Some(Target::Talk);
    voice.recorded_keys = if voice.talk_recording {
        recorded.clone()
    } else {
        String::new()
    };
    voice.talk_note = talk_note;
    let page = &mut settings.keybinds;
    page.rows = rows;
    page.note = note;
    page.recording = match target {
        Some(Target::Row(row)) => Some(row),
        _ => None,
    };
    page.recorded = if page.recording.is_some() {
        recorded
    } else {
        String::new()
    };
}
