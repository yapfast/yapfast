//! The settings frame: which page of the person's settings shows, who is
//! signed in above the sections, the build under them, and the bar that says
//! a draft differs from what is saved. The Slint side: `SettingsScreen`
//! (`ui/settings.slint:582`), `SettingsNavigation`
//! (`ui/yapfast/settings-nav.slint:10`), `BuildStamp` and `UpdateNotice`
//! (`ui/yapfast/build-info.slint:78`, `:108`) and `UnsavedBar`
//! (`ui/settings.slint:311`). Task S01's.
//!
//! The page itself is another module's view-model (`settings::account`,
//! `settings::voice`...): the window draws the one `page` names beside this.

use crate::kind::Avatar;

/// A page of the person's settings. Slint numbered them (`settings-section`:
/// 0 account, 1 voice and video, 2 messages, 3 notifications, 4 Discord,
/// 5 sounds, 6 advanced, 7 appearance, 8 game activity, 9 about,
/// 10 keybinds); the sections list them in the order of [`Page::ALL`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Page {
    #[default]
    Account,
    Voice,
    Messages,
    Notifications,
    Sounds,
    Appearance,
    Discord,
    Games,
    Keybinds,
    Advanced,
    About,
}

impl Page {
    /// Every page, in the sections' order.
    pub const ALL: [Page; 11] = [
        Page::Account,
        Page::Voice,
        Page::Messages,
        Page::Notifications,
        Page::Sounds,
        Page::Appearance,
        Page::Discord,
        Page::Games,
        Page::Keybinds,
        Page::Advanced,
        Page::About,
    ];

    /// The pages that exist only in a build with Discord: the sections hide
    /// them otherwise (`with-discord`, `ui/yapfast/settings-nav.slint:93`).
    pub fn needs_discord(self) -> bool {
        matches!(self, Page::Discord | Page::Games)
    }
}

/// What kind of build this is, which tints its second line: a release in the
/// live green, a nightly in the accent, a development build in red
/// (`BuildInfo.tone` 0, 1, 2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Release,
    Nightly,
    Development,
}

/// The two lines under the sections that say which build is running, so a
/// report says whether it has a feature.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Build {
    /// The first line: "Anomi acd138".
    pub headline: String,
    /// The second line's bold start: "Development build".
    pub channel: String,
    /// What follows it, quieter: the build's date.
    pub details: String,
    /// What a click copies, as one line.
    pub summary: String,
    pub tone: Tone,
}

/// Discord's bar for a draft that differs from what is saved.
#[derive(Clone, Debug, PartialEq)]
pub struct Unsaved {
    /// The draft is on its way: both buttons wait, and Save says so.
    pub saving: bool,
    pub may_reset: bool,
    pub may_save: bool,
    /// Counts the times leaving was refused for the changes: each one shakes
    /// the bar and keeps it red a moment, as Discord's does.
    pub nudge: u32,
}

impl Default for Unsaved {
    fn default() -> Self {
        Self {
            saving: false,
            may_reset: true,
            may_save: true,
            nudge: 0,
        }
    }
}

/// The settings frame: what its screen draws. The window holds it as
/// `vm.settings.frame`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    /// The page showing.
    pub page: Page,
    /// Who is signed in, above the sections.
    pub name: String,
    pub avatar: Avatar,
    /// The profile can be edited: Discord is linked.
    pub may_edit_profile: bool,
    /// The build has Discord, so its pages show.
    pub with_discord: bool,
    /// What the search over the sections holds.
    pub query: String,
    /// The pages with something new on them, which get a NEW tag until seen.
    pub fresh: Vec<Page>,
    /// The build under the sections; none while it isn't known.
    pub build: Option<Build>,
    /// The version an update is available for.
    pub update: Option<String>,
    /// What the close button's tooltip says: "Close Settings".
    pub close: String,
    /// The build was copied a moment ago: it says so instead of its channel.
    pub copied: bool,
    /// The bar over the page's bottom, while a draft differs.
    pub unsaved: Option<Unsaved>,
}

/// The settings frame: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A section was chosen (opening Voice & Video also refreshes the
    /// devices, `ui/settings.slint:640`).
    Choose(Page),
    /// The close button; Escape is the layer's.
    Close,
    /// "Edit Profile" under the name: closes the settings and opens the
    /// editor.
    EditProfile,
    /// The text of the search over the sections changed.
    Search(String),
    /// The build's summary, `frame.build.summary`, for the clipboard.
    Copy,
    /// The unsaved bar's Reset.
    Undo,
    /// The unsaved bar's Save Changes.
    Save,
}
