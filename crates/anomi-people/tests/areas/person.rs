//! About a person: nickname and note: task P11's tests, driving
//! `anomi_people::person::view` over its fixtures in the simulator
//! (`super::support`).

use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced::{Point, Size};
use iced_test::Simulator;

use anomi_people::person::{CANCEL, CONFIRM, field_id, view};
use anomi_vm::fixtures::people::person;
use anomi_vm::people::person::{Intent, Person};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(700.0, 600.0);

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

fn simulate(vm: &Person) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(vm, front()))
}

/// Focus the button and press it with Enter.
fn press_confirm(ui: &mut Ui<'_>) {
    let mut focus = focusable::focus::<()>(Id::new(CONFIRM));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

#[test]
fn the_button_answers_with_the_field() {
    let vm = person::note_typed();
    let mut ui = simulate(&vm);
    press_confirm(&mut ui);
    assert_eq!(asked(ui), [Intent::Answer("Met at the meetup".into())]);
}

#[test]
fn enter_in_the_field_answers_too() {
    let vm = person::note_typed();
    let mut ui = simulate(&vm);
    ui.click(field_id()).expect("the field");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert_eq!(asked(ui), [Intent::Answer("Met at the meetup".into())]);
}

#[test]
fn typing_asks_to_keep_the_text() {
    let vm = person::note();
    let mut ui = simulate(&vm);
    ui.click(field_id()).expect("the field");
    let _ = ui.typewrite("hi");
    assert_eq!(asked(ui).last(), Some(&Intent::Text("hi".into())));
}

#[test]
fn a_dialog_that_only_asks_answers_with_nothing() {
    let vm = person::unfriend();
    assert!(!vm.has_field());
    let mut ui = simulate(&vm);
    assert!(ui.click(field_id()).is_err());
    press_confirm(&mut ui);
    assert_eq!(asked(ui), [Intent::Answer(String::new())]);
}

#[test]
fn nothing_goes_while_it_loads_or_saves() {
    for vm in [person::loading(), person::saving()] {
        let mut ui = simulate(&vm);
        press_confirm(&mut ui);
        let _ = ui.click(field_id());
        ui.tap_key(keyboard::Key::Named(Named::Enter));
        assert!(
            asked(ui)
                .iter()
                .all(|intent| !matches!(intent, Intent::Answer(_))),
            "answered while busy"
        );
    }
    let loading = person::loading();
    assert!(simulate(&loading).find("Loading...").is_ok());
}

#[test]
fn a_loading_field_takes_no_text() {
    let vm = person::loading();
    let mut ui = simulate(&vm);
    let _ = ui.click(field_id());
    let _ = ui.typewrite("x");
    assert!(
        asked(ui)
            .iter()
            .all(|intent| !matches!(intent, Intent::Text(_)))
    );
}

#[test]
fn a_choice_asks_for_its_index() {
    let vm = person::timeout();
    let mut ui = simulate(&vm);
    ui.click("1 hour").expect("a length");
    assert_eq!(asked(ui), [Intent::Choose(3)]);
}

#[test]
fn the_heading_and_the_choices_show() {
    let vm = person::ban();
    let mut ui = simulate(&vm);
    assert!(ui.find("DELETE THEIR MESSAGES FROM").is_ok());
    assert!(ui.find("The last hour").is_ok());
}

#[test]
fn the_box_asks_to_flip() {
    let vm = person::call();
    let mut ui = simulate(&vm);
    ui.click("Don't ask again").expect("the box");
    assert_eq!(asked(ui), [Intent::Tick(true)]);
    let mut ticked = person::call();
    ticked.tick = Some(anomi_vm::people::person::tick("Don't ask again", true));
    let mut ui = simulate(&ticked);
    ui.click("Don't ask again").expect("the box");
    assert_eq!(asked(ui), [Intent::Tick(false)]);
}

#[test]
fn cancel_closes() {
    let vm = person::unfriend();
    let mut ui = simulate(&vm);
    let mut focus = focusable::focus::<()>(Id::new(CANCEL));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn what_it_says_shows() {
    let vm = person::failed();
    let mut ui = simulate(&vm);
    assert!(ui.find("Missing Access").is_ok());
    assert!(ui.find("Note on Bob").is_ok());
    assert!(ui.find("Only you see your note, in Discord too.").is_ok());
    let ok = person::note();
    assert!(simulate(&ok).find("Missing Access").is_err());
}

#[test]
fn the_x_closes() {
    let vm = person::unfriend();
    let mut ui = simulate(&vm);
    ui.point_at(Point::new(406.0, 34.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [Intent::Close]);
}
