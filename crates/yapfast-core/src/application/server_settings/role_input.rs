//! What Roles does with what's done to it: the role list's search and
//! order, the editor's tabs and fields, and what Discord's client does at
//! once, making, copying and deleting roles and giving them to members.

use super::*;
use crate::chat::{RoleAction, RoleChange, RoleDetail, RoleMember};

/// Something done on Roles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoleInput {
    /// Filter the list by name.
    Search(String),
    /// Open a role in the editor, by id, or go back to the list.
    Open(Option<String>),
    Tab(RoleTab),
    Name(String),
    /// One of Discord's colours, or its default for none, or any other.
    Color(u32),
    /// A Unicode emoji for the role's icon, in a picture's place.
    Emoji(String),
    Hoist,
    Mentionable,
    /// Flip a permission, by its bit.
    Permission(u64),
    ClearPermissions,
    PermissionSearch(String),
    /// Move a role to a place in the list, 0 the top.
    Move {
        role: String,
        to: usize,
    },
    Create,
    Duplicate(String),
    /// Ask before deleting a role.
    Delete(String),
    /// Ask before taking the open role from a member.
    RemoveMember(String),
    /// Answer what Roles asked.
    Confirm,
    Cancel,
    MemberSearch(String),
    /// Open Add Members, search in it, pick or unpick someone, add those
    /// picked, or close it.
    AddMembers,
    FindCandidates(String),
    Pick(String),
    AddPicked,
    CloseAdding,
}

impl Application {
    /// What this account may do on a server's Roles.
    pub fn role_rules(&self, server: &str) -> RoleRules {
        #[cfg(feature = "discord")]
        if let Some((known, _, me)) = self.discord_place(server) {
            let roles = &known.marks.roles;
            let without = roles
                .iter()
                .map(|role| {
                    let rest: Vec<String> =
                        roles.iter().filter(|held| *held != role).cloned().collect();
                    (role.clone(), known.server_permissions(me, &rest))
                })
                .collect();
            return RoleRules {
                owner: known.owner_id.as_deref() == Some(me),
                top: known.top_position(roles),
                roles: roles.clone(),
                held: known.server_permissions(me, roles),
                without,
            };
        }
        let _ = server;
        RoleRules {
            // Nothing is this account's to change without Discord.
            top: i64::MIN,
            ..Default::default()
        }
    }

    pub(crate) fn role_input(&mut self, input: RoleInput) -> Vec<Effect> {
        let Some(settings) = self.state.server_settings.as_ref() else {
            return vec![];
        };
        let rules = self.role_rules(&settings.server);
        let Some(settings) = self.state.server_settings.as_mut() else {
            return vec![];
        };
        let server = settings.server.clone();
        let changed = settings.changed();
        let saving = settings.saving;
        let Some(Page::Roles(page)) = settings.page.as_mut() else {
            return vec![];
        };
        let action = |action: RoleAction| {
            vec![Effect::Backend(Command::Discord(
                DiscordCommand::RoleAction {
                    server: server.clone(),
                    action,
                },
            ))]
        };
        // A role this account may change, as the page has it.
        let unlocked = |page: &RolesPage, id: &str| {
            page.role(id)
                .is_some_and(|role| rules.lock(id, role.position).is_none())
        };
        // Neither managed nor @everyone: one people hold by hand.
        let assignable = |page: &RolesPage, id: &str| {
            unlocked(page, id)
                && !page.is_everyone(id)
                && page.role(id).is_some_and(|role| role.managed.is_none())
        };
        match input {
            RoleInput::Search(text) => page.search = text,
            RoleInput::PermissionSearch(text) => page.permission_search = text,
            RoleInput::MemberSearch(text) => {
                if let Some(draft) = &mut page.open {
                    draft.member_search = text;
                }
            }
            RoleInput::Open(id) => {
                if page.role_changed() {
                    settings.nudge = settings.nudge.wrapping_add(1);
                    return vec![];
                }
                page.adding = None;
                page.confirm = None;
                page.permission_search.clear();
                page.open = id.as_deref().and_then(|id| {
                    let role = page.role(id)?;
                    // @everyone has only permissions to show.
                    let tab = if page.is_everyone(id) {
                        RoleTab::Permissions
                    } else {
                        RoleTab::Display
                    };
                    Some(RoleDraft::new(role, tab))
                });
            }
            RoleInput::Tab(tab) => {
                let everyone = page
                    .open
                    .as_ref()
                    .is_some_and(|draft| page.is_everyone(&draft.id));
                let Some(draft) = &mut page.open else {
                    return vec![];
                };
                if everyone && tab != RoleTab::Permissions {
                    return vec![];
                }
                draft.tab = tab;
                if tab == RoleTab::Members && draft.members.is_none() {
                    return action(RoleAction::Members(draft.id.clone()));
                }
            }
            RoleInput::Name(_)
            | RoleInput::Color(_)
            | RoleInput::Hoist
            | RoleInput::Mentionable
            | RoleInput::Permission(_)
            | RoleInput::ClearPermissions => {
                let Some(id) = page.open.as_ref().map(|draft| draft.id.clone()) else {
                    return vec![];
                };
                if saving || !unlocked(page, &id) {
                    return vec![];
                }
                let everyone = page.is_everyone(&id);
                let Some(draft) = &mut page.open else {
                    return vec![];
                };
                match input {
                    RoleInput::Name(text) if !everyone => draft.name = text,
                    RoleInput::Color(color) if !everyone => {
                        // A colour of Discord's makes the role a solid one.
                        draft.color = if color == DEFAULT_ROLE_COLOR {
                            0
                        } else {
                            color
                        };
                        draft.secondary_color = None;
                        draft.tertiary_color = None;
                    }
                    RoleInput::Hoist if !everyone => draft.hoist = !draft.hoist,
                    RoleInput::Mentionable if !everyone => draft.mentionable = !draft.mentionable,
                    RoleInput::Permission(bit) => {
                        let on = draft.permissions & bit != 0;
                        if rules.permission_lock(&id, bit, on).is_none() {
                            draft.permissions ^= bit;
                        }
                    }
                    RoleInput::ClearPermissions => {
                        let all = draft.permissions;
                        let held = (0..64)
                            .map(|bit| 1u64 << bit)
                            .filter(|bit| all & bit != 0)
                            .all(|bit| rules.permission_lock(&id, bit, true).is_none());
                        if held {
                            draft.permissions = 0;
                        }
                    }
                    _ => {}
                }
            }
            RoleInput::Emoji(text) => {
                let emoji = text.trim();
                // Only an emoji: ASCII alone is no emoji, though a keycap
                // has a digit in it.
                if saving || !page.icon_editable(&rules) || emoji.is_ascii() {
                    return vec![];
                }
                let picture = page.open_role().is_some_and(|role| role.icon.is_some());
                let Some(draft) = &mut page.open else {
                    return vec![];
                };
                draft.emoji = Some(emoji.to_owned());
                draft.icon = if picture {
                    PictureEdit::Reset
                } else {
                    PictureEdit::Keep
                };
            }
            RoleInput::Move { role, to } => {
                if !saving {
                    let locked: Vec<String> = page
                        .order
                        .iter()
                        .filter(|id| !unlocked(page, id))
                        .cloned()
                        .collect();
                    page.move_role(&role, to, |id| locked.iter().any(|known| known == id));
                }
            }
            RoleInput::Create => {
                if changed || saving {
                    settings.nudge = settings.nudge.wrapping_add(1);
                    return vec![];
                }
                settings.saving = true;
                settings.error = None;
                return action(RoleAction::Create);
            }
            RoleInput::Duplicate(id) => {
                if changed || saving {
                    settings.nudge = settings.nudge.wrapping_add(1);
                    return vec![];
                }
                if !assignable(page, &id) {
                    return vec![];
                }
                let Some(role) = page.role(&id) else {
                    return vec![];
                };
                let name = page.copy_name(&role.name);
                let copy = RoleChange {
                    id: role.id.clone(),
                    name: name.clone(),
                    permissions: role.permissions,
                    color: role.color,
                    secondary_color: role.secondary_color,
                    tertiary_color: role.tertiary_color,
                    hoist: role.hoist,
                    mentionable: role.mentionable,
                    icon: None,
                    unicode_emoji: role.unicode_emoji.clone(),
                };
                settings.saving = true;
                settings.error = None;
                return action(RoleAction::Duplicate {
                    role: Box::new(copy),
                    name,
                });
            }
            RoleInput::Delete(id) => {
                if assignable(page, &id) {
                    page.confirm = Some(RoleConfirm::Delete(id));
                }
            }
            RoleInput::RemoveMember(member) => {
                let Some(id) = page.open.as_ref().map(|draft| draft.id.clone()) else {
                    return vec![];
                };
                let found = page
                    .shown_members()
                    .into_iter()
                    .find(|known| known.id == member)
                    .cloned();
                if let Some(member) = found
                    && assignable(page, &id)
                {
                    page.confirm = Some(RoleConfirm::RemoveMember(member));
                }
            }
            RoleInput::Cancel => page.confirm = None,
            RoleInput::Confirm => match page.confirm.take() {
                Some(RoleConfirm::Delete(id)) => {
                    if changed || saving {
                        page.confirm = Some(RoleConfirm::Delete(id));
                        settings.nudge = settings.nudge.wrapping_add(1);
                        return vec![];
                    }
                    settings.saving = true;
                    settings.error = None;
                    return action(RoleAction::Delete(id));
                }
                Some(RoleConfirm::RemoveMember(member)) => {
                    let Some(draft) = &mut page.open else {
                        return vec![];
                    };
                    draft.members = None;
                    return action(RoleAction::RemoveMember {
                        role: draft.id.clone(),
                        member: member.id,
                    });
                }
                None => {}
            },
            RoleInput::AddMembers => {
                let Some(id) = page.open.as_ref().map(|draft| draft.id.clone()) else {
                    return vec![];
                };
                if assignable(page, &id) {
                    page.adding = Some(AddMembers::default());
                    return action(RoleAction::FindMembers(String::new()));
                }
            }
            RoleInput::FindCandidates(query) => {
                if let Some(adding) = &mut page.adding {
                    adding.query.clone_from(&query);
                    return action(RoleAction::FindMembers(query));
                }
            }
            RoleInput::Pick(member) => {
                let Some(adding) = &mut page.adding else {
                    return vec![];
                };
                if let Some(at) = adding.chosen.iter().position(|known| known.id == member) {
                    adding.chosen.remove(at);
                } else if adding.chosen.len() < ADD_MEMBERS_LIMIT
                    && let Some(found) = adding.found.iter().find(|known| known.id == member)
                {
                    adding.chosen.push(found.clone());
                }
            }
            RoleInput::AddPicked => {
                let Some(adding) = page.adding.take() else {
                    return vec![];
                };
                let Some(draft) = &mut page.open else {
                    return vec![];
                };
                if adding.chosen.is_empty() {
                    return vec![];
                }
                draft.members = None;
                return action(RoleAction::AddMembers {
                    role: draft.id.clone(),
                    members: adding.chosen.into_iter().map(|member| member.id).collect(),
                });
            }
            RoleInput::CloseAdding => page.adding = None,
        }
        vec![]
    }

    /// The members holding a role, as Discord sent them.
    pub(crate) fn role_holders(
        &mut self,
        server: String,
        role: String,
        result: Result<Vec<RoleMember>, String>,
    ) {
        let Some(Page::Roles(page)) = self
            .state
            .server_settings
            .as_mut()
            .filter(|settings| settings.server == server)
            .and_then(|settings| settings.page.as_mut())
        else {
            return;
        };
        if let Some(draft) = page.open.as_mut().filter(|draft| draft.id == role) {
            draft.members = Some(result);
        }
    }

    pub(crate) fn role_count(&mut self, server: &str, role: &str, count: u64) {
        if let Some(Page::Roles(page)) = self
            .state
            .server_settings
            .as_mut()
            .filter(|settings| settings.server == server)
            .and_then(|settings| settings.page.as_mut())
            && let Some(known) = page
                .original
                .roles
                .iter_mut()
                .find(|known| known.id == role)
        {
            known.members = count;
        }
    }

    /// Members Add Members found for its search, without those who hold
    /// the role already.
    pub(crate) fn role_candidates(
        &mut self,
        server: String,
        query: String,
        found: Vec<RoleMember>,
    ) {
        let Some(Page::Roles(page)) = self
            .state
            .server_settings
            .as_mut()
            .filter(|settings| settings.server == server)
            .and_then(|settings| settings.page.as_mut())
        else {
            return;
        };
        let holding: Vec<String> = match &page.open {
            Some(RoleDraft {
                members: Some(Ok(members)),
                ..
            }) => members.iter().map(|member| member.id.clone()).collect(),
            _ => vec![],
        };
        if let Some(adding) = page.adding.as_mut().filter(|adding| adding.query == query) {
            adding.found = found
                .into_iter()
                .filter(|member| !holding.contains(&member.id))
                .collect();
        }
    }

    /// A role made, changed or deleted elsewhere, or by this account,
    /// reaching an open Roles, even while Discord takes a change: one that
    /// fails leaves the page as Discord now has it.
    pub(crate) fn server_role_changed(
        &mut self,
        server: String,
        role: Option<RoleDetail>,
        gone: Option<String>,
    ) {
        let Some(Page::Roles(page)) = self
            .state
            .server_settings
            .as_mut()
            .filter(|settings| settings.server == server)
            .and_then(|settings| settings.page.as_mut())
        else {
            return;
        };
        if let Some(role) = role {
            page.follow(role);
        }
        if let Some(gone) = gone {
            page.forget(&gone);
        }
    }
}
