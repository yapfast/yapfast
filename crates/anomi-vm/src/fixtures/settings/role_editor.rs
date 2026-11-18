//! Samples of a server's role editor for tests and shots, after the fake
//! Discord's data: task X09's.
//!
//! The golden screenshot opens Leads, which Alice may not change; the
//! others open Mods, which she may, with its icon.

use iced::Color;

use crate::Vm;
use crate::fixtures::pictures;
use crate::fixtures::settings::roles::{self, LEADS, MODS};
use crate::fixtures::settings::server::server;
use crate::fixtures::{avatar, shell};
use crate::kind::Expected;
use crate::settings::role_editor::{Adding, Confirm, Member, Permission, RoleEditor, Swatch, Tab};
use crate::settings::server::{Section, Server};
use crate::shell::Layer;
use yapfast_lang::tr;

/// Discord's twenty colours (`ROLE_COLORS`).
const COLORS: [u32; 20] = [
    0x1abc9c, 0x2ecc71, 0x3498db, 0x9b59b6, 0xe91e63, 0xf1c40f, 0xe67e22, 0xe74c3c, 0x95a5a6,
    0x607d8b, 0x11806a, 0x1f8b4c, 0x206694, 0x71368a, 0xad1457, 0xc27c0e, 0xa84300, 0x992d22,
    0x979c9f, 0x546e7a,
];

fn rgb(value: u32) -> Color {
    Color::from_rgb8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

/// The swatches of a row of ten, `chosen` of the twenty ticked.
fn swatches(row: usize, chosen: Option<usize>) -> Vec<Swatch> {
    COLORS[row * 10..row * 10 + 10]
        .iter()
        .enumerate()
        .map(|(at, &value)| Swatch {
            color: rgb(value),
            chosen: chosen == Some(row * 10 + at),
        })
        .collect()
}

fn permission(title: &str, description: &str, bit: u32, on: bool) -> Permission {
    Permission {
        title: title.into(),
        description: description.into(),
        bit,
        on,
        ..Permission::default()
    }
}

fn heading(title: &str) -> Permission {
    Permission {
        heading: true,
        title: title.into(),
        ..Permission::default()
    }
}

/// A few of Discord's permissions in their groups, one with a warning and
/// one Alice may not change.
pub fn permissions() -> Vec<Permission> {
    let mut administrator = permission(
        "Administrator",
        "Members with this permission have every permission and bypass channel permissions.",
        3,
        false,
    );
    administrator.warning = "This is a dangerous permission to grant.".into();
    let mut manage = permission(
        "Manage Roles",
        "Members can create and edit roles under theirs.",
        28,
        true,
    );
    manage.lock = tr!("You can't change a permission you don't have yourself.").into();
    vec![
        heading("General Server Permissions"),
        administrator,
        permission(
            "View Channels",
            "Members can view channels by default.",
            10,
            true,
        ),
        manage,
        heading("Membership Permissions"),
        permission(
            "Create Invite",
            "Members can invite new people to this server.",
            0,
            true,
        ),
        permission(
            "Kick Members",
            "Members can remove other members from this server.",
            1,
            false,
        ),
    ]
}

/// The members who hold the role.
pub fn members() -> Vec<Member> {
    let who = |id: &str, name: &str, username: &str, tint: u32| Member {
        id: id.into(),
        name: name.into(),
        username: username.into(),
        avatar: pictures::pictured(avatar(&name[..1], tint), name),
        chosen: false,
    };
    vec![
        who("1400000000000000201", "Alice", "alice", 0xe0807a),
        who("1400000000000000202", "Bob", "bob_2", 0x5865f2),
        who("1400000000000000203", "Carol", "carol", 0x3ba55d),
    ]
}

/// Leads, as the golden screenshot has it: locked, with its notice, the
/// grey chosen, its icon waiting for Level 2 and one member.
pub fn locked() -> RoleEditor {
    RoleEditor {
        id: LEADS.into(),
        locked: true,
        notice: tr!("Role is locked because it is a higher rank than your highest role.").into(),
        name: "Leads".into(),
        default_color: true,
        bright: swatches(0, None),
        dark: swatches(1, None),
        color: roles_grey(),
        hex: "#99aab5".into(),
        icon_locked: tr!("Unlocks at Level {}", 2),
        members_title: tr!("Manage Members ({})", 1),
        permissions: permissions(),
        members: members()[..1].to_vec(),
        ..RoleEditor::default()
    }
}

fn roles_grey() -> Color {
    crate::settings::roles::DEFAULT_COLOR
}

/// Mods, which Alice may change: the blue chosen, an icon, both switches.
pub fn editable() -> RoleEditor {
    RoleEditor {
        id: MODS.into(),
        notice: String::new(),
        locked: false,
        name: "Mods".into(),
        default_color: false,
        bright: swatches(0, Some(2)),
        dark: swatches(1, Some(2)),
        color: rgb(COLORS[2]),
        hex: "#3498db".into(),
        hoist: true,
        icon: Expected::Ready(pictures::picture(&format!(
            "role-icons/{MODS}/5f4dcc3b5aa765d61d8327deb882cf99.png"
        ))),
        has_icon: true,
        icon_locked: String::new(),
        members_title: tr!("Manage Members ({})", 3),
        may_clear: true,
        may_add: true,
        members: members(),
        ..locked()
    }
}

/// A colour of none of Discord's, with the emoji for the icon.
pub fn custom() -> RoleEditor {
    RoleEditor {
        bright: swatches(0, None),
        dark: swatches(1, None),
        color: rgb(0x12ab34),
        hex: "#12ab34".into(),
        custom_color: true,
        icon: Expected::None,
        has_icon: false,
        emoji: "🛠️".into(),
        icon_locked: tr!("Unlocks at Level {}", 2),
        ..editable()
    }
}

/// The window with the server's settings open on an editor over `editor`,
/// on `tab`.
pub fn window(editor: RoleEditor, tab: Tab) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ServerSettings);
    vm.settings.roles = roles::roles();
    vm.settings.server = Server {
        section: Section::Roles,
        title: Some(tr!("Edit Role — {}", editor.name)),
        fill: tab != Tab::Display,
        ..server()
    };
    vm.settings.role_editor = RoleEditor { tab, ..editor };
    vm
}

/// The window as the golden screenshot shows it.
pub fn open() -> Vm {
    window(locked(), Tab::Display)
}

/// Delete Role's question over Mods.
pub fn confirm_delete() -> Vm {
    let mut vm = window(editable(), Tab::Display);
    vm.settings.role_editor.confirm = Some(Confirm::Delete {
        role: "Mods".into(),
    });
    vm
}

/// Remove member's question.
pub fn confirm_remove() -> Vm {
    let mut vm = window(editable(), Tab::Members);
    vm.settings.role_editor.confirm = Some(Confirm::Remove {
        role: "Mods".into(),
        member: "Bob".into(),
    });
    vm
}

/// Add Members with two of the server's people picked.
pub fn adding() -> Adding {
    let mut candidates = members();
    candidates[1].chosen = true;
    Adding {
        role: "Mods".into(),
        search: String::new(),
        candidates,
    }
}
