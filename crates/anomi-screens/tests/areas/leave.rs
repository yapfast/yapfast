//! Leaving a server, and the move-sync confirmation: task N13's tests,
//! driving `anomi_screens::places::leave::view` over its fixtures in the
//! simulator (`super::support`).

use iced::Widget as _;
use iced::keyboard::key::Named;
use iced_test::simulator::Simulator;

use anomi_screens::places::leave::view;
use anomi_vm::fixtures::places::leave as sample;
use anomi_vm::places::leave::{Intent, Leave};

use super::support::{SIZE, asked, front, settings};

fn simulate(leave: &Leave) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(leave, front()).boxed())
}

#[test]
fn the_red_button_confirms() {
    let leave = sample::server();
    let mut ui = simulate(&leave);
    ui.click("Leave server").expect("the button");
    assert_eq!(asked(ui), [Intent::Confirm]);
}

#[test]
fn enter_confirms_and_cancel_and_the_x_close() {
    let leave = sample::server();
    let mut ui = simulate(&leave);
    ui.tap_key(Named::Enter);
    ui.click("Cancel").expect("cancel");
    assert_eq!(asked(ui), [Intent::Confirm, Intent::Close]);
}

#[test]
fn a_group_offers_the_box_and_a_press_ticks_it() {
    let leave = sample::group();
    let mut ui = simulate(&leave);
    ui.click("Leave without notifying other members")
        .expect("the box");
    assert_eq!(asked(ui), [Intent::Tick]);
}

#[test]
fn a_server_has_no_box_and_a_refusal_shows() {
    let leave = sample::refused();
    let mut ui = simulate(&leave);
    assert!(ui.find("Leave without notifying other members").is_err());
    assert!(ui.find(leave.error.as_str()).is_ok());
}

#[test]
fn nothing_is_asked_twice_while_it_is_sent() {
    let leave = sample::leaving();
    let mut ui = simulate(&leave);
    ui.tap_key(Named::Enter);
    let _ = ui.click("Leave server");
    assert_eq!(asked(ui), []);
}

#[test]
fn move_sync_answers_with_the_alternative_in_place_of_cancel() {
    let leave = sample::move_sync();
    let mut ui = simulate(&leave);
    assert!(ui.find("Cancel").is_err());
    ui.click("Keep Current Permissions").expect("alternative");
    ui.click("Sync Permissions").expect("confirm");
    assert_eq!(asked(ui), [Intent::Alternative, Intent::Confirm]);
}
