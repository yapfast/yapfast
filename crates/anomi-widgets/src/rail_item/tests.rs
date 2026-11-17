//! The rail item in the simulator: a click on its row sends, a press outside
//! it doesn't, Tab then Space or Enter sends, a still window lands its pill
//! at once and a folder is taller than a server.

use iced::advanced::widget::Id;
use iced::advanced::widget::Operation;
use iced::advanced::widget::operation::Focusable;
use iced::keyboard::{self, key::Named};
use iced::{Color, Event, Point, Rectangle, Size};
use iced_test::{Simulator, simulator};

use anomi_theme::{Renderer, Theme};

use super::*;

type Ui<'a> = Simulator<'a, u8, Theme, Renderer>;

fn front() -> Motion {
    Motion {
        front: true,
        ..Motion::default()
    }
}

fn ui<'a>(view: impl iced::advanced::Widget<u8, Theme, Renderer> + 'a) -> Ui<'a> {
    Simulator::with_size(iced::Settings::default(), Size::new(200.0, 200.0), view)
}

/// What Tab does to the first focusable it meets.
struct Tab;

impl Operation for Tab {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        state.focus();
    }
}

fn sent(ui: Ui<'_>) -> Vec<u8> {
    ui.into_messages().collect()
}

#[test]
fn a_click_on_its_row_sends_and_one_outside_it_does_not() {
    let item = rail_item(Face::Initials("S"), front()).on_press(1);
    let mut simulator = ui(item);
    // The row is 72 wide and 40 tall: its sides count, not only the tile.
    simulator.point_at(Point::new(150.0, 20.0));
    simulator.simulate(simulator::click());
    simulator.point_at(Point::new(36.0, 60.0));
    simulator.simulate(simulator::click());
    assert!(simulator.drain().next().is_none());
    simulator.point_at(Point::new(4.0, 20.0));
    simulator.simulate(simulator::click());
    assert_eq!(sent(simulator), [1]);
}

#[test]
fn a_click_elsewhere_takes_the_focus_away() {
    let item = rail_item(Face::Initials("S"), front()).on_press(5);
    let mut simulator = ui(item);
    simulator.operate(&mut Tab);
    simulator.point_at(Point::new(150.0, 150.0));
    simulator.simulate(simulator::click());
    simulator.tap_key(keyboard::Key::Named(Named::Space));
    simulator.tap_key(keyboard::Key::Named(Named::Enter));
    assert!(sent(simulator).is_empty());
}

#[test]
fn the_menu_key_and_shift_f10_ask_for_the_menu_at_the_item() {
    let item = rail_item(Face::Initials("S"), front())
        .on_press(1)
        .on_menu(|at| if at == Point::new(60.0, 24.0) { 7 } else { 8 });
    let mut simulator = ui(item);
    simulator.tap_key(keyboard::Key::Named(Named::ContextMenu));
    simulator.operate(&mut Tab);
    simulator.tap_key(keyboard::Key::Named(Named::ContextMenu));
    simulator.simulate([Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(Named::F10),
        modified_key: keyboard::Key::Named(Named::F10),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::F10),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::SHIFT,
        text: None,
        repeat: false,
    })]);
    assert_eq!(sent(simulator), [7, 7]);
}

#[test]
fn tab_focuses_it_and_space_or_enter_press_it() {
    let item = rail_item(Face::Initials("S"), front()).on_press(5);
    let mut simulator = ui(item);
    simulator.tap_key(keyboard::Key::Named(Named::Space));
    simulator.operate(&mut Tab);
    simulator.tap_key(keyboard::Key::Named(Named::Space));
    simulator.tap_key(keyboard::Key::Named(Named::Enter));
    simulator.tap_key(keyboard::Key::Named(Named::Escape));
    assert_eq!(sent(simulator), [5, 5]);
}

#[test]
fn an_item_with_nothing_to_press_takes_no_focus() {
    let item = rail_item::<u8>(Face::Initials("S"), front());
    let mut simulator = ui(item);
    simulator.operate(&mut Tab);
    simulator.tap_key(keyboard::Key::Named(Named::Enter));
    assert!(sent(simulator).is_empty());
}

#[test]
fn a_folder_is_taller_than_a_server_and_its_pill_goes_while_open() {
    let closed = Face::Folder {
        open: false,
        tint: Color::WHITE,
        previews: Vec::new(),
    };
    let open = Face::Folder {
        open: true,
        tint: Color::WHITE,
        previews: Vec::new(),
    };
    let server = rail_item::<u8>(Face::Initials("S"), front()).selected(true);
    let folder = rail_item::<u8>(closed, front()).selected(true);
    let opened = rail_item::<u8>(open, front()).selected(true);
    assert_eq!(server.side(), metrics::RAIL_TILE);
    assert_eq!(folder.side(), metrics::FOLDER_TILE);
    assert_eq!(server.pill_target(false), metrics::RAIL_PILL_SELECTED);
    assert_eq!(opened.pill_target(true), 0.0);
    // A folder isn't lit by its server's selection, only by the pointer.
    assert!(!folder.lit(false));
    assert!(folder.lit(true));
    assert!(server.lit(false));
}

#[test]
fn it_draws_marks_and_a_faded_item_without_failing() {
    let item = rail_item::<u8>(Face::Initials("S"), front())
        .selected(true)
        .badge("99+")
        .call(true)
        .discord(true)
        .fade(0.5)
        .on_press(1);
    let mut simulator = ui(item);
    assert!(simulator.snapshot(&Theme::default()).is_ok());
}
