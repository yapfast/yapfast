//! The server rail: task N02's tests, driving the whole window over the
//! shell's fixtures in the simulator (`super::support`). The rail draws its
//! tiles itself, so a click goes to a point: the tiles are 40 tall, 8 apart,
//! the first at y 65 under the home tile and its line.

use iced::advanced::widget::Operation;
use iced::advanced::widget::operation::{Outcome, focusable};
use iced::{Event, Point, keyboard, mouse};
use iced_test::Simulator;

use anomi_theme::{Renderer, Theme};

use anomi_vm::fixtures::{rail as samples, shell};
use anomi_vm::{Intent, Vm, rail};

use super::support::{asked, simulate};

fn click_at(vm: &Vm, x: f32, y: f32) -> Vec<Intent> {
    let mut ui = simulate(vm);
    ui.point_at(Point::new(x, y));
    ui.simulate(iced_test::simulator::click());
    asked(ui)
}

fn right_click_at(vm: &Vm, x: f32, y: f32) -> Vec<Intent> {
    let mut ui = simulate(vm);
    ui.point_at(Point::new(x, y));
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
    ]);
    asked(ui)
}

fn rail_intent(intent: rail::Intent) -> Vec<Intent> {
    vec![Intent::Rail(intent)]
}

#[test]
fn a_server_on_the_rail_is_a_button() {
    let vm = shell::server_home();
    // The second server's tile.
    assert_eq!(
        click_at(&vm, 36.0, 133.0),
        rail_intent(rail::Intent::Server("discord:g2".into()))
    );
}

#[test]
fn the_home_tile_shows_the_dms() {
    let vm = shell::server_home();
    assert_eq!(click_at(&vm, 36.0, 28.0), rail_intent(rail::Intent::Home));
}

#[test]
fn an_unread_dm_above_the_servers_opens_its_conversation() {
    let vm = samples::unread_dms_window();
    assert_eq!(
        click_at(&vm, 36.0, 85.0),
        rail_intent(rail::Intent::Dm("discord:d1".into()))
    );
    // The servers moved down by its tile and the line under it: the first
    // is now at 65 + 48 + 9.
    assert_eq!(
        click_at(&vm, 36.0, 142.0),
        rail_intent(rail::Intent::Server("discord:g1".into()))
    );
}

#[test]
fn a_folder_asks_to_open_and_close() {
    let vm = samples::folders_window();
    // Under the DM, the line and the first server: the open folder's tile.
    assert_eq!(
        click_at(&vm, 36.0, 194.0),
        rail_intent(rail::Intent::Folder("f1".into()))
    );
}

#[test]
fn a_server_in_an_open_folder_is_a_server_and_a_closed_folders_are_hidden() {
    let vm = samples::folders_window();
    // The folder's tile is 48 and 4 more join it to its first server.
    assert_eq!(
        click_at(&vm, 36.0, 170.0 + 52.0 + 20.0),
        rail_intent(rail::Intent::Server("discord:g2".into()))
    );
    // The closed folder after the two shows no server of its own: one row.
    let rows = vm
        .rail
        .servers
        .iter()
        .filter(|s| s.folder.as_deref() == Some("f2"))
        .count();
    assert_eq!(rows, 4);
}

#[test]
fn a_right_click_asks_for_the_servers_menu_at_the_pointer() {
    let vm = shell::server_home();
    let asked = right_click_at(&vm, 36.0, 85.0);
    assert_eq!(asked.len(), 1);
    let Intent::Rail(rail::Intent::Menu(id, spot)) = &asked[0] else {
        panic!("expected a menu, got {asked:?}");
    };
    assert_eq!(id, "discord:g1");
    assert_eq!(spot.anchor.position(), Point::new(36.0, 85.0));
}

#[test]
fn the_plus_offers_to_join_a_server_unless_the_account_cannot() {
    let vm = shell::server_home();
    assert_eq!(click_at(&vm, 36.0, 762.0), rail_intent(rail::Intent::Join));
    let mut vm = shell::server_home();
    vm.rail.can_join = false;
    assert_eq!(click_at(&vm, 36.0, 762.0), []);
}

#[test]
fn a_click_at_the_side_of_a_row_counts_and_one_in_the_gap_under_it_does_not() {
    let vm = shell::server_home();
    // The row is the rail's 72 wide: 10 is beside the tile, not outside.
    assert_eq!(
        click_at(&vm, 10.0, 85.0),
        rail_intent(rail::Intent::Server("discord:g1".into()))
    );
    // The 8 px between two tiles belong to neither.
    assert_eq!(click_at(&vm, 36.0, 108.0), []);
}

/// Tab: `focus_next`, chained until the operation is done.
fn tab(ui: &mut Simulator<'_, Intent, Theme, Renderer>) {
    let mut op: Box<dyn Operation<()>> = Box::new(focusable::focus_next::<()>());
    loop {
        ui.operate(op.as_mut());
        match op.finish() {
            Outcome::Chain(next) => op = next,
            _ => break,
        }
    }
}

#[test]
fn shift_f10_on_a_focused_server_asks_for_its_menu() {
    let vm = shell::server_home();
    // Tab walks the window's focusable things in tree order; the rail's
    // first server is one of the first few.
    let found = (1..=8).any(|presses| {
        let mut ui = simulate(&vm);
        for _ in 0..presses {
            tab(&mut ui);
        }
        ui.simulate([Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::F10),
            modified_key: keyboard::Key::Named(keyboard::key::Named::F10),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::F10),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::SHIFT,
            text: None,
            repeat: false,
        })]);
        matches!(
            asked(ui).as_slice(),
            [Intent::Rail(rail::Intent::Menu(id, _))] if id == "discord:g1"
        )
    });
    assert!(found, "no focused tile asked for the first server's menu");
}
