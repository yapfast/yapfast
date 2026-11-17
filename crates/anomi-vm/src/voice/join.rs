//! Joining a voice channel or a DM's call: what it shows and what it asks
//! for. Task V07 (docs/iced/tasks/voice.md), after `JoinVoice` and
//! `CallInvite` (`ui/voice.slint:629`, `:598`) and the projection in
//! `src/ui/refresh.rs:4724`-`:4790`.

use crate::kind::Avatar;

/// Joining a voice channel or a DM's call: what its screen draws. The window
/// holds it as `vm.voice.join`; the conversation's `Call::Elsewhere` says it
/// shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Join {
    #[default]
    None,
    /// A voice channel this account isn't in (`JoinVoice`).
    Channel(Channel),
    /// A DM's call going on, or ringing (`CallInvite`).
    Call(Call),
}

/// A voice channel: what it is, and a way to join.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Channel {
    pub name: String,
    /// Why it can't be joined, or what it is (`voice-description`).
    pub description: String,
    /// The local preview: the encryption notice never shows there.
    pub preview: bool,
    /// Discord's call is end-to-end encrypted; an unencrypted channel is
    /// refused.
    pub encrypted: bool,
    /// Another voice channel is joined: the button switches.
    pub switching: bool,
    pub can_join: bool,
}

/// A DM's call.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Call {
    /// Who is in it, as avatars.
    pub people: Vec<Avatar>,
    /// "Bob in the call." or "Bob calling you.".
    pub note: String,
    /// It rings: Decline and Accept, rather than Join the call.
    pub incoming: bool,
    /// The DM's room, which an answer names.
    pub room: String,
    pub can_join: bool,
}

/// Joining a voice channel or a DM's call: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Join the voice channel in view, or switch to it (`join-voice`).
    Join,
    /// Answer a DM's call: accept it, or decline it (`answer-call`).
    Answer { room: String, accept: bool },
}
