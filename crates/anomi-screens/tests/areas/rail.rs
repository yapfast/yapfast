//! The server rail: task N02's tests, driving the whole window over the
//! shell's fixtures in the simulator (`super::support`).

use anomi_vm::fixtures::shell;
use anomi_vm::{Intent, rail};

use super::support::{asked, simulate};

#[test]
fn a_server_on_the_rail_is_a_button() {
    let vm = shell::server_home();
    let mut ui = simulate(&vm);
    // The rail draws its tiles itself; a click at the second server's tile.
    ui.point_at(iced::Point::new(36.0, 133.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(
        asked(ui),
        [Intent::Rail(rail::Intent::Server("discord:g2".into()))]
    );
}
