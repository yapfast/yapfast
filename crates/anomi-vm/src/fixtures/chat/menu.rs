//! Samples of the message menu for tests and shots, after the fake
//! Discord's data: task C12's. The menu is `vm.chat.menu`, open as
//! `Layer::MessageMenu` at the pointer over #general's history, as
//! `menu/message_context.png` and `menu/apps_submenu.png` have it.

use iced::Point;

use crate::Vm;
use crate::chat::emoji_picker::Emoji;
use crate::chat::menu::{App, Facts, Media, Menu};
use crate::fixtures::chat::message::history;
use crate::fixtures::chat::timeline::message_key;
use crate::fixtures::shell;
use crate::menu::Spot;
use crate::shell::Layer;

/// Where `menu/message_context.png` right-clicked.
pub const POINTER: Point = Point::new(700.0, 300.0);

/// The four quick reactions this device uses most.
pub fn reactions() -> Vec<Emoji> {
    ["👍", "❤️", "😂", "🎉"]
        .into_iter()
        .filter_map(Emoji::unicode)
        .collect()
}

/// What the golden menu has: a Discord message of someone else's, in a
/// channel with threads and apps, whose text can be copied and which can be
/// pinned or marked unread.
pub fn facts() -> Facts {
    Facts {
        can_react: true,
        can_reply: true,
        can_forward: true,
        can_pin: true,
        can_start_thread: true,
        composer_enabled: true,
        forward_allowed: true,
        delivered: true,
        body: "Anyone up for the weekend trip?".into(),
        can_mark_unread: true,
        link: "https://discord.com/channels/1/2/3".into(),
        id: "1100000000000000003".into(),
        ..Facts::default()
    }
}

/// The apps' message commands.
pub fn apps() -> Vec<App> {
    vec![App {
        id: "quote".into(),
        name: "Quote".into(),
        app: "Status Bot".into(),
    }]
}

/// The menu's data before it is open: the facts, tiles and apps.
pub fn menu() -> Menu {
    Menu {
        key: message_key(46),
        facts: facts(),
        reactions: reactions(),
        apps: apps(),
        ..Menu::default()
    }
}

fn open(menu: Menu) -> Vm {
    let mut vm = shell::open_at(
        shell::server_home(),
        Layer::MessageMenu,
        Spot::pointer(POINTER),
    );
    vm.chat.timeline = history();
    vm.chat.menu = menu;
    vm
}

/// Bob's message with its menu, `menu/message_context.png`. The window
/// holds the facts; the rows, which carry the text, are
/// `anomi_chat::menu::build`'s, made as the app opens the menu.
pub fn message_context() -> Vm {
    open(menu())
}

/// `menu/apps_submenu.png`: the pointer on Apps.
pub fn apps_submenu() -> Vm {
    open(Menu {
        apps_open: true,
        ..menu()
    })
}

/// The Apps submenu where the menu has no room on its right: on its left.
pub fn apps_flipped() -> Vm {
    let mut vm = apps_submenu();
    vm.chat.menu.flip = true;
    vm
}

/// Your own message: edit and delete too, and the reactions to see.
pub fn own() -> Vm {
    open(Menu {
        facts: Facts {
            can_edit: true,
            can_delete: true,
            has_reactions: true,
            ..facts()
        },
        ..menu()
    })
}

/// A picture right-clicked: save and copy it, its link.
pub fn picture() -> Vm {
    open(Menu {
        facts: Facts {
            media: Some(Media {
                index: 0,
                video: false,
            }),
            address: "https://cdn.example/sunset.png".into(),
            body: String::new(),
            ..facts()
        },
        apps: Vec::new(),
        ..menu()
    })
}

/// A video: no copy of it as a picture.
pub fn video() -> Vm {
    let mut vm = picture();
    vm.chat.menu.facts.media = Some(Media {
        index: 1,
        video: true,
    });
    vm
}

/// A message that is on its way: nothing to pin, thread or run.
pub fn sending() -> Vm {
    open(Menu {
        facts: Facts {
            delivered: false,
            link: String::new(),
            id: String::new(),
            can_mark_unread: false,
            ..facts()
        },
        apps: Vec::new(),
        ..menu()
    })
}

/// A poll this account made in a thread, pinned, with text selected.
pub fn poll() -> Vm {
    open(Menu {
        facts: Facts {
            can_end_poll: true,
            thread: Some("discord:t1".into()),
            pinned: true,
            selection: "weekend".into(),
            ..facts()
        },
        ..menu()
    })
}
