//! Roles: the list and the open role's editor from the core's page
//! (`roles_prompt`, `src/ui/server_settings.rs:518`). What only the window
//! keeps stays across a build: the role being dragged, the hex and emoji
//! fields being written (the hex field takes the colour again when the
//! colour changes, as the Slint field followed its binding).

use yapfast_core::application::{
    AppState, Application, DEFAULT_ROLE_COLOR, PermissionLock, PictureEdit, ROLE_COLORS,
    ROLE_PERMISSIONS, RoleConfirm, RoleLock, RoleRules, RoleTab, RolesPage, ServerSettings,
};
use yapfast_core::chat::{RoleManager, RoleMember};
use yapfast_lang::tr;

use anomi_vm::kind::Expected;
use anomi_vm::settings::Settings;
use anomi_vm::settings::role_editor::{
    Adding, Confirm, Member, Permission, RoleEditor, Swatch, Tab,
};
use anomi_vm::settings::roles::{Role, Roles};
use anomi_vm::shell::{Layer, Shell};

use super::{Shown, dialog, set};
use crate::project::rows;

/// A role's colour as the list shows it: Discord's grey for none.
pub(super) fn color(color: u32) -> iced::Color {
    rows::rgb(if color == 0 {
        DEFAULT_ROLE_COLOR
    } else {
        color
    })
}

/// A role's lock in Discord's words.
fn lock_text(lock: Option<RoleLock>) -> &'static str {
    match lock {
        Some(RoleLock::Highest) => tr!(
            "Role is locked because it is your highest ranked role. Please ask a higher rank or Server Owner for help."
        ),
        Some(RoleLock::Higher) => {
            tr!("Role is locked because it is a higher rank than your highest role.")
        }
        None => "",
    }
}

/// A member as Manage Members and Add Members show them.
pub(super) fn member(state: &AppState, member: &RoleMember, chosen: bool) -> Member {
    Member {
        id: member.id.clone(),
        name: member.name.clone(),
        username: member.username.clone().unwrap_or_default(),
        avatar: rows::avatar(state, &member.name, &member.id, Some(&member.avatar)),
        chosen,
    }
}

/// Roles, the list or the role open, and their dialogs.
pub(super) fn show(
    core: &Application,
    settings: &ServerSettings,
    page: &RolesPage,
    all: &mut Settings,
    shell: &mut Shell,
    shown: &mut Shown,
    focus: &mut Option<&'static str>,
) {
    let state = &core.state;
    let rules = core.role_rules(&settings.server);
    list(state, page, &rules, settings.saving, &mut all.roles);
    let editor = &mut all.role_editor;
    let name = |id: &str| {
        page.role(id)
            .map(|role| role.name.clone())
            .unwrap_or_default()
    };
    let confirm = match &page.confirm {
        Some(RoleConfirm::Delete(id)) => Some(Confirm::Delete { role: name(id) }),
        Some(RoleConfirm::RemoveMember(member)) => Some(Confirm::Remove {
            role: page
                .open_role()
                .map(|role| role.name.clone())
                .unwrap_or_default(),
            member: member.name.clone(),
        }),
        None => None,
    };
    dialog(shell, Layer::RoleConfirm, &mut editor.confirm, confirm);
    let adding = page.adding.as_ref().map(|adding| {
        let mut candidates: Vec<Member> = adding
            .found
            .iter()
            .map(|found| {
                let chosen = adding.chosen.iter().any(|known| known.id == found.id);
                member(state, found, chosen)
            })
            .collect();
        candidates.extend(
            adding
                .chosen
                .iter()
                .filter(|chosen| !adding.found.iter().any(|known| known.id == chosen.id))
                .map(|chosen| member(state, chosen, true)),
        );
        Adding {
            role: page
                .open_role()
                .map(|role| role.name.clone())
                .unwrap_or_default(),
            search: adding.query.clone(),
            candidates,
        }
    });
    if dialog(shell, Layer::RoleAdding, &mut editor.adding, adding) {
        *focus = Some(anomi_settings::server::roles::editor::ADD_SEARCH);
    }
    open(state, page, &rules, settings, editor, shown);
}

/// The list: every role in the draft's order, each one's lock.
fn list(state: &AppState, page: &RolesPage, rules: &RoleRules, saving: bool, vm: &mut Roles) {
    let everyone = page.everyone();
    let rows: Vec<Role> = page
        .listed()
        .into_iter()
        .map(|role| {
            let lock = rules.lock(&role.id, role.position);
            Role {
                id: role.id.clone(),
                name: role.name.clone(),
                color: color(role.color),
                icon: rows::expected(state, role.icon.as_deref()),
                emoji: role.unicode_emoji.clone().unwrap_or_default(),
                members: role.members.to_string(),
                lock: lock_text(lock).to_owned(),
                badge: match role.managed {
                    Some(RoleManager::Bot) => tr!("BOT"),
                    Some(RoleManager::Booster) => tr!("BOOSTER"),
                    Some(RoleManager::Integration) => tr!("INTEGRATION"),
                    None => "",
                }
                .to_owned(),
                may_copy: lock.is_none() && role.managed.is_none(),
            }
        })
        .collect();
    vm.first_movable = rows.iter().take_while(|row| row.locked()).count();
    if vm.rows != rows {
        vm.rows = rows;
    }
    set(
        &mut vm.everyone,
        everyone.map_or("", |role| role.id.as_str()),
    );
    set(
        &mut vm.everyone_name,
        everyone.map_or("@everyone", |role| role.name.as_str()),
    );
    set(&mut vm.search, &page.search);
    vm.may_drag = page.search.is_empty();
    vm.may_create = true;
    vm.saving = saving;
}

/// The role open: Display, Permissions and Manage Members.
fn open(
    state: &AppState,
    page: &RolesPage,
    rules: &RoleRules,
    settings: &ServerSettings,
    vm: &mut RoleEditor,
    shown: &mut Shown,
) {
    let (Some(draft), Some(role)) = (&page.open, page.open_role()) else {
        shown.role = None;
        return;
    };
    let lock = rules.lock(&role.id, role.position);
    let everyone = page.is_everyone(&role.id);
    let colour = if draft.color == 0 {
        DEFAULT_ROLE_COLOR
    } else {
        draft.color
    };
    // A role opened anew, or a colour chosen, writes the hex field again.
    if shown.role.as_ref() != Some(&(role.id.clone(), colour)) {
        if shown.role.as_ref().is_none_or(|(id, _)| *id != role.id) {
            vm.emoji_text.clear();
        }
        shown.role = Some((role.id.clone(), colour));
        vm.hex = format!("#{colour:06x}");
    }
    set(&mut vm.id, &role.id);
    vm.everyone = everyone;
    vm.tab = match draft.tab {
        RoleTab::Display => Tab::Display,
        RoleTab::Permissions => Tab::Permissions,
        RoleTab::Members => Tab::Members,
    };
    vm.locked = lock.is_some();
    let notice = match (lock, role.managed) {
        (Some(_), _) => lock_text(lock),
        (None, Some(RoleManager::Booster)) => tr!(
            "This role is automatically managed by Discord for Server Boosting. It cannot be manually assigned to members or deleted."
        ),
        (None, Some(_)) => tr!("This role is automatically managed by an integration."),
        (None, None) => "",
    };
    set(&mut vm.notice, notice);
    vm.saving = settings.saving;
    vm.reading = settings.reading.is_some();
    set(&mut vm.name, &draft.name);
    vm.default_color = draft.color == 0 || draft.color == DEFAULT_ROLE_COLOR;
    let swatch = |value: &u32| Swatch {
        color: rows::rgb(*value),
        chosen: draft.color == *value,
    };
    let bright: Vec<Swatch> = ROLE_COLORS[..10].iter().map(swatch).collect();
    let dark: Vec<Swatch> = ROLE_COLORS[10..].iter().map(swatch).collect();
    if vm.bright != bright {
        vm.bright = bright;
    }
    if vm.dark != dark {
        vm.dark = dark;
    }
    vm.color = rows::rgb(colour);
    vm.custom_color = !vm.default_color && !ROLE_COLORS.contains(&draft.color);
    vm.hoist = draft.hoist;
    vm.mentionable = draft.mentionable;
    let (icon, new) = match &draft.icon {
        PictureEdit::Keep => (role.icon.as_deref(), None),
        PictureEdit::Reset => (None, None),
        PictureEdit::New(picked) => (None, Some(&picked.preview)),
    };
    let picture = match new {
        Some(preview) => crate::pictures::shown(preview).map_or(Expected::Waiting, Expected::Ready),
        None => rows::expected(state, icon),
    };
    if vm.icon != picture {
        vm.icon = picture;
    }
    set(&mut vm.emoji, draft.emoji.as_deref().unwrap_or_default());
    vm.has_icon = icon.is_some() || new.is_some() || draft.emoji.is_some();
    set(
        &mut vm.icon_locked,
        &if page.original.role_icons {
            String::new()
        } else {
            tr!("Unlocks at Level {}", 2)
        },
    );
    set(&mut vm.permission_search, &page.permission_search);
    let permissions = permissions(page, rules, &role.id, draft.permissions);
    if vm.permissions != permissions {
        vm.permissions = permissions;
    }
    let clearable = (0..64)
        .map(|bit| 1u64 << bit)
        .filter(|bit| draft.permissions & bit != 0)
        .all(|bit| rules.permission_lock(&role.id, bit, true).is_none());
    vm.may_clear = draft.permissions != 0 && clearable;
    set(
        &mut vm.clear_lock,
        if clearable {
            ""
        } else {
            tr!(
                "You cannot clear permissions because it would remove one or more permissions from you."
            )
        },
    );
    set(
        &mut vm.members_title,
        &tr!("Manage Members ({})", role.members),
    );
    vm.members_loading = draft.members.is_none() && draft.tab == RoleTab::Members;
    set(
        &mut vm.members_error,
        match &draft.members {
            Some(Err(error)) => error,
            _ => "",
        },
    );
    set(&mut vm.member_search, &draft.member_search);
    vm.not_all = draft.member_search.is_empty()
        && matches!(&draft.members, Some(Ok(members)) if (members.len() as u64) < role.members);
    let members: Vec<Member> = page
        .shown_members()
        .into_iter()
        .map(|shown| member(state, shown, false))
        .collect();
    if vm.members != members {
        vm.members = members;
    }
    vm.may_add = lock.is_none() && !everyone && role.managed.is_none();
}

/// The permissions the search finds, under their groups' headings, each
/// one's lock in Discord's words.
fn permissions(page: &RolesPage, rules: &RoleRules, role: &str, held: u64) -> Vec<Permission> {
    let search = page.permission_search.to_lowercase();
    let found = |text: &'static str| yapfast_lang::text(text).to_lowercase().contains(&search);
    let mut permissions = vec![];
    for group in &ROLE_PERMISSIONS {
        let mut shown = group
            .permissions
            .iter()
            .filter(|info| page.original.community || !info.community)
            .filter(|info| search.is_empty() || found(info.title) || found(info.description))
            .peekable();
        if shown.peek().is_none() {
            continue;
        }
        permissions.push(Permission {
            heading: true,
            title: yapfast_lang::text(group.heading).to_uppercase(),
            ..Permission::default()
        });
        for info in shown {
            let bit = 1u64 << info.bit;
            let on = held & bit != 0;
            permissions.push(Permission {
                heading: false,
                title: yapfast_lang::text(info.title).to_owned(),
                description: yapfast_lang::text(info.description).to_owned(),
                warning: info.warning.map_or("", yapfast_lang::text).to_owned(),
                bit: info.bit,
                on,
                lock: match rules.permission_lock(role, bit, on) {
                    Some(PermissionLock::NotHeld) => tr!(
                        "You cannot modify this permission because none of your roles have it."
                    ),
                    Some(PermissionLock::WouldLose) => tr!(
                        "You cannot modify this permission because removing it would remove it from you."
                    ),
                    None => "",
                }
                .to_owned(),
            });
        }
    }
    permissions
}
