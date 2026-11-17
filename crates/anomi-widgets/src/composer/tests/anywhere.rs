//! The field under the window's keys (`crate::focus::scope`), as the shell
//! places it: typing anywhere lands at the end of its draft
//! (`vendor/i-slint-core/yapfast_typing.rs`), Escape nothing in it wanted
//! reaches the window (`escaped()`, `ui/composer-field.slint:130`), and
//! Escape the suggestions take doesn't.

use iced::Widget as _;
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;
use iced::widget::column;

use super::*;
use crate::button::{Kind, action_button};
use crate::focus::{COMPOSER, scope};

/// A button and the field, the window's keys around them, typing going to
/// the field.
fn window(field: Composer<'_, Asked>) -> Ui<'_> {
    let content = column![
        action_button("Messages", Kind::Ghost, front()).on_press(Asked::Browse),
        field.id(COMPOSER),
    ];
    let view = scope(content)
        .on_escape(Asked::Escape)
        .type_into(COMPOSER)
        .boxed();
    Simulator::with_size(Settings::default(), Size::new(WIDTH, 400.0), view)
}

#[test]
fn typing_anywhere_lands_at_the_end_of_the_draft() {
    let mut ui = window(field("Hello", 1));
    ui.typewrite("!");
    let asked = asked(&mut ui);
    assert!(asked.contains(&Asked::Focus(true)), "{asked:?}");
    assert_eq!(asked.last(), Some(&Asked::Edit(1, "Hello!".into(), 6)));
    let mut ui = window(field("Hello", 1));
    ui.simulate([named(Named::Backspace, Modifiers::empty())]);
    assert_eq!(edits(&mut ui), [Asked::Edit(1, "Hell".into(), 4)]);
}

#[test]
fn escape_nothing_in_the_field_wanted_reaches_the_window() {
    let mut ui = window(field("Hello", 1));
    ui.typewrite("!");
    let _ = asked(&mut ui);
    ui.simulate([named(Named::Escape, Modifiers::empty())]);
    assert_eq!(asked(&mut ui), [Asked::Escape]);
    // Showing suggestions, the field keeps it.
    let mut ui = window(field("hi @b", 1).suggesting(true));
    ui.typewrite("o");
    let _ = asked(&mut ui);
    ui.simulate([named(Named::Escape, Modifiers::empty())]);
    assert_eq!(asked(&mut ui), [Asked::Suggest(Suggestion::Dismiss)]);
}
