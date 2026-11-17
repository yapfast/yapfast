//! A call's connection details: task V02's tests, driving
//! `anomi_voice::connection::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced::widget::text;
use iced_test::Simulator;

use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::voice::connection as samples;
use anomi_vm::voice::connection::{Connection, Intent, Tab};
use anomi_voice::connection;
use anomi_widgets::tip::Side;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(420.0, 560.0);

fn simulate(vm: &Connection) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, connection::view(vm, front()))
}

#[test]
fn a_call_without_others_has_no_tabs() {
    let vm = samples::connection();
    let mut ui = simulate(&vm);
    assert!(ui.find("Average ping:").is_ok());
    assert!(ui.find("Last ping:").is_ok());
    assert!(ui.find("Outbound packet loss rate:").is_ok());
    assert!(ui.find("Secure connection").is_ok());
    assert!(ui.find("Privacy").is_err());
    assert!(ui.find("Stream").is_err());
}

#[test]
fn no_pings_yet_says_unknown_and_hides_the_rest() {
    let vm = samples::measuring();
    let mut ui = simulate(&vm);
    assert!(ui.find("Unknown").is_ok());
    assert!(ui.find("Last ping:").is_err());
    assert!(ui.find("Outbound packet loss rate:").is_err());
    assert!(ui.find("10 ms").is_ok());
    assert!(ui.find("5 ms").is_ok());
}

#[test]
fn the_graph_names_its_top_and_half() {
    let vm = samples::connection();
    let mut ui = simulate(&vm);
    let top = vm.top();
    assert!(ui.find(format!("{top} ms")).is_ok());
    assert!(ui.find(format!("{} ms", top / 2)).is_ok());
}

#[test]
fn picking_a_tab_asks_for_it() {
    let vm = Connection {
        tab: Tab::Connection,
        ..samples::private()
    };
    let mut ui = simulate(&vm);
    ui.click("Privacy").unwrap();
    ui.click("Connection").unwrap();
    assert_eq!(
        asked(ui),
        [Intent::Tab(Tab::Privacy), Intent::Tab(Tab::Connection)]
    );
}

#[test]
fn the_stream_tab_is_there_only_while_streaming() {
    let vm = samples::streaming();
    let mut ui = simulate(&vm);
    assert!(ui.find("Stream").is_ok());
    assert!(ui.find("Encoder:").is_ok());
    assert!(ui.find("NVENC on the GPU").is_ok());
    assert!(ui.find("Packets resent:").is_ok());
    assert!(ui.find("Average ping:").is_err());
}

#[test]
fn the_code_shows_in_groups_and_copies_with_spaces() {
    let vm = samples::private();
    let mut ui = simulate(&vm);
    assert!(ui.find("Voice Privacy Code").is_ok());
    assert!(ui.find("67890").is_ok());
    assert!(ui.find("End-to-end encrypted").is_ok());
    ui.click(iced_test::selector::id(connection::COPY)).unwrap();
    assert_eq!(
        asked(ui),
        [Intent::Copy(
            "12345 67890 24680 13579 11223 44556".to_owned()
        )]
    );
}

#[test]
fn a_copied_code_says_so() {
    let vm = Connection {
        copied: true,
        ..samples::private()
    };
    let mut ui = simulate(&vm);
    assert!(ui.find("Copied").is_ok());
    assert!(ui.find("Copy").is_err());
    ui.click(iced_test::selector::id(connection::COPY)).unwrap();
    assert_eq!(asked(ui).len(), 1);
}

#[test]
fn a_privacy_tab_with_no_code_falls_back_to_connection() {
    let vm = Connection {
        tab: Tab::Privacy,
        ..samples::connection()
    };
    let mut ui = simulate(&vm);
    assert!(ui.find("Average ping:").is_ok());
}

#[test]
fn the_signal_opens_the_popout_once_measured() {
    let ping = "12 ms".to_owned();
    let mut ui = Simulator::with_size(
        settings(),
        SIZE,
        connection::touch(text("Signal"), &ping, true, Side::Above, front()),
    );
    ui.click("Signal").unwrap();
    let asked = asked(ui);
    assert!(matches!(asked.as_slice(), [Intent::Open(_)]));

    let mut ui = Simulator::with_size(
        settings(),
        SIZE,
        connection::touch(text("Signal"), &ping, false, Side::Above, front()),
    );
    ui.click("Signal").unwrap();
    assert!(self::asked(ui).is_empty());
}

#[test]
fn the_pointer_on_the_signal_reads_the_link_again() {
    let ping = "12 ms".to_owned();
    let mut ui = Simulator::with_size(
        settings(),
        SIZE,
        connection::touch(text("Signal"), &ping, true, Side::Above, front()),
    );
    ui.point_at(iced::Point::new(10.0, 10.0));
    assert!(asked(ui).contains(&Intent::Read));

    let mut ui = Simulator::with_size(
        settings(),
        SIZE,
        connection::touch(text("Signal"), &ping, false, Side::Above, front()),
    );
    ui.point_at(iced::Point::new(10.0, 10.0));
    assert!(asked(ui).is_empty());
}
