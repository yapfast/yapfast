//! Roles: a server's roles in their order, and one open in its editor, as
//! Discord's client shows them. The draft is the order and the open
//! role's fields; saving sends the order first, with `PATCH
//! /guilds/{id}/roles`, then the role, with `PATCH
//! /guilds/{id}/roles/{role}`, as Discord's does. Making, copying and
//! deleting a role, and giving it or taking it from members, happen at
//! once, as in Discord.

use super::*;
use crate::{
    application::profile_editor::PictureEdit,
    chat::{RoleChange, RoleDetail, RoleMember, ServerRoles},
};

/// The longest role name Discord takes.
pub const ROLE_NAME_LIMIT: usize = 100;
/// The most members Add Members gives a role at once.
pub const ADD_MEMBERS_LIMIT: usize = 30;
/// The largest role icon Discord takes, in bytes.
pub const ROLE_ICON_LIMIT: usize = 256 * 1024;
/// Discord's default role colour, which it saves as none.
pub const DEFAULT_ROLE_COLOR: u32 = 0x99aab5;
/// The colours Discord offers a role, a bright row then a dark one.
pub const ROLE_COLORS: [u32; 20] = [
    0x1abc9c, 0x2ecc71, 0x3498db, 0x9b59b6, 0xe91e63, 0xf1c40f, 0xe67e22, 0xe74c3c, 0x95a5a6,
    0x607d8b, 0x11806a, 0x1f8b4c, 0x206694, 0x71368a, 0xad1457, 0xc27c0e, 0xa84300, 0x992d22,
    0x979c9f, 0x546e7a,
];

/// A tab of the role editor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoleTab {
    #[default]
    Display,
    Permissions,
    Members,
}

/// Roles, with its draft.
#[derive(Clone, Debug)]
pub struct RolesPage {
    /// What Discord has.
    pub original: ServerRoles,
    /// Every role but @everyone, highest first, as the draft orders them.
    pub order: Vec<String>,
    /// The role open in the editor.
    pub open: Option<RoleDraft>,
    /// What the list and the Permissions tab are filtered by.
    pub search: String,
    pub permission_search: String,
    /// A question waiting for Yes or No.
    pub confirm: Option<RoleConfirm>,
    /// Add Members, while it's open.
    pub adding: Option<AddMembers>,
}

/// The role open in the editor, as its draft stands.
#[derive(Clone, Debug)]
pub struct RoleDraft {
    pub id: String,
    pub tab: RoleTab,
    pub name: String,
    pub color: u32,
    pub secondary_color: Option<u32>,
    pub tertiary_color: Option<u32>,
    pub hoist: bool,
    pub mentionable: bool,
    pub permissions: u64,
    pub icon: PictureEdit,
    /// The Unicode emoji that shows in an icon's place; a picture chosen,
    /// or none, takes its place, as Discord's client sends them.
    pub emoji: Option<String>,
    /// Its members, once Discord sends them, or why not.
    pub members: Option<Result<Vec<RoleMember>, String>>,
    pub member_search: String,
}

/// What Roles asks before doing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoleConfirm {
    /// Delete Role, by id.
    Delete(String),
    /// Remove member, from the open role.
    RemoveMember(RoleMember),
}

/// Add Members: what was searched, who it found, and who was picked.
#[derive(Clone, Debug, Default)]
pub struct AddMembers {
    pub query: String,
    pub found: Vec<RoleMember>,
    pub chosen: Vec<RoleMember>,
}

impl RoleDraft {
    pub(super) fn new(role: &RoleDetail, tab: RoleTab) -> Self {
        Self {
            id: role.id.clone(),
            tab,
            name: role.name.clone(),
            color: role.color,
            secondary_color: role.secondary_color,
            tertiary_color: role.tertiary_color,
            hoist: role.hoist,
            mentionable: role.mentionable,
            permissions: role.permissions,
            icon: PictureEdit::Keep,
            emoji: role.unicode_emoji.clone(),
            members: None,
            member_search: String::new(),
        }
    }

    /// The role as saving it would send it, or none when nothing changed.
    fn change(&self, role: &RoleDetail) -> Option<RoleChange> {
        let icon = match &self.icon {
            PictureEdit::Keep => None,
            PictureEdit::Reset => {
                (role.icon.is_some() || role.unicode_emoji.is_some()).then_some(None)
            }
            PictureEdit::New(picked) => Some(Some(picked.clone())),
        };
        let name = self.name.trim();
        let same = name == role.name
            && self.color == role.color
            && self.secondary_color == role.secondary_color
            && self.tertiary_color == role.tertiary_color
            && self.hoist == role.hoist
            && self.mentionable == role.mentionable
            && self.permissions == role.permissions
            && self.emoji == role.unicode_emoji
            && icon.is_none();
        (!same).then(|| RoleChange {
            id: self.id.clone(),
            name: name.to_owned(),
            permissions: self.permissions,
            color: self.color,
            secondary_color: self.secondary_color,
            tertiary_color: self.tertiary_color,
            hoist: self.hoist,
            mentionable: self.mentionable,
            unicode_emoji: self.emoji.clone(),
            icon,
        })
    }

    /// Takes what changed elsewhere in `now`, against `was`, where the
    /// draft kept what `was` had.
    fn follow(&mut self, was: &RoleDetail, now: &RoleDetail) {
        fn take<T: PartialEq + Clone>(draft: &mut T, was: &T, now: &T) {
            if draft == was {
                draft.clone_from(now);
            }
        }
        take(&mut self.name, &was.name, &now.name);
        take(&mut self.color, &was.color, &now.color);
        take(
            &mut self.secondary_color,
            &was.secondary_color,
            &now.secondary_color,
        );
        take(
            &mut self.tertiary_color,
            &was.tertiary_color,
            &now.tertiary_color,
        );
        take(&mut self.hoist, &was.hoist, &now.hoist);
        take(&mut self.mentionable, &was.mentionable, &now.mentionable);
        take(&mut self.permissions, &was.permissions, &now.permissions);
        // A picture chosen takes the emoji's place, whatever came since.
        if !matches!(self.icon, PictureEdit::New(_)) {
            take(&mut self.emoji, &was.unicode_emoji, &now.unicode_emoji);
        }
    }
}

impl RolesPage {
    pub(super) fn new(original: ServerRoles) -> Self {
        let order = order(&original.roles);
        let open = original
            .open
            .as_deref()
            .and_then(|id| original.roles.iter().find(|role| role.id == id))
            .map(|role| RoleDraft::new(role, RoleTab::Display));
        Self {
            original,
            order,
            open,
            search: String::new(),
            permission_search: String::new(),
            confirm: None,
            adding: None,
        }
    }

    /// The page as Discord now has it, keeping what `was` showed: its
    /// searches, and the role it had open, on its tab, unless Discord's
    /// answer opens another.
    pub(super) fn carry(mut self, was: RolesPage) -> Self {
        self.search = was.search;
        self.permission_search = was.permission_search;
        if self.open.is_none()
            && let Some(open) = was.open
            && let Some(role) = self.role(&open.id)
        {
            let mut draft = RoleDraft::new(role, open.tab);
            draft.members = open.members;
            draft.member_search = open.member_search;
            self.open = Some(draft);
        }
        self
    }

    /// A role by id.
    pub fn role(&self, id: &str) -> Option<&RoleDetail> {
        self.original.roles.iter().find(|role| role.id == id)
    }

    /// @everyone, the last role.
    pub fn everyone(&self) -> Option<&RoleDetail> {
        self.original.roles.last()
    }

    /// Whether `id` is @everyone.
    pub fn is_everyone(&self, id: &str) -> bool {
        self.everyone().is_some_and(|everyone| everyone.id == id)
    }

    /// The roles the list shows, in the draft's order, which the search
    /// filters by name, as Discord's does.
    pub fn listed(&self) -> Vec<&RoleDetail> {
        let search = self.search.to_lowercase();
        self.order
            .iter()
            .filter_map(|id| self.role(id))
            .filter(|role| search.is_empty() || role.name.to_lowercase().contains(&search))
            .collect()
    }

    /// Whether the open role's icon may change: the server's level gives
    /// roles icons, and this account may change the role, which isn't
    /// @everyone.
    pub fn icon_editable(&self, rules: &RoleRules) -> bool {
        self.original.role_icons
            && self.open_role().is_some_and(|role| {
                !self.is_everyone(&role.id) && rules.lock(&role.id, role.position).is_none()
            })
    }

    /// The open role's original.
    pub fn open_role(&self) -> Option<&RoleDetail> {
        self.role(&self.open.as_ref()?.id)
    }

    /// The roles that move, as `(id, position)`, once the draft reorders
    /// them: every role whose place in the order differs from its
    /// position, as Discord's client works it out, @everyone being 0.
    fn positions(&self) -> Vec<(String, i64)> {
        if self.order == order(&self.original.roles) {
            return vec![];
        }
        let count = self.order.len() as i64 + 1;
        self.order
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let position = count - 1 - index as i64;
                let role = self.role(id)?;
                (role.position != position).then(|| (id.clone(), position))
            })
            .collect()
    }

    /// What saving would change; none when nothing does.
    pub fn change(&self) -> Option<ServerSettingsChange> {
        let positions = self.positions();
        let role = self
            .open
            .as_ref()
            .and_then(|draft| draft.change(self.role(&draft.id)?))
            .map(Box::new);
        (!positions.is_empty() || role.is_some())
            .then_some(ServerSettingsChange::Roles { positions, role })
    }

    /// Whether the open role's draft differs from Discord's.
    pub fn role_changed(&self) -> bool {
        self.open.as_ref().is_some_and(|draft| {
            self.role(&draft.id)
                .is_some_and(|role| draft.change(role).is_some())
        })
    }

    /// Why the draft can't be saved as it is.
    pub fn problem(&self) -> Option<String> {
        let draft = self.open.as_ref()?;
        let name = draft.name.trim().chars().count();
        if name == 0 {
            Some(yapfast_lang::tr!("Give the role a name.").to_owned())
        } else if name > ROLE_NAME_LIMIT {
            Some(yapfast_lang::tr!(
                "Role names can be up to {} characters.",
                ROLE_NAME_LIMIT
            ))
        } else {
            None
        }
    }

    pub(super) fn undo(&mut self) {
        self.order = order(&self.original.roles);
        if let Some(draft) = self.open.take()
            && let Some(role) = self.role(&draft.id)
        {
            let mut fresh = RoleDraft::new(role, draft.tab);
            fresh.members = draft.members;
            fresh.member_search = draft.member_search;
            self.open = Some(fresh);
        }
    }

    /// Moves a role to `to` in the order, below every role in `locked`'s
    /// keeping, which can't move and nothing goes above.
    pub(super) fn move_role(&mut self, role: &str, to: usize, locked: impl Fn(&str) -> bool) {
        let Some(from) = self.order.iter().position(|id| id == role) else {
            return;
        };
        if locked(role) {
            return;
        }
        let first = self.order.iter().take_while(|id| locked(id)).count();
        let to = to.clamp(first, self.order.len() - 1);
        let id = self.order.remove(from);
        self.order.insert(to, id);
    }

    /// Takes a role made or changed elsewhere.
    pub(super) fn follow(&mut self, mut role: RoleDetail) {
        let at = self
            .original
            .roles
            .iter()
            .position(|known| known.id == role.id);
        if let Some(at) = at {
            role.members = self.original.roles[at].members;
        }
        let was = at.map(|at| self.original.roles[at].clone());
        let ordered = self.order == order(&self.original.roles);
        match at {
            Some(at) => self.original.roles[at] = role.clone(),
            None => self.original.roles.push(role.clone()),
        }
        sort(&mut self.original.roles);
        if ordered {
            self.order = order(&self.original.roles);
        } else if !self.order.contains(&role.id) && !self.is_everyone(&role.id) {
            // A new role goes where Discord put it, among the draft's.
            let at = self
                .order
                .iter()
                .position(|id| {
                    self.role(id)
                        .is_some_and(|known| known.position < role.position)
                })
                .unwrap_or(self.order.len());
            self.order.insert(at, role.id.clone());
        }
        if let (Some(draft), Some(was)) = (self.open.as_mut(), was)
            && draft.id == role.id
        {
            draft.follow(&was, &role);
        }
    }

    /// Drops a role deleted elsewhere, and closes it if it was open.
    pub(super) fn forget(&mut self, role: &str) {
        self.original.roles.retain(|known| known.id != role);
        self.order.retain(|id| id != role);
        if self.open.as_ref().is_some_and(|draft| draft.id == role) {
            self.open = None;
        }
        if matches!(&self.confirm, Some(RoleConfirm::Delete(id)) if id == role) {
            self.confirm = None;
        }
    }

    /// The name Discord gives a copy of a role: " copy" added until no
    /// role has it.
    pub fn copy_name(&self, name: &str) -> String {
        let mut copy = format!("{name} copy");
        while self.original.roles.iter().any(|role| role.name == copy) {
            copy.push_str(" copy");
        }
        copy
    }

    /// The open role's members the search finds, by name or username.
    pub fn shown_members(&self) -> Vec<&RoleMember> {
        let Some(RoleDraft {
            members: Some(Ok(members)),
            member_search,
            ..
        }) = &self.open
        else {
            return vec![];
        };
        let search = member_search.to_lowercase();
        members
            .iter()
            .filter(|member| {
                search.is_empty()
                    || member.name.to_lowercase().contains(&search)
                    || member
                        .username
                        .as_deref()
                        .is_some_and(|username| username.contains(&search))
            })
            .collect()
    }
}

/// Every role but @everyone, highest first.
fn order(roles: &[RoleDetail]) -> Vec<String> {
    let mut ids: Vec<String> = roles.iter().map(|role| role.id.clone()).collect();
    ids.pop();
    ids
}

/// Highest first, @everyone, at 0, last; ties by id, as Discord's client
/// breaks them.
fn sort(roles: &mut [RoleDetail]) {
    roles.sort_by(|a, b| b.position.cmp(&a.position).then_with(|| a.id.cmp(&b.id)));
}

/// What a role's locks are, as Discord's client works them out for this
/// account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleLock {
    /// It's this account's highest role.
    Highest,
    /// It's above this account's highest role.
    Higher,
}

/// Why a permission's switch can't change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionLock {
    /// None of this account's roles have it.
    NotHeld,
    /// Turning it off would take it from this account.
    WouldLose,
}

/// What this account may do on Roles, as of the snapshot.
#[derive(Clone, Debug, Default)]
pub struct RoleRules {
    pub owner: bool,
    /// Its highest role's position.
    pub top: i64,
    /// Its roles, and what they give it.
    pub roles: Vec<String>,
    pub held: u64,
    /// What it would hold without each of its roles, by id.
    pub without: Vec<(String, u64)>,
}

impl RoleRules {
    /// Why this account can't change `role`, at `position`, if it can't.
    pub fn lock(&self, role: &str, position: i64) -> Option<RoleLock> {
        if self.owner || self.top > position {
            return None;
        }
        Some(
            if self.roles.iter().any(|mine| mine == role) && self.top == position {
                RoleLock::Highest
            } else {
                RoleLock::Higher
            },
        )
    }

    /// Why this account can't flip `bit` on `role`, now `on`, if it can't.
    pub fn permission_lock(&self, role: &str, bit: u64, on: bool) -> Option<PermissionLock> {
        if self.owner {
            return None;
        }
        if self.held & bit == 0 {
            return Some(PermissionLock::NotHeld);
        }
        let lose = on
            && self
                .without
                .iter()
                .any(|(mine, held)| mine == role && held & bit == 0);
        lose.then_some(PermissionLock::WouldLose)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(id: &str, position: i64) -> RoleDetail {
        RoleDetail {
            id: id.into(),
            name: format!("Role {id}"),
            position,
            ..Default::default()
        }
    }

    fn page() -> RolesPage {
        RolesPage::new(ServerRoles {
            roles: vec![role("3", 3), role("2", 2), role("1", 1), role("0", 0)],
            ..Default::default()
        })
    }

    #[test]
    fn a_new_order_sends_the_roles_that_move_then_the_open_role() {
        let mut page = page();
        assert!(page.change().is_none());
        page.move_role("1", 0, |id| id == "3");
        assert_eq!(
            page.order,
            ["3", "1", "2"],
            "nothing goes above a locked role"
        );
        page.open = Some(RoleDraft::new(page.role("2").unwrap(), RoleTab::Display));
        page.open.as_mut().unwrap().permissions = 8;
        let Some(ServerSettingsChange::Roles { positions, role }) = page.change() else {
            panic!("a change");
        };
        assert_eq!(positions, [("1".to_owned(), 2), ("2".to_owned(), 1)]);
        let role = role.unwrap();
        assert_eq!((role.id.as_str(), role.permissions), ("2", 8));
        page.undo();
        assert!(page.change().is_none());
    }

    #[test]
    fn a_change_elsewhere_reaches_what_the_draft_kept() {
        let mut page = page();
        page.open = Some(RoleDraft::new(page.role("2").unwrap(), RoleTab::Display));
        page.open.as_mut().unwrap().name = "Mine".into();
        let mut changed = role("2", 2);
        changed.name = "Theirs".into();
        changed.hoist = true;
        page.follow(changed);
        let draft = page.open.as_ref().unwrap();
        assert_eq!((draft.name.as_str(), draft.hoist), ("Mine", true));
        page.follow(role("4", 4));
        assert_eq!(page.order, ["4", "3", "2", "1"]);
        page.forget("2");
        assert!(page.open.is_none());
        assert_eq!(page.order, ["4", "3", "1"]);
    }

    #[test]
    fn a_copy_is_named_until_no_role_has_its_name() {
        let mut page = page();
        page.original.roles[0].name = "Role 2 copy".into();
        assert_eq!(page.copy_name("Role 2"), "Role 2 copy copy");
    }

    #[test]
    fn locks_follow_this_accounts_highest_role() {
        let rules = RoleRules {
            top: 2,
            roles: vec!["2".into()],
            held: 1 << 28,
            without: vec![("2".into(), 0)],
            ..Default::default()
        };
        assert_eq!(rules.lock("3", 3), Some(RoleLock::Higher));
        assert_eq!(rules.lock("2", 2), Some(RoleLock::Highest));
        assert_eq!(rules.lock("1", 1), None);
        assert_eq!(
            rules.permission_lock("1", 1 << 3, false),
            Some(PermissionLock::NotHeld)
        );
        assert_eq!(
            rules.permission_lock("2", 1 << 28, true),
            Some(PermissionLock::WouldLose)
        );
        assert_eq!(rules.permission_lock("1", 1 << 28, true), None);
        let owner = RoleRules {
            owner: true,
            ..Default::default()
        };
        assert_eq!(owner.lock("3", 3), None);
    }
}
