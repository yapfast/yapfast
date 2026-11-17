//! Samples of the voice dock for tests and shots, after the fake Discord's
//! data: task V01's. The dock's own states, and the windows it tops.

use yapfast_lang::tr;

use crate::Vm;
use crate::voice::dock::{Dock, Signal, Tone};

use super::super::shell;

/// In the Lounge's call a moment after it connected: Discord's call, so
/// noise suppression, the soundboard, the camera and sharing are offered
/// (`voice/joined_sidebar_dock.png`).
pub fn joined() -> Dock {
    Dock {
        status: tr!("Voice connected").into(),
        time: "0:02".into(),
        detail: tr!("{} · Secure connection", "Lounge"),
        name: "Lounge".into(),
        tone: Tone::Connected,
        active: true,
        ready: true,
        signal: Some(Signal::Unknown),
        ping: "42 ms".into(),
        can_share: true,
        can_film: true,
        can_soundboard: true,
        noise_on: true,
        can_noise: true,
        share_sound_on: true,
        ..Dock::default()
    }
}

/// A DM call (`dmcall/connected_bob_dm.png`): the same, named for Bob.
pub fn dm_call() -> Dock {
    Dock {
        time: "0:06".into(),
        detail: tr!("{} · Secure connection", "Bob"),
        name: "Bob".into(),
        signal: Some(Signal::Fine),
        ..joined()
    }
}

/// Before the connection is up: the speaker in the accent, nothing to press
/// but hanging up.
pub fn joining() -> Dock {
    Dock {
        status: tr!("Joining voice...").into(),
        time: String::new(),
        tone: Tone::Working,
        ready: false,
        signal: None,
        ping: String::new(),
        ..joined()
    }
}

/// The camera on its way, and noise suppression off (red).
pub fn starting_camera() -> Dock {
    Dock {
        camera_starting: true,
        camera_on: true,
        noise_on: false,
        ..joined()
    }
}

/// Sharing, live, with three watching and the stream's sound on.
pub fn sharing() -> Dock {
    Dock {
        sharing: true,
        share_live: true,
        share_watchers: 3,
        ..joined()
    }
}

/// Sharing, the system's dialog still choosing what to share.
pub fn choosing() -> Dock {
    Dock {
        sharing: true,
        share_live: false,
        share_status: tr!("Choose what to share").into(),
        ..joined()
    }
}

/// Connected with a warning to read, the signal poor.
pub fn warned() -> Dock {
    Dock {
        signal: Some(Signal::Bad),
        warning: "Bob: packets are being lost".into(),
        camera_on: true,
        ..joined()
    }
}

/// An error in a call that goes on: it has a cross.
pub fn failed_in_call() -> Dock {
    Dock {
        error: "The camera is in use by another program".into(),
        ..joined()
    }
}

/// The call is over and failed: only its error and "Dismiss".
pub fn failed() -> Dock {
    Dock {
        status: tr!("Voice disconnected").into(),
        tone: Tone::Disconnected,
        error: "Couldn't reach the voice server".into(),
        ..Dock::default()
    }
}

/// The window with `dock` on the account card.
pub fn window(dock: Dock) -> Vm {
    with(shell::server_home(), dock)
}

/// A DM's window with `dock` on the account card.
pub fn dm_window(dock: Dock) -> Vm {
    with(super::stage::dm_window(super::stage::dm_call()), dock)
}

fn with(mut vm: Vm, dock: Dock) -> Vm {
    vm.shell.voice_dock = true;
    vm.voice.dock = dock;
    vm
}
