//! Apps' commands, as Discord's application command indexes list them: a
//! server's, the DM with a bot's, and those of the apps this account
//! installed for itself. See docs/discord/messages.md, "Apps' commands", and
//! <https://docs.discord.food/interactions/application-commands#application-command-index-object>.
//!
//! An index is fetched only once the person types `/` or opens a menu with
//! Apps (see [`crate::service::Discord::command_index`]), and kept until
//! Discord's client would count it out of date ([`Stale`]): then the next use
//! fetches it again. One that couldn't be had isn't asked for again for
//! [`RETRY_AFTER`]. Nothing here asks ahead or on its own.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
use tokio::sync::{Mutex, broadcast};

use crate::{ready::permission, service::Live};

/// `CHAT_INPUT`: a command typed after `/`.
pub const CHAT_INPUT: u8 = 1;
/// `USER`: a command a person's menu offers under Apps.
pub const USER: u8 = 2;
/// `MESSAGE`: a command a message's menu offers under Apps.
pub const MESSAGE: u8 = 3;
/// `PRIMARY_ENTRY_POINT`: an app's way to start its Activity.
pub const PRIMARY_ENTRY_POINT: u8 = 4;

/// The kinds of a command's option, by its `type`.
pub mod option {
    pub const SUB_COMMAND: u8 = 1;
    pub const SUB_COMMAND_GROUP: u8 = 2;
    pub const STRING: u8 = 3;
    pub const INTEGER: u8 = 4;
    pub const BOOLEAN: u8 = 5;
    pub const USER: u8 = 6;
    pub const CHANNEL: u8 = 7;
    pub const ROLE: u8 = 8;
    pub const MENTIONABLE: u8 = 9;
    pub const NUMBER: u8 = 10;
    pub const ATTACHMENT: u8 = 11;
}

/// `USE_APPLICATION_COMMANDS`, which a server's commands need in a channel.
pub const USE_APPLICATION_COMMANDS: u64 = 1 << 31;

/// `GUILD_INSTALL` and `USER_INSTALL` among a command's `integration_types`.
const GUILD_INSTALL: u8 = 0;
const USER_INSTALL: u8 = 1;

/// The most indexes kept: a few servers' and DMs' besides this account's.
/// The one used longest ago goes first.
const KEPT: usize = 16;

/// How long an index that couldn't be had isn't asked for again, as
/// Discord's client waits.
pub const RETRY_AFTER: Duration = Duration::from_secs(5);

/// How often an index Discord is still building (a 202) is asked for, and
/// how long apart, as Discord's client asks.
const BUILDING_TRIES: usize = 3;
const BUILDING_WAIT: Duration = Duration::from_secs(5);

/// Where a command may be used, as a command's `contexts` list them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Context {
    /// A server's channel.
    Guild = 0,
    /// The DM with the app's own bot.
    BotDm = 1,
    /// Any other DM or group DM.
    PrivateChannel = 2,
}

/// An application command index, or several merged: the apps and their
/// commands. Each is shared with the index it came from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandIndex {
    pub applications: Vec<Arc<CommandApp>>,
    pub commands: Vec<Arc<Command>>,
    /// The index's version, which GUILD_APPLICATION_COMMAND_INDEX_UPDATE
    /// names when it changed.
    pub version: String,
    /// The apps installed where the commands are used: those of the
    /// server's or the DM's index, rather than only this account's.
    pub installed_here: Vec<String>,
}

/// An app a command index lists.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct CommandApp {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Its icon's hash, for `/app-icons/{id}/{icon}.png`.
    #[serde(default)]
    pub icon: Option<String>,
    /// Its bot's user id: a DM with that user is the app's [`Context::BotDm`].
    #[serde(default)]
    pub bot_id: Option<String>,
    /// Who may use its commands in the server, unless a command says
    /// otherwise.
    #[serde(default)]
    pub permissions: Option<Permissions>,
}

/// Who may use an app's commands, or one command, in a server: this
/// account's own override, its roles' (the server's id for @everyone) and
/// the channels' (the server's id less one for all of them).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub user: Option<bool>,
    #[serde(default)]
    pub roles: HashMap<String, bool>,
    #[serde(default)]
    pub channels: HashMap<String, bool>,
}

/// A command as an index gives it, with the entry itself in `raw`, which
/// `POST /interactions` takes whole in `data.application_command`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Command {
    pub id: String,
    /// [`CHAT_INPUT`], [`USER`], [`MESSAGE`] or [`PRIMARY_ENTRY_POINT`].
    #[serde(rename = "type", default = "chat_input")]
    pub kind: u8,
    pub application_id: String,
    #[serde(default)]
    pub guild_id: Option<String>,
    /// In Discord's locale: the index sends `name_localized` as `name`.
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub options: Vec<CommandOption>,
    /// The permissions, as a decimal string, a member needs to use it in a
    /// server; `"0"` leaves it to administrators.
    #[serde(default)]
    pub default_member_permissions: Option<String>,
    /// Where it may be used. With none, as an older command made it:
    /// servers and its bot's DM, unless `dm_permission` is false.
    #[serde(default)]
    pub contexts: Option<Vec<u8>>,
    #[serde(default)]
    pub dm_permission: Option<bool>,
    #[serde(default)]
    pub integration_types: Option<Vec<u8>>,
    /// Age-restricted.
    #[serde(default)]
    pub nsfw: bool,
    #[serde(default)]
    pub permissions: Option<Permissions>,
    #[serde(default)]
    pub version: String,
    /// From this account's own index, `/users/@me`, rather than the
    /// place's: a server's permissions don't apply to it if it's installed
    /// for the account (`integration_types` has `USER_INSTALL`).
    #[serde(skip)]
    pub user_installed: bool,
    /// The entry as Discord sent it, which Discord's client sends back
    /// as it came.
    #[serde(skip)]
    pub raw: Box<str>,
}

fn chat_input() -> u8 {
    CHAT_INPUT
}

/// A command's option, or a subcommand or group of them with theirs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CommandOption {
    /// See [`option`].
    #[serde(rename = "type")]
    pub kind: u8,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub choices: Vec<Choice>,
    /// A subcommand's options, or a group's subcommands.
    #[serde(default)]
    pub options: Vec<CommandOption>,
    #[serde(default)]
    pub channel_types: Option<Vec<u64>>,
    #[serde(default)]
    pub min_value: Option<Number>,
    #[serde(default)]
    pub max_value: Option<Number>,
    #[serde(default)]
    pub min_length: Option<u32>,
    #[serde(default)]
    pub max_length: Option<u32>,
    #[serde(default)]
    pub autocomplete: bool,
}

/// One of the values an option takes, by the name it shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub name: String,
    /// A string, an integer or a number, as the option's kind.
    pub value: Value,
}

/// A command as the `/` picker lists it: a command without subcommands, or
/// one subcommand of it, with its options.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant<'a> {
    pub command: &'a Command,
    /// The group and subcommand names after the command's, as the options
    /// of `POST /interactions` nest them.
    pub path: Vec<&'a str>,
    pub description: &'a str,
    pub options: &'a [CommandOption],
}

impl Variant<'_> {
    /// What the picker shows after `/`: `config set`.
    pub fn name(&self) -> String {
        let mut name = self.command.name.clone();
        for part in &self.path {
            name.push(' ');
            name.push_str(part);
        }
        name
    }
}

impl Command {
    /// The command as Discord's index gave it.
    pub fn to_json(&self) -> Value {
        serde_json::from_str(&self.raw).unwrap_or_default()
    }

    /// The command itself, or each of its subcommands, groups' included, in
    /// Discord's order.
    pub fn variants(&self) -> Vec<Variant<'_>> {
        let sub = |option: &CommandOption| option.kind == option::SUB_COMMAND;
        let group = |option: &CommandOption| option.kind == option::SUB_COMMAND_GROUP;
        if !self
            .options
            .iter()
            .any(|option| sub(option) || group(option))
        {
            return vec![Variant {
                command: self,
                path: vec![],
                description: &self.description,
                options: &self.options,
            }];
        }
        let mut variants = vec![];
        for option in &self.options {
            if sub(option) {
                variants.push(Variant {
                    command: self,
                    path: vec![&option.name],
                    description: &option.description,
                    options: &option.options,
                });
            } else if group(option) {
                for inner in option.options.iter().filter(|inner| sub(inner)) {
                    variants.push(Variant {
                        command: self,
                        path: vec![&option.name, &inner.name],
                        description: &inner.description,
                        options: &inner.options,
                    });
                }
            }
        }
        variants
    }
}

/// Where this account is about to use a command, as far as which ones it
/// may use goes.
#[derive(Clone, Copy, Debug)]
pub struct Place<'a> {
    /// The server; none in a DM or group DM.
    pub guild: Option<&'a str>,
    pub channel: &'a str,
    /// A thread's channel, whose command permissions the thread takes.
    pub parent: Option<&'a str>,
    /// In a one-to-one DM, the other person: the DM is the
    /// [`Context::BotDm`] of their app's commands.
    pub recipient: Option<&'a str>,
    /// This account's permissions in the channel; [`permission::ALL`] for
    /// an owner or administrator. Not read outside a server.
    pub permissions: u64,
    /// This account's roles in the server.
    pub roles: &'a [String],
    /// Age-restricted commands are offered here: the account may see
    /// age-restricted content, and the channel or server is age-restricted,
    /// or in a DM its settings offer them.
    pub nsfw: bool,
}

impl CommandIndex {
    /// An index as `GET .../application-command-index` gives it, with
    /// `user_installed` for `/users/@me`'s. A command or app Yapfast can't
    /// read is left out.
    pub fn parse(value: &Value, user_installed: bool) -> Self {
        let each = |key: &str| value[key].as_array().into_iter().flatten();
        let applications: Vec<Arc<CommandApp>> = each("applications")
            .filter_map(|app| {
                CommandApp::deserialize(app)
                    .inspect_err(
                        |error| tracing::debug!(%error, "unreadable app in a command index"),
                    )
                    .ok()
            })
            .map(Arc::new)
            .collect();
        let commands = each("application_commands")
            .filter_map(|raw| {
                Command::deserialize(raw)
                    .inspect_err(|error| tracing::debug!(%error, "unreadable command in an index"))
                    .ok()
                    .map(|command| (command, raw))
            })
            .map(|(mut command, raw)| {
                command.user_installed = user_installed;
                command.raw = raw.to_string().into_boxed_str();
                Arc::new(command)
            })
            .collect();
        let installed_here = if user_installed {
            vec![]
        } else {
            applications.iter().map(|app| app.id.clone()).collect()
        };
        Self {
            applications,
            commands,
            version: value["version"].as_str().unwrap_or_default().to_owned(),
            installed_here,
        }
    }

    /// This account's own index, `self`, with a place's, in the place's
    /// order and then the account's: Discord's client takes a command in
    /// both from the account's, and its app from the place's, which says
    /// who may use it there. Each is shared.
    pub fn merged(&self, place: &CommandIndex) -> CommandIndex {
        let mut merged = CommandIndex {
            applications: place.applications.clone(),
            commands: place
                .commands
                .iter()
                .map(|command| self.command(&command.id).unwrap_or(command).clone())
                .collect(),
            version: place.version.clone(),
            installed_here: place.installed_here.clone(),
        };
        for app in &self.applications {
            if !merged.applications.iter().any(|kept| kept.id == app.id) {
                merged.applications.push(app.clone());
            }
        }
        for command in &self.commands {
            if !merged.commands.iter().any(|kept| kept.id == command.id) {
                merged.commands.push(command.clone());
            }
        }
        merged
    }

    pub fn app(&self, id: &str) -> Option<&CommandApp> {
        self.applications
            .iter()
            .find(|app| app.id == id)
            .map(|app| &**app)
    }

    pub fn command(&self, id: &str) -> Option<&Arc<Command>> {
        self.commands.iter().find(|command| command.id == id)
    }

    /// The commands this account may use at `place`, in the index's
    /// order, as Discord's client decides; see [`Self::may_use`].
    pub fn usable<'a>(&'a self, place: &'a Place<'a>) -> impl Iterator<Item = &'a Arc<Command>> {
        self.commands
            .iter()
            .filter(move |command| self.may_use(command, place))
    }

    /// Whether this account may use `command` at `place`, step by step as
    /// Discord's client: where it may be used, whether it's age-restricted,
    /// then in a server, unless an administrator or it's installed for the
    /// account, the permissions to use commands and send there, the
    /// command's and then its app's overrides for the channel, the
    /// command's for the member, its app's for the member, which can only
    /// refuse, then `default_member_permissions`.
    pub fn may_use(&self, command: &Command, place: &Place) -> bool {
        let app = self.app(&command.application_id);
        let context = match (place.guild, place.recipient) {
            (Some(_), _) => Context::Guild,
            (None, Some(person)) if app.and_then(|app| app.bot_id.as_deref()) == Some(person) => {
                Context::BotDm
            }
            (None, _) => Context::PrivateChannel,
        };
        let allowed_here = match &command.contexts {
            Some(contexts) => contexts.contains(&(context as u8)),
            None => match context {
                Context::Guild => true,
                Context::BotDm => command.dm_permission != Some(false),
                Context::PrivateChannel => false,
            },
        };
        if !allowed_here || (command.nsfw && !place.nsfw) {
            return false;
        }
        let Some(guild) = place.guild else {
            return true;
        };
        if command.guild_id.as_deref().is_some_and(|its| its != guild) {
            return false;
        }
        let installs = command.integration_types.as_deref();
        if place.permissions == permission::ALL
            || place.permissions & permission::ADMINISTRATOR != 0
            || (command.user_installed
                && installs.is_some_and(|types| types.contains(&USER_INSTALL)))
        {
            return true;
        }
        let send = if place.parent.is_some() {
            permission::SEND_MESSAGES_IN_THREADS
        } else {
            permission::SEND_MESSAGES
        };
        let base = permission::VIEW_CHANNEL | USE_APPLICATION_COMMANDS | send;
        let installed = self.installed_here.contains(&command.application_id);
        if place.permissions & base != base
            && installed
            && installs.is_none_or(|types| types.contains(&GUILD_INSTALL))
        {
            return false;
        }
        let app = app.and_then(|app| app.permissions.as_ref());
        let own = command.permissions.as_ref();
        let channel = own.and_then(|own| channel_rule(own, guild, place));
        let app_channel = app.and_then(|app| channel_rule(app, guild, place));
        if channel == Some(false) || (channel.is_none() && app_channel == Some(false)) {
            return false;
        }
        if let Some(allowed) = own.and_then(|own| member_rule(own, guild, place)) {
            return allowed;
        }
        if app.and_then(|app| member_rule(app, guild, place)) == Some(false) {
            return false;
        }
        match &command.default_member_permissions {
            None => true,
            Some(needed) => needed
                .parse::<u64>()
                .is_ok_and(|needed| needed != 0 && place.permissions & needed == needed),
        }
    }
}

/// What a command's or app's permissions say of `place`'s channel: its own
/// (a thread its channel's), else all channels'.
fn channel_rule(permissions: &Permissions, guild: &str, place: &Place) -> Option<bool> {
    let channel = place.parent.unwrap_or(place.channel);
    permissions.channels.get(channel).copied().or_else(|| {
        let all = guild.parse::<u64>().ok()?.checked_sub(1)?.to_string();
        permissions.channels.get(&all).copied()
    })
}

/// What a command's or app's permissions say of this account: its own
/// override, else any of its roles allowed, else any denied, else
/// @everyone's.
fn member_rule(permissions: &Permissions, guild: &str, place: &Place) -> Option<bool> {
    if let Some(user) = permissions.user {
        return Some(user);
    }
    let mut roles = place
        .roles
        .iter()
        .filter_map(|role| permissions.roles.get(role).copied())
        .peekable();
    if roles.peek().is_some() {
        return Some(roles.any(|allowed| allowed));
    }
    permissions.roles.get(guild).copied()
}

/// Whose index: a server's, a DM's or group DM's, or this account's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    Guild(String),
    Channel(String),
    User,
}

impl Owner {
    /// The resource the index is under, as `GET /{this}/application-command-index`
    /// takes it.
    pub fn resource(&self) -> String {
        match self {
            Owner::Guild(guild) => format!("guilds/{guild}"),
            Owner::Channel(channel) => format!("channels/{channel}"),
            Owner::User => "users/@me".to_owned(),
        }
    }
}

/// Which kept indexes are out of date, as Discord's client counts them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stale {
    /// A new gateway session (READY): every one.
    All,
    /// GUILD_APPLICATION_COMMAND_INDEX_UPDATE: a server's, now at
    /// `version`, and the DMs with its apps' bots.
    Guild {
        guild: String,
        version: Option<String>,
    },
    /// USER_APPLICATION_UPDATE or USER_APPLICATION_REMOVE: this account's own.
    User,
}

/// The indexes fetched, until Discord's client would count them out of
/// date. Shared by the clones of [`crate::service::Discord`].
#[derive(Clone, Default)]
pub struct Kept(Arc<std::sync::Mutex<Vec<Slot>>>);

/// An index kept, or being fetched, and what's known of it.
struct Slot {
    owner: Owner,
    /// For a DM's, the bot it's with.
    bot: Option<String>,
    /// For a server's, its apps' bots, whose DMs' indexes go out of date
    /// with it.
    bots: Vec<String>,
    /// The kept index's version.
    version: String,
    stale: bool,
    /// When it last couldn't be had.
    failed: Option<Instant>,
    /// The index, locked while it's fetched, so one index is fetched once
    /// however many ask, while others' go on.
    index: Arc<Mutex<Option<Arc<CommandIndex>>>>,
}

impl Kept {
    /// Keeps indexes, marking them out of date as the gateway says.
    pub fn watching(mut live: broadcast::Receiver<Live>) -> Self {
        let kept = Self::default();
        let watched = kept.clone();
        tokio::spawn(async move {
            loop {
                let stale = match live.recv().await {
                    Ok(Live::CommandIndexStale(stale)) => stale,
                    Ok(_) => continue,
                    // A change may have gone by unseen.
                    Err(broadcast::error::RecvError::Lagged(_)) => Stale::All,
                    Err(broadcast::error::RecvError::Closed) => return,
                };
                watched.mark(&stale);
            }
        });
        kept
    }

    fn slots(&self) -> std::sync::MutexGuard<'_, Vec<Slot>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Marks the indexes `stale` names out of date, for the next use to
    /// fetch again.
    pub fn mark(&self, stale: &Stale) {
        let mut slots = self.slots();
        let bots: Vec<String> = match stale {
            Stale::Guild { guild, version } => slots
                .iter_mut()
                .filter(|slot| matches!(&slot.owner, Owner::Guild(id) if id == guild))
                .filter(|slot| version.as_ref() != Some(&slot.version))
                .flat_map(|slot| {
                    slot.stale = true;
                    slot.bots.clone()
                })
                .collect(),
            _ => vec![],
        };
        for slot in slots.iter_mut() {
            let out = match stale {
                Stale::All => true,
                Stale::User => slot.owner == Owner::User,
                Stale::Guild { .. } => slot.bot.as_ref().is_some_and(|bot| bots.contains(bot)),
            };
            slot.stale |= out;
        }
    }

    /// `owner`'s index, kept or fetched with `fetch`, given its
    /// [`Owner::resource`], which gives None while Discord is still building
    /// it. `bot` is the bot a DM's is with. One out of date that can't be
    /// fetched again is still given.
    pub async fn get<F, Fetched>(
        &self,
        owner: Owner,
        bot: Option<&str>,
        mut fetch: F,
    ) -> anyhow::Result<Arc<CommandIndex>>
    where
        F: FnMut(String) -> Fetched,
        Fetched: std::future::Future<Output = anyhow::Result<Option<Value>>>,
    {
        let index = self.slot(&owner, bot);
        let mut index = index.lock().await;
        let (stale, failed) = self
            .slots()
            .iter()
            .find(|slot| slot.owner == owner)
            .map_or((false, None), |slot| (slot.stale, slot.failed));
        if let Some(kept) = index.as_ref()
            && !stale
        {
            return Ok(kept.clone());
        }
        if failed.is_some_and(|at| at.elapsed() < RETRY_AFTER) {
            return index.clone().ok_or_else(|| {
                anyhow!("Discord didn't list the commands. Try again in a moment.")
            });
        }
        let mut fetched = Ok(None);
        for tries in 0..BUILDING_TRIES {
            if tries > 0 {
                tokio::time::sleep(BUILDING_WAIT).await;
            }
            fetched = fetch(owner.resource()).await;
            if !matches!(fetched, Ok(None)) {
                break;
            }
        }
        let fetched = fetched.and_then(|value| {
            value.ok_or_else(|| anyhow!("Discord is still listing the commands. Try again soon."))
        });
        let mut slots = self.slots();
        let slot = slots.iter_mut().find(|slot| slot.owner == owner);
        match fetched {
            Ok(value) => {
                let fresh = Arc::new(CommandIndex::parse(&value, owner == Owner::User));
                if let Some(slot) = slot {
                    slot.version = fresh.version.clone();
                    slot.bots = fresh
                        .applications
                        .iter()
                        .filter_map(|app| app.bot_id.clone())
                        .collect();
                    slot.stale = false;
                    slot.failed = None;
                }
                *index = Some(fresh.clone());
                Ok(fresh)
            }
            Err(error) => {
                if let Some(slot) = slot {
                    slot.failed = Some(Instant::now());
                }
                index.clone().ok_or(error)
            }
        }
    }

    /// The slot of `owner`'s index, made if there's none, and now the one
    /// used last.
    fn slot(&self, owner: &Owner, bot: Option<&str>) -> Arc<Mutex<Option<Arc<CommandIndex>>>> {
        let mut slots = self.slots();
        let slot = match slots.iter().position(|slot| slot.owner == *owner) {
            Some(at) => slots.remove(at),
            None => {
                if slots.len() >= KEPT
                    && let Some(at) = slots.iter().position(|slot| slot.owner != Owner::User)
                {
                    slots.remove(at);
                }
                Slot {
                    owner: owner.clone(),
                    bot: bot.map(str::to_owned),
                    bots: vec![],
                    version: String::new(),
                    stale: false,
                    failed: None,
                    index: Arc::default(),
                }
            }
        };
        let index = slot.index.clone();
        slots.push(slot);
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const GUILD: &str = "100";
    const MEMBER: u64 =
        permission::VIEW_CHANNEL | USE_APPLICATION_COMMANDS | permission::SEND_MESSAGES;

    fn index() -> CommandIndex {
        CommandIndex::parse(
            &json!({
                "applications": [
                    {"id": "7", "name": "Status Bot", "icon": null, "bot_id": "7", "flags": "0",
                     "permissions": {"channels": {"77": false}}},
                ],
                "application_commands": [
                    {"id": "1", "type": 1, "application_id": "7", "name": "ping", "description": "Pong",
                     "default_member_permissions": null, "version": "1", "name_default": "ping"},
                    {"id": "2", "type": 1, "application_id": "7", "name": "admin", "description": "Admins",
                     "default_member_permissions": "8", "version": "1"},
                    {"id": "3", "type": 1, "application_id": "7", "name": "config", "description": "Settings",
                     "default_member_permissions": null, "version": "1", "options": [
                        {"type": 1, "name": "get", "description": "Show it"},
                        {"type": 2, "name": "role", "description": "Roles", "options": [
                            {"type": 1, "name": "set", "description": "Set it", "options": [
                                {"type": 8, "name": "role", "description": "Which", "required": true}]}]}]},
                    {"id": "4", "type": 1, "application_id": "7", "name": "spicy", "description": "18+",
                     "nsfw": true, "default_member_permissions": null, "version": "1"},
                    {"id": "5", "type": 1, "application_id": "7", "name": "quiet", "description": "Mods",
                     "default_member_permissions": null, "version": "1",
                     "permissions": {"roles": {"20": true, "100": false}}},
                    {"id": "6", "type": 3, "application_id": "7", "name": "Quote", "description": "",
                     "default_member_permissions": null, "version": "1", "contexts": [1, 2]},
                    {"id": "11", "type": 1, "application_id": "7", "name": "owners", "description": "",
                     "default_member_permissions": "0", "version": "1"},
                    {"name": "broken"},
                ],
                "version": "9"
            }),
            false,
        )
    }

    fn names<'a>(index: &'a CommandIndex, place: &'a Place<'a>) -> Vec<&'a str> {
        index
            .usable(place)
            .map(|command| command.name.as_str())
            .collect()
    }

    fn channel(permissions: u64, roles: &[String]) -> Place<'_> {
        Place {
            guild: Some(GUILD),
            channel: "50",
            parent: None,
            recipient: None,
            permissions,
            roles,
            nsfw: false,
        }
    }

    fn dm(recipient: Option<&str>) -> Place<'_> {
        Place {
            guild: None,
            channel: "60",
            parent: None,
            recipient,
            permissions: 0,
            roles: &[],
            nsfw: false,
        }
    }

    #[test]
    fn a_command_goes_back_to_discord_as_the_index_gave_it() {
        let index = index();
        assert_eq!(index.commands.len(), 7, "the unreadable one is left out");
        assert_eq!(index.version, "9");
        let ping = index.command("1").unwrap();
        assert_eq!(
            ping.to_json(),
            json!({"id": "1", "type": 1, "application_id": "7", "name": "ping", "description": "Pong",
                   "default_member_permissions": null, "version": "1", "name_default": "ping"})
        );
        let config = index.command("3").unwrap();
        assert_eq!(
            config.to_json()["options"][1]["options"][0]["options"][0]["required"],
            true
        );
        let variants: Vec<_> = config.variants().iter().map(Variant::name).collect();
        assert_eq!(variants, ["config get", "config role set"]);
        assert_eq!(config.variants()[1].path, ["role", "set"]);
        assert_eq!(config.variants()[1].options[0].kind, option::ROLE);
    }

    #[test]
    fn a_servers_commands_follow_its_permissions() {
        let index = index();
        let none: Vec<String> = vec![];
        assert_eq!(names(&index, &channel(MEMBER, &none)), ["ping", "config"]);
        let mods = vec!["20".to_owned()];
        assert_eq!(
            names(&index, &channel(MEMBER, &mods)),
            ["ping", "config", "quiet"]
        );
        assert_eq!(
            names(&index, &channel(permission::ALL, &none)),
            ["ping", "admin", "config", "quiet", "owners"],
            "an administrator may use all but the age-restricted, \"0\" included"
        );
        assert_eq!(
            names(&index, &channel(MEMBER | permission::ADMINISTRATOR, &none)),
            ["ping", "admin", "config", "quiet", "owners"]
        );
        assert!(
            names(&index, &channel(MEMBER & !USE_APPLICATION_COMMANDS, &mods)).is_empty(),
            "commands need Use Application Commands"
        );
        assert!(
            names(&index, &channel(MEMBER & !permission::SEND_MESSAGES, &mods)).is_empty(),
            "and to send there"
        );
        let in_thread = |permissions| Place {
            channel: "98",
            parent: Some("50"),
            ..channel(permissions, &none)
        };
        assert!(names(&index, &in_thread(MEMBER)).is_empty());
        assert_eq!(
            names(
                &index,
                &in_thread(MEMBER | permission::SEND_MESSAGES_IN_THREADS)
            ),
            ["ping", "config"],
            "a thread needs sending in threads"
        );
        let mut off = channel(MEMBER, &none);
        off.channel = "77";
        assert!(names(&index, &off).is_empty(), "the app is off in #77");
        let thread = Place {
            channel: "98",
            parent: Some("77"),
            ..off
        };
        assert!(names(&index, &thread).is_empty(), "and in its threads");
        let everywhere = Permissions {
            channels: HashMap::from([("99".to_owned(), false), ("50".to_owned(), true)]),
            ..Permissions::default()
        };
        assert_eq!(
            channel_rule(&everywhere, GUILD, &thread),
            Some(false),
            "the server's id less one is all channels"
        );
        let general = channel(MEMBER, &none);
        assert_eq!(channel_rule(&everywhere, GUILD, &general), Some(true));
        let adult = Place {
            nsfw: true,
            ..channel(MEMBER, &none)
        };
        assert_eq!(names(&index, &adult), ["ping", "config", "spicy"]);
    }

    #[test]
    fn an_apps_overrides_refuse_but_only_a_commands_allow_past_its_permissions() {
        let index = CommandIndex::parse(
            &json!({
                "applications": [{"id": "7", "name": "Status Bot", "bot_id": "7",
                                  "permissions": {"roles": {"20": true, "21": false}}}],
                "application_commands": [
                    {"id": "1", "application_id": "7", "name": "ban", "default_member_permissions": "8", "version": "1"},
                    {"id": "2", "application_id": "7", "name": "kick", "default_member_permissions": "8", "version": "1",
                     "permissions": {"roles": {"20": true}}},
                    {"id": "3", "application_id": "7", "name": "ping", "version": "1"},
                ],
            }),
            false,
        );
        let allowed = vec!["20".to_owned()];
        assert_eq!(
            names(&index, &channel(MEMBER, &allowed)),
            ["kick", "ping"],
            "the app's allow leaves /ban to its default permissions"
        );
        let denied = vec!["21".to_owned()];
        assert!(names(&index, &channel(MEMBER, &denied)).is_empty());
    }

    #[test]
    fn contexts_say_where_a_command_goes() {
        let index = CommandIndex::parse(
            &json!({
                "applications": [{"id": "7", "name": "Status Bot", "bot_id": "7"},
                                 {"id": "8", "name": "Notes", "bot_id": "8"}],
                "application_commands": [
                    {"id": "1", "application_id": "7", "name": "old", "version": "1"},
                    {"id": "2", "application_id": "7", "name": "nodm", "dm_permission": false, "version": "1"},
                    {"id": "3", "application_id": "8", "name": "botdm", "contexts": [1], "version": "1"},
                    {"id": "4", "application_id": "8", "name": "anywhere", "contexts": [0, 2], "version": "1"},
                    {"id": "5", "application_id": "7", "type": 3, "name": "Quote", "contexts": [1, 2], "version": "1"},
                ],
            }),
            false,
        );
        assert_eq!(
            names(&index, &dm(Some("7"))),
            ["old", "anywhere", "Quote"],
            "a DM with Status Bot is only its app's bot DM"
        );
        assert_eq!(
            names(&index, &dm(Some("8"))),
            ["botdm", "Quote"],
            "and a DM with Notes' bot is its bot DM"
        );
        assert_eq!(
            names(&index, &dm(None)),
            ["anywhere", "Quote"],
            "a group DM takes no command without contexts"
        );
        let none: Vec<String> = vec![];
        assert_eq!(
            names(&index, &channel(MEMBER, &none)),
            ["old", "nodm", "anywhere"]
        );
    }

    #[test]
    fn the_accounts_own_apps_are_its_own_where_installed_for_it() {
        let server = index();
        let own = CommandIndex::parse(
            &json!({"applications": [{"id": "7", "name": "Status Bot", "bot_id": "7"},
                                     {"id": "8", "name": "Notes", "flags": "0"}],
                    "application_commands": [
                        {"id": "1", "type": 1, "application_id": "7", "name": "ping", "description": "Pong",
                         "integration_types": [0, 1], "version": "1"},
                        {"id": "9", "type": 1, "application_id": "8", "name": "note", "description": "",
                         "default_member_permissions": "8", "integration_types": [1], "contexts": [0, 1, 2],
                         "version": "1"},
                        {"id": "10", "type": 1, "application_id": "8", "name": "unsure", "description": "",
                         "default_member_permissions": "8", "version": "1"}]}),
            true,
        );
        assert!(own.installed_here.is_empty());
        let merged = own.merged(&server).merged(&server);
        assert_eq!(merged.commands.len(), 9);
        assert_eq!(merged.applications.len(), 2);
        assert!(
            merged.app("7").unwrap().permissions.is_some(),
            "the app as the server has it"
        );
        assert!(
            merged.command("1").unwrap().user_installed,
            "the account's copy"
        );
        let none: Vec<String> = vec![];
        let mut off = channel(MEMBER, &none);
        off.channel = "77";
        assert_eq!(
            names(&merged, &off),
            ["ping", "note"],
            "the server's permissions don't hold back what's installed for the account"
        );
        assert_eq!(
            names(
                &merged,
                &channel(MEMBER & !permission::SEND_MESSAGES, &none)
            ),
            ["ping", "note"]
        );
        assert!(
            !names(&merged, &channel(MEMBER, &none)).contains(&"unsure"),
            "without USER_INSTALL the server's rules hold"
        );
    }

    #[test]
    fn age_restricted_commands_go_only_where_offered() {
        let index = index();
        let none: Vec<String> = vec![];
        assert!(!names(&index, &channel(permission::ALL, &none)).contains(&"spicy"));
        let mut adult = dm(Some("7"));
        adult.nsfw = true;
        assert!(names(&index, &adult).contains(&"spicy"));
    }

    fn owner(name: &str) -> Owner {
        Owner::Guild(name.into())
    }

    fn served(version: &str) -> Value {
        json!({"applications": [{"id": "7", "name": "Status Bot", "bot_id": "7"}],
               "application_commands": [], "version": version})
    }

    #[tokio::test]
    async fn an_index_is_kept_until_discords_client_would_fetch_it_again() {
        let kept = Kept::default();
        let asked = std::cell::Cell::new(0);
        let mut fetch = |_: String| {
            asked.set(asked.get() + 1);
            async { Ok(Some(served("1"))) }
        };
        kept.get(owner("1"), None, &mut fetch).await.unwrap();
        kept.get(owner("1"), None, &mut fetch).await.unwrap();
        kept.get(Owner::Channel("5".into()), Some("7"), &mut fetch)
            .await
            .unwrap();
        assert_eq!(asked.get(), 2);
        kept.mark(&Stale::Guild {
            guild: "1".into(),
            version: Some("1".into()),
        });
        kept.get(owner("1"), None, &mut fetch).await.unwrap();
        assert_eq!(asked.get(), 2, "the version it has");
        kept.mark(&Stale::Guild {
            guild: "1".into(),
            version: Some("2".into()),
        });
        kept.get(owner("1"), None, &mut fetch).await.unwrap();
        kept.get(Owner::Channel("5".into()), Some("7"), &mut fetch)
            .await
            .unwrap();
        assert_eq!(asked.get(), 4, "a newer one, and the DM with its bot");
        kept.mark(&Stale::User);
        kept.get(owner("1"), None, &mut fetch).await.unwrap();
        assert_eq!(asked.get(), 4);
        kept.mark(&Stale::All);
        kept.get(owner("1"), None, &mut fetch).await.unwrap();
        assert_eq!(asked.get(), 5);
    }

    #[tokio::test]
    async fn a_failed_index_waits_before_asking_again_and_the_oldest_goes_first() {
        let kept = Kept::default();
        let asked = std::cell::Cell::new(0);
        let mut failing = |_: String| {
            asked.set(asked.get() + 1);
            async { Err(anyhow!("refused")) }
        };
        assert!(kept.get(owner("1"), None, &mut failing).await.is_err());
        assert!(kept.get(owner("1"), None, &mut failing).await.is_err());
        assert_eq!(asked.get(), 1, "not again within {RETRY_AFTER:?}");
        let mut fetch = |_: String| async { Ok(Some(served("1"))) };
        let kept = Kept::default();
        for guild in 0..KEPT {
            kept.get(owner(&format!("g{guild}")), None, &mut fetch)
                .await
                .unwrap();
        }
        // The first used again, then one more: the second goes.
        kept.get(owner("g0"), None, &mut fetch).await.unwrap();
        kept.get(owner("new"), None, &mut fetch).await.unwrap();
        let slots = kept.slots();
        assert_eq!(slots.len(), KEPT);
        assert!(slots.iter().any(|slot| slot.owner == owner("g0")));
        assert!(!slots.iter().any(|slot| slot.owner == owner("g1")));
    }
}
