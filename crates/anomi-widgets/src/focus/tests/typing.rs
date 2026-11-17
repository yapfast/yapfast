//! Keys nothing took: typing anywhere goes to the composer used last, with
//! its caret at the end, and not from under a layer; Escape undoes what's
//! being written once nothing above it wanted it.

use iced::keyboard::key::{Code, Named};
use iced::keyboard::{Key, Modifiers};

use super::super::{ASIDE_COMPOSER, COMPOSER, TIMELINE};
use super::*;

#[test]
fn typing_with_nothing_focused_goes_to_the_composer() {
    let mut ui = simulate(window(Window::default()));
    assert_eq!(focused(&mut ui), None);
    ui.typewrite("!");
    assert_eq!(
        asked(&mut ui),
        [Asked::Composer("Hello!".into())],
        "the text goes on the end"
    );
    assert_eq!(focused(&mut ui), Some(COMPOSER), "the composer has it now");
}

#[test]
fn a_focused_widget_that_doesnt_write_lets_typing_go_to_the_composer() {
    let mut ui = simulate(window(Window::default()));
    focus(&mut ui, Id::new("filter"));
    ui.typewrite("a");
    assert_eq!(
        asked(&mut ui),
        [Asked::Filter("a".into())],
        "a field keeps it"
    );
    focus(&mut ui, TIMELINE);
    ui.typewrite("b");
    assert_eq!(asked(&mut ui), [Asked::Composer("Hellob".into())]);
}

#[test]
fn backspace_delete_and_space_edit_the_end_of_the_draft() {
    let mut ui = simulate(window(Window::default()));
    ui.simulate([named(Named::Backspace, Modifiers::empty())]);
    assert_eq!(asked(&mut ui), [Asked::Composer("Hell".into())]);

    let mut ui = simulate(window(Window::default()));
    ui.simulate([named(Named::Space, Modifiers::empty())]);
    assert_eq!(asked(&mut ui), [Asked::Composer("Hello ".into())]);

    // Delete reaches it too, at the end, where it removes nothing.
    let mut ui = simulate(window(Window::default()));
    ui.simulate([named(Named::Delete, Modifiers::empty())]);
    assert_eq!(focused(&mut ui), Some(COMPOSER));
}

#[test]
fn enter_held_keys_and_keys_that_write_nothing_stay_where_they_are() {
    let mut ui = simulate(window(Window::default()));
    let x = |modifiers| press(Key::Character("x".into()), Code::KeyX, modifiers, Some("x"));
    ui.simulate([
        // Enter doesn't send, unlike Discord's: a stray one after a menu
        // closes shouldn't.
        named(Named::Enter, Modifiers::empty()),
        named(Named::ArrowDown, Modifiers::empty()),
        x(Modifiers::CTRL),
        x(Modifiers::ALT),
        x(Modifiers::LOGO),
    ]);
    assert!(asked(&mut ui).is_empty());
    assert_eq!(focused(&mut ui), None);
}

#[test]
fn typing_doesnt_reach_the_composer_from_under_a_layer() {
    let mut ui = simulate(window(Window {
        dialog: true,
        ..Window::default()
    }));
    ui.typewrite("x");
    ui.simulate([named(Named::Backspace, Modifiers::empty())]);
    assert!(asked(&mut ui).is_empty());
    assert_eq!(focused(&mut ui), None);
}

#[test]
fn typing_goes_where_the_window_says() {
    // Beside a thread whose composer was used last, to that one.
    let mut ui = simulate(window(Window {
        aside: true,
        ..Window::default()
    }));
    ui.typewrite("a");
    assert_eq!(asked(&mut ui), [Asked::Aside("a".into())]);

    // With nowhere to go, the key stays where it is.
    let mut ui = simulate(window(Window {
        keys: false,
        ..Window::default()
    }));
    ui.typewrite("a");
    assert!(asked(&mut ui).is_empty());
}

#[test]
fn a_key_that_talks_while_held_doesnt_type() {
    let mut ui = simulate(window(Window {
        talk: true,
        ..Window::default()
    }));
    let key = |letter: &str, code| {
        press(
            Key::Character(letter.into()),
            code,
            Modifiers::empty(),
            Some(letter),
        )
    };
    ui.simulate([key("v", Code::KeyV)]);
    assert!(asked(&mut ui).is_empty());
    assert_eq!(focused(&mut ui), None);
    ui.simulate([key("w", Code::KeyW)]);
    assert_eq!(asked(&mut ui), [Asked::Composer("Hellow".into())]);
}

#[test]
fn stepping_through_the_messages_only_letters_and_digits_leave_them() {
    let mut ui = simulate(window(Window::default()));
    focus(&mut ui, TIMELINE);
    ui.typewrite("?");
    ui.simulate([named(Named::Backspace, Modifiers::empty())]);
    assert!(asked(&mut ui).is_empty());
    assert_eq!(focused(&mut ui), Some(TIMELINE));
    ui.typewrite("7");
    assert_eq!(asked(&mut ui), [Asked::Composer("Hello7".into())]);
    assert_eq!(focused(&mut ui), Some(COMPOSER));
}

#[test]
fn escape_nothing_took_undoes_what_is_being_written() {
    let mut ui = simulate(window(Window::default()));
    ui.tap_key(Named::Escape);
    assert_eq!(asked(&mut ui), [Asked::Escape]);
    // From a field too: iced's lets the key go, and the focus with it, as
    // Escape is its `Unfocus` binding (`core/src/text/editor.rs:906` at the
    // pin; `widget/src/text_input.rs` has no arm of its own).
    focus(&mut ui, COMPOSER);
    ui.tap_key(Named::Escape);
    assert_eq!(asked(&mut ui), [Asked::Escape]);
    assert_eq!(focused(&mut ui), None);

    // A layer has it first: it closes, and that's all.
    let mut ui = ui.rebuild(window(Window {
        dialog: true,
        ..Window::default()
    }));
    ui.tap_key(Named::Escape);
    assert_eq!(asked(&mut ui), [Asked::Close]);
}

#[test]
fn moving_the_focus_from_the_app_reaches_only_what_is_in_reach() {
    use super::super::reach::{Caret, Reach};
    use super::super::walk::run;

    fn reach(ui: &mut Ui<'_>, id: Id, caret: Caret) {
        run(Box::new(Reach::new(id, caret)), &mut |operation| {
            ui.operate(operation);
        });
    }

    // `into`: the caret at the end.
    let mut ui = simulate(window(Window::default()));
    reach(&mut ui, COMPOSER, Caret::End);
    ui.typewrite("!");
    assert_eq!(asked(&mut ui), [Asked::Composer("Hello!".into())]);
    // `select`: what it holds is replaced.
    reach(&mut ui, COMPOSER, Caret::All);
    ui.typewrite("x");
    assert_eq!(asked(&mut ui), [Asked::Composer("x".into())]);

    // Under a dialog it isn't found, and the dialog's field keeps the focus.
    let mut ui = ui.rebuild(window(Window {
        dialog: true,
        ..Window::default()
    }));
    focus(&mut ui, Id::new("name"));
    reach(&mut ui, COMPOSER, Caret::End);
    assert_eq!(focused(&mut ui), Some(Id::new("name")));
    // A widget that isn't there neither.
    reach(&mut ui, ASIDE_COMPOSER, Caret::Kept);
    assert_eq!(focused(&mut ui), Some(Id::new("name")));
}
