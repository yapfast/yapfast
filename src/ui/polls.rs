//! Discord polls on messages: their cards, and what the cards do. What
//! this device alone knows of a poll, the answers picked before voting and
//! whether "Show results" was clicked, the adapter keeps among `revealed`,
//! as `{item}/pick{id}` and `{item}/results`.

use yapfast_core::{
    application::{PollEdit, QUESTION_LIMIT},
    chat::Poll,
};

use super::*;

/// A poll's card on the message `item`, at `now` in seconds since the Unix
/// epoch, as Discord's client shows it. One not `interactive`, in a pinned
/// message or a search result, shows the votes only once this account voted
/// or the poll closed, and sends you to the message to vote.
pub(super) fn poll_card(
    item: &str,
    poll: &Poll,
    emoji: &[yapfast_core::chat::CustomEmoji],
    revealed: &HashSet<String>,
    now: u64,
    interactive: bool,
) -> PollCard {
    let closed = poll.closed(now);
    let voted = poll.voted();
    let shown = interactive && revealed.contains(&results_key(item));
    let results = closed || voted || shown;
    let picked = |id: u64| revealed.contains(&pick_key(item, id));
    let total: usize = poll.answers.iter().map(|answer| answer.votes).sum();
    let most = poll.answers.iter().map(|answer| answer.votes).max();
    let answers: Vec<PollAnswerRow> = poll
        .answers
        .iter()
        .map(|answer| PollAnswerRow {
            id: i32::try_from(answer.id).unwrap_or(i32::MAX),
            text: answer.text.as_str().into(),
            emoji: answer
                .emoji
                .as_deref()
                .map(|key| custom_emoji::text(key, emoji))
                .unwrap_or_default()
                .into(),
            picked: if results {
                answer.mine
            } else {
                picked(answer.id)
            },
            votes: votes_text(answer.votes).into(),
            percent: (answer.votes * 100 + total / 2)
                .checked_div(total)
                .map_or(0, |percent| percent as i32),
            leading: answer.votes > 0 && Some(answer.votes) == most,
        })
        .collect();
    let any_picked = poll.answers.iter().any(|answer| picked(answer.id));
    if !interactive {
        return PollCard {
            question: poll.question.as_str().into(),
            multiselect: poll.multiselect,
            answers: ModelRc::new(VecModel::from(answers)),
            results,
            total: if closed {
                votes_text(total)
            } else {
                tr("View original message to interact").to_owned()
            }
            .into(),
            left: if closed { tr("Poll closed") } else { "" }.into(),
            ..Default::default()
        };
    }
    PollCard {
        question: poll.question.as_str().into(),
        prompt: match (closed, poll.multiselect) {
            (true, _) => "",
            (false, true) => tr("Select one or more answers"),
            (false, false) => tr("Select one answer"),
        }
        .into(),
        multiselect: poll.multiselect,
        answers: ModelRc::new(VecModel::from(answers)),
        results,
        can_pick: !results,
        total: votes_text(total).into(),
        left: if closed {
            tr("Poll closed").into()
        } else {
            poll.expiry
                .map(|expiry| time_left(expiry.saturating_sub(now)))
                .unwrap_or_default()
                .into()
        },
        action: if closed {
            PollAction::None
        } else if voted {
            PollAction::RemoveVote
        } else if shown {
            PollAction::BackToVote
        } else {
            PollAction::Vote
        },
        action_enabled: voted || shown || any_picked,
        can_show_results: !results,
        can_end: poll.can_end && !closed,
        interactive: true,
    }
}

/// Whether a timeline shows a poll still open at `now`, whose time left
/// goes down.
pub(super) fn open_poll(messages: &[Arc<ChatMessage>], now: u64) -> bool {
    messages.iter().any(|message| {
        message
            .poll
            .as_ref()
            .is_some_and(|poll| poll.expiry.is_some() && !poll.closed(now))
    })
}

/// "1 vote", "3 votes".
fn votes_text(votes: usize) -> String {
    if votes == 1 {
        "1 vote".to_owned()
    } else {
        fill(tr("{} votes"), &[&votes])
    }
}

/// How long a poll has left, as Discord's client says it.
fn time_left(seconds: u64) -> String {
    match seconds {
        86_400.. => fill(tr("{}d left"), &[&(seconds / 86_400)]),
        3_600.. => fill(tr("{}h left"), &[&(seconds / 3_600)]),
        60.. => fill(tr("{}m left"), &[&(seconds / 60)]),
        _ => "< 1m left".to_owned(),
    }
}

fn results_key(item: &str) -> String {
    format!("{}/results", item)
}

fn pick_key(item: &str, answer: u64) -> String {
    format!("{0}/pick{1}", item, answer)
}

impl Adapter {
    /// The "Create a Poll" dialog. Its answers keep their places by id, so
    /// one removed takes its field with it.
    pub(super) fn show_poll_draft(&self, window: &YapfastWindow) {
        let draft = self.app.state.poll_draft.as_ref();
        // Opening it starts from an empty question.
        if draft.is_some() && !window.get_poll_open() {
            window.set_poll_question(SharedString::default());
        }
        window.set_poll_open(draft.is_some());
        let Some(draft) = draft else {
            if self.models.poll_answers.row_count() > 0 {
                self.models.poll_answers.set_vec(Vec::new());
            }
            return;
        };
        window.set_poll_question_count(
            format!("{} / {QUESTION_LIMIT}", draft.question.chars().count()).into(),
        );
        window.set_poll_duration(draft.duration as i32);
        window.set_poll_multiselect(draft.multiselect);
        window.set_poll_can_add(draft.can_add());
        window.set_poll_can_post(draft.can_post());
        window.set_poll_posting(draft.posting);
        window.set_poll_problem(
            draft
                .error
                .as_deref()
                .or(draft.problem())
                .unwrap_or_default()
                .into(),
        );
        let rows = draft
            .answers
            .iter()
            .map(|answer| PollAnswerDraft {
                id: answer.id as i32,
                text: answer.text.as_str().into(),
                emoji: answer
                    .emoji
                    .as_deref()
                    .map_or_else(EmojiCell::default, emoji::cell),
            })
            .collect();
        update_model_by_id(&self.models.poll_answers, rows, |row| {
            row.id.to_string().into()
        });
    }
}

pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    macro_rules! act {
        ($callback:ident($($arg:ident),*) => $action:expr) => {{
            let state = adapter.clone();
            let weak = window.as_weak();
            window.$callback(move |$($arg),*| {
                if let Some(window) = weak.upgrade() {
                    state.borrow_mut().action(&window, $action);
                }
            });
        }};
    }
    let index = |index: i32| usize::try_from(index).unwrap_or(usize::MAX);
    act!(on_open_poll() => Action::OpenPollDraft);
    act!(on_poll_question_edited(text) => Action::EditPoll(PollEdit::Question(text.into())));
    act!(on_poll_answer_edited(at, text) => Action::EditPoll(PollEdit::Answer {
        index: index(at),
        text: text.into(),
    }));
    act!(on_poll_answer_emoji(at, key) => Action::EditPoll(PollEdit::Emoji {
        index: index(at),
        emoji: Some(key.into()),
    }));
    act!(on_poll_add_answer() => Action::EditPoll(PollEdit::AddAnswer));
    act!(on_poll_remove_answer(at) => Action::EditPoll(PollEdit::RemoveAnswer(index(at))));
    act!(on_poll_duration_chosen(at) => Action::EditPoll(PollEdit::Duration(index(at))));
    act!(on_poll_multiselect_set(on) => Action::EditPoll(PollEdit::Multiselect(on)));
    act!(on_post_poll() => Action::PostPoll);
    act!(on_close_poll() => Action::ClosePollDraft);

    let polls = window.global::<Polls>();
    // Changes what this device keeps of a poll, then shows it.
    let keep = |change: fn(&mut Adapter, &str, i32)| {
        let state = adapter.clone();
        let weak = window.as_weak();
        move |item: SharedString, answer: i32| {
            let Some(window) = weak.upgrade() else { return };
            let mut adapter = state.borrow_mut();
            change(&mut adapter, &item, answer);
            adapter.restyle = true;
            adapter.refresh(&window);
        }
    };
    polls.on_pick(keep(|adapter, item, answer| {
        let Ok(answer) = u64::try_from(answer) else {
            return;
        };
        let multiselect = adapter
            .app
            .state
            .message(item)
            .and_then(|message| message.poll.as_ref())
            .is_some_and(|poll| poll.multiselect);
        let key = pick_key(item, answer);
        if !adapter.revealed.remove(&key) {
            if !multiselect {
                let picks = format!("{}/pick", item);
                adapter.revealed.retain(|kept| !kept.starts_with(&picks));
            }
            adapter.revealed.insert(key);
        }
    }));
    let show = keep(|adapter, item, _| {
        adapter.revealed.insert(results_key(item));
    });
    polls.on_show_results(move |item| show(item, 0));
    let state = adapter.clone();
    let weak = window.as_weak();
    polls.on_act(move |item, action| {
        let Some(window) = weak.upgrade() else { return };
        let mut adapter = state.borrow_mut();
        let item = item.to_string();
        let picks = format!("{}/pick", item);
        match action {
            PollAction::Vote => {
                let answers = adapter
                    .revealed
                    .iter()
                    .filter_map(|kept| kept.strip_prefix(&picks)?.parse().ok())
                    .collect();
                adapter.action(&window, Action::Vote { item, answers });
            }
            // Taking the vote back starts the poll afresh.
            PollAction::RemoveVote => {
                adapter
                    .revealed
                    .retain(|kept| !kept.starts_with(&picks) && *kept != results_key(&item));
                adapter.restyle = true;
                adapter.action(
                    &window,
                    Action::Vote {
                        item,
                        answers: vec![],
                    },
                );
            }
            PollAction::BackToVote => {
                adapter.revealed.remove(&results_key(&item));
                adapter.restyle = true;
                adapter.refresh(&window);
            }
            PollAction::None => {}
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    polls.on_voters(move |item, answer| {
        if let Some(window) = weak.upgrade() {
            let answer = u64::try_from(answer).ok();
            let item = item.into();
            state
                .borrow_mut()
                .action(&window, Action::ShowVoters { item, answer });
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    polls.on_ask_end(move |item| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::AskEndPoll(item.into()));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    polls.on_end(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(&window, Action::EndPoll);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    polls.on_cancel_end(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(&window, Action::CancelEndPoll);
        }
    });
}

#[cfg(test)]
mod tests {
    use yapfast_core::chat::PollAnswer;

    use super::*;

    fn answers(card: &PollCard) -> Vec<(bool, String, i32, bool)> {
        card.answers
            .iter()
            .map(|row| (row.picked, row.votes.to_string(), row.percent, row.leading))
            .collect()
    }

    #[test]
    fn a_polls_card_shows_votes_once_voted_closed_or_asked() {
        let answer = |id, votes, mine| PollAnswer {
            id,
            text: format!("Answer {id}"),
            votes,
            mine,
            ..Default::default()
        };
        let mut poll = Poll {
            question: "Lunch?".into(),
            answers: vec![
                answer(1, 2, false),
                answer(2, 1, false),
                answer(3, 0, false),
            ],
            expiry: Some(10_000),
            ..Default::default()
        };
        let mut kept = HashSet::new();
        let card = |poll: &Poll, kept: &HashSet<String>| {
            poll_card("d:5", poll, &[], kept, 10_000 - 7_200, true)
        };

        // Before voting: answers to pick, and nothing to vote with yet.
        let open = card(&poll, &kept);
        assert!(!open.results && open.can_pick && open.can_show_results);
        assert_eq!(open.prompt, "Select one answer");
        assert_eq!(
            (open.total.as_str(), open.left.as_str()),
            ("3 votes", "2h left")
        );
        assert_eq!(
            (open.action, open.action_enabled),
            (PollAction::Vote, false)
        );
        kept.insert(pick_key("d:5", 2));
        let picked = card(&poll, &kept);
        assert!(picked.action_enabled);
        assert!(picked.answers.row_data(1).unwrap().picked);

        // "Show results" shows the votes, without this account's pick.
        kept.insert(results_key("d:5"));
        let shown = card(&poll, &kept);
        assert!(shown.results && !shown.can_pick && !shown.can_show_results);
        assert_eq!(shown.action, PollAction::BackToVote);
        assert_eq!(
            answers(&shown),
            [
                (false, "2 votes".into(), 67, true),
                (false, "1 vote".into(), 33, false),
                (false, "0 votes".into(), 0, false),
            ]
        );

        // Voted: its answer is marked, and the vote can be taken back.
        kept.clear();
        poll.answers[1] = answer(2, 2, true);
        let voted = card(&poll, &kept);
        assert!(voted.results);
        assert_eq!(
            (voted.action, voted.action_enabled),
            (PollAction::RemoveVote, true)
        );
        assert_eq!(
            answers(&voted)
                .iter()
                .map(|(picked, _, _, leading)| (*picked, *leading))
                .collect::<Vec<_>>(),
            [(false, true), (true, true), (false, false)]
        );

        // Closed: nothing more to do but see who voted.
        poll.finalized = true;
        poll.multiselect = true;
        let closed = card(&poll, &kept);
        assert!(closed.results && !closed.can_show_results);
        assert_eq!(closed.prompt, "");
        assert_eq!(
            (closed.left.as_str(), closed.action),
            ("Poll closed", PollAction::None)
        );
    }

    #[test]
    fn a_pinned_or_found_poll_sends_you_to_its_message_to_vote() {
        let answer = |id, votes, mine| PollAnswer {
            id,
            votes,
            mine,
            ..Default::default()
        };
        let mut poll = Poll {
            answers: vec![answer(1, 2, false), answer(2, 1, false)],
            expiry: Some(10_000),
            ..Default::default()
        };
        // "Show results" clicked on the timeline doesn't show them here.
        let kept = HashSet::from([results_key("d:5")]);
        let card = |poll: &Poll| poll_card("d:5", poll, &[], &kept, 5_000, false);
        let open = card(&poll);
        assert!(!open.interactive && !open.results && !open.can_pick && !open.can_show_results);
        assert_eq!(open.action, PollAction::None);
        assert_eq!(open.prompt, "");
        assert_eq!(
            (open.total.as_str(), open.left.as_str()),
            ("View original message to interact", "")
        );
        poll.answers[1] = answer(2, 2, true);
        let voted = card(&poll);
        assert!(voted.results && voted.answers.row_data(1).unwrap().picked);
        poll.finalized = true;
        let closed = card(&poll);
        assert_eq!(
            (closed.total.as_str(), closed.left.as_str()),
            ("4 votes", "Poll closed")
        );
    }

    #[test]
    fn a_polls_time_left_counts_down_as_discords() {
        assert_eq!(time_left(3 * 86_400 + 5), "3d left");
        assert_eq!(time_left(86_399), "23h left");
        assert_eq!(time_left(59 * 60), "59m left");
        assert_eq!(time_left(30), "< 1m left");
    }
}
