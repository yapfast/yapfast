//! The frame's shots: the golden screenshots of the settings window, the
//! whole window each from a fixture (see `crate::shots`).

use anomi_vm::fixtures::settings::frame::{account, unsaved};

use crate::shots::{GOLDEN, SMALL, Shot};

/// Its shots for `anomi-shots`: `settings/account.png` and
/// `small/settings.png` as the Slint app showed them, and the frame with
/// the bar for unsaved changes.
pub fn shots() -> Vec<Shot> {
    vec![
        Shot {
            area: "settings",
            name: "account",
            size: GOLDEN,
            theme: "bonfire",
            vm: account,
        },
        Shot {
            area: "small",
            name: "settings",
            size: SMALL,
            theme: "bonfire",
            vm: account,
        },
        Shot {
            area: "settings",
            name: "unsaved",
            size: GOLDEN,
            theme: "bonfire",
            vm: || {
                let mut vm = account();
                vm.settings.frame = unsaved();
                vm
            },
        },
    ]
}
