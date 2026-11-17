//! Samples of the join dialog for tests and shots, after the fake Discord's
//! data: task N12's.

use yapfast_lang::{tr, trn};

use crate::Vm;
use crate::fixtures::{avatar, pictures, shell};
use crate::places::join::{Found, Join};
use crate::shell::Layer;

/// Join a Server with the field empty (`dialog/add_server.png`).
pub fn adding_dialog() -> Join {
    Join {
        title: tr!("Join a Server").into(),
        field: true,
        confirm: tr!("Find Invite").into(),
        ..Join::default()
    }
}

/// The server an invite leads to, with its icon and its members.
pub fn server() -> Found {
    let face = avatar("F", 0x5865f2);
    Found {
        name: "Fake Friends".into(),
        avatar: pictures::pictured(face, "Server"),
        detail: trn!("{} online, {n} member", "{} online, {n} members", 40, 12),
    }
}

/// The invite looked up: the server shows, and Join Server is on offer, with
/// joining in the browser beside it.
pub fn found() -> Join {
    Join {
        link: "https://discord.gg/yapfast".into(),
        found: Some(server()),
        confirm: tr!("Join Server").into(),
        risk: true,
        ..adding_dialog()
    }
}

/// Looking the invite up: the buttons wait.
pub fn looking() -> Join {
    Join {
        link: "https://discord.gg/yapfast".into(),
        confirm: tr!("Find Invite").into(),
        busy: true,
        status: tr!("Looking up the invite...").into(),
        ..adding_dialog()
    }
}

/// Joining: the server shows and the buttons wait.
pub fn joining() -> Join {
    Join {
        busy: true,
        status: tr!("Joining...").into(),
        risk: false,
        ..found()
    }
}

/// The invite is no good.
pub fn failed() -> Join {
    Join {
        link: "nonsense".into(),
        error: tr!("This invite is invalid or has expired.").into(),
        ..adding_dialog()
    }
}

/// Discord asks for a captcha first.
pub fn captcha() -> Join {
    Join {
        captcha: true,
        status: tr!("Discord asks for a captcha first. Solve it in your browser.").into(),
        ..found()
    }
}

/// A clicked link: no field, the server, and Join Server.
pub fn from_link() -> Join {
    Join {
        title: tr!("You've been invited to join a server").into(),
        field: false,
        ..found()
    }
}

/// The server asks for a form, which opens in the browser.
pub fn form() -> Join {
    Join {
        form: true,
        confirm: tr!("Open in browser").into(),
        status: tr!(
            "You joined. This server asks new members to fill in a form before they can talk, which Yapfast can't show yet. Finish it in Discord's web app."
        )
        .into(),
        risk: false,
        ..from_link()
    }
}

/// The window on the first server with the dialog open on `join`.
pub fn over(join: Join) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::Join);
    vm.places.join = join;
    vm
}

/// Join a Server from the rail's +.
pub fn adding() -> Vm {
    over(adding_dialog())
}
