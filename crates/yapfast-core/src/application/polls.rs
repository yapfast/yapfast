//! Discord polls: making one, as Discord's "Create a Poll" dialog does,
//! voting, and closing this account's poll before its time, once asked to
//! make sure. Who voted shows in the Reactions window; see `reactions`.

use super::*;
use crate::chat::{NewPoll, NewPollAnswer, Permission};

/// How long a new poll takes votes, in hours, as Discord's dialog offers:
/// from an hour to two weeks.
pub const POLL_HOURS: [u32; 7] = [1, 4, 8, 24, 72, 168, 336];
/// A day, Discord's choice until another is made.
const DEFAULT_DURATION: usize = 3;
/// The most answers a poll has.
pub const POLL_ANSWERS: usize = 10;
/// The longest question and answer, in characters.
pub const QUESTION_LIMIT: usize = 300;
pub const ANSWER_LIMIT: usize = 55;

/// A poll being made in the "Create a Poll" dialog, for the open Discord
/// conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PollDraft {
    pub room: String,
    pub question: String,
    pub answers: Vec<DraftAnswer>,
    /// The id the next answer added gets.
    next: u32,
    /// Its place in [`POLL_HOURS`].
    pub duration: usize,
    pub multiselect: bool,
    /// On its way to Discord.
    pub posting: bool,
    pub error: Option<String>,
}

/// An answer of a poll being made. Its id tells it from the others while
/// they come and go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DraftAnswer {
    pub id: u32,
    pub text: String,
    /// The emoji itself, or a custom one's `name:id`.
    pub emoji: Option<String>,
}

impl PollDraft {
    /// An empty poll, with two answers to fill, as Discord's dialog starts.
    fn new(room: String) -> Self {
        Self {
            room,
            question: String::new(),
            answers: (0..2)
                .map(|id| DraftAnswer {
                    id,
                    ..Default::default()
                })
                .collect(),
            next: 2,
            duration: DEFAULT_DURATION,
            multiselect: false,
            posting: false,
            error: None,
        }
    }

    /// What's too long to post, if anything.
    pub fn problem(&self) -> Option<&'static str> {
        if self.question.chars().count() > QUESTION_LIMIT {
            Some(yapfast_lang::tr!(
                "The question can be at most 300 characters."
            ))
        } else if self
            .answers
            .iter()
            .any(|answer| answer.text.chars().count() > ANSWER_LIMIT)
        {
            Some(yapfast_lang::tr!("Answers can be at most 55 characters."))
        } else {
            None
        }
    }

    /// It has a question and an answer, none too long, and isn't on its way.
    pub fn can_post(&self) -> bool {
        !self.posting
            && !self.question.trim().is_empty()
            && self
                .answers
                .iter()
                .any(|answer| !answer.text.trim().is_empty())
            && self.problem().is_none()
    }

    /// Another answer can be added.
    pub fn can_add(&self) -> bool {
        self.answers.len() < POLL_ANSWERS
    }

    /// The poll it makes, without the answers left empty.
    fn poll(&self) -> NewPoll {
        NewPoll {
            question: self.question.trim().to_owned(),
            answers: self
                .answers
                .iter()
                .filter(|answer| !answer.text.trim().is_empty())
                .map(|answer| NewPollAnswer {
                    text: answer.text.trim().to_owned(),
                    emoji: answer.emoji.clone(),
                })
                .collect(),
            hours: POLL_HOURS[self.duration],
            multiselect: self.multiselect,
        }
    }
}

/// A change to the poll being made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PollEdit {
    Question(String),
    Answer {
        index: usize,
        text: String,
    },
    /// An answer's emoji, or none.
    Emoji {
        index: usize,
        emoji: Option<String>,
    },
    AddAnswer,
    RemoveAnswer(usize),
    /// Its place in [`POLL_HOURS`].
    Duration(usize),
    Multiselect(bool),
}

impl Application {
    /// Whether a poll can be made in the open conversation: a Discord DM or
    /// channel this account may write in, and in a server, post polls.
    pub fn can_create_polls(&self) -> bool {
        self.state.conversation_that(|_| true).is_some_and(|room| {
            room.network == Network::Discord
                && self.state.discord_online
                && self.state.forum().is_none()
                && self.state.can_write_in(&room.id)
                && (room.direct || self.may(&room.id, Permission::SendPolls))
        })
    }

    /// Open the "Create a Poll" dialog for the open conversation.
    pub(super) fn open_poll_draft(&mut self) -> Vec<Effect> {
        if self.can_create_polls()
            && let Some(room) = self.state.selected.clone()
        {
            self.state.poll_draft = Some(PollDraft::new(room));
        }
        vec![]
    }

    pub(super) fn edit_poll(&mut self, edit: PollEdit) -> Vec<Effect> {
        let Some(draft) = &mut self.state.poll_draft else {
            return vec![];
        };
        if draft.posting {
            return vec![];
        }
        draft.error = None;
        match edit {
            PollEdit::Question(text) => draft.question = text,
            PollEdit::Answer { index, text } => {
                if let Some(answer) = draft.answers.get_mut(index) {
                    answer.text = text;
                }
            }
            PollEdit::Emoji { index, emoji } => {
                if let Some(answer) = draft.answers.get_mut(index) {
                    answer.emoji = emoji.filter(|emoji| !emoji.is_empty());
                }
            }
            PollEdit::AddAnswer if draft.can_add() => {
                draft.answers.push(DraftAnswer {
                    id: draft.next,
                    ..Default::default()
                });
                draft.next += 1;
            }
            PollEdit::AddAnswer => {}
            // One answer stays to fill.
            PollEdit::RemoveAnswer(index)
                if draft.answers.len() > 1 && index < draft.answers.len() =>
            {
                draft.answers.remove(index);
            }
            PollEdit::RemoveAnswer(_) => {}
            PollEdit::Duration(index) if index < POLL_HOURS.len() => draft.duration = index,
            PollEdit::Duration(_) => {}
            PollEdit::Multiselect(on) => draft.multiselect = on,
        }
        vec![]
    }

    /// Post the poll being made. The dialog closes once Discord has it.
    pub(super) fn post_poll(&mut self) -> Vec<Effect> {
        let preview = self.state.preview;
        let Some(draft) = &mut self.state.poll_draft else {
            return vec![];
        };
        if !draft.can_post() {
            return vec![];
        }
        if preview {
            self.state.poll_draft = None;
            return vec![];
        }
        draft.posting = true;
        vec![Effect::Backend(Command::SendPoll {
            room: draft.room.clone(),
            poll: draft.poll(),
        })]
    }

    pub(super) fn poll_posted(&mut self, room: String, result: Result<(), String>) {
        let Some(draft) = &mut self.state.poll_draft else {
            return;
        };
        if draft.room != room || !draft.posting {
            return;
        }
        match result {
            Ok(()) => self.state.poll_draft = None,
            Err(error) => {
                draft.posting = false;
                draft.error = Some(error);
            }
        }
    }

    /// Vote for `answers` of the poll in `item` in place of this account's
    /// vote, or take it back with none.
    pub(super) fn vote(&mut self, item: String, answers: Vec<u64>) -> Vec<Effect> {
        let Some(room) = self.state.room_of(&item).cloned() else {
            return vec![];
        };
        let now = crate::clock::now_ms() / 1000;
        let Some(poll) = self
            .state
            .message(&item)
            .and_then(|message| message.poll.as_ref())
        else {
            return vec![];
        };
        let known = answers
            .iter()
            .all(|answer| poll.answers.iter().any(|known| known.id == *answer));
        if poll.closed(now) || !known || (answers.len() > 1 && !poll.multiselect) {
            return vec![];
        }
        if self.state.preview {
            self.update_sample(&room, &item, |message| {
                for answer in message.poll.iter_mut().flat_map(|poll| &mut poll.answers) {
                    let mine = answers.contains(&answer.id);
                    if mine != answer.mine {
                        answer.mine = mine;
                        answer.votes = if mine {
                            answer.votes + 1
                        } else {
                            answer.votes.saturating_sub(1)
                        };
                    }
                }
            });
            return vec![];
        }
        vec![Effect::Backend(Command::Vote {
            room,
            item,
            answers,
        })]
    }

    /// Ask to make sure before closing this account's poll in `item`.
    pub(super) fn ask_end_poll(&mut self, item: String) -> Vec<Effect> {
        let now = crate::clock::now_ms() / 1000;
        if self
            .state
            .message(&item)
            .and_then(|message| message.poll.as_ref())
            .is_some_and(|poll| poll.can_end && !poll.closed(now))
        {
            self.state.confirm_end_poll = Some(item);
        }
        vec![]
    }

    /// Close the poll asked about now.
    pub(super) fn end_poll(&mut self) -> Vec<Effect> {
        let Some(item) = self.state.confirm_end_poll.take() else {
            return vec![];
        };
        let Some(room) = self.state.room_of(&item).cloned() else {
            return vec![];
        };
        if self.state.preview {
            self.update_sample(&room, &item, |message| {
                if let Some(poll) = &mut message.poll {
                    poll.finalized = true;
                    poll.can_end = false;
                }
            });
            return vec![];
        }
        vec![Effect::Backend(Command::EndPoll { room, item })]
    }
}
