//! Bots' controls used and forms sent: the request, and whether Discord
//! answered it.

use super::*;

/// What a message says under the control that failed, as Discord's client.
pub(super) const FAILED: &str = "This interaction failed";

/// Discord's client says this when a control is clicked faster than
/// Discord takes.
const TOO_FAST: &str = "Slow down! You're clicking too fast.";

/// How long an interaction waits for Discord's answer, through a gateway
/// that reconnects in between, before the bot counts as not responding.
/// Discord itself says a bot failed after 3 s.
const ANSWER_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

/// Carries out an interaction, or gives back any other command.
pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    let Command::Discord(DiscordCommand::Interact(body)) = command else {
        return Some(command);
    };
    let nonce = body["nonce"].as_str().unwrap_or_default().to_owned();
    let discord = match discord {
        Ok(discord) => discord.clone(),
        Err(why) => {
            let _ = events.send(Event::InteractionFailed {
                nonce,
                reason: why.into(),
            });
            return None;
        }
    };
    let events = events.clone();
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + ANSWER_WAIT;
        match tokio::time::timeout_at(deadline, discord.interact(body)).await {
            Ok(Err(error)) => {
                tracing::debug!(%error, "Discord refused an interaction");
                let reason = match error.downcast_ref::<yapfast_discord::api::Refused>() {
                    Some(refused) if refused.rate_limited() => TOO_FAST.to_owned(),
                    Some(_) => FAILED.to_owned(),
                    // Not connected, or asked too often: Yapfast's reason.
                    None => error.to_string(),
                };
                let _ = events.send(Event::InteractionFailed { nonce, reason });
            }
            Ok(Ok(())) | Err(_) => {
                tokio::time::sleep_until(deadline).await;
                let _ = events.send(Event::InteractionSettled(nonce));
            }
        }
    });
    None
}
