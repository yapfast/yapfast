//! The shots of the chips and the Reactions window. `chan/..._chips` of the
//! golden shots is `server/general_unread_divider`'s chips and
//! `chan/showcase_emoji_stickers_gif`'s.

use anomi_vm::Vm;
use anomi_vm::fixtures::chat::reactions as sample;

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm: fn() -> Vm| Shot {
        area: "reactions",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("chips", sample::in_channel),
        shot("window", || sample::window(sample::dialog())),
        shot("window_loading", || sample::window(sample::loading())),
        shot("window_failed", || sample::window(sample::failed())),
        shot("window_voters", || sample::window(sample::voters())),
    ]
}
