//! Adding a friend: task P03's tests, driving
//! `anomi_people::friends::add::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced_test::Simulator;

use anomi_people::friends::add::{CLOSE, SEND, name_id, view};
use anomi_vm::fixtures::people::add_friend;
use anomi_vm::people::add_friend::{AddFriend, Intent};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(600.0, 400.0);

fn simulate(vm: &AddFriend) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(vm, front()))
}

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

/// Focus the Send request button and press it with Enter.
fn press_send(ui: &mut Ui<'_>) {
    let mut focus = focusable::focus::<()>(Id::new(SEND));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

#[test]
fn the_button_sends_the_typed_name() {
    let vm = add_friend::typed();
    let mut ui = simulate(&vm);
    press_send(&mut ui);
    assert_eq!(asked(ui), [Intent::Send("bob".into())]);
}

#[test]
fn enter_in_the_field_sends_it_too() {
    let vm = add_friend::typed();
    let mut ui = simulate(&vm);
    ui.click(name_id()).expect("the field");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert_eq!(asked(ui), [Intent::Send("bob".into())]);
}

#[test]
fn nothing_goes_without_a_name() {
    let vm = add_friend::empty();
    let mut ui = simulate(&vm);
    press_send(&mut ui);
    ui.click(name_id()).expect("the field");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert!(asked(ui).is_empty());
}

#[test]
fn nothing_goes_while_one_is_sending() {
    let vm = add_friend::sending();
    let mut ui = simulate(&vm);
    press_send(&mut ui);
    ui.click(name_id()).expect("the field");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert!(ui.find("Sending...").is_ok());
    assert!(asked(ui).is_empty());
}

#[test]
fn typing_asks_to_keep_the_name() {
    let vm = add_friend::empty();
    let mut ui = simulate(&vm);
    ui.click(name_id()).expect("the field");
    let _ = ui.typewrite("bo");
    assert_eq!(asked(ui).last(), Some(&Intent::Name("bo".into())));
}

#[test]
fn the_close_button_asks_to_close() {
    let vm = add_friend::empty();
    let mut ui = simulate(&vm);
    let mut focus = focusable::focus::<()>(Id::new(CLOSE));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn what_it_says_shows_under_the_field() {
    let sent = add_friend::sent();
    assert!(simulate(&sent).find("Sent bob a friend request.").is_ok());
    let failed = add_friend::failed();
    assert!(simulate(&failed).find("Unknown user").is_ok());
    let empty = add_friend::empty();
    assert!(simulate(&empty).find("Unknown user").is_err());
}
