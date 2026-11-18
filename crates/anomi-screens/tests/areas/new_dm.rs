//! The new DM and group DM dialog: task N11's tests, driving
//! `anomi_screens::new_dm::view` over its fixtures in the simulator
//! (`super::support`).

use iced::{Point, Size};
use iced_test::Simulator;

use anomi_screens::new_dm::{query_id, view};
use anomi_vm::fixtures::new_dm;
use anomi_vm::new_dm::{Intent, NewDm};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(600.0, 700.0);

fn simulate(vm: &NewDm) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(vm, front()))
}

/// Presses by point: a button's face reports no label to the selector.
fn click_at(ui: &mut Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer>, at: Point) {
    ui.point_at(at);
    ui.simulate(iced_test::simulator::click());
}

/// The first row's Message button and the close button. The simulator lays
/// `view` out at the window's top-left, so the card's corner is (0, 0).
const MESSAGE: Point = Point::new(390.0, 132.0);
const CLOSE: Point = Point::new(406.0, 34.0);

#[test]
fn the_message_button_starts_that_person() {
    let vm = new_dm::friends();
    let mut ui = simulate(&vm);
    click_at(&mut ui, MESSAGE);
    assert_eq!(asked(ui), [Intent::Start("bob".into())]);
}

#[test]
fn pointing_at_a_row_starts_that_person() {
    let vm = new_dm::friends();
    let mut ui = simulate(&vm);
    ui.click("Person 2").expect("Person 2's row");
    assert_eq!(asked(ui), [Intent::Start("person-2".into())]);
}

#[test]
fn typing_in_the_field_asks_to_search() {
    let vm = new_dm::friends();
    let mut ui = simulate(&vm);
    ui.click(query_id()).expect("the field");
    let _ = ui.typewrite("Bo");
    assert_eq!(
        asked(ui).last(),
        Some(&Intent::Query("Bo".into())),
        "the last edit holds what was typed"
    );
}

#[test]
fn the_close_button_asks_to_close() {
    let vm = new_dm::friends();
    let mut ui = simulate(&vm);
    click_at(&mut ui, CLOSE);
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn nothing_starts_while_one_is_starting() {
    let vm = new_dm::starting();
    let mut ui = simulate(&vm);
    let _ = ui.click("Bob");
    click_at(&mut ui, MESSAGE);
    assert!(ui.find("Starting the conversation...").is_ok());
    assert_eq!(asked(ui), []);
}

#[test]
fn the_note_shows_under_the_rows_or_alone() {
    for (vm, note) in [
        (
            new_dm::hint(),
            "Type a name to find people you know on Discord.",
        ),
        (new_dm::nobody(), "No one found."),
        (new_dm::failed(), "Couldn't start the conversation."),
    ] {
        let mut ui = simulate(&vm);
        assert!(ui.find(note).is_ok(), "{note}");
    }
}

#[test]
fn the_placeholder_says_matrix_ids_may_be_typed() {
    let with_matrix = NewDm {
        matrix: true,
        ..NewDm::default()
    };
    let mut ui = simulate(&with_matrix);
    assert!(ui.find("Find a friend, or type a Matrix id").is_ok());
    let plain = NewDm::default();
    let mut ui = simulate(&plain);
    assert!(ui.find("Find a friend").is_ok());
}
