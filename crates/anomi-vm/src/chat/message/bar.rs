//! The hover bar: what floats over the top edge of the lit message, as
//! Discord's client offers it (`message-bar`, `ui/yapfast.slint:3953`).

use iced::widget::image;

/// The hover bar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bar {
    /// The reactions this device uses most, three, before the picker's
    /// button; left out where the conversation is narrow (`bar-reactions`).
    pub frequent: Vec<Quick>,
    /// What this account may do to the message here.
    pub may: May,
    /// It's among the conversation's pinned messages: Shift offers Unpin.
    pub pinned: bool,
    /// Shift is held, and neither the picker nor the menu is open for it:
    /// Pin, Create Thread and, on one's own message, Reply take the
    /// frequent reactions' place, and Delete, which doesn't ask, More's.
    pub shift: bool,
    /// Its deletion waits: "Delete for everyone?" with Delete and Cancel
    /// in the bar's place (`confirm-delete`).
    pub confirming: bool,
}

/// What may be done to a message from its bar. The projection folds in
/// what the conversation allows: the composer taking text, reactions,
/// threads, forwarding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct May {
    /// React to it: the picker's button, and the frequent reactions.
    pub react: bool,
    /// Edit it: it's this account's own (`own && can-edit`), and the
    /// composer takes text. Edit then stands where Reply would.
    pub edit: bool,
    /// Reply to it, the composer taking text.
    pub reply: bool,
    pub forward: bool,
    /// Pin or unpin it.
    pub pin: bool,
    /// Start a thread from it: the channel allows threads, it may be
    /// replied to and has none yet.
    pub thread: bool,
    /// Delete it, the composer taking text.
    pub delete: bool,
}

/// One of the frequent reactions: Unicode's emoji, drawn by Twemoji, or a
/// server's with its picture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quick {
    /// What reacting adds: the emoji itself, or a server's `name:id`.
    pub key: String,
    /// Its name in its tooltip, as ":thumbsup:".
    pub name: String,
    /// A server's emoji's picture; none for Unicode's.
    pub picture: Option<image::Handle>,
}

/// What lies over a message, which says where its ground starts and how
/// high its bar floats (`head`, `ui/yapfast.slint:3801`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Edge {
    /// Another message: its ground reaches up over the space between them
    /// to its row's top, and the bar floats 16 px over that.
    #[default]
    Message,
    /// Nothing, the first row of the list: the bar is 4 px inside it.
    Top,
    /// A date, the "New" divider or the history's edge: the ground starts
    /// at the message, and the bar floats 4 px over it where it would reach
    /// the middle of the divider's line, 16 px where it wouldn't.
    Divider,
}
