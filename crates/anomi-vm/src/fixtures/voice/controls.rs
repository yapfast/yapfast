//! Samples of the call controls for tests and shots, after the fake Discord's
//! data: task V05's. The Lounge's call as Discord's has it: camera, sharing
//! and the soundboard offered.

use crate::Vm;
use crate::voice::controls::Controls;

use super::super::{conversation, shell};

/// In the Lounge's call, everything at rest (`voice/stage.png`'s pill).
pub fn joined() -> Controls {
    Controls {
        ready: true,
        can_share: true,
        can_film: true,
        can_soundboard: true,
        ..Controls::default()
    }
}

/// A DM's call (`dmcall/connected_bob_dm.png`): the same pill as the
/// Lounge's, soundboard included.
pub fn dm_call() -> Controls {
    joined()
}

/// Muted and deafened, both red.
pub fn silenced() -> Controls {
    Controls {
        muted: true,
        deafened: true,
        ..joined()
    }
}

/// The camera on its way.
pub fn starting_camera() -> Controls {
    Controls {
        camera_on: true,
        camera_starting: true,
        ..joined()
    }
}

/// The camera on and the screen shared: both lit, the share's cross
/// stops it.
pub fn live() -> Controls {
    Controls {
        camera_on: true,
        sharing: true,
        ..joined()
    }
}

/// Joining: nothing but the hang-up answers yet.
pub fn joining() -> Controls {
    Controls {
        ready: false,
        ..joined()
    }
}

/// Hanging up: the hang-up faded.
pub fn leaving() -> Controls {
    Controls {
        leaving: true,
        ..joined()
    }
}

/// The Lounge's window with `controls` under its stage.
pub fn window(controls: Controls) -> Vm {
    let mut vm = shell::channel_with(conversation::voice);
    vm.voice.controls = controls;
    vm
}

/// Bob's DM with the call above its messages.
pub fn dm_window(controls: Controls) -> Vm {
    let mut vm = shell::dm_with(conversation::dm_call_here);
    vm.voice.controls = controls;
    vm
}
