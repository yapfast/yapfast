//! Typing: each edit reports the whole text and the caret's byte offset, a
//! move of the caret alone its offset, Enter sends and Shift+Enter breaks
//! the line, an input method's text goes in when it commits, a press
//! elsewhere takes the keys, even one another widget took while a command's
//! pills show, and a field without `on_edit` takes nothing.

use iced::advanced::input_method;
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;

use super::*;

#[test]
fn typing_reports_the_text_and_the_caret_in_bytes() {
    let mut ui = simulate(field("", 1));
    focus(&mut ui);
    ui.typewrite("hi @ж");
    let asked = asked(&mut ui);
    assert_eq!(asked.first(), Some(&Asked::Focus(true)), "{asked:?}");
    assert_eq!(asked.last(), Some(&Asked::Edit(1, "hi @ж".into(), 6)));
}

#[test]
fn moving_the_caret_reports_its_offset() {
    let mut ui = simulate(field("héllo", 1));
    focus(&mut ui);
    ui.simulate([named(Named::ArrowLeft, Modifiers::empty())]);
    ui.simulate([named(Named::Home, Modifiers::empty())]);
    ui.simulate([named(Named::End, Modifiers::empty())]);
    let asked = asked(&mut ui);
    assert_eq!(
        asked,
        [
            Asked::Focus(true),
            Asked::Caret(1, 5),
            Asked::Caret(1, 0),
            Asked::Caret(1, 6)
        ]
    );
}

#[test]
fn enter_sends_and_shift_enter_breaks_the_line() {
    let mut ui = simulate(field("", 1));
    focus(&mut ui);
    ui.typewrite("a");
    ui.simulate([named(Named::Enter, Modifiers::SHIFT)]);
    ui.typewrite("b");
    ui.simulate([named(Named::Enter, Modifiers::empty())]);
    let asked: Vec<_> = asked(&mut ui)
        .into_iter()
        .filter(|asked| !matches!(asked, Asked::Focus(_)))
        .collect();
    assert_eq!(
        asked,
        [
            Asked::Edit(1, "a".into(), 1),
            Asked::Edit(1, "a\n".into(), 2),
            Asked::Edit(1, "a\nb".into(), 3),
            Asked::Send
        ]
    );
}

#[test]
fn backspace_and_selection_edit_as_a_text_field_does() {
    let mut ui = simulate(field("hello", 1));
    focus(&mut ui);
    ui.simulate([named(Named::Backspace, Modifiers::empty())]);
    ui.simulate([named(Named::ArrowLeft, Modifiers::SHIFT)]);
    ui.simulate([named(Named::ArrowLeft, Modifiers::SHIFT)]);
    ui.typewrite("p");
    assert_eq!(
        edits(&mut ui),
        [
            Asked::Edit(1, "hell".into(), 4),
            Asked::Edit(1, "hep".into(), 3)
        ]
    );
}

#[test]
fn undo_takes_back_what_was_typed() {
    let mut ui = simulate(field("", 1));
    focus(&mut ui);
    ui.typewrite("ok");
    let _ = asked(&mut ui);
    ui.simulate([ctrl("z", Code::KeyZ)]);
    let undone = edits(&mut ui);
    assert!(
        matches!(undone.last(), Some(Asked::Edit(1, text, _)) if text.len() < 2),
        "{undone:?}"
    );
}

#[test]
fn an_input_method_writes_when_it_commits_and_keeps_its_enter() {
    let mut ui = simulate(field("", 1));
    focus(&mut ui);
    let ime = |event| Event::InputMethod(event);
    ui.simulate([
        ime(input_method::Event::Opened),
        ime(input_method::Event::Preedit("ni".into(), Some(0..2))),
    ]);
    // Enter belongs to the composition: no message goes.
    ui.simulate([named(Named::Enter, Modifiers::empty())]);
    ui.simulate([
        ime(input_method::Event::Preedit(String::new(), None)),
        ime(input_method::Event::Commit("你".into())),
    ]);
    let asked: Vec<_> = asked(&mut ui)
        .into_iter()
        .filter(|asked| !matches!(asked, Asked::Focus(_)))
        .collect();
    assert_eq!(asked, [Asked::Edit(1, "你".into(), 3)]);
}

#[test]
fn a_field_without_on_edit_takes_nothing() {
    let off = composer::<Asked>("hi", front())
        .id(Id::new(FIELD))
        .on_send(Asked::Send);
    let mut ui = simulate(off);
    focus(&mut ui);
    ui.typewrite("x");
    ui.simulate([named(Named::Enter, Modifiers::empty())]);
    assert!(asked(&mut ui).is_empty());
}

#[test]
fn a_click_gives_it_the_keys_and_a_click_elsewhere_takes_them() {
    let mut ui = simulate(field("hello", 1));
    let bounds = ui
        .find(Id::new(FIELD))
        .expect("the field")
        .visible_bounds()
        .expect("shown");
    ui.point_at(bounds.center());
    ui.simulate(iced_test::simulator::click());
    ui.point_at(Point::new(
        bounds.center().x,
        bounds.y + bounds.height + 50.0,
    ));
    ui.simulate(iced_test::simulator::click());
    let asked: Vec<_> = asked(&mut ui)
        .into_iter()
        .filter(|asked| matches!(asked, Asked::Focus(_)))
        .collect();
    assert_eq!(asked, [Asked::Focus(true), Asked::Focus(false)]);
}

#[test]
fn a_press_another_widget_took_takes_the_keys_while_pills_show() {
    use iced::widget::{button, column, space, text};
    let commanding = field("hello", 1)
        .command(Some(Command {
            options: 1,
            ..Command::default()
        }))
        .pills(space().height(26));
    let view = column![button(text("row")).on_press(Asked::Up), commanding];
    let mut ui = Simulator::with_size(Settings::default(), Size::new(WIDTH, 400.0), view);
    focus(&mut ui);
    // The sidebar's row takes the press before the field sees it.
    ui.click("row").expect("the row");
    ui.typewrite("x");
    let asked = asked(&mut ui);
    assert!(asked.contains(&Asked::Up), "the row had it: {asked:?}");
    assert!(asked.contains(&Asked::Focus(false)), "{asked:?}");
    assert!(
        !asked.iter().any(|asked| matches!(asked, Asked::Edit(..))),
        "{asked:?}"
    );
}
