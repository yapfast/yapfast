//! A channel's permissions: what it shows and what it asks for. Task X14
//! (docs/iced/tasks/settings.md). The Slint side: `PermissionsSection`,
//! `ThreeState`, `OverwriteConfirmDialog` and `AddOverwriteDialog`
//! (`ui/channel_settings.slint:221`, `:80`, `:322`, `:383`) over
//! `ChannelSettingsPrompt`'s permissions half. The window holds it in
//! `vm.settings.channel.permissions`.

use crate::kind::Avatar;

/// Whether the channel follows its category's permissions
/// (`ChannelSettingsPrompt.sync`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sync {
    /// No category: no notice.
    #[default]
    None,
    Synced,
    /// Differs from the category's: the notice offers Sync Now.
    Apart,
}

/// A role or member with an overwrite in the list on the left.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overwrite {
    pub id: String,
    pub name: String,
    pub member: bool,
    /// The role's colour as 0xRRGGBB; 0 for none.
    pub color: u32,
    pub selected: bool,
}

/// One line of the shown overwrite's permissions: a group's heading or a
/// permission with its three buttons.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub heading: bool,
    pub title: String,
    pub description: String,
    /// The permission's bit: what a choice names.
    pub bit: u32,
    /// 0 denied, 1 left to the roles, 2 allowed.
    pub state: u8,
    /// Why it can't change, in Discord's words; empty when it can.
    pub lock: String,
}

/// A person Add's search found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub id: String,
    pub name: String,
    pub username: String,
    pub avatar: Avatar,
}

/// Add's picker, over the screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Adding {
    pub query: String,
    /// The roles without an overwrite yet.
    pub roles: Vec<Overwrite>,
    /// The members whose names start with the query.
    pub members: Vec<Found>,
}

/// What a dialog asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirm {
    /// Removing the shown overwrite.
    Remove,
    /// Syncing with the category.
    Sync,
    /// A change refused for taking the permission from this account.
    Refused,
}

/// A channel's permissions: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Permissions {
    /// "Use permissions to customize who can do what in this channel."
    pub subtitle: String,
    pub sync: Sync,
    /// The category's name, for the notice and the sync question.
    pub category: String,
    /// The channel's name, for the sync question.
    pub channel: String,
    pub private: bool,
    pub private_title: String,
    pub private_text: String,
    /// Why the switch can't change; empty when it can.
    pub private_lock: String,
    pub overwrites: Vec<Overwrite>,
    pub lines: Vec<Line>,
    /// The Remove button's words: "Remove {name}" of the shown overwrite.
    pub remove: String,
    /// The shown one is `@everyone`, which can't be removed.
    pub shown_everyone: bool,
    /// This account may change the overwrites at all.
    pub may_edit: bool,
    pub saving: bool,
    pub adding: Option<Adding>,
    pub confirm: Option<Confirm>,
    /// What the dialog names: the overwrite's name.
    pub confirm_name: String,
}

impl Permissions {
    /// The page's buttons answer: the account may edit and nothing is on
    /// its way.
    pub fn ready(&self) -> bool {
        self.may_edit && !self.saving
    }
}

/// A channel's permissions: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// An overwrite was chosen to show, by id (`input` 13).
    Show(String),
    /// A permission was set: its bit, and 0 deny, 1 neither, 2 allow
    /// (`input` 14, `bit * 3 + state`).
    Set(u32, u8),
    /// The private switch (`input` 15).
    Private(bool),
    /// Add was pressed (`input` 20).
    Add,
    /// Add's search text changed (`input` 2).
    Search(String),
    /// Add's picker closed (`input` 21).
    CloseAdd,
    /// A role was added, by id (`input` 22).
    AddRole(String),
    /// A member was added, by id (`input` 23).
    AddMember(String),
    /// Remove the shown overwrite (`input` 24).
    Remove,
    /// Sync Now (`input` 25).
    SyncNow,
    /// The dialog's confirm button (`input` 26).
    Confirm,
    /// The dialog's cancel, Escape or a press outside (`input` 27).
    Cancel,
}
