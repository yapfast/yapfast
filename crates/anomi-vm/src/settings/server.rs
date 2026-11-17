//! A server's settings frame and its profile page: what it shows and what it
//! asks for. Task X01 (docs/iced/tasks/settings.md). The Slint side:
//! `ServerSettingsScreen` and `ServerProfileSection`
//! (`ui/server_settings.slint:320`, `:100`) with the glue of
//! `src/ui/server_settings.rs`.
//!
//! The page itself is another module's view-model (`settings::engagement`,
//! `settings::members`...): the window draws the one `section` names beside
//! this.

use crate::kind::Avatar;
use crate::settings::frame::Unsaved;

/// A page of a server's settings. Slint numbered them (`section-id`: 0
/// profile, 1 engagement, 2 boost perks, 3 roles, 4 safety, 5 bans, 6 audit
/// log, 7 members, 8 invites, 9 emoji, 10 stickers).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Section {
    #[default]
    Profile,
    Engagement,
    BoostPerks,
    Roles,
    Safety,
    Bans,
    AuditLog,
    Members,
    Invites,
    Emoji,
    Stickers,
}

/// The server profile page: its name, icon and description.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    /// What the name field holds.
    pub name: String,
    /// What the description holds.
    pub description: String,
    /// The icon as it will be: Discord's, or a new one's preview; its
    /// letter and tint stand in without a picture.
    pub icon: Avatar,
    /// There is an icon to remove (`has-icon`).
    pub has_icon: bool,
    /// A chosen picture is being read: the hint says so, the buttons wait.
    pub reading: bool,
    /// How many characters the description has left; below 0, too many.
    pub description_left: i32,
}

/// A server's settings frame: what its screen draws. The window holds it as
/// `vm.settings.server`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Server {
    /// The server's name, over the sections.
    pub name: String,
    /// The sections this account may open, in the order of [`Section`]'s
    /// groups; an empty group's heading is left out.
    pub sections: Vec<Section>,
    /// Delete Server shows last in the sections, for its owner.
    pub may_delete: bool,
    /// The section showing.
    pub section: Section,
    /// The section's data is on its way (`loading`).
    pub loading: bool,
    /// The section's data is here (`loaded`); its page draws only then.
    pub loaded: bool,
    /// Discord is taking a change: the fields go still.
    pub saving: bool,
    /// A title that replaces the section's own in the bar: an open role's.
    pub title: Option<String>,
    /// Quieter words after the title: Boost Perks' level.
    pub detail: String,
    /// The page takes the frame's height instead of scrolling in it (a
    /// role's permissions, bans, audit log, members, invites, emoji).
    pub fill: bool,
    /// What the section shows that Discord refused (`problem`) or failed at
    /// (`error`).
    pub problem: String,
    pub error: String,
    /// The bar over the page's bottom while changes are not saved.
    pub unsaved: Option<Unsaved>,
    /// What the close button's tooltip says.
    pub close: String,
    pub profile: Profile,
}

/// A server's settings frame and its profile page: what a click or a key on
/// it asks for. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A section was chosen.
    Choose(Section),
    /// Delete Server in the sections: opens its dialog.
    Delete,
    /// The close button.
    Close,
    /// The unsaved bar's Reset.
    Undo,
    /// The unsaved bar's Save Changes.
    Save,
    /// The name field changed (`write(0, text)`).
    Name(String),
    /// The description changed (`write(1, text)`).
    Description(String),
    /// "Change Server Icon": pick a file (`pick-picture(0)`).
    PickIcon,
    /// "Remove Icon" (`remove-picture(0)`).
    RemoveIcon,
}
