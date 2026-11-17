//! The two-factor prompt: task S11's tests, driving
//! `anomi_settings::mfa::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::key::Named;
use iced_test::simulator::Simulator;

use anomi_settings::mfa::{BACK, CANCEL, CODE, CONFIRM, RESEND, SURE, view};
use anomi_vm::fixtures::settings::mfa as sample;
use anomi_vm::settings::mfa::{Intent, Method, Mfa};

use super::support::{SIZE, asked, front, settings};

fn simulate(mfa: &Mfa) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(mfa, front()).boxed())
}

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

/// Presses a button as a keyboard does: focus it, then Enter. A button draws
/// its own text, so the simulator can't find it by its words.
fn press(ui: &mut Ui<'_>, id: &'static str) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(Named::Enter);
}

/// Where the keys go to the code field.
fn into_field(ui: &mut Ui<'_>) {
    let mut focus = focusable::focus::<()>(Id::new(CODE));
    ui.operate(&mut focus as &mut dyn Operation);
}

#[test]
fn the_list_offers_each_way_and_a_press_chooses_it() {
    let mfa = sample::listing();
    let mut ui = simulate(&mfa);
    assert!(ui.find("Choose how to confirm it's you.").is_ok());
    ui.click("Use your password").expect("the row");
    ui.click("Use a backup code").expect("the row");
    // The list has no Confirm and no way back to itself.
    press(&mut ui, CONFIRM);
    press(&mut ui, BACK);
    assert_eq!(
        asked(ui),
        [
            Intent::Choose(Method::Password),
            Intent::Choose(Method::Backup)
        ]
    );
}

#[test]
fn typing_writes_and_confirm_waits_for_a_ready_code() {
    let mfa = sample::app();
    let mut ui = simulate(&mfa);
    into_field(&mut ui);
    ui.typewrite("1");
    press(&mut ui, CONFIRM);
    assert_eq!(asked(ui), [Intent::Write("1".into())]);
}

#[test]
fn a_ready_code_confirms_by_button_and_by_enter() {
    let mfa = sample::app_ready();
    let mut ui = simulate(&mfa);
    into_field(&mut ui);
    ui.tap_key(Named::Enter);
    press(&mut ui, CONFIRM);
    assert_eq!(asked(ui), [Intent::Confirm, Intent::Confirm]);
}

#[test]
fn an_unready_code_does_not_confirm_on_enter() {
    let mfa = sample::app();
    let mut ui = simulate(&mfa);
    into_field(&mut ui);
    ui.tap_key(Named::Enter);
    assert_eq!(asked(ui), []);
}

#[test]
fn while_it_is_sent_nothing_is_asked_again() {
    let mfa = sample::busy();
    let mut ui = simulate(&mfa);
    into_field(&mut ui);
    ui.typewrite("7");
    ui.tap_key(Named::Enter);
    press(&mut ui, CONFIRM);
    press(&mut ui, BACK);
    assert_eq!(asked(ui), []);
}

#[test]
fn discords_refusal_shows() {
    let mfa = sample::refused();
    let mut ui = simulate(&mfa);
    assert!(ui.find(mfa.error.as_str()).is_ok());
}

#[test]
fn another_way_goes_back_to_the_list_and_cancel_fails_the_request() {
    let mfa = sample::app();
    let mut ui = simulate(&mfa);
    press(&mut ui, BACK);
    press(&mut ui, CANCEL);
    assert_eq!(asked(ui), [Intent::Back, Intent::Cancel]);
}

#[test]
fn a_text_may_be_sent_again_and_an_app_code_may_not() {
    let mfa = sample::sms();
    let mut ui = simulate(&mfa);
    press(&mut ui, RESEND);
    assert_eq!(asked(ui), [Intent::Resend]);
    let mfa = sample::app();
    let mut ui = simulate(&mfa);
    press(&mut ui, RESEND);
    assert_eq!(asked(ui), []);
}

#[test]
fn with_one_way_there_is_nothing_to_go_back_to() {
    let mfa = sample::warning_alone();
    let mut ui = simulate(&mfa);
    press(&mut ui, BACK);
    assert_eq!(asked(ui), []);
}

#[test]
fn the_warning_button_waits_then_confirms() {
    let mfa = sample::warning();
    let mut ui = simulate(&mfa);
    assert!(ui.find("Slow down! Are you sure?").is_ok());
    press(&mut ui, SURE);
    // No field in the warning.
    into_field(&mut ui);
    ui.typewrite("1");
    assert_eq!(asked(ui), []);
    let mfa = sample::warning_over();
    let mut ui = simulate(&mfa);
    press(&mut ui, SURE);
    assert_eq!(asked(ui), [Intent::Confirm]);
}

#[test]
fn the_warning_counts_its_seconds_in_the_singular_too() {
    let mfa = Mfa {
        wait: 1,
        ..sample::warning()
    };
    let mut ui = simulate(&mfa);
    assert!(
        ui.find(
            "If you do not have access to any other methods, the button below will activate in 1 second."
        )
        .is_ok()
    );
    let mfa = sample::warning();
    let mut ui = simulate(&mfa);
    assert!(
        ui.find(
            "If you do not have access to any other methods, the button below will activate in 10 seconds."
        )
        .is_ok()
    );
}

#[test]
fn a_passkey_alone_has_words_and_no_field_and_no_confirm() {
    let mfa = sample::passkey();
    let mut ui = simulate(&mfa);
    assert!(ui.find(mfa.text.as_str()).is_ok());
    press(&mut ui, CONFIRM);
    press(&mut ui, CANCEL);
    assert_eq!(asked(ui), [Intent::Cancel]);
}
