//! Sample view-models after the fake Discord's account
//! (`testing/fake_discord.ts --dms 6 --servers 3`): Alice signed in, Bob and
//! a group among her DMs. Snapshot tests and `anomi-shots` draw these. Text
//! the projection or the core would translate goes through `tr!` here too,
//! so a shot shows the language `anomi-shots` chose; tests choose none and
//! read the code's English.

use iced::Color;
use yapfast_lang::{tr, trn};

use crate::kind::{Avatar, Presence};
use crate::rail::{Rail, Server, Tile};
use crate::sidebar::{ChannelKind, ChannelRow, DmRow, Dms, Place, Sidebar};
use crate::{Vm, account, conversation, shell};

fn avatar(initial: &str, tint: u32) -> Avatar {
    Avatar {
        initial: initial.into(),
        tint: Color::from_rgb8((tint >> 16) as u8, (tint >> 8) as u8, tint as u8),
        picture: None,
    }
}

fn dm(id: &str, name: &str, tint: u32, presence: Presence, detail: &str) -> DmRow {
    DmRow {
        id: id.into(),
        name: name.into(),
        avatar: avatar(&name[..1], tint),
        presence,
        detail: detail.into(),
        ..DmRow::default()
    }
}

/// The rail with the DM home selected and three servers, the first unread.
pub fn rail() -> Rail {
    let server = |id: &str, name: &str, initials: &str, unread: bool| Server {
        id: id.into(),
        name: name.into(),
        avatar: avatar(initials, 0x5865f2),
        tile: Tile {
            unread,
            ..Tile::default()
        },
        call: false,
    };
    Rail {
        home: Tile {
            selected: true,
            badge: "1".into(),
            ..Tile::default()
        },
        servers: vec![
            server("discord:g1", "Server", "S", true),
            server("discord:g2", "F2", "F2", false),
            server("discord:g3", "F3", "F3", false),
        ],
    }
}

/// The DM list, Person 5 open.
pub fn dms() -> Sidebar {
    let mut rows = vec![
        dm(
            "discord:d1",
            "Bob",
            0xc2554d,
            Presence::DoNotDisturb,
            &tr!("Playing {}", "Factorio"),
        ),
        dm(
            "discord:d2",
            "Weekend plans",
            0xb86e22,
            Presence::None,
            &trn!("{n} member", "{n} members", 3),
        ),
    ];
    for person in (0..6).rev() {
        let name = format!("Person {person}");
        rows.push(dm(
            &format!("discord:p{person}"),
            &name,
            0x3a9357,
            Presence::Offline,
            "",
        ));
    }
    rows[2].selected = true;
    rows[0].unread = true;
    Sidebar {
        place: Place::Dms(Dms {
            requests_badge: "1".into(),
            rows,
            ..Dms::default()
        }),
        filter: String::new(),
        version: 1,
    }
}

/// The channel list of the first server, #general open.
pub fn channels() -> Sidebar {
    let channel = |id: &str, name: &str, heading: &str, kind| ChannelRow {
        id: id.into(),
        name: name.into(),
        heading: heading.into(),
        kind,
        ..ChannelRow::default()
    };
    let mut rows = vec![
        channel("discord:c1", "general", "Channels", ChannelKind::Text),
        channel("discord:c2", "Plans", "", ChannelKind::Thread),
        channel("discord:c3", "ideas", "", ChannelKind::Forum),
        channel("discord:c4", "Lounge", "", ChannelKind::Voice),
    ];
    rows[0].selected = true;
    rows[2].unread = true;
    Sidebar {
        place: Place::Server(crate::sidebar::Server {
            name: "Server".into(),
            rows,
        }),
        filter: String::new(),
        version: 1,
    }
}

pub fn account() -> account::Account {
    account::Account {
        name: "Alice".into(),
        line: "Testing Yapfast".into(),
        avatar: avatar("A", 0x7a5fc9),
        presence: Presence::Online,
        muted: false,
        deafened: false,
    }
}

/// The whole window on the DM home, Person 5's DM open.
pub fn dm_home() -> Vm {
    Vm {
        shell: shell::Shell {
            connecting: false,
            ..shell::Shell::default()
        },
        rail: rail(),
        sidebar: dms(),
        account: account(),
        conversation: conversation::Conversation {
            header: conversation::Header::Direct {
                name: "Person 5".into(),
                avatar: avatar("P", 0x3a9357),
            },
        },
    }
}

/// The whole window on the first server, #general open.
pub fn server_home() -> Vm {
    let mut vm = dm_home();
    vm.rail.home.selected = false;
    vm.rail.servers[0].tile.selected = true;
    vm.sidebar = channels();
    vm.conversation.header = conversation::Header::Channel {
        name: "general".into(),
        topic: "Co-op nights, plans and links.".into(),
    };
    vm
}

/// The window before the first sync: nothing listed yet.
pub fn connecting() -> Vm {
    Vm {
        account: account(),
        ..Vm::default()
    }
}
