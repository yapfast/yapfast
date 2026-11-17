//! The panel for `anomi-shots`, each the whole window on #general, whose
//! member list the panel tops. The golden `server/general_unread_divider.png`
//! shows it with three cards of five; the card on a profile is the profile
//! card's shot (P07), which holds [`card::view`](super::card::view).

use anomi_vm::Vm;
use anomi_vm::fixtures::people::activity;
use anomi_vm::fixtures::shell;
use anomi_vm::people::activity::Activity;

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm: fn() -> Vm| Shot {
        area: "server",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("activity_panel", || window(activity::panel())),
        shot("activity_opened", || window(activity::opened())),
        shot("activity_hidden", || window(activity::hidden())),
        shot("activity_two", || window(activity::two())),
    ]
}

fn window(activity: Activity) -> Vm {
    let mut vm = shell::server_home();
    vm.people.activity = activity;
    vm
}
