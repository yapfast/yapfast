//! An incoming call and the question before switching calls: what they show
//! and what they ask for. Slint's `IncomingCall` and `SwitchDialog`
//! (`ui/voice.slint:1213`, `:1269`), set by `show_incoming` (`src/ui.rs:3345`)
//! and the switch prompt of `src/ui/refresh.rs:4959`, answered by
//! `src/ui/callbacks.rs:1095`.

use crate::kind::Avatar;

/// A call ringing this account in a DM that isn't open, and the prompt to
/// switch calls. The window holds it as `vm.voice.incoming`; the layers
/// `IncomingCall` and `SwitchCall` show its two halves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Incoming {
    /// The DM ringing, as `Accept` and `Decline` name it.
    pub room: String,
    /// Who calls: the DM's name.
    pub name: String,
    /// "Incoming call", or "Bob and Carol are calling" for a group's.
    pub detail: String,
    pub avatar: Avatar,
    /// What switching calls asks.
    pub switch: Switch,
}

/// The question before leaving one call for another, or joining one to
/// watch a stream.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Switch {
    /// What it asks, as the app words it (`switch_prompt`).
    pub text: String,
    /// It joins a call from none, to watch someone's stream: the title and
    /// the button say "Watch stream?" and "Join".
    pub joins: bool,
}

/// What a click or a key on either asks for.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// "Accept": join the call ringing in `room` (`Action::AcceptCall`).
    Accept(String),
    /// "Decline": turn the call in `room` down (`Action::DeclineCall`).
    Decline(String),
    /// "Switch channel" or "Join", or Enter (`Action::ConfirmVoiceSwitch`).
    ConfirmSwitch,
    /// "Cancel" or Escape (`Action::CancelVoiceSwitch`). A press on the
    /// backdrop is the shell's close: the app maps it to this too.
    CancelSwitch,
}
