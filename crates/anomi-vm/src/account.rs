//! The account card at the bottom of the sidebar: who is signed in, and the
//! microphone, headphones and settings buttons.
//! The Slint side: `AccountCard` in `ui/yapfast.slint` and `show_account` in
//! `src/ui/refresh.rs`.

use crate::kind::{Avatar, Presence};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub name: String,
    /// Their custom status or what they're doing, else their presence.
    pub line: String,
    pub avatar: Avatar,
    pub presence: Presence,
    pub muted: bool,
    pub deafened: bool,
}

/// What the account card asks for. Keep `#[non_exhaustive]`: the app matches it
/// with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Intent {
    ToggleMute,
    ToggleDeafen,
    Settings,
}
