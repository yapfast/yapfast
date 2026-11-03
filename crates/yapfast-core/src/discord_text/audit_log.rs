//! A Discord server's audit log, as Audit Log in its settings fetches it:
//! `GET /guilds/{id}/audit-logs`, a page of entries and the names they
//! refer to, read into [`AuditLog`]. See "Audit Log" in docs/discord/server-settings.md.

use serde_json::Value;
use yapfast_discord::ready;

use super::{commands::NOT_DISCORD, *};
use crate::chat::{AuditChange, AuditEntry, AuditLog, AuditQuery, AuditUser};

/// Fetches a page of the audit log, or gives back any other command.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let Command::Discord(DiscordCommand::AuditLog { server, query }) = command else {
        return Some(command);
    };
    let (Ok(discord), Some(guild)) = (discord, discord::parse_server(&server)) else {
        let why = discord.err().unwrap_or(NOT_DISCORD);
        let _ = events.send(Event::AuditLogLoaded {
            server,
            query,
            result: Err(why.into()),
        });
        return None;
    };
    let (discord, guild, events) = (discord.clone(), guild.to_owned(), events.clone());
    tokio::spawn(async move {
        let result = page(&discord, &guild, &query).await;
        let _ = events.send(Event::AuditLogLoaded {
            server,
            query,
            result,
        });
    });
    None
}

/// A page of the audit log, as `query` asks.
pub(super) async fn page(
    discord: &Discord,
    guild: &str,
    query: &AuditQuery,
) -> Result<AuditLog, String> {
    discord
        .audit_log(
            guild,
            query.before.as_deref(),
            query.user.as_deref(),
            query.action,
        )
        .await
        .map(|log| parse(&log))
        .map_err(|error| {
            tracing::warn!(
                error = format!("{error:#}"),
                "could not fetch the audit log"
            );
            format!("Discord didn't send the audit log: {error}")
        })
}

/// Discord's answer, with only what Audit Log reads.
pub(super) fn parse(log: &Value) -> AuditLog {
    let list = |key: &str| log[key].as_array().into_iter().flatten();
    let entries = list("audit_log_entries")
        .filter_map(|entry| {
            Some(AuditEntry {
                id: entry["id"].as_str()?.to_owned(),
                action: entry["action_type"]
                    .as_u64()
                    .and_then(|action| u16::try_from(action).ok())?,
                target: text(&entry["target_id"]),
                user: text(&entry["user_id"]),
                reason: text(&entry["reason"]).filter(|reason| !reason.is_empty()),
                changes: entry["changes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|change| {
                        Some(AuditChange {
                            key: change["key"].as_str()?.to_owned(),
                            old: change["old_value"].clone(),
                            new: change["new_value"].clone(),
                        })
                    })
                    .collect(),
                options: entry["options"].clone(),
            })
        })
        .collect();
    let users = list("users")
        .filter_map(|user| {
            let id = user["id"].as_str()?;
            let username = user["username"].as_str().unwrap_or_default();
            // A legacy username keeps its tag, as Discord's log shows it.
            let name = match user["discriminator"].as_str() {
                Some(tag) if tag != "0" && !tag.is_empty() => format!("{username}#{tag}"),
                _ => username.to_owned(),
            };
            let avatar = ready::avatar(id, user["avatar"].as_str());
            Some((id.to_owned(), AuditUser { name, avatar }))
        })
        .collect();
    let names = [
        "webhooks",
        "integrations",
        "threads",
        "guild_scheduled_events",
        "auto_moderation_rules",
        "application_commands",
    ]
    .into_iter()
    .flat_map(list)
    .filter_map(|named| Some((text(&named["id"])?, text(&named["name"])?)))
    .collect();
    AuditLog {
        entries,
        users,
        names,
    }
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_entries_and_the_names_they_refer_to() {
        let log = parse(&json!({
            "audit_log_entries": [{
                "id": "900", "action_type": 20, "target_id": "4", "user_id": "1",
                "reason": "spam", "changes": [{"key": "nick", "old_value": "a"}],
                "options": {},
            }, {"id": "899", "action_type": 1, "target_id": null, "user_id": "2", "reason": ""}],
            "users": [{"id": "1", "username": "alice", "discriminator": "0", "avatar": null},
                      {"id": "2", "username": "old", "discriminator": "1234"}],
            "webhooks": [{"id": "7", "name": "Captain Hook"}],
        }));
        assert_eq!(log.entries.len(), 2);
        let kick = &log.entries[0];
        assert_eq!((kick.action, kick.reason.as_deref()), (20, Some("spam")));
        assert_eq!(kick.changes[0].new, Value::Null);
        assert_eq!(log.entries[1].reason, None);
        assert_eq!(log.users["2"].name, "old#1234");
        assert_eq!(log.names["7"], "Captain Hook");
    }
}
