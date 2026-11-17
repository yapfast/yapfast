//! The join dialog, for an invite's server or adding one: what it shows and
//! what it asks for. The Slint side: `JoinPrompt` and `JoinDialog`
//! (`ui/server.slint:112`, `:139`), filled by `show_join`
//! (`src/ui/refresh.rs:2775`) and answered by the callbacks at
//! `src/ui/callbacks.rs:1323` to `:1360`. The window holds it as
//! `vm.places.join`.

use crate::kind::Avatar;

/// The server an invite leads to, once Discord found it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub name: String,
    /// Its icon, or its initial until the icon arrives.
    pub avatar: Avatar,
    /// Its members, or the channel joining lands in; empty for none.
    pub detail: String,
}

/// The join dialog: the field for an invite, the server once found, and how
/// joining goes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Join {
    /// "Join a Server", or "You've been invited to join a server".
    pub title: String,
    /// Whether it has a field for the invite; a clicked link's has none.
    pub field: bool,
    /// What the field holds.
    pub link: String,
    pub found: Option<Found>,
    /// What the main button says, such as "Join Server"; empty for none.
    pub confirm: String,
    /// Looking up or joining: the buttons wait.
    pub busy: bool,
    /// How it's going, such as "Joining..."; empty for none.
    pub status: String,
    /// Discord asked for a captcha, which opens again from a button.
    pub captcha: bool,
    /// The server asks for a form, which its button opens in the browser.
    pub form: bool,
    /// Joining in the browser is on offer, with a warning about the risk.
    pub risk: bool,
    /// Why it failed; empty for none.
    pub error: String,
}

impl Join {
    /// What the main button asks for: the form, joining the server found, or
    /// looking the invite up.
    pub fn confirmed(&self) -> Intent {
        if self.form {
            Intent::OpenForm
        } else if self.found.is_some() {
            Intent::Join
        } else {
            Intent::Find(self.link.clone())
        }
    }

    /// The first button closes the dialog and says Close when there is
    /// nothing more to do, Cancel while there is.
    pub fn closes_only(&self) -> bool {
        self.form || self.confirm.is_empty()
    }
}

/// The join dialog: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    Close,
    /// The field changed.
    Link(String),
    /// Look the invite up (Enter in the field, or the main button).
    Find(String),
    /// Join the server found.
    Join,
    OpenCaptcha,
    OpenForm,
    JoinInBrowser,
}
