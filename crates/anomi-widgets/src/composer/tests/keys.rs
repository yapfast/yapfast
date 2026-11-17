//! The field's own keys in the simulator: suggestions, an app's command,
//! Up and Alt+Up, Ctrl+G and Ctrl+E, Ctrl+V asking the app and the paste
//! it answers with, and Escape left for the window.

use std::sync::Arc;

use iced::advanced::clipboard::{self, Content};
use iced::event::Status;
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;

use super::*;
use crate::composer::operation::reach_for_tests;

fn tap(ui: &mut Ui<'_>, key: Named, modifiers: Modifiers) -> Status {
    ui.simulate([named(key, modifiers)])[0]
}

/// What was asked, without the focus the first key notices.
fn asked_now(ui: &mut Ui<'_>) -> Vec<Asked> {
    asked(ui)
        .into_iter()
        .filter(|asked| !matches!(asked, Asked::Focus(_) | Asked::Caret(..)))
        .collect()
}

#[test]
fn suggestions_take_the_arrows_tab_enter_and_escape() {
    let mut ui = simulate(field("hi @b", 1).suggesting(true));
    focus(&mut ui);
    let none = Modifiers::empty();
    for key in [
        Named::ArrowDown,
        Named::ArrowUp,
        Named::Tab,
        Named::Enter,
        Named::Escape,
    ] {
        assert_eq!(tap(&mut ui, key, none), Status::Captured, "{key:?}");
    }
    assert_eq!(
        asked_now(&mut ui),
        [
            Asked::Suggest(Suggestion::Move(1)),
            Asked::Suggest(Suggestion::Move(-1)),
            Asked::Suggest(Suggestion::Choose),
            Asked::Suggest(Suggestion::Choose),
            Asked::Suggest(Suggestion::Dismiss),
        ]
    );
}

#[test]
fn escape_and_tab_are_left_for_the_window() {
    let mut ui = simulate(field("hi", 1));
    focus(&mut ui);
    assert_eq!(
        tap(&mut ui, Named::Escape, Modifiers::empty()),
        Status::Ignored
    );
    assert_eq!(
        tap(&mut ui, Named::Tab, Modifiers::empty()),
        Status::Ignored
    );
    assert!(asked_now(&mut ui).is_empty());
}

#[test]
fn a_command_takes_tab_escape_and_backspace_in_its_empty_first_option() {
    let writing = Command {
        options: 2,
        focus: 0,
        wants_file: false,
    };
    let mut ui = simulate(field("", 1).command(Some(writing)));
    focus(&mut ui);
    let none = Modifiers::empty();
    tap(&mut ui, Named::Tab, none);
    tap(&mut ui, Named::Tab, Modifiers::SHIFT);
    tap(&mut ui, Named::Backspace, none);
    tap(&mut ui, Named::Escape, none);
    assert_eq!(
        asked_now(&mut ui),
        [
            Asked::Command(CommandKey::Tab(1)),
            Asked::Command(CommandKey::Tab(-1)),
            Asked::Command(CommandKey::Close { back: true }),
            Asked::Command(CommandKey::Close { back: false }),
        ]
    );
    let file = Command {
        focus: 1,
        wants_file: true,
        ..writing
    };
    let mut ui = simulate(field("", 1).command(Some(file)));
    focus(&mut ui);
    tap(&mut ui, Named::Enter, none);
    assert_eq!(asked_now(&mut ui), [Asked::Command(CommandKey::File(1))]);
}

#[test]
fn escape_closes_the_slash_picker() {
    let mut ui = simulate(field("/", 1).picking(true));
    focus(&mut ui);
    tap(&mut ui, Named::Escape, Modifiers::empty());
    assert_eq!(
        asked_now(&mut ui),
        [Asked::Command(CommandKey::Close { back: false })]
    );
}

#[test]
fn up_edits_the_last_message_from_an_empty_field_and_alt_up_browses() {
    let mut ui = simulate(field("", 1));
    focus(&mut ui);
    tap(&mut ui, Named::ArrowUp, Modifiers::empty());
    assert_eq!(asked_now(&mut ui), [Asked::Up]);
    let mut ui = simulate(field("hi", 1));
    focus(&mut ui);
    tap(&mut ui, Named::ArrowUp, Modifiers::empty());
    tap(&mut ui, Named::ArrowUp, Modifiers::ALT);
    assert_eq!(asked_now(&mut ui), [Asked::Browse]);
}

#[test]
fn ctrl_g_and_ctrl_e_open_the_gifs_and_the_emoji() {
    let mut ui = simulate(field("", 1));
    focus(&mut ui);
    ui.simulate([ctrl("g", Code::KeyG), ctrl("e", Code::KeyE)]);
    assert_eq!(
        asked_now(&mut ui),
        [
            Asked::Expressions(Expressions::Gifs),
            Asked::Expressions(Expressions::Emoji)
        ]
    );
}

fn clipboard_text(text: &str) -> Event {
    Event::Clipboard(clipboard::Event::Read(Ok(Arc::new(Content::Text(
        text.to_owned(),
    )))))
}

#[test]
fn ctrl_v_asks_the_app_which_answers_with_a_paste() {
    let mut ui = simulate(field("hi ", 1));
    focus(&mut ui);
    ui.simulate([ctrl("v", Code::KeyV)]);
    assert_eq!(asked_now(&mut ui), [Asked::Paste]);
    // Text read for anyone else doesn't land in the field.
    ui.simulate([clipboard_text("stray")]);
    assert!(asked_now(&mut ui).is_empty());
    // The app found text: the field asks the clipboard at its next event,
    // and pastes what comes, its line ends made plain.
    ui.operate(&mut reach_for_tests(Id::new(FIELD), None));
    ui.simulate([Event::Window(window::Event::RedrawRequested(
        std::time::Instant::now(),
    ))]);
    ui.simulate([clipboard_text("there\r\nyou")]);
    assert_eq!(
        asked_now(&mut ui),
        [Asked::Edit(1, "hi there\nyou".into(), 12)]
    );
}

#[test]
fn without_on_paste_it_pastes_text_itself() {
    let mut rig_field =
        composer::<Asked>("", front()).on_edit(|text, caret| Asked::Edit(1, text, caret));
    let mut rig = Rig::new(&mut rig_field);
    rig.focus();
    let reply = rig.send(&mut rig_field, ctrl("v", Code::KeyV));
    assert!(reply.captured);
    assert_eq!(reply.reads, 1, "it asks the clipboard for text");
    let reply = rig.send(&mut rig_field, clipboard_text("ok"));
    assert_eq!(reply.asked, [Asked::Edit(1, "ok".into(), 2)]);
}
