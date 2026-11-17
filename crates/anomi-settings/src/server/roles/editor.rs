//! A server's role editor. Task X09 (docs/iced/tasks/settings.md) fills it;
//! F01's stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::settings::role_editor::{Intent, RoleEditor};

pub fn view(_role_editor: &RoleEditor, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until X09 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
