//! Apps' commands: the `/` picker in the composer, the options of the
//! command being written, and the commands a message's or a person's menu
//! offers. See docs/discord/messages.md, "Apps' commands".
//!
//! The rules it keeps: the index is fetched only once the person types `/`
//! ([`DiscordCommand::CommandIndex`]), a server's is kept until
//! [`Event::CommandIndexChanged`], autocomplete is asked only on a
//! keystroke, and a command runs only on Enter or a menu choice.
//!
//! This holds the open room's index ([`AppCommands`]) and says which of its
//! commands the person may use there ([`Application::slash_commands`]); the
//! picker, the options and running a command build on it.

use super::*;
use crate::chat::CommandIndex;

/// The open room's apps' commands, once the person typed `/` there.
#[derive(Default)]
pub struct AppCommands {
    /// The room whose index was asked for, until it comes.
    asked: Option<String>,
    /// A room and its index, merged from its server's or DM's and the
    /// account's own; the service keeps the indexes themselves.
    kept: Option<(String, Arc<CommandIndex>)>,
    /// Why the open room's index couldn't load.
    pub error: Option<String>,
    /// What follows the `/` in the draft.
    pub query: String,
}

impl AppCommands {
    /// `room`'s index, once it came.
    pub fn index(&self, room: &str) -> Option<&Arc<CommandIndex>> {
        self.kept
            .as_ref()
            .filter(|(kept, _)| kept == room)
            .map(|(_, index)| index)
    }

    /// Whether `room`'s index was asked for and hasn't come yet.
    pub fn loading(&self, room: &str) -> bool {
        self.asked.as_deref() == Some(room)
    }
}

/// A command the `/` picker offers: a command, or one of its subcommands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashCommand {
    /// The command's id in the index.
    pub command: String,
    /// The group and subcommand names after the command's, if any.
    pub path: Vec<String>,
    /// As the picker shows it after `/`: `config set`.
    pub name: String,
    pub description: String,
    pub app: String,
    pub app_name: String,
    /// The address of the app's icon, if it has one.
    pub app_icon: Option<String>,
}

impl Application {
    /// [`Action::SlashQuery`], [`Action::PickCommand`],
    /// [`Action::SetCommandOption`] and [`Action::RunAppCommand`].
    pub(super) fn command_action(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::SlashQuery(query) => {
                self.state.app_commands.query = query;
                self.ask_command_index().into_iter().collect()
            }
            Action::PickCommand(_)
            | Action::SetCommandOption { .. }
            | Action::RunAppCommand { .. } => vec![],
            _ => vec![],
        }
    }

    /// Asks for the open room's command index, unless it's kept or on its
    /// way: the person just typed `/`.
    fn ask_command_index(&mut self) -> Option<Effect> {
        let room = self.state.selected.clone()?;
        if !self.state.discord_online || !is_discord_room(&room) {
            return None;
        }
        let commands = &mut self.state.app_commands;
        if commands.index(&room).is_some() || commands.loading(&room) {
            return None;
        }
        commands.asked = Some(room.clone());
        commands.error = None;
        Some(Effect::Backend(Command::Discord(
            DiscordCommand::CommandIndex { room },
        )))
    }

    /// [`Event::InteractionCreated`], [`Event::CommandIndex`],
    /// [`Event::CommandIndexChanged`] and [`Event::AutocompleteChoices`].
    pub(super) fn command_event(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::CommandIndex { room, index } => {
                let commands = &mut self.state.app_commands;
                if commands.asked.as_deref() != Some(room.as_str()) {
                    return vec![];
                }
                commands.asked = None;
                match index {
                    Ok(index) => {
                        commands.kept = Some((room, index));
                        commands.error = None;
                    }
                    Err(why) => commands.error = Some(why),
                }
                vec![]
            }
            Event::CommandIndexChanged { guild } => {
                self.forget_command_index(guild.as_deref());
                vec![]
            }
            Event::InteractionCreated { .. } | Event::AutocompleteChoices { .. } => vec![],
            _ => vec![],
        }
    }

    /// Drops the kept index if it's `guild`'s, or with none whichever it is,
    /// so the next `/` asks again; one on its way is asked for again too.
    fn forget_command_index(&mut self, guild: Option<&str>) {
        let commands = &mut self.state.app_commands;
        let of_guild = |room: &str| {
            #[cfg(feature = "discord")]
            return guild
                .is_none_or(|guild| discord::parse_room(room).is_none_or(|(its, _)| its == guild));
            #[cfg(not(feature = "discord"))]
            {
                let _ = room;
                true
            }
        };
        if commands
            .kept
            .as_ref()
            .is_some_and(|(room, _)| of_guild(room))
        {
            commands.kept = None;
        }
        if commands.asked.as_deref().is_some_and(of_guild) {
            commands.asked = None;
        }
    }

    /// The slash commands the person may use in `room`, whose `query`
    /// (what follows the `/`) starts their name or a word of it, in the
    /// index's order; none until the index came.
    pub fn slash_commands(&self, room: &str, query: &str) -> Vec<SlashCommand> {
        #[cfg(feature = "discord")]
        if let Some(index) = self.state.app_commands.index(room) {
            return self.usable_slash_commands(room, index, query);
        }
        let _ = (room, query);
        vec![]
    }

    #[cfg(feature = "discord")]
    fn usable_slash_commands(
        &self,
        room: &str,
        index: &CommandIndex,
        query: &str,
    ) -> Vec<SlashCommand> {
        use yapfast_discord::commands::CHAT_INPUT;
        let Some(place) = self.command_place(room, index) else {
            return vec![];
        };
        let query = query.trim().to_lowercase();
        let fits = |name: &str| {
            let name = name.to_lowercase();
            query.is_empty()
                || name
                    .split([' ', '-', '_'])
                    .any(|word| word.starts_with(&query))
                || name.starts_with(&query)
        };
        index
            .usable(&place)
            .filter(|command| command.kind == CHAT_INPUT)
            .flat_map(|command| {
                let app = index.app(&command.application_id);
                command
                    .variants()
                    .into_iter()
                    .filter(|variant| fits(&variant.name()))
                    .map(|variant| SlashCommand {
                        command: command.id.clone(),
                        path: variant.path.iter().map(|part| (*part).to_owned()).collect(),
                        name: variant.name(),
                        description: variant.description.to_owned(),
                        app: command.application_id.clone(),
                        app_name: app.map(|app| app.name.clone()).unwrap_or_default(),
                        app_icon: app.and_then(|app| {
                            let icon = app.icon.as_deref()?;
                            Some(yapfast_discord::ready::application_icon(&app.id, icon))
                        }),
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Where `room` is, as far as which commands the person may use there
    /// goes: its server, roles and permissions, or the kind of DM it is.
    #[cfg(feature = "discord")]
    fn command_place<'a>(
        &'a self,
        room: &'a str,
        index: &CommandIndex,
    ) -> Option<yapfast_discord::commands::Place<'a>> {
        use yapfast_discord::{
            commands::{Context, Place},
            ready::permission,
        };
        let (guild, channel) = discord::parse_room(room)?;
        if guild == discord::DM_GUILD {
            let dm = self
                .discord
                .as_ref()?
                .conversations
                .iter()
                .find(|dm| dm.id == channel)?;
            let bot = dm
                .person
                .as_deref()
                .is_some_and(|person| index.is_bot(person));
            return Some(Place {
                guild: None,
                channel,
                parent: None,
                context: if bot {
                    Context::BotDm
                } else {
                    Context::PrivateChannel
                },
                permissions: permission::ALL,
                roles: &[],
                nsfw: false,
            });
        }
        let (server, _, me) = self.discord_place(room)?;
        let found = server.server.channel(channel)?;
        // A thread takes its channel's command permissions, and whether
        // it's age-restricted.
        let parent = found.thread.as_ref().and(found.parent.as_deref());
        let nsfw = match parent {
            Some(parent) => server
                .server
                .channel(parent)
                .is_some_and(|parent| parent.nsfw),
            None => found.nsfw,
        };
        Some(Place {
            guild: Some(guild),
            channel,
            parent,
            context: Context::Guild,
            permissions: server.channel_permissions(found, me, &server.marks.roles),
            roles: &server.marks.roles,
            nsfw,
        })
    }
}

fn is_discord_room(room: &str) -> bool {
    #[cfg(feature = "discord")]
    return discord::parse_room(room).is_some();
    #[cfg(not(feature = "discord"))]
    {
        let _ = room;
        false
    }
}
