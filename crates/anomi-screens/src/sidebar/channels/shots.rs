//! The channel list's shots: the whole window as the golden screenshots
//! have it (`server/general_unread_divider`, `voice/joined_sidebar_dock`,
//! `thread/book_club_not_joined`, `menu/channel_context`), then the states
//! they don't show. Shots have no pointer: the hover fill, the chat button
//! and the card of who is in a voice channel are the tests' and the app's.

use anomi_vm::Vm;
use anomi_vm::fixtures::shell;
use anomi_vm::fixtures::sidebar::channels;

use crate::shots::{GOLDEN, SMALL, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |area, name, vm: fn() -> Vm| Shot {
        area,
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("channels", "general_unread_divider", shell::server_home),
        shot("channels", "joined_sidebar_dock", channels::joined_window),
        shot(
            "channels",
            "book_club_not_joined",
            channels::book_club_window,
        ),
        shot("menu", "channel_context", channels::channel_menu),
        shot("channels", "speaking", channels::speaking_window),
        shot("channels", "folded", channels::folded_window),
        shot("channels", "pinned_locked_muted", channels::pinned_window),
        shot("channels", "arranging", channels::arranging_window),
        shot("channels", "moving", channels::moving_window),
        shot("channels", "empty", channels::empty_window),
        shot("channels", "browsing", channels::browsing_window),
        Shot {
            theme: "catppuccin-latte",
            ..shot("channels", "general_latte", shell::server_home)
        },
        Shot {
            size: SMALL,
            ..shot("channels", "general_small", shell::server_home)
        },
    ]
}
