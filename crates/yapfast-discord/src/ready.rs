//! The parts of Discord's gateway payloads Yapfast reads. User accounts get a
//! different shape from bots, and capability bits change it further, so read
//! fields defensively.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use serde_json::Value;

use crate::{
    client,
    forum::{self, Forum, GUILD_FORUM},
    notifications::{Level, Mute},
};

/// Discord serves pictures in powers of two; this covers the 80px avatar in a
/// call. Larger ones would cost memory for every person shown.
const PICTURE_SIZE: u32 = 128;

const GUILD_TEXT: u64 = 0;
const DM: u64 = 1;
const GUILD_VOICE: u64 = 2;
const GROUP_DM: u64 = 3;
const GUILD_CATEGORY: u64 = 4;
const GUILD_ANNOUNCEMENT: u64 = 5;
/// Thread channel types.
pub const ANNOUNCEMENT_THREAD: u64 = 10;
pub const PUBLIC_THREAD: u64 = 11;
pub const PRIVATE_THREAD: u64 = 12;
const GUILD_STAGE_VOICE: u64 = 13;

/// Discord's permission bits, as roles and channel overwrites carry them;
/// see <https://docs.discord.food/topics/permissions>.
pub mod permission {
    pub const CREATE_INSTANT_INVITE: u64 = 1 << 0;
    pub const KICK_MEMBERS: u64 = 1 << 1;
    pub const BAN_MEMBERS: u64 = 1 << 2;
    pub const ADMINISTRATOR: u64 = 1 << 3;
    pub const MANAGE_CHANNELS: u64 = 1 << 4;
    pub const MANAGE_GUILD: u64 = 1 << 5;
    pub const VIEW_AUDIT_LOG: u64 = 1 << 7;
    /// Deleting others' messages and removing their reactions.
    pub const MANAGE_MESSAGES: u64 = 1 << 13;
    pub const VIEW_CHANNEL: u64 = 1 << 10;
    pub const SEND_MESSAGES: u64 = 1 << 11;
    pub const READ_MESSAGE_HISTORY: u64 = 1 << 16;
    pub const MENTION_EVERYONE: u64 = 1 << 17;
    pub const VIEW_GUILD_ANALYTICS: u64 = 1 << 19;
    pub const CONNECT: u64 = 1 << 20;
    /// Muting, deafening and moving members in voice channels; moving
    /// takes them out of voice too.
    pub const MUTE_MEMBERS: u64 = 1 << 22;
    pub const DEAFEN_MEMBERS: u64 = 1 << 23;
    pub const MOVE_MEMBERS: u64 = 1 << 24;
    pub const CHANGE_NICKNAME: u64 = 1 << 26;
    pub const MANAGE_NICKNAMES: u64 = 1 << 27;
    pub const MANAGE_ROLES: u64 = 1 << 28;
    pub const MANAGE_WEBHOOKS: u64 = 1 << 29;
    /// Changing and deleting any of a server's emoji, stickers and sounds.
    pub const MANAGE_GUILD_EXPRESSIONS: u64 = 1 << 30;
    /// Seeing every private thread, and sending in locked ones.
    pub const MANAGE_THREADS: u64 = 1 << 34;
    pub const CREATE_PUBLIC_THREADS: u64 = 1 << 35;
    pub const CREATE_PRIVATE_THREADS: u64 = 1 << 36;
    pub const SEND_MESSAGES_IN_THREADS: u64 = 1 << 38;
    /// Timing members out.
    pub const MODERATE_MEMBERS: u64 = 1 << 40;
    /// Adding emoji, stickers and sounds, and changing one's own.
    pub const CREATE_GUILD_EXPRESSIONS: u64 = 1 << 43;
    /// Posting polls, besides sending messages.
    pub const SEND_POLLS: u64 = 1 << 49;
    /// Pinning messages and unpinning them, which `MANAGE_MESSAGES` once gave.
    pub const PIN_MESSAGES: u64 = 1 << 51;
    /// Sending without waiting out a channel's slowmode, which
    /// `MANAGE_MESSAGES` and `MANAGE_CHANNELS` give too.
    pub const BYPASS_SLOWMODE: u64 = 1 << 52;
    /// Every permission, as the owner and administrators hold.
    pub const ALL: u64 = u64::MAX;
    /// Any of these opens a server's settings, as Discord's client has it
    /// in `canAccessGuildSettings`.
    pub const SETTINGS: u64 = MANAGE_GUILD
        | MANAGE_ROLES
        | ADMINISTRATOR
        | BAN_MEMBERS
        | MANAGE_NICKNAMES
        | CREATE_GUILD_EXPRESSIONS
        | MANAGE_GUILD_EXPRESSIONS
        | MANAGE_WEBHOOKS
        | VIEW_AUDIT_LOG
        | VIEW_GUILD_ANALYTICS;
}

use permission::{
    ADMINISTRATOR, CONNECT, MANAGE_THREADS, MENTION_EVERYONE, SEND_MESSAGES,
    SEND_MESSAGES_IN_THREADS, VIEW_CHANNEL,
};

/// A member's timeout: until it ends, Discord leaves them only viewing
/// channels and reading their history, unless they own the server or are
/// administrators.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Timeout {
    pub user: String,
    /// When it ends, in Unix milliseconds.
    pub until: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    /// The address of the server's icon, if it has one.
    pub icon: Option<String>,
    /// The address of its banner, if it has one.
    pub banner: Option<String>,
    /// How many members it has, as of READY or GUILD_CREATE.
    pub member_count: Option<u64>,
    /// Text and announcement channels, in Discord's order.
    pub text_channels: Vec<Channel>,
    pub voice_channels: Vec<Channel>,
    /// The threads Yapfast knows of in its text and announcement channels,
    /// the newest first: those this account joined that are still active,
    /// which Discord sends, and others it opened. Each has
    /// [`Channel::thread`], and its channel for [`Channel::parent`].
    #[serde(default)]
    pub threads: Vec<Channel>,
    // The categories, roles and emoji are shared, as the channels' overwrites
    // are: a change to the server copies it while a snapshot holds it, and
    // they're most of a large server.
    /// Categories by id, which group and order the channels in them.
    pub categories: Arc<HashMap<String, Category>>,
    pub owner_id: Option<String>,
    /// Each role's permission bits, by role id. The @everyone role has the
    /// server's id.
    pub roles: Arc<HashMap<String, u64>>,
    /// Role names by id, for mentions.
    pub role_names: Arc<HashMap<String, String>>,
    /// The position and RGB colour of each coloured role, by id.
    pub role_colors: Arc<HashMap<String, (i64, u32)>>,
    /// Each role's position, by id: higher roles list first.
    pub role_positions: Arc<HashMap<String, i64>>,
    /// Roles anyone may mention, by id.
    pub mentionable_roles: Arc<HashSet<String>>,
    /// Roles an integration such as a bot or boosting manages, by id: nobody
    /// grants or takes them by hand.
    #[serde(default)]
    pub managed_roles: Arc<HashSet<String>>,
    /// Which messages notify its members unless they choose: Discord's
    /// `default_message_notifications`.
    #[serde(default)]
    pub default_level: Level,
    /// The server turned forwarding its messages off, a feature it lists
    /// as `FORWARDING_DISABLED`.
    #[serde(default)]
    pub forwarding_disabled: bool,
    /// Verified or Partnered, which Discord's client won't give away.
    #[serde(default)]
    pub verified: bool,
    /// A Community server, where the channel list may list only the
    /// channels this account follows; see
    /// [`crate::marked::MarkedServer::follows_only`].
    #[serde(default)]
    pub community: bool,
    /// This account's timeout here while it lasts, set on a copy of the
    /// server when the service publishes it; see [`crate::marked`] for the
    /// rest of what's this account's.
    #[serde(default)]
    pub my_timeout: Option<Timeout>,
    /// Its boost level, from 0 to 3, which raises everyone's upload limit
    /// in it; see [`crate::api::upload_limit`].
    pub premium_tier: u8,
    /// 1 when its moderators need two-factor authentication, which its
    /// owner then needs to delete it or give it away.
    #[serde(default)]
    pub mfa_level: u8,
    /// Its custom emoji, by name.
    pub emojis: Arc<Vec<Emoji>>,
    /// When this account joined it, as Discord's ISO 8601 time.
    pub joined_at: Option<String>,
    /// Out of reach in a Discord outage: still listed, with what was last
    /// known of it, until Discord sends it again. Not saved, since the next
    /// start learns afresh.
    #[serde(skip)]
    pub unavailable: bool,
}

/// A server's custom emoji.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Emoji {
    pub id: String,
    pub name: String,
    pub animated: bool,
    /// Usable: a server that loses boosts keeps emoji it no longer has room for,
    /// unavailable.
    pub available: bool,
}

/// A server's emoji list, from a guild object or GUILD_EMOJIS_UPDATE.
pub fn emojis(list: &Value) -> Vec<Emoji> {
    let mut emojis: Vec<Emoji> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|emoji| {
            Some(Emoji {
                id: emoji["id"].as_str()?.to_owned(),
                name: emoji["name"].as_str()?.to_owned(),
                animated: emoji["animated"].as_bool().unwrap_or(false),
                available: emoji["available"].as_bool().unwrap_or(true),
            })
        })
        .collect();
    emojis.sort_by_key(|emoji| emoji.name.to_lowercase());
    emojis
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub position: i64,
    /// The category the channel sits in.
    pub parent: Option<String>,
    /// What the channel is for, in Discord's markup, as its header shows
    /// it; shared by the server's copies.
    #[serde(default)]
    pub topic: Option<Arc<str>>,
    pub overwrites: Arc<Vec<Overwrite>>,
    /// What a thread has besides; none for a channel.
    #[serde(default)]
    pub thread: Option<Box<Thread>>,
    /// An announcement channel, whose threads are announcement threads and
    /// never private.
    #[serde(default)]
    pub announcement: bool,
    /// How many minutes of quiet archive a thread started here, as the
    /// channel's `default_auto_archive_duration` says; 0 when it doesn't.
    #[serde(default)]
    pub archive_after: u16,
    /// What a forum has besides, whose messages are its posts' own; none
    /// for another channel.
    #[serde(default)]
    pub forum: Option<Box<Forum>>,
    /// How many seconds each member waits between messages here, as
    /// `rate_limit_per_user` says; 0 without slowmode.
    #[serde(default)]
    pub slowmode: u32,
}

/// What a thread has that a channel doesn't; see
/// <https://docs.discord.food/topics/threads>.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Thread {
    /// Who started it.
    pub owner: Option<String>,
    /// Only those in it, and those who may manage threads, see it.
    pub private: bool,
    /// Closed for inactivity or by hand: Discord stops listing it until a
    /// message opens it again.
    pub archived: bool,
    /// Only those who may manage threads write in it or open it again.
    pub locked: bool,
    /// This account is a member, so it lists under its channel.
    pub joined: bool,
    /// This account's mute in it, from its thread member.
    pub mute: Mute,
    /// Which of its messages notify this account, from its thread member's
    /// flags; none for its channel's.
    pub level: Option<Level>,
    /// Its messages after the first, as Discord counts them.
    pub messages: u64,
    /// The snowflake of its newest message, or 0 before it has one.
    #[serde(default)]
    pub last_message: u64,
    /// How many minutes of quiet archive it, from its `auto_archive_duration`;
    /// 0 when Discord didn't say.
    #[serde(default)]
    pub archive_after: u16,
    /// When it was last archived or opened again, in Unix milliseconds, from
    /// its `archive_timestamp`; 0 when Discord didn't say.
    #[serde(default)]
    pub archive_changed: u64,
    /// How many are in it, as Discord counts them: up to 50.
    #[serde(default)]
    pub members: u32,
    /// A few of those in it, by user id, as Discord picks them: the first
    /// [`MEMBER_PREVIEW`], which a list of threads shows.
    #[serde(default)]
    pub member_preview: Vec<String>,
    /// A forum post's tags, by id, and whether it's pinned to the top.
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub pinned: bool,
    /// Its newest message, as a list of threads shows it, while Yapfast knows
    /// it: Discord sends it with a server's active threads, and Yapfast follows
    /// the messages that come after.
    #[serde(skip)]
    pub recent: Option<Arc<Recent>>,
}

/// A thread's newest message, as a list of threads shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recent {
    /// Its snowflake.
    pub id: u64,
    /// Its author, by user id.
    pub author: String,
    /// Its text in Discord's markup, cut to [`RECENT_TEXT`] bytes.
    pub content: String,
}

/// How much of a thread's newest message Yapfast keeps: more than a list's
/// row shows on one line.
pub const RECENT_TEXT: usize = 200;

/// How many of a thread's members Yapfast keeps, as a list of threads shows
/// their avatars.
pub const MEMBER_PREVIEW: usize = 3;

/// The first [`MEMBER_PREVIEW`] ids of a `member_ids_preview`.
pub fn member_preview(ids: &Value) -> Option<Vec<String>> {
    Some(
        ids.as_array()?
            .iter()
            .filter_map(Value::as_str)
            .take(MEMBER_PREVIEW)
            .map(str::to_owned)
            .collect(),
    )
}

impl Recent {
    /// A message object as a thread's newest message; none for one without
    /// an author.
    pub fn of(message: &Value) -> Option<Self> {
        let mut content = message["content"].as_str().unwrap_or_default().to_owned();
        if content.len() > RECENT_TEXT {
            let mut end = RECENT_TEXT;
            while !content.is_char_boundary(end) {
                end -= 1;
            }
            content.truncate(end);
        }
        Some(Self {
            id: snowflake(message["id"].as_str()?),
            author: message["author"]["id"].as_str()?.to_owned(),
            content,
        })
    }
}

/// A snowflake as a number; 0 for none.
fn snowflake(id: &str) -> u64 {
    id.parse().unwrap_or_default()
}

/// Thread member flags that choose its notifications.
const THREAD_ALL_MESSAGES: u64 = 1 << 1;
const THREAD_ONLY_MENTIONS: u64 = 1 << 2;
const THREAD_NO_MESSAGES: u64 = 1 << 3;

impl Thread {
    /// Takes this account's thread member object: it's in the thread, and
    /// these are its settings there.
    pub fn join(&mut self, member: &Value) {
        self.joined = true;
        self.mute = Mute::read(member);
        let flags = member["flags"].as_u64().unwrap_or_default();
        self.level = if flags & THREAD_ALL_MESSAGES != 0 {
            Some(Level::All)
        } else if flags & THREAD_ONLY_MENTIONS != 0 {
            Some(Level::Mentions)
        } else if flags & THREAD_NO_MESSAGES != 0 {
            Some(Level::Nothing)
        } else {
            None
        };
    }

    /// This account left it, or was taken out.
    pub fn leave(&mut self) {
        self.joined = false;
        self.mute = Mute::default();
        self.level = None;
    }

    /// Takes in a message sent in it, which is its newest.
    pub fn sent(&mut self, message: &Value) {
        let Some(recent) = Recent::of(message).filter(|recent| recent.id >= self.last_message)
        else {
            return;
        };
        self.last_message = recent.id;
        self.recent = Some(Arc::new(recent));
    }

    /// When the thread `id` goes quiet, in Unix milliseconds: its
    /// [`Thread::archive_after`] after its newest message, its start or its
    /// last archiving or opening, whichever came last. Discord's client
    /// stops listing a thread under its channel then, unless it's unread,
    /// mentions this account or is on screen, rather than wait for Discord
    /// to archive it. Never for a thread without an archive duration.
    pub fn quiet_at(&self, id: &str) -> u64 {
        if self.archive_after == 0 {
            return u64::MAX;
        }
        let newest = match self.last_message {
            0 => snowflake(id),
            last => last,
        };
        let newest = (newest >> 22) + crate::message::DISCORD_EPOCH_MS;
        newest.max(self.archive_changed) + u64::from(self.archive_after) * 60_000
    }
}

/// Where a channel or category goes in its server's list: its position,
/// and for a channel moving to another category, that category, none for
/// none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub id: String,
    pub position: i64,
    pub parent: Option<Option<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Category {
    pub name: String,
    pub position: i64,
    /// Who may see and manage it, as a channel's overwrites say.
    #[serde(default)]
    pub overwrites: Arc<Vec<Overwrite>>,
}

/// What an account may do in a channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Access {
    /// Connect to a voice channel, or send in a text channel.
    #[default]
    Join,
    /// Visible but closed: a voice channel shows a lock, as in Discord, and a
    /// text channel can only be read.
    Locked,
    /// Hidden from this account by the server's permissions.
    Hidden,
}

/// Whose messages an account may delete in a channel, as Discord's client
/// offers it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Deletes {
    /// Its own, as in a DM.
    #[default]
    Own,
    /// Anyone's, with Manage Messages.
    Any,
    /// None: the channel is an archived thread it may not open again.
    None,
}

impl Deletes {
    /// Whether `me` may delete `message`: it's theirs, or answers their
    /// command, unless they may delete anyone's; and Discord lets it be
    /// deleted at all.
    pub fn allow(self, message: &crate::message::Message, me: &str) -> bool {
        !message.undeletable
            && match self {
                Deletes::Any => true,
                Deletes::Own => message.author_id == me || message.invoker.as_deref() == Some(me),
                Deletes::None => false,
            }
    }
}

/// Where DMs and group DMs sit in place of a server, as in Discord's links.
pub const PRIVATE: &str = "@me";

/// A DM or group DM.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Private {
    pub id: String,
    pub group: bool,
    /// A group DM's own name, if it was given one.
    pub name: Option<String>,
    /// The address of a group DM's icon, if it has one.
    pub icon: Option<String>,
    /// Everyone in it besides this account.
    pub recipients: Vec<String>,
    /// A message request: someone outside this account's DMs wrote, and it
    /// waits to be accepted or ignored.
    pub request: bool,
    /// A request Discord thinks is spam.
    pub spam: bool,
}

/// A DM or group DM, or a message request, from a channel object.
pub fn private_channel(channel: &Value) -> Option<Private> {
    let group = match channel["type"].as_u64() {
        Some(DM) => false,
        Some(GROUP_DM) => true,
        _ => return None,
    };
    let spam = channel["is_spam"] == true;
    let id = channel["id"].as_str()?.to_owned();
    // With deduplicated users, READY gives ids and lists the users apart.
    let recipients = match channel["recipient_ids"].as_array() {
        Some(ids) => ids
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        None => channel["recipients"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|user| user["id"].as_str().map(str::to_owned))
            .collect(),
    };
    Some(Private {
        group,
        name: channel["name"]
            .as_str()
            .filter(|name| !name.is_empty())
            .map(str::to_owned),
        icon: channel["icon"].as_str().map(|hash| {
            format!(
                "{}/channel-icons/{id}/{hash}.png?size={PICTURE_SIZE}",
                cdn()
            )
        }),
        recipients,
        request: spam || channel["is_message_request"] == true,
        spam,
        id,
    })
}

/// A channel's change to the permissions of one role or member.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Overwrite {
    pub id: String,
    pub member: bool,
    pub allow: u64,
    pub deny: u64,
}

impl Server {
    /// Who can read `channel`, as Discord's id for its member list, which
    /// channels with the same readers share: `everyone` when @everyone sees
    /// it and no overwrite hides it, else a hash of the overwrites that allow
    /// or deny seeing it. Discord's client works it out the same way.
    pub fn readers(&self, channel: &str) -> String {
        let Some(mut channel) = self.channel(channel) else {
            return format!("channel:{channel}");
        };
        // A thread lists its channel's readers.
        if let Some(parent) = channel.thread.as_ref().and_then(|_| self.parent(channel)) {
            channel = parent;
        }
        // Without the server's roles, @everyone is taken to see channels,
        // as it does on most servers.
        let everyone_sees = self
            .roles
            .get(&self.id)
            .is_none_or(|permissions| permissions & VIEW_CHANNEL != 0);
        let hidden = channel
            .overwrites
            .iter()
            .any(|overwrite| overwrite.deny & VIEW_CHANNEL != 0);
        if everyone_sees && !hidden {
            return "everyone".into();
        }
        let mut readers: Vec<String> = channel
            .overwrites
            .iter()
            .filter_map(|overwrite| {
                if overwrite.allow & VIEW_CHANNEL != 0 {
                    Some(format!("allow:{}", overwrite.id))
                } else if overwrite.deny & VIEW_CHANNEL != 0 {
                    Some(format!("deny:{}", overwrite.id))
                } else {
                    None
                }
            })
            .collect();
        readers.sort();
        murmur3(readers.join(",").as_bytes()).to_string()
    }

    /// Whether `user`, holding `roles`, can see `channel` and connect to it,
    /// or send in it for a text channel. Computed as Discord does: the roles'
    /// permissions, then the channel's overwrites for @everyone, for the roles,
    /// and for the member.
    pub fn access(&self, channel: &Channel, user: &str, roles: &[String]) -> Access {
        // Without the server's roles there is nothing to decide with.
        if self.roles.is_empty() {
            return Access::Join;
        }
        let permissions = self.channel_permissions(channel, user, roles);
        if let Some(thread) = &channel.thread {
            let manages = permissions & MANAGE_THREADS != 0;
            return if permissions & VIEW_CHANNEL == 0
                || thread.private && !thread.joined && !manages
            {
                Access::Hidden
            } else if permissions & SEND_MESSAGES_IN_THREADS == 0 || thread.locked && !manages {
                Access::Locked
            } else {
                Access::Join
            };
        }
        let act = if self.text_channels.iter().any(|text| text.id == channel.id) {
            SEND_MESSAGES
        } else {
            CONNECT
        };
        if permissions & VIEW_CHANNEL == 0 {
            Access::Hidden
        } else if permissions & act == 0 {
            Access::Locked
        } else {
            Access::Join
        }
    }

    /// Whether `user`, holding `roles`, may send in `channel`'s text. A voice
    /// channel's chat asks this apart from [`Self::access`], which is about
    /// connecting to it.
    pub fn may_send(&self, channel: &Channel, user: &str, roles: &[String]) -> bool {
        self.roles.is_empty() || self.channel_permissions(channel, user, roles) & SEND_MESSAGES != 0
    }

    /// Whether `user`, holding `roles`, waits out `channel`'s slowmode
    /// between messages, as those who may bypass it don't.
    pub fn slowed(&self, channel: &Channel, user: &str, roles: &[String]) -> bool {
        use permission::{BYPASS_SLOWMODE, MANAGE_CHANNELS, MANAGE_MESSAGES};
        channel.slowmode > 0
            && self.channel_permissions(channel, user, roles)
                & (BYPASS_SLOWMODE | MANAGE_MESSAGES | MANAGE_CHANNELS)
                == 0
    }

    /// Whether `user`, holding `roles`, may forward `channel`'s messages, as
    /// Discord's client allows: the server lets its messages be forwarded,
    /// and they may read the channel's history. Without the server's roles,
    /// only the first is known.
    pub fn forwards_from(&self, channel: &Channel, user: &str, roles: &[String]) -> bool {
        !self.forwarding_disabled
            && (self.roles.is_empty()
                || self.channel_permissions(channel, user, roles)
                    & permission::READ_MESSAGE_HISTORY
                    != 0)
    }

    /// Whose messages `user`, holding `roles`, may delete in `channel`, as
    /// Discord's client decides: anyone's with Manage Messages, and none in
    /// an archived thread that's locked, or that they couldn't open again
    /// by sending in it. Without the server's roles, their own.
    pub fn deletes(&self, channel: &Channel, user: &str, roles: &[String]) -> Deletes {
        let permissions = self.channel_permissions(channel, user, roles);
        let reopens = SEND_MESSAGES | SEND_MESSAGES_IN_THREADS;
        match &channel.thread {
            Some(thread)
                if thread.archived
                    && (thread.locked
                        || !self.roles.is_empty() && permissions & reopens != reopens) =>
            {
                Deletes::None
            }
            _ if permissions & permission::MANAGE_MESSAGES != 0 => Deletes::Any,
            _ => Deletes::Own,
        }
    }

    /// Adds or replaces a channel or thread from a channel object.
    pub fn put_channel(&mut self, channel: &Value) {
        let Some(id) = channel["id"].as_str() else {
            return;
        };
        if is_thread(channel) {
            self.put_thread(channel);
            return;
        }
        self.forget(id);
        self.add_channel(channel);
        self.sort_channels();
    }

    /// Adds a channel or category from a channel object, where the lists
    /// end: one not known yet, for [`Self::sort_channels`] to place.
    fn add_channel(&mut self, channel: &Value) {
        let Some(id) = channel["id"].as_str() else {
            return;
        };
        match channel["type"].as_u64() {
            // A forum lists among the text channels, as in Discord's client.
            Some(GUILD_TEXT | GUILD_ANNOUNCEMENT | GUILD_FORUM) => {
                if let Some(channel) = parse_channel(channel) {
                    self.text_channels.push(channel);
                }
            }
            Some(GUILD_VOICE | GUILD_STAGE_VOICE) => {
                if let Some(channel) = parse_channel(channel) {
                    self.voice_channels.push(channel);
                }
            }
            Some(GUILD_CATEGORY) => {
                Arc::make_mut(&mut self.categories).insert(
                    id.to_owned(),
                    Category {
                        name: channel["name"].as_str().unwrap_or_default().to_owned(),
                        position: channel["position"].as_i64().unwrap_or_default(),
                        overwrites: Arc::new(overwrites(channel)),
                    },
                );
            }
            _ => {}
        }
    }

    /// Puts channels and categories where `placements` say, as the
    /// CHANNEL_UPDATE for each would, and gives where they were.
    pub fn place(&mut self, placements: &[Placement]) -> Vec<Placement> {
        let mut was = Vec::with_capacity(placements.len());
        for placement in placements {
            let id = placement.id.as_str();
            if let Some(channel) = self
                .text_channels
                .iter_mut()
                .chain(&mut self.voice_channels)
                .find(|channel| channel.id == id)
            {
                was.push(Placement {
                    id: id.to_owned(),
                    position: std::mem::replace(&mut channel.position, placement.position),
                    parent: placement
                        .parent
                        .clone()
                        .map(|parent| std::mem::replace(&mut channel.parent, parent)),
                });
            } else if self.categories.contains_key(id)
                && let Some(category) = Arc::make_mut(&mut self.categories).get_mut(id)
            {
                was.push(Placement {
                    id: id.to_owned(),
                    position: std::mem::replace(&mut category.position, placement.position),
                    parent: None,
                });
            }
        }
        self.sort_channels();
        was
    }

    /// Removes a deleted channel, category or thread, and a channel's
    /// threads.
    pub fn remove_channel(&mut self, id: &str) {
        self.forget(id);
        self.threads
            .retain(|thread| thread.id != id && thread.parent.as_deref() != Some(id));
    }

    /// Takes a channel or category out of the lists, leaving its threads.
    fn forget(&mut self, id: &str) {
        self.text_channels.retain(|channel| channel.id != id);
        self.voice_channels.retain(|channel| channel.id != id);
        // Only a category's: a snapshot may share them, and they'd be copied.
        if self.categories.contains_key(id) {
            Arc::make_mut(&mut self.categories).remove(id);
        }
    }

    /// Adds or replaces a thread from a channel object. One without
    /// this account's thread member keeps whether it's in it, as
    /// THREAD_UPDATE leaves the member out.
    pub fn put_thread(&mut self, value: &Value) {
        let Some(mut thread) = parse_thread(value) else {
            return;
        };
        let known = self.threads.iter().position(|known| known.id == thread.id);
        if let Some(known) = known.map(|at| self.threads.remove(at))
            && let Some(was) = known.thread
            && let Some(details) = thread.thread.as_deref_mut()
        {
            if !details.joined {
                details.joined = was.joined;
                details.mute = was.mute;
                details.level = was.level;
            }
            // An update from an older payload doesn't count backwards.
            details.messages = details.messages.max(was.messages);
            if was.last_message >= details.last_message {
                details.last_message = was.last_message;
                details.recent = was.recent;
            }
            if details.member_preview.is_empty() {
                details.member_preview = was.member_preview;
            }
            if details.archive_after == 0 {
                details.archive_after = was.archive_after;
            }
            if details.archive_changed == 0 {
                details.archive_changed = was.archive_changed;
            }
        }
        // The newest first: a later thread has a larger snowflake.
        let order = |channel: &Channel| std::cmp::Reverse(channel.id.parse::<u64>().unwrap_or(0));
        let at = self
            .threads
            .partition_point(|known| order(known) < order(&thread));
        self.threads.insert(at, thread);
    }

    /// A known thread, to change.
    pub fn thread_mut(&mut self, id: &str) -> Option<&mut Thread> {
        self.threads
            .iter_mut()
            .find(|thread| thread.id == id)?
            .thread
            .as_deref_mut()
    }

    /// A text or voice channel, or a thread, by id.
    pub fn channel(&self, id: &str) -> Option<&Channel> {
        self.text_channels
            .iter()
            .chain(&self.voice_channels)
            .chain(&self.threads)
            .find(|channel| channel.id == id)
    }

    /// The channel a thread is in.
    pub fn parent(&self, thread: &Channel) -> Option<&Channel> {
        let parent = thread.parent.as_deref()?;
        self.text_channels
            .iter()
            .find(|channel| channel.id == parent)
    }

    /// The channel's category, if it has one this server still lists; a
    /// thread's is its channel's.
    pub fn category(&self, channel: &Channel) -> Option<&Category> {
        let channel = match &channel.thread {
            Some(_) => self.parent(channel)?,
            None => channel,
        };
        self.categories.get(channel.parent.as_ref()?)
    }

    /// Where a channel's category comes in Discord's order: channels outside
    /// categories first, then each category by position.
    pub fn category_order(&self, channel: &Channel) -> (bool, i64, String) {
        match self.category(channel) {
            Some(category) => (
                true,
                category.position,
                channel.parent.clone().unwrap_or_default(),
            ),
            None => (false, 0, String::new()),
        }
    }

    /// Discord's order: by category, then by position.
    fn sort_channels(&mut self) {
        let text = self.order(&self.text_channels);
        let voice = self.order(&self.voice_channels);
        permute(&mut self.text_channels, text);
        permute(&mut self.voice_channels, voice);
    }

    /// Where each of `channels` was, in Discord's order: the keys borrow
    /// their ids, as [`Self::category_order`]'s copies would cost a large
    /// server two allocations a channel on each sort.
    fn order(&self, channels: &[Channel]) -> Vec<usize> {
        let mut keys: Vec<_> = channels
            .iter()
            .enumerate()
            .map(|(at, channel)| {
                // Channels outside categories first, as category_order has it.
                let category = self
                    .category(channel)
                    .map(|category| (category.position, channel.parent.as_deref()));
                (category, channel.position, channel.id.as_str(), at)
            })
            .collect();
        keys.sort_unstable();
        keys.into_iter().map(|(.., at)| at).collect()
    }

    /// A text or voice channel's name, or a thread's.
    pub fn channel_name(&self, id: &str) -> Option<&str> {
        self.channel(id).map(|channel| channel.name.as_str())
    }

    /// What `user`, holding `roles`, may do in the server as a whole: the
    /// @everyone role's permissions and their roles'. The owner and
    /// administrators may do everything.
    pub fn server_permissions(&self, user: &str, roles: &[String]) -> u64 {
        self.timed_out(user, self.role_permissions(user, roles))
    }

    /// What `user`'s roles give them, before a timeout takes it away.
    fn role_permissions(&self, user: &str, roles: &[String]) -> u64 {
        if self.owner_id.as_deref() == Some(user) {
            return permission::ALL;
        }
        let mut base = self.roles.get(&self.id).copied().unwrap_or_default();
        for role in roles {
            base |= self.roles.get(role).copied().unwrap_or_default();
        }
        if base & ADMINISTRATOR != 0 {
            return permission::ALL;
        }
        base
    }

    /// What `user`, holding `roles`, may do in `channel`, computed as Discord
    /// does: [`Self::server_permissions`], then the channel's overwrites for
    /// @everyone, for the roles, and for the member.
    /// A thread takes its channel's overwrites.
    pub fn channel_permissions(&self, channel: &Channel, user: &str, roles: &[String]) -> u64 {
        let overwrites = match &channel.thread {
            Some(_) => self
                .parent(channel)
                .map_or(&channel.overwrites, |parent| &parent.overwrites),
            None => &channel.overwrites,
        };
        self.overwritten(overwrites, user, roles)
    }

    /// What `user`, holding `roles`, may do in a category, as
    /// [`Self::channel_permissions`] works it out for a channel; none for
    /// one the server doesn't have.
    pub fn category_permissions(
        &self,
        category: &str,
        user: &str,
        roles: &[String],
    ) -> Option<u64> {
        let category = self.categories.get(category)?;
        Some(self.overwritten(&category.overwrites, user, roles))
    }

    /// [`Self::server_permissions`] with a channel's or category's
    /// overwrites applied: @everyone's, the roles', then the member's.
    /// Channel Permissions asks it of a draft's overwrites too.
    pub fn overwritten(&self, overwrites: &[Overwrite], user: &str, roles: &[String]) -> u64 {
        let mut permissions = self.role_permissions(user, roles);
        if permissions == permission::ALL {
            return permissions;
        }
        let mut apply = |allow: u64, deny: u64| {
            permissions &= !deny;
            permissions |= allow;
        };
        if let Some(everyone) = overwrites
            .iter()
            .find(|overwrite| !overwrite.member && overwrite.id == self.id)
        {
            apply(everyone.allow, everyone.deny);
        }
        let (allow, deny) = overwrites
            .iter()
            .filter(|overwrite| !overwrite.member && roles.contains(&overwrite.id))
            .fold((0, 0), |(allow, deny), overwrite| {
                (allow | overwrite.allow, deny | overwrite.deny)
            });
        apply(allow, deny);
        if let Some(member) = overwrites
            .iter()
            .find(|overwrite| overwrite.member && overwrite.id == user)
        {
            apply(member.allow, member.deny);
        }
        self.timed_out(user, permissions)
    }

    /// `permissions` as a timeout of `user`'s leaves them: viewing channels
    /// and reading their history, unless the owner or an administrator,
    /// as Discord's client works it out after the overwrites.
    fn timed_out(&self, user: &str, permissions: u64) -> u64 {
        match &self.my_timeout {
            Some(timeout) if timeout.user == user && permissions != permission::ALL => {
                permissions & (VIEW_CHANNEL | permission::READ_MESSAGE_HISTORY)
            }
            _ => permissions,
        }
    }

    /// The position of the highest of `roles`, which decides whom a member
    /// outranks; 0, @everyone's, with none.
    pub fn top_position(&self, roles: &[String]) -> i64 {
        roles
            .iter()
            .filter_map(|role| self.role_positions.get(role))
            .copied()
            .max()
            .unwrap_or_default()
    }

    /// Whether `me`, holding `my_roles`, outranks `target`, holding
    /// `target_roles`, as Discord requires before kicking, banning, timing
    /// out, renaming or changing the roles of someone: nobody outranks the
    /// owner, the owner outranks everyone else, and otherwise the highest
    /// role must be higher. Nobody outranks themselves; their own nickname
    /// takes [`permission::CHANGE_NICKNAME`] instead.
    pub fn outranks(
        &self,
        me: &str,
        my_roles: &[String],
        target: &str,
        target_roles: &[String],
    ) -> bool {
        let owner = self.owner_id.as_deref();
        if me == target || owner == Some(target) {
            return false;
        }
        owner == Some(me) || self.top_position(my_roles) > self.top_position(target_roles)
    }

    /// Whether `me`, holding `my_roles`, may give `role` to someone or take
    /// it away: it takes [`permission::MANAGE_ROLES`] and a highest role
    /// above it, unless `me` owns the server. Nobody grants @everyone, a
    /// role an integration manages, or one the server doesn't have.
    pub fn may_grant(&self, me: &str, my_roles: &[String], role: &str) -> bool {
        let Some(position) = self.role_positions.get(role) else {
            return false;
        };
        if role == self.id || self.managed_roles.contains(role) {
            return false;
        }
        if self.owner_id.as_deref() == Some(me) {
            return true;
        }
        self.server_permissions(me, my_roles) & permission::MANAGE_ROLES != 0
            && self.top_position(my_roles) > *position
    }
}

/// Discord sends permission bits as decimal strings.
fn bits(value: &Value) -> u64 {
    value
        .as_str()
        .and_then(|bits| bits.parse().ok())
        .or_else(|| value.as_u64())
        .unwrap_or_default()
}

/// A role object as its id and permission bits.
pub fn role(role: &Value) -> Option<(String, u64)> {
    Some((role["id"].as_str()?.to_owned(), bits(&role["permissions"])))
}

impl Server {
    /// Adds or replaces a role from a role object.
    pub fn put_role(&mut self, value: &Value) {
        if let Some((id, permissions)) = role(value) {
            if let Some(name) = value["name"].as_str() {
                Arc::make_mut(&mut self.role_names).insert(id.clone(), name.to_owned());
            }
            if value["managed"].as_bool() == Some(true) {
                Arc::make_mut(&mut self.managed_roles).insert(id.clone());
            } else {
                Arc::make_mut(&mut self.managed_roles).remove(&id);
            }
            if value["mentionable"].as_bool() == Some(true) {
                Arc::make_mut(&mut self.mentionable_roles).insert(id.clone());
            } else {
                Arc::make_mut(&mut self.mentionable_roles).remove(&id);
            }
            Arc::make_mut(&mut self.role_positions)
                .insert(id.clone(), value["position"].as_i64().unwrap_or_default());
            match value["color"].as_u64().filter(|color| *color != 0) {
                Some(color) => {
                    let position = value["position"].as_i64().unwrap_or_default();
                    Arc::make_mut(&mut self.role_colors)
                        .insert(id.clone(), (position, color as u32));
                }
                None => {
                    Arc::make_mut(&mut self.role_colors).remove(&id);
                }
            }
            Arc::make_mut(&mut self.roles).insert(id, permissions);
        }
    }

    /// Removes a deleted role.
    pub fn remove_role(&mut self, id: &str) {
        Arc::make_mut(&mut self.roles).remove(id);
        Arc::make_mut(&mut self.role_names).remove(id);
        Arc::make_mut(&mut self.role_colors).remove(id);
        Arc::make_mut(&mut self.role_positions).remove(id);
        Arc::make_mut(&mut self.mentionable_roles).remove(id);
        Arc::make_mut(&mut self.managed_roles).remove(id);
    }

    /// Whether `user`, holding `roles`, may mention @everyone, @here and every role
    /// in `channel`.
    pub fn may_mention_everyone(&self, channel: &Channel, user: &str, roles: &[String]) -> bool {
        !self.roles.is_empty()
            && self.channel_permissions(channel, user, roles) & MENTION_EVERYONE != 0
    }

    /// The roles a message in a channel may mention, highest first, with their ids and
    /// colours: every named role with `everyone`, else those anyone may mention.
    pub fn mentionable(&self, everyone: bool) -> Vec<(String, String, Option<u32>)> {
        let mut roles: Vec<_> = self
            .role_names
            .iter()
            .filter(|(id, _)| **id != self.id && (everyone || self.mentionable_roles.contains(*id)))
            .map(|(id, name)| {
                let position = self.role_positions.get(id).copied().unwrap_or_default();
                let color = self.role_colors.get(id).map(|(_, color)| *color);
                (std::cmp::Reverse(position), id.clone(), name.clone(), color)
            })
            .collect();
        roles.sort();
        roles
            .into_iter()
            .map(|(_, id, name, color)| (id, name, color))
            .collect()
    }

    /// A member's named roles, highest first, each with its colour if it has one.
    pub fn named_roles(&self, roles: &[String]) -> Vec<(String, Option<u32>)> {
        let mut named: Vec<_> = roles
            .iter()
            .filter(|role| **role != self.id)
            .filter_map(|role| {
                let name = self.role_names.get(role)?;
                let position = self.role_positions.get(role).copied().unwrap_or_default();
                let color = self.role_colors.get(role).map(|(_, color)| *color);
                Some((std::cmp::Reverse(position), name.clone(), color))
            })
            .collect();
        named.sort();
        named
            .into_iter()
            .map(|(_, name, color)| (name, color))
            .collect()
    }

    /// The colour Discord gives a member's name: their highest coloured role's.
    pub fn color(&self, roles: &[String]) -> Option<u32> {
        roles
            .iter()
            .filter_map(|role| self.role_colors.get(role))
            .max_by_key(|(position, _)| *position)
            .map(|(_, color)| *color)
    }
}

pub fn user_id(ready: &Value) -> Option<&str> {
    ready["user"]["id"].as_str()
}

/// The login Discord refreshed this one to, which takes the place of the
/// one Yapfast identified with from now on. The web client's capabilities,
/// which Yapfast sends, ask for it.
pub fn auth_token(ready: &Value) -> Option<&str> {
    ready["auth_token"]
        .as_str()
        .filter(|token| !token.is_empty())
}

/// Servers from READY's guilds, in READY's order; the service orders them as
/// the account's settings say.
pub fn servers(ready: &Value) -> Vec<Server> {
    ready["guilds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(server)
        .collect()
}

/// One guild object, from READY or GUILD_CREATE.
pub fn server(guild: &Value) -> Option<Server> {
    let id = guild["id"].as_str()?.to_owned();
    let name = guild["name"]
        .as_str()
        .or_else(|| guild["properties"]["name"].as_str())
        .unwrap_or(&id)
        .to_owned();
    let owner_id = guild["owner_id"]
        .as_str()
        .or_else(|| guild["properties"]["owner_id"].as_str())
        .map(str::to_owned);
    let icon = guild["icon"]
        .as_str()
        .or_else(|| guild["properties"]["icon"].as_str())
        .map(|hash| server_icon(&id, hash));
    let banner = guild["banner"]
        .as_str()
        .or_else(|| guild["properties"]["banner"].as_str())
        .map(|hash| self::banner(&id, hash));
    let features = guild["features"]
        .as_array()
        .or_else(|| guild["properties"]["features"].as_array());
    let has = |name: &str| {
        features
            .into_iter()
            .flatten()
            .any(|feature| feature.as_str() == Some(name))
    };
    let premium_tier = guild["premium_tier"]
        .as_u64()
        .or_else(|| guild["properties"]["premium_tier"].as_u64())
        .unwrap_or_default();
    let mut server = Server {
        id,
        name,
        icon,
        banner,
        member_count: guild["member_count"].as_u64(),
        owner_id,
        premium_tier: u8::try_from(premium_tier).unwrap_or(u8::MAX),
        mfa_level: guild["mfa_level"]
            .as_u64()
            .or_else(|| guild["properties"]["mfa_level"].as_u64())
            .map_or(0, |level| u8::try_from(level).unwrap_or(1)),
        emojis: Arc::new(emojis(&guild["emojis"])),
        joined_at: guild["joined_at"].as_str().map(str::to_owned),
        default_level: Level::from(&guild["properties"]["default_message_notifications"])
            .or_else(|| Level::from(&guild["default_message_notifications"]))
            .unwrap_or_default(),
        unavailable: guild["unavailable"].as_bool().unwrap_or(false),
        forwarding_disabled: has("FORWARDING_DISABLED"),
        verified: has("VERIFIED") || has("PARTNERED"),
        community: has("COMMUNITY"),
        ..Default::default()
    };
    for role in guild["roles"].as_array().into_iter().flatten() {
        server.put_role(role);
    }
    let channels = guild["channels"].as_array().into_iter().flatten();
    // Sorted once all are in: one sort a channel, each copying every key,
    // took 10.7 ms for a server of 500 channels.
    for channel in channels {
        if is_thread(channel) {
            server.put_thread(channel);
        } else {
            server.add_channel(channel);
        }
    }
    server.sort_channels();
    // A user account hears only of the active threads it joined. The newest
    // first, as put_thread keeps them.
    let mut threads: Vec<Channel> = guild["threads"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_thread)
        .collect();
    for details in threads
        .iter_mut()
        .filter_map(|thread| thread.thread.as_deref_mut())
    {
        details.joined = true;
    }
    threads.append(&mut server.threads);
    threads.sort_by_cached_key(|thread| std::cmp::Reverse(thread.id.parse::<u64>().unwrap_or(0)));
    threads.dedup_by(|later, earlier| later.id == earlier.id);
    server.threads = threads;
    Some(server)
}

/// Reorders `list` to hold what was at each of `order` in turn, as
/// `sort_by_cached_key` applies its sorted indices.
fn permute<T>(list: &mut [T], mut order: Vec<usize>) {
    for at in 0..list.len() {
        let mut from = order[at];
        // What was there has moved on already, to where `from` was.
        while from < at {
            from = order[from];
        }
        order[at] = from;
        list.swap(at, from);
    }
}

/// Whether a channel object is a thread.
pub fn is_thread(channel: &Value) -> bool {
    matches!(
        channel["type"].as_u64(),
        Some(ANNOUNCEMENT_THREAD | PUBLIC_THREAD | PRIVATE_THREAD)
    )
}

/// A channel object's permission overwrites.
fn overwrites(channel: &Value) -> Vec<Overwrite> {
    channel["permission_overwrites"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|overwrite| {
            Some(Overwrite {
                id: overwrite["id"].as_str()?.to_owned(),
                // Type 1, or "member" in older payloads, targets one member.
                member: overwrite["type"].as_u64() == Some(1) || overwrite["type"] == "member",
                allow: bits(&overwrite["allow"]),
                deny: bits(&overwrite["deny"]),
            })
        })
        .collect()
}

/// A thread from its channel object, with this account's thread member
/// when it carries one; none for anything but a thread.
pub fn parse_thread(value: &Value) -> Option<Channel> {
    if !is_thread(value) {
        return None;
    }
    let mut thread = parse_channel(value)?;
    let metadata = &value["thread_metadata"];
    let mut details = Thread {
        owner: value["owner_id"].as_str().map(str::to_owned),
        private: value["type"].as_u64() == Some(PRIVATE_THREAD),
        archived: metadata["archived"].as_bool().unwrap_or(false),
        locked: metadata["locked"].as_bool().unwrap_or(false),
        messages: value["message_count"].as_u64().unwrap_or_default(),
        last_message: value["last_message_id"].as_str().map_or(0, snowflake),
        archive_after: metadata["auto_archive_duration"]
            .as_u64()
            .and_then(|minutes| u16::try_from(minutes).ok())
            .unwrap_or_default(),
        archive_changed: metadata["archive_timestamp"]
            .as_str()
            .and_then(crate::message::iso_ms)
            .unwrap_or_default(),
        members: value["member_count"]
            .as_u64()
            .and_then(|count| u32::try_from(count).ok())
            .unwrap_or_default(),
        member_preview: member_preview(&value["member_ids_preview"]).unwrap_or_default(),
        tags: forum::applied_tags(value),
        pinned: forum::pinned(value),
        ..Default::default()
    };
    if value["member"].is_object() {
        details.join(&value["member"]);
    }
    thread.thread = Some(Box::new(details));
    Some(thread)
}

fn parse_channel(channel: &Value) -> Option<Channel> {
    Some(Channel {
        id: channel["id"].as_str()?.to_owned(),
        name: channel["name"].as_str().unwrap_or_default().to_owned(),
        position: channel["position"].as_i64().unwrap_or_default(),
        parent: channel["parent_id"].as_str().map(str::to_owned),
        topic: channel["topic"]
            .as_str()
            .filter(|topic| !topic.trim().is_empty())
            .map(Arc::from),
        overwrites: Arc::new(overwrites(channel)),
        announcement: channel["type"].as_u64() == Some(GUILD_ANNOUNCEMENT),
        archive_after: channel["default_auto_archive_duration"]
            .as_u64()
            .and_then(|minutes| u16::try_from(minutes).ok())
            .unwrap_or_default(),
        forum: forum::parse(channel),
        slowmode: channel["rate_limit_per_user"]
            .as_u64()
            .and_then(|seconds| u32::try_from(seconds).ok())
            .unwrap_or_default(),
        ..Default::default()
    })
}

/// The name Discord shows for a user object: their display name, else their
/// username.
pub fn display_name(user: &Value) -> Option<&str> {
    user["global_name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .or_else(|| user["username"].as_str())
}

/// Discord's CDN, or the test server.
pub(crate) fn cdn() -> &'static str {
    client::test_server().unwrap_or("https://cdn.discordapp.com")
}

/// Discord's media proxy, or the test server.
pub(crate) fn media() -> &'static str {
    client::test_server().unwrap_or("https://media.discordapp.net")
}

/// A sticker's picture at the size Discord shows it. Animated GIF stickers come
/// from the media proxy; Lottie stickers are JSON from the CDN.
pub fn sticker(id: &str, format: u64) -> String {
    if format == 4 {
        format!("{}/stickers/{id}.gif?size=160", media())
    } else if format == 3 {
        format!("{}/stickers/{id}.json", cdn())
    } else {
        format!("{}/stickers/{id}.png?size=160", cdn())
    }
}

/// A custom emoji's picture, at `size` pixels at most.
pub fn custom_emoji(id: &str, animated: bool, size: u32) -> String {
    let format = if animated { "gif" } else { "png" };
    format!("{}/emojis/{id}.{format}?size={size}", cdn())
}

pub fn server_icon(guild: &str, hash: &str) -> String {
    format!("{}/icons/{guild}/{hash}.png?size={PICTURE_SIZE}", cdn())
}

/// A user's avatar. Without one, Discord shows one of its default avatars,
/// picked from the account's id.
pub fn avatar(user: &str, hash: Option<&str>) -> String {
    match hash {
        Some(hash) => format!("{}/avatars/{user}/{hash}.png?size={PICTURE_SIZE}", cdn()),
        None => {
            let index = user.parse::<u64>().map_or(0, |id| (id >> 22) % 6);
            format!("{}/embed/avatars/{index}.png", cdn())
        }
    }
}

/// A profile or server banner, at the profile card's widest.
pub fn banner(user: &str, hash: &str) -> String {
    format!("{}/banners/{user}/{hash}.png?size=600", cdn())
}

/// A server's invite background, at the width its settings show it.
pub fn splash(guild: &str, hash: &str) -> String {
    format!("{}/splashes/{guild}/{hash}.png?size=600", cdn())
}

/// A role's icon, as its settings show it.
pub fn role_icon(role: &str, hash: &str) -> String {
    format!("{}/role-icons/{role}/{hash}.png?size=64", cdn())
}

/// The banner a member set for one server.
pub fn member_banner(guild: &str, user: &str, hash: &str) -> String {
    format!(
        "{}/guilds/{guild}/users/{user}/banners/{hash}.png?size=600",
        cdn()
    )
}

/// The avatar a member set for one server, which Discord shows there instead.
pub fn member_avatar(guild: &str, user: &str, hash: &str) -> String {
    format!(
        "{}/guilds/{guild}/users/{user}/avatars/{hash}.png?size={PICTURE_SIZE}",
        cdn()
    )
}

/// MurmurHash3's 32-bit hash with seed 0, which Discord's client hashes
/// member list ids with.
fn murmur3(data: &[u8]) -> u32 {
    const C1: u32 = 0xcc9e_2d51;
    const C2: u32 = 0x1b87_3593;
    let scramble = |k: u32| k.wrapping_mul(C1).rotate_left(15).wrapping_mul(C2);
    let mut hash = 0u32;
    let mut blocks = data.chunks_exact(4);
    for block in &mut blocks {
        let k = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
        hash = (hash ^ scramble(k))
            .rotate_left(13)
            .wrapping_mul(5)
            .wrapping_add(0xe654_6b64);
    }
    let tail = blocks.remainder();
    if !tail.is_empty() {
        let k = tail
            .iter()
            .rev()
            .fold(0, |k, &byte| k << 8 | u32::from(byte));
        hash ^= scramble(k);
    }
    hash ^= data.len() as u32;
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85eb_ca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2_ae35);
    hash ^ hash >> 16
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_user_ready_guilds() {
        let ready = json!({
            "user": {"id": "1"},
            "guilds": [{
                "id": "10",
                "properties": {"name": "Test", "icon": "abc"},
                "channels": [
                    {"id": "12", "name": "Second", "type": 2, "position": 2},
                    {"id": "11", "name": "general", "type": 0, "position": 0},
                    {"id": "13", "name": "Lounge", "type": 2, "position": 1},
                ],
            }],
        });
        assert_eq!(user_id(&ready), Some("1"));
        let servers = servers(&ready);
        assert_eq!(servers[0].name, "Test");
        assert_eq!(
            servers[0].icon.as_deref(),
            Some("https://cdn.discordapp.com/icons/10/abc.png?size=128")
        );
        let names: Vec<_> = servers[0]
            .voice_channels
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, ["Lounge", "Second"]);
    }

    #[test]
    fn channels_with_the_same_readers_share_a_list() {
        let overwrite = |id: &str, allow: u64, deny: u64| Overwrite {
            id: id.into(),
            member: false,
            allow,
            deny,
        };
        let channel = |id: &str, overwrites: Vec<Overwrite>| Channel {
            id: id.into(),
            overwrites: overwrites.into(),
            ..Default::default()
        };
        let mut server = Server {
            id: "10".into(),
            roles: HashMap::from([("10".into(), VIEW_CHANNEL)]).into(),
            text_channels: vec![
                channel("1", vec![]),
                // Only sending is changed: the readers stay everyone.
                channel("2", vec![overwrite("10", 0, SEND_MESSAGES)]),
                // Letting a role see what everyone sees changes nothing.
                channel("5", vec![overwrite("30", VIEW_CHANNEL, 0)]),
                // Hiding it from a role does.
                channel("6", vec![overwrite("10", 0, VIEW_CHANNEL)]),
                channel(
                    "3",
                    vec![
                        overwrite("10", 0, VIEW_CHANNEL),
                        overwrite("30", VIEW_CHANNEL, 0),
                    ],
                ),
                channel(
                    "4",
                    vec![
                        overwrite("30", VIEW_CHANNEL, 0),
                        overwrite("10", 0, VIEW_CHANNEL),
                    ],
                ),
            ],
            ..Default::default()
        };
        assert_eq!(server.readers("1"), "everyone");
        assert_eq!(server.readers("2"), "everyone");
        assert_eq!(server.readers("5"), "everyone");
        // Discord's ids: the hashes of "allow:30,deny:10" and "deny:10".
        assert_eq!(server.readers("3"), "3643570923");
        assert_eq!(server.readers("3"), server.readers("4"));
        assert_eq!(server.readers("6"), "3007946959");
        assert_ne!(
            server.readers("9"),
            server.readers("1"),
            "unknown channels share nothing"
        );
        // Where @everyone sees nothing, a role's overwrite is the list.
        server.roles = HashMap::from([("10".into(), 0)]).into();
        assert_eq!(server.readers("5"), murmur3(b"allow:30").to_string());
    }

    #[test]
    fn hashes_as_discords_client_does() {
        for (text, hash) in [
            ("", 0),
            ("a", 1_009_084_850),
            ("ab", 2_613_040_991),
            ("abc", 3_017_643_002),
            ("hello", 613_153_351),
        ] {
            assert_eq!(murmur3(text.as_bytes()), hash, "{text:?}");
        }
    }

    #[test]
    fn channel_permissions_follow_roles_and_overwrites() {
        let everyone = VIEW_CHANNEL | CONNECT;
        let server = Server {
            id: "10".into(),
            owner_id: Some("99".into()),
            roles: HashMap::from([
                ("10".into(), everyone),
                ("20".into(), 0),
                ("30".into(), ADMINISTRATOR),
            ])
            .into(),
            ..Default::default()
        };
        let open = Channel::default();
        // Hidden from @everyone, shown to role 20.
        let staff = Channel {
            overwrites: vec![
                Overwrite {
                    id: "10".into(),
                    member: false,
                    allow: 0,
                    deny: VIEW_CHANNEL,
                },
                Overwrite {
                    id: "20".into(),
                    member: false,
                    allow: VIEW_CHANNEL,
                    deny: 0,
                },
            ]
            .into(),
            ..Default::default()
        };
        // Visible, but one member may not connect.
        let banned = Channel {
            overwrites: vec![Overwrite {
                id: "1".into(),
                member: true,
                allow: 0,
                deny: CONNECT,
            }]
            .into(),
            ..Default::default()
        };
        assert_eq!(server.access(&open, "1", &[]), Access::Join);
        assert_eq!(server.access(&staff, "1", &[]), Access::Hidden);
        assert_eq!(server.access(&staff, "1", &["20".into()]), Access::Join);
        assert_eq!(server.access(&banned, "1", &[]), Access::Locked);
        assert_eq!(server.access(&banned, "2", &[]), Access::Join);
        assert_eq!(
            server.access(&staff, "99", &[]),
            Access::Join,
            "the owner sees everything"
        );
        assert_eq!(
            server.access(&staff, "1", &["30".into()]),
            Access::Join,
            "administrators too"
        );
    }

    #[test]
    fn slowmode_and_forwarding_follow_permissions_and_features() {
        use permission::{MANAGE_MESSAGES, READ_MESSAGE_HISTORY};
        let mut server = server(&json!({
            "id": "10", "properties": {"name": "Test", "features": ["FORWARDING_DISABLED"]},
        }))
        .unwrap();
        assert!(server.forwarding_disabled);
        server.forwarding_disabled = false;
        server.roles = HashMap::from([
            ("10".into(), VIEW_CHANNEL | READ_MESSAGE_HISTORY),
            ("20".into(), MANAGE_MESSAGES),
        ])
        .into();
        let slow =
            parse_channel(&json!({"id": "11", "type": 0, "rate_limit_per_user": 30})).unwrap();
        assert_eq!(slow.slowmode, 30);
        assert!(server.slowed(&slow, "1", &[]));
        assert!(
            !server.slowed(&slow, "1", &["20".into()]),
            "moderators bypass it"
        );
        assert!(server.forwards_from(&slow, "1", &[]));
        let unread = Channel {
            overwrites: vec![Overwrite {
                id: "10".into(),
                member: false,
                allow: 0,
                deny: READ_MESSAGE_HISTORY,
            }]
            .into(),
            ..Default::default()
        };
        assert!(!server.forwards_from(&unread, "1", &[]));
    }

    #[test]
    fn a_timeout_leaves_its_member_viewing_and_reading() {
        use permission::{MANAGE_MESSAGES, READ_MESSAGE_HISTORY, SEND_MESSAGES};
        let mut server = Server {
            id: "10".into(),
            owner_id: Some("99".into()),
            roles: HashMap::from([
                (
                    "10".into(),
                    VIEW_CHANNEL | READ_MESSAGE_HISTORY | SEND_MESSAGES,
                ),
                ("20".into(), MANAGE_MESSAGES | permission::KICK_MEMBERS),
                ("30".into(), ADMINISTRATOR),
            ])
            .into(),
            my_timeout: Some(Timeout {
                user: "1".into(),
                until: u64::MAX,
            }),
            ..Default::default()
        };
        // Even a member's own overwrite doesn't give back what it takes.
        let general = Channel {
            overwrites: vec![Overwrite {
                id: "1".into(),
                member: true,
                allow: SEND_MESSAGES,
                deny: 0,
            }]
            .into(),
            ..Default::default()
        };
        let mods = ["20".to_owned()];
        assert_eq!(
            server.channel_permissions(&general, "1", &mods),
            VIEW_CHANNEL | READ_MESSAGE_HISTORY
        );
        assert_eq!(
            server.server_permissions("1", &mods),
            VIEW_CHANNEL | READ_MESSAGE_HISTORY
        );
        assert!(!server.may_send(&general, "1", &mods));
        assert_eq!(server.deletes(&general, "1", &mods), Deletes::Own);
        assert!(
            server.may_send(&general, "2", &mods),
            "only the one timed out"
        );
        assert_eq!(
            server.server_permissions("1", &["30".into()]),
            permission::ALL,
            "nor administrators"
        );
        server.my_timeout.as_mut().unwrap().user = "99".into();
        assert!(server.may_send(&general, "99", &[]), "nor the owner");
    }

    #[test]
    fn deletes_follow_manage_messages_and_archived_threads() {
        use permission::{MANAGE_MESSAGES, SEND_MESSAGES};
        let mut server = Server {
            id: "10".into(),
            ..Default::default()
        };
        let general = Channel::default();
        assert_eq!(
            server.deletes(&general, "1", &["20".into()]),
            Deletes::Own,
            "without the roles"
        );
        server.roles = HashMap::from([
            (
                "10".into(),
                VIEW_CHANNEL | SEND_MESSAGES | SEND_MESSAGES_IN_THREADS,
            ),
            ("20".into(), MANAGE_MESSAGES),
        ])
        .into();
        assert_eq!(server.deletes(&general, "1", &[]), Deletes::Own);
        assert_eq!(server.deletes(&general, "1", &["20".into()]), Deletes::Any);
        let thread = |archived, locked| Channel {
            thread: Some(Box::new(Thread {
                archived,
                locked,
                ..Default::default()
            })),
            ..Default::default()
        };
        assert_eq!(
            server.deletes(&thread(false, true), "1", &["20".into()]),
            Deletes::Any,
            "an open thread, even locked"
        );
        assert_eq!(server.deletes(&thread(true, false), "1", &[]), Deletes::Own);
        assert_eq!(
            server.deletes(&thread(true, true), "1", &["20".into()]),
            Deletes::None
        );
        let quiet = Channel {
            overwrites: vec![Overwrite {
                id: "10".into(),
                member: false,
                allow: 0,
                deny: SEND_MESSAGES_IN_THREADS,
            }]
            .into(),
            ..thread(true, false)
        };
        assert_eq!(
            server.deletes(&quiet, "1", &["20".into()]),
            Deletes::None,
            "one it may not open again"
        );
        let message = |author: &str, invoker: Option<&str>| crate::message::Message {
            author_id: author.into(),
            invoker: invoker.map(str::to_owned),
            ..Default::default()
        };
        assert!(Deletes::Own.allow(&message("1", None), "1"));
        assert!(!Deletes::Own.allow(&message("2", None), "1"));
        assert!(
            Deletes::Own.allow(&message("2", Some("1")), "1"),
            "an app's answer to its command"
        );
        assert!(!Deletes::Own.allow(&message("2", Some("3")), "1"));
        assert!(Deletes::Any.allow(&message("2", None), "1"));
        assert!(!Deletes::None.allow(&message("1", None), "1"));
        let call = crate::message::Message {
            undeletable: true,
            ..message("1", None)
        };
        assert!(!Deletes::Any.allow(&call, "1"));
    }

    #[test]
    fn placing_channels_moves_them_and_says_where_they_were() {
        let mut server = server(&json!({
            "id": "10",
            "channels": [
                {"id": "4", "type": 4, "name": "Talk", "position": 0},
                {"id": "5", "type": 4, "name": "Games", "position": 1},
                {"id": "11", "type": 0, "name": "general", "position": 0, "parent_id": "4"},
                {"id": "12", "type": 0, "name": "news", "position": 1, "parent_id": "4"},
            ],
        }))
        .unwrap();
        let placement = |id: &str, position, parent: Option<Option<&str>>| Placement {
            id: id.into(),
            position,
            parent: parent.map(|parent| parent.map(str::to_owned)),
        };
        let was = server.place(&[
            placement("12", 0, Some(Some("5"))),
            placement("11", 1, None),
            placement("5", 0, None),
            placement("4", 1, None),
        ]);
        assert_eq!(
            was,
            [
                placement("12", 1, Some(Some("4"))),
                placement("11", 0, None),
                placement("5", 1, None),
                placement("4", 0, None),
            ]
        );
        let order: Vec<_> = server
            .text_channels
            .iter()
            .map(|channel| (channel.id.as_str(), channel.parent.as_deref()))
            .collect();
        assert_eq!(
            order,
            [("12", Some("5")), ("11", Some("4"))],
            "Games comes first"
        );
        server.place(&was);
        assert_eq!(server.text_channels[0].id, "11");
    }

    #[test]
    fn categories_keep_their_overwrites() {
        let mut server = server(&json!({
            "id": "10",
            "roles": [{"id": "10", "permissions": VIEW_CHANNEL.to_string()},
                      {"id": "20", "permissions": "0"}],
            "channels": [{"id": "4", "type": 4, "name": "Staff", "position": 0,
                          "permission_overwrites": [
                              {"id": "10", "type": 0, "allow": "0", "deny": VIEW_CHANNEL.to_string()},
                              {"id": "20", "type": 0, "allow": VIEW_CHANNEL.to_string(), "deny": "0"},
                          ]}],
        }))
        .unwrap();
        let sees = |server: &Server, roles: &[String]| {
            server
                .category_permissions("4", "1", roles)
                .map(|held| held & VIEW_CHANNEL != 0)
        };
        assert_eq!(sees(&server, &[]), Some(false));
        assert_eq!(sees(&server, &["20".into()]), Some(true));
        assert_eq!(server.category_permissions("5", "1", &[]), None);
        // CHANNEL_UPDATE opens it to everyone.
        server.put_channel(&json!({"id": "4", "type": 4, "name": "Staff", "position": 0}));
        assert_eq!(sees(&server, &[]), Some(true));
    }

    #[test]
    fn server_permissions_and_the_role_hierarchy() {
        use permission::*;
        let mut server = server(&json!({
            "id": "10", "owner_id": "99",
            "properties": {"name": "Test", "default_message_notifications": 0},
            "roles": [
                {"id": "10", "name": "@everyone", "permissions": "1024", "position": 0},
                {"id": "20", "name": "Mods", "permissions": (KICK_MEMBERS | MANAGE_ROLES).to_string(), "position": 3},
                {"id": "30", "name": "Helpers", "permissions": "0", "position": 2},
                {"id": "40", "name": "Admins", "permissions": ADMINISTRATOR.to_string(), "position": 5},
                {"id": "50", "name": "Bot", "permissions": "0", "position": 1, "managed": true},
            ],
        }))
        .unwrap();
        assert_eq!(server.default_level, Level::All);
        let mods = ["20".to_owned()];
        let helpers = ["30".to_owned()];
        let admins = ["40".to_owned()];
        let may = |user: &str, roles: &[String], bit: u64| {
            server.server_permissions(user, roles) & bit != 0
        };
        assert!(may("1", &mods, KICK_MEMBERS));
        assert!(!may("1", &mods, BAN_MEMBERS));
        assert!(!may("1", &[], KICK_MEMBERS));
        assert!(may("1", &[], VIEW_CHANNEL), "from @everyone");
        assert!(
            may("1", &admins, MODERATE_MEMBERS),
            "administrators may all"
        );
        assert!(may("99", &[], MANAGE_GUILD), "so may the owner");

        // A member acts on those below their highest role, never the owner
        // or themselves; the owner acts on everyone else.
        assert!(server.outranks("1", &mods, "2", &helpers));
        assert!(!server.outranks("2", &helpers, "1", &mods));
        assert!(!server.outranks("1", &mods, "3", &mods), "an equal");
        assert!(!server.outranks("1", &admins, "99", &[]));
        assert!(server.outranks("99", &[], "1", &admins));
        assert!(!server.outranks("1", &admins, "1", &admins));

        // Roles below one's highest, with Manage Roles, but not @everyone
        // or a managed one.
        assert!(server.may_grant("1", &mods, "30"));
        assert!(!server.may_grant("1", &mods, "20"), "not their own level");
        assert!(!server.may_grant("1", &mods, "40"));
        assert!(!server.may_grant("1", &mods, "50"), "managed");
        assert!(!server.may_grant("1", &mods, "10"), "@everyone");
        assert!(!server.may_grant("2", &helpers, "50"));
        assert!(!server.may_grant("2", &helpers, "10"));
        assert!(!server.may_grant("2", &["30".to_owned()], "10"));
        assert!(server.may_grant("99", &[], "40"), "the owner grants any");
        assert!(!server.may_grant("99", &[], "77"), "not one it lacks");
        assert!(server.may_grant("1", &admins, "20"));

        server.remove_role("50");
        assert!(server.managed_roles.is_empty());
    }

    #[test]
    fn falls_back_to_default_avatars() {
        assert_eq!(
            avatar("1", Some("abc")),
            "https://cdn.discordapp.com/avatars/1/abc.png?size=128"
        );
        // (1400000000000000200 >> 22) % 6 == 2
        assert_eq!(
            avatar("1400000000000000200", None),
            "https://cdn.discordapp.com/embed/avatars/2.png"
        );
    }

    #[test]
    fn prefers_display_names() {
        assert_eq!(
            display_name(&json!({"username": "ann", "global_name": "Ann"})),
            Some("Ann")
        );
        assert_eq!(
            display_name(&json!({"username": "ann", "global_name": null})),
            Some("ann")
        );
    }

    #[test]
    fn a_thread_goes_quiet_its_archive_duration_after_its_last_activity() {
        let at = |ms: u64| crate::message::snowflake_at(ms).to_string();
        let day = 86_400_000;
        let start = 1_760_000_000_000;
        let thread = parse_thread(&json!({
            "id": at(start), "type": 11, "name": "Plans", "parent_id": "11",
            "thread_metadata": {"archived": false, "auto_archive_duration": 1440,
                "archive_timestamp": "2025-10-09T08:53:20+00:00"},
        }))
        .unwrap();
        let details = thread.thread.as_deref().unwrap();
        assert_eq!(
            (details.archive_after, details.archive_changed),
            (1440, start)
        );
        // A day after it started, or after its newest message.
        assert_eq!(details.quiet_at(&thread.id), start + day);
        let mut later = details.clone();
        later.last_message = at(start + 2 * day).parse().unwrap();
        assert_eq!(later.quiet_at(&thread.id), start + 3 * day);
        // Opened again after its newest message, a day after that.
        later.archive_changed = start + 4 * day;
        assert_eq!(later.quiet_at(&thread.id), start + 5 * day);
        // Without a duration, never.
        later.archive_after = 0;
        assert_eq!(later.quiet_at(&thread.id), u64::MAX);
    }
}
