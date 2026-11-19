//! Polls: making one, voting, and who voted.

use super::*;

#[test]
#[cfg(feature = "discord")]
fn a_poll_is_made_in_discords_dialog_where_polls_may_go() {
    use crate::chat::{NewPoll, NewPollAnswer};
    use yapfast_discord::{
        ready::{self, permission},
        service::{Snapshot, Status},
    };
    let bits = |bits: u64| bits.to_string();
    let server = ready::server(&serde_json::json!({
        "id": "10", "owner_id": "99", "properties": {"name": "Test"},
        "roles": [{"id": "10", "name": "@everyone", "position": 0, "permissions": bits(
            permission::VIEW_CHANNEL | permission::SEND_MESSAGES | permission::SEND_POLLS)}],
        "channels": [
            {"id": "11", "type": 0, "name": "general", "position": 0},
            {"id": "14", "type": 0, "name": "quiet", "position": 1,
             "permission_overwrites": [{"id": "10", "type": 0, "allow": "0",
                                        "deny": bits(permission::SEND_POLLS)}]},
        ],
    }))
    .unwrap();
    let mut app = Application::default();
    app.apply(Event::Discord(Arc::new(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        servers: vec![MarkedServer::from(server)],
        ..Default::default()
    })));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    let open = |app: &mut Application, room: &str| {
        app.dispatch(Action::SelectRoom(room.into()));
        app.apply(Event::Timeline {
            room: room.into(),
            messages: timeline(vec![]),
        });
    };
    open(&mut app, "discord:10:14");
    assert!(!app.can_create_polls(), "Send Polls is denied there");
    app.dispatch(Action::OpenPollDraft);
    assert!(app.state.poll_draft.is_none());
    let general = "discord:10:11".to_owned();
    open(&mut app, &general);
    assert!(app.can_create_polls());

    // It starts with two answers to fill, for a day.
    app.dispatch(Action::OpenPollDraft);
    let draft = |app: &Application| app.state.poll_draft.clone().unwrap();
    assert_eq!(draft(&app).answers.len(), 2);
    assert_eq!(POLL_HOURS[draft(&app).duration], 24);
    assert!(!draft(&app).can_post(), "it has no question yet");
    let edit = |app: &mut Application, edit: PollEdit| app.dispatch(Action::EditPoll(edit));
    edit(&mut app, PollEdit::Question(" Lunch? ".into()));
    edit(
        &mut app,
        PollEdit::Answer {
            index: 1,
            text: "Soup".into(),
        },
    );
    assert!(draft(&app).can_post(), "one answer is enough");
    edit(
        &mut app,
        PollEdit::Answer {
            index: 0,
            text: "x".repeat(56),
        },
    );
    assert_eq!(
        draft(&app).problem(),
        Some("Answers can be at most 55 characters.")
    );
    assert!(!draft(&app).can_post());
    edit(
        &mut app,
        PollEdit::Answer {
            index: 0,
            text: "Pizza".into(),
        },
    );
    edit(
        &mut app,
        PollEdit::Emoji {
            index: 0,
            emoji: Some("🍕".into()),
        },
    );

    // Ten answers at most, and one stays.
    for _ in 0..10 {
        edit(&mut app, PollEdit::AddAnswer);
    }
    assert_eq!(draft(&app).answers.len(), POLL_ANSWERS);
    assert!(!draft(&app).can_add());
    let ids: Vec<u32> = draft(&app).answers.iter().map(|answer| answer.id).collect();
    edit(&mut app, PollEdit::RemoveAnswer(1));
    let kept: Vec<u32> = draft(&app).answers.iter().map(|answer| answer.id).collect();
    assert_eq!(
        kept,
        [&ids[..1], &ids[2..]].concat(),
        "the others keep their ids"
    );
    edit(
        &mut app,
        PollEdit::Answer {
            index: 1,
            text: "Soup".into(),
        },
    );
    edit(&mut app, PollEdit::Duration(6));
    edit(&mut app, PollEdit::Multiselect(true));

    // Posting leaves out the empty answers, and keeps the dialog till
    // Discord answers.
    let posted = app.dispatch(Action::PostPoll);
    let expected = NewPoll {
        question: "Lunch?".into(),
        answers: vec![
            NewPollAnswer {
                text: "Pizza".into(),
                emoji: Some("🍕".into()),
            },
            NewPollAnswer {
                text: "Soup".into(),
                emoji: None,
            },
        ],
        hours: 336,
        multiselect: true,
    };
    assert!(matches!(
        posted.as_slice(),
        [Effect::Backend(Command::SendPoll { room, poll })] if *room == general && *poll == expected
    ));
    assert!(draft(&app).posting && !draft(&app).can_post());
    assert!(app.dispatch(Action::PostPoll).is_empty(), "once");
    app.apply(Event::PollPosted {
        room: general.clone(),
        result: Err("no".into()),
    });
    assert_eq!(draft(&app).error.as_deref(), Some("no"));
    assert!(!draft(&app).posting);
    app.dispatch(Action::PostPoll);
    app.apply(Event::PollPosted {
        room: general,
        result: Ok(()),
    });
    assert!(app.state.poll_draft.is_none());
}

#[test]
fn votes_go_to_discord_and_the_window_lists_who_voted() {
    use crate::chat::{Poll, PollAnswer};
    let mut app = connected();
    let answer = |id, votes, mine| PollAnswer {
        id,
        text: format!("Answer {id}"),
        votes,
        mine,
        ..Default::default()
    };
    let mut asked = message("e:$poll", true);
    asked.poll = Some(Poll {
        question: "Lunch?".into(),
        answers: vec![answer(1, 2, false), answer(2, 0, false)],
        can_end: true,
        ..Default::default()
    });
    let mut closed = message("e:$closed", false);
    closed.poll = Some(Poll {
        answers: vec![answer(1, 1, false)],
        finalized: true,
        ..Default::default()
    });
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![asked, closed]),
    });
    let vote = |app: &mut Application, item: &str, answers: Vec<u64>| {
        app.dispatch(Action::Vote {
            item: item.into(),
            answers,
        })
    };

    assert!(matches!(
        vote(&mut app, "e:$poll", vec![2]).as_slice(),
        [Effect::Backend(Command::Vote { item, answers, .. })] if item == "e:$poll" && answers == &[2]
    ));
    // Taking it back votes for none.
    assert!(matches!(
        vote(&mut app, "e:$poll", vec![]).as_slice(),
        [Effect::Backend(Command::Vote { answers, .. })] if answers.is_empty()
    ));
    // One answer only, of those it has, while it's open.
    assert!(vote(&mut app, "e:$poll", vec![1, 2]).is_empty());
    assert!(vote(&mut app, "e:$poll", vec![9]).is_empty());
    assert!(vote(&mut app, "e:$closed", vec![1]).is_empty());

    // Closing it asks first, and only its author may.
    app.dispatch(Action::AskEndPoll("e:$closed".into()));
    assert!(app.state.confirm_end_poll.is_none());
    app.dispatch(Action::AskEndPoll("e:$poll".into()));
    assert_eq!(app.state.confirm_end_poll.as_deref(), Some("e:$poll"));
    app.dispatch(Action::CancelEndPoll);
    assert!(app.dispatch(Action::EndPoll).is_empty());
    app.dispatch(Action::AskEndPoll("e:$poll".into()));
    assert!(matches!(
        app.dispatch(Action::EndPoll).as_slice(),
        [Effect::Backend(Command::EndPoll { item, .. })] if item == "e:$poll"
    ));
    assert!(app.state.confirm_end_poll.is_none());

    // Who voted shows by answer, the first unless one is asked for.
    assert!(matches!(
        app.dispatch(Action::ShowVoters {
            item: "e:$poll".into(),
            answer: None
        })
        .as_slice(),
        [Effect::Backend(Command::Voters {
            answer: 1,
            after: None,
            limit: 100,
            ..
        })]
    ));
    let ids: Vec<u64> = app
        .state
        .window_answers()
        .iter()
        .map(|answer| answer.id)
        .collect();
    assert_eq!(ids, [1, 2]);
    assert!(app.state.window_reactions().is_empty());
    app.apply(Event::Reactors {
        room: "dm".into(),
        item: "e:$poll".into(),
        key: "1".into(),
        after: None,
        limit: 100,
        result: Ok(vec![crate::chat::Reactor {
            id: "a".into(),
            me: true,
            ..Default::default()
        }]),
    });
    assert_eq!(app.state.reactions.as_ref().unwrap().people.len(), 1);
    // Both counted votes haven't come, but the page came short.
    assert!(!app.state.more_reactors());
    // A vote isn't taken back from the window.
    assert!(app.dispatch(Action::RemoveReactor("a".into())).is_empty());
    assert!(matches!(
        app.dispatch(Action::ShowReactors("2".into())).as_slice(),
        [Effect::Backend(Command::Voters { answer: 2, .. })]
    ));
}
