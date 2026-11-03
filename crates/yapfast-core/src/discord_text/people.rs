//! Members and Invites in a Discord server's settings: searching its
//! members as Discord's client does, a chunk at a time, and listing,
//! revoking and pausing its invites. See "Members" and "Invites" in
//! docs/discord/server-settings.md.

use std::time::Duration;

use serde_json::{Map, Value, json};
use yapfast_discord::{
    api::MembersSearch,
    server_settings::{FoundMember, Invite, parse_members_search},
};

use super::{commands::NOT_DISCORD, *};
use crate::chat::{
    InvitesAction, JoinMethod, ListedMember, MembersQuery, RoleMember, ServerInvite,
};

/// How many times Members asks again while Discord indexes the server, as
/// Discord's client does.
const INDEXING_RETRIES: usize = 3;

/// Discord's member flags for a name, bio or server tag AutoMod
/// quarantined.
const QUARANTINED: u64 = 128 | 256 | 1024;

/// Unusual DMs count for two days after Discord last saw them.
const UNUSUAL_DMS_MS: u64 = 2 * 86_400_000;

/// Searches members or acts on invites, or gives back any other command.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let (server, work) = match command {
        Command::Discord(DiscordCommand::Members { server, query }) => (server, Ok(query)),
        Command::Discord(DiscordCommand::Invites { server, action }) => (server, Err(action)),
        other => return Some(other),
    };
    let (Ok(discord), Some(guild)) = (discord, discord::parse_server(&server)) else {
        let why = discord.err().unwrap_or(NOT_DISCORD).to_owned();
        let _ = events.send(match work {
            Ok(query) => Event::MembersFound {
                server,
                query,
                result: Err(why),
            },
            Err(action) => refused(server, action, why),
        });
        return None;
    };
    let (discord, guild, events) = (discord.clone(), guild.to_owned(), events.clone());
    tokio::spawn(async move {
        match work {
            Ok(query) => {
                let result = members(&discord, &guild, &server, &query, &events).await;
                let _ = events.send(Event::MembersFound {
                    server,
                    query,
                    result,
                });
            }
            Err(action) => {
                let event = invites(&discord, &guild, server, action).await;
                let _ = events.send(event);
            }
        }
    });
    None
}

/// What an invites action answers when it fails.
fn refused(server: String, action: InvitesAction, why: String) -> Event {
    match action {
        InvitesAction::Reload => Event::InvitesLoaded {
            server,
            result: Err(why),
        },
        InvitesAction::Revoke(code) => Event::InviteRevoked {
            server,
            code,
            result: Err(why),
        },
        InvitesAction::Pause { .. } => Event::ServerPaused {
            server,
            result: Err(why),
        },
    }
}

async fn invites(discord: &Discord, guild: &str, server: String, action: InvitesAction) -> Event {
    match action {
        InvitesAction::Reload => Event::InvitesLoaded {
            server,
            result: list(discord, guild).await,
        },
        InvitesAction::Revoke(code) => {
            let result = discord
                .stepping_up(|| discord.revoke_invite(&code))
                .await
                .map_err(|error| {
                    tracing::info!(error = format!("{error:#}"), "Discord kept an invite");
                    format!("Couldn't revoke the invite: {error}")
                });
            Event::InviteRevoked {
                server,
                code,
                result,
            }
        }
        InvitesAction::Pause {
            invites,
            dms,
            hours,
        } => {
            let result = discord
                .stepping_up(|| discord.pause_server(guild, invites, dms, hours))
                .await
                .map_err(|error| {
                    tracing::info!(error = format!("{error:#}"), "Discord didn't pause");
                    format!("Couldn't save the security actions: {error}")
                });
            Event::ServerPaused { server, result }
        }
    }
}

/// A server's invites, as Invites lists them.
pub(super) async fn list(discord: &Discord, guild: &str) -> Result<Vec<ServerInvite>, String> {
    discord
        .guild_invites(guild)
        .await
        .map(|invites| invites.into_iter().map(invite).collect())
        .map_err(|error| {
            tracing::warn!(error = format!("{error:#}"), "could not fetch invites");
            format!("Discord didn't send the invites: {error}")
        })
}

fn invite(invite: Invite) -> ServerInvite {
    ServerInvite {
        code: invite.code,
        // Invites name the inviter by username, as Discord's list does.
        inviter: invite.inviter.map(|user| RoleMember {
            id: user.id,
            name: user.username,
            username: None,
            avatar: user.avatar,
        }),
        channel: invite
            .channel
            .map(|(_, name)| format!("#{name}"))
            .unwrap_or_default(),
        uses: invite.uses,
        max_uses: invite.max_uses,
        expires: invite.expires,
        roles: invite.roles,
        bypass: invite.bypass,
    }
}

/// A chunk of members for `query`, asking again while Discord indexes the
/// server.
pub(super) async fn members(
    discord: &Discord,
    guild: &str,
    server: &str,
    query: &MembersQuery,
    events: &UnboundedSender<Event>,
) -> Result<(Vec<ListedMember>, Option<u64>), String> {
    let body = search_body(query);
    for attempt in 0..=INDEXING_RETRIES {
        let found = discord
            .members_search(guild, &body)
            .await
            .map_err(|error| {
                tracing::warn!(error = format!("{error:#}"), "could not search members");
                format!("Discord didn't send the members: {error}")
            })?;
        match found {
            MembersSearch::Found(found) => {
                let (members, total) = parse_members_search(&found);
                let now = crate::clock::now_ms();
                return Ok((
                    members
                        .into_iter()
                        .map(|member| listed(member, now))
                        .collect(),
                    total,
                ));
            }
            MembersSearch::Indexing(wait) if attempt < INDEXING_RETRIES => {
                let _ = events.send(Event::MembersIndexing {
                    server: server.to_owned(),
                    query: query.clone(),
                });
                tokio::time::sleep(Duration::from_secs_f64(wait.clamp(0.0, 60.0))).await;
            }
            MembersSearch::Indexing(_) => break,
        }
    }
    Err("Discord is still indexing the server's members. Try again in a while.".to_owned())
}

/// Discord's `members-search` body for `query`: names and ids typed,
/// split at commas, in its `and_query`, as Discord's client sends them.
pub(super) fn search_body(query: &MembersQuery) -> Value {
    let mut and = Map::new();
    let text = query.text.trim();
    // Discord's client ignores a single character.
    if text.chars().count() > 1 {
        let (ids, names): (Vec<&str>, Vec<&str>) = text
            .split(',')
            .map(str::trim)
            .filter(|term| !term.is_empty())
            .partition(|term| {
                (17..=19).contains(&term.len()) && term.bytes().all(|byte| byte.is_ascii_digit())
            });
        if !names.is_empty() {
            and.insert("usernames".into(), json!({ "or_query": names }));
        }
        if !ids.is_empty() {
            and.insert("user_id".into(), json!({ "or_query": ids }));
        }
    }
    let mut body = json!({"or_query": {}, "and_query": and, "limit": query.chunk()});
    if query.sort != 0 {
        body["sort"] = json!(query.sort);
    }
    if let Some((joined, user)) = &query.after {
        body["after"] = json!({"guild_joined_at": joined, "user_id": user});
    }
    body
}

/// A found member as Members lists them, with their name here.
fn listed(member: FoundMember, now: u64) -> ListedMember {
    let user = member.user;
    let name = member
        .nick
        .or(user.global_name)
        .unwrap_or_else(|| user.username.clone());
    let joined_by = match member.join_source {
        1 => JoinMethod::Bot,
        2 => JoinMethod::Integration(member.integration),
        3 => JoinMethod::Discovery,
        4 => JoinMethod::Hub,
        5 => member
            .invite_code
            .map_or(JoinMethod::Unknown, JoinMethod::Invite),
        6 => member
            .invite_code
            .map_or(JoinMethod::Unknown, JoinMethod::Vanity),
        7 => JoinMethod::Manual(member.invite_code),
        8 => JoinMethod::LinkedChannel,
        _ => JoinMethod::Unknown,
    };
    ListedMember {
        person: RoleMember {
            id: user.id,
            username: (name != user.username).then_some(user.username),
            name,
            avatar: user.avatar,
        },
        roles: member.roles,
        joined: member.joined_at,
        timed_out_until: member.timed_out_until,
        unusual_dms: member.unusual_dms_until + UNUSUAL_DMS_MS >= now
            && member.unusual_dms_until != 0,
        spammer: member.spammer,
        bot: member.bot,
        quarantined: member.flags & QUARANTINED != 0,
        joined_by,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_sends_names_and_ids_as_discords_client_does() {
        let query = |text: &str| MembersQuery {
            text: text.into(),
            ..Default::default()
        };
        assert_eq!(
            search_body(&query("")),
            json!({"or_query": {}, "and_query": {}, "limit": 250})
        );
        assert_eq!(
            search_body(&query(" a ")),
            search_body(&query("")),
            "one character"
        );
        let body = search_body(&MembersQuery {
            text: "mal, 1400000000000000901,".into(),
            sort: 2,
            after: Some((1_700_000_000_000, "5".into())),
            limit: 500,
        });
        assert_eq!(
            body,
            json!({
                "or_query": {},
                "and_query": {
                    "usernames": {"or_query": ["mal"]},
                    "user_id": {"or_query": ["1400000000000000901"]},
                },
                "limit": 500,
                "sort": 2,
                "after": {"guild_joined_at": 1_700_000_000_000u64, "user_id": "5"},
            })
        );
    }
}
