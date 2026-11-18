//! The dialogs about places from the core's state: the quick switcher, a
//! new DM, a server's invite, joining one, the dialogs a clicked link opens
//! (a scheduled event, a gift, an authorization) and leaving a server or
//! the move-sync question (`show_invite` to `show_oauth`,
//! `src/ui/refresh.rs:2717`-`:2939`; `show_leave`, `show_move_sync`,
//! `:3320`, `:3356`; the new DM and the switcher of `show_dialogs`,
//! `:3528`).
//!
//! Each dialog's layer is open while the core has the dialog, and closes as
//! the core lets it go; the area's own state (a field's text, the invite's
//! choices, the switcher's chosen place) starts afresh as it opens. The
//! switcher's places and the new DM's people are built again only when
//! their key changes; the other dialogs are a handful of fields, set when
//! they differ.

mod links;
mod new_dm;
mod servers;
mod switcher;
#[cfg(test)]
mod tests;

use yapfast_core::application::AppState;

use anomi_vm::Vm;
use anomi_vm::shell::{Layer, Shell};

use crate::layout::Layout;
use crate::pictures::Requests;

/// What the switcher and the new DM showed last.
#[derive(Default)]
pub(super) struct Shown {
    switcher: switcher::Shown,
    new_dm: new_dm::Shown,
}

/// Shows the dialogs the core has, and closes those it let go. `focus`
/// takes the field a dialog that opens gives the keys to.
pub(super) fn show(
    state: &AppState,
    layout: &Layout,
    vm: &mut Vm,
    shown: &mut Shown,
    requests: &mut Requests,
    focus: &mut Option<&'static str>,
) {
    let shell = &mut vm.shell;
    switcher::show(
        state,
        shell,
        &mut vm.switcher,
        &mut shown.switcher,
        requests,
        focus,
    );
    new_dm::show(
        state,
        shell,
        &mut vm.new_dm,
        &mut shown.new_dm,
        requests,
        focus,
    );
    let places = &mut vm.places;
    servers::invite(state, shell, &mut places.invite);
    servers::join(state, shell, &mut places.join, requests, focus);
    servers::leave(state, shell, &mut places.leave);
    links::event(state, layout.twelve_hour_clock, shell, &mut places.event);
    links::gift(state, shell, &mut places.gift);
    links::oauth(state, shell, &mut places.oauth, requests);
}

/// A dialog played its way out: the rows it held go, with their pictures.
pub(super) fn gone(layer: Layer, vm: &mut Vm, shown: &mut Shown) {
    match layer {
        Layer::Switcher => {
            vm.switcher.places = Vec::new();
            shown.switcher = switcher::Shown::default();
        }
        Layer::NewDm => {
            vm.new_dm.people = Vec::new();
            shown.new_dm = new_dm::Shown::default();
        }
        Layer::LeavingLink => vm.places.leaving_link = Default::default(),
        _ => {}
    }
}

/// Opens `layer` while `open`, else closes it; whether it opened now.
fn follow(shell: &mut Shell, layer: Layer, open: bool) -> bool {
    match (open, shell.is_open(layer)) {
        (true, false) => {
            shell.open(layer);
            true
        }
        (false, true) => {
            shell.close(layer);
            false
        }
        _ => false,
    }
}

/// Sets `field` to `value` when they differ, keeping its allocation.
fn set(field: &mut String, value: &str) {
    if field != value {
        value.clone_into(field);
    }
}
