//! The two-factor prompt: what it shows and what it asks for. Slint's
//! `MfaDialog` over an `MfaPrompt` (`ui/mfa.slint:36`, `:6`), filled by
//! `show_mfa` (`src/ui/server_settings.rs:410`) from the core's
//! `MfaPrompt` (`application/mfa.rs`). Discord holds a request back until
//! this account confirms: a code from an authenticator app or a text, the
//! password, or a backup code, then the request goes again. Cancelling
//! fails the request. The window holds it as `vm.settings.mfa`, and
//! `Layer::Mfa` opens it.

/// A way to confirm. Slint's `MfaChoice` carries an id (0 app, 1 text, 2
/// password, 3 backup code); this is the same four. A passkey, which
/// Yapfast can't use, is never listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    App,
    Sms,
    Password,
    Backup,
}

/// How long Discord's warning keeps its button off, in seconds
/// (`property <int> wait: 10`, `ui/mfa.slint:56`).
pub const WARNING_WAIT: u32 = 10;

use crate::menu::Row;

/// The two-factor prompt: what its screen draws. Its `Debug` leaves out
/// what was typed: a password, or a code.
#[derive(Clone, Default, PartialEq)]
pub struct Mfa {
    /// Which prompt it is: one waiting after another starts its own wait,
    /// so the app starts the warning's count over when it changes.
    pub id: u64,
    /// Listing the ways to confirm, as "Verify with something else" does,
    /// rather than using one.
    pub listing: bool,
    /// Discord's warning before a backup code, "Slow down! Are you
    /// sure?", which Confirm goes past.
    pub warning: bool,
    /// Seconds left before the warning's button works; the app counts it
    /// down once a second while the warning shows and the window is in
    /// front ([`Intent::Tick`]), and starts it at [`WARNING_WAIT`] each
    /// time the warning shows.
    pub wait: u32,
    /// What it asks for, and the field's placeholder; no field when Discord
    /// offers only a passkey.
    pub text: String,
    pub field: String,
    pub password: bool,
    /// What was typed in the field.
    pub code: String,
    /// The ways to confirm, each a row sending [`Intent::Choose`], its words
    /// already in the language ("Use your authenticator app"); with more
    /// than one, another may be chosen.
    pub choices: Vec<Row<Intent>>,
    /// A text's code may be sent again.
    pub resend: bool,
    /// The answer was sent and waits for Discord.
    pub busy: bool,
    /// The answer may be sent.
    pub ready: bool,
    /// Why Discord refused; empty for none.
    pub error: String,
}

impl Mfa {
    /// The prompt asks for a code or password in a field.
    pub fn asks(&self) -> bool {
        !self.listing && !self.warning && !self.field.is_empty()
    }
}

/// The two-factor prompt: what a click or a key on it asks for
/// (`choose`, `write`, `resend`, `confirm`, `cancel`: `ui/mfa.slint:41`;
/// `Action::ChooseMfa`, `WriteMfa`, ... in `src/ui.rs:3251`). Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant. Its `Debug` leaves out what was typed.
#[derive(Clone, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A way in the list.
    Choose(Method),
    /// "Verify with something else": back to the list (`choose(-1)`).
    Back,
    /// The field's text changed.
    Write(String),
    /// "Resend Code" of a text.
    Resend,
    /// Confirm, Enter in the field, or the warning's button.
    Confirm,
    /// Cancel, the X or Escape: the request fails.
    Cancel,
    /// A second of the warning's wait went by.
    Tick,
}

impl std::fmt::Debug for Mfa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Every field named, so a new one is a choice to show or hide.
        let Self {
            id,
            listing,
            warning,
            wait,
            text,
            field,
            password,
            code,
            choices,
            resend,
            busy,
            ready,
            error,
        } = self;
        f.debug_struct("Mfa")
            .field("id", id)
            .field("listing", listing)
            .field("warning", warning)
            .field("wait", wait)
            .field("text", text)
            .field("field", field)
            .field("password", password)
            .field("code", &crate::sign_in::Hidden(code))
            .field("choices", choices)
            .field("resend", resend)
            .field("busy", busy)
            .field("ready", ready)
            .field("error", error)
            .finish()
    }
}

impl std::fmt::Debug for Intent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Choose(method) => f.debug_tuple("Choose").field(method).finish(),
            Self::Back => f.write_str("Back"),
            Self::Write(typed) => f
                .debug_tuple("Write")
                .field(&crate::sign_in::Hidden(typed))
                .finish(),
            Self::Resend => f.write_str("Resend"),
            Self::Confirm => f.write_str("Confirm"),
            Self::Cancel => f.write_str("Cancel"),
            Self::Tick => f.write_str("Tick"),
        }
    }
}
