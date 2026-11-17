//! anomi-voice's shots: each area module's `shots()`, collected for
//! `anomi-shots` (through `anomi_screens::shots::all`). F01 keeps this list;
//! a task fills its own module's `shots()`.

pub use anomi_vm::fixtures::{GOLDEN, SMALL, Shot};

pub fn all() -> Vec<Shot> {
    [
        crate::connection::shots(),
        crate::controls::shots(),
        crate::devices::shots(),
        crate::dock::shots(),
        crate::incoming::shots(),
        crate::join::shots(),
        crate::participant_menu::shots(),
        crate::pip::shots(),
        crate::share::shots(),
        crate::soundboard::shots(),
        crate::stage::shots(),
        crate::tiles::shots(),
    ]
    .into_iter()
    .flatten()
    .collect()
}
