//! The custom status dialog and the status menu. Task P10
//! (docs/iced/tasks/people.md). The Slint side is `StatusDialog`,
//! `StatusMenu` and `StatusOption` (`ui/dialogs.slint:27`, `:621`, `:567`);
//! what each asks for is `anomi_vm::people::status::Intent`.
//!
//! - [`view`] is the dialog, the layer `Layer::Status`: the shell gives it
//!   the backdrop, Escape and the way in, and this draws the card.
//! - [`menu::view`] is the classic style's menu of the four statuses. The
//!   iced interface draws the Enabled style alone, where the account's own
//!   card shows the statuses (`account::popover`, N06), so no layer opens
//!   it; it is here, ported, for whoever places it.
//!
//! The golden screenshots `status/menu.png` and `status/submenu.png` are the
//! account card's (N06's); `status/custom_dialog.png` is the dialog.

mod choices;
mod dialog;
mod emoji_button;
mod link;
pub mod menu;
mod option;

use anomi_theme::prelude::*;
use anomi_vm::people::status::{Intent, Status};

pub use dialog::{CANCEL, SAVE, text_id};
pub use option::option;

pub fn view(status: &Status, motion: Motion) -> impl View<Intent> + '_ {
    dialog::view(status, motion)
}

/// Its shots for `anomi-shots`: the whole window over the dialog, the first
/// the golden screenshot's (`status/custom_dialog.png`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::Vm;
    use anomi_vm::fixtures::people::status;
    use anomi_vm::fixtures::shell;
    use anomi_vm::shell::Layer;

    use crate::shots::{GOLDEN, Shot};

    fn over(status: Status) -> Vm {
        let mut vm = shell::open(shell::server_home(), Layer::Status);
        vm.people.status = status;
        vm
    }
    let shot = |name, vm| Shot {
        area: "status",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("custom_dialog", || over(status::set())),
        shot("custom_dialog_empty", || over(status::empty())),
        shot("custom_dialog_timed", || over(status::timed())),
        shot("custom_dialog_saving", || over(status::saving())),
        shot("custom_dialog_failed", || over(status::failed())),
    ]
}
