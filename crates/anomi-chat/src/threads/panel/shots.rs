//! The panel's shots: the golden screenshots (`forum/post_open`,
//! `thread/panel_menu`, `thread/panel`) and the states none shows. The
//! messages are C02's: until it passes they draw nothing, and the rows show
//! only what the timeline puts around them.

use anomi_vm::fixtures::chat::thread_panel as sample;

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |area, name, vm: fn() -> anomi_vm::Vm| Shot {
        area,
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("forum", "post_open", sample::forum_post_open),
        shot("thread", "panel_menu", sample::menu_open),
        shot("thread", "panel", sample::plans_as_channel),
        shot("thread", "panel_loading", || {
            sample::in_forum(sample::loading())
        }),
        shot(
            "thread",
            "panel_empty",
            || sample::in_forum(sample::empty()),
        ),
        shot("thread", "panel_earlier", || {
            sample::in_forum(sample::earlier())
        }),
        shot("thread", "panel_failed", || {
            sample::in_forum(sample::failed())
        }),
        shot("thread", "panel_replying", || {
            sample::in_forum(sample::replying())
        }),
        shot("thread", "panel_locked", || {
            sample::in_forum(sample::locked())
        }),
    ]
}
