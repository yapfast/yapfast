//! A gift's dialog, such as Nitro that a clicked gift link opened: what it
//! shows and what it asks for. The Slint side: `GiftPrompt` and
//! `GiftDialog` (`ui/server.slint:292`, `:311`), filled by `show_gift`
//! (`src/ui/refresh.rs:2934`) and answered by `on_redeem_gift` and
//! `on_open_gift_in_browser` (`src/ui/callbacks.rs:1345`). The window holds
//! it as `vm.places.gift`.

/// The gift, once found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    /// What it grants.
    pub name: String,
    /// Its plan and its giver; empty for none.
    pub detail: String,
}

/// A gift's dialog: what it grants shows before anyone redeems it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gift {
    /// "You've been sent a gift".
    pub title: String,
    pub found: Option<Found>,
    /// What the main button says, such as "Redeem Gift"; empty for none.
    pub confirm: String,
    /// The main button opens the gift in the browser, for a gift only
    /// Discord can redeem.
    pub browser: bool,
    pub busy: bool,
    /// How it's going, such as "Redeeming..."; empty for none.
    pub status: String,
    pub error: String,
}

impl Gift {
    /// What the main button asks for.
    pub fn confirmed(&self) -> Intent {
        if self.browser {
            Intent::OpenInBrowser
        } else {
            Intent::Redeem
        }
    }
}

/// A gift's dialog: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    Close,
    Redeem,
    OpenInBrowser,
}
