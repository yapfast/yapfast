//! Tab and Shift+Tab: through a dialog and round, its field's text selected
//! as Tab reaches it, Enter pressing the button reached; with nothing
//! focused, Tab goes to the composer and types nothing.

use iced::event::Status;
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;

use super::super::{COMPOSER, SEARCH, next, previous};
use super::*;

fn tab(ui: &mut Ui<'_>) -> Vec<Status> {
    ui.simulate([named(Named::Tab, Modifiers::empty())])
}

fn back_tab(ui: &mut Ui<'_>) -> Vec<Status> {
    ui.simulate([named(Named::Tab, Modifiers::SHIFT)])
}

fn dialog() -> Window {
    Window {
        dialog: true,
        ..Window::default()
    }
}

#[test]
fn tab_goes_through_a_dialog_and_round() {
    let mut ui = simulate(window(dialog()));
    // Nothing in the dialog has the focus, and the composer under it is out
    // of reach: Tab goes to the dialog's first.
    assert_eq!(tab(&mut ui), [Status::Captured]);
    assert_eq!(focused(&mut ui), Some(Id::new("name")));
    tab(&mut ui);
    assert_eq!(focused(&mut ui), Some(Id::new("cancel")));
    tab(&mut ui);
    assert_eq!(focused(&mut ui), Some(Id::new("save")));
    tab(&mut ui);
    assert_eq!(
        focused(&mut ui),
        Some(Id::new("name")),
        "round to the first"
    );
    back_tab(&mut ui);
    assert_eq!(
        focused(&mut ui),
        Some(Id::new("save")),
        "and back to the last"
    );
    // Enter presses the button Tab reached.
    ui.tap_key(Named::Enter);
    assert_eq!(asked(&mut ui), [Asked::Save]);
}

#[test]
fn a_field_tab_reaches_has_its_text_selected() {
    let mut ui = simulate(window(dialog()));
    tab(&mut ui);
    ui.typewrite("x");
    assert_eq!(asked(&mut ui), [Asked::Name("x".into())]);
}

#[test]
fn tab_with_nothing_focused_goes_to_the_composer_and_types_nothing() {
    let mut ui = simulate(window(Window::default()));
    assert_eq!(tab(&mut ui), [Status::Captured]);
    assert_eq!(focused(&mut ui), Some(COMPOSER));
    assert!(asked(&mut ui).is_empty());
    // Its caret is at the end, not over its text.
    ui.typewrite("!");
    assert_eq!(asked(&mut ui), [Asked::Composer("Hello!".into())]);
    // From there, Tab goes on through the window.
    tab(&mut ui);
    assert_eq!(focused(&mut ui), Some(SEARCH));
}

#[test]
fn shift_tab_with_nothing_focused_goes_to_the_last() {
    let mut ui = simulate(window(Window::default()));
    back_tab(&mut ui);
    assert_eq!(focused(&mut ui), Some(SEARCH));
}

#[test]
fn tab_with_nowhere_to_type_goes_to_the_first() {
    let mut ui = simulate(window(Window {
        keys: false,
        ..Window::default()
    }));
    tab(&mut ui);
    assert_eq!(focused(&mut ui), Some(Id::new("filter")));
}

#[test]
fn the_apps_walk_is_tabs() {
    use super::super::walk::{Walk, run};

    // `next` and `previous` run this; their tasks need a runtime.
    let _ = (next::<Asked>(), previous::<Asked>());
    let mut ui = simulate(window(dialog()));
    let walk = |ui: &mut Ui<'_>, back| {
        run(Box::new(Walk::new(back)), &mut |operation| {
            ui.operate(operation);
        });
    };
    walk(&mut ui, true);
    assert_eq!(focused(&mut ui), Some(Id::new("save")));
    walk(&mut ui, false);
    assert_eq!(focused(&mut ui), Some(Id::new("name")));
}
