//! The Account settings page: what it shows and what it asks for. The Slint
//! side: `AccountSettings` (`ui/settings.slint:654`), filled by
//! `show_account` in `src/ui/refresh.rs:1700`. Task S02's
//! (docs/iced/tasks/settings.md).

use iced::widget::image;

use crate::kind::{Avatar, Status};

/// The Account settings page: what its screen draws. The window holds it as
/// `vm.settings.account`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub name: String,
    pub avatar: Avatar,
    /// The decoration round the avatar (`AccountLook.decoration-url`).
    pub decoration: Option<image::Handle>,
    /// The custom status, what they play or how the sync stands, as the
    /// card's line: the emoji's picture, then the words.
    pub status: Status,
    /// Where the login is kept, a line under the card; empty draws nothing.
    pub storage: String,
    /// The local preview: no account signed in. The sign-out button goes
    /// back to the sign-in and there is nothing to reconnect.
    pub preview: bool,
    pub online: bool,
    /// A reconnect is going: its button waits.
    pub busy: bool,
    /// A message is being sent: sign-out waits.
    pub sending: bool,
    /// A Discord account is linked: the profile and status buttons show.
    pub linked: bool,
    /// A custom status is set: its button says "Change".
    pub has_custom_status: bool,
}

impl Account {
    /// The reconnect button shows: signed in for real and not online.
    pub fn offline(&self) -> bool {
        !self.preview && !self.online
    }

    /// Sign-out may be pressed.
    pub fn may_sign_out(&self) -> bool {
        !self.busy && !self.sending
    }
}

/// The Account settings page: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Try the sync again (`retry-sync`).
    Reconnect,
    /// Open the profile editor (`edit-profile`).
    EditProfile,
    /// Open the custom status dialog (`set-custom-status`).
    SetCustomStatus,
    /// Sign out, or in the preview go back to the sign-in (`sign-out`).
    SignOut,
}
