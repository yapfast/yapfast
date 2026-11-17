//! Deleting a server or handing it over: what it shows and what it asks for.
//! Slint's `OwnershipPrompt` (`ui/server_ownership.slint:6`), filled by
//! `ownership_prompt` (`src/ui/server_settings.rs:348`), so the words come
//! already translated, the server's and the new owner's names in bold. The
//! window holds it as `vm.settings.ownership`.

use crate::kind::Avatar;
use crate::text::Text;

/// Which dialog is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// Neither: the view draws nothing.
    #[default]
    None,
    /// "Delete 'Server'".
    Delete,
    /// "Transfer Ownership".
    Transfer,
}

/// Deleting a server or handing it over: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ownership {
    pub kind: Kind,
    pub title: String,
    /// Discord's words, with the names bold.
    pub text: Text,
    /// Delete Server asks for the server's name typed without it...
    pub by_name: bool,
    /// ...and says when it is wrong.
    pub wrong: bool,
    /// What the field holds: the server's name, or the emailed code.
    pub typed: String,
    /// Transfer Ownership: both owners' avatars.
    pub owner: Avatar,
    pub new_owner: Avatar,
    /// Its acknowledgement, and whether it is ticked.
    pub acknowledge: Text,
    pub agreed: bool,
    /// The server is Verified or Partnered, which blocks the transfer.
    pub verified: bool,
    /// The emailed code's step.
    pub code: bool,
    /// Another code is on its way, or has come.
    pub resending: bool,
    pub resent: bool,
    /// It was sent and waits for Discord: nothing may be asked again.
    pub sending: bool,
    /// Whether the transfer button works.
    pub may_confirm: bool,
    /// Why Discord refused; empty for none.
    pub error: String,
}

/// What a click or a key on it asks for (the `act` callbacks of
/// `ui/server_ownership.slint:64`, `:103`: 0 typed, 1 confirm, 2 cancel,
/// 3 acknowledge, 4 resend). Keep `#[non_exhaustive]`: the app matches it
/// with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The field's text changed.
    Write(String),
    /// Delete Server or Transfer Ownership, or Enter in the field.
    Confirm,
    /// Cancel; the layer's Escape and a press outside ask it too.
    Cancel,
    /// The acknowledgement's box.
    Agree,
    /// "Didn't receive a code or it expired? Resend it."
    Resend,
}
