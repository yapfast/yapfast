//! The Discord settings page: what it shows and what it asks for. Task S08
//! (docs/iced/tasks/settings.md), after `DiscordSettings` and
//! `DiscordLinkSettings` (`ui/settings.slint:2195`, `:2110`), filled by
//! `src/ui/refresh.rs` and answered by `src/ui/callbacks.rs`. Every switch is
//! reported as the value it should have, and the page shows what it is
//! given.

use crate::kind::Avatar;

/// How many blocked accounts show at first, and how many more each "See
/// more" adds (`blocked-shown: 5`).
pub const BLOCKED_STEP: usize = 5;

/// Someone the Discord account blocked (`BlockedRow`).
#[derive(Clone, Debug, PartialEq)]
pub struct Blocked {
    /// The user id, which unblocking names.
    pub id: String,
    pub name: String,
    /// Their handle; empty when Discord gave none.
    pub username: String,
    pub avatar: Avatar,
}

/// The Discord settings page: what its screen draws. The window holds it as
/// `vm.settings.discord`.
#[derive(Clone, Debug, PartialEq)]
pub struct Discord {
    /// A Discord account is linked; until then the page offers to link one.
    pub linked: bool,
    /// The build has Matrix too, where Unlink is the page's first line.
    pub matrix_build: bool,
    pub show_hidden_channels: bool,
    pub sync_reads: bool,
    pub show_idle: bool,
    pub hide_blocked: bool,
    pub blocked: Vec<Blocked>,
    /// How many of the blocked show; the app grows it by [`BLOCKED_STEP`]
    /// on `MoreBlocked`.
    pub blocked_shown: usize,
}

impl Default for Discord {
    fn default() -> Self {
        Self {
            linked: true,
            matrix_build: false,
            show_hidden_channels: false,
            sync_reads: false,
            show_idle: false,
            hide_blocked: false,
            blocked: Vec::new(),
            blocked_shown: BLOCKED_STEP,
        }
    }
}

/// The Discord settings page: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Show the code to link an account (`link-discord`), while none is.
    Link,
    /// Sign out of Discord on this computer (`unlink-discord`).
    Unlink,
    ShowHiddenChannels(bool),
    SyncReads(bool),
    ShowIdle(bool),
    HideBlocked(bool),
    /// Unblock someone, by user id.
    Unblock(String),
    /// Show five more of the blocked.
    MoreBlocked,
}
