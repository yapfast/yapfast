//! Authorizing an application from its link: what the dialog shows and what
//! it asks for. The Slint side: `OAuthPrompt` and `OAuthDialog`
//! (`ui/server.slint:366`, `:395`), filled by `show_oauth`
//! (`src/ui/refresh.rs:2996`) and answered by `on_choose_oauth_guild`,
//! `on_authorize_oauth` and `on_close_oauth` (`src/ui/callbacks.rs:1327`).
//! The window holds it as `vm.places.oauth`.

use crate::kind::Avatar;

/// The application, once found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct App {
    pub name: String,
    /// Its picture, or its initial until it arrives.
    pub avatar: Avatar,
    /// What it says about itself or the bot it adds; empty for none.
    pub detail: String,
}

/// The servers the application could be added to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Servers {
    pub names: Vec<String>,
    /// The index of the one chosen.
    pub chosen: usize,
}

/// Authorizing an application: what it asks for shows before anyone agrees.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OAuth {
    /// The dialog's title.
    pub title: String,
    pub app: Option<App>,
    /// What it asks for, a line each; empty for none.
    pub wants: String,
    /// What of that can take over the account or a server, in red above it;
    /// empty for none.
    pub warning: String,
    /// The servers it offers, for a server install.
    pub servers: Option<Servers>,
    /// The one server a link names, which can't be changed; empty for none.
    pub server: String,
    /// Where Discord sends the person next when Yapfast won't open it, to
    /// copy; empty for none.
    pub address: String,
    /// The address was copied: Copy reads "Copied". The app sets it on
    /// [`Intent::Copy`] and clears it when the address changes.
    pub copied: bool,
    /// What the main button says, such as "Authorize"; empty for none.
    pub confirm: String,
    pub busy: bool,
    pub status: String,
    pub error: String,
}

/// Authorizing an application: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    Close,
    /// Add it to the server at this index of [`Servers::names`].
    ChooseServer(usize),
    Authorize,
    /// Put the address on the clipboard.
    Copy(String),
}
