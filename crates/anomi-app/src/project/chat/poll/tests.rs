//! A poll's card in the timeline: its votes show once voted, closed or
//! asked; a vote on its way shows at once and settles as the core's poll
//! counts it, or goes as Discord refuses it; its time left changes by
//! itself, the minute clock waking for the change alone.

use std::sync::Arc;

use yapfast_core::application::{Action, Application};
use yapfast_core::chat::{ChatMessage, Poll, PollAnswer};

use anomi_vm::chat::poll::Action as Act;
use anomi_vm::chat::timeline::{Timeline, group};

use super::{changes, due, left, tick};
use crate::pictures::Requests;
use crate::project::chat::timeline::{Shown, show};
use crate::project::rail::tests::layout;

const ROOM: &str = "discord:10:7";

pub(crate) fn now() -> u64 {
    yapfast_core::clock::now_ms() / 1000
}

/// A Discord channel open on `messages`, as the core holds it.
pub(crate) fn core_with(messages: Vec<Arc<ChatMessage>>) -> Application {
    let mut core = Application::default();
    drop(core.dispatch(Action::Preview));
    let state = &mut core.state;
    let room = yapfast_core::chat::ChatRoom {
        id: ROOM.into(),
        name: "general".into(),
        direct: false,
        ..(*state.rooms[0]).clone()
    };
    state.rooms.push(Arc::new(room));
    state.preview = false;
    state.selected = Some(ROOM.into());
    state.messages = Arc::new(messages);
    core
}

fn answer(id: u64, votes: usize, mine: bool) -> PollAnswer {
    PollAnswer {
        id,
        text: format!("Answer {id}"),
        votes,
        mine,
        ..PollAnswer::default()
    }
}

/// Bob's poll "Lunch?" in `d:5`, ending in a little over two hours.
pub(crate) fn poll_message(answers: Vec<PollAnswer>) -> Arc<ChatMessage> {
    Arc::new(ChatMessage {
        id: "d:5".into(),
        item: "d:5".into(),
        author: "Bob".into(),
        sender: "discord:2".into(),
        timestamp: now(),
        poll: Some(Poll {
            question: "Lunch?".into(),
            answers,
            expiry: Some(now() + 7_230),
            ..Poll::default()
        }),
        ..ChatMessage::default()
    })
}

fn shown(core: &Application, timeline: &mut Timeline, shown: &mut Shown) {
    show(
        core,
        &layout(),
        true,
        timeline,
        shown,
        &mut Requests::default(),
    );
}

fn card(timeline: &Timeline) -> &anomi_vm::chat::poll::Poll {
    &timeline.rows[0].message.poll
}

fn votes(timeline: &Timeline) -> Vec<(bool, &str, u8, bool)> {
    let answers = card(timeline).answers.iter();
    answers
        .map(|row| (row.picked, row.votes.as_str(), row.percent, row.leading))
        .collect()
}

#[test]
fn a_polls_card_shows_votes_once_voted_closed_or_asked() {
    let answers = vec![
        answer(1, 2, false),
        answer(2, 1, false),
        answer(3, 0, false),
    ];
    let core = core_with(vec![poll_message(answers)]);
    let (mut timeline, mut rows) = (Timeline::default(), Shown::default());
    shown(&core, &mut timeline, &mut rows);
    let open = card(&timeline);
    assert!(!open.results && open.can_pick && open.can_show_results);
    assert_eq!(open.prompt, "Select one answer");
    assert_eq!(
        (open.total.as_str(), open.left.as_str()),
        ("3 votes", "2h left")
    );
    assert_eq!((open.action, open.action_enabled), (Act::Vote, false));
    // An answer picked: it can be voted for.
    let key = group::key("d:5");
    rows.kept.poll("d:5").picked.push(2);
    rows.again.push(key);
    shown(&core, &mut timeline, &mut rows);
    assert!(card(&timeline).action_enabled && card(&timeline).answers[1].picked);
    // "Show results" shows the votes, without this account's pick.
    rows.kept.poll("d:5").results = true;
    rows.again.push(key);
    shown(&core, &mut timeline, &mut rows);
    let results = card(&timeline);
    assert!(results.results && !results.can_pick && !results.can_show_results);
    assert_eq!(results.action, Act::BackToVote);
    assert_eq!(
        votes(&timeline),
        [
            (false, "2 votes", 67, true),
            (false, "1 vote", 33, false),
            (false, "0 votes", 0, false),
        ]
    );
}

#[test]
fn a_vote_shows_at_once_and_settles_as_the_core_counts_it() {
    let mut core = core_with(vec![poll_message(vec![
        answer(1, 2, false),
        answer(2, 1, false),
    ])]);
    let (mut timeline, mut rows) = (Timeline::default(), Shown::default());
    shown(&core, &mut timeline, &mut rows);
    let key = group::key("d:5");
    // Sent: counted as Discord will count it, and not to be sent again.
    rows.kept.poll("d:5").voting = Some(vec![2]);
    rows.again.push(key);
    shown(&core, &mut timeline, &mut rows);
    assert!(card(&timeline).results);
    assert_eq!(
        votes(&timeline),
        [(false, "2 votes", 50, true), (true, "2 votes", 50, true)]
    );
    assert_eq!(card(&timeline).action, Act::RemoveVote);
    assert!(!card(&timeline).action_enabled, "one on its way");
    // Discord counted it: the core's poll has it, and the row shows the core's.
    let counted = poll_message(vec![answer(1, 2, false), answer(2, 2, true)]);
    core.state.messages = Arc::new(vec![counted]);
    shown(&core, &mut timeline, &mut rows);
    assert!(
        rows.kept
            .polls
            .get("d:5")
            .is_none_or(|poll| poll.voting.is_none())
    );
    assert_eq!(
        votes(&timeline),
        [(false, "2 votes", 50, true), (true, "2 votes", 50, true)]
    );
    assert!(card(&timeline).action_enabled);
}

#[test]
fn a_vote_discord_refuses_shows_the_poll_as_it_was() {
    let mut core = core_with(vec![poll_message(vec![
        answer(1, 2, false),
        answer(2, 1, false),
    ])]);
    let (mut timeline, mut rows) = (Timeline::default(), Shown::default());
    shown(&core, &mut timeline, &mut rows);
    let poll = rows.kept.poll("d:5");
    poll.picked.push(2);
    poll.voting = Some(vec![2]);
    rows.again.push(group::key("d:5"));
    shown(&core, &mut timeline, &mut rows);
    assert!(card(&timeline).results);
    // The room's notice says why: the vote goes, the pick stays.
    core.state.notice = Some("Couldn't vote: Missing Access".into());
    let built = timeline.rows[0].message.poll.clone();
    shown(&core, &mut timeline, &mut rows);
    assert_ne!(card(&timeline), &built);
    assert!(!card(&timeline).results);
    assert_eq!(rows.kept.polls["d:5"].picked, [2]);
    assert_eq!(votes(&timeline)[1], (true, "1 vote", 33, false));
}

#[test]
fn time_left_changes_as_it_drops_below_what_it_says() {
    let end = 1_000_000;
    let poll = Poll {
        expiry: Some(end),
        ..Poll::default()
    };
    assert_eq!(left(&poll, end - 7_200), "2h left");
    assert_eq!(changes(end, end - 7_200), Some(end - 7_200 + 1));
    assert_eq!(changes(end, end - 5_000), Some(end - 3_600 + 1));
    assert_eq!(left(&poll, end - 3_601), "1h left");
    assert_eq!(left(&poll, end - 3_599), "59m left");
    assert_eq!(changes(end, end - 90), Some(end - 60 + 1));
    assert_eq!(changes(end, end - 30), Some(end));
    assert_eq!(left(&poll, end), "Poll closed");
    assert_eq!(changes(end, end), None);
    assert_eq!(left(&poll, end - 3 * 86_400), "3d left");
}

#[test]
fn the_minute_clock_wakes_for_a_polls_time_left_and_builds_its_row_alone() {
    let core = core_with(vec![poll_message(vec![answer(1, 0, false)])]);
    let (mut timeline, mut rows) = (Timeline::default(), Shown::default());
    shown(&core, &mut timeline, &mut rows);
    let end = core.state.messages[0]
        .poll
        .as_ref()
        .and_then(|poll| poll.expiry)
        .unwrap();
    assert_eq!(rows.kept.timed, [(group::key("d:5"), end)]);
    let at = due(&rows, now() * 1000).expect("a poll counts down");
    assert!(at > now() * 1000 && at <= (end - 3_600 + 1) * 1000);
    // Nothing changed yet: no row to build.
    assert!(!tick(&timeline, &mut rows, now()));
    // An hour later it reads otherwise: its row alone is named.
    assert!(tick(&timeline, &mut rows, now() + 3_600));
    assert_eq!(rows.again, [group::key("d:5")]);
}

#[test]
fn a_found_poll_sends_to_its_message() {
    let core = core_with(vec![poll_message(vec![answer(1, 1, false)])]);
    let message = &core.state.messages[0];
    let poll = message.poll.as_ref().unwrap();
    let found = super::found(&core.state, message, poll, 7, now());
    assert!(!found.interactive && !found.results);
    assert_eq!(found.total, "View original message to interact");
}
