//! Bots' buttons, select menus and forms used as this account, and the
//! gateway's word on each, by its nonce. See docs/discord/messages.md, "Bots'
//! buttons, menus and forms". Each interaction comes from a click or a
//! submit; none is sent again on its own.

use super::*;
use crate::chat::Interaction;
use yapfast_discord::interactions::{self, Interaction as DiscordInteraction};

/// Carries out an interaction command, or gives back any other.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let Command::Discord(command) = command else {
        return Some(command);
    };
    match command {
        DiscordCommand::Interact(interaction) => {
            interact(interaction, discord, events);
            None
        }
        // Phase 1c of the bot commands sends these.
        DiscordCommand::RunCommand(run) => {
            let _ = events.send(Event::InteractionFailed {
                nonce: run.nonce,
                reason: "Yapfast can't run app commands yet.".into(),
            });
            None
        }
        DiscordCommand::Autocomplete(_) => None,
        command => Some(Command::Discord(command)),
    }
}

/// Sends `interaction`, then says if Discord refused it, or if the gateway
/// never answered it. The answer itself comes as a live event; see [`live`].
fn interact(
    interaction: Box<Interaction>,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) {
    let nonce = interaction.nonce().to_owned();
    let discord = match discord {
        Ok(discord) => discord.clone(),
        Err(why) => {
            let _ = events.send(Event::InteractionFailed {
                nonce,
                reason: why.to_owned(),
            });
            return;
        }
    };
    let events = events.clone();
    tokio::spawn(async move {
        match discord.interact(&for_discord(*interaction)).await {
            Ok(true) => {}
            Ok(false) => {
                let _ = events.send(Event::InteractionSettled(nonce));
            }
            Err(error) => {
                let _ = events.send(Event::InteractionFailed {
                    nonce,
                    reason: error.to_string(),
                });
            }
        }
    });
}

fn for_discord(interaction: Interaction) -> DiscordInteraction {
    match interaction {
        Interaction::Component {
            nonce,
            guild,
            channel,
            message,
            message_flags,
            application,
            author,
            component_type,
            custom_id,
            values,
        } => DiscordInteraction::Component {
            nonce,
            guild,
            channel,
            message,
            message_flags,
            application,
            author,
            component_type,
            custom_id,
            values,
        },
        Interaction::Modal {
            nonce,
            guild,
            channel,
            application,
            id,
            custom_id,
            components,
        } => DiscordInteraction::Modal {
            nonce,
            guild,
            channel,
            application,
            id,
            custom_id,
            components,
        },
    }
}

/// Turns the gateway's word on an interaction, or on app commands, into its
/// event, or gives back any other live event.
pub(super) fn live(live: Live, events: &UnboundedSender<Event>) -> Option<Live> {
    let event = match live {
        Live::InteractionCreated { nonce, id } => Event::InteractionCreated { nonce, id },
        Live::InteractionSuccess { nonce } => Event::InteractionSucceeded(nonce),
        Live::InteractionFailure { nonce, reason } => Event::InteractionFailed {
            nonce,
            reason: interactions::failure_reason(reason).into(),
        },
        Live::InteractionModal(form) => Event::InteractionModal(form),
        Live::InteractionPage(page) => Event::InteractionFailed {
            nonce: page["nonce"].as_str().unwrap_or_default().to_owned(),
            reason: "This app opens a page Yapfast can't show. Use it in Discord.".into(),
        },
        Live::AutocompleteChoices { nonce, choices } => {
            Event::AutocompleteChoices { nonce, choices }
        }
        Live::CommandIndexChanged { guild } => Event::CommandIndexChanged { guild },
        other => return Some(other),
    };
    let _ = events.send(event);
    None
}
