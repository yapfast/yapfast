//! Samples of the server, tag and role cards for tests and shots, after the
//! fake Discord's data: task P12's. Server is the fake's with its icon and
//! banner; the tag card is Bob's tag from `fixtures::kind`; the roles are the
//! member list's (Mod, in the blue the fake gives it).

use iced::{Color, Point, Rectangle, Size};
use yapfast_lang::{tr, trn};

use crate::Vm;
use crate::fixtures::pictures::{icon, picture, pictured};
use crate::fixtures::{avatar, kind::bob_tag, shell};
use crate::kind::{Expected, Tag};
use crate::menu::Spot;
use crate::people::cards::{Cards, Member, Role, Server, ServerTag};
use crate::shell::Layer;

/// The fake's server, with its picture and banner and a nickname.
pub fn server() -> Server {
    Server {
        id: "discord:g1".into(),
        name: "Server".into(),
        initial: "S".into(),
        icon: Expected::Ready(icon("1400000000000000100", "server1")),
        banner: Expected::Ready(picture("/banners/1400000000000000100/banner1.png")),
        members: trn!("{n} member", "{n} members", 31),
        channels: trn!("{n} channel", "{n} channels", 4),
        nick: "Bobby".into(),
    }
}

/// A server with no picture, no banner and no member count: its initial on
/// the raised ground.
pub fn server_plain() -> Server {
    Server {
        id: "discord:g2".into(),
        name: "Filler 2".into(),
        initial: "F".into(),
        channels: trn!("{n} channel", "{n} channels", 1),
        ..Server::default()
    }
}

/// Bob's tag, whose server Discord described: this account is in it.
pub fn tag() -> ServerTag {
    ServerTag {
        server: "discord:g1".into(),
        name: "Server".into(),
        initial: "S".into(),
        icon: Expected::Ready(icon("1400000000000000100", "server1")),
        tag: Tag {
            badge: Expected::Ready(picture("/clan-badges/1400000000000000100/bobtag.png")),
            ..bob_tag()
        },
        online: tr!("{} Online", "1,200"),
        members: trn!("{} Member", "{} Members", 4644, "4,644"),
        established: tr!("Est. {}", "Nov 2026"),
        description: "A small server for the people who make things, and for those who watch them \
             being made. Come for the projects, stay for the long threads about keyboards \
             and the weekly screenshot swap that goes on and on."
            .into(),
        joined: true,
        ..ServerTag::default()
    }
}

/// Discord hasn't answered: skeletons where its answer goes.
pub fn tag_loading() -> ServerTag {
    ServerTag {
        name: String::new(),
        icon: Expected::None,
        initial: "B".into(),
        online: String::new(),
        members: String::new(),
        established: String::new(),
        description: String::new(),
        loading: true,
        joined: false,
        ..tag()
    }
}

/// A server that shows its profile only to its members.
pub fn tag_limited() -> ServerTag {
    ServerTag {
        name: tr!("Private Server").into(),
        icon: Expected::None,
        initial: "P".into(),
        online: String::new(),
        members: String::new(),
        established: String::new(),
        description: String::new(),
        note: tr!("The server has limited who can see this profile.").into(),
        limited: true,
        joined: false,
        ..tag()
    }
}

/// A profile that failed to load: its note tries again.
pub fn tag_failed() -> ServerTag {
    ServerTag {
        name: tr!("Unknown Server").into(),
        icon: Expected::None,
        initial: "U".into(),
        online: String::new(),
        members: String::new(),
        established: String::new(),
        description: String::new(),
        note: tr!("Unable to load server profile").into(),
        failed: true,
        joined: false,
        ..tag()
    }
}

/// A server this account isn't in: nothing opens it.
pub fn tag_stranger() -> ServerTag {
    ServerTag {
        joined: false,
        ..tag()
    }
}

fn blue() -> Color {
    Color::from_rgb8(0x3a, 0x6e, 0xd8)
}

fn member(id: u64, name: &str, tint: u32, color: Option<Color>) -> Member {
    Member {
        id: format!("{}", 1_750_000_000_000_000_000 + id),
        name: name.into(),
        avatar: pictured(avatar(&name[..1], tint), name),
        color,
    }
}

/// The Mod role: Bob and four Members, as the member list's group has them.
pub fn role() -> Role {
    Role {
        name: "Mod".into(),
        color: Some(blue()),
        note: String::new(),
        members: vec![
            member(0, "Bob", 0xc2554d, Some(blue())),
            member(1, "Member 1", 0x3a9357, None),
            member(2, "Member 2", 0x4a5bd0, Some(blue())),
            member(3, "Member 3", 0xc2554d, None),
            member(4, "Member 4", 0x3a9357, None),
        ],
        limit: 800.0,
    }
}

/// A role without a colour, as `@everyone`'s sort: the muted dot and the
/// plain ink.
pub fn role_plain() -> Role {
    Role {
        name: "Regulars".into(),
        color: None,
        ..role()
    }
}

/// Asked for, not answered yet.
pub fn role_loading() -> Role {
    Role {
        note: tr!("Loading...").into(),
        members: Vec::new(),
        ..role()
    }
}

/// Answered: no one known to have it.
pub fn role_empty() -> Role {
    Role {
        note: tr!("Yapfast hasn't seen anyone with this role yet.").into(),
        members: Vec::new(),
        ..role()
    }
}

/// Hundreds of members: the list scrolls under its cap of 320 px.
pub fn role_many() -> Role {
    Role {
        members: (0..400)
            .map(|n| member(n, &format!("Member {n}"), 0x3a9357, None))
            .collect(),
        ..role()
    }
}

/// The cards with one of them open, as the app fills them.
pub fn showing_server(server: Server) -> Cards {
    Cards {
        server: Some(server),
        ..Cards::default()
    }
}

pub fn showing_tag(tag: ServerTag) -> Cards {
    Cards {
        tag: Some(tag),
        ..Cards::default()
    }
}

pub fn showing_role(role: Role) -> Cards {
    Cards {
        role: Some(role),
        ..Cards::default()
    }
}

/// The row a card opens beside: the member list's third, as the shots'
/// profile cards open.
pub fn beside() -> Spot {
    Spot::beside(Rectangle::new(
        Point::new(1135.0, 200.0),
        Size::new(265.0, 42.0),
    ))
}

/// The window with a server card over it.
pub fn server_over(cards: Cards) -> Vm {
    let mut vm = shell::open_at(shell::server_home(), Layer::Card, beside());
    vm.people.cards = cards;
    vm
}

/// The window with a role card over it.
pub fn role_over(cards: Cards) -> Vm {
    let mut vm = shell::open_at(shell::server_home(), Layer::RoleCard, beside());
    vm.people.cards = cards;
    vm
}
