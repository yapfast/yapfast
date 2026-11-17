//! The composer's slash commands: what it shows and what it asks for. The
//! Slint side: `CommandFrame`, `CommandHeading`, `CommandList` and
//! `CommandPills` (`ui/app_commands.slint:22`, `:31`, `:40`, `:117`); what
//! the app fills them from is `show_app_commands`
//! (`src/ui/app_commands.rs`), which reads the core's `command_rows`,
//! `command_heading` and `command_draft`.
//!
//! Two things come of one `/`: the list over the composer, which first
//! offers the room's commands (each with its app) and, once one is chosen,
//! the values the focused option may take; and the pills over the field's
//! text, the command's name and one pill per option. [`Commands::shown`]
//! is the window's `command-picker-open`: the list shows while `/` is typed
//! even with nothing to list yet (its heading then says why), but an
//! option's values only when there are some. Up, Down, Tab, Enter and
//! Escape are the composer's keys ([`Suggest`](super::composer::Suggest),
//! [`Key`](super::composer::Key)); [`Commands::chosen`] is the row they
//! moved to.

use iced::widget::image;

/// The composer's slash commands: what its screen draws. The window holds
/// it as `vm.chat.composer.commands`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Commands {
    /// The list over the composer shows (`command-picker-open`).
    pub shown: bool,
    /// What it lists, or in place of rows, why there are none ("Loading
    /// commands…", "No commands match"); the core has made it capitals
    /// (`command-heading`).
    pub heading: String,
    /// Its rows: the commands, or the focused option's values.
    pub choices: Vec<Choice>,
    /// The highlighted row (`command-choice-index`).
    pub chosen: usize,
    /// The rows are commands, written with their `/`, and one without a
    /// picture draws a "/" in its place (`command-rows-are-commands`).
    pub commands: bool,
    /// The command being written, shown as pills over the field's text.
    pub written: Option<Written>,
}

/// A row of the list: a command the picker offers, with its app and the
/// command's name (a subcommand's is "config set") and description; or a
/// value an option may take, with a person's avatar for its picture and no
/// app (`CommandChoice`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choice {
    /// The app's name, at the row's right end.
    pub app: String,
    /// The app's icon, or the person's avatar; none until it is loaded.
    pub icon: Option<image::Handle>,
    pub name: String,
    pub description: String,
}

/// The command being written (`command-name`, `command-options`,
/// `command-focus`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Written {
    /// Its name, as the first pill writes it after a "/".
    pub name: String,
    pub options: Vec<Pill>,
    /// The option the text writes; outlined.
    pub focused: usize,
}

/// An option of the command being written (`CommandOptionPill`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pill {
    pub name: String,
    pub description: String,
    /// What was entered, as it shows.
    pub value: String,
    pub required: bool,
    /// Why it is wrong, in red; empty when it is not.
    pub error: String,
    /// It takes a file: a file icon opens it, and a click picks the file.
    pub file: bool,
}

/// The composer's slash commands: what a click on them asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A click on the list's row at this index: take it (`pick-command`).
    Pick(usize),
    /// A click on the pill of the option at this index: the text writes it
    /// now (`focus-command-option`).
    Option(usize),
    /// A click on the pill of a file option: pick its file (`command-file`).
    File(usize),
}
