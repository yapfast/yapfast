//! The edit menu: a right click opens it with Paste and Select All, and Cut
//! and Copy over a selection; Cut takes the selection out, Paste asks the
//! app, and Escape or a press outside closes it. Another field's menu takes
//! the keys from the one that had them.

use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;

use super::*;

/// Right-clicks the field's middle.
fn right_click(ui: &mut Ui<'_>) {
    right_click_on(ui, FIELD);
}

/// Right-clicks the middle of the field `id`.
fn right_click_on(ui: &mut Ui<'_>, id: &'static str) {
    let bounds = ui
        .find(Id::new(id))
        .expect("the field")
        .visible_bounds()
        .expect("shown");
    ui.point_at(bounds.center());
    ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Right,
    ))]);
    ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(
        mouse::Button::Right,
    ))]);
}

#[test]
fn select_all_then_cut_empties_the_field() {
    let mut ui = simulate(field("hello", 1));
    right_click(&mut ui);
    assert!(ui.find("Cut").is_err(), "nothing selected yet");
    ui.click("Select All").expect("Select All");
    right_click(&mut ui);
    ui.click("Cut").expect("Cut over the selection");
    let asked = asked(&mut ui);
    assert_eq!(asked.first(), Some(&Asked::Focus(true)), "it took the keys");
    assert_eq!(edits_of(asked), [Asked::Edit(1, String::new(), 0)]);
}

fn edits_of(asked: Vec<Asked>) -> Vec<Asked> {
    asked
        .into_iter()
        .filter(|asked| matches!(asked, Asked::Edit(..)))
        .collect()
}

#[test]
fn paste_asks_the_app() {
    let mut ui = simulate(field("", 1));
    right_click(&mut ui);
    assert!(ui.find("Copy").is_err());
    ui.click("Paste").expect("Paste");
    assert!(asked(&mut ui).contains(&Asked::Paste));
    assert!(ui.find("Paste").is_err(), "closed after the choice");
}

#[test]
fn escape_or_a_press_outside_closes_it() {
    let mut ui = simulate(field("hello", 1));
    right_click(&mut ui);
    assert!(ui.find("Paste").is_ok());
    ui.simulate([named(Named::Escape, Modifiers::empty())]);
    assert!(ui.find("Paste").is_err(), "Escape closed it");
    right_click(&mut ui);
    ui.point_at(Point::new(WIDTH - 5.0, 390.0));
    ui.simulate(iced_test::simulator::click());
    assert!(ui.find("Paste").is_err(), "the press outside closed it");
    // While it was open, a key typed nothing.
    right_click(&mut ui);
    ui.typewrite("x");
    assert!(edits(&mut ui).is_empty());
}

#[test]
fn another_fields_menu_takes_the_keys() {
    const OTHER: &str = "thread composer";
    let fields = iced::widget::column![field("channel", 1), field("thread", 2).id(Id::new(OTHER))];
    let mut ui = Simulator::with_size(Settings::default(), Size::new(WIDTH, 400.0), fields);
    focus(&mut ui);
    right_click_on(&mut ui, OTHER);
    ui.simulate([named(Named::Escape, Modifiers::empty())]);
    ui.typewrite("x");
    let asked = asked(&mut ui);
    assert!(asked.contains(&Asked::Focus(false)), "{asked:?}");
    assert_eq!(
        edits_of(asked),
        [Asked::Edit(2, "threadx".into(), 7)],
        "only the field right-clicked types"
    );
}
