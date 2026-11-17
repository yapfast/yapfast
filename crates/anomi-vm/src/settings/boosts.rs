//! A server's Boost Perks page: what it shows and what it asks for. Task X02
//! (docs/iced/tasks/settings.md). The Slint side: `BoostSection`,
//! `BoostBackground` (`ui/server_settings.slint:280`, `:237`) and
//! `BoostPrompt`.

use iced::widget::image;

/// Which background (Slint's `pick(1)` and `pick(2)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    /// The server banner's.
    Banner,
    /// The server invite's.
    Invite,
}

/// A banner or invite background as the page shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picture {
    /// The picture, Discord's or a new one's preview; none yet shows words.
    pub picture: Option<image::Handle>,
    /// There is one to remove (`shown`).
    pub shown: bool,
    /// The server's boost level doesn't unlock it yet: "Unlocks at Level 2".
    pub locked: String,
}

/// A server's Boost Perks page: what its screen draws. The window holds it
/// as `vm.settings.boosts`; the level beside the title is the frame's
/// (`Server::detail`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Boosts {
    pub banner: Picture,
    pub invite: Picture,
    pub progress_bar: bool,
    /// Discord is taking a change: the controls go still.
    pub saving: bool,
    /// A chosen picture is being read: the buttons wait.
    pub reading: bool,
}

/// A server's Boost Perks page: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// "Upload Background": pick a file.
    Pick(Background),
    /// "Remove Background".
    Remove(Background),
    /// The progress bar's switch.
    ToggleProgressBar,
}
