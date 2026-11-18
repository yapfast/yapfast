//! What this device alone keeps of the open room's polls and bots'
//! controls, beside the core's messages, as the Slint app kept some of it
//! among `revealed` (`src/ui/polls.rs`): a poll's answers picked before
//! voting and its results asked for, a vote sent that Discord hasn't
//! counted yet, a menu's choice sent that its bot hasn't answered. And what
//! the rows were built with of the core's own (the controls it holds back,
//! the one that failed, the poll asked to be ended, the polls that count
//! down), so that a change builds again only the rows it touches
//! ([`moved`]).
//!
//! A vote or a choice shows at once and settles as the core's message comes
//! with it; one Discord refuses, which the room's notice tells, or whose
//! poll closed first, shows what the core has again.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use yapfast_core::application::AppState;
use yapfast_core::chat::ChatMessage;

use anomi_vm::chat::timeline::group;

/// What the timeline's rows were built with of polls and bots, beside the
/// core's messages.
#[derive(Default)]
pub(crate) struct Kept {
    /// Each poll's own, by its message's item.
    pub polls: HashMap<String, Poll>,
    /// Menus' choices sent, each shown in its box until its message changes
    /// or its bot's answer fails.
    pub chosen: Vec<Chosen>,
    /// The open polls that end, as (row, end in Unix seconds), as the list
    /// was last read: the minute clock wakes for their time left.
    pub timed: Vec<(u64, u64)>,
    listed: Weak<Vec<Arc<ChatMessage>>>,
    /// The messages whose controls wait for their bot, as last built.
    busy: Vec<String>,
    /// The message whose control failed, and the poll asked to be ended.
    failed: Option<String>,
    confirming: Option<String>,
    /// The room's notice as the last vote went: Discord refusing a vote
    /// says so there.
    pub notice: Option<String>,
}

/// A poll's own on this device.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Poll {
    /// The answers picked to vote for.
    pub picked: Vec<u64>,
    /// "Show results" was pressed.
    pub results: bool,
    /// A vote sent, by the answers it names (none takes the vote back),
    /// until the core's poll counts it.
    pub voting: Option<Vec<u64>>,
}

/// A menu's choice sent from a message, by what its box then says.
#[derive(Debug)]
pub(crate) struct Chosen {
    pub item: String,
    pub custom_id: String,
    pub text: String,
    /// The message as it was sent from: a new one is the bot's answer.
    message: Weak<ChatMessage>,
}

impl Chosen {
    pub(crate) fn new(message: &Arc<ChatMessage>, custom_id: String, text: String) -> Self {
        Self {
            item: message.item.clone(),
            custom_id,
            text,
            message: Arc::downgrade(message),
        }
    }
}

impl Kept {
    /// What a menu of the message `item` shows as chosen, if one was sent.
    pub(crate) fn chosen(&self, item: &str, custom_id: &str) -> Option<&str> {
        self.chosen
            .iter()
            .find(|chosen| chosen.item == item && chosen.custom_id == custom_id)
            .map(|chosen| chosen.text.as_str())
    }

    /// The poll of the message `item`, to change.
    pub(crate) fn poll(&mut self, item: &str) -> &mut Poll {
        self.polls.entry(item.to_owned()).or_default()
    }
}

/// The rows to build again for what changed of the core's beside their
/// messages (the channel's, or the thread beside's), at `now` in Unix
/// seconds; settles the votes and choices the
/// core's messages now carry, and lets go of those it refused.
pub(crate) fn moved(
    state: &AppState,
    messages: &Arc<Vec<Arc<ChatMessage>>>,
    kept: &mut Kept,
    now: u64,
) -> Vec<u64> {
    let mut rows = Vec::new();
    let row = |item: &str| {
        messages
            .iter()
            .find(|message| message.item == item)
            .map(|message| group::key(&message.id))
    };
    if !Weak::ptr_eq(&kept.listed, &Arc::downgrade(messages)) {
        kept.listed = Arc::downgrade(messages);
        kept.timed = messages
            .iter()
            .filter_map(|message| {
                let poll = message.poll.as_ref()?;
                let end = poll.expiry.filter(|_| !poll.closed(now))?;
                Some((group::key(&message.id), end))
            })
            .collect();
    }
    if state.interactions.newest_busy().is_some() || !kept.busy.is_empty() {
        let busy: Vec<String> = messages
            .iter()
            .filter(|message| !message.cards.is_empty())
            .filter(|message| state.interactions.busy(&message.item))
            .map(|message| message.item.clone())
            .collect();
        let changed = kept.busy.iter().filter(|item| !busy.contains(item));
        let changed = changed.chain(busy.iter().filter(|item| !kept.busy.contains(item)));
        rows.extend(changed.filter_map(|item| row(item)));
        kept.busy = busy;
    }
    let failed = state.interaction_failed.as_ref().map(|(item, _)| item);
    if kept.failed.as_ref() != failed {
        rows.extend(
            [kept.failed.as_deref(), failed.map(String::as_str)]
                .into_iter()
                .flatten()
                .filter_map(row),
        );
        kept.failed = failed.cloned();
    }
    if kept.confirming != state.confirm_end_poll {
        let both = [&kept.confirming, &state.confirm_end_poll];
        rows.extend(both.into_iter().flatten().filter_map(|item| row(item)));
        kept.confirming.clone_from(&state.confirm_end_poll);
    }
    let refused = state.notice.is_some() && state.notice != kept.notice;
    for (item, poll) in &mut kept.polls {
        let Some(answers) = &poll.voting else {
            continue;
        };
        let message = messages.iter().find(|message| &message.item == item);
        let Some((message, core)) =
            message.and_then(|message| Some((message, message.poll.as_ref()?)))
        else {
            poll.voting = None;
            continue;
        };
        let counted = core
            .answers
            .iter()
            .all(|answer| answer.mine == answers.contains(&answer.id));
        if counted || refused || core.closed(now) {
            poll.voting = None;
            if !counted {
                rows.push(group::key(&message.id));
            }
        }
    }
    kept.polls.retain(|_, poll| *poll != Poll::default());
    kept.chosen.retain(|chosen| {
        let Some(message) = messages.iter().find(|message| message.item == chosen.item) else {
            return false;
        };
        let failed = failed.is_some_and(|failed| *failed == chosen.item);
        let answered = !std::ptr::eq(chosen.message.as_ptr(), Arc::as_ptr(message));
        if failed || answered {
            rows.push(group::key(&message.id));
        }
        !failed && !answered
    });
    rows
}
