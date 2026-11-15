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
    [1, 2].map(|answer| app.revealed.contains(&pick_key("poll", answer)))
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
