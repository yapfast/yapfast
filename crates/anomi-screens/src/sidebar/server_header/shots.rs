//! The server's header's shots, each the whole window: the golden
//! `menu/server_header.png` (the dropdown under the name, the chevron
//! turned), and the header as a Community server with a banner and a
//! verified one, which the golden window doesn't have.

use anomi_vm::Vm;
use anomi_vm::fixtures::sidebar::server_header as samples;
use anomi_vm::fixtures::{GOLDEN, Shot, menu, shell};
use anomi_vm::sidebar::Place;
use anomi_vm::sidebar::server_header::ServerHeader;

/// The window with the header's fields replaced.
fn with(mut vm: Vm, header: ServerHeader) -> Vm {
    if let Place::Server(server) = &mut vm.sidebar.place {
        server.header = header;
    }
    vm
}

fn dropdown() -> Vm {
    with(
        menu::server_header(),
        ServerHeader {
            open: true,
            ..samples::server_header()
        },
    )
}

fn community() -> Vm {
    with(shell::server_home(), samples::community())
}

fn verified() -> Vm {
    with(shell::server_home(), samples::verified())
}

pub fn shots() -> Vec<Shot> {
    let shot = |area, name, vm| Shot {
        area,
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("menu", "server_header", dropdown),
        shot("server", "header_community", community),
        shot("server", "header_verified", verified),
    ]
}
