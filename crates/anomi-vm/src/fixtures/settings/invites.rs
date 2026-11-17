//! Samples of a server's Invites settings for tests and shots, after the fake
//! Discord's data: task X04's.

use iced::Color;

use crate::Vm;
use crate::fixtures::pictures::pictured;
use crate::fixtures::{avatar, settings::server, shell};
use crate::settings::invites::{Invites, Row};
use crate::settings::server::Section;
use crate::shell::Layer;

/// The fake's two invites: Bob's that never ends, and Carol's for a week,
/// made a day ago, giving Mods.
pub fn invites() -> Invites {
    let row = |code: &str, who: &str, tint, uses: &str, expires_in| Row {
        code: code.into(),
        inviter: pictured(avatar(&who[..1], tint), who),
        name: who.to_lowercase(),
        channel: "#general".into(),
        uses: uses.into(),
        expires_in,
        ..Row::default()
    };
    Invites {
        rows: vec![
            row("bobinvite", "Bob", 0xe0807a, "12", -1),
            Row {
                role: "Mods".into(),
                role_color: Color::from_rgb8(0x34, 0x98, 0xdb),
                ..row("carol1", "Carol", 0x9b59b6, "3/10", 6 * 86_400 - 2)
            },
        ],
        may_create: true,
        ..Invites::default()
    }
}

/// The window with the server's settings open on the Invites.
pub fn page() -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ServerSettings);
    vm.settings.server = server::server();
    vm.settings.server.section = Section::Invites;
    vm.settings.invites = invites();
    vm
}

/// The same with invites paused, Carol's giving two roles more and Alice's
/// about to end.
pub fn paused() -> Vm {
    let mut vm = page();
    let invites = &mut vm.settings.invites;
    invites.paused = true;
    invites.rows[1].more_roles = "+2".into();
    invites.rows.push(Row {
        code: "aB3dEf".into(),
        inviter: pictured(avatar("A", 0x5b8def), "Alice"),
        name: "alice".into(),
        channel: "#general".into(),
        uses: "0/1".into(),
        expires_in: 45,
        ..Row::default()
    });
    vm
}

/// No invite links.
pub fn empty() -> Vm {
    let mut vm = page();
    vm.settings.invites.rows.clear();
    vm
}
