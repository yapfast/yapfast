//! Forwarding a message: what it shows and what it asks for. Task C28
//! (docs/iced/tasks/chat.md). The Slint side is `ForwardDialog`
//! (`ui/dialogs.slint:231`), filled by `src/ui/refresh.rs:3824` and answered
//! by `src/ui/callbacks.rs:1278` and `:1842`.
//!
//! As Discord's client does it: choose up to `limit` conversations, add a
//! message if you like, and send. Clicking outside and Escape are the shell's.

pub use crate::switcher::Mark;

/// A place the message can go (`PlaceRow` with `chosen`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Target {
    /// What `Intent::Choose` carries back.
    pub id: String,
    pub name: String,
    /// Where it is: "Direct message", a server's name.
    pub place: String,
    /// A person's, group's or server's picture, or a channel's icon.
    pub mark: Mark,
    /// Chosen to forward the message to.
    pub chosen: bool,
}

/// Forwarding a message: what its screen draws. The window holds it as
/// `vm.chat.forward`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Forward {
    /// The message, quoted in one line.
    pub quote: String,
    /// What the search field holds; choosing a place clears it.
    pub query: String,
    /// The note to send with it; the app clears it as the dialog opens.
    pub note: String,
    /// The places, as the search left them.
    pub targets: Vec<Target>,
    /// How many places are chosen, which may be more than the list shows.
    pub chosen: usize,
    /// The most places there may be (`application::FORWARD_LIMIT`).
    pub limit: usize,
    /// The send button's words: "Send", or "Send (3)" for two places or
    /// more (`tr!("Send ({})", chosen)`).
    pub send: String,
    /// What the list says when it is empty, and why.
    pub empty: String,
    /// Which chosen channels won't get the note, for their slowmode; empty
    /// for none.
    pub slowmode: String,
    /// Why the last try failed, and where; empty for none.
    pub error: String,
}

impl Forward {
    /// Whether the place at `index` may be chosen or dropped: a chosen one
    /// always, another while there is room.
    pub fn can_choose(&self, index: usize) -> bool {
        self.targets
            .get(index)
            .is_some_and(|target| target.chosen || self.chosen < self.limit)
    }

    /// Whether Send does anything: some place is chosen.
    pub fn can_send(&self) -> bool {
        self.chosen > 0
    }
}

/// Forwarding a message: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The search field changed (`Action::SearchForward`).
    Query(String),
    /// The note field changed; the app keeps it for `Send`.
    Note(String),
    /// Choose or drop the place with this id (`Action::ChooseForward`). Never
    /// asked of a place that `can_choose` refuses.
    Choose(String),
    /// Send with the note and the places chosen (`Action::Forward`). Never
    /// asked while none is chosen.
    Send,
    /// The close button (Escape and a press outside are the shell's).
    Close,
}
