//! Samples of a server's Boost Perks settings for tests and shots, after the
//! fake Discord's data: task X02's.

use crate::Vm;
use crate::fixtures::pictures::sunset;
use crate::fixtures::settings::server::server;
use crate::fixtures::shell;
use crate::settings::boosts::{Boosts, Picture};
use crate::settings::server::{Section, Server};
use crate::shell::Layer;
use yapfast_lang::tr;

/// A server before its first level: both backgrounds locked, as the golden
/// screenshot has them.
pub fn boosts() -> Boosts {
    Boosts {
        banner: Picture {
            locked: tr!("Unlocks at Level {}", 2),
            ..Picture::default()
        },
        invite: Picture {
            locked: tr!("Unlocks at Level {}", 1),
            ..Picture::default()
        },
        ..Boosts::default()
    }
}

/// At Level 2: both backgrounds unlocked, the banner set.
pub fn unlocked() -> Boosts {
    Boosts {
        banner: Picture {
            picture: Some(sunset(480, 270)),
            shown: true,
            ..Picture::default()
        },
        invite: Picture::default(),
        progress_bar: true,
        ..Boosts::default()
    }
}

/// The frame of a server at a level.
fn frame(detail: String) -> Server {
    Server {
        section: Section::BoostPerks,
        detail,
        ..server()
    }
}

/// The window with the server's settings open on Boost Perks.
pub fn page() -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ServerSettings);
    vm.settings.server = frame(tr!("No Level").into());
    vm.settings.boosts = boosts();
    vm
}

/// The same at Level 2.
pub fn level_two() -> Vm {
    let mut vm = page();
    vm.settings.server = frame(tr!("Level {}", 2));
    vm.settings.boosts = unlocked();
    vm
}
