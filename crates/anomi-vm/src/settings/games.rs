//! The Game activity settings page: what it shows and what it asks for. Task
//! S08 (docs/iced/tasks/settings.md), after `GameActivitySettings`
//! (`ui/settings.slint:2134`) and `src/ui/games.rs`. The two drafts and the
//! game being edited are the window's, as the Slint page's private
//! properties were: the app keeps what was typed and projects it back.

/// The longest a game's text may be (`src/ui/games.rs`), in characters.
pub const TEXT_LIMIT: usize = 128;

/// A game named by its executable and what to show after Playing
/// (`GameExecutable`).
#[derive(Clone, Debug, PartialEq)]
pub struct Executable {
    pub executable: String,
    pub text: String,
}

/// The Game activity settings page: what its screen draws. The window holds
/// it as `vm.settings.games`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Games {
    /// The Discord account shares the game it plays.
    pub shown: bool,
    /// A Discord account is linked; the switch is locked without one.
    pub linked: bool,
    pub games: Vec<Executable>,
    /// The game the form edits; `None` adds a new one.
    pub editing: Option<usize>,
    pub executable_draft: String,
    pub text_draft: String,
    /// The last save was refused.
    pub invalid: bool,
}

/// The Game activity settings page: what a click or a key on it asks for.
/// Keep `#[non_exhaustive]`: the app matches it with a wildcard until its I
/// task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Share the game, or stop (`toggled`).
    Share(bool),
    /// The fields were edited: the whole text.
    Executable(String),
    Text(String),
    /// Fill the form with a game to change.
    Edit(usize),
    /// Remove a game (`remove`).
    Remove(usize),
    /// Add the game, or save the one being edited (`save`): the app takes
    /// `editing` and the two drafts it holds, and sets `invalid` when the
    /// game is refused.
    Save,
    /// Leave the edit and empty the form.
    Cancel,
}
