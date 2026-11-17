//! The draft is the view-model's: a new room shows its own draft and never
//! the last one's, a text the app sets replaces the field's, an edit the
//! app hasn't answered yet isn't undone by the view before it, the answer
//! to a send clears it even in the batch of its typing, and the app places
//! the caret by a byte offset.

use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;

use super::*;
use crate::composer::operation::reach_for_tests;

#[test]
fn a_new_room_shows_its_own_draft_and_never_the_last_ones() {
    let mut ui = simulate(field("draft one", 1));
    focus(&mut ui);
    ui.typewrite("!");
    assert_eq!(edits(&mut ui), [Asked::Edit(1, "draft one!".into(), 10)]);
    // The app opens room 2, whose draft is empty, before it took the "!".
    let mut ui = ui.rebuild(field("", 2));
    ui.typewrite("x");
    let asked = asked(&mut ui);
    assert!(
        asked.contains(&Asked::Edit(2, "x".into(), 1)),
        "room 2 gets its own text: {asked:?}"
    );
    assert!(
        !asked
            .iter()
            .any(|asked| matches!(asked, Asked::Edit(1, ..))),
        "nothing more for room 1: {asked:?}"
    );
    // Nothing of room 1's typing to undo here.
    ui.simulate([ctrl("z", Code::KeyZ)]);
    ui.simulate([ctrl("z", Code::KeyZ)]);
    let undone = edits(&mut ui);
    assert!(
        undone
            .iter()
            .all(|asked| matches!(asked, Asked::Edit(2, text, _) if text.is_empty())),
        "{undone:?}"
    );
}

#[test]
fn coming_back_to_a_room_shows_its_draft_with_the_caret_at_the_end() {
    let mut first = field("hello", 1);
    let mut rig = Rig::new(&mut first);
    rig.focus();
    let mut second = field("bonjour @Bo", 2);
    rig.rebuild(&mut second);
    assert_eq!(rig.state().text, "bonjour @Bo");
    // The app hears the caret of the draft it comes back to, for its
    // suggestions.
    let reply = rig.frame(&mut second, std::time::Instant::now());
    assert!(
        reply.asked.contains(&Asked::Caret(2, 11)),
        "{:?}",
        reply.asked
    );
}

#[test]
fn a_text_the_app_sets_replaces_the_fields() {
    let mut ui = simulate(field("hello", 1));
    focus(&mut ui);
    ui.simulate([named(Named::Enter, Modifiers::empty())]);
    assert!(asked(&mut ui).contains(&Asked::Send));
    // Sent: the app clears the draft.
    let mut ui = ui.rebuild(field("", 1));
    ui.typewrite("a");
    assert_eq!(edits(&mut ui), [Asked::Edit(1, "a".into(), 1)]);
}

#[test]
fn a_message_sent_in_the_batch_of_its_typing_leaves_the_field() {
    let mut view = field("", 1);
    let mut rig = Rig::new(&mut view);
    rig.focus();
    rig.type_text(&mut view, "ok");
    let reply = rig.send(&mut view, named(Named::Enter, Modifiers::empty()));
    assert!(reply.asked.contains(&Asked::Send), "{:?}", reply.asked);
    // The app took the edits and the send in one update and cleared the
    // draft: the view after it gives "" as the one before did.
    let mut cleared = field("", 1);
    rig.rebuild(&mut cleared);
    assert_eq!(rig.state().text, "");
    let asked = rig.type_text(&mut cleared, "x");
    assert!(asked.contains(&Asked::Edit(1, "x".into(), 1)), "{asked:?}");
}

#[test]
fn an_edit_the_app_has_not_answered_is_kept() {
    let mut ui = simulate(field("", 1));
    focus(&mut ui);
    ui.typewrite("a");
    // A view built before the app took the "a" still holds the old draft.
    let mut ui = ui.rebuild(field("", 1));
    ui.typewrite("b");
    assert_eq!(
        edits(&mut ui),
        [
            Asked::Edit(1, "a".into(), 1),
            Asked::Edit(1, "ab".into(), 2)
        ]
    );
}

#[test]
fn the_app_places_the_caret_by_a_byte_offset() {
    let mut ui = simulate(field("hé llo", 1));
    focus(&mut ui);
    ui.operate(&mut reach_for_tests(Id::new(FIELD), Some(3)));
    // The window redraws after an operation; the field tells of its caret.
    ui.simulate([Event::Window(window::Event::RedrawRequested(
        std::time::Instant::now(),
    ))]);
    ui.typewrite("X");
    let asked = asked(&mut ui);
    assert!(asked.contains(&Asked::Caret(1, 3)), "{asked:?}");
    assert_eq!(asked.last(), Some(&Asked::Edit(1, "héX llo".into(), 4)));
}

#[test]
fn the_app_inserting_an_emoji_then_placing_the_caret_lands_after_it() {
    let mut ui = simulate(field("hi there", 1));
    focus(&mut ui);
    ui.simulate([named(Named::Home, Modifiers::empty())]);
    let _ = asked(&mut ui);
    // The app put 🙂 at the start, as `emoji_chosen` does, and places the
    // caret after it.
    let mut ui = ui.rebuild(field("🙂hi there", 1));
    ui.operate(&mut reach_for_tests(Id::new(FIELD), Some("🙂".len())));
    ui.typewrite("!");
    assert_eq!(edits(&mut ui), [Asked::Edit(1, "🙂!hi there".into(), 5)]);
}
