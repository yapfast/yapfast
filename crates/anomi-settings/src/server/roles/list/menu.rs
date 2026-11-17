//! A role's More menu: Move Up, Move Down, Duplicate Role and Delete
//! (`more`, `ui/roles.slint:254`-`:290`), each live only where the list
//! allows it. The app opens it where [`Intent::More`] says, from these rows,
//! and the shell draws it as every menu.

use anomi_vm::menu::{Menu, Row, Spot};
use anomi_vm::settings::roles::{Intent, Roles};
use yapfast_lang::tr;

/// Its width (`width: 180px`).
pub const WIDTH: f32 = 180.0;

/// The rows of the menu of the role `id`; none for a role the list doesn't
/// hold.
pub fn rows(roles: &Roles, id: &str) -> Vec<Row> {
    let Some(index) = roles.position(id) else {
        return Vec::new();
    };
    let copyable = roles.rows[index].may_copy;
    let to = |to: usize| Intent::Move {
        id: id.to_owned(),
        to,
    };
    vec![
        Row::new(tr!("Move Up"), to(index.saturating_sub(1)).into())
            .disabled(!roles.may_move_up(index)),
        Row::new(tr!("Move Down"), to(index + 1).into()).disabled(!roles.may_move_down(index)),
        Row::new(tr!("Duplicate Role"), Intent::Copy(id.to_owned()).into()).disabled(!copyable),
        Row::new(tr!("Delete"), Intent::Delete(id.to_owned()).into())
            .danger()
            .disabled(!copyable),
    ]
}

/// The menu of the role `id`, open at `at` as the app's `serial`th.
pub fn menu(roles: &Roles, id: &str, at: Spot, serial: u64) -> Menu {
    Menu {
        narrowest: Some(WIDTH),
        ..Menu::new(serial, at, rows(roles, id))
    }
}
