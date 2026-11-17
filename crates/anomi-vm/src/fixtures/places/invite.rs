//! Samples of the invite dialog for tests and shots, after the fake Discord's
//! data: task N12's.

use yapfast_lang::{tr, trn};

use crate::Vm;
use crate::fixtures::shell;
use crate::places::invite::Invite;
use crate::shell::Layer;

/// A link made: seven days, no limit, as the golden shot shows it.
pub fn link() -> Invite {
    Invite {
        title: tr!("Invite friends to {}", "Server"),
        detail: tr!("They'll land in {}.", "#general"),
        link: "https://discord.gg/qsGhsSbQ".into(),
        note: tr!(
            "This link {}.",
            trn!("expires in {n} day", "expires in {n} days", 7)
        ),
        age: 5,
        uses: 0,
        ..Invite::default()
    }
}

/// Discord is making one: no link yet, and Generate waits.
pub fn making() -> Invite {
    Invite {
        link: String::new(),
        note: String::new(),
        making: true,
        ..link()
    }
}

/// Making one failed.
pub fn failed() -> Invite {
    Invite {
        link: String::new(),
        note: String::new(),
        error: "Discord refused to make the link.".into(),
        ..link()
    }
}

/// The window on the first server with the dialog open on `invite`.
pub fn over(invite: Invite) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::Invite);
    vm.places.invite = invite;
    vm
}

/// The dialog with the link made (`dialog/invite_to_server.png`).
pub fn made() -> Vm {
    over(link())
}
