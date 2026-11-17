//! Samples of a server's settings frame and its profile page for tests and
//! shots, after the fake Discord's data: task X01's.

use crate::Vm;
use crate::fixtures::pictures;
use crate::fixtures::{avatar, shell};
use crate::kind::Avatar;
use crate::settings::frame::Unsaved;
use crate::settings::server::{Profile, Section, Server};
use crate::shell::Layer;
use yapfast_lang::tr;

/// What the fake's account sees: every section and the profile page loaded,
/// as the golden screenshot has it.
pub fn server() -> Server {
    Server {
        name: "Server".into(),
        sections: vec![
            Section::Profile,
            Section::Engagement,
            Section::BoostPerks,
            Section::Emoji,
            Section::Stickers,
            Section::Members,
            Section::Roles,
            Section::Invites,
            Section::Safety,
            Section::AuditLog,
            Section::Bans,
        ],
        loaded: true,
        close: tr!("Close {}", tr!("Settings")),
        profile: Profile {
            name: "Server".into(),
            description_left: 300,
            icon: Avatar {
                picture: pictures::server("Server"),
                ..avatar("S", 0xe0807a)
            },
            has_icon: true,
            ..Profile::default()
        },
        ..Server::default()
    }
}

/// The window with the server's settings open on the profile.
pub fn profile() -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ServerSettings);
    vm.settings.server = server();
    vm
}

/// What the server's owner sees: `server` with Delete Server last.
pub fn owner() -> Server {
    Server {
        may_delete: true,
        ..server()
    }
}

/// The owner's with the bar for changes not saved, after the name was
/// edited.
pub fn edited() -> Server {
    let mut server = owner();
    server.profile.name = "Server of friends".into();
    server.unsaved = Some(Unsaved::default());
    server
}
