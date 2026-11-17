//! The pins popover: task C22's tests, driving `anomi_chat::pins::view` over
//! its fixtures in the simulator (`super::support`).

use iced::{Point, Size};
use iced_test::{Simulator, simulator};

use anomi_chat::pins::view;
use anomi_vm::chat::pins::{Intent, Pins};
use anomi_vm::fixtures::chat::pins as sample;

use super::support;

const SIZE: Size = Size::new(420.0, 600.0);

fn popover(pins: &Pins) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(support::settings(), SIZE, view(pins, support::front()))
}

#[test]
fn a_card_jumps_to_its_message() {
    let pins = sample::one();
    let mut ui = popover(&pins);
    // The card sits under the 48 px heading.
    ui.point_at(Point::new(200.0, 90.0));
    ui.simulate(simulator::click());
    let asked = support::asked(ui);
    assert!(
        matches!(asked.as_slice(), [Intent::Jump { item, .. }] if *item == pins.pins[0].item),
        "{asked:?}"
    );
}

#[test]
fn the_cross_closes() {
    let pins = sample::one();
    let mut ui = popover(&pins);
    ui.point_at(Point::new(396.0, 24.0));
    ui.simulate(simulator::click());
    assert_eq!(support::asked(ui), vec![Intent::Close]);
}

#[test]
fn a_note_stands_for_the_list() {
    let pins = sample::none();
    let mut ui = popover(&pins);
    assert!(ui.find(pins.note.as_str()).is_ok());
    assert!(ui.find("Pinned Messages").is_ok());
}

#[test]
fn each_card_jumps_to_its_own_message() {
    let pins = sample::several();
    let mut ui = popover(&pins);
    // The second card starts 8 px under the first, which ends near 125.
    ui.point_at(Point::new(300.0, 140.0));
    ui.simulate(simulator::click());
    let asked = support::asked(ui);
    assert!(
        matches!(asked.as_slice(), [Intent::Jump { item, .. }] if *item == pins.pins[1].item),
        "{asked:?}"
    );
}
