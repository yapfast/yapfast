//! Roles in a Discord server's settings: the page as it opens, saving its
//! draft, and what Discord's client does at once, as its requests go. See
//! "Roles" in docs/discord/server-settings.md.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use yapfast_discord::server_settings::{Managed, Role as DiscordRole, Roles as DiscordRoles};

use super::{channels::SEARCH_PAUSE, commands::NOT_DISCORD, own_profile::picture, *};
use crate::chat::{
    RoleAction, RoleChange, RoleDetail, RoleManager, RoleMember, ServerRoles, ServerSettingsPage,
};

/// Carries out a [`DiscordCommand::RoleAction`], or gives back any other
/// command.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    searches: &Arc<AtomicU64>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let Command::Discord(DiscordCommand::RoleAction { server, action }) = command else {
        return Some(command);
    };
    let events = events.clone();
    let (Ok(discord), Some(guild)) = (discord, discord::parse_server(&server)) else {
        let why = discord.err().unwrap_or(NOT_DISCORD).to_owned();
        let _ = match action {
            RoleAction::Members(role) => events.send(Event::RoleHolders {
                server,
                role,
                result: Err(why),
                count: None,
            }),
            RoleAction::FindMembers(_) => Ok(()),
            _ => events.send(Event::ServerSettingsSaved {
                server,
                result: Err(why),
            }),
        };
        return None;
    };
    let (discord, guild) = (discord.clone(), guild.to_owned());
    match action {
        RoleAction::Members(role) => {
            tokio::spawn(async move {
                let result = holders(&discord, &guild, &role).await;
                let _ = events.send(Event::RoleHolders {
                    server,
                    role,
                    result,
                    count: None,
                });
            });
        }
        RoleAction::FindMembers(query) => {
            let searches = searches.clone();
            let search = searches.fetch_add(1, Ordering::Relaxed) + 1;
            tokio::spawn(async move {
                // Typing on asks for a longer name, which covers this one.
                tokio::time::sleep(SEARCH_PAUSE).await;
                if searches.load(Ordering::Relaxed) != search {
                    return;
                }
                let found = match discord.search_members(&guild, &query).await {
                    Ok(found) => found,
                    Err(error) => {
                        tracing::debug!(%error, "could not search Discord members");
                        return;
                    }
                };
                let members = found
                    .into_iter()
                    .map(|user| member(&discord, &guild, user))
                    .collect();
                let _ = events.send(Event::RoleCandidates {
                    server,
                    query,
                    members,
                });
            });
        }
        RoleAction::AddMembers { role, members } => {
            tokio::spawn(async move {
                let added = discord
                    .stepping_up(|| discord.add_role_members(&guild, &role, &members))
                    .await;
                changed_holders(&discord, &guild, server, role, added, &events).await;
            });
        }
        RoleAction::RemoveMember { role, member } => {
            tokio::spawn(async move {
                let removed = discord
                    .stepping_up(|| discord.remove_role_member(&guild, &role, &member))
                    .await;
                changed_holders(&discord, &guild, server, role, removed, &events).await;
            });
        }
        action => {
            tokio::spawn(async move {
                let result = act(&discord, &guild, action).await.map_err(|error| {
                    tracing::warn!(error = format!("{error:#}"), "could not change roles");
                    format!("That didn't work: {error}")
                });
                let _ = events.send(Event::ServerSettingsSaved { server, result });
            });
        }
    }
    None
}

/// Makes, copies or deletes a role, as Discord's client does, then gives
/// the page as it stands, opening the role made.
async fn act(
    discord: &Discord,
    guild: &str,
    action: RoleAction,
) -> anyhow::Result<ServerSettingsPage> {
    let open = match action {
        RoleAction::Create => {
            let body = json!({
                "name": "new role",
                "color": 0,
                "colors": {"primary_color": 0, "secondary_color": null, "tertiary_color": null},
                "permissions": "0",
            });
            Some(
                discord
                    .stepping_up(|| discord.create_role(guild, &body))
                    .await?,
            )
        }
        RoleAction::Duplicate { role, name } => {
            let made = json!({
                "name": name,
                "color": role.color,
                "colors": colors(&role),
                "permissions": "0",
            });
            let copy = discord
                .stepping_up(|| discord.create_role(guild, &made))
                .await?;
            let body = role_body(&RoleChange {
                name,
                ..(*role).clone()
            });
            discord
                .stepping_up(|| discord.save_roles(guild, &[], Some((&copy, &body))))
                .await?;
            // Just below the role it copies, as Discord's client puts it;
            // only the places that change go, so roles above this
            // account's stay out of it.
            let now = discord.roles_page(guild).await?.roles;
            let mut order: Vec<&str> = now
                .iter()
                .map(|known| known.id.as_str())
                .filter(|id| *id != copy && *id != guild)
                .collect();
            let below = order
                .iter()
                .position(|id| *id == role.id)
                .map_or(order.len(), |at| at + 1);
            order.insert(below, &copy);
            let count = order.len() as i64 + 1;
            let positions: Vec<(String, i64)> = order
                .into_iter()
                .enumerate()
                .map(|(index, id)| (id.to_owned(), count - 1 - index as i64))
                .filter(|(id, position)| {
                    now.iter()
                        .any(|known| known.id == *id && known.position != *position)
                })
                .collect();
            if !positions.is_empty() {
                discord
                    .stepping_up(|| discord.save_roles(guild, &positions, None))
                    .await?;
            }
            Some(copy)
        }
        RoleAction::Delete(role) => {
            discord
                .stepping_up(|| discord.delete_role(guild, &role))
                .await?;
            None
        }
        RoleAction::Members(_)
        | RoleAction::FindMembers(_)
        | RoleAction::AddMembers { .. }
        | RoleAction::RemoveMember { .. } => None,
    };
    let mut page = page(discord.roles_page(guild).await?);
    page.open = open;
    Ok(ServerSettingsPage::Roles(page))
}

/// Saves Roles' draft: the order, then the open role.
pub(super) async fn save(
    discord: &Discord,
    guild: &str,
    positions: Vec<(String, i64)>,
    role: Option<Box<RoleChange>>,
) -> anyhow::Result<ServerSettingsPage> {
    let body = role
        .as_deref()
        .map(|role| (role.id.clone(), role_body(role)));
    drop(role);
    discord
        .stepping_up(|| {
            let role = body.as_ref().map(|(id, body)| (id.as_str(), body));
            discord.save_roles(guild, &positions, role)
        })
        .await?;
    Ok(ServerSettingsPage::Roles(page(
        discord.roles_page(guild).await?,
    )))
}

/// Roles as Discord sent them.
pub(super) fn page(roles: DiscordRoles) -> ServerRoles {
    let has = |feature: &str| roles.features.iter().any(|known| known == feature);
    ServerRoles {
        role_icons: roles.premium_tier >= 2 || has("ROLE_ICONS"),
        community: has("COMMUNITY"),
        open: None,
        roles: roles
            .roles
            .into_iter()
            .map(|role| {
                let members = roles.counts.get(&role.id).copied().unwrap_or_default();
                detail(role, members)
            })
            .collect(),
    }
}

/// A role as Roles shows it, held by `members`.
pub(super) fn detail(role: DiscordRole, members: u64) -> RoleDetail {
    RoleDetail {
        id: role.id,
        name: role.name,
        permissions: role.permissions,
        position: role.position,
        color: role.color,
        secondary_color: role.secondary_color,
        tertiary_color: role.tertiary_color,
        hoist: role.hoist,
        mentionable: role.mentionable,
        managed: role.managed.map(|managed| match managed {
            Managed::Bot => RoleManager::Bot,
            Managed::Booster => RoleManager::Booster,
            Managed::Integration => RoleManager::Integration,
        }),
        icon: role.icon,
        unicode_emoji: role.unicode_emoji,
        members,
    }
}

/// A role's colours as Discord's client sends them.
fn colors(role: &RoleChange) -> Value {
    json!({
        "primary_color": role.color,
        "secondary_color": role.secondary_color,
        "tertiary_color": role.tertiary_color,
    })
}

/// What Discord's client sends to save a role, for `PATCH
/// /guilds/{id}/roles/{role}`: every field, permissions as a decimal
/// string, and the icon only when it changes.
fn role_body(role: &RoleChange) -> Value {
    let mut body = json!({
        "name": role.name,
        "permissions": role.permissions.to_string(),
        "color": role.color,
        "colors": colors(role),
        "hoist": role.hoist,
        "mentionable": role.mentionable,
        "unicode_emoji": role.unicode_emoji,
    });
    if let Some(icon) = &role.icon {
        body["icon"] = picture(icon);
    }
    body
}

/// A member as Manage Members and Add Members list them.
fn member(discord: &Discord, guild: &str, user: String) -> RoleMember {
    RoleMember {
        name: discord.name(guild, &user).unwrap_or_else(|| user.clone()),
        username: discord.username(&user),
        avatar: discord.avatar(Some(guild), &user),
        id: user,
    }
}

/// The members holding a role, by name.
async fn holders(discord: &Discord, guild: &str, role: &str) -> Result<Vec<RoleMember>, String> {
    let ids = discord.role_members(guild, role).await.map_err(|error| {
        tracing::warn!(
            error = format!("{error:#}"),
            "could not list a role's members"
        );
        format!("Discord didn't send the role's members: {error}")
    })?;
    let mut members: Vec<RoleMember> = ids
        .into_iter()
        .map(|user| member(discord, guild, user))
        .collect();
    members.sort_by_key(|member| member.name.to_lowercase());
    Ok(members)
}

/// Sends a role's members after giving it to some or taking it from one,
/// and how many hold it now; or why the change didn't go.
async fn changed_holders(
    discord: &Discord,
    guild: &str,
    server: String,
    role: String,
    changed: anyhow::Result<()>,
    events: &UnboundedSender<Event>,
) {
    let (result, count) = match changed {
        Ok(()) => {
            let count = discord
                .role_member_counts(guild)
                .await
                .ok()
                .map(|counts| counts.get(&role).copied().unwrap_or_default());
            (holders(discord, guild, &role).await, count)
        }
        Err(error) => {
            tracing::warn!(
                error = format!("{error:#}"),
                "could not change a role's members"
            );
            (Err(format!("That didn't work: {error}")), None)
        }
    };
    let _ = events.send(Event::RoleHolders {
        server,
        role,
        result,
        count,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_role_goes_out_whole_with_its_permissions_as_a_decimal_string() {
        let role = RoleChange {
            id: "2".into(),
            name: "Mods".into(),
            permissions: 1 << 40,
            color: 0x3498db,
            secondary_color: Some(1),
            tertiary_color: None,
            hoist: true,
            mentionable: false,
            icon: None,
            unicode_emoji: None,
        };
        assert_eq!(
            role_body(&role),
            json!({
                "name": "Mods", "permissions": "1099511627776", "color": 0x3498db,
                "colors": {"primary_color": 0x3498db, "secondary_color": 1, "tertiary_color": null},
                "hoist": true, "mentionable": false, "unicode_emoji": null,
            })
        );
        let removed = RoleChange {
            icon: Some(None),
            ..role
        };
        assert_eq!(role_body(&removed)["icon"], Value::Null);
        assert!(role_body(&removed).get("icon").is_some());
    }
}
