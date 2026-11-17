//! A new thread: task C25's tests, driving `anomi_chat::threads::new::view`
//! over its fixtures in the simulator (`super::support`).

use iced::Size;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced_test::Simulator;

use anomi_chat::threads::new::leave::{LEAVE, STAY};
use anomi_chat::threads::new::{CLOSE_ID, CREATE_ID, name_id, view};
use anomi_vm::chat::Intent as Chat;
use anomi_vm::chat::new_thread::{Intent, NewThread};
use anomi_vm::fixtures::chat::new_thread as sample;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(420.0, 860.0);

fn simulate(vm: &NewThread) -> Simulator<'_, Chat, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(vm, front()))
}

fn new(intents: Vec<Chat>) -> Vec<Intent> {
    intents
        .into_iter()
        .map(|intent| match intent {
            Chat::NewThread(intent) => intent,
            other => panic!("not the form's: {other:?}"),
        })
        .collect()
}

fn press(
    ui: &mut Simulator<'_, Chat, anomi_theme::Theme, anomi_theme::Renderer>,
    id: &'static str,
) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

#[test]
fn the_cross_asks_to_close() {
    let vm = sample::blank();
    let mut ui = simulate(&vm);
    press(&mut ui, CLOSE_ID);
    assert_eq!(new(asked(ui)), [Intent::Close]);
}

#[test]
fn typing_a_name_asks_for_it() {
    let vm = sample::blank();
    let mut ui = simulate(&vm);
    ui.click(name_id()).expect("the field");
    let _ = ui.typewrite("Plans");
    assert_eq!(new(asked(ui)).last(), Some(&Intent::Name("Plans".into())));
}

#[test]
fn enter_in_the_name_moves_to_the_message() {
    let vm = sample::written();
    let mut ui = simulate(&vm);
    ui.click(name_id()).expect("the name");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert_eq!(new(asked(ui)), [Intent::ToBody]);
}

#[test]
fn a_click_on_the_switch_label_flips_it() {
    let vm = sample::blank();
    let mut ui = simulate(&vm);
    ui.click("Private Thread").expect("the switch");
    assert_eq!(new(asked(ui)), [Intent::Private(true)]);
    let vm = sample::private();
    let mut ui = simulate(&vm);
    ui.click("Private Thread").expect("the switch");
    assert_eq!(new(asked(ui)), [Intent::Private(false)]);
}

#[test]
fn the_switch_is_gone_without_private_threads() {
    let vm = sample::public_only();
    let mut ui = simulate(&vm);
    assert!(ui.find("Private Thread").is_err());
}

#[test]
fn only_private_cannot_be_flipped() {
    let vm = sample::only_private();
    let mut ui = simulate(&vm);
    ui.click("Private Thread").expect("the label");
    assert_eq!(new(asked(ui)), []);
}

#[test]
fn the_private_text_says_how_to_invite() {
    let vm = sample::private();
    let mut ui = simulate(&vm);
    assert!(ui.find("Only people you invite and moderators can see. You can invite new people by @mentioning them.").is_ok());
    let vm = sample::blank();
    let mut ui = simulate(&vm);
    assert!(
        ui.find("Only people you invite and moderators can see")
            .is_ok()
    );
}

#[test]
fn from_a_message_the_name_is_optional_and_the_message_shows() {
    let vm = sample::from_message();
    let mut ui = simulate(&vm);
    assert!(ui.find("THREAD NAME (OPTIONAL)").is_ok());
    ui.click("Message 3: 👍 sounds good").expect("the message");
    let intents = new(asked(ui));
    assert!(matches!(intents.as_slice(), [Intent::Jump { .. }]));
    let vm = sample::blank();
    let mut ui = simulate(&vm);
    assert!(ui.find("THREAD NAME").is_ok());
    assert!(ui.find("THREAD NAME (OPTIONAL)").is_err());
}

#[test]
fn the_send_button_creates() {
    let vm = sample::written();
    let mut ui = simulate(&vm);
    press(&mut ui, CREATE_ID);
    assert_eq!(new(asked(ui)), [Intent::Create]);
}

#[test]
fn while_starting_nothing_takes_input() {
    let vm = sample::starting();
    let mut ui = simulate(&vm);
    ui.click("Creating...").ok();
    ui.click("Private Thread").ok();
    assert_eq!(new(asked(ui)), []);
}

#[test]
fn a_refusal_shows_under_the_switch() {
    let vm = sample::refused();
    let mut ui = simulate(&vm);
    assert!(ui.find(vm.error.as_str()).is_ok());
}

#[test]
fn the_question_offers_both_answers() {
    let vm = sample::leaving();
    let mut ui = simulate(&vm);
    assert!(ui.find("Are you sure you want to go?").is_ok());
    press(&mut ui, STAY);
    assert_eq!(new(asked(ui)), [Intent::Stay]);
    let mut ui = simulate(&vm);
    press(&mut ui, LEAVE);
    assert_eq!(new(asked(ui)), [Intent::Leave]);
}

#[test]
fn the_question_by_keys() {
    let vm = sample::leaving();
    let mut ui = simulate(&vm);
    press(&mut ui, LEAVE);
    assert_eq!(new(asked(ui)), [Intent::Leave]);
    let mut ui = simulate(&vm);
    press(&mut ui, STAY);
    assert_eq!(new(asked(ui)), [Intent::Stay]);
}

#[test]
fn without_the_question_it_is_not_drawn() {
    let vm = sample::written();
    let mut ui = simulate(&vm);
    assert!(ui.find("Are you sure you want to go?").is_err());
}
