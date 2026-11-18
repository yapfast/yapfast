//! A message's poll and the poll being made (`src/ui/polls.rs`'s `bind`).
//! The answers picked and "Show results" are this device's own, kept with
//! the timeline's rows; a vote goes to the core and shows at once, its row
//! alone built again, until the core's poll counts it or Discord refuses
//! it (`project::chat::kept::moved`). One on its way can't be sent twice.

use iced::Task;
use yapfast_core::application::{Action, PollEdit};

use anomi_vm::chat::poll::{Action as Act, Intent};

use crate::app::App;
use crate::message::Message;
use crate::project::Reach;
use crate::project::chat::kept::Poll;

pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let edit = |app: &mut App, edit| app.dispatch(Action::EditPoll(edit));
    match intent {
        Intent::Pick(key, answer) => pick(app, key, answer),
        Intent::Act(key, action) => act(app, key, action),
        Intent::ShowResults(key) => keep(app, key, |poll| poll.results = true),
        Intent::Voters(key, answer) => {
            super::on(app, key, |item| Action::ShowVoters { item, answer })
        }
        Intent::CancelEnd(_) => app.dispatch(Action::CancelEndPoll),
        Intent::End(_) => app.dispatch(Action::EndPoll),
        Intent::Question(text) => edit(app, PollEdit::Question(text)),
        Intent::Answer(index, text) => edit(app, PollEdit::Answer { index, text }),
        Intent::PickEmoji(index, spot) => super::super::pickers::for_poll(app, index, spot),
        Intent::RemoveEmoji(index) => edit(app, PollEdit::Emoji { index, emoji: None }),
        Intent::AddAnswer => edit(app, PollEdit::AddAnswer),
        Intent::RemoveAnswer(index) => edit(app, PollEdit::RemoveAnswer(index)),
        Intent::Duration(index) => edit(app, PollEdit::Duration(index)),
        Intent::Multiselect(on) => edit(app, PollEdit::Multiselect(on)),
        Intent::Post => app.dispatch(Action::PostPoll),
        Intent::Close => app.dispatch(Action::ClosePollDraft),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The item of the row `key`'s message, and whether its poll takes several
/// answers.
fn poll_of(app: &App, key: u64) -> Option<(String, bool)> {
    let message = app.shown.timeline.message(key)?;
    let poll = message.poll.as_ref()?;
    Some((message.item.clone(), poll.multiselect))
}

/// Changes what this device keeps of the row `key`'s poll, and builds that
/// row again.
fn keep(app: &mut App, key: u64, change: impl FnOnce(&mut Poll)) -> Task<Message> {
    let Some((item, _)) = poll_of(app, key) else {
        return Task::none();
    };
    change(app.shown.timeline.kept.poll(&item));
    app.shown.timeline.again.push(key);
    app.refresh();
    Task::none()
}

/// An answer picked, or unpicked; one answer of a poll taking one only.
fn pick(app: &mut App, key: u64, answer: u64) -> Task<Message> {
    let Some((_, multiselect)) = poll_of(app, key) else {
        return Task::none();
    };
    keep(app, key, |poll| {
        if let Some(at) = poll.picked.iter().position(|&picked| picked == answer) {
            poll.picked.remove(at);
        } else {
            if !multiselect {
                poll.picked.clear();
            }
            poll.picked.push(answer);
        }
    })
}

/// The button beside the votes.
fn act(app: &mut App, key: u64, action: Act) -> Task<Message> {
    let Some((item, _)) = poll_of(app, key) else {
        return Task::none();
    };
    let poll = app.shown.timeline.kept.poll(&item);
    if poll.voting.is_some() {
        return Task::none();
    }
    match action {
        Act::Vote => {
            let answers = poll.picked.clone();
            vote(app, key, item, answers)
        }
        // Taking the vote back starts the poll afresh.
        Act::RemoveVote => {
            *poll = Poll::default();
            vote(app, key, item, Vec::new())
        }
        Act::BackToVote => keep(app, key, |poll| poll.results = false),
        Act::None => Task::none(),
    }
}

/// Votes for `answers` (none takes the vote back). Sent to Discord, it
/// shows at once until the core's poll counts it.
fn vote(app: &mut App, key: u64, item: String, answers: Vec<u64>) -> Task<Message> {
    let action = Action::Vote {
        item: item.clone(),
        answers: answers.clone(),
    };
    let reach = Reach::after(&action);
    let effects = app.core.dispatch(action);
    if !effects.is_empty() {
        let kept = &mut app.shown.timeline.kept;
        kept.poll(&item).voting = Some(answers);
        kept.notice.clone_from(&app.core.state.notice);
    }
    app.shown.timeline.again.push(key);
    let task = crate::effects::run(app, effects);
    app.show(reach);
    task
}
