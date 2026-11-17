//! A server's role list: what it shows and what it asks for. Task X08
//! (docs/iced/tasks/settings.md). The Slint side: `RolesSection`'s list,
//! `RoleListRow` and `RoleMark` (`ui/roles.slint:341`, `:143`, `:127`) with
//! the glue of `src/ui/server_settings.rs:518` (`roles_prompt`).
//!
//! The roles in their order, highest first, as this account sees them: the
//! ones above its own highest are locked, with Discord's words for why. A
//! role moves by dragging its handle, or by Move Up and Move Down in its
//! menu, only among the roles under the locked ones and only while the
//! search hides none. The role editor is `settings::role_editor`'s (X09).

use iced::Color;

use crate::kind::Expected;
use crate::menu::Spot;

/// Discord's grey for a role without a colour (`DEFAULT_ROLE_COLOR`).
pub const DEFAULT_COLOR: Color = Color::from_rgb(
    0x99 as f32 / 255.0,
    0xaa as f32 / 255.0,
    0xb5 as f32 / 255.0,
);

/// A role as the list shows it (`RoleRow`).
#[derive(Clone, Debug, PartialEq)]
pub struct Role {
    /// Discord's id: what the row, its menu and its editor name.
    pub id: String,
    pub name: String,
    /// Its colour, or [`DEFAULT_COLOR`] for none: the dot.
    pub color: Color,
    /// Its icon's picture, which takes the dot's place once it's here.
    pub icon: Expected,
    /// Its emoji, in the dot's place while there is no picture.
    pub emoji: String,
    /// How many members hold it, as the list says it.
    pub members: String,
    /// Why this account can't change it, in Discord's words; empty when it
    /// may. A locked role shows a lock and opens to View, not Edit.
    pub lock: String,
    /// What gives it, such as "BOT", when people don't.
    pub badge: String,
    /// Duplicate Role and Delete show for it: More is live.
    pub may_copy: bool,
}

impl Default for Role {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            color: DEFAULT_COLOR,
            icon: Expected::None,
            emoji: String::new(),
            members: String::new(),
            lock: String::new(),
            badge: String::new(),
            may_copy: false,
        }
    }
}

impl Role {
    /// This account can't change it (`locked`).
    pub fn locked(&self) -> bool {
        !self.lock.is_empty()
    }
}

/// A server's role list: what its screen draws. The window holds it as
/// `vm.settings.roles`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Roles {
    /// @everyone's id, which Default Permissions opens, and its name.
    pub everyone: String,
    pub everyone_name: String,
    /// The roles the search leaves, highest first.
    pub rows: Vec<Role>,
    /// What the search field holds.
    pub search: String,
    /// Dragging works: the search hides no role.
    pub may_drag: bool,
    /// The highest place a role can move to: the first under the locked ones.
    pub first_movable: usize,
    /// Create Role is live.
    pub may_create: bool,
    /// Discord is taking a change: nothing moves and Create Role waits.
    pub saving: bool,
    /// The role being dragged, by id: its handle stays lit.
    pub dragging: Option<String>,
}

impl Roles {
    /// Whether the role at `index` may be picked up or moved: dragging
    /// works, nothing is being saved, and the role isn't locked.
    pub fn may_move(&self, index: usize) -> bool {
        self.may_drag && !self.saving && self.rows.get(index).is_some_and(|role| !role.locked())
    }

    /// Whether the role at `from` may go to `to`'s place: among the roles
    /// under the locked ones.
    pub fn may_drop(&self, from: usize, to: usize) -> bool {
        self.may_move(from) && to >= self.first_movable && to < self.rows.len()
    }

    /// Move Up is live for the role at `index`.
    pub fn may_move_up(&self, index: usize) -> bool {
        self.may_move(index) && index > self.first_movable
    }

    /// Move Down is live for the role at `index`.
    pub fn may_move_down(&self, index: usize) -> bool {
        self.may_move(index) && index + 1 < self.rows.len()
    }

    /// The place of the role `id` in the list.
    pub fn position(&self, id: &str) -> Option<usize> {
        self.rows.iter().position(|role| role.id == id)
    }

    /// The row at `index`'s identity for the list: Discord's id, a number.
    pub fn key(&self, index: usize) -> u64 {
        self.rows
            .get(index)
            .and_then(|role| role.id.parse().ok())
            .unwrap_or(index as u64)
    }
}

/// A server's role list: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The search field changed (`write(0, text)`).
    Search(String),
    /// Open a role's editor by id: a row, its Edit or View, or Default
    /// Permissions with @everyone's (`open(id)`).
    Open(String),
    /// Create Role (`act(0)`).
    Create,
    /// Duplicate Role, by id (`act(1, id)`).
    Copy(String),
    /// Delete, by id: the app asks first (`act(2, id)`, X09's dialog).
    Delete(String),
    /// Move a role to a place in the list: dropped there, or Move Up and
    /// Move Down (`move(id, to)`).
    Move { id: String, to: usize },
    /// A role was picked up by its handle (`Some`, again as it passes over
    /// the rows) or put back where it was (`None`): the app keeps it in
    /// `dragging` until a [`Intent::Move`] or a `None`.
    Dragging(Option<String>),
    /// A role's More: the app opens its menu there (the screen's `menu`
    /// builds its rows).
    More(String, Spot),
}
