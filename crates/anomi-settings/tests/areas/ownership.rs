//! Deleting a server or handing it over: task X12's tests, driving
//! `anomi_settings::server::ownership::view` over its fixtures in the
//! simulator (`super::support`).

use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::key::Named;
use iced_test::simulator::Simulator;

use anomi_settings::server::ownership::{CANCEL, CODE, CONFIRM, NAME, view};
use anomi_vm::fixtures::settings::ownership as sample;
use anomi_vm::settings::ownership::{Intent, Ownership};

use super::support::{SIZE, asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

fn simulate(ownership: &Ownership) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(ownership, front()))
}

fn focus(ui: &mut Ui<'_>, id: &'static str) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
}

fn press(ui: &mut Ui<'_>, id: &'static str) {
    focus(ui, id);
    ui.tap_key(Named::Enter);
}

#[test]
fn delete_asks_for_the_name_and_says_when_it_is_wrong() {
    let ownership = sample::delete_wrong();
    let mut ui = simulate(&ownership);
    assert!(ui.find("Delete 'Server'").is_ok());
    assert!(ui.find("ENTER SERVER NAME").is_ok());
    assert!(
        ui.find("You didn't enter the server name correctly")
            .is_ok()
    );
}

#[test]
fn typing_the_name_writes_and_enter_confirms() {
    let ownership = sample::delete_typed();
    let mut ui = simulate(&ownership);
    focus(&mut ui, NAME);
    ui.typewrite("x");
    ui.tap_key(Named::Enter);
    press(&mut ui, CONFIRM);
    press(&mut ui, CANCEL);
    assert_eq!(
        asked(ui),
        [
            Intent::Write("xServer".into()),
            Intent::Confirm,
            Intent::Confirm,
            Intent::Cancel
        ]
    );
}

#[test]
fn the_acknowledgement_ticks() {
    let ownership = sample::transfer();
    let mut ui = simulate(&ownership);
    ui.click("Acknowledge the transfer")
        .expect("the acknowledgement");
    assert_eq!(asked(ui), [Intent::Agree]);
}

#[test]
fn a_verified_or_sent_transfer_ticks_nothing() {
    for ownership in [sample::transfer_verified(), sample::transferring()] {
        let mut ui = simulate(&ownership);
        ui.click("Acknowledge the transfer").ok();
        assert_eq!(asked(ui), []);
    }
}

#[test]
fn a_sent_delete_asks_nothing_again() {
    let ownership = sample::deleting();
    let mut ui = simulate(&ownership);
    assert!(ui.find("Deleting...").is_ok());
    press(&mut ui, CONFIRM);
    press(&mut ui, CANCEL);
    focus(&mut ui, NAME);
    ui.typewrite("y");
    ui.tap_key(Named::Enter);
    assert_eq!(asked(ui), []);
}

#[test]
fn transfer_needs_the_acknowledgement() {
    let ownership = sample::transfer();
    let mut ui = simulate(&ownership);
    press(&mut ui, CONFIRM);
    assert_eq!(asked(ui), []);
    let ownership = sample::transfer_agreed();
    let mut ui = simulate(&ownership);
    press(&mut ui, CONFIRM);
    assert_eq!(asked(ui), [Intent::Confirm]);
}

#[test]
fn a_verified_server_blocks_the_transfer() {
    let ownership = sample::transfer_verified();
    let mut ui = simulate(&ownership);
    assert!(ui.find("Partnered or Verified servers must reach out to the Community team for ownership transfer requests. However, our Partner and Verified Server Programs are currently paused and not able to handle any requests.").is_ok());
    press(&mut ui, CONFIRM);
    assert_eq!(asked(ui), []);
}

#[test]
fn the_code_step_takes_typing_and_a_resend() {
    let ownership = sample::code_typed();
    let mut ui = simulate(&ownership);
    assert!(ui.find("VERIFICATION CODE").is_ok());
    focus(&mut ui, CODE);
    ui.typewrite("7");
    ui.click("Didn’t receive a code or it expired? Resend it.")
        .expect("resend");
    assert_eq!(asked(ui), [Intent::Write("7123456".into()), Intent::Resend]);
}

#[test]
fn a_resent_code_says_so() {
    let ownership = sample::code_resent();
    let mut ui = simulate(&ownership);
    assert!(ui.find("Verification code sent.").is_ok());
}

#[test]
fn nothing_open_draws_nothing() {
    let ownership = Ownership::default();
    let mut ui = simulate(&ownership);
    assert!(ui.find("Cancel").is_err());
}
