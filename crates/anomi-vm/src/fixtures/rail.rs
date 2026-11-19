//! Samples of the rail, after the golden screenshots' (`server/`, `voice/`,
//! `menu/`): the DM home and three servers, unread DMs above them, a call
//! going on, folders, and a server's menu. Task N02's. A folder's menu
//! and its settings, after the owner's references of Discord's
//! (`discord-folder-menu-move-submenu.png`,
//! `discord-folder-settings-dialog.png`).

use iced::{Color, Point, Rectangle, Size};
use yapfast_lang::tr;

use crate::Vm;
use crate::kind::Avatar;
use crate::menu::{Choice, Menu, Row, Spot};
use crate::rail::folder::FolderSettings;
use crate::rail::{Dm, Folder, Rail, Server, Tile};
use crate::shell::Layer;

use super::avatar;
use super::pictures;
use super::settings::role_editor::COLORS;
use super::shell;

fn plain(id: &str, name: &str, initials: &str, unread: bool) -> Server {
    Server {
        id: id.into(),
        name: name.into(),
        avatar: Avatar {
            picture: pictures::server(name),
            ..avatar(initials, 0x5865f2)
        },
        tile: Tile {
            unread,
            ..Tile::default()
        },
        movable: true,
        ..Server::default()
    }
}

/// The rail with the DM home selected and three servers, the first unread.
pub fn rail() -> Rail {
    Rail {
        home: Tile {
            selected: true,
            unread: true,
            badge: "1".into(),
        },
        servers: vec![
            plain("discord:g1", "Server", "S", true),
            plain("discord:g2", "F2", "F2", false),
            plain("discord:g3", "F3", "F3", false),
        ],
        ..Rail::default()
    }
}

/// `rail()` with the first server selected, two DM mentions waiting, and
/// the unread DM from Discord's system user above the servers (`menu/server_icon_context.png`).
pub fn with_dms() -> Rail {
    let mut rail = rail();
    rail.home = Tile {
        unread: true,
        badge: "2".into(),
        ..Tile::default()
    };
    rail.dms = vec![Dm {
        id: "discord:d6".into(),
        name: "Discord".into(),
        avatar: Avatar {
            picture: Some(pictures::picture("/embed/avatars/5.png")),
            ..avatar("D", 0x5865f2)
        },
        badge: "1".into(),
        discord: false,
    }];
    rail.servers[0].tile.selected = true;
    rail
}

/// A call going on in the first server (`voice/joined_sidebar_dock.png`).
pub fn call() -> Rail {
    let mut rail = with_dms();
    rail.servers[0].call = true;
    rail
}

/// Folders: one open with two servers, one closed with four and a call,
/// then a Discord server out of reach, and one with a count.
pub fn folders() -> Rail {
    let mut rail = with_dms();
    let in_folder = |mut server: Server, folder: &str| {
        server.folder = Some(folder.into());
        server
    };
    rail.servers = vec![
        Server {
            discord: true,
            ..plain("discord:g1", "Server", "S", true)
        },
        in_folder(plain("discord:g2", "F2", "F2", false), "f1"),
        in_folder(plain("discord:g3", "F3", "F3", true), "f1"),
        in_folder(plain("discord:g4", "Alpha", "A", false), "f2"),
        in_folder(plain("discord:g5", "Beta", "B", false), "f2"),
        in_folder(plain("discord:g6", "Gamma", "G", false), "f2"),
        in_folder(plain("discord:g7", "Delta", "D", false), "f2"),
        Server {
            unavailable: true,
            tip: tr!("{} (unavailable in a Discord outage)", "Epsilon"),
            discord: true,
            ..plain("discord:g8", "Epsilon", "E", false)
        },
    ];
    rail.servers[0].tile.badge = "3".into();
    rail.servers[0].tile.selected = true;
    rail.folders = vec![
        Folder {
            id: "f1".into(),
            name: "F2, F3".into(),
            open: true,
            tint: Color::from_rgb8(0xed, 0x42, 0x45),
            discord: true,
            ..Folder::default()
        },
        Folder {
            id: "f2".into(),
            name: "Alpha, Beta, Gamma, …".into(),
            tint: Color::from_rgb8(0x23, 0xa5, 0x5a),
            tile: Tile {
                unread: true,
                badge: "12".into(),
                ..Tile::default()
            },
            call: true,
            discord: true,
            ..Folder::default()
        },
    ];
    rail
}

/// The first server's window with `rail` on it.
fn window(rail: Rail) -> Vm {
    let mut vm = shell::server_home();
    vm.rail = rail;
    vm
}

/// The window with unread DMs above the servers.
pub fn unread_dms_window() -> Vm {
    window(with_dms())
}

/// The window with a call's speaker on the first server.
pub fn call_window() -> Vm {
    window(call())
}

/// The window with folders on the rail.
pub fn folders_window() -> Vm {
    window(folders())
}

/// A server's menu, as the rail opens it at the pointer: the first
/// server's, over its tile.
pub fn server_menu() -> Vm {
    let target = "discord:g1";
    let row = |text: &str, id: &str| Row::new(text, Choice::new(target, id).into());
    let rows = vec![
        row(tr!("Mark as read"), "mark-read").disabled(true),
        row(tr!("Invite to server"), "invite").separated(),
        Row::opens(
            tr!("Mute server"),
            vec![row(tr!("Until I turn it back on"), "mute-always")],
        )
        .separated(),
        Row::opens(
            tr!("Notification settings"),
            vec![row(tr!("Only @mentions"), "level-mentions")],
        )
        .subtitle(tr!("Only @mentions")),
        row(tr!("Hide muted channels"), "hide-muted").checkbox(false),
        row(tr!("Server settings"), "server-settings").separated(),
        row(tr!("Edit server profile"), "edit-server-profile"),
        row(tr!("Create channel"), "create-channel"),
        row(tr!("Create category"), "create-category"),
        row(tr!("Leave server"), "leave-server")
            .danger()
            .separated(),
        row(tr!("Copy link"), "copy-link").separated(),
    ];
    let mut vm = window(with_dms());
    let rows = rows.into_iter().map(Row::plain).collect();
    vm.menu = Menu::new(1, Spot::pointer(iced::Point::new(36.0, 142.0)), rows);
    vm
}

/// The closed folder's menu, as the rail opens it at the pointer over its
/// tile, with Move's submenu showing: its servers unread, the other
/// folder open.
pub fn folder_menu() -> Vm {
    let target = "f2";
    let row = |text: &str, id: &str| Row::new(text, Choice::new(target, id).into()).plain();
    let ends = vec![
        row(tr!("Top of list"), "folder-top"),
        row(tr!("Bottom of list"), "folder-bottom"),
    ];
    let rows = vec![
        row(tr!("Mark folder as read"), "mark-read"),
        Row::opens(tr!("Move…"), ends).plain().separated(),
        row(tr!("Folder settings"), "folder-settings").separated(),
        row(tr!("Close all folders"), "close-folders"),
    ];
    let mut vm = folders_window();
    vm.menu = Menu {
        submenu: Some(1),
        ..Menu::new(1, Spot::pointer(Point::new(36.0, 410.0)), rows)
    };
    vm
}

/// The folders' window with a folder's settings open on `settings`.
fn with_settings(settings: FolderSettings) -> Vm {
    let mut vm = folders_window();
    vm.rail.settings = settings;
    shell::open(vm, Layer::FolderSettings)
}

/// The open folder's settings: named "Games", in Discord's red, which is
/// none of the twenty, so the custom swatch has it.
fn games() -> FolderSettings {
    FolderSettings::opened("f1", "Games", "F2, F3", Some(0xed4245), &COLORS)
}

/// A folder's settings named and coloured.
pub fn folder_settings_named() -> Vm {
    with_settings(games())
}

/// A folder's settings with no name, its servers' as the placeholder, and
/// the default colour.
pub fn folder_settings_untouched() -> Vm {
    let settings = FolderSettings::opened("f2", "", "Alpha, Beta, Gamma, …", None, &COLORS);
    with_settings(settings)
}

/// The named folder's settings with the custom colour's picker open under
/// its swatch, which sits there in a window of the shots' size.
pub fn folder_settings_picker() -> Vm {
    let mut settings = games();
    settings.open_custom();
    let swatch = Rectangle::new(Point::new(564.0, 421.0), Size::new(70.0, 50.0));
    shell::open_at(
        with_settings(settings),
        Layer::FolderColour,
        Spot::below(swatch),
    )
}
