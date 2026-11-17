//! The whole window around each sample of the channel list, for shots and
//! the window's tests: the first server open (`fixtures::shell::server_home`)
//! with the list as the sample has it.

use iced::Point;
use yapfast_lang::{tr, trn};

use crate::Vm;
use crate::fixtures::{rail, shell};
use crate::menu::{Choice, Menu, Row, Spot};
use crate::sidebar::Place;
use crate::sidebar::channels::Channels;

use super::GENERAL;

/// Where `menu/channel_context.png` right-clicked #general: the middle of
/// its row, where the menu's top-left corner is (142, 166) on the golden
/// row's 150-181. This window lists the row 91-122 px down (the golden one
/// has the events row above the list).
pub const MENU_POINT: Point = Point::new(142.0, 107.0);

/// The server's window with `channels` listed.
pub fn window(channels: Channels) -> Vm {
    with(shell::server_home(), channels)
}

fn with(mut vm: Vm, channels: Channels) -> Vm {
    if let Place::Server(server) = &mut vm.sidebar.place {
        server.channels = channels;
    }
    vm
}

/// `voice/joined_sidebar_dock.png`: in the Lounge's call, the dock on the
/// account card and the call's speaker on the server.
pub fn joined_window() -> Vm {
    let mut vm = with(rail::call_window(), super::joined());
    vm.shell.voice_dock = true;
    vm
}

/// `thread/book_club_not_joined.png`'s list.
pub fn book_club_window() -> Vm {
    window(super::book_club())
}

/// The call's participants speaking, joining, and on another client.
pub fn speaking_window() -> Vm {
    let mut vm = with(rail::call_window(), super::speaking());
    vm.shell.voice_dock = true;
    vm
}

pub fn folded_window() -> Vm {
    window(super::folded())
}

pub fn pinned_window() -> Vm {
    window(super::pinned())
}

pub fn arranging_window() -> Vm {
    window(super::arranging())
}

pub fn moving_window() -> Vm {
    window(super::moving())
}

pub fn empty_window() -> Vm {
    window(super::empty())
}

pub fn browsing_window() -> Vm {
    window(super::browsing())
}

/// `menu/channel_context.png`: #general's menu at the pointer, as Discord
/// orders it, without icons; Mark as read greyed with nothing to read.
pub fn channel_menu() -> Vm {
    let row = |text: &str, id: &str| Row::new(text, Choice::new(GENERAL, id).into()).plain();
    let levels = [
        (tr!("All messages"), "level-all"),
        (tr!("Only @mentions"), "level-mentions"),
        (tr!("Nothing"), "level-nothing"),
    ];
    let levels = levels
        .into_iter()
        .map(|(text, id)| row(text, id).checked(id == "level-mentions"))
        .collect();
    let mute_times = [15, 60, 8 * 60]
        .into_iter()
        .map(|minutes: u64| {
            let text = if minutes.is_multiple_of(60) {
                trn!("For {n} hour", "For {n} hours", minutes / 60)
            } else {
                trn!("For {n} minute", "For {n} minutes", minutes)
            };
            row(&text, &format!("mute-{minutes}"))
        })
        .chain([row(tr!("Until I turn it back on"), "mute-always")])
        .collect();
    let rows = vec![
        row(tr!("Mark as read"), "mark-read").disabled(true),
        row(tr!("Invite to channel"), "invite").separated(),
        row(tr!("Pin channel to top"), "pin-channel"),
        row(tr!("Copy link"), "copy-link"),
        row(tr!("Mute channel"), "mute-always")
            .submenu(mute_times)
            .separated(),
        Row::opens(tr!("Notification settings"), levels)
            .subtitle(tr!("Only @mentions"))
            .plain(),
        row(tr!("Edit channel"), "edit-channel").separated(),
        row(tr!("Duplicate channel"), "duplicate-channel"),
        row(tr!("Create text channel"), "create-channel"),
        row(tr!("Delete channel"), "delete-channel")
            .danger()
            .separated(),
    ];
    let mut vm = shell::server_home();
    vm.menu = Menu::new(1, Spot::pointer(MENU_POINT), rows);
    vm
}
