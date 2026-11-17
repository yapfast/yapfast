//! A new thread: what it shows and what it asks for. Task C25
//! (docs/iced/tasks/chat.md). The Slint side is `NewThreadPanel`
//! (`ui/thread.slint:452`); what the app filled it from is `show_new_thread`
//! (`src/ui/refresh.rs:1789`), and what it asked for is
//! `src/ui/callbacks.rs:1677`-`:1709` and `:1761`.
//!
//! Discord's "New Thread" form, beside the channel in place of its member
//! list: the message it starts from, if any, its name, whether it is
//! private, and the first message, which Enter sends with the thread.
//! Closing it after writing asks first.

use crate::chat::search::Found;
use crate::text::Link;

/// A new thread: what its screen draws. The window holds it as
/// `vm.chat.new_thread`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewThread {
    /// The message it starts from, shown as a found message (`from-message`,
    /// `starter`).
    pub starter: Option<Found>,
    /// The name typed (`name`).
    pub name: String,
    /// What an empty name becomes, or "New Thread" when this is empty
    /// (`name-placeholder`).
    pub placeholder: String,
    /// The first message (`body`).
    pub body: String,
    /// The first message's draft key, which a new form changes so its
    /// draft shows afresh.
    pub key: u64,
    /// The first message's field has the keys: the frame's outline lightens.
    pub focused: bool,
    /// The form offers a private thread (`can-private`).
    pub can_private: bool,
    /// ... and only that (`only-private`).
    pub only_private: bool,
    /// The thread is private (`private`).
    pub private: bool,
    /// The thread is being made: nothing takes input (`starting`).
    pub starting: bool,
    /// Why it wasn't made, when it wasn't (`error`).
    pub error: String,
    /// Closing asks first, as something was written (`leaving`).
    pub leaving: bool,
}

impl NewThread {
    /// The name's own label: it is optional when the thread starts from a
    /// message, which names it.
    pub fn name_optional(&self) -> bool {
        self.starter.is_some()
    }

    /// Create may be pressed: nothing is being made already.
    pub fn can_create(&self) -> bool {
        !self.starting
    }

    /// The Create button is the form's main action: something is written
    /// and nothing is being made.
    pub fn emphasised(&self) -> bool {
        !self.starting && !self.body.is_empty()
    }
}

/// A new thread: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The name's text changed (`name-edited`).
    Name(String),
    /// Enter in the name: the keys go to the first message
    /// (`focus-composer`); the app moves the focus to
    /// `focus::ASIDE_COMPOSER`.
    ToBody,
    /// The first message's text changed, with the caret's byte offset
    /// (`body-edited`).
    Body { text: String, caret: usize },
    /// The first message's field got or lost the keys (`composer-focused`).
    Focus(bool),
    /// The switch was flipped to this (`private-changed`).
    Private(bool),
    /// Enter in the message, or the send button (`create`).
    Create,
    /// The cross (`close`): asks first when something was written.
    Close,
    /// "Exit" in the question (`leave`).
    Leave,
    /// "Back to editing" in the question (`stay`).
    Stay,
    /// Show the message it starts from (`jump-to`).
    Jump { room: String, item: String },
    /// A link in that message's text (`open-link`).
    Link(Link),
}
