//! A poll's card: the answers picked before a vote, one at a time unless
//! the poll takes several, as Discord's client picks them.

use std::sync::Arc;

use yapfast_core::chat::{ChatMessage, Poll, PollAnswer};

use super::{change, preview, shows};
use crate::{app::Message, polls::Event, polls::pick_key};

fn with_poll(multiselect: bool) -> (crate::app::App, tempfile::TempDir) {
    let (mut app, folder) = preview();
    change(&mut app, |state| {
        let template = (*state.messages[0]).clone();
        let answer = |id: u64, text: &str| PollAnswer {
            id,
            text: text.into(),
            ..Default::default()
        };
        let poll = ChatMessage {
            id: "poll".into(),
            item: "poll".into(),
            poll: Some(Poll {
                question: "Which map next?".into(),
                answers: vec![answer(1, "Forest"), answer(2, "Desert")],
                expiry: None,
                finalized: false,
                multiselect,
                can_end: false,
            }),
            ..template
        };
        let mut messages = state.messages.to_vec();
        messages.push(Arc::new(poll));
        state.messages = Arc::new(messages);
    });
    (app, folder)
}

fn picked(app: &crate::app::App) -> [bool; 2] {
    [1, 2].map(|answer| app.timeline.revealed.contains(&pick_key("poll", answer)))
}

#[test]
fn a_single_choice_polls_second_pick_takes_the_firsts_place() {
    let (mut app, _folder) = with_poll(false);
    assert!(shows(&app, "Which map next?"));
    let _ = app.update(Message::Poll(Event::Pick("poll".into(), 1)));
    let _ = app.update(Message::Poll(Event::Pick("poll".into(), 2)));
    assert_eq!(picked(&app), [false, true], "only the last pick");
    // Picked again, it's unpicked.
    let _ = app.update(Message::Poll(Event::Pick("poll".into(), 2)));
    assert_eq!(picked(&app), [false, false]);
}

#[test]
fn a_poll_of_several_choices_keeps_each_pick_until_the_vote_is_taken_back() {
    let (mut app, _folder) = with_poll(true);
    let _ = app.update(Message::Poll(Event::Pick("poll".into(), 1)));
    let _ = app.update(Message::Poll(Event::Pick("poll".into(), 2)));
    assert_eq!(picked(&app), [true, true], "both");
    let _ = app.update(Message::Poll(Event::RemoveVote("poll".into())));
    assert_eq!(
        picked(&app),
        [false, false],
        "taking the vote back starts afresh"
    );
}

/// The open room's pins, holding one message with `poll`.
fn pinned_poll(app: &mut crate::app::App, poll: Poll) {
    change(app, |state| {
        let room = state.selected.clone().expect("an open room");
        state.pins = Some(yapfast_core::application::Pins {
            room,
            messages: Some(Ok(vec![ChatMessage {
                item: "pinned-poll".into(),
                author: "Sam".into(),
                timestamp: 1,
                poll: Some(poll),
                ..Default::default()
            }])),
        });
    });
}

fn answer(id: u64, text: &str, votes: usize) -> PollAnswer {
    PollAnswer {
        id,
        text: text.into(),
        votes,
        ..Default::default()
    }
}

#[test]
fn a_pinned_polls_card_shows_its_answers_and_sends_to_the_message_to_vote() {
    let (mut app, _folder) = preview();
    pinned_poll(
        &mut app,
        Poll {
            question: "Which map next?".into(),
            answers: vec![answer(1, "Forest", 2), answer(2, "Desert", 1)],
            ..Default::default()
        },
    );
    for words in [
        "Which map next?",
        "Forest",
        "Desert",
        "View original message to interact",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    // Not voted in, its votes don't show yet, and nothing in it is a button.
    for words in ["Select one answer", "Show results", "Vote", "67%"] {
        assert!(!shows(&app, words), "{words}");
    }
    // A click on an answer goes to the card, which jumps to the message.
    super::interact(&mut app, |ui| {
        ui.click("Forest").expect("the answer");
    });
    assert!(app.client.state().pins.is_none(), "jumped");
    assert!(
        !app.timeline.revealed.contains(&pick_key("pinned-poll", 1)),
        "nothing picked"
    );
}

#[test]
fn a_pinned_polls_card_shows_the_shares_once_it_closed() {
    let (mut app, _folder) = preview();
    pinned_poll(
        &mut app,
        Poll {
            question: "Which map next?".into(),
            answers: vec![answer(1, "Forest", 2), answer(2, "Desert", 1)],
            finalized: true,
            ..Default::default()
        },
    );
    for words in ["67%", "33%", "3 votes", "\u{00b7}  Poll closed"] {
        assert!(shows(&app, words), "{words}");
    }
    assert!(!shows(&app, "View original message to interact"));
}

#[test]
fn once_the_results_show_an_answer_or_the_votes_show_who_voted() {
    let (mut app, _folder) = with_poll(false);
    // Before the results show, a click on an answer picks it.
    super::interact(&mut app, |ui| {
        ui.click("Desert").expect("the answer");
    });
    assert!(app.client.state().reactions.is_none());
    assert_eq!(picked(&app), [false, true]);
    change(&mut app, |state| {
        let mut messages = state.messages.to_vec();
        let message = Arc::make_mut(messages.last_mut().expect("the poll"));
        let poll = message.poll.as_mut().expect("its poll");
        poll.answers[0].votes = 2;
        poll.answers[0].mine = true;
        poll.answers[1].votes = 1;
        state.messages = Arc::new(messages);
    });
    super::interact(&mut app, |ui| {
        ui.click("Desert").expect("the answer");
    });
    let window = app.client.state().reactions.as_ref().expect("the voters");
    assert!(window.poll);
    assert_eq!((window.item.as_str(), window.key.as_str()), ("poll", "2"));
    assert!(shows(&app, "Votes"));
    let _ = app.update(Message::Reactions(crate::reactions::Event::Close));
    // The votes so far open it on the first answer.
    super::interact(&mut app, |ui| {
        ui.click("3 votes").expect("the votes");
    });
    let window = app.client.state().reactions.as_ref().expect("the voters");
    assert_eq!(window.key, "1");
}
