//! A server's Safety Setup: what it shows and what it asks for. Task X05
//! (docs/iced/tasks/settings.md). The Slint side is `SafetyPrompt`
//! (`ui/moderation.slint:20`) and, for Security Actions, the part of
//! `InvitesPrompt` that `security` opens (`ui/server_people.slint:77`).

use iced::Color;

/// One of a list's choices as Discord words it: its title in its own colour
/// and what it means (`SafetyOption`, `ui/moderation.slint:9`).
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    /// What a click asks for: Discord's value for it.
    pub value: u8,
    pub title: String,
    pub description: String,
    /// Discord's colour for the title; none draws it in the text colour.
    pub color: Option<Color>,
    /// Discord offers it: a Community server can't have the lowest level.
    pub enabled: bool,
}

/// Security Actions while it is open (Invites' button opens it): how long
/// to pause for, what to pause, and whether Save may go.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Actions {
    /// The place of the hours chosen in [`HOURS`].
    pub hours: usize,
    pub pause_invites: bool,
    pub pause_dms: bool,
    /// Something changed, so Save may go.
    pub may_save: bool,
    /// Discord is taking it: nothing changes meanwhile.
    pub saving: bool,
    /// Discord's refusal, empty for none.
    pub error: String,
}

/// The hours the dialog offers, in the order of its list.
pub const HOURS: [u32; 6] = [1, 2, 4, 6, 12, 24];

/// A server's Safety Setup: what its screen draws. The window holds it as
/// `vm.settings.safety`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Safety {
    /// Explains the verification level, as Discord words it for the kind of
    /// server.
    pub verification_text: String,
    pub verification_options: Vec<Offer>,
    /// The value chosen (what Discord has, or what is on its way).
    pub verification: u8,
    pub filter_options: Vec<Offer>,
    pub filter: u8,
    /// Moderators need two-factor authentication: the switch, its words, why
    /// it can't move (empty if it can), and a note under it for an owner
    /// without 2FA (empty for none).
    pub mfa_on: bool,
    pub mfa_text: String,
    pub mfa_lock: String,
    pub mfa_note: String,
    /// A choice is on its way to Discord: no other may go.
    pub saving: bool,
    /// Security Actions, open over the settings.
    pub actions: Option<Actions>,
}

/// A server's Safety Setup: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A verification level, by Discord's value.
    Verification(u8),
    /// A sensitive content filter, by Discord's value.
    Filter(u8),
    /// Moderators need two-factor authentication, on or off.
    RequireMfa(bool),
    /// Security Actions: the hours by their place in [`HOURS`].
    Hours(usize),
    PauseInvites,
    PauseDms,
    SaveActions,
    CloseActions,
}
