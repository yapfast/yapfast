//! Samples of leaving a server, and the move-sync confirmation, for tests and
//! shots, after the fake Discord's data (`show_leave`, `show_move_sync`:
//! `src/ui/refresh.rs:3530`, `:3566`): task N13's.

use yapfast_lang::tr;

use crate::Vm;
use crate::fixtures::shell;
use crate::places::leave::Leave;
use crate::shell::Layer;

/// Leaving a server, as `show_leave` words it.
pub fn server() -> Leave {
    let name = "Server";
    Leave {
        title: tr!("Leave '{}'", name),
        detail: tr!(
            "Are you sure you want to leave {}? You won't be able to rejoin this server unless you are re-invited.",
            name
        ),
        confirm: tr!("Leave server").into(),
        danger: true,
        ..Leave::default()
    }
}

/// Leaving a group DM, with the box to leave without telling the others.
pub fn group() -> Leave {
    let name = "Weekend plans";
    Leave {
        title: tr!("Leave '{}'", name),
        detail: tr!(
            "Are you sure you want to leave {}? You won't be able to rejoin this group unless you are re-invited.",
            name
        ),
        confirm: tr!("Leave group").into(),
        option: tr!("Leave without notifying other members").into(),
        danger: true,
        ..Leave::default()
    }
}

/// The group's, with the box ticked.
pub fn silent() -> Leave {
    Leave {
        ticked: true,
        ..group()
    }
}

/// Leaving, sent and waiting for Discord.
pub fn leaving() -> Leave {
    Leave {
        saving: true,
        ..server()
    }
}

/// Discord refused.
pub fn refused() -> Leave {
    Leave {
        error: "You are the owner of this server. Transfer ownership first.".into(),
        ..server()
    }
}

/// Whether a channel dragged into a category takes its permissions.
pub fn move_sync() -> Leave {
    Leave {
        title: tr!("Sync permissions?").into(),
        detail: tr!(
            "Would you like to sync {} permissions with {}?",
            "announcements",
            "Information"
        ),
        confirm: tr!("Sync Permissions").into(),
        alternative: tr!("Keep Current Permissions").into(),
        ..Leave::default()
    }
}

/// The server's window with `leave` asked over it.
pub fn asked(leave: Leave) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::Leave);
    vm.places.leave = leave;
    vm
}
