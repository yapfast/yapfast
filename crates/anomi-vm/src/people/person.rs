//! About a person: nickname and note, a call, unfriending, blocking, and in
//! a server a nickname, a timeout, a kick or a ban: what the dialog shows and
//! what it asks for. Task P11 (docs/iced/tasks/people.md). The Slint side is
//! `PersonDialog` and `PersonPrompt` (`ui/dialogs.slint:680`, `:705`), filled
//! by `src/ui/refresh.rs:3587` (`show_person_dialog`, which says each ask's
//! words) and answered by `src/ui/callbacks.rs:1026`, `:1314`, `:1604`
//! (`Action::AnswerPerson`, `ChoosePersonOption`, `ClosePersonDialog`).

use crate::menu::Row;

/// What the dialog draws: the core's `PersonDialog` put into words by the
/// projection. The window holds it as `vm.people.person`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Person {
    /// "Note on Bob", "Block Bob?".
    pub title: String,
    /// What it explains, under the title; empty for none.
    pub text: String,
    /// Over the choices, such as "DURATION"; empty when there are none.
    pub options_heading: String,
    /// Choices to pick one of, such as a timeout's length, the picked one
    /// (the core's `choice`) checked: build them with [`choices`]. They are
    /// rows because the view lays them out as menu items, which borrow them.
    pub options: Vec<Row<Intent>>,
    /// What the field is for ("Note", "Reason (optional)"); empty for a
    /// dialog that only asks, which has no field.
    pub field: String,
    /// What the field holds. The projection fills it from the note or
    /// nickname there is once it arrives, not again while it is open
    /// (`refresh.rs:3591`); the app keeps what is typed ([`Intent::Text`]).
    pub text_now: String,
    /// A box to tick under it ("Don't ask again" on a call), ticked or not
    /// (`skip_call_prompt`): build it with [`tick`]; none for no box.
    pub tick: Option<Row<Intent>>,
    /// What its button says.
    pub confirm: String,
    /// What it does is hard to undo: the button is red.
    pub danger: bool,
    /// The field's text is on its way: it shows "Loading..." and takes
    /// nothing, and the button waits.
    pub loading: bool,
    /// What was asked is on its way: the button waits.
    pub saving: bool,
    /// Why it failed, in red above the buttons; empty for none.
    pub error: String,
}

/// The rows of a dialog's choices: `labels` in order, `chosen` checked, each
/// sending [`Intent::Choose`] with its index.
pub fn choices<S: Into<String>>(
    labels: impl IntoIterator<Item = S>,
    chosen: usize,
) -> Vec<Row<Intent>> {
    labels
        .into_iter()
        .enumerate()
        .map(|(index, label)| Row::new(label, Intent::Choose(index)).checked(index == chosen))
        .collect()
}

/// The box under the field: a checkbox row that sends [`Intent::Tick`] with
/// the opposite of how it stands.
pub fn tick(label: impl Into<String>, ticked: bool) -> Row<Intent> {
    Row::new(label, Intent::Tick(!ticked)).checkbox(ticked)
}

impl Person {
    /// The dialog asks for text.
    pub fn has_field(&self) -> bool {
        !self.field.is_empty()
    }

    /// The button and Enter may answer (`enabled: !loading && !saving`).
    pub fn may_answer(&self) -> bool {
        !self.loading && !self.saving
    }
}

/// What a click or a key on it asks for. Keep `#[non_exhaustive]`: the app
/// matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The field changed: the app keeps the whole text.
    Text(String),
    /// The button, or Enter: answer with the field's text (`Action::
    /// AnswerPerson`; empty for a dialog without a field). Never while it
    /// loads or saves.
    Answer(String),
    /// A choice was picked, by its index (`Action::ChoosePersonOption`).
    Choose(usize),
    /// The box was ticked or cleared, to this (`skip-call-prompt-toggled`).
    Tick(bool),
    /// Cancel and the X (Escape and a press outside are the shell's;
    /// `Action::ClosePersonDialog`).
    Close,
}
