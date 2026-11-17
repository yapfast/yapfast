//! Adding a friend: what it shows and what it asks for. Task P03
//! (docs/iced/tasks/people.md). The Slint side is `AddFriendDialog`
//! (`ui/friends.slint:420`), filled by `src/ui/refresh.rs:3740`
//! (`show_dialogs`) and answered by `src/ui/callbacks.rs:1612`
//! (`Action::AddFriend`).

/// How the last request went, which the line under the field says.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Note {
    /// Nothing yet: the line is empty.
    #[default]
    None,
    /// The request is on its way ("Sending...").
    Sending,
    /// Discord took it, for this name: "Sent {name} a friend request." in
    /// green.
    Sent(String),
    /// Discord refused, or it failed: what it said (the core's text, shown
    /// as it is), in red.
    Failed(String),
}

/// Adding a friend: what its screen draws. The window holds it as
/// `vm.people.add_friend`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AddFriend {
    /// What the field holds. The app empties it as the dialog opens and
    /// again when a request went (`refresh.rs:3742`, `:3749`).
    pub name: String,
    pub note: Note,
}

impl AddFriend {
    /// A request is on its way: the button and the Enter key do nothing.
    pub fn sending(&self) -> bool {
        self.note == Note::Sending
    }

    /// Whether the request may go: a name, and none already on its way
    /// (`enabled: !sending && name != ""`).
    pub fn may_send(&self) -> bool {
        !self.sending() && !self.name.is_empty()
    }
}

/// Adding a friend: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The field changed: the app keeps what was typed.
    Name(String),
    /// Send a friend request to this username (`Action::AddFriend`); asked
    /// by the button and Enter, never while one is sending or the name is
    /// empty.
    Send(String),
    /// The close button (Escape and a press outside are the shell's).
    Close,
}
