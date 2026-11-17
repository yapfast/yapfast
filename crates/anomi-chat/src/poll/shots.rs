//! The poll's shots: the whole window, the first two the golden
//! screenshots' (`chan/bot_embed_card_poll`, `dialog/create_poll`).

use anomi_vm::Vm;
use anomi_vm::chat::poll::{Dialog, Poll};
use anomi_vm::fixtures::chat::poll as fixtures;
use anomi_vm::fixtures::{GOLDEN, Shot, shell};
use anomi_vm::shell::Layer;

fn channel(poll: Poll) -> Vm {
    let mut vm = shell::server_home();
    vm.chat.timeline = fixtures::timeline(poll);
    vm
}

fn making(dialog: Dialog) -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::Poll);
    vm.chat.poll = dialog;
    vm
}

pub fn all() -> Vec<Shot> {
    let shot = |area, name, vm| Shot {
        area,
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("chan", "bot_embed_card_poll", || channel(fixtures::open())),
        shot("dialog", "create_poll", || making(fixtures::fresh())),
        shot("poll", "picked", || channel(fixtures::picked())),
        shot("poll", "multiple", || channel(fixtures::multiple())),
        shot("poll", "voted", || channel(fixtures::voted())),
        shot("poll", "closed", || channel(fixtures::closed())),
        shot("poll", "confirming", || channel(fixtures::confirming())),
        shot("poll", "shown", || channel(fixtures::shown())),
        shot(
            "dialog",
            "create_poll_filled",
            || making(fixtures::dialog()),
        ),
        shot("dialog", "create_poll_full", || making(fixtures::full())),
        shot("dialog", "create_poll_posting", || {
            making(fixtures::posting())
        }),
    ]
}
