//! An incoming call: task V08's tests, driving `anomi_voice::incoming::view`
//! over its fixtures in the simulator (`super::support`).

use iced::Widget as _;
use iced::keyboard::key::Named;
use iced_test::selector;
use iced_test::simulator::Simulator;

use anomi_vm::fixtures::voice::incoming as sample;
use anomi_vm::voice::incoming::{Incoming, Intent};
use anomi_voice::incoming::{self, switch, view};

use super::support::{SIZE, asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

fn ringing(incoming: &Incoming) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(incoming, front()).boxed())
}

fn asking(incoming: &Incoming) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, switch(incoming, front()).boxed())
}

#[test]
fn it_names_the_caller_and_what_rings() {
    let incoming = sample::ringing();
    let mut ui = ringing(&incoming);
    assert!(ui.find("Bob").is_ok());
    assert!(ui.find("Incoming call").is_ok());
    assert!(ui.find(selector::id(incoming::DECLINE)).is_ok());
    assert!(ui.find(selector::id(incoming::ACCEPT)).is_ok());
}

#[test]
fn a_group_says_who_is_calling() {
    let incoming = sample::group();
    let mut ui = ringing(&incoming);
    assert!(ui.find("Weekend plans").is_ok());
    assert!(ui.find(incoming.detail.as_str()).is_ok());
}

#[test]
fn a_long_name_and_detail_still_draw() {
    let incoming = sample::long();
    let mut ui = ringing(&incoming);
    assert!(ui.find(selector::id(incoming::ACCEPT)).is_ok());
    assert!(ui.find(incoming.detail.as_str()).is_ok());
}

#[test]
fn accept_and_decline_name_the_room() {
    let incoming = sample::ringing();
    let room = incoming.room.clone();
    let mut ui = ringing(&incoming);
    ui.click(selector::id(incoming::DECLINE)).expect("decline");
    ui.click(selector::id(incoming::ACCEPT)).expect("accept");
    assert_eq!(
        asked(ui),
        [Intent::Decline(room.clone()), Intent::Accept(room)]
    );
}

#[test]
fn a_still_window_still_draws_the_card() {
    let incoming = sample::ringing();
    // `Motion::default()` is a window behind another: the card still draws.
    let mut ui = Simulator::with_size(
        settings(),
        SIZE,
        view(&incoming, anomi_theme::Motion::default()).boxed(),
    );
    assert!(ui.find(selector::id(incoming::ACCEPT)).is_ok());
}

#[test]
fn switching_asks_to_leave_and_names_the_channels() {
    let incoming = sample::switching();
    let mut ui = asking(&incoming);
    assert!(ui.find("Switch voice channel?").is_ok());
    assert!(ui.find(selector::id(incoming::CONFIRM)).is_ok());
    assert!(ui.find(incoming.switch.text.as_str()).is_ok());
}

#[test]
fn joining_to_watch_says_so() {
    let incoming = sample::watching();
    let mut ui = asking(&incoming);
    assert!(ui.find("Watch stream?").is_ok());
    assert!(ui.find(selector::id(incoming::CONFIRM)).is_ok());
}

#[test]
fn the_buttons_switch_and_cancel() {
    let incoming = sample::switching();
    let mut ui = asking(&incoming);
    ui.click(selector::id(incoming::CANCEL)).expect("cancel");
    ui.click(selector::id(incoming::CONFIRM)).expect("switch");
    assert_eq!(asked(ui), [Intent::CancelSwitch, Intent::ConfirmSwitch]);
}

#[test]
fn enter_switches_and_escape_cancels() {
    let incoming = sample::switching();
    let mut ui = asking(&incoming);
    ui.tap_key(Named::Enter);
    ui.tap_key(Named::Escape);
    assert_eq!(asked(ui), [Intent::ConfirmSwitch, Intent::CancelSwitch]);
}
