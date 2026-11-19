//! Apps' commands: the index of those a room offers, its server's or DM's
//! merged with the account's own. It's fetched only once the person types
//! `/` ([`DiscordCommand::CommandIndex`]); the service keeps each index
//! until Discord says it changed, so asking again for the same server
//! fetches nothing. See docs/discord/messages.md, "Apps' commands".

use super::*;

/// Carries out an app command index command, or gives back any other.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let Command::Discord(DiscordCommand::CommandIndex { room }) = command else {
        return Some(command);
    };
    let discord = match (discord, discord::parse_room(&room)) {
        (Ok(discord), Some(_)) => discord.clone(),
        (Err(why), _) => {
            let _ = events.send(Event::CommandIndex {
                room,
                index: Err(why.to_owned()),
            });
            return None;
        }
        // Only Discord's channels have commands.
        (Ok(_), None) => return None,
    };
    let events = events.clone();
    tokio::spawn(async move {
        let Some((guild, channel)) = discord::parse_room(&room) else {
            return;
        };
        let guild = (guild != discord::DM_GUILD).then_some(guild);
        let index = discord
            .command_index(guild, channel)
            .await
            .map_err(|error| {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "could not fetch apps' commands"
                );
                format!("Couldn't load apps' commands: {error}")
            });
        let _ = events.send(Event::CommandIndex { room, index });
    });
    None
}
