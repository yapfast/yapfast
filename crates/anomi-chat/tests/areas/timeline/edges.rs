//! What the timeline shows without messages, and at the edges of those
//! loaded: the start, the welcome, the button that loads more, its loading
//! and its Retry.

use anomi_vm::chat::timeline::{Edge, Intent, Timeline};
use anomi_vm::fixtures::chat::timeline as fixtures;

use super::{asked, shown, shows};

#[test]
fn with_no_place_chosen_it_says_to_pick_one() {
    let timeline = Timeline::default();
    let mut ui = shown(&timeline);
    assert!(shows(&mut ui, "Pick a conversation"));
    assert!(shows(
        &mut ui,
        "Choose a DM or a server channel on the left."
    ));
}

#[test]
fn a_room_whose_history_hasnt_come_shows_no_start() {
    let timeline = fixtures::loading();
    let mut ui = shown(&timeline);
    assert!(!shows(&mut ui, "Pick a conversation"));
    assert!(!shows(&mut ui, "Welcome to #general"));
}

#[test]
fn a_conversation_without_messages_opens_with_its_start() {
    let timeline = fixtures::empty_dm();
    let mut ui = shown(&timeline);
    assert!(shows(&mut ui, "Person 5"));
    assert!(shows(
        &mut ui,
        "This is the beginning of your messages with Person 5."
    ));
    let timeline = fixtures::empty_channel();
    let mut ui = shown(&timeline);
    assert!(shows(&mut ui, "Welcome to #Lounge"));
    assert!(shows(
        &mut ui,
        "This is the start of #Lounge. No messages here yet."
    ));
    let timeline = fixtures::empty_thread();
    let mut ui = shown(&timeline);
    assert!(shows(&mut ui, "Plans"));
    assert!(shows(
        &mut ui,
        "This is the start of the Plans thread. No messages here yet."
    ));
}

#[test]
fn the_first_message_of_a_dm_has_its_start_and_its_day_over_it() {
    let timeline = fixtures::person_chat();
    let mut ui = shown(&timeline);
    assert!(shows(
        &mut ui,
        "This is the beginning of your messages with Person 5."
    ));
    assert!(shows(&mut ui, "1 October 2026"));
    assert!(!shows(&mut ui, "Earlier messages"));
}

#[test]
fn a_server_channel_opens_with_discords_welcome() {
    let timeline = fixtures::welcome_to_server();
    let mut ui = shown(&timeline);
    assert!(shows(&mut ui, "Welcome to"));
    assert!(shows(&mut ui, "Server"));
    assert!(shows(&mut ui, "This is the beginning of this server."));
    let timeline = fixtures::welcome_to_channel();
    let mut ui = shown(&timeline);
    assert!(shows(&mut ui, "Welcome to #plans!"));
    assert!(shows(&mut ui, "This is the start of the #plans channel."));
}

#[test]
fn the_edge_loads_earlier_messages_and_tries_again_after_a_failure() {
    let timeline = fixtures::start_of_general(Edge::More);
    let mut ui = shown(&timeline);
    ui.click("Earlier messages").expect("the button");
    assert_eq!(asked(&mut ui), [Intent::Earlier]);

    let timeline = fixtures::start_of_general(Edge::Failed);
    let mut ui = shown(&timeline);
    ui.click("Couldn't load earlier messages. Retry")
        .expect("the button");
    assert_eq!(asked(&mut ui), [Intent::Earlier]);
}

#[test]
fn nothing_loads_while_they_come_or_in_a_preview() {
    let timeline = fixtures::start_of_general(Edge::Loading);
    let mut ui = shown(&timeline);
    ui.click("Loading earlier messages...").expect("the line");
    assert_eq!(asked(&mut ui), []);

    let timeline = Timeline {
        locked: true,
        ..fixtures::start_of_general(Edge::More)
    };
    let mut ui = shown(&timeline);
    ui.click("Earlier messages").expect("the button");
    assert_eq!(asked(&mut ui), []);
}

#[test]
fn a_window_of_older_messages_loads_newer_ones_below() {
    let timeline = fixtures::older_window(Edge::More);
    let mut ui = shown(&timeline);
    ui.click("Newer messages").expect("the button");
    assert_eq!(asked(&mut ui), [Intent::Later]);

    let timeline = fixtures::older_window(Edge::Failed);
    let mut ui = shown(&timeline);
    ui.click("Couldn't load newer messages. Retry")
        .expect("the button");
    assert_eq!(asked(&mut ui), [Intent::Later]);

    let timeline = fixtures::older_window(Edge::Loading);
    let mut ui = shown(&timeline);
    assert!(shows(&mut ui, "Loading newer messages..."));
}

#[test]
fn the_new_divider_and_a_day_stand_over_their_messages() {
    let timeline = fixtures::general();
    let mut ui = shown(&timeline);
    assert!(shows(&mut ui, "New"), "message 51 is near the end");
    let timeline = fixtures::bob();
    let mut ui = shown(&timeline);
    assert!(shows(&mut ui, "2 October 2026"));
    assert!(shows(&mut ui, "New"));
}
