//! The whole window's samples: the DM home, a server, connecting, and the
//! window's own states (layers, notes, panels). Each area's shot starts
//! from one of them and fills its own field; an area that opens over the
//! window opens its layer with [`open`]. Task N01's.

use iced::{Point, Rectangle, Size};

use crate::menu::Spot;
use crate::shell::{self, Files, Layer};
use crate::{Vm, conversation::Conversation};

use super::{account, chrome, conversation, header, menu, rail, sidebar};

/// The whole window on the DM home, Person 5's DM open with their profile
/// beside it.
pub fn dm_home() -> Vm {
    Vm {
        shell: shell::Shell {
            connecting: false,
            ..shell::Shell::default()
        },
        rail: rail::rail(),
        sidebar: sidebar::home(),
        account: account::account(),
        header: header::dm(),
        conversation: conversation::dm(),
        ..Vm::default()
    }
}

/// The whole window on the first server, #general open with its member
/// list beside it.
pub fn server_home() -> Vm {
    let mut vm = dm_home();
    vm.rail.home.selected = false;
    vm.rail.servers[0].tile.selected = true;
    vm.sidebar = sidebar::server();
    vm.header = header::channel();
    vm.conversation = conversation::channel();
    vm
}

/// The window before the first sync: nothing listed yet.
pub fn connecting() -> Vm {
    Vm {
        account: account::account(),
        ..Vm::default()
    }
}

/// No account signed in: the sign-in screen.
pub fn signed_out() -> Vm {
    let mut vm = connecting();
    vm.shell.signed_out = true;
    vm.shell.connecting = false;
    vm
}

/// `vm` with a layer open over it, where it opens by itself.
pub fn open(mut vm: Vm, layer: Layer) -> Vm {
    vm.shell.open(layer);
    vm
}

/// `vm` with a layer open from a spot.
pub fn open_at(mut vm: Vm, layer: Layer, at: Spot) -> Vm {
    vm.shell.open_at(layer, at);
    vm
}

/// The server's channel with its conversation as `conversation` makes it.
pub fn channel_with(conversation: fn() -> Conversation) -> Vm {
    Vm {
        conversation: conversation(),
        ..server_home()
    }
}

/// The DM with its conversation as `conversation` makes it.
pub fn dm_with(conversation: fn() -> Conversation) -> Vm {
    Vm {
        conversation: conversation(),
        ..dm_home()
    }
}

/// Starting a DM: a dialog over the window.
pub fn new_dm() -> Vm {
    open(dm_home(), Layer::NewDm)
}

/// The channel's pins, under the header's pin button at (1075, 25).
pub fn pins() -> Vm {
    let button = Rectangle::new(Point::new(1059.0, 9.0), Size::new(32.0, 32.0));
    open_at(server_home(), Layer::Pins, Spot::below(button))
}

/// The settings over the window.
pub fn settings() -> Vm {
    open(server_home(), Layer::Settings)
}

/// The zoom level a moment after it changed.
pub fn zoomed() -> Vm {
    let mut vm = server_home();
    vm.shell.zoom = Some(shell::zoom::note(1.1));
    vm
}

/// Files dragged over the window, bound for #general.
pub fn dropping() -> Vm {
    let mut vm = server_home();
    vm.shell.drop = Some(Files::Over {
        destination: "#general".into(),
        aside: false,
    });
    vm
}

/// A drop refused: too large for a server without boosts.
pub fn drop_refused() -> Vm {
    let mut vm = server_home();
    vm.shell.drop = Some(refused(false));
    vm
}

/// The same drop refused over the thread beside #general, 420 px wide: the
/// card stays over the thread.
pub fn drop_refused_beside() -> Vm {
    let mut vm = channel_with(conversation::thread_beside);
    vm.shell.drop = Some(refused(true));
    vm
}

fn refused(aside: bool) -> Files {
    Files::Refused {
        reason: yapfast_lang::tr!("Your files are too powerful").into(),
        help: yapfast_lang::tr!("Max file size is {} MB please.", "20.00"),
        aside,
    }
}

/// A notice over the conversation, as the core gives one when something
/// fails.
pub fn notice() -> Vm {
    let mut vm = server_home();
    vm.shell.notice = Some(yapfast_lang::tr!(
        "Couldn't change your entrance sound: {}",
        "429 Too Many Requests"
    ));
    vm
}

/// The sidebar, the member list and the thread beside a channel at sizes
/// they were dragged to.
pub fn dragged() -> Vm {
    let mut vm = channel_with(conversation::thread_beside);
    vm.shell.sidebar_width = 260.0;
    vm.shell.aside_width = Some(500.0);
    vm
}

/// Anomi's own bar over a short window, and a message's menu that has no
/// room under the pointer: above it, 8 px from the window's top, over the
/// bar as the Slint window's popups draw.
pub fn popup_over_bar() -> Vm {
    let mut vm = menu::message_context();
    vm.chrome = chrome::framed();
    vm
}
