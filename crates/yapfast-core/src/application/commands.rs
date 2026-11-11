//! Apps' commands: the `/` picker in the composer, the options of the
//! command being written, and the commands a message's or a person's menu
//! offers. See docs/discord/messages.md, "Apps' commands".
//!
//! The rules it keeps: the index is fetched only once the person types `/`
//! or opens a message's or a person's menu ([`DiscordCommand::CommandIndex`]), a server's is kept until
//! [`Event::CommandIndexChanged`], autocomplete is asked only on a
//! keystroke, and a command runs only on Enter or a menu choice.
//!
//! This holds the open room's index ([`AppCommands`]) and says which of its
//! commands the person may use there ([`Application::slash_commands`],
//! [`Application::commands_for_target`]); the picker, the options and
//! running a command (application/slash.rs) build on it, and the "Apps"
//! submenus of a message's and a person's menu run theirs here.

use super::*;
use crate::chat::{CommandIndex, CommandRun};

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

/// Which "Apps" submenu a command is for: a message's menu, whose message
/// commands (`MESSAGE`, type 3) act on that message, or a person's, whose
/// user commands (`USER`, type 2) act on them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandTarget {
    Message,
    User,
}

impl Application {
    /// [`Action::SlashQuery`], [`Action::PickCommand`],
    /// [`Action::SetCommandOption`], [`Action::FocusCommandOption`],
    /// [`Action::CloseCommand`], [`Action::RunAppCommand`] and
    /// [`Action::AskAppCommands`].
    pub(super) fn command_action(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::SlashQuery(query) => self.slash_query_action(query),
            Action::PickCommand(index) => self.pick_command(index),
            Action::SetCommandOption { index, value } => self.set_command_option(index, value),
            Action::FocusCommandOption(index) => self.focus_option(index),
            Action::CloseCommand(back) => self.close_command(back),
            Action::RunAppCommand {
                command: _,
                target: None,
            } => match self.state.selected.clone() {
                Some(room) => self.run_written_command(&room),
                None => vec![],
            },
            Action::RunAppCommand {
                command,
                target: Some(target),
            } => self.run_menu_command(&command, &target),
            Action::AskAppCommands => self.ask_command_index().into_iter().collect(),
            _ => vec![],
        }
    }

    /// Asks for the open room's command index, unless it's kept or on its
    /// way: the person just typed `/`, or opened a message's or a person's
    /// menu, whose "Apps" lists its message or user commands.
    pub(super) fn ask_command_index(&mut self) -> Option<Effect> {
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
                let asked = self.forget_command_index(guild.as_deref());
                // One the open picker waited for is asked for again, as
                // Discord's client fetches an index it's showing that went
                // out of date.
                match asked {
                    Some(room) if self.slash_open_in(&room) => {
                        self.ask_command_index().into_iter().collect()
                    }
                    _ => vec![],
                }
            }
            Event::AutocompleteChoices { nonce, choices } => {
                self.slash_autocomplete(&nonce, &choices)
            }
            Event::InteractionCreated { nonce, id } => {
                self.command_created(&nonce, id);
                vec![]
            }
            _ => vec![],
        }
    }

    /// Drops the kept index if it's `guild`'s or a DM's, which may be with
    /// one of its bots, or with none whichever it is, so the next `/` asks
    /// again. Gives the room of one on its way, whose answer is dropped.
    fn forget_command_index(&mut self, guild: Option<&str>) -> Option<String> {
        let commands = &mut self.state.app_commands;
        let of_guild = |room: &str| {
            #[cfg(feature = "discord")]
            return guild.is_none_or(|guild| {
                discord::parse_room(room)
                    .is_none_or(|(its, _)| its == guild || its == discord::DM_GUILD)
            });
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
            return commands.asked.take();
        }
        None
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

    /// The message or user commands the person may use in `room`, for its
    /// messages' or its people's "Apps" submenu, in the index's order; none
    /// until the index came. Each is a [`SlashCommand`] without a path.
    pub fn commands_for_target(&self, room: &str, target: CommandTarget) -> Vec<SlashCommand> {
        #[cfg(feature = "discord")]
        if let Some(index) = self.state.app_commands.index(room)
            && let Some(place) = self.command_place(room)
        {
            use yapfast_discord::commands::{MESSAGE, USER};
            let kind = match target {
                CommandTarget::Message => MESSAGE,
                CommandTarget::User => USER,
            };
            return index
                .usable(&place)
                .filter(|command| command.kind == kind)
                .map(|command| {
                    let app = index.app(&command.application_id);
                    SlashCommand {
                        command: command.id.clone(),
                        path: vec![],
                        name: command.name.clone(),
                        description: command.description.clone(),
                        app: command.application_id.clone(),
                        app_name: app.map(|app| app.name.clone()).unwrap_or_default(),
                        app_icon: app.and_then(|app| {
                            let icon = app.icon.as_deref()?;
                            Some(yapfast_discord::ready::application_icon(&app.id, icon))
                        }),
                    }
                })
                .collect();
        }
        let _ = (room, target);
        vec![]
    }

    /// Puts "Apps" in the menu of `user`, seen from `room`, the open room,
    /// when they're a Discord user and its index has user commands the
    /// person may use there: before what's about how this account stands
    /// with them, as Discord's has it.
    pub(super) fn offer_apps(&self, room: &str, user: &str, choices: &mut Vec<MenuChoice>) {
        let discord_user = !user.is_empty() && user.bytes().all(|byte| byte.is_ascii_digit());
        // A choice runs in the open room, so another's menu has none.
        if self.state.preview
            || self.state.selected.as_deref() != Some(room)
            || !discord_user
            || self
                .commands_for_target(room, CommandTarget::User)
                .is_empty()
        {
            return;
        }
        let at = choices
            .iter()
            .position(|choice| {
                matches!(
                    choice,
                    MenuChoice::InviteToServer
                        | MenuChoice::AddFriend
                        | MenuChoice::AcceptFriend
                        | MenuChoice::CancelFriendRequest
                        | MenuChoice::Unfriend
                        | MenuChoice::Ignore
                        | MenuChoice::Unignore
                        | MenuChoice::Block
                        | MenuChoice::Unblock
                        | MenuChoice::CopyLink
                        | MenuChoice::CopyId
                )
            })
            .unwrap_or(choices.len());
        choices.insert(at, MenuChoice::Apps);
    }

    /// A message's or a person's "Apps" choice: runs `command` in the open
    /// room on `target`, a message's item or a Discord user id, as one
    /// written in the composer runs, with no options. A command the room
    /// doesn't offer for that kind of target, or a message that isn't one
    /// of Discord's own (sending, or only this account's), runs nothing.
    fn run_menu_command(&mut self, command: &str, target: &str) -> Vec<Effect> {
        let Some(room) = self.state.selected.clone() else {
            return vec![];
        };
        let message = self
            .state
            .messages
            .iter()
            .find(|message| message.item == target);
        let (kind, target) = match message {
            Some(message) => {
                const EPHEMERAL: u64 = 1 << 6;
                let id = message
                    .item
                    .strip_prefix("d:")
                    .filter(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()));
                match id {
                    Some(id) if message.flags & EPHEMERAL == 0 => {
                        (CommandTarget::Message, id.to_owned())
                    }
                    _ => return vec![],
                }
            }
            None => (CommandTarget::User, target.to_owned()),
        };
        let Some(found) = self
            .commands_for_target(&room, kind)
            .into_iter()
            .find(|offered| offered.command == command)
        else {
            return vec![];
        };
        let Some((spec, _)) = self.slash_command_spec(&room, &found) else {
            return vec![];
        };
        let run = CommandRun {
            // send_command gives it one.
            nonce: String::new(),
            room,
            application: found.app,
            command: spec,
            options: serde_json::Value::Array(vec![]),
            target: Some(target),
            files: vec![],
        };
        self.start_command(run, found.app_name, found.app_icon)
    }

    /// A command `slash_commands` offered, as Discord's index gives it, to
    /// send along with it, and the options of its subcommand, if it has one.
    pub(super) fn slash_command_spec(
        &self,
        room: &str,
        picked: &SlashCommand,
    ) -> Option<(Arc<serde_json::Value>, Vec<serde_json::Value>)> {
        #[cfg(feature = "discord")]
        {
            let command = self
                .state
                .app_commands
                .index(room)?
                .command(&picked.command)?;
            let variant = command
                .variants()
                .into_iter()
                .find(|variant| variant.path.iter().eq(picked.path.iter()))?;
            let options = variant
                .options
                .iter()
                .filter_map(|option| serde_json::to_value(option).ok())
                .collect();
            Some((Arc::new(command.to_json()), options))
        }
        #[cfg(not(feature = "discord"))]
        {
            let _ = (room, picked);
            None
        }
    }

    #[cfg(feature = "discord")]
    fn usable_slash_commands(
        &self,
        room: &str,
        index: &CommandIndex,
        query: &str,
    ) -> Vec<SlashCommand> {
        use yapfast_discord::commands::CHAT_INPUT;
        let Some(place) = self.command_place(room) else {
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
    /// goes: its server, roles and permissions, or who a DM is with, and
    /// whether age-restricted commands are offered, as Discord's client
    /// decides: only to an account allowed to see age-restricted content,
    /// in a server's age-restricted channel or in an age-restricted server,
    /// and in DMs when its settings offer them.
    #[cfg(feature = "discord")]
    fn command_place<'a>(&'a self, room: &'a str) -> Option<yapfast_discord::commands::Place<'a>> {
        use yapfast_discord::commands::Place;
        let (guild, channel) = discord::parse_room(room)?;
        let account = self.discord.as_ref()?.account.as_ref();
        let adult = account.is_some_and(|account| account.nsfw_allowed);
        if guild == discord::DM_GUILD {
            let dm = self
                .discord
                .as_ref()?
                .conversations
                .iter()
                .find(|dm| dm.id == channel)?;
            return Some(Place {
                guild: None,
                channel,
                parent: None,
                recipient: if dm.group { None } else { dm.person.as_deref() },
                permissions: 0,
                roles: &[],
                nsfw: adult && account.is_some_and(|account| account.nsfw_commands_in_dms),
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
            recipient: None,
            permissions: server.channel_permissions(found, me, &server.marks.roles),
            roles: &server.marks.roles,
            nsfw: adult && (nsfw || server.server.age_restricted),
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
