//! The core's `AppState` as the screens' view-models: what
//! `src/ui/refresh.rs` does for the Slint window.
//!
//! Each area has a module with a `show` that rebuilds its view-model only
//! when what it shows changed. It keeps a key of what it showed last (the
//! core's change counters, the selection, the area's own inputs) in
//! [`Shown`], and returns at once when the key is the same: a backend event
//! that changes nothing an area shows costs that area a comparison. An
//! area's key must cover everything its rows read.
//!
//! Every area's module is here, its I task's (docs/iced/tasks/integration.md);
//! each task adds its `show` call to [`refresh`] and its key to [`Shown`] in
//! turn.

mod account;
mod chat;
mod editor;
mod header;
mod inbox;
mod menus;
mod people;
mod places;
mod rail;
pub mod rows;
mod server_settings;
mod settings;
mod shell;
mod sidebar;
mod sign_in;
#[cfg(test)]
mod tests;
mod voice;

use yapfast_core::application::AppState;

use anomi_vm::Vm;

use crate::layout::Layout;

/// What each area showed last.
#[derive(Default)]
pub struct Shown {
    rail: Option<rail::Key>,
    sidebar: Option<sidebar::Key>,
    account: Option<account::Key>,
    header: Option<header::Key>,
}

impl Shown {
    /// Shows every area again on the next refresh, as after the language
    /// changes: their text was made in the old one.
    pub fn forget(&mut self) {
        *self = Self::default();
    }
}

/// Shows the state in every area whose inputs changed.
pub fn refresh(state: &AppState, layout: &Layout, vm: &mut Vm, shown: &mut Shown) {
    vm.shell.connecting = state.busy && state.rooms.is_empty();
    vm.shell.notice = state.notice.clone();
    rail::show(state, &mut vm.rail, &mut shown.rail);
    sidebar::show(state, &mut vm.sidebar, &mut shown.sidebar);
    account::show(state, layout, &mut vm.account, &mut shown.account);
    header::show(state, &mut vm.header, &mut shown.header);
}
