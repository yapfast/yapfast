//! Presses: a double press selects a word and a triple one a line; a click
//! selects nothing and clears what was; Escape clears it; a right press
//! tells the menu what it would copy; a link opens on a click, not after a
//! drag from it.

use iced::keyboard::key::Named;
use iced::{Point, mouse};

use super::*;

/// The middle of the first word of a text at `bounds`.
fn first_word(bounds: Rectangle) -> Point {
    Point::new(bounds.x + 12.0, bounds.center_y())
}

#[test]
fn a_double_press_selects_a_word_and_a_triple_press_a_line() {
    let chat = Chat::new(20, &[("Sam", "Thinking **co-op** after nine.")]);
    let mut ui = simulate(chat.view(front()));
    let body = bounds(&mut ui, "Thinking co-op after nine.");
    let at = first_word(body);
    click(&mut ui, at);
    assert_eq!(copy(&mut ui), None, "a click selects nothing");
    click(&mut ui, at);
    assert_eq!(copy(&mut ui).as_deref(), Some("Thinking"));
    click(&mut ui, at);
    assert_eq!(copy(&mut ui).as_deref(), Some("Thinking co-op after nine."));
}

#[test]
fn a_click_clears_the_selection_and_escape_does_too() {
    let chat = Chat::new(20, &[("Sam", "First."), ("Riley", "Second.")]);
    let mut ui = simulate(chat.view(front()));
    let second = bounds(&mut ui, "Second.");
    drag(&mut ui, end_of(second), &[Point::new(300.0, 4.0)]);
    assert!(copy(&mut ui).is_some());
    // A press anywhere, here on nothing, clears it.
    click(&mut ui, Point::new(300.0, 4.0));
    assert_eq!(copy(&mut ui), None);

    drag(&mut ui, end_of(second), &[Point::new(300.0, 4.0)]);
    assert!(copy(&mut ui).is_some());
    ui.tap_key(Named::Escape);
    assert_eq!(copy(&mut ui), None);
}

#[test]
fn a_right_press_tells_the_menu_the_selection_under_it() {
    let chat = Chat::new(20, &[("Sam", "First."), ("Riley", "Second.")]);
    let mut ui = simulate(chat.view(front()));
    let first = bounds(&mut ui, "First.");
    let second = bounds(&mut ui, "Second.");
    // "Second." alone.
    drag(
        &mut ui,
        end_of(second),
        &[Point::new(second.x - 4.0, second.center_y())],
    );
    let _ = ui.drain().count();
    move_to(&mut ui, second.center());
    press(&mut ui, mouse::Button::Right);
    release(&mut ui, mouse::Button::Right);
    move_to(&mut ui, first.center());
    press(&mut ui, mouse::Button::Right);
    release(&mut ui, mouse::Button::Right);
    assert_eq!(
        ui.drain().collect::<Vec<_>>(),
        [Asked::Menu(Some("Second.".into())), Asked::Menu(None)]
    );
}

#[test]
fn a_link_opens_on_a_click_but_not_after_a_drag_from_it() {
    let chat = Chat::new(20, &[("Sam", "See https://example.com/yapfast now")]);
    let link = Link::Url("https://example.com/yapfast".into());
    let mut ui = simulate(chat.view(front()));
    let body = bounds(&mut ui, "See https://example.com/yapfast now");
    // Past "See ", on the address.
    let on_link = Point::new(body.x + 60.0, body.center_y());
    click(&mut ui, on_link);
    assert_eq!(ui.drain().collect::<Vec<_>>(), [Asked::Open(link)]);

    // Elsewhere on the address, so it's no double press.
    drag(
        &mut ui,
        Point::new(body.x + 100.0, body.center_y()),
        &[Point::new(body.x + 180.0, body.center_y())],
    );
    let asked: Vec<Asked> = ui.drain().collect();
    assert!(
        matches!(asked.as_slice(), [Asked::Selected(selected)] if !selected.is_empty()),
        "{asked:?}"
    );
}

#[test]
fn a_press_on_a_name_starts_a_selection() {
    let chat = Chat::new(20, &[("Sam", "First."), ("Riley", "Second.")]);
    let mut ui = simulate(chat.view(front()));
    let name = bounds(&mut ui, "Riley");
    let second = bounds(&mut ui, "Second.");
    click(&mut ui, name.center());
    assert_eq!(copy(&mut ui), None, "a click on a name selects nothing");
    // Elsewhere on it, so it's no double press, and down past its text.
    drag(
        &mut ui,
        Point::new(name.x + 2.0, name.center_y()),
        &[end_of(second)],
    );
    assert_eq!(copy(&mut ui).as_deref(), Some("Riley 19:21\nSecond."));
}

#[test]
fn an_autoscroll_takes_the_press_that_ends_it_and_keeps_escape() {
    let chat = Chat::many(40);
    let mut ui = simulate(chat.view(front()));
    let lowest = bounds(&mut ui, "Message 39.");
    let above = bounds(&mut ui, "Message 37.");
    drag(
        &mut ui,
        end_of(lowest),
        &[Point::new(lowest.x - 4.0, lowest.center_y())],
    );
    assert_eq!(copy(&mut ui).as_deref(), Some("Message 39."));
    let middle_click = |ui: &mut Ui<'_>| {
        move_to(ui, above.center());
        press(ui, mouse::Button::Middle);
        release(ui, mouse::Button::Middle);
    };
    // Escape ends the autoscroll and does nothing else.
    middle_click(&mut ui);
    ui.tap_key(Named::Escape);
    assert_eq!(copy(&mut ui).as_deref(), Some("Message 39."));
    // The press ending it clears the selection and starts none.
    middle_click(&mut ui);
    drag(&mut ui, above.center(), &[end_of(lowest)]);
    assert_eq!(copy(&mut ui), None);
    let _ = ui.drain().count();
}

#[test]
fn a_press_off_the_text_starts_nothing() {
    let chat = Chat::new(20, &[("Sam", "First.")]);
    let mut ui = simulate(chat.view(front()));
    // Above every row, on no text: the window's own selection runs, and
    // leaves the rows alone.
    let first = bounds(&mut ui, "First.");
    drag(
        &mut ui,
        Point::new(300.0, 4.0),
        &[Point::new(first.center_x(), first.center_y())],
    );
    assert_eq!(copy(&mut ui), None);
    assert!(ui.drain().next().is_none());
}
