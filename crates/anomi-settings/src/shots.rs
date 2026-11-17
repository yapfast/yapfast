//! anomi-settings's shots: each area module's `shots()`, collected for
//! `anomi-shots` (through `anomi_screens::shots::all`). F01 keeps this list;
//! a task fills its own module's `shots()`.

pub use anomi_vm::fixtures::{GOLDEN, SMALL, Shot};

pub fn all() -> Vec<Shot> {
    [
        crate::about::shots(),
        crate::account::shots(),
        crate::advanced::shots(),
        crate::appearance::shots(),
        crate::appearance::editor::shots(),
        crate::channel::shots(),
        crate::channel::create::shots(),
        crate::channel::permissions::shots(),
        crate::discord::shots(),
        crate::frame::shots(),
        crate::games::shots(),
        crate::keybinds::shots(),
        crate::messages::shots(),
        crate::mfa::shots(),
        crate::notifications::shots(),
        crate::server::shots(),
        crate::server::audit::shots(),
        crate::server::bans::shots(),
        crate::server::boosts::shots(),
        crate::server::emoji::shots(),
        crate::server::engagement::shots(),
        crate::server::invites::shots(),
        crate::server::members::shots(),
        crate::server::ownership::shots(),
        crate::server::roles::editor::shots(),
        crate::server::roles::list::shots(),
        crate::server::safety::shots(),
        crate::server::stickers::shots(),
        crate::sounds::shots(),
        crate::voice::shots(),
    ]
    .into_iter()
    .flatten()
    .collect()
}
