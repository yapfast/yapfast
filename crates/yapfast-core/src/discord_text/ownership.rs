//! Deleting a Discord server and giving it away, as its owner, as Discord's
//! client does: two-factor authentication confirms either when Discord asks;
//! without it, a code Discord emails confirms a transfer. See "Delete
//! Server and Transfer Ownership" in docs/discord/server-settings.md.

use yapfast_discord::api;

use super::{commands::NOT_DISCORD, *};

/// Discord's code for a new owner who can't take over the server's
/// subscriptions.
const INELIGIBLE_OWNER: u64 = 50164;

/// Deletes or gives away a server, or emails the code for it, or gives back
/// any other command.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let (server, work) = match command {
        Command::Discord(DiscordCommand::DeleteServer { server }) => (server, Work::Delete),
        Command::Discord(DiscordCommand::TransferServer { server, user, code }) => {
            (server, Work::Transfer { user, code })
        }
        Command::Discord(DiscordCommand::SendServerPincode { server }) => (server, Work::Pincode),
        other => return Some(other),
    };
    let (Ok(discord), Some(guild)) = (discord, discord::parse_server(&server)) else {
        let why = discord.err().unwrap_or(NOT_DISCORD).to_owned();
        let _ = events.send(work.answer(server, Err(why)));
        return None;
    };
    let (discord, guild, events) = (discord.clone(), guild.to_owned(), events.clone());
    tokio::spawn(async move {
        let result = match &work {
            Work::Delete => discord
                .stepping_up(|| discord.delete_guild(&guild))
                .await
                .map_err(|error| format!("Couldn't delete the server: {error}")),
            Work::Transfer { user, code } => discord
                .stepping_up(|| discord.transfer_guild(&guild, user, code.as_deref()))
                .await
                .map_err(|error| match api::code(&error) {
                    Some(INELIGIBLE_OWNER) => "Server ownership transfer failed. It looks like \
                                               the new owner is ineligible for Server \
                                               Monetization."
                        .to_owned(),
                    _ => error.to_string(),
                }),
            Work::Pincode => discord
                .send_guild_pincode(&guild)
                .await
                .map_err(|error| error.to_string()),
        };
        if let Err(error) = &result {
            tracing::info!(error, "Discord kept the server as it was");
        }
        let _ = events.send(work.answer(server, result));
    });
    None
}

enum Work {
    Delete,
    Transfer { user: String, code: Option<String> },
    Pincode,
}

impl Work {
    fn answer(&self, server: String, result: Result<(), String>) -> Event {
        match self {
            Work::Delete => Event::ServerDeleted { server, result },
            Work::Transfer { .. } => Event::ServerTransferred { server, result },
            Work::Pincode => Event::ServerPincodeSent { server, result },
        }
    }
}
