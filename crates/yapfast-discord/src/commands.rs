//! Apps' commands, as Discord's application command indexes list them: a
//! server's, a DM's or group DM's, and those of the apps this account
//! installed for itself. See docs/discord/messages.md, "Apps' commands", and
//! <https://docs.discord.food/interactions/application-commands#application-command-index-object>.
//!
//! An index is fetched only once the person types `/` (see
//! [`crate::service::Discord::command_index`]), and kept until Discord says
//! it changed: GUILD_APPLICATION_COMMAND_INDEX_UPDATE for a server's, and
//! APPLICATION_COMMAND_PERMISSIONS_UPDATE for all of them. Nothing here asks
//! ahead or again on its own.

use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
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

/// `USER_INSTALL` among a command's `integration_types`.
const USER_INSTALL: u8 = 1;

/// The most indexes kept: a few servers' and DMs' besides this account's.
const KEPT: usize = 16;

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
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Permissions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<bool>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub roles: HashMap<String, bool>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub channels: HashMap<String, bool>,
}

/// A command as an index gives it. What Yapfast doesn't read stays in
/// `other`, so [`Command::to_json`] gives back the object Discord sent, as
/// `POST /interactions` takes it in `data.application_command`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub id: String,
    /// [`CHAT_INPUT`], [`USER`], [`MESSAGE`] or [`PRIMARY_ENTRY_POINT`].
    #[serde(rename = "type", default = "chat_input")]
    pub kind: u8,
    pub application_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
    /// In Discord's locale: the index sends `name_localized` as `name`.
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<CommandOption>,
    /// The permissions, as a decimal string, a member needs to use it in a
    /// server; `"0"` leaves it to administrators.
    #[serde(default)]
    pub default_member_permissions: Option<String>,
    /// Where it may be used; none for anywhere its app may be.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contexts: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration_types: Option<Vec<u8>>,
    /// Age-restricted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nsfw: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
    #[serde(default)]
    pub version: String,
    /// From this account's own index, `/users/@me`, rather than the place's:
    /// a server's permissions don't apply to it.
    #[serde(skip)]
    pub user_installed: bool,
    #[serde(flatten)]
    pub other: Map<String, Value>,
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Choice>,
    /// A subcommand's options, or a group's subcommands.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<CommandOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_types: Option<Vec<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_value: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_value: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub autocomplete: bool,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// One of the values an option takes, by the name it shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub name: String,
    /// A string, an integer or a number, as the option's kind.
    pub value: Value,
    #[serde(flatten)]
    pub other: Map<String, Value>,
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
        serde_json::to_value(self).unwrap_or_default()
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
    pub context: Context,
    /// This account's permissions in the channel; [`permission::ALL`] for
    /// an owner or administrator.
    pub permissions: u64,
    /// This account's roles in the server.
    pub roles: &'a [String],
    /// The channel is age-restricted.
    pub nsfw: bool,
}

impl CommandIndex {
    /// An index as `GET .../application-command-index` gives it, with
    /// `user_installed` for `/users/@me`'s. A command or app Yapfast can't
    /// read is left out.
    pub fn parse(value: &Value, user_installed: bool) -> Self {
        let each = |key: &str| value[key].as_array().into_iter().flatten();
        let applications = each("applications")
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
            .filter_map(|command| {
                Command::deserialize(command)
                    .inspect_err(|error| tracing::debug!(%error, "unreadable command in an index"))
                    .ok()
            })
            .filter(|command| {
                // The user index lists the commands of apps installed for
                // this account; only those installed so apply.
                !user_installed
                    || command
                        .integration_types
                        .as_ref()
                        .is_none_or(|types| types.contains(&USER_INSTALL))
            })
            .map(|mut command| {
                command.user_installed = user_installed;
                Arc::new(command)
            })
            .collect();
        Self {
            applications,
            commands,
        }
    }

    /// `self`'s apps and commands, then those of `more` it doesn't have,
    /// sharing each.
    pub fn merged(&self, more: &CommandIndex) -> CommandIndex {
        let mut merged = self.clone();
        for app in &more.applications {
            if !merged.applications.iter().any(|kept| kept.id == app.id) {
                merged.applications.push(app.clone());
            }
        }
        for command in &more.commands {
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

    /// Whether a DM with `person` is an app's [`Context::BotDm`]: one of
    /// the apps is theirs.
    pub fn is_bot(&self, person: &str) -> bool {
        self.applications
            .iter()
            .any(|app| app.bot_id.as_deref() == Some(person))
    }

    /// The commands this account may use at `place`, in the index's order:
    /// those whose `contexts` take it, age-restricted ones only in an
    /// age-restricted channel, and in a server those its permissions allow.
    pub fn usable<'a>(&'a self, place: &'a Place<'a>) -> impl Iterator<Item = &'a Arc<Command>> {
        self.commands
            .iter()
            .filter(move |command| self.may_use(command, place))
    }

    fn may_use(&self, command: &Command, place: &Place) -> bool {
        if let Some(contexts) = &command.contexts
            && !contexts.contains(&(place.context as u8))
        {
            return false;
        }
        if command.nsfw && !place.nsfw {
            return false;
        }
        let Some(guild) = place.guild else {
            return true;
        };
        if command.user_installed {
            return true;
        }
        if command.guild_id.as_deref().is_some_and(|its| its != guild) {
            return false;
        }
        // Administrators may use every command.
        if place.permissions == permission::ALL {
            return true;
        }
        if place.permissions & USE_APPLICATION_COMMANDS == 0 {
            return false;
        }
        let app = self
            .app(&command.application_id)
            .and_then(|app| app.permissions.as_ref());
        let own = command.permissions.as_ref();
        let channel = own
            .and_then(|own| channel_rule(own, guild, place))
            .or_else(|| app.and_then(|app| channel_rule(app, guild, place)));
        if channel == Some(false) {
            return false;
        }
        let member = own
            .and_then(|own| member_rule(own, guild, place))
            .or_else(|| app.and_then(|app| member_rule(app, guild, place)));
        if let Some(allowed) = member {
            return allowed;
        }
        match &command.default_member_permissions {
            None => true,
            Some(needed) => needed
                .parse::<u64>()
                .is_ok_and(|needed| place.permissions & needed == needed),
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

/// The indexes fetched, until Discord says they changed. Shared by the
/// clones of [`crate::service::Discord`].
#[derive(Clone, Default)]
pub struct Kept(Arc<Mutex<Vec<KeptIndex>>>);

/// An index kept, and whose it is.
type KeptIndex = (Owner, Arc<CommandIndex>);

impl Kept {
    /// Keeps indexes, forgetting each once the gateway says it changed.
    pub fn watching(mut live: broadcast::Receiver<Live>) -> Self {
        let kept = Self::default();
        let watched = kept.clone();
        tokio::spawn(async move {
            loop {
                let forget = match live.recv().await {
                    Ok(Live::CommandIndexChanged { guild }) => guild,
                    Ok(_) => continue,
                    // A change may have gone by unseen.
                    Err(broadcast::error::RecvError::Lagged(_)) => None,
                    Err(broadcast::error::RecvError::Closed) => return,
                };
                watched.forget(forget.as_deref()).await;
            }
        });
        kept
    }

    /// Forgets `guild`'s index, or with none every index.
    pub async fn forget(&self, guild: Option<&str>) {
        let mut kept = self.0.lock().await;
        match guild {
            Some(guild) => {
                kept.retain(|(owner, _)| !matches!(owner, Owner::Guild(id) if id == guild))
            }
            None => kept.clear(),
        }
    }

    /// `owner`'s index, kept or fetched with `fetch`. Fetches wait for one
    /// another, so one index is fetched once however many ask.
    pub async fn get<F>(&self, owner: Owner, fetch: F) -> anyhow::Result<Arc<CommandIndex>>
    where
        F: AsyncFnOnce(&Owner) -> anyhow::Result<Value>,
    {
        let mut kept = self.0.lock().await;
        if let Some((_, index)) = kept.iter().find(|(kept, _)| *kept == owner) {
            return Ok(index.clone());
        }
        let value = fetch(&owner).await?;
        let index = Arc::new(CommandIndex::parse(&value, owner == Owner::User));
        if kept.len() >= KEPT {
            // The oldest kept, other than this account's own.
            if let Some(at) = kept.iter().position(|(owner, _)| *owner != Owner::User) {
                kept.remove(at);
            }
        }
        kept.push((owner, index.clone()));
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const GUILD: &str = "100";

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
                    {"name": "broken"},
                ],
                "version": "1"
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
            context: Context::Guild,
            permissions,
            roles,
            nsfw: false,
        }
    }

    #[test]
    fn a_command_goes_back_to_discord_as_the_index_gave_it() {
        let index = index();
        assert_eq!(index.commands.len(), 6, "the unreadable one is left out");
        let ping = index.command("1").unwrap();
        assert_eq!(ping.to_json()["name_default"], "ping");
        assert_eq!(ping.to_json()["type"], 1);
        let config = index.command("3").unwrap();
        let variants: Vec<_> = config.variants().iter().map(Variant::name).collect();
        assert_eq!(variants, ["config get", "config role set"]);
        assert_eq!(config.variants()[1].path, ["role", "set"]);
        assert_eq!(config.variants()[1].options[0].kind, option::ROLE);
    }

    #[test]
    fn a_servers_commands_follow_its_permissions() {
        let index = index();
        let member = USE_APPLICATION_COMMANDS | permission::SEND_MESSAGES;
        let none: Vec<String> = vec![];
        assert_eq!(names(&index, &channel(member, &none)), ["ping", "config"]);
        let mods = vec!["20".to_owned()];
        assert_eq!(
            names(&index, &channel(member, &mods)),
            ["ping", "config", "quiet"]
        );
        assert_eq!(
            names(&index, &channel(permission::ALL, &none)),
            ["ping", "admin", "config", "quiet"],
            "an administrator may use all but the age-restricted"
        );
        assert!(names(&index, &channel(permission::SEND_MESSAGES, &mods)).is_empty());
        let mut off = channel(member, &none);
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
        let general = channel(member, &none);
        assert_eq!(channel_rule(&everywhere, GUILD, &general), Some(true));
        let adult = Place {
            nsfw: true,
            ..channel(member, &none)
        };
        assert_eq!(names(&index, &adult), ["ping", "config", "spicy"]);
    }

    #[test]
    fn contexts_and_installs_say_where_a_command_goes() {
        let index = index();
        let dm = Place {
            guild: None,
            channel: "60",
            parent: None,
            context: Context::BotDm,
            permissions: permission::ALL,
            roles: &[],
            nsfw: false,
        };
        assert_eq!(
            names(&index, &dm),
            ["ping", "admin", "config", "quiet", "Quote"]
        );
        let none: Vec<String> = vec![];
        let guild = channel(USE_APPLICATION_COMMANDS, &none);
        assert!(!names(&index, &guild).contains(&"Quote"));
        let user = CommandIndex::parse(
            &json!({"applications": [{"id": "8", "name": "Notes", "flags": "0"}],
                    "application_commands": [
                        {"id": "9", "type": 1, "application_id": "8", "name": "note", "description": "",
                         "default_member_permissions": "8", "integration_types": [1], "contexts": [0, 1, 2],
                         "version": "1"},
                        {"id": "10", "type": 1, "application_id": "8", "name": "guildonly", "description": "",
                         "default_member_permissions": null, "integration_types": [0], "version": "1"}]}),
            true,
        );
        assert_eq!(
            user.commands.len(),
            1,
            "only what's installed for the account"
        );
        let merged = index.merged(&user).merged(&user);
        assert_eq!(merged.commands.len(), 7);
        assert_eq!(merged.applications.len(), 2);
        assert!(
            names(&merged, &guild).contains(&"note"),
            "a server's permissions don't hold back the account's own apps"
        );
        assert!(index.is_bot("7") && !index.is_bot("8"));
    }
}
