//! The Requests screen: what it shows and what it asks for. Task P02
//! (docs/iced/tasks/people.md). The Slint side: `RequestsScreen`,
//! `RequestItem` and `RequestRow` (`ui/friends.slint:345`, `:82`, `:6`) and
//! `request_rows` (`src/ui/refresh.rs:6094`), whose handlers are at
//! `src/ui/callbacks.rs:979`, `:1062`.
//!
//! A person's friend request and their message request are one row, as the
//! Slint screen lists them; the projection sorts the rows into those waiting
//! for this account's answer, those it sent, and spam.

use crate::kind::Avatar;
use crate::text::Text;

/// The Requests screen: what its screen draws. The window holds it as
/// `vm.people.requests`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Requests {
    /// Waiting for this account's answer (`incoming`).
    pub incoming: Vec<Request>,
    /// The friend requests this account sent (`sent`).
    pub sent: Vec<Request>,
    /// What Discord took for spam (`spam`); folded away until opened.
    pub spam: Vec<Request>,
    /// The spam is unfolded (`spam-open`).
    pub spam_open: bool,
    /// Discord's mark shows on its people (the `Badges.discord` setting).
    pub marks: bool,
}

/// What a person asked of this account, if anything, as a friend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Friend {
    /// Only a message (`friend: 0`).
    #[default]
    No,
    /// They sent this account a friend request (`1`).
    Asked,
    /// This account sent them one (`2`).
    Sent,
}

/// Someone waiting for an answer (`RequestRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    /// Their user id, or a group's room id: what Accept and Ignore name.
    pub id: String,
    pub name: String,
    pub avatar: Avatar,
    pub friend: Friend,
    /// Their message request, when they sent one.
    pub message: Option<Message>,
}

/// A message request's latest message, under who sent it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Message {
    /// The conversation it opens (`room`).
    pub room: String,
    /// Empty when nothing is known of it (`text`): the row invites to open it.
    pub text: Text,
    /// When it came, as words (`time`); empty when unknown.
    pub time: String,
    pub unread: bool,
}

impl Requests {
    /// Nothing waits, was sent or was held back as spam.
    pub fn is_empty(&self) -> bool {
        self.incoming.is_empty() && self.sent.is_empty() && self.spam.is_empty()
    }
}

impl Request {
    /// They wrote to this account and it has not read it.
    pub fn unread(&self) -> bool {
        self.message.as_ref().is_some_and(|message| message.unread)
    }
}

/// The Requests screen: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Accept their friend request (their user id): `Action::Befriend(user)`
    /// (the Slint screen's `befriend(id)`). Not `AnswerRequest(bool)`, which
    /// takes no user: it answers the message request of the open DM.
    Accept(String),
    /// Ignore the friend request they sent, or cancel the one this account
    /// sent (their user id): `RemoveFriendRequest`.
    Remove(String),
    /// Open the conversation their message is in (its room id), which
    /// answers the message request.
    Open(String),
    /// Fold or unfold the spam.
    ToggleSpam,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_waiting_is_empty() {
        let mut requests = Requests::default();
        assert!(requests.is_empty());
        requests.spam.push(Request::default());
        assert!(!requests.is_empty());
    }

    #[test]
    fn a_request_is_unread_with_an_unread_message() {
        let mut request = Request::default();
        assert!(!request.unread());
        request.message = Some(Message {
            unread: true,
            ..Message::default()
        });
        assert!(request.unread());
    }
}
