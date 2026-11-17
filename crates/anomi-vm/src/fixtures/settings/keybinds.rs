//! Samples of the Keybinds settings page for tests and shots, after the fake
//! Discord's data: task S09's.

use crate::Vm;
use crate::fixtures::settings::frame::settings;
use crate::settings::frame::Page;
use crate::settings::keybinds::{Action, Bind, Keybinds};

fn bind(action: Action, keys: &str, desktop: &str) -> Bind {
    Bind {
        action,
        keys: keys.into(),
        desktop: desktop.into(),
    }
}

/// No keybind yet: the golden screenshot's.
pub fn empty() -> Keybinds {
    Keybinds::default()
}

/// A few keybinds, one without keys.
pub fn some() -> Keybinds {
    Keybinds {
        rows: vec![
            bind(Action::PushToTalk, "Ctrl + Shift + M", ""),
            bind(Action::ToggleMute, "F9", ""),
            bind(Action::LeaveCall, "", ""),
        ],
        ..Keybinds::default()
    }
}

/// The window with the settings open on Keybinds, with none.
pub fn page() -> Vm {
    let mut vm = settings(Page::Keybinds);
    vm.settings.keybinds = empty();
    vm
}

/// With keybinds, where the desktop runs them, and the second row recording.
pub fn listed() -> Vm {
    let mut vm = settings(Page::Keybinds);
    let mut keybinds = some();
    keybinds.note = "Your desktop runs these keybinds in every app: it may ask you to allow each, or give one other keys. Change them in your system's global shortcuts settings.".into();
    keybinds.rows[0].desktop = "Your desktop gave it the shortcut CTRL+SHIFT+m.".into();
    keybinds.rows[1].desktop = "Your desktop hasn't allowed it yet.".into();
    keybinds.recording = Some(1);
    keybinds.recorded = "Ctrl + ".into();
    vm.settings.keybinds = keybinds;
    vm
}
