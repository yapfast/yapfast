//! Samples of the context menu open over the window, after the menus of the
//! golden screenshots (`menu/`, `header/notifications.png`): the server
//! name's dropdown, a channel's bell, and a message's menu with its Apps
//! submenu. Each is the whole window ([`super::shell::server_home`]) with
//! `vm.menu` open where the Slint window opened it. Rows send what
//! `src/ui/menus.rs` reads by id ([`Choice`]). Task F06's.

use iced::{Point, Rectangle, Size};
use yapfast_lang::{tr, trn};

use crate::Vm;
use crate::menu::{Choice, Menu, Row, Spot};

use super::shell;

/// The server header's button in the golden window: its name, 8 px in.
pub const HEADER: Rectangle = Rectangle {
    x: 80.0,
    y: 8.0,
    width: 248.0,
    height: 32.0,
};

/// The open channel's bell in the conversation's header.
pub const BELL: Rectangle = Rectangle {
    x: 1018.0,
    y: 9.0,
    width: 32.0,
    height: 32.0,
};

/// Where `menu/message_context.png` right-clicked.
pub const MESSAGE_POINT: Point = Point::new(700.0, 300.0);

/// The first server's id, as the rail's sample has it.
const SERVER: &str = "discord:g1";
/// `#general`'s room id.
const CHANNEL: &str = "discord:c1";
/// A message of `#general`, as the timeline's samples name them.
const MESSAGE: &str = "d:m1";

/// The window with a menu open.
fn open(at: Spot, rows: Vec<Row>) -> Vm {
    let mut vm = shell::server_home();
    vm.menu = Menu::new(1, at, rows);
    vm
}

/// A row of a menu about `target`, choosing `id`.
fn row(text: &str, target: &str, id: &str) -> Row {
    Row::new(text, Choice::new(target, id).into())
}

/// The levels a notification setting offers, `current` checked.
fn levels(target: &str, current: usize) -> Vec<Row> {
    [
        (tr!("All messages"), "level-all"),
        (tr!("Only @mentions"), "level-mentions"),
        (tr!("Nothing"), "level-nothing"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (text, id))| row(text, target, id).checked(index == current))
    .collect()
}

/// The server name's dropdown: `menu/server_header.png`.
pub fn server_header() -> Vm {
    let target = format!("header:{SERVER}");
    let target = target.as_str();
    let mut notifications = levels(target, 1);
    notifications.extend([
        row(
            tr!("Suppress @everyone and @here"),
            target,
            "suppress-everyone",
        )
        .checkbox(false)
        .separated(),
        row(tr!("Suppress all role @mentions"), target, "suppress-roles").checkbox(false),
    ]);
    let rows = vec![
        row(tr!("Invite to server"), target, "invite"),
        row(tr!("Server settings"), target, "server-settings").separated(),
        row(tr!("Create channel"), target, "create-channel"),
        row(tr!("Create category"), target, "create-category"),
        Row::opens(tr!("Notification settings"), notifications)
            .subtitle(tr!("Only @mentions"))
            .separated(),
        row(tr!("Edit server profile"), target, "edit-server-profile").separated(),
        row(tr!("Hide muted channels"), target, "hide-muted")
            .checkbox(false)
            .separated(),
        row(tr!("Leave server"), target, "leave-server")
            .danger()
            .separated(),
    ];
    open(
        Spot::below(HEADER),
        rows.into_iter().map(Row::plain).collect(),
    )
}

/// How long to mute for: the bell's submenu (`mute_times`).
fn mute_times(target: &str) -> Vec<Row> {
    let minutes = [15, 60, 3 * 60, 8 * 60, 24 * 60];
    minutes
        .into_iter()
        .map(|minutes| {
            let text = if minutes % 60 == 0 {
                trn!("For {n} hour", "For {n} hours", minutes / 60)
            } else {
                trn!("For {n} minute", "For {n} minutes", minutes)
            };
            row(&text, target, &format!("mute-{minutes}"))
        })
        .chain([row(tr!("Until I turn it back on"), target, "mute-always")])
        .collect()
}

/// A channel's bell: `header/notifications.png`. Muting, which mutes until
/// turned back on when clicked and says for how long on hover, then the
/// levels, the category's checked.
pub fn notifications() -> Vm {
    let target = format!("notifications:{CHANNEL}");
    let target = target.as_str();
    let mute = row(tr!("Mute channel"), target, "mute-always")
        .icon("bell-off")
        .submenu(mute_times(target));
    let mut rows = vec![
        mute,
        row(tr!("Use category default"), target, "level-default")
            .subtitle(tr!("Only @mentions"))
            .checked(true)
            .separated(),
    ];
    rows.extend(levels(target, usize::MAX));
    open(Spot::below(BELL), rows)
}

/// A message's menu at the pointer: `menu/message_context.png`, without
/// the quick reactions over it, which are the message menu's own (C12).
/// Hovering Apps opens `menu/apps_submenu.png`'s submenu.
pub fn message_context() -> Vm {
    let rows = vec![
        row(tr!("Add reaction"), MESSAGE, "react").icon("smile-plus"),
        row(tr!("Reply"), MESSAGE, "reply").icon("reply"),
        row(tr!("Forward"), MESSAGE, "forward").icon("forward"),
        row(tr!("Create Thread"), MESSAGE, "thread").icon("thread"),
        row(tr!("Copy text"), MESSAGE, "copy-text").icon("copy"),
        row(tr!("Pin message"), MESSAGE, "pin").icon("pin"),
        Row::opens(
            tr!("Apps"),
            vec![
                row("Quote", MESSAGE, "app:quote")
                    .subtitle("Status Bot")
                    .plain(),
            ],
        )
        .icon("slash"),
        row(tr!("Mark Unread"), MESSAGE, "mark-unread").icon("message-circle"),
        row(tr!("Copy Message Link"), MESSAGE, "copy-link").icon("link"),
        row(tr!("Copy Message ID"), MESSAGE, "copy-id")
            .icon("hash")
            .separated(),
    ];
    open(Spot::pointer(MESSAGE_POINT), rows)
}

/// A menu taller than a small window, which scrolls.
pub fn long() -> Vm {
    let rows = (1..=40)
        .map(|n| row(&format!("Row {n}"), SERVER, &format!("row-{n}")))
        .collect();
    open(Spot::pointer(Point::new(400.0, 200.0)), rows)
}

/// The window size the long menu overflows.
pub const LONG_WINDOW: Size = Size::new(1000.0, 640.0);
