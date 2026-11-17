//! Samples of a message's poll, and the dialog that makes one for tests and
//! shots, after the fake Discord's data (`/_control poll`): task C07's.

use crate::chat::message::Message;
use crate::chat::poll::{Action, Answer, Dialog, Draft, Poll};
use crate::chat::timeline::{Edge, Row, Timeline};
use crate::fixtures::chat::message::bobs;
use crate::fixtures::chat::timeline::message_key;
use crate::fixtures::pictures;
use crate::people::status::Emoji;

/// The key of the message the polls sit on.
pub fn key() -> u64 {
    message_key(7)
}

/// The fake's server emoji on the third answer (`EMOJI_TALK`).
const TALK: &str = "1400000000000000300";

fn votes(count: usize) -> String {
    if count == 1 {
        "1 vote".into()
    } else {
        format!("{count} votes")
    }
}

/// The fake's answers (`/_control poll`), with `counts` votes each.
fn answers(counts: [usize; 4]) -> Vec<Answer> {
    let total: usize = counts.iter().sum();
    let most = counts.iter().copied().max().unwrap_or(0);
    let talk = Emoji::custom(
        &format!("talk:{TALK}"),
        "talk",
        Some(pictures::badge(TALK, 40)),
    );
    [
        (
            1,
            "Pizza by the harbour",
            Some(Emoji::unicode("🍕", "pizza")),
        ),
        (2, "Ramen", Some(Emoji::unicode("🍜", "ramen"))),
        (3, "Wherever the talk is", Some(talk)),
        (4, "Home, I'm tired", None),
    ]
    .into_iter()
    .zip(counts)
    .map(|((id, text, emoji), count)| Answer {
        id,
        text: text.into(),
        emoji,
        picked: false,
        votes: votes(count),
        percent: (count * 100 + total / 2).checked_div(total).unwrap_or(0) as u8,
        leading: count > 0 && count == most,
    })
    .collect()
}

fn base(counts: [usize; 4]) -> Poll {
    let total = counts.iter().sum();
    Poll {
        key: key(),
        question: "Where should we eat after the ferry?".into(),
        prompt: "Select one answer".into(),
        answers: answers(counts),
        can_pick: true,
        total: votes(total),
        left: "23h left".into(),
        action: Action::Vote,
        can_show_results: true,
        interactive: true,
        ..Poll::default()
    }
}

/// The poll the golden screenshot shows: Carol voted for the pizza, Dave and
/// Bob for ramen, nothing picked here yet.
pub fn open() -> Poll {
    base([1, 2, 0, 0])
}

/// The second answer picked, the vote not yet sent.
pub fn picked() -> Poll {
    let mut poll = open();
    poll.answers[1].picked = true;
    poll.action_enabled = true;
    poll
}

/// Several answers may be picked.
pub fn multiple() -> Poll {
    let mut poll = picked();
    poll.multiselect = true;
    poll.prompt = "Select one or more answers".into();
    poll.answers[0].picked = true;
    poll
}

/// This account voted for ramen too: the votes show.
pub fn voted() -> Poll {
    let mut poll = base([1, 3, 0, 0]);
    poll.answers[1].picked = true;
    poll.results = true;
    poll.can_pick = false;
    poll.action = Action::RemoveVote;
    poll.action_enabled = true;
    poll.can_show_results = false;
    poll
}

/// Closed: the results, no button, "Poll closed".
pub fn closed() -> Poll {
    let mut poll = voted();
    poll.prompt = String::new();
    poll.left = "Poll closed".into();
    poll.action = Action::None;
    poll.action_enabled = false;
    poll
}

/// The author closing it before its time, made sure of.
pub fn confirming() -> Poll {
    let mut poll = open();
    poll.can_end = true;
    poll.confirming = true;
    poll
}

/// In a pinned message or a search result: shown, not voted in.
pub fn shown() -> Poll {
    let mut poll = closed();
    poll.interactive = false;
    poll.total = "View original message to interact".into();
    poll.left = String::new();
    poll
}

/// The message the poll sits on.
pub fn message(poll: Poll) -> Message {
    Message {
        poll,
        ..bobs(7, true)
    }
}

/// #general at its start with one message carrying `poll`.
pub fn timeline(poll: Poll) -> Timeline {
    let mut timeline = super::timeline::general();
    timeline.rows = vec![Row {
        key: key(),
        first: true,
        message: message(poll),
        ..Row::default()
    }];
    timeline.earlier = Edge::Start;
    timeline.unread = None;
    timeline
}

fn draft(id: u64, text: &str, emoji: &str) -> Draft {
    Draft {
        id,
        text: text.into(),
        emoji: (!emoji.is_empty()).then(|| Emoji::unicode(emoji, "emoji")),
    }
}

/// The dialog as the golden shows it: a question and two answers, one with
/// an emoji.
pub fn dialog() -> Dialog {
    Dialog {
        question: "Which game tonight?".into(),
        question_count: "19 / 300".into(),
        answers: vec![draft(1, "Chess", "♟️"), draft(2, "Cards", "")],
        duration: 3,
        can_add: true,
        can_post: true,
        ..Dialog::default()
    }
}

/// Just opened: nothing typed, two empty answers, posting not yet allowed.
pub fn fresh() -> Dialog {
    Dialog {
        question_count: "0 / 300".into(),
        answers: vec![draft(1, "", ""), draft(2, "", "")],
        duration: 3,
        can_add: true,
        ..Dialog::default()
    }
}

/// Ten answers, several allowed, one too long to post.
pub fn full() -> Dialog {
    Dialog {
        answers: (1..=10)
            .map(|id| draft(id, &format!("Answer {id}"), ""))
            .collect(),
        can_add: false,
        multiselect: true,
        problem: "An answer is too long.".into(),
        can_post: false,
        ..dialog()
    }
}

/// Posting, the button saying so.
pub fn posting() -> Dialog {
    Dialog {
        posting: true,
        can_post: false,
        ..dialog()
    }
}
