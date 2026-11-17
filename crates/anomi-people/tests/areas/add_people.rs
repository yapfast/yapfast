//! Adding people to a DM: task P13's tests, driving
//! `anomi_people::add_people::view` over its fixtures in the simulator
//! (`super::support`).

use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced::{Point, Size};
use iced_test::{Simulator, simulator};

use anomi_people::add_people::{ADD, query_id, view};
use anomi_vm::fixtures::people::add_people;
use anomi_vm::people::add_people::{AddPeople, Intent};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(600.0, 700.0);

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

/// The add button focused and pressed with Enter (it draws its own text).
fn press_add(ui: &mut Ui<'_>) {
    let mut focus = focusable::focus::<()>(Id::new(ADD));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

fn simulate(vm: &AddPeople) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(vm, front()))
}

#[test]
fn a_click_on_a_friend_chooses_them() {
    let vm = add_people::empty();
    let mut ui = simulate(&vm);
    ui.click("Bob").expect("a row");
    assert_eq!(asked(ui), [Intent::Choose("1400000000000000002".into())]);
}

#[test]
fn a_click_on_a_chosen_one_asks_again_to_unchoose() {
    let vm = add_people::chosen();
    let mut ui = simulate(&vm);
    ui.click("Person 0").expect("a row");
    assert_eq!(asked(ui), [Intent::Choose("1600000000000000000".into())]);
}

#[test]
fn nobody_else_can_be_chosen_when_the_room_is_used() {
    let vm = add_people::at_limit();
    let mut ui = simulate(&vm);
    ui.click("Bob").expect("a row");
    assert!(asked(ui).is_empty());
    // The one chosen still answers, to be taken back.
    let mut ui = simulate(&vm);
    ui.click("Carol").expect("a row");
    assert_eq!(asked(ui), [Intent::Choose("1400000000000000003".into())]);
}

#[test]
fn nothing_is_chosen_while_adding() {
    let vm = add_people::adding();
    let mut ui = simulate(&vm);
    ui.click("Bob").expect("a row");
    ui.click("Person 0").expect("a row");
    assert!(asked(ui).is_empty());
}

#[test]
fn the_button_adds_those_chosen() {
    let vm = add_people::chosen();
    let mut ui = simulate(&vm);
    press_add(&mut ui);
    assert_eq!(asked(ui), [Intent::Add]);
}

#[test]
fn enter_in_the_field_adds_too() {
    let vm = add_people::chosen();
    let mut ui = simulate(&vm);
    ui.click(query_id()).expect("the field");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert_eq!(asked(ui), [Intent::Add]);
}

#[test]
fn nothing_goes_with_none_chosen() {
    let vm = add_people::empty();
    let mut ui = simulate(&vm);
    press_add(&mut ui);
    ui.click(query_id()).expect("the field");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert!(asked(ui).is_empty());
}

#[test]
fn nothing_goes_while_adding() {
    let vm = add_people::adding();
    let mut ui = simulate(&vm);
    press_add(&mut ui);
    ui.click(query_id()).expect("the field");
    ui.tap_key(keyboard::Key::Named(Named::Enter));
    assert!(asked(ui).is_empty());
}

#[test]
fn typing_asks_to_search() {
    let vm = add_people::empty();
    let mut ui = simulate(&vm);
    ui.click(query_id()).expect("the field");
    let _ = ui.typewrite("al");
    assert_eq!(asked(ui).last(), Some(&Intent::Query("al".into())));
}

#[test]
fn the_close_button_asks_to_close() {
    let vm = add_people::empty();
    let mut ui = simulate(&vm);
    // The button draws its own glyph: 10 px from the popover's right edge.
    ui.point_at(Point::new(416.0, 32.0));
    let _ = ui.simulate(simulator::click());
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn the_heading_counts_the_room_left() {
    let empty = add_people::empty();
    assert!(simulate(&empty).find("You can add 8 more friends.").is_ok());
    let chosen = add_people::chosen();
    assert!(
        simulate(&chosen)
            .find("You can add 6 more friends.")
            .is_ok()
    );
    let last = AddPeople {
        room: 3,
        ..add_people::chosen()
    };
    assert!(simulate(&last).find("You can add 1 more friend.").is_ok());
    let none = add_people::no_room();
    assert!(
        simulate(&none)
            .find("This group has no room for more.")
            .is_ok()
    );
}

#[test]
fn the_list_says_why_it_is_empty() {
    let none = add_people::no_match();
    assert!(simulate(&none).find("No friends match.").is_ok());
    let all = add_people::all_here();
    assert!(
        simulate(&all)
            .find("All your friends are here already.")
            .is_ok()
    );
}

#[test]
fn a_failure_shows_above_the_button() {
    let vm = add_people::failed();
    assert!(simulate(&vm).find("Missing Permissions").is_ok());
    let fine = add_people::chosen();
    assert!(simulate(&fine).find("Missing Permissions").is_err());
}
