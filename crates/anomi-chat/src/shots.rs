//! anomi-chat's shots: each area module's `shots()`, collected for
//! `anomi-shots` (through `anomi_screens::shots::all`). F01 keeps this list;
//! a task fills its own module's `shots()`.

pub use anomi_vm::fixtures::{GOLDEN, SMALL, Shot};

pub fn all() -> Vec<Shot> {
    [
        crate::attachments::shots(),
        crate::bot_forms::shots(),
        crate::cards::shots(),
        crate::components::shots(),
        crate::composer::shots(),
        crate::composer::commands::shots(),
        crate::composer::suggestions::shots(),
        crate::embeds::shots(),
        crate::forum::shots(),
        crate::forum::new_post::shots(),
        crate::forward::shots(),
        crate::inline_edit::shots(),
        crate::menu::shots(),
        crate::message::shots(),
        crate::pickers::shots(),
        crate::pickers::emoji::shots(),
        crate::pickers::gif::shots(),
        crate::pickers::sticker::shots(),
        crate::pins::shots(),
        crate::poll::shots(),
        crate::reactions::shots(),
        crate::search::shots(),
        crate::threads::new::shots(),
        crate::threads::panel::shots(),
        crate::threads::popover::shots(),
        crate::timeline::shots(),
        crate::video::shots(),
        crate::viewer::shots(),
        crate::voice_message::shots(),
    ]
    .into_iter()
    .flatten()
    .collect()
}
