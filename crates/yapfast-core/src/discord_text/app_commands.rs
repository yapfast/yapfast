//! Apps' commands: the index of those the person may use where they are.
//! A stub that phase 1a of the bot commands fills in (see
//! bots-gap-analysis.md): it fetches the index only once the person types
//! `/`, keeps a server's until [`Event::CommandIndexChanged`], and never
//! asks ahead.

use super::*;

/// Carries out an app command index command, or gives back any other.
pub(super) fn command(
    command: Command,
    _discord: Result<&Discord, &str>,
    _events: &UnboundedSender<Event>,
) -> Option<Command> {
    match command {
        // Nothing sends it yet.
        Command::Discord(DiscordCommand::CommandIndex { .. }) => None,
        command => Some(command),
    }
}
