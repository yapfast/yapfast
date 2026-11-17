//! The editor's shots: `settings/theme_editor.png` as the Slint app showed it
//! (a new theme copied from Bonfire), and a saved theme with a colour whose
//! text is none.

use anomi_vm::fixtures::settings::theme_editor::{open, wrong};

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    [
        ("theme_editor", open as fn() -> anomi_vm::Vm),
        ("theme_editor_wrong", wrong),
    ]
    .into_iter()
    .map(|(name, vm)| Shot {
        area: "settings",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    })
    .collect()
}
