//! The join dialog: task N12's tests, driving
//! `anomi_screens::places::join::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced_test::Simulator;

use anomi_screens::places::join::{self, view};
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::places::join as samples;
use anomi_vm::places::join::{Intent, Join};

use super::support::{asked, front, settings};

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

fn shown(join: &Join) -> Ui<'_> {
    shown_in(join, 600.0)
}

fn shown_in(join: &Join, width: f32) -> Ui<'_> {
    Simulator::with_size(settings(), Size::new(width, 520.0), view(join, front()))
}

/// Focus the control with this id and press Enter.
fn press(ui: &mut Ui<'_>, id: &'static str) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

#[test]
fn typing_in_the_field_sends_the_link() {
    let join = samples::adding_dialog();
    let mut ui = shown(&join);
    ui.click(iced_test::selector::id(Id::new(join::LINK)))
        .expect("the field");
    ui.typewrite("abc");
    assert_eq!(asked(ui).last(), Some(&Intent::Link("abc".into())));
}

#[test]
fn enter_in_the_field_looks_the_invite_up() {
    let join = samples::found();
    let mut ui = shown(&join);
    ui.click(iced_test::selector::id(Id::new(join::LINK)))
        .expect("the field");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert_eq!(
        asked(ui),
        [Intent::Find("https://discord.gg/yapfast".into())]
    );
}

#[test]
fn enter_waits_while_busy() {
    let join = samples::looking();
    let mut ui = shown(&join);
    ui.click(iced_test::selector::id(Id::new(join::LINK)))
        .expect("the field");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert!(asked(ui).is_empty());
}

#[test]
fn the_main_button_finds_then_joins() {
    let adding = Join {
        link: "x".into(),
        ..samples::adding_dialog()
    };
    let mut ui = shown(&adding);
    press(&mut ui, "dialog-confirm");
    assert_eq!(asked(ui), [Intent::Find("x".into())]);

    let found = samples::found();
    let mut ui = shown(&found);
    press(&mut ui, "dialog-confirm");
    assert_eq!(asked(ui), [Intent::Join]);

    let form = samples::form();
    let mut ui = shown(&form);
    press(&mut ui, "dialog-confirm");
    assert_eq!(asked(ui), [Intent::OpenForm]);
}

/// A pointer press at the centre of the control with this id.
fn click_id(ui: &mut Ui<'_>, id: &'static str) {
    ui.click(iced_test::selector::id(Id::new(id))).unwrap();
}

#[test]
fn the_card_widens_so_the_captcha_buttons_can_be_pressed() {
    let captcha = samples::captcha();
    // Busy hides the button from the selector, so ask the same card idle.
    let joining = Join {
        busy: false,
        ..samples::joining()
    };
    let confirm_width = |join: &Join| {
        let mut ui = shown_in(join, 800.0);
        ui.find(iced_test::selector::id(Id::new("dialog-confirm")))
            .expect("the main button")
            .bounds()
            .width
    };
    assert_eq!(confirm_width(&captcha), confirm_width(&joining));
    let mut ui = shown_in(&captcha, 800.0);
    click_id(&mut ui, "dialog-confirm");
    assert_eq!(asked(ui), [Intent::Join]);
    let mut ui = shown_in(&captcha, 800.0);
    click_id(&mut ui, join::BROWSER);
    assert_eq!(asked(ui), [Intent::JoinInBrowser]);
}

#[test]
fn the_main_button_waits_while_busy() {
    let joining = samples::joining();
    let mut ui = shown(&joining);
    press(&mut ui, "dialog-confirm");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_other_buttons_ask_for_their_own() {
    let join = Join {
        captcha: true,
        ..samples::found()
    };
    let mut ui = shown(&join);
    press(&mut ui, join::CAPTCHA);
    press(&mut ui, join::BROWSER);
    press(&mut ui, "dialog-cancel");
    assert_eq!(
        asked(ui),
        [Intent::OpenCaptcha, Intent::JoinInBrowser, Intent::Close]
    );
}

#[test]
fn the_risk_and_the_browser_button_show_only_with_a_server_found() {
    let risk = "Discord can take a join made from an app other than its own for suspicious and sign you out on every device. Joining in your browser avoids that.";
    let adding = samples::adding_dialog();
    let mut ui = shown(&adding);
    assert!(ui.find("INVITE LINK").is_ok());
    assert!(ui.find(risk).is_err());
    assert!(
        ui.find(iced_test::selector::id(Id::new(join::BROWSER)))
            .is_err()
    );
    let from_link = samples::from_link();
    let mut ui = shown(&from_link);
    assert!(ui.find("INVITE LINK").is_err());
    let found = samples::found();
    let mut ui = shown(&found);
    assert!(ui.find(risk).is_ok());
    assert!(
        ui.find(iced_test::selector::id(Id::new(join::BROWSER)))
            .is_ok()
    );
    let joining = samples::joining();
    let mut ui = shown(&joining);
    assert!(ui.find("Server").is_ok());
    assert!(ui.find("Joining...").is_ok());
    assert!(ui.find(risk).is_err());
    assert!(
        ui.find(iced_test::selector::id(Id::new(join::BROWSER)))
            .is_err()
    );
}
