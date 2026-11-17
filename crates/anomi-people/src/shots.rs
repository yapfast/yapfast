//! anomi-people's shots: each area module's `shots()`, collected for
//! `anomi-shots` (through `anomi_screens::shots::all`). F01 keeps this list;
//! a task fills its own module's `shots()`.

pub use anomi_vm::fixtures::{GOLDEN, SMALL, Shot};

pub fn all() -> Vec<Shot> {
    [
        crate::active_now::shots(),
        crate::activity::shots(),
        crate::add_people::shots(),
        crate::cards::shots(),
        crate::editor::shots(),
        crate::editor::name_style::shots(),
        crate::editor::preview::shots(),
        crate::friends::shots(),
        crate::friends::add::shots(),
        crate::friends::requests::shots(),
        crate::members::shots(),
        crate::person::shots(),
        crate::profile::badges::shots(),
        crate::profile::card::shots(),
        crate::profile::full::shots(),
        crate::status::shots(),
    ]
    .into_iter()
    .flatten()
    .collect()
}
