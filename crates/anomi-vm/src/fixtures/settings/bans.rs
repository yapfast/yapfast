//! Samples of a server's Bans for tests and shots, after the fake Discord's
//! data: task X06's.

use crate::Vm;
use crate::fixtures::pictures::picture;
use crate::fixtures::{avatar, settings::server, shell};
use crate::kind::Avatar;
use crate::settings::bans::{Bans, Open, Row};
use crate::settings::server::Section;
use crate::shell::Layer;

/// Two bans, one page: the fake's, who have no avatar of their own and so
/// show Discord's default one, the same for both (`(id >> 22) % 6` is 2).
pub fn bans() -> Bans {
    let row = |id: &str, name: &str, username: &str| Row {
        id: id.into(),
        avatar: Avatar {
            picture: Some(picture("/embed/avatars/2.png")),
            ..avatar(&name[..1], 0x5865f2)
        },
        name: name.into(),
        username: username.into(),
    };
    Bans {
        rows: vec![
            row("1400000000000000901", "Mallory", "mallory"),
            row("1400000000000000902", "trent_42", ""),
        ],
        page: 0,
        pages: 1,
        ..Bans::default()
    }
}

/// A ban opened, for someone who may lift it.
pub fn open() -> Open {
    Open {
        name: "Mallory".into(),
        reason: "Spam in #general".into(),
        may_unban: true,
        ..Open::default()
    }
}

/// The Bans with Mallory's ban open over them.
pub fn dialog() -> Vm {
    let mut vm = page();
    vm.settings.bans.open = Some(open());
    shell::open(vm, Layer::Ban)
}

/// The window with the server's settings open on the Bans.
pub fn page() -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ServerSettings);
    vm.settings.server = server::server();
    vm.settings.server.section = Section::Bans;
    vm.settings.bans = bans();
    vm
}
