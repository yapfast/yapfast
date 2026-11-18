//! A message's poll as its card shows it (`poll_card`, `src/ui/polls.rs`),
//! and the "Create a Poll" dialog ([`dialog`]). What this device alone
//! knows of a poll (the answers picked, "Show results", a vote on its way)
//! is the timeline's `kept`: a vote shows at once, counted as Discord will
//! count it, and settles as the core's poll does.
//!
//! A poll's time left changes by itself: the minute clock wakes at its next
//! change ([`due`]) and [`tick`] names the rows whose words it changed.

pub(crate) mod dialog;

use yapfast_core::chat::{ChatMessage, Poll as CorePoll, PollAnswer};
use yapfast_lang::{tr, trn};

use anomi_vm::chat::poll::{Action, Answer, Poll};
use anomi_vm::chat::timeline::Timeline;
use anomi_vm::people::status::Emoji;

use super::kept::Poll as Kept;
use super::message::Context;
use super::timeline::Shown;
use crate::pictures;
use crate::project::account::status;

/// The poll of `message`, the row `key`, as its card shows it in the
/// timeline: one to vote in.
pub(crate) fn card(cx: &Context<'_>, message: &ChatMessage, poll: &CorePoll, key: u64) -> Poll {
    let kept = cx.kept.polls.get(&message.item);
    let mut card = build(cx.state, message, poll, kept, cx.now, true);
    card.key = key;
    card.confirming = cx.state.confirm_end_poll.as_deref() == Some(message.item.as_str());
    card
}

/// The poll of a pinned or found message, the row `key`: its votes show
/// once this account voted or it closed, and it sends to the message to
/// vote (I13's pins and search).
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "I13's pins and search show their polls through it"
    )
)]
pub(crate) fn found(
    state: &yapfast_core::application::AppState,
    message: &ChatMessage,
    poll: &CorePoll,
    key: u64,
    now: u64,
) -> Poll {
    Poll {
        key,
        ..build(state, message, poll, None, now, false)
    }
}

fn build(
    state: &yapfast_core::application::AppState,
    message: &ChatMessage,
    poll: &CorePoll,
    kept: Option<&Kept>,
    now: u64,
    interactive: bool,
) -> Poll {
    let closed = poll.closed(now);
    let voting = kept.and_then(|kept| kept.voting.as_deref());
    // A vote on its way counts as Discord will count it.
    let mine = |answer: &PollAnswer| voting.map_or(answer.mine, |voted| voted.contains(&answer.id));
    let votes = |answer: &PollAnswer| {
        (answer.votes + usize::from(mine(answer))).saturating_sub(usize::from(answer.mine))
    };
    let voted = poll.answers.iter().any(mine);
    let shown = interactive && kept.is_some_and(|kept| kept.results);
    let results = closed || voted || shown;
    let picked = |id: u64| kept.is_some_and(|kept| kept.picked.contains(&id));
    let total: usize = poll.answers.iter().map(votes).sum();
    let most = poll.answers.iter().map(votes).max();
    let answers = poll
        .answers
        .iter()
        .map(|answer| Answer {
            id: answer.id,
            text: answer.text.clone(),
            emoji: answer
                .emoji
                .as_deref()
                .map(|key| emoji(state, message, key)),
            picked: if results {
                mine(answer)
            } else {
                picked(answer.id)
            },
            votes: votes_text(votes(answer)),
            percent: (votes(answer) * 100 + total / 2)
                .checked_div(total)
                .map_or(0, |percent| percent.min(100) as u8),
            leading: votes(answer) > 0 && Some(votes(answer)) == most,
        })
        .collect();
    if !interactive {
        return Poll {
            question: poll.question.clone(),
            multiselect: poll.multiselect,
            answers,
            results,
            total: if closed {
                votes_text(total)
            } else {
                tr!("View original message to interact").to_owned()
            },
            left: if closed { tr!("Poll closed") } else { "" }.to_owned(),
            ..Poll::default()
        };
    }
    let any_picked = poll.answers.iter().any(|answer| picked(answer.id));
    Poll {
        question: poll.question.clone(),
        prompt: match (closed, poll.multiselect) {
            (true, _) => "",
            (false, true) => tr!("Select one or more answers"),
            (false, false) => tr!("Select one answer"),
        }
        .to_owned(),
        multiselect: poll.multiselect,
        answers,
        results,
        can_pick: !results,
        total: votes_text(total),
        left: left(poll, now),
        action: if closed {
            Action::None
        } else if voted {
            Action::RemoveVote
        } else if shown {
            Action::BackToVote
        } else {
            Action::Vote
        },
        // A vote on its way can't be sent again before it settles.
        action_enabled: (voted || shown || any_picked) && voting.is_none(),
        can_show_results: !results,
        can_end: poll.can_end && !closed,
        interactive: true,
        ..Poll::default()
    }
}

/// An answer's emoji: Unicode's own, or one of the message's custom emoji
/// by `:name:`, with its picture once it came.
fn emoji(state: &yapfast_core::application::AppState, message: &ChatMessage, key: &str) -> Emoji {
    match message.emoji.iter().find(|emoji| emoji.text == key) {
        Some(custom) => Emoji::custom(
            &format!("{}:{}", key.trim_matches(':'), custom.id),
            key,
            pictures::find(state, Some(&custom.picture)),
        ),
        None => status::unicode(key),
    }
}

/// "1 vote", "3 votes".
fn votes_text(votes: usize) -> String {
    trn!("{n} vote", "{n} votes", votes)
}

/// How long the poll has left at `now`, as Discord's client says it: "3h
/// left", or "Poll closed"; nothing for one that never ends.
pub(crate) fn left(poll: &CorePoll, now: u64) -> String {
    if poll.closed(now) {
        return tr!("Poll closed").to_owned();
    }
    let Some(expiry) = poll.expiry else {
        return String::new();
    };
    match expiry.saturating_sub(now) {
        seconds @ 86_400.. => tr!("{}d left", seconds / 86_400),
        seconds @ 3_600.. => tr!("{}h left", seconds / 3_600),
        seconds @ 60.. => tr!("{}m left", seconds / 60),
        _ => tr!("< 1m left").to_owned(),
    }
}

/// When the time left of a poll ending at `end` next changes after `now`,
/// both in Unix seconds: as it drops below the whole days, hours or minutes
/// it says, or as it closes.
fn changes(end: u64, now: u64) -> Option<u64> {
    let seconds = end.checked_sub(now).filter(|&seconds| seconds > 0)?;
    let unit = match seconds {
        86_400.. => 86_400,
        3_600.. => 3_600,
        60.. => 60,
        _ => return Some(end),
    };
    Some(end - seconds / unit * unit + 1)
}

/// When the minute clock wakes for the polls the timeline shows, in Unix
/// milliseconds: the soonest change of their time left.
pub(crate) fn due(shown: &Shown, now_ms: u64) -> Option<u64> {
    shown.room()?;
    let now = now_ms / 1000;
    let soonest = shown
        .kept
        .timed
        .iter()
        .filter_map(|&(_, end)| changes(end, now))
        .min();
    soonest.map(|at| at * 1000)
}

/// A minute turned: the polls whose time left now reads otherwise, at `now`
/// in Unix seconds, are named to be built again; whether any was.
pub(crate) fn tick(timeline: &Timeline, shown: &mut Shown, now: u64) -> bool {
    let changed: Vec<u64> = shown
        .kept
        .timed
        .iter()
        .filter_map(|&(key, _)| {
            let row = timeline.rows.iter().find(|row| row.key == key)?;
            let poll = shown.message(key)?.poll.as_ref()?;
            (left(poll, now) != row.message.poll.left).then_some(key)
        })
        .collect();
    let any = !changed.is_empty();
    shown.again.extend(changed);
    any
}

#[cfg(test)]
pub(crate) mod tests;
