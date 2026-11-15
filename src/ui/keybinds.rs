//! Keybinds in settings: the Keybinds page's rows, recording a keybind's
//! keys from the window's key events, and what a toggling keybind does. The
//! keys themselves are watched by [`crate::keybinds`].

use slint::winit_030::winit::{
    event::{ElementState, WindowEvent},
    keyboard::{Key as LogicalKey, NamedKey},
};

use super::*;
use crate::keybinds::{Action as KeyAction, Binding, Recorder, Shortcut, Status, Watched};

thread_local! {
    /// The keys pressed so far while a recorder records.
    static RECORDER: RefCell<Recorder> = RefCell::new(Recorder::default());
}

pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    let weak = window.as_weak();
    window.on_record_talk_key(move |on| {
        if let Some(window) = weak.upgrade() {
            window.set_keybind_recording(-1);
            window.set_talk_recording(on);
            recording_changed(&window);
        }
    });
    let weak = window.as_weak();
    window.on_record_keybind(move |index, on| {
        if let Some(window) = weak.upgrade() {
            window.set_talk_recording(false);
            window.set_keybind_recording(if on { index } else { -1 });
            recording_changed(&window);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_add_keybind(move || {
        let mut adapter = state.borrow_mut();
        let keybinds = &mut adapter.layout.keybinds;
        let id = keybinds.iter().map(|binding| binding.id).max().unwrap_or(0) + 1;
        keybinds.push(Binding {
            id,
            action: KeyAction::Unassigned,
            keys: None,
        });
        changed(&mut adapter, weak.upgrade());
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_remove_keybind(move |index| {
        let mut adapter = state.borrow_mut();
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        if index >= adapter.layout.keybinds.len() {
            return;
        }
        adapter.layout.keybinds.remove(index);
        if let Some(window) = weak.upgrade() {
            stop(&window);
        }
        changed(&mut adapter, weak.upgrade());
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_keybind_action_chosen(move |index, action| {
        let mut adapter = state.borrow_mut();
        let (Ok(index), Some(&action)) = (
            usize::try_from(index),
            usize::try_from(action)
                .ok()
                .and_then(|action| KeyAction::OFFERED.get(action)),
        ) else {
            return;
        };
        let Some(binding) = adapter.layout.keybinds.get_mut(index) else {
            return;
        };
        if binding.action != action {
            binding.action = action;
            changed(&mut adapter, weak.upgrade());
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_keybind_status_changed(move || {
        if let (Some(window), Ok(adapter)) = (weak.upgrade(), state.try_borrow()) {
            adapter.show_keybinds(&window);
        }
    });
}

/// Saves the keybinds, watches them as they are now, and shows them.
fn changed(adapter: &mut Adapter, window: Option<YapfastWindow>) {
    adapter.layout.write(&adapter.root);
    if let Some(window) = window {
        adapter.watch_keybinds(&window);
    }
}

/// Recording starts or stops: keybinds rest while it records, and it starts
/// with no keys.
fn recording_changed(window: &YapfastWindow) {
    crate::keybinds::pause(window.get_talk_recording() || window.get_keybind_recording() >= 0);
    RECORDER.take();
    window.set_recorded_keys("".into());
}

fn stop(window: &YapfastWindow) {
    window.set_talk_recording(false);
    window.set_keybind_recording(-1);
    recording_changed(window);
}

/// Which recorder records.
#[derive(Clone, Copy)]
enum Target {
    Talk,
    Row(usize),
}

/// While a recorder records, the window's keys go to it, and Mouse 4 and 5
/// on Windows: each key pressed joins the keybind, and it's recorded once
/// they're all up again. Escape, pressed first, stops without changing it.
/// Returns whether the event went to the recorder.
pub(super) fn record(
    window: &YapfastWindow,
    adapter: &std::rc::Weak<RefCell<Adapter>>,
    event: &WindowEvent,
) -> bool {
    let target = if window.get_talk_recording() {
        Target::Talk
    } else if let Ok(row) = usize::try_from(window.get_keybind_recording()) {
        Target::Row(row)
    } else {
        return false;
    };
    if !window.get_settings_open() {
        stop(window);
        return false;
    }
    let (key, pressed) = match event {
        WindowEvent::KeyboardInput { event: key, .. } => {
            let pressed = key.state == ElementState::Pressed;
            if pressed && key.repeat {
                return true;
            }
            if pressed
                && key.logical_key == LogicalKey::Named(NamedKey::Escape)
                && RECORDER.with_borrow(Recorder::is_empty)
            {
                stop(window);
                return true;
            }
            match crate::keybinds::Key::recorded(key) {
                Some(recorded) => (recorded, pressed),
                None => return true,
            }
        }
        #[cfg(windows)]
        WindowEvent::MouseInput { state, button, .. } => {
            match crate::keybinds::Key::mouse(*button) {
                Some(recorded) => (recorded, *state == ElementState::Pressed),
                None => return false,
            }
        }
        WindowEvent::Focused(false) => {
            // Its keys' release won't come: what was pressed is the keybind.
            let done = RECORDER.with_borrow_mut(Recorder::finish);
            recorded(window, adapter, target, done);
            return false;
        }
        _ => return false,
    };
    let (done, shown) = RECORDER.with_borrow_mut(|recorder| {
        let done = recorder.key(key, pressed);
        (done, recorder.shown())
    });
    window.set_recorded_keys(shown.into());
    if done.is_some() {
        recorded(window, adapter, target, done);
    }
    true
}

/// Recording ended, with these keys or none.
fn recorded(
    window: &YapfastWindow,
    adapter: &std::rc::Weak<RefCell<Adapter>>,
    target: Target,
    keys: Option<Shortcut>,
) {
    stop(window);
    let Some(keys) = keys else {
        return;
    };
    // After the event, which may come while the adapter is borrowed.
    let (adapter, weak) = (adapter.clone(), window.as_weak());
    slint::Timer::single_shot(Duration::ZERO, move || {
        let (Some(adapter), Some(window)) = (adapter.upgrade(), weak.upgrade()) else {
            return;
        };
        let mut adapter = adapter.borrow_mut();
        match target {
            Target::Talk => adapter.set_talk_key(&window, keys),
            Target::Row(row) => {
                let Some(binding) = adapter.layout.keybinds.get_mut(row) else {
                    return;
                };
                tracing::info!(keys = keys.name(), action = ?binding.action, "keybind recorded");
                binding.keys = Some(keys);
                changed(&mut adapter, Some(window));
            }
        }
    });
}

/// A toggling keybind went down, on the window's thread.
pub(super) fn toggled(window: &YapfastWindow, action: KeyAction) {
    tracing::debug!(?action, "keybind toggled");
    match action {
        KeyAction::ToggleMute => window.invoke_toggle_mute(),
        KeyAction::ToggleDeafen => window.invoke_toggle_deafen(),
        KeyAction::ToggleVoiceMode => {
            window.set_input_mode(if window.get_input_mode() == 1 { 0 } else { 1 });
            window.invoke_preferences_changed();
        }
        KeyAction::Unassigned | KeyAction::PushToTalk | KeyAction::PushToMute => {}
    }
}

/// Shows the Keybinds page's rows, and where keybinds work.
pub(super) fn show(
    window: &YapfastWindow,
    keybinds: &[Binding],
    watched: &[Watched],
    status: Option<&Status>,
) {
    let rows: Vec<KeybindRow> = keybinds
        .iter()
        .map(|binding| {
            let id = binding.shortcut_id();
            let desktop = status
                .filter(|_| watched.iter().any(|watched| watched.id == id))
                .and_then(|status| status.given_note(&id))
                .unwrap_or_default();
            KeybindRow {
                action: KeyAction::OFFERED
                    .iter()
                    .position(|&action| action == binding.action)
                    .unwrap_or(0) as i32,
                keys: binding
                    .keys
                    .as_ref()
                    .map(Shortcut::name)
                    .unwrap_or_default()
                    .into(),
                description: yapfast_lang::text(binding.action.description()).into(),
                desktop: desktop.into(),
            }
        })
        .collect();
    window.set_keybind_rows(ModelRc::new(VecModel::from(rows)));
    window.set_keybinds_note(status.map(Status::page_note).unwrap_or_default().into());
}
