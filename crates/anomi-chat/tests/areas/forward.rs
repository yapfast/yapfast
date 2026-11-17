//! Forwarding a message: task C28's tests, driving
//! `anomi_chat::forward::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced_test::Simulator;

use anomi_chat::forward::{CLOSE, SEND, note_id, query_id, view};
use anomi_vm::chat::forward::{Forward, Intent};
use anomi_vm::fixtures::chat::forward as sample;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(600.0, 700.0);
type Ui<'a> = Simulator<'a, Intent, anomi_theme::Theme, anomi_theme::Renderer>;

/// Focus a button by its id and press it with Enter.
fn press(ui: &mut Ui<'_>, id: &'static str) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
}

fn simulate(vm: &Forward) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(vm, front()))
}

#[test]
fn pressing_a_place_chooses_it_by_id() {
    let vm = sample::open();
    let mut ui = simulate(&vm);
    ui.click("Bob").expect("Bob's row");
    assert_eq!(asked(ui), [Intent::Choose("discord:d1".into())]);
}

#[test]
fn pressing_a_chosen_place_drops_it() {
    let vm = sample::chosen_two();
    let mut ui = simulate(&vm);
    ui.click("Bob").expect("Bob's row");
    assert_eq!(asked(ui), [Intent::Choose("discord:d1".into())]);
}

#[test]
fn at_the_limit_a_place_left_out_takes_no_press() {
    let vm = sample::at_limit();
    assert!(!vm.can_choose(5));
    let mut ui = simulate(&vm);
    let _ = ui.click("ideas");
    assert_eq!(asked(ui), []);
}

#[test]
fn at_the_limit_a_chosen_place_can_still_be_dropped() {
    let vm = sample::at_limit();
    let mut ui = simulate(&vm);
    ui.click("Carol").expect("Carol's row");
    assert_eq!(asked(ui), [Intent::Choose("discord:d2".into())]);
}

#[test]
fn the_hint_gives_way_to_the_limit() {
    let vm = sample::open();
    assert!(
        simulate(&vm)
            .find("Select where you want to share this message.")
            .is_ok()
    );
    let vm = sample::at_limit();
    assert!(simulate(&vm).find("Maximum 5 places at once.").is_ok());
}

#[test]
fn send_is_dead_until_a_place_is_chosen() {
    let vm = sample::open();
    let mut ui = simulate(&vm);
    press(&mut ui, SEND);
    assert_eq!(asked(ui), []);
}

#[test]
fn send_asks_once_a_place_is_chosen_and_counts_two() {
    let vm = sample::chosen_one();
    let mut ui = simulate(&vm);
    press(&mut ui, SEND);
    assert_eq!(asked(ui), [Intent::Send]);
    let vm = sample::chosen_two();
    assert_eq!(vm.send, "Send (2)");
}

#[test]
fn typing_asks_to_search_and_to_note() {
    let vm = sample::open();
    let mut ui = simulate(&vm);
    ui.click(query_id()).expect("the search");
    let _ = ui.typewrite("Bo");
    assert_eq!(asked(ui).last(), Some(&Intent::Query("Bo".into())));
    let mut ui = simulate(&vm);
    ui.click(note_id()).expect("the note");
    let _ = ui.typewrite("hi");
    assert_eq!(asked(ui).last(), Some(&Intent::Note("hi".into())));
}

#[test]
fn enter_in_the_note_sends_only_with_a_place_chosen() {
    use iced::keyboard::{Key, key::Named};
    let vm = sample::chosen_one();
    let mut ui = simulate(&vm);
    ui.click(note_id()).expect("the note");
    let _ = ui.tap_key(Key::Named(Named::Enter));
    assert_eq!(asked(ui), [Intent::Send]);
    let vm = sample::open();
    let mut ui = simulate(&vm);
    ui.click(note_id()).expect("the note");
    let _ = ui.tap_key(Key::Named(Named::Enter));
    assert_eq!(asked(ui), []);
}

#[test]
fn the_close_button_asks_to_close() {
    let vm = sample::open();
    let mut ui = simulate(&vm);
    press(&mut ui, CLOSE);
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn the_empty_list_says_why() {
    for (vm, words) in [
        (
            sample::nothing_found(),
            "We searched far and wide. Unfortunately, no results were found.",
        ),
        (
            sample::nowhere(),
            "There's nowhere to forward to yet. Yapfast forwards Matrix messages only into encrypted conversations.",
        ),
    ] {
        assert!(simulate(&vm).find(words).is_ok(), "{words}");
    }
}

#[test]
fn slowmode_warns_only_with_a_note_and_the_error_shows() {
    let vm = sample::slowmode();
    assert!(simulate(&vm).find(vm.slowmode.as_str()).is_ok());
    let quiet = Forward {
        note: String::new(),
        ..vm
    };
    assert!(simulate(&quiet).find(quiet.slowmode.as_str()).is_err());
    let vm = sample::failed();
    assert!(simulate(&vm).find("Couldn't forward to Bob.").is_ok());
}

#[test]
fn the_quote_shows_the_message() {
    let vm = sample::open();
    assert!(simulate(&vm).find(vm.quote.as_str()).is_ok());
}
