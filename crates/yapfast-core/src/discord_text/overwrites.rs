//! A channel's overwrites, as its settings' Permissions page changes them,
//! and the names of the members they're for. See "Channel settings" in
//! docs/discord/server-settings.md.

use yapfast_discord::ready::Overwrite;

use super::{commands::NOT_DISCORD, *};
use crate::chat::{ChannelOverwrite, OverwriteChange, RoleMember};

/// Carries out [`DiscordCommand::ChangeOverwrites`] and
/// [`DiscordCommand::OverwriteNames`], or gives back any other command.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let Command::Discord(
        command @ (DiscordCommand::ChangeOverwrites { .. } | DiscordCommand::OverwriteNames { .. }),
    ) = command
    else {
        return Some(command);
    };
    let events = events.clone();
    match command {
        DiscordCommand::ChangeOverwrites { room, change } => {
            let target = discord::parse_room(&room).map(|(_, channel)| channel.to_owned());
            let (Ok(discord), Some(channel)) = (discord, target) else {
                let why = discord.err().unwrap_or(NOT_DISCORD).to_owned();
                let _ = events.send(Event::OverwritesChanged {
                    room,
                    result: Err(why),
                });
                return None;
            };
            let discord = discord.clone();
            tokio::spawn(async move {
                let result = change_overwrites(&discord, &channel, change)
                    .await
                    .map_err(|error| {
                        tracing::warn!(error = format!("{error:#}"), "could not change overwrites");
                        format!("That didn't work: {error}")
                    });
                let _ = events.send(Event::OverwritesChanged { room, result });
            });
        }
        DiscordCommand::OverwriteNames { room, members } => {
            let (Ok(discord), Some((guild, _))) = (discord, discord::parse_room(&room)) else {
                return None;
            };
            let (discord, guild) = (discord.clone(), guild.to_owned());
            tokio::spawn(async move {
                discord.learn_members(&guild, &members).await;
                let members = members
                    .into_iter()
                    .map(|user| RoleMember {
                        name: discord.name(&guild, &user).unwrap_or_else(|| user.clone()),
                        username: discord.username(&user),
                        avatar: discord.avatar(Some(&guild), &user),
                        id: user,
                    })
                    .collect();
                let _ = events.send(Event::OverwriteNames { room, members });
            });
        }
        _ => {}
    }
    None
}

/// Has Discord make a change to a channel's overwrites, as Discord's client
/// sends it.
async fn change_overwrites(
    discord: &Discord,
    channel: &str,
    change: OverwriteChange,
) -> anyhow::Result<()> {
    let overwrites = |list: &[ChannelOverwrite]| -> Vec<Overwrite> {
        list.iter()
            .map(|overwrite| Overwrite {
                id: overwrite.id.clone(),
                member: overwrite.member,
                allow: overwrite.allow,
                deny: overwrite.deny,
            })
            .collect()
    };
    match change {
        OverwriteChange::Put(list) => {
            let list = overwrites(&list);
            discord
                .stepping_up(|| discord.put_overwrites(channel, &list))
                .await
        }
        OverwriteChange::Delete(id) => {
            discord
                .stepping_up(|| discord.delete_overwrite(channel, &id))
                .await
        }
        OverwriteChange::Sync(list) => {
            let list = overwrites(&list);
            discord
                .stepping_up(|| discord.sync_overwrites(channel, &list))
                .await
        }
    }
}
