//! A message's poll, and the dialog that makes one: what it shows and what it
//! asks for. Task C07 (docs/iced/tasks/chat.md). The Slint side is `PollView`
//! (`ui/messages.slint:690`) and `PollDialog` (`ui/dialogs.slint:932`), filled
//! by `src/ui/polls.rs` (`poll_card`, `show_poll_draft`) and answered by the
//! same file's `bind`.

use crate::kind::Expected;
use crate::menu::Spot;
use crate::people::status::Emoji;

/// What the button beside the votes does (`PollAction`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Action {
    /// No button: the poll is closed.
    #[default]
    None,
    /// Vote for the answers picked.
    Vote,
    /// Take this account's vote back.
    RemoveVote,
    /// From the results shown before voting, back to the answers.
    BackToVote,
}

/// One answer of a poll (`PollAnswerRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Answer {
    /// Its id in the poll; what a click on it carries.
    pub id: u64,
    pub text: String,
    /// Its emoji, if it has one.
    pub emoji: Option<Emoji>,
    /// Picked to vote for; with the results showing, voted for by this
    /// account.
    pub picked: bool,
    /// Its votes as "3 votes", and its share of all of them, for the results.
    pub votes: String,
    pub percent: u8,
    /// No answer has more votes.
    pub leading: bool,
}

/// A message's poll (`PollCard`). The default one, with no answers, is a
/// message with no poll: nothing draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Poll {
    /// The message's key: what each intent carries.
    pub key: u64,
    pub question: String,
    /// "Select one answer" while it takes votes; empty once closed.
    pub prompt: String,
    pub multiselect: bool,
    pub answers: Vec<Answer>,
    /// Each answer's votes show: this account voted, the poll closed, or
    /// "Show results" was pressed. Pressing an answer then shows who voted.
    pub results: bool,
    /// Pressing an answer picks it.
    pub can_pick: bool,
    /// "5 votes", then "3h left" or "Poll closed" (empty when it never ends).
    pub total: String,
    pub left: String,
    pub action: Action,
    pub action_enabled: bool,
    pub can_show_results: bool,
    /// This account may close it now: it's the author's. The message's menu
    /// offers it; the card only asks to make sure ([`Self::confirming`]).
    pub can_end: bool,
    /// It can be voted in here, rather than shown in a pinned message or a
    /// search result, whose presses go to their card.
    pub interactive: bool,
    /// Closing it before its time is being made sure of (`Polls.confirm-end`).
    pub confirming: bool,
}

/// An answer of the poll being made (`PollAnswerDraft`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    /// Its place by id: removing one takes its field with it.
    pub id: u64,
    pub text: String,
    pub emoji: Option<Emoji>,
}

impl Draft {
    /// A server emoji's picture on its way, as the picker hands it over.
    pub fn waiting(&self) -> bool {
        self.emoji
            .as_ref()
            .is_some_and(|emoji| emoji.glyph.is_empty() && emoji.picture == Expected::Waiting)
    }
}

/// How long the poll takes votes: the places of `Dialog::duration`.
pub const DURATIONS: usize = 7;

/// The dialog that makes a poll. The window holds it as `vm.chat.poll`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dialog {
    pub question: String,
    /// "12 / 300", as the question grows.
    pub question_count: String,
    pub answers: Vec<Draft>,
    /// Its place among the [`DURATIONS`] offered.
    pub duration: usize,
    pub multiselect: bool,
    pub can_add: bool,
    pub can_post: bool,
    pub posting: bool,
    /// What's too long, or why posting failed; empty says nothing.
    pub problem: String,
}

/// A message's poll, and the dialog that makes one: what a click or a key on
/// it asks for. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Pick the answer, or unpick it, on the message's poll (`Polls.pick`).
    Pick(u64, u64),
    /// The button beside the votes (`Polls.act`).
    Act(u64, Action),
    /// "Show results" (`Polls.show-results`).
    ShowResults(u64),
    /// Who voted for an answer, or for any with none (`Polls.voters`).
    Voters(u64, Option<u64>),
    /// Close the confirmation (`Polls.cancel-end`).
    CancelEnd(u64),
    /// End the poll now (`Polls.end`).
    End(u64),
    /// The question's words changed.
    Question(String),
    /// The words of the answer at this index changed.
    Answer(usize, String),
    /// The emoji of the answer at this index is wanted from the picker,
    /// beside the well.
    PickEmoji(usize, Spot),
    RemoveEmoji(usize),
    AddAnswer,
    RemoveAnswer(usize),
    /// A duration was chosen, by its place.
    Duration(usize),
    Multiselect(bool),
    /// Post it (Enter in the question or an answer does too).
    Post,
    /// The close button and Cancel (Escape and a press outside are the
    /// shell's).
    Close,
}
