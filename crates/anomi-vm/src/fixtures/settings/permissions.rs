//! Samples of a channel's permissions for tests and shots, after the fake
//! Discord's data: task X14's.

use crate::Vm;
use crate::fixtures::pictures::pictured;
use crate::fixtures::{avatar, settings::channel};
use crate::settings::channel::{Channel, Page};
use crate::settings::permissions::{Adding, Confirm, Found, Line, Overwrite, Permissions, Sync};

fn heading(title: &str) -> Line {
    Line {
        heading: true,
        title: title.into(),
        ..Line::default()
    }
}

fn line(title: &str, description: &str, bit: u32, state: u8, lock: &str) -> Line {
    Line {
        title: title.into(),
        description: description.into(),
        bit,
        state,
        lock: lock.into(),
        ..Line::default()
    }
}

fn overwrite(id: &str, name: &str, member: bool, color: u32, selected: bool) -> Overwrite {
    Overwrite {
        id: id.into(),
        name: name.into(),
        member,
        color,
        selected,
    }
}

/// `#general`'s permissions as its `@everyone` has them, synced with the
/// category `Channels`: the account may edit all but two locked lines.
pub fn permissions() -> Permissions {
    let lock = "You can't change this permission: you don't have it in this channel.";
    Permissions {
        subtitle: "Use permissions to customize who can do what in this channel.".into(),
        sync: Sync::Synced,
        category: "Channels".into(),
        channel: "general".into(),
        private_title: "Private Channel".into(),
        private_text: "By making a channel private, only select members and roles will be able to view this channel."
        .into(),
        overwrites: vec![overwrite("1", "@everyone", false, 0, true)],
        lines: vec![
            heading("GENERAL CHANNEL PERMISSIONS"),
            line(
                "View Channel",
                "Allows members to view this channel by default.",
                10,
                1,
                "",
            ),
            line(
                "Manage Channel",
                "Allows members to change this channel's name and settings, and to delete it.",
                4,
                1,
                "",
            ),
            line(
                "Manage Permissions",
                "Allows members to change this channel's permissions.",
                28,
                1,
                lock,
            ),
            line(
                "Manage Webhooks",
                "Allows members to create, edit and delete webhooks in this channel.",
                29,
                1,
                lock,
            ),
            heading("MEMBERSHIP PERMISSIONS"),
            line(
                "Create Invite",
                "Allows members to invite new people to this server.",
                0,
                2,
                "",
            ),
            line(
                "Send Messages",
                "Allows members to send messages in this channel.",
                11,
                0,
                "",
            ),
        ],
        remove: "Remove @everyone".into(),
        shown_everyone: true,
        may_edit: true,
        ..Permissions::default()
    }
}

/// A role with an overwrite shown, the private switch on, the sync apart,
/// a member among the overwrites.
pub fn apart() -> Permissions {
    Permissions {
        sync: Sync::Apart,
        private: true,
        overwrites: vec![
            overwrite("1", "@everyone", false, 0, false),
            overwrite("2", "Moderators", false, 0x5865f2, true),
            overwrite("3", "Bob", true, 0, false),
        ],
        remove: "Remove Moderators".into(),
        shown_everyone: false,
        ..permissions()
    }
}

/// An account that may not edit: the lines' buttons rest.
pub fn locked() -> Permissions {
    Permissions {
        may_edit: false,
        private_lock: "You can't change this: you lack Manage Permissions.".into(),
        ..permissions()
    }
}

/// The draft on its way.
pub fn saving() -> Permissions {
    Permissions {
        saving: true,
        ..apart()
    }
}

/// Add's picker with two roles and two members found.
pub fn adding() -> Adding {
    let found = |id: &str, name: &str, username: &str| Found {
        id: id.into(),
        name: name.into(),
        username: username.into(),
        avatar: pictured(avatar(&name[..1], 0x5865f2), name),
    };
    Adding {
        query: String::new(),
        roles: vec![
            overwrite("10", "Moderators", false, 0x5865f2, false),
            overwrite("11", "Members", false, 0x2ecc71, false),
        ],
        members: vec![found("201", "Alice", "alice"), found("202", "Bob", "bob")],
    }
}

/// The permissions page of `#general` as a channel's settings.
pub fn page(permissions: Permissions) -> Channel {
    Channel {
        page: Page::Permissions,
        title: "Channel permissions".into(),
        permissions,
        ..channel::overview()
    }
}

/// The window with the channel's settings open on `permissions`.
pub fn asked(permissions: Permissions) -> Vm {
    channel::asked(page(permissions))
}

/// A question the page asks.
pub fn confirming(confirm: Confirm) -> Permissions {
    Permissions {
        confirm: Some(confirm),
        confirm_name: "Moderators".into(),
        ..apart()
    }
}
