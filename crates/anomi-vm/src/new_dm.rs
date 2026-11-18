//! The new DM and group DM dialog: what it shows and what it asks for. Task
//! N11 (docs/iced/tasks/shell.md). The Slint side is `NewDmDialog`
//! (`ui/dialogs.slint:137`), filled by `src/ui/refresh.rs:3787` and answered
//! by `src/ui/callbacks.rs:1815`, `:1825`.

use crate::kind::Avatar;

/// One person the search found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Person {
    /// Their Discord or Matrix user id, which `Intent::Start` carries.
    pub id: String,
    pub name: String,
    /// Who they are to this account ("Discord friend"), or their Matrix id.
    pub detail: String,
    pub avatar: Avatar,
    /// From Discord, which the row marks. The app sets it only while marks
    /// show (the default Discord-only build has none).
    pub discord: bool,
}

/// The new DM and group DM dialog: what its screen draws. The window holds it
/// as `vm.new_dm`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewDm {
    /// What the field holds; the app clears it as the dialog opens.
    pub query: String,
    /// Matrix ids can be typed too (the placeholder says so).
    pub matrix: bool,
    /// Who it found, Discord's first.
    pub people: Vec<Person>,
    /// A conversation is starting: the rows take no press.
    pub starting: bool,
    /// What it says under the rows: why the last try failed, that one is
    /// starting, what to type, or that no one was found. Empty says nothing.
    pub note: String,
}

/// The new DM and group DM dialog: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The field changed: search for this (`Action::NewDmQuery`).
    Query(String),
    /// Write to the person with this id, starting the conversation
    /// (`Action::StartDm`): by id, as rows may move between the view a
    /// click came from and the click. The app sends it once at a time:
    /// nothing while `starting`.
    Start(String),
    /// The close button (Escape and a press outside are the shell's).
    Close,
}
