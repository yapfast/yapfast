//! The About settings page, with updates: what it shows and what it asks
//! for. Task S10 (docs/iced/tasks/settings.md). The Slint side:
//! `AboutSettings` (`ui/settings.slint:1962`) and `src/ui/updates.rs`.

use super::frame::Build;

/// What the update check found (`UpdateStatus`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Not asked yet: the line says nothing, or that checks are off.
    #[default]
    Idle,
    Checking,
    Current,
    Available,
    /// A newer release the person chose to skip.
    Skipped,
    Failed,
    /// A build from source: nothing to update.
    Source,
}

/// An update being prepared.
#[derive(Clone, Debug, PartialEq)]
pub struct Install {
    /// "Downloading 40%".
    pub text: String,
    /// How far it is, none while that isn't known.
    pub progress: Option<f32>,
    /// Cancelling still works (before the program is replaced).
    pub can_cancel: bool,
}

/// The About settings page, with updates: what its screen draws. The window
/// holds it as `vm.settings.about`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct About {
    /// The build; its headline and channel head the card.
    pub build: Build,
    /// The build was copied a moment ago.
    pub copied: bool,
    /// The program's name, in the text about a fork.
    pub product: String,
    /// The label of the button to the original project: "Original: Yapfast".
    pub original: String,
    /// This build has releases to look for.
    pub releases: bool,
    /// The package manager that installed it, whose it is to update.
    pub manager: String,
    pub status: Status,
    /// The release's version, for `Available` and `Skipped`.
    pub latest: String,
    /// Why the check failed.
    pub error: String,
    /// The release's notes.
    pub notes: String,
    /// "Update and restart" is offered.
    pub can_install: bool,
    /// What an update will do, or why this copy can't.
    pub install_note: String,
    /// Why the last update failed.
    pub install_error: String,
    pub installing: Option<Install>,
    /// The switches: check for updates, include nightlies.
    pub check: bool,
    pub nightlies: bool,
    /// Those switches are new to this person: their names carry NEW
    /// (`new-key` "check-updates" and "update-nightlies").
    pub fresh_check: bool,
    pub fresh_nightlies: bool,
}

/// The About settings page, with updates: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Copy the build as one line.
    CopyVersion,
    Install,
    CancelInstall,
    /// Open the release's page.
    ViewRelease,
    SkipVersion,
    CheckNow,
    Check(bool),
    Nightlies(bool),
    OpenGithub,
    OpenWebsite,
}
