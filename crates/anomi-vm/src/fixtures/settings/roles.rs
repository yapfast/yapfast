//! Samples of a server's role list for tests and shots, after the fake
//! Discord's data: task X08's.
//!
//! "Server"'s roles as Alice sees them (`SERVER_ROLES`,
//! `testing/fake_discord.ts:2307`): Leads above her highest role, Staff
//! that one, both locked; Crew, Bots and Mods hers to move, Mods with an
//! icon as the golden screenshot has it.

use crate::Vm;
use crate::fixtures::pictures;
use crate::fixtures::settings::server::server;
use crate::fixtures::shell;
use crate::kind::Expected;
use crate::settings::roles::{Role, Roles};
use crate::settings::server::{Section, Server};
use crate::shell::Layer;
use yapfast_lang::tr;

/// The fake's ids: the server's, which is @everyone's, and each role's.
pub const EVERYONE: &str = "1400000000000000100";
pub const MODS: &str = "1400000000000000110";
pub const BOTS: &str = "1400000000000000111";
pub const CREW: &str = "1400000000000000112";
pub const LEADS: &str = "1400000000000000113";
pub const STAFF: &str = "1400000000000000114";

fn role(id: &str, name: &str, members: &str) -> Role {
    Role {
        id: id.into(),
        name: name.into(),
        members: members.into(),
        may_copy: true,
        ..Role::default()
    }
}

fn locked(mut role: Role, lock: &str) -> Role {
    role.lock = lock.into();
    role.may_copy = false;
    role
}

/// The list as the golden screenshot has it: five roles, the first two
/// locked, Mods with its icon.
pub fn roles() -> Roles {
    let higher = tr!("Role is locked because it is a higher rank than your highest role.");
    let highest = tr!(
        "Role is locked because it is your highest ranked role. Please ask a higher rank or Server Owner for help."
    );
    let mods = Role {
        icon: Expected::Ready(pictures::picture(&format!(
            "role-icons/{MODS}/5f4dcc3b5aa765d61d8327deb882cf99.png"
        ))),
        ..role(MODS, "Mods", "5")
    };
    Roles {
        everyone: EVERYONE.into(),
        everyone_name: "@everyone".into(),
        rows: vec![
            locked(role(LEADS, "Leads", "1"), higher),
            locked(role(STAFF, "Staff", "1"), highest),
            role(CREW, "Crew", "1"),
            role(BOTS, "Bots", "0"),
            mods,
        ],
        may_drag: true,
        first_movable: 2,
        may_create: true,
        ..Roles::default()
    }
}

/// Every kind of mark and line: an emoji in the dot's place, a coloured
/// dot, an icon on its way, a role a bot holds, a long name.
pub fn marks() -> Roles {
    let mut roles = roles();
    roles.rows[2].emoji = "🛠️".into();
    roles.rows[3].color = iced::Color::from_rgb8(0x1a, 0xbc, 0x9c);
    roles.rows[3].badge = tr!("BOT").into();
    roles.rows[3].may_copy = false;
    roles.rows.push(Role {
        icon: Expected::Waiting,
        color: iced::Color::from_rgb8(0xe9, 0x1e, 0x63),
        ..role(
            "1400000000000000115",
            "Night shift moderators of the voice channels and the stage",
            "12",
        )
    });
    roles
}

/// A search that hides roles: nothing may be dragged.
pub fn searched() -> Roles {
    let mut roles = roles();
    roles.search = "s".into();
    roles
        .rows
        .retain(|role| role.name.to_lowercase().contains('s'));
    roles.may_drag = false;
    roles.first_movable = 2;
    roles
}

/// A search nothing matches.
pub fn none() -> Roles {
    Roles {
        search: "zz".into(),
        rows: Vec::new(),
        may_drag: false,
        first_movable: 0,
        ..roles()
    }
}

/// The window with the server's settings open on `roles`.
pub fn window(roles: Roles) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ServerSettings);
    vm.settings.server = Server {
        section: Section::Roles,
        ..server()
    };
    vm.settings.roles = roles;
    vm
}

/// The window with the server's settings open on Roles, as the golden
/// screenshot shows them.
pub fn open() -> Vm {
    window(roles())
}
