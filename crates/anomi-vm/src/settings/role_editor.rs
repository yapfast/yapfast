//! A server's role editor: what it shows and what it asks for. Task X09
//! (docs/iced/tasks/settings.md). The Slint side: the open half of
//! `RolesSection`, `RoleTabButton`, `RoleMemberLine`, `RoleConfirmDialog`
//! and `AddMembersDialog` (`ui/roles.slint:295`, `:312`, `:664`, `:713`)
//! with the glue of `src/ui/server_settings.rs:518` (`roles_prompt`).
//!
//! The role's title in the frame's bar is `Server.title`, which is `Some`
//! while this editor is open. The two dialogs it asks for, [`Confirm`] and
//! [`Adding`], are drawn over the window by the shell's layer.

use iced::Color;

use crate::kind::{Avatar, Expected};

/// A tab of the editor (`tab`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Display,
    Permissions,
    Members,
}

/// One of Discord's role colours (`RoleSwatch`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swatch {
    pub color: Color,
    pub chosen: bool,
}

/// A row of the Permissions tab: a group's heading, or a permission's
/// switch by its bit (`PermissionRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Permission {
    pub heading: bool,
    pub title: String,
    pub description: String,
    /// A warning after the description, in bold.
    pub warning: String,
    pub bit: u32,
    pub on: bool,
    /// Why the switch can't change, in Discord's words; empty when it can.
    pub lock: String,
}

/// A member on Manage Members or in Add Members (`RoleMemberRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Member {
    pub id: String,
    pub name: String,
    pub username: String,
    pub avatar: Avatar,
    /// Picked in Add Members.
    pub chosen: bool,
}

/// What the editor asks before it goes on (`confirm`): the dialog the
/// shell draws over the window.
#[derive(Clone, Debug, PartialEq)]
pub enum Confirm {
    /// Delete the role, by its name.
    Delete { role: String },
    /// Take the role from a member, by their names.
    Remove { role: String, member: String },
}

/// Add Members while it's open: the search of the server's members and
/// those picked, up to [`ADD_LIMIT`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Adding {
    pub role: String,
    pub search: String,
    pub candidates: Vec<Member>,
}

/// Most members one Add Members adds (`ADD_MEMBERS_LIMIT`).
pub const ADD_LIMIT: usize = 30;

impl Adding {
    /// How many are picked.
    pub fn picked(&self) -> usize {
        self.candidates
            .iter()
            .filter(|member| member.chosen)
            .count()
    }

    /// Add is live: some are picked and not too many.
    pub fn may_add(&self) -> bool {
        (1..=ADD_LIMIT).contains(&self.picked())
    }
}

/// A server's role editor: what its screen draws. The window holds it as
/// `vm.settings.role_editor`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoleEditor {
    /// The role open, by Discord's id.
    pub id: String,
    /// It is @everyone: only Permissions is open.
    pub everyone: bool,
    pub tab: Tab,
    /// This account can't change the role (`locked`).
    pub locked: bool,
    /// Why, or what manages it: a line under the Back link.
    pub notice: String,
    /// Discord is taking a change: nothing edits.
    pub saving: bool,
    /// A picture for the icon is being read.
    pub reading: bool,
    // Display.
    pub name: String,
    /// The role has no colour: Discord's grey is chosen.
    pub default_color: bool,
    /// Discord's colours, a bright row then a dark one.
    pub bright: Vec<Swatch>,
    pub dark: Vec<Swatch>,
    /// The colour, as the custom one's square shows it.
    pub color: Color,
    /// What the hex field holds, "#rrggbb" once it was set.
    pub hex: String,
    /// The colour is none of Discord's: the custom square is ticked.
    pub custom_color: bool,
    /// Display role members separately from online members.
    pub hoist: bool,
    pub mentionable: bool,
    pub icon: Expected,
    /// The emoji that stands in for a picture, shown in the well.
    pub emoji: String,
    /// What the emoji field holds before it's used.
    pub emoji_text: String,
    pub has_icon: bool,
    /// "Unlocks at Level 2", until the server's level gives roles icons.
    pub icon_locked: String,
    // Permissions.
    pub permission_search: String,
    pub permissions: Vec<Permission>,
    pub may_clear: bool,
    /// Why Clear Permissions can't be used, in its tooltip.
    pub clear_lock: String,
    // Manage Members.
    /// "Manage Members (3)".
    pub members_title: String,
    pub member_search: String,
    pub members_loading: bool,
    pub members_error: String,
    pub members: Vec<Member>,
    /// More hold the role than Discord listed.
    pub not_all: bool,
    pub may_add: bool,
    // Dialogs.
    pub confirm: Option<Confirm>,
    pub adding: Option<Adding>,
}

impl RoleEditor {
    /// Fields and switches take input: nothing is saving and the role isn't
    /// locked (`editable`).
    pub fn editable(&self) -> bool {
        !self.saving && !self.locked
    }
}

/// A server's role editor: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Back to the list (`open("")`).
    Back,
    /// A tab was chosen (`tab(n)`).
    Tab(Tab),
    /// The role's name changed (`write(1, text)`).
    Name(String),
    /// A colour: `None` Discord's default, else one of its twenty by place,
    /// the bright row first (`color(n)`).
    Color(Option<usize>),
    /// The hex field changed: what it holds, not yet a colour.
    HexText(String),
    /// The hex field was entered: set the colour (`write(6, text)`).
    Hex(String),
    /// Display role members separately (`toggle(0)`).
    Hoist,
    /// Allow anyone to @mention (`toggle(1)`).
    Mentionable,
    /// Choose Image (`pick-icon`).
    PickIcon,
    /// Remove Icon (`remove-icon`).
    RemoveIcon,
    /// The emoji field changed.
    EmojiText(String),
    /// Use Emoji, or Enter in its field (`write(5, text)`).
    Emoji(String),
    /// The permissions' search changed (`write(2, text)`).
    PermissionSearch(String),
    /// Clear permissions (`toggle(2)`).
    ClearPermissions,
    /// A permission's switch, by its bit (`permission(bit)`).
    Permission(u32),
    /// The members' search changed (`write(3, text)`).
    MemberSearch(String),
    /// Add Members: opens its dialog (`act(6, "")`).
    AddMembers,
    /// A member's remove button: the app asks first (`act(3, id)`).
    RemoveMember(String),
    /// The confirm dialog's Delete or Remove.
    Confirm,
    /// The confirm dialog's Cancel or Escape.
    Cancel,
    /// Add Members' search changed.
    AddingSearch(String),
    /// A member was ticked or unticked in Add Members, by id.
    AddingPick(String),
    /// Add Members' Add.
    AddingAdd,
    /// Add Members' Cancel or Escape.
    AddingClose,
}

/// A colour written as hex, "#rrggbb" or without its "#"
/// (`hex_color`, `src/ui/server_settings.rs:756`).
pub fn hex_color(text: &str) -> Option<u32> {
    let hex = text.trim();
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}
