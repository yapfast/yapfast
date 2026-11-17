//! The whole window's samples: the DM home, a server, and connecting. Each
//! area's shot starts from one of them and fills its own field. Task N01's.

use crate::{Vm, shell};

use super::{account, header, rail, sidebar};

/// The whole window on the DM home, Person 5's DM open.
pub fn dm_home() -> Vm {
    Vm {
        shell: shell::Shell {
            connecting: false,
            ..shell::Shell::default()
        },
        rail: rail::rail(),
        sidebar: sidebar::home(),
        account: account::account(),
        header: header::dm(),
        ..Vm::default()
    }
}

/// The whole window on the first server, #general open.
pub fn server_home() -> Vm {
    let mut vm = dm_home();
    vm.rail.home.selected = false;
    vm.rail.servers[0].tile.selected = true;
    vm.sidebar = sidebar::server();
    vm.header = header::channel();
    vm
}

/// The window before the first sync: nothing listed yet.
pub fn connecting() -> Vm {
    Vm {
        account: account::account(),
        ..Vm::default()
    }
}
