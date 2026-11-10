//! Bots' buttons, select menus and forms used as this account, apps'
//! commands run and autocompleted, and the gateway's word on each, by its
//! nonce. See docs/discord/messages.md, "Bots' buttons, menus and forms" and
//! "Sending apps' commands". Each interaction comes from a click, a submit,
//! a command sent or a keystroke; none is sent again on its own.

use super::*;
use crate::chat::{CommandRun, Interaction};
use yapfast_discord::interactions::{self, CommandUse, Interaction as DiscordInteraction};

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
        DiscordCommand::RunCommand(run) => {
            run_command(*run, discord, events);
            None
        }
        DiscordCommand::Autocomplete(run) => {
            autocomplete(*run, discord, events);
            None
        }
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

/// Runs an app's command: reads its files, sends them with it in one
/// request, then says, as [`interact`] does, if Discord refused it or never
/// answered.
fn run_command(run: CommandRun, discord: Result<&Discord, &str>, events: &UnboundedSender<Event>) {
    let nonce = run.nonce.clone();
    let failed = |reason: String| Event::InteractionFailed {
        nonce: nonce.clone(),
        reason,
    };
    let discord = match discord {
        Ok(discord) => discord.clone(),
        Err(why) => {
            let _ = events.send(failed(why.to_owned()));
            return;
        }
    };
    let (command, paths) = match command_use(run) {
        Ok(found) => found,
        Err(why) => {
            let _ = events.send(failed(why.to_owned()));
            return;
        }
    };
    let events = events.clone();
    tokio::spawn(async move {
        let mut files = Vec::with_capacity(paths.len());
        for path in &paths {
            match tokio::fs::read(path).await {
                Ok(data) => files.push(data),
                Err(_) => {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let _ = events.send(Event::InteractionFailed {
                        nonce,
                        reason: format!("Yapfast could not read {name}."),
                    });
                    return;
                }
            }
        }
        match discord.run_command(command, files).await {
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

/// Asks an app for its choices for the option being typed, and passes them
/// on if this is still the newest autocomplete when they come. A failed one
/// gives nothing: the choices already shown stay.
fn autocomplete(run: CommandRun, discord: Result<&Discord, &str>, events: &UnboundedSender<Event>) {
    let Ok(discord) = discord else { return };
    let Ok((mut command, _)) = command_use(run) else {
        return;
    };
    command.files.clear();
    let discord = discord.clone();
    let events = events.clone();
    tokio::spawn(async move {
        let nonce = command.nonce.clone();
        match discord.autocomplete(command).await {
            Ok(Some(choices)) => {
                let _ = events.send(Event::AutocompleteChoices { nonce, choices });
            }
            Ok(None) => {}
            Err(error) => tracing::debug!("an autocomplete failed: {error:#}"),
        }
    });
}

/// A command to send, with the paths of its files, or why it can't go.
fn command_use(
    CommandRun {
        nonce,
        room,
        application,
        command,
        options,
        target,
        files,
    }: CommandRun,
) -> Result<(CommandUse, Vec<PathBuf>), &'static str> {
    let (guild, channel) = discord::parse_room(&room).ok_or("Open a Discord channel first.")?;
    if files.len() > MAX_COMMAND_FILES {
        return Err("A command carries up to 25 files.");
    }
    let names = files
        .iter()
        .map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .ok_or("Choose a file to send.")
        })
        .collect::<Result<_, _>>()?;
    Ok((
        CommandUse {
            nonce,
            guild: (guild != discord::DM_GUILD).then(|| guild.to_owned()),
            channel: channel.to_owned(),
            application,
            command,
            options,
            target,
            files: names,
        },
        files,
    ))
}

/// The most files one interaction carries, as docs.discord.food gives it.
const MAX_COMMAND_FILES: usize = 25;

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
        // Only the newest autocomplete's, which [`autocomplete`] passes on.
        Live::AutocompleteChoices { .. } => return None,
        Live::CommandIndexChanged { guild } => Event::CommandIndexChanged { guild },
        other => return Some(other),
    };
    let _ = events.send(event);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(room: &str, files: Vec<PathBuf>) -> CommandRun {
        CommandRun {
            nonce: "5".into(),
            room: room.into(),
            application: "9".into(),
            command: Arc::new(serde_json::json!({"id": "30", "name": "upload"})),
            options: serde_json::json!([{"type": 11, "name": "file", "value": "0"}]),
            target: None,
            files,
        }
    }

    #[test]
    fn a_command_is_sent_where_it_was_typed_with_its_files_named() {
        let (command, paths) = command_use(run(
            &discord::room_id("1", "2"),
            vec!["/tmp/a b.png".into()],
        ))
        .unwrap();
        assert_eq!(command.guild.as_deref(), Some("1"));
        assert_eq!(command.channel, "2");
        assert_eq!(command.files, ["a b.png"]);
        assert_eq!(paths, [PathBuf::from("/tmp/a b.png")]);
        let (command, _) =
            command_use(run(&discord::room_id(discord::DM_GUILD, "3"), vec![])).unwrap();
        assert_eq!(command.guild, None);
        assert!(command_use(run("matrix:!room", vec![])).is_err());
    }
}
