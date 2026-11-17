//! A message's poll, and the dialog that makes one: task C07's tests, driving
//! `anomi_chat::poll::view` and `poll::dialog` over its fixtures in the
//! simulator (`super::support`).

use iced::Size;
use iced_test::Simulator;

use anomi_chat::poll::{dialog, question_id, view};
use anomi_vm::chat::poll::{Action, Dialog, Intent, Poll};
use anomi_vm::fixtures::chat::poll as fixtures;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(700.0, 600.0);

type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

fn card(poll: &Poll) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, view(poll, front()))
}

fn making(poll: &Dialog) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, dialog(poll, front()))
}

/// What pressing `label` on the card asks for.
fn press(poll: &Poll, label: &str) -> Vec<Intent> {
    let mut ui = card(poll);
    let _ = ui.click(label);
    asked(ui)
}

#[test]
fn an_answer_picks_itself() {
    let key = fixtures::key();
    assert_eq!(press(&fixtures::open(), "Ramen"), [Intent::Pick(key, 2)]);
    // Pressing a picked one asks again: the app unpicks it.
    assert_eq!(press(&fixtures::picked(), "Ramen"), [Intent::Pick(key, 2)]);
}

#[test]
fn voting_is_asked_for_once_an_answer_is_picked() {
    let key = fixtures::key();
    assert!(press(&fixtures::open(), "Vote").is_empty());
    assert_eq!(
        press(&fixtures::picked(), "Vote"),
        [Intent::Act(key, Action::Vote)]
    );
}

#[test]
fn results_can_be_shown_before_voting() {
    assert_eq!(
        press(&fixtures::open(), "Show results"),
        [Intent::ShowResults(fixtures::key())]
    );
    assert!(press(&fixtures::voted(), "Show results").is_empty());
}

#[test]
fn a_vote_can_be_taken_back() {
    assert_eq!(
        press(&fixtures::voted(), "Remove Vote"),
        [Intent::Act(fixtures::key(), Action::RemoveVote)]
    );
}

#[test]
fn in_the_results_an_answer_and_the_count_ask_for_their_voters() {
    let key = fixtures::key();
    assert_eq!(
        press(&fixtures::voted(), "Pizza by the harbour"),
        [Intent::Voters(key, Some(1))]
    );
    assert_eq!(
        press(&fixtures::voted(), "4 votes"),
        [Intent::Voters(key, None)]
    );
}

#[test]
fn a_closed_poll_has_no_button() {
    let poll = fixtures::closed();
    assert!(press(&poll, "Remove Vote").is_empty());
    assert!(press(&poll, "Vote").is_empty());
}

#[test]
fn a_poll_that_is_only_shown_takes_no_press() {
    let poll = fixtures::shown();
    assert!(press(&poll, "Pizza by the harbour").is_empty());
    assert!(press(&poll, "View original message to interact").is_empty());
}

#[test]
fn ending_early_is_made_sure_of() {
    let key = fixtures::key();
    assert_eq!(
        press(&fixtures::confirming(), "End Poll Now"),
        [Intent::End(key)]
    );
    assert_eq!(
        press(&fixtures::confirming(), "Cancel"),
        [Intent::CancelEnd(key)]
    );
    // Not asked: the card has no such buttons.
    assert!(press(&fixtures::open(), "End Poll Now").is_empty());
}

#[test]
fn a_message_without_a_poll_draws_nothing() {
    let poll = Poll::default();
    assert!(press(&poll, "Vote").is_empty());
}

#[test]
fn the_dialog_asks_for_each_of_its_parts() {
    let poll = fixtures::dialog();
    for (label, intent) in [
        ("Add another answer", Intent::AddAnswer),
        ("Allow Multiple Answers", Intent::Multiselect(true)),
        ("Post", Intent::Post),
        ("Cancel", Intent::Close),
        ("1 hour", Intent::Duration(0)),
        ("4 hours", Intent::Duration(1)),
        ("8 hours", Intent::Duration(2)),
        ("1 day", Intent::Duration(3)),
        ("3 days", Intent::Duration(4)),
        ("1 week", Intent::Duration(5)),
        ("2 weeks", Intent::Duration(6)),
    ] {
        let mut ui = making(&poll);
        ui.click(label).expect(label);
        assert_eq!(asked(ui), [intent], "{label}");
    }
}

#[test]
fn the_checkbox_turns_off_too() {
    let poll = fixtures::full();
    // A window tall enough for the whole form, the checkbox under it.
    let mut ui = Simulator::with_size(settings(), Size::new(700.0, 900.0), dialog(&poll, front()));
    ui.click("Allow Multiple Answers").expect("the checkbox");
    assert_eq!(asked(ui), [Intent::Multiselect(false)]);
}

#[test]
fn posting_waits_until_it_can() {
    for poll in [fixtures::fresh(), fixtures::posting()] {
        let mut ui = making(&poll);
        let _ = ui.click("Post");
        let _ = ui.click("Posting...");
        assert!(asked(ui).is_empty());
    }
}

#[test]
fn no_answer_can_be_added_past_ten() {
    let poll = fixtures::full();
    let mut ui = making(&poll);
    assert!(ui.click("Add another answer").is_err());
}

#[test]
fn typing_asks_to_keep_the_question() {
    let poll = fixtures::fresh();
    let mut ui = making(&poll);
    ui.click(question_id()).expect("the field");
    let _ = ui.typewrite("Why");
    assert_eq!(asked(ui).last(), Some(&Intent::Question("Why".into())));
}

#[test]
fn enter_in_the_question_posts() {
    let poll = fixtures::dialog();
    let mut ui = making(&poll);
    ui.click(question_id()).expect("the field");
    ui.tap_key(iced::keyboard::Key::Named(
        iced::keyboard::key::Named::Enter,
    ));
    assert_eq!(asked(ui), [Intent::Post]);
}
