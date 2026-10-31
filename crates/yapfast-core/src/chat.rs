//! The desktop owns presentation; this task owns the networks and voice.
//! Matrix and Discord are Cargo features, so a build carries either or both.
//! Each network runs in its own module beside this one: `matrix_chat` and
//! `discord_text`. This module routes the UI's commands to them.
use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    network::{Capabilities, Network},
    voice,
};

/// The text limit Yapfast enforces before queueing a message or an edit.
pub const MAX_MESSAGE_BYTES: usize = 16_000;
/// Larger downloads only get a file card, never an inline preview.
pub(crate) const MAX_PREVIEW_DOWNLOAD_BYTES: usize = 20 * 1024 * 1024;
pub const PREVIEW_WIDTH: u32 = 360;
pub const PREVIEW_HEIGHT: u32 = 270;
/// The pictures a pinned message's or a search result's card shows at
/// most; it names the rest.
pub const FOUND_PICTURES: usize = 4;
/// The image viewer shows pictures up to this size, so a full-size photo
/// costs at most about 9 MiB while it's open.
pub const VIEW_WIDTH: u32 = 1920;
pub const VIEW_HEIGHT: u32 = 1200;

/// What someone is doing on Discord: a game, music, a stream or a Hang
/// status, shared as the gateway parsed it.
#[cfg(feature = "discord")]
pub use yapfast_discord::activity::{Activities, Activity, Kind as ActivityKind};

/// Only Discord says what people are doing.
#[cfg(not(feature = "discord"))]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Activity {}
#[cfg(not(feature = "discord"))]
pub type Activities = Arc<[Activity]>;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ChatRoom {
    pub id: String,
    /// The network it's on, which its id names too.
    pub network: Network,
    /// What its network lets Yapfast do here.
    pub capabilities: Capabilities,
    pub name: String,
    pub direct: bool,
    pub voice: bool,
    pub encrypted: bool,
    pub members: u64,
    pub unread: u64,
    pub mentions: u64,
    /// Muted in the network's notification settings: a Discord DM.
    pub muted: bool,
    /// The timeline item of the last message this account read here, which
    /// may not be loaded: a Discord `d:` item, ordered by its snowflake, or
    /// a Matrix `e:` event from its read receipt.
    pub last_read: Option<String>,
    /// Listed, but this account may not read it, such as a Discord channel
    /// its roles keep it out of.
    pub locked: bool,
    /// A voice channel this account may not join, which Discord lists with
    /// a lock; its chat may still be readable.
    pub join_locked: bool,
    /// Readable, but this account may not send here, such as a Discord
    /// announcement channel.
    pub read_only: bool,
    /// A Discord channel or thread whose slowmode this account waits out
    /// between messages.
    pub slowed: bool,
    /// When this account's timeout in the room's Discord server ends, in
    /// Unix milliseconds, while it lasts.
    pub timed_out_until: Option<u64>,
    /// The Discord category the channel is listed under, or for a message
    /// request "Message requests" or "Spam"; empty for none.
    pub category: String,
    /// The room id of that Discord category, which tells it from others of
    /// the same name; shared by its channels.
    pub category_id: Option<Arc<str>>,
    /// A Discord message request: readable, but answered by accepting or
    /// ignoring it before replying.
    pub request: bool,
    /// The other person in a one-to-one DM: their Discord user id or Matrix ID.
    pub person: Option<String>,
    /// Who is in a Discord voice channel or DM call, known without joining it.
    pub people: Vec<VoicePerson>,
    /// Whom a Discord DM call here is ringing, other than this account.
    pub ringing: Vec<VoicePerson>,
    /// A Discord DM call here is ringing this account.
    pub incoming: bool,
    /// The address of a Discord DM's picture: the other person's avatar or
    /// the group's icon. See [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
    /// The largest file a message here may carry, in bytes.
    pub upload_limit: u64,
    /// A one-to-one Discord DM's other person: whether they're around, when
    /// Discord says, and their custom status or what they're doing.
    pub presence: Option<Presence>,
    pub activity: Option<String>,
    /// What they're doing, in full, as their profile shows it.
    pub activities: Activities,
    /// A Discord thread's channel, by room id: the thread lists under it.
    pub parent: Option<String>,
    /// A Discord forum, whose posts show in place of messages.
    pub forum: bool,
    /// A Discord server's text channel its channel list leaves out unless
    /// it's open or mentions this account, as Discord's client does with
    /// Show All Channels off for one this account doesn't follow; or a
    /// thread of one.
    pub unlisted: bool,
    /// A Discord message request's latest message, when it's known.
    pub preview: Option<Box<RequestPreview>>,
}

impl ChatRoom {
    /// It has messages: every room but a Matrix voice room, as a Discord
    /// voice channel has a chat beside its call.
    pub fn has_text(&self) -> bool {
        !self.voice || self.network == Network::Discord
    }

    /// A voice channel with a chat beside its call that this account may
    /// read.
    pub fn voice_chat(&self) -> bool {
        self.voice && self.has_text() && !self.locked
    }

    /// A call here is end-to-end encrypted: Discord's always are, by DAVE,
    /// and a Matrix room's when the room is.
    pub fn call_encrypted(&self) -> bool {
        self.network == Network::Discord || self.encrypted
    }
}

/// A Discord message request's latest message, as the requests list shows it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RequestPreview {
    /// Plain text on one line, or what files it carries.
    pub text: String,
    /// When it came, in Unix milliseconds.
    pub at: u64,
}

/// The most files one message carries, as Discord allows.
pub const MAX_FILES: usize = 10;

/// A Discord custom status: its text and emoji, and when it clears itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomStatus {
    pub text: String,
    /// A Unicode emoji, or a custom emoji's name.
    pub emoji: Option<String>,
    /// A custom emoji's id.
    pub emoji_id: Option<String>,
    /// Unix milliseconds when it clears itself; none when it stays.
    pub expires_at_ms: Option<u64>,
}

/// The game the Discord account shows it plays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playing {
    pub name: String,
    /// When it started, in Unix milliseconds, when the game says.
    pub started_ms: Option<u64>,
}

/// A Discord friend request waiting for an answer.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FriendRequest {
    /// Their Discord user id.
    pub id: String,
    pub name: String,
    /// The address of their avatar; see [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
    /// Sent to this account, rather than by it.
    pub incoming: bool,
    /// Discord took it for spam.
    pub spam: bool,
}

/// Someone the Discord account blocked, as the blocked list shows them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Blocked {
    /// Their Discord user id.
    pub id: String,
    pub name: String,
    /// Their username, such as `ann`, when their name is a display name.
    pub username: Option<String>,
    /// The address of their avatar; see [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
}

/// A Discord friend, as the friends list shows them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Friend {
    /// Their Discord user id.
    pub id: String,
    /// The nickname this account gave them, else their name.
    pub name: String,
    pub nickname: Option<String>,
    /// The address of their avatar; see [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
    /// Whether they're around, when Discord says, and their custom status or
    /// what they're doing.
    pub presence: Option<Presence>,
    pub activity: Option<String>,
    pub activities: Activities,
}

/// A file waiting in the composer to go with the next message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staged {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    /// A picture the composer can preview.
    pub image: bool,
    /// Covered until clicked, where the room's network has spoilers.
    pub spoiler: bool,
}

impl Staged {
    /// The file at `path`, if it's one a message here may carry: at most
    /// `limit` bytes.
    pub fn read(path: PathBuf, limit: u64) -> Result<Self, String> {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let metadata = std::fs::metadata(&path)
            .ok()
            .filter(std::fs::Metadata::is_file)
            .ok_or_else(|| format!("Yapfast could not read {name}."))?;
        if metadata.len() > limit {
            return Err(format!(
                "{name} is {}; files here can be up to {}.",
                file_size(metadata.len()),
                file_size(limit)
            ));
        }
        let image = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                ["png", "jpg", "jpeg", "gif", "webp"].contains(&extension.to_lowercase().as_str())
            });
        Ok(Self {
            path,
            name,
            size: metadata.len(),
            image,
            spoiler: false,
        })
    }
}

/// Why files dropped on the window can't go with the next message, in the
/// words of Discord's client, which refuses such a drop whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Something other than files, such as a folder.
    Invalid,
    /// With the files already waiting, more than [`MAX_FILES`].
    TooMany,
    /// One is larger than the conversation's limit, in bytes.
    TooLarge(u64),
}

impl Refusal {
    /// Why files dropped with `staged` waiting and a limit of `limit` bytes
    /// each can't go, if they can't.
    pub fn of(paths: &[PathBuf], staged: usize, limit: u64) -> Option<Self> {
        if paths.is_empty() || !paths.iter().all(|path| path.is_file()) {
            return Some(Self::Invalid);
        }
        if staged + paths.len() > MAX_FILES {
            return Some(Self::TooMany);
        }
        paths
            .iter()
            .any(|path| std::fs::metadata(path).is_ok_and(|metadata| metadata.len() > limit))
            .then_some(Self::TooLarge(limit))
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Invalid => "Invalid file type",
            Self::TooMany => "Too many uploads!",
            Self::TooLarge(_) => "Your files are too powerful",
        }
    }

    pub fn help(&self) -> String {
        match self {
            Self::Invalid => "That file type is not supported.".into(),
            Self::TooMany => format!("You can only upload {MAX_FILES} files at a time!"),
            // As Discord's client writes sizes here, to two places.
            Self::TooLarge(limit) => format!(
                "Max file size is {:.2} MB please.",
                *limit as f64 / 1_048_576.
            ),
        }
    }
}

/// A file going with a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutgoingFile {
    pub path: PathBuf,
    /// Marked as a spoiler, which only Discord keeps.
    pub spoiler: bool,
}

impl From<PathBuf> for OutgoingFile {
    fn from(path: PathBuf) -> Self {
        Self {
            path,
            spoiler: false,
        }
    }
}

/// A file size as people read it: bytes, KB, then MB.
pub fn file_size(bytes: u64) -> String {
    match bytes {
        0..1_024 => format!("{bytes} B"),
        1_024..1_048_576 => format!("{:.0} KB", bytes as f64 / 1_024.),
        _ => {
            let megabytes = bytes as f64 / 1_048_576.;
            if megabytes.fract() == 0.0 {
                format!("{megabytes:.0} MB")
            } else {
                format!("{megabytes:.1} MB")
            }
        }
    }
}

/// Makes `list` share `items`, keeping what it held at each place where the
/// item there is the same one, or equal to it. Returns whether it changed.
pub fn sync_list<'a, T: PartialEq + 'a>(
    list: &mut Vec<Arc<T>>,
    items: impl IntoIterator<Item = &'a Arc<T>>,
) -> bool {
    sync_with(
        list,
        items,
        |held, item| Arc::ptr_eq(held, item) || **held == ***item,
        Arc::clone,
    )
}

/// [`sync_list`] for items made afresh: the list keeps what it held where
/// that's equal to the new item, so whoever shares it goes on sharing it.
pub fn share_list<T: PartialEq>(
    list: &mut Vec<Arc<T>>,
    items: impl IntoIterator<Item = T>,
) -> bool {
    sync_with(list, items, |held, item| **held == *item, Arc::new)
}

fn sync_with<T, I>(
    list: &mut Vec<Arc<T>>,
    items: impl IntoIterator<Item = I>,
    same: impl Fn(&Arc<T>, &I) -> bool,
    share: impl Fn(I) -> Arc<T>,
) -> bool {
    let mut changed = false;
    let mut count = 0;
    for item in items {
        match list.get_mut(count) {
            Some(held) if same(held, &item) => {}
            Some(held) => {
                *held = share(item);
                changed = true;
            }
            None => {
                list.push(share(item));
                changed = true;
            }
        }
        count += 1;
    }
    changed |= list.len() > count;
    list.truncate(count);
    // Growing a room at a time can leave room for twice as many.
    if list.capacity() > count + count / 4 {
        list.shrink_to_fit();
    }
    changed
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct VoicePerson {
    pub id: String,
    pub name: String,
    /// The address of their avatar; see [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
    /// Muted or deafened, by themselves or by a Discord server.
    pub muted: bool,
    pub deafened: bool,
    /// Muted or deafened by the Discord server, which shows in red.
    pub server_muted: bool,
    pub server_deafened: bool,
    /// Sharing their screen.
    pub streaming: bool,
    /// Their camera is on.
    pub video: bool,
    /// This account, in the call on another client, as Discord shows it:
    /// dimmed, without its icons.
    pub elsewhere: bool,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ChatServer {
    pub id: String,
    pub name: String,
    pub channels: Vec<String>,
    /// A Discord server, reached through the linked Discord account.
    pub discord: bool,
    /// Its unread messages don't mark it in the rail; mentions still do.
    pub muted: bool,
    /// Out of reach in a Discord outage, shown as last known until it's back.
    pub unavailable: bool,
    /// The address of the server's icon; see [`DiscordCommand::Pictures`].
    pub icon: Option<String>,
    /// The folder it sits in on the rail. A folder's servers come one after
    /// another in [`RoomIndex::servers`].
    pub folder: Option<ServerFolder>,
    /// Its muted channels stay out of the channel list, as this account's
    /// Discord notification settings for it say.
    pub hide_muted: bool,
    /// A Discord Community server, whose channel list opens Browse
    /// Channels.
    pub community: bool,
    /// The room ids of the channels this account pinned to the top of its
    /// channel list, a Discord server's, in the order they show there.
    pub pinned: Vec<String>,
}

/// Where a Discord channel or category goes in its server's channel list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelPlacement {
    pub room: String,
    /// Its position among those it lists with.
    pub position: i64,
    /// For a channel moving to another category, that category's room id,
    /// none for none.
    pub parent: Option<Option<String>>,
}

/// A Discord server's role, as a roles menu lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerRole {
    pub id: String,
    pub name: String,
    /// Higher roles list first, and outrank lower ones.
    pub position: i64,
    /// Its RGB colour, if it has one.
    pub color: Option<u32>,
    /// An integration such as a bot or boosting gives it; nobody grants or
    /// takes it by hand.
    pub managed: bool,
}

/// What a Discord server's roles may let this account do, as Discord names
/// it: [`crate::application::Application::may`] asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    CreateInvite,
    Kick,
    Ban,
    /// Time members out.
    Moderate,
    ManageRoles,
    ManageChannels,
    ManageServer,
    /// Change its own nickname.
    ChangeNickname,
    ManageNicknames,
    ViewChannel,
    SendMessages,
    /// Read a channel's earlier messages, which its list of threads needs
    /// for the archived ones and its search, and starting a public thread.
    ReadHistory,
    CreatePublicThreads,
    CreatePrivateThreads,
    /// Remove others' reactions, among more.
    ManageMessages,
    /// Post polls, besides sending messages.
    SendPolls,
    /// Manage threads and forum posts, and give posts a forum's moderated
    /// tags.
    ManageThreads,
    /// Join a voice channel.
    Connect,
    /// Any permission that opens the server's settings: Manage Server,
    /// Manage Roles, Ban Members and more, as Discord's client has them.
    Settings,
    /// Mute, deafen, and move or disconnect members in voice channels.
    MuteMembers,
    DeafenMembers,
    MoveMembers,
    ViewAuditLog,
    /// Change and delete any of a server's emoji and stickers, and add
    /// them and change one's own.
    ManageExpressions,
    CreateExpressions,
}

/// Which messages notify, in a Discord server or channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyLevel {
    All,
    Mentions,
    Nothing,
}

/// How a Discord server, channel or category notifies this account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Notifications {
    /// Muted itself, at least till `muted_until`. A channel in a muted
    /// category shows muted too ([`ChatRoom::muted`]), but isn't itself.
    pub muted: bool,
    /// When the mute ends, in Unix milliseconds; none while it lasts until
    /// unmuted.
    pub muted_until: Option<u64>,
    /// Its own level; none to follow its category, or for a server, the
    /// server's default.
    pub level: Option<NotifyLevel>,
    /// The level that applies when `level` is none, as a menu's subtitle
    /// shows it.
    pub inherited: NotifyLevel,
}

/// How a Discord server notifies this account, and its switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerNotifications {
    pub own: Notifications,
    /// @everyone and @here don't call on this account.
    pub suppress_everyone: bool,
    /// Role mentions don't call on this account.
    pub suppress_roles: bool,
    /// See [`ChatServer::hide_muted`].
    pub hide_muted: bool,
    /// Its channel list shows every channel, not only those this account
    /// follows; none where it can't choose, outside a Community server.
    pub show_all: Option<bool>,
}

/// A change to a Discord server's, channel's or category's notification
/// settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationChange {
    Mute(Muting),
    /// Which messages notify; none to follow the category's or the server's.
    Level(Option<NotifyLevel>),
    /// The rest are a server's alone.
    HideMuted(bool),
    SuppressEveryone(bool),
    SuppressRoles(bool),
}

/// What a Discord channel is: a text or voice channel, or a category.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChannelKind {
    #[default]
    Text,
    Voice,
    Category,
}

/// What a Discord channel's dialog sets: its name, and for a text channel
/// its topic, how many seconds its slowmode makes each person wait between
/// messages, and whether it's age-restricted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChannelSettings {
    pub name: String,
    pub topic: String,
    pub slowmode: u32,
    pub nsfw: bool,
}

/// A folder of servers on the rail, as Discord's server list keeps them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServerFolder {
    /// Unique across networks, such as `discord-folder:7`.
    pub id: String,
    /// Its name; none when it has none, and the rail names it by its servers.
    pub name: Option<String>,
    /// Its colour as RGB; none for the default.
    pub color: Option<u32>,
}

/// A room's standing from its power levels: Matrix's admin is 100 and moderator 50.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Admin,
    Moderator,
    Member,
}

/// A person's profile, as their profile card shows it. Matrix gives what the
/// member list knows; Discord adds the rest.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    /// Their Discord user id or Matrix ID, as in [`ChatMember::id`].
    pub id: String,
    pub name: String,
    /// Their Discord username or Matrix ID.
    pub handle: String,
    /// The address of their avatar; see [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
    /// The address of their banner, and its colour when there's no picture.
    pub banner: Option<String>,
    pub accent: Option<u32>,
    pub pronouns: String,
    /// Their "About me", as Markdown.
    pub bio: String,
    pub presence: Option<Presence>,
    pub activity: Option<String>,
    pub activities: Activities,
    /// Their roles, highest first: Discord roles in this server, or Matrix
    /// room roles above member.
    pub roles: Vec<ProfileRole>,
    /// When they made their Discord account, as a date.
    pub created: Option<String>,
    /// When they joined this server, as a date.
    pub joined: Option<String>,
    /// The servers and friends this account shares with them.
    pub servers: Vec<ProfileServer>,
    pub friends: Vec<ProfileFriend>,
}

/// A server a profile's person shares with this account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileServer {
    /// As in [`ChatServer::id`].
    pub id: String,
    pub name: String,
    /// The addresses of its icon and banner; see [`DiscordCommand::Pictures`].
    pub icon: Option<String>,
    pub banner: Option<String>,
    /// How many members it has, when known, and channels this account sees.
    pub members: Option<u64>,
    pub channels: usize,
    /// The person's nickname there, when they set one.
    pub nick: Option<String>,
}

/// A friend a profile's person shares with this account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileFriend {
    /// Their Discord user id.
    pub id: String,
    pub name: String,
    /// The address of their avatar; see [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileRole {
    pub name: String,
    /// Its RGB colour, for a coloured Discord role.
    pub color: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMember {
    pub id: String,
    /// The display name, or the user ID without one, as on the member's messages.
    pub name: String,
    /// Their Discord username, such as `ann`.
    pub username: Option<String>,
    pub role: Role,
    /// The address of their avatar; see [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
    /// The heading Discord lists them under, with its size, such as
    /// `Online — 12`. Matrix members are grouped by `role` instead.
    pub group: Option<String>,
    /// Whether they're around; Matrix members have no presence here.
    pub presence: Option<Presence>,
    /// The RGB colour of their name, from their highest coloured Discord role.
    pub color: Option<u32>,
    /// Their custom status, else what they're doing, from Discord, and
    /// what they're doing in full.
    pub activity: Option<String>,
    pub activities: Activities,
    /// Their Discord roles' ids, as far as they're known, shared with what
    /// learned them; empty for Matrix. See [`crate::application::Application::server_roles`].
    pub roles: Arc<[String]>,
    /// When their Discord timeout ends, in Unix milliseconds, while they're
    /// timed out as far as Yapfast knows.
    pub timed_out_until: Option<u64>,
}

/// Someone to start a DM with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contact {
    /// Their Discord or Matrix user id.
    pub id: String,
    pub name: String,
    /// The address of their avatar; see [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
    /// Who they are to this account, such as "Discord friend", or their Matrix id.
    pub detail: String,
    pub discord: bool,
}

/// A role or group a message can mention, such as a Discord role or @everyone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionGroup {
    pub name: String,
    /// How a message says it on its network, such as `<@&20>` or `@everyone`.
    pub mention: String,
    /// The RGB colour of the role.
    pub color: Option<u32>,
}

/// Whether a Discord member is around, as their status dot shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Presence {
    Online,
    /// Online, and only on a phone.
    Mobile,
    Idle,
    DoNotDisturb,
    Offline,
}

pub struct RoomIndex {
    pub rooms: Vec<ChatRoom>,
    pub servers: Vec<ChatServer>,
}

/// Where a message is in the SDK's persistent send queue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Delivery {
    #[default]
    Sent,
    Sending,
    /// A connection problem paused the queue; Yapfast sends it when the connection returns.
    Waiting,
    /// The server refused it; it stays until the user retries or cancels it.
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub author: String,
    /// The address of the author's avatar, where the network gives one; see
    /// [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
    pub body: String,
    /// The original message's item, to jump to it; none when it's gone.
    pub item: Option<String>,
}

/// A Discord thread a message leads to: the one started from it, or that
/// its notice names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadLink {
    /// The thread's room id, which [`crate::application::Action::Open`] opens.
    pub room: String,
    pub name: String,
    /// Its messages after the first, when Yapfast knows.
    pub messages: Option<u64>,
    pub archived: bool,
}

/// A Discord thread as a channel's list of its threads shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ListedThread {
    /// Its room id, which [`crate::application::Action::Open`] opens.
    pub room: String,
    pub name: String,
    pub private: bool,
    /// This account is in it.
    pub joined: bool,
    pub archived: bool,
    /// The snowflake of its newest message, else its own: lists order by it.
    pub last: u64,
    /// When it was last active, in Unix milliseconds.
    pub active_ms: u64,
    /// Who started it, as an archived thread's row says.
    pub owner: Option<ThreadPerson>,
    /// Its newest message, as an active thread's row shows it.
    pub recent: Option<RecentMessage>,
    /// How many are in it, as Discord counts them: up to 50.
    pub members: u32,
    /// The avatars of a few of them.
    pub member_avatars: Vec<String>,
    /// A forum post's tags, by id, and whether it's pinned to the top.
    pub tags: Vec<String>,
    pub pinned: bool,
    /// Its messages after the first, as Discord counts them.
    pub replies: u64,
    /// This account opened it, and has messages there to read.
    pub opened: bool,
    pub unread: bool,
    /// When it started, in Unix milliseconds.
    pub created_ms: u64,
}

/// A forum post's first message, as its card shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PostStarter {
    pub author: ThreadPerson,
    /// Its text on one line, mentions and custom emoji as text.
    pub text: String,
    /// The reaction most given, and how many gave it.
    pub reaction: Option<(ShownEmoji, u64)>,
    /// The address of its first picture, as a thumbnail, and whether it's a
    /// spoiler.
    pub picture: Option<String>,
    pub spoiler: bool,
}

/// Forum posts' first messages, by room id; none where it was deleted.
pub type PostStarters = Vec<(String, Option<PostStarter>)>;

/// An emoji as a forum shows it on a tag or a reaction: a Unicode one's
/// text, or the address of a custom one's picture, with its name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ShownEmoji {
    pub text: String,
    pub picture: Option<String>,
}

/// A tag a Discord forum's posts may take.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ForumTag {
    pub id: String,
    pub name: String,
    pub emoji: Option<ShownEmoji>,
    /// Only those who manage threads may give it.
    pub moderated: bool,
}

/// Someone a thread's row names.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ThreadPerson {
    pub name: String,
    pub avatar: String,
    /// Their role colour, when they have one.
    pub color: Option<u32>,
}

/// A thread's newest message, as its row shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RecentMessage {
    pub author: ThreadPerson,
    /// Its text on one line, mentions and custom emoji as text.
    pub text: String,
}

/// A Discord custom emoji, which shows as `text` until its picture arrives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomEmoji {
    /// Its name between colons, such as `:pog:`, as message bodies and
    /// reaction keys show it.
    pub text: String,
    pub id: String,
    pub animated: bool,
    /// The address of its picture; see [`DiscordCommand::Pictures`].
    pub picture: String,
}

/// A message a search found, and the room it's in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchHit {
    pub room: String,
    pub message: ChatMessage,
}

/// A Discord channel's topic, as its header shows it: Markdown with
/// mentions by name, as a message's body is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Topic {
    pub body: String,
    pub mentions: Vec<crate::mention::Mentioned>,
    pub emoji: Vec<CustomEmoji>,
}

/// The size custom emoji pictures are downloaded at: enough for text's emoji on
/// a doubled display, and for a message of nothing but emoji.
pub const EMOJI_PICTURE_SIZE: u32 = 64;

/// A server's custom emoji that this account may use where it's writing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmojiSet {
    pub server: String,
    /// The address of the server's icon; see [`ChatServer::icon`].
    pub icon: Option<String>,
    pub emoji: Vec<CustomEmoji>,
}

/// `text` with each `:name:` of a usable custom emoji, outside code, in Discord's
/// form, `<:name:id>`. The first emoji of a name wins.
pub fn wire_emoji(text: &str, usable: &[CustomEmoji]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut code = false;
    let mut rest = text;
    while let Some(next) = rest.chars().next() {
        if next == '`' {
            code = !code;
        }
        if !code
            && next == ':'
            && !out.ends_with('<')
            && !out.ends_with("<a")
            && let Some(end) = rest[1..].find(':')
            && let name = &rest[1..1 + end]
            && name.len() >= 2
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && let Some(emoji) = usable
                .iter()
                .find(|emoji| emoji.text[1..emoji.text.len() - 1] == *name)
        {
            let animated = if emoji.animated { "a" } else { "" };
            out.push_str(&format!("<{animated}:{name}:{}>", emoji.id));
            rest = &rest[end + 2..];
            continue;
        }
        out.push(next);
        rest = &rest[next.len_utf8()..];
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reaction {
    pub key: String,
    pub count: usize,
    pub mine: bool,
}

/// Someone who reacted to a message, as a list of them shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reactor {
    /// Their user id.
    pub id: String,
    /// Their name where the message is.
    pub name: String,
    /// Their username, or Matrix id; empty when it's their name.
    pub username: String,
    /// Their avatar's address; empty for none.
    pub avatar: String,
    /// It's this account.
    pub me: bool,
}

/// A decoded, size-limited preview. Yapfast never hands undecoded media to the UI.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// The picture, or an animation's first frame.
    pub rgba: Pixels,
    /// What moves; none for a still.
    pub animation: Option<Animation>,
}

/// A picture's RGBA pixels. Once the UI has said how to make its own buffers
/// ([`set_pixel_buffers`]), they're made in one, which it shows without a
/// copy; until then, and without a UI, they're a plain vector.
pub struct Pixels(Box<dyn PixelBuffer>);

/// Memory a picture's RGBA pixels live in.
pub trait PixelBuffer: std::any::Any + Send + Sync {
    fn bytes(&self) -> &[u8];
}

impl PixelBuffer for Vec<u8> {
    fn bytes(&self) -> &[u8] {
        self
    }
}

/// Makes a buffer for a `width` by `height` picture holding `rgba`.
pub type MakePixels = fn(u32, u32, &[u8]) -> Box<dyn PixelBuffer>;

static MAKE_PIXELS: std::sync::OnceLock<MakePixels> = std::sync::OnceLock::new();

/// Makes every picture's pixels with `make` from now on.
pub fn set_pixel_buffers(make: MakePixels) {
    let _ = MAKE_PIXELS.set(make);
}

impl Pixels {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        match MAKE_PIXELS.get() {
            Some(make) => Self(make(width, height, &rgba)),
            None => Self(Box::new(rgba)),
        }
    }

    /// The buffer the pixels are in, when it's a `T`.
    pub fn buffer<T: PixelBuffer>(&self) -> Option<&T> {
        (self.0.as_ref() as &dyn std::any::Any).downcast_ref()
    }
}

impl Default for Pixels {
    fn default() -> Self {
        Self(Box::new(Vec::new()))
    }
}

impl std::ops::Deref for Pixels {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.0.bytes()
    }
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Pixels({} bytes)", self.len())
    }
}

impl PartialEq for Pixels {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl Eq for Pixels {}

/// An animation, kept as its file and decoded a frame at a time as it shows:
/// decoded whole, a few seconds of a GIF take tens of megabytes.
#[derive(Debug, PartialEq, Eq)]
pub struct Animation {
    /// How long each frame shows, in milliseconds.
    pub delays: Vec<u32>,
    file: AnimationFile,
    format: AnimationFormat,
}

/// Where an animation's file is: in memory, or on disk, where Yapfast keeps
/// the pictures it downloaded, and where it takes no memory.
#[derive(Debug, PartialEq, Eq)]
enum AnimationFile {
    Memory(Arc<[u8]>),
    Disk(PathBuf),
    /// As Discord's cache keeps it, perhaps encrypted.
    #[cfg(feature = "discord")]
    Kept(yapfast_discord::cache::KeptPicture),
}

impl AnimationFile {
    /// `data`, or `kept` when that's its file.
    fn of(data: &[u8], kept: Option<&std::path::Path>) -> Self {
        kept.map_or_else(
            || Self::Memory(Arc::from(data)),
            |file| Self::Disk(file.to_owned()),
        )
    }

    fn open(&self) -> Option<AnimationReader> {
        match self {
            Self::Memory(data) => Some(AnimationReader::Memory(std::io::Cursor::new(data.clone()))),
            Self::Disk(path) => std::fs::File::open(path)
                .inspect_err(|error| tracing::debug!(%error, "could not read an animation"))
                .ok()
                .map(|file| AnimationReader::Disk(std::io::BufReader::new(file))),
            #[cfg(feature = "discord")]
            Self::Kept(kept) => kept
                .open()
                .inspect_err(|error| tracing::debug!(%error, "could not read an animation"))
                .ok()
                .map(|file| AnimationReader::Kept(std::io::BufReader::new(file))),
        }
    }
}

/// An animation's file, read from the start.
pub(crate) enum AnimationReader {
    Memory(std::io::Cursor<Arc<[u8]>>),
    Disk(std::io::BufReader<std::fs::File>),
    #[cfg(feature = "discord")]
    Kept(std::io::BufReader<yapfast_discord::cache::KeptReader>),
}

impl std::io::Read for AnimationReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Memory(reader) => reader.read(buffer),
            Self::Disk(reader) => reader.read(buffer),
            #[cfg(feature = "discord")]
            Self::Kept(reader) => reader.read(buffer),
        }
    }
}

impl std::io::BufRead for AnimationReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        match self {
            Self::Memory(reader) => reader.fill_buf(),
            Self::Disk(reader) => reader.fill_buf(),
            #[cfg(feature = "discord")]
            Self::Kept(reader) => reader.fill_buf(),
        }
    }

    fn consume(&mut self, amount: usize) {
        match self {
            Self::Memory(reader) => reader.consume(amount),
            Self::Disk(reader) => reader.consume(amount),
            #[cfg(feature = "discord")]
            Self::Kept(reader) => reader.consume(amount),
        }
    }
}

impl std::io::Seek for AnimationReader {
    fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
        match self {
            Self::Memory(reader) => reader.seek(to),
            Self::Disk(reader) => reader.seek(to),
            #[cfg(feature = "discord")]
            Self::Kept(reader) => reader.seek(to),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnimationFormat {
    Gif,
    Png,
    WebP,
    #[cfg(feature = "discord")]
    Video,
}

impl Picture {
    /// An animation's frames from the first, as RGBA at the picture's size,
    /// each decoded as it's taken; none for a still.
    pub fn frames(&self) -> Box<dyn Iterator<Item = Vec<u8>>> {
        let Some(animation) = &self.animation else {
            return Box::new(std::iter::empty());
        };
        let (width, height) = (self.width, self.height);
        let Some(file) = animation.file.open() else {
            return Box::new(std::iter::empty());
        };
        let frames: Box<dyn Iterator<Item = Vec<u8>>> = match animation.format {
            #[cfg(feature = "discord")]
            AnimationFormat::Video => crate::video::frames(file, width, height),
            format => match image_frames(file, format) {
                Some((_, frames)) => Box::new(
                    frames
                        .map_while(Result::ok)
                        .map(move |frame| shrink(frame.into_buffer(), width, height)),
                ),
                None => Box::new(std::iter::empty()),
            },
        };
        Box::new(frames.take(animation.delays.len()))
    }

    /// Reads an animation's frames from `kept` from now on, rather than
    /// keeping its bytes in memory: `kept` is the picture it was decoded
    /// from, kept on disk. One of another size isn't, and is left alone.
    #[cfg(feature = "discord")]
    pub fn read_from(&mut self, kept: yapfast_discord::cache::KeptPicture) {
        let Some(animation) = &mut self.animation else {
            return;
        };
        let AnimationFile::Memory(data) = &animation.file else {
            return;
        };
        if kept.length() == Some(data.len() as u64) {
            animation.file = AnimationFile::Kept(kept);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    pub size: Option<u64>,
    pub image: bool,
    /// The full image's width and height, when the sender said.
    pub dimensions: Option<(u32, u32)>,
    pub preview: Option<Arc<Picture>>,
    /// The preview could not be downloaded or decoded; the file can still be saved.
    pub preview_failed: bool,
    pub placement: Placement,
    /// Marked as a spoiler: its picture is covered until clicked.
    pub spoiler: bool,
    /// A video that plays in place of its picture, which is its poster:
    /// [`Command::PlayVideo`].
    pub video: bool,
    /// A GIF that can be a favourite: a linked one, or an attached `.gif`.
    pub gif: Option<Gif>,
    /// Its address on the web, which Copy Link and Open Link give, and from
    /// which [`Command::CopyAttachment`] copies its picture. Only Discord's
    /// attachments have one.
    pub url: Option<Arc<str>>,
}

/// What [`Command::Video`] tells the video playing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VideoCommand {
    /// Play, from the start once it ended.
    Play,
    Pause,
    /// Move to this many milliseconds in.
    Seek(u64),
    /// Draw it to fit this many pixels, as when the window grows.
    Size {
        width: u32,
        height: u32,
    },
    Mute(bool),
    /// Play it this loud, from 0 to 1 as the slider stands, unmuted; 0
    /// mutes it, keeping the level it had.
    Volume(f32),
    Stop,
}

/// The video playing in the conversation view.
#[derive(Clone, Debug)]
pub struct Video {
    pub room: String,
    pub item: String,
    /// Its attachment's index in the item's attachments.
    pub index: usize,
    /// Its pictures, while it plays.
    pub frames: Option<Arc<crate::frames::Frames>>,
    /// How long it plays, once the file says.
    pub duration_ms: Option<u64>,
    /// Its pictures' full width and height, once its file opens.
    pub size: Option<(u32, u32)>,
    /// Until its file opens.
    pub loading: bool,
    pub playing: bool,
    /// It played to its end.
    pub ended: bool,
    /// It has sound that plays.
    pub sound: bool,
    pub muted: bool,
    /// How loud it plays unmuted, from 0 to 1 as the slider stands.
    pub volume: f32,
    /// Why it stopped, for the person.
    pub error: Option<String>,
}

/// Where an attachment's picture shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    /// With the message's files and images.
    #[default]
    Message,
    /// Large, in the message's embed at this index.
    Embed(usize),
    /// Small, beside the text of the embed at this index.
    Thumbnail(usize),
    /// On its own, without a frame: a Discord sticker.
    Sticker,
    /// In the Discord component row with this slot: [`CardRow::Media`], or
    /// beside a section's text, [`Beside::Thumbnail`].
    Component(usize),
}

/// A Discord link preview or bot card. Empty texts are left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Embed {
    /// The RGB colour of the bar down its side.
    pub color: Option<u32>,
    /// The site, such as "YouTube".
    pub provider: String,
    pub author: String,
    pub title: String,
    /// Where the title leads.
    pub url: String,
    /// Markdown.
    pub description: String,
    pub fields: Vec<EmbedField>,
    pub footer: String,
    /// Its large picture is a video's poster, which opens `url`.
    pub video: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbedField {
    pub name: String,
    /// Markdown.
    pub value: String,
    /// Shares a row with the inline fields beside it.
    pub inline: bool,
}

/// A Discord message's container of components, or a run of components
/// outside one, which shows without a frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Card {
    pub framed: bool,
    /// The RGB colour of a container's accent bar.
    pub color: Option<u32>,
    pub rows: Vec<CardRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardRow {
    /// Markdown, and what a section shows beside it.
    Text {
        text: String,
        beside: Option<Beside>,
    },
    /// The attachments placed [`Placement::Component`] with this slot: a
    /// gallery's pictures, or a file.
    Media(usize),
    /// A gap, and with `divider` a line across it.
    Separator {
        divider: bool,
        large: bool,
    },
    Buttons(Vec<CardButton>),
    /// A menu, by what it shows before it's opened.
    Select {
        text: String,
        disabled: bool,
    },
    /// Components of a kind Yapfast can't show.
    Unknown,
}

/// What a section shows beside its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Beside {
    /// The picture placed [`Placement::Component`] with this slot.
    Thumbnail(usize),
    Button(CardButton),
}

/// An app's button. Only a link's does anything in Yapfast: the others
/// would ask the app, which Yapfast can't yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardButton {
    /// Its emoji, a custom one by `:name:`, then its label.
    pub label: String,
    pub style: ButtonStyle,
    /// Where a link button leads.
    pub url: Option<String>,
    pub disabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    Success,
    Danger,
    Link,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatMessage {
    /// Stable row key across the local echo and its remote event.
    pub id: String,
    /// The timeline item that actions apply to: `e:` event ID or `t:` transaction ID.
    pub item: String,
    pub author: String,
    /// The author's Matrix user ID, which tells apart people sharing a display name.
    pub sender: String,
    /// Its text hides what's between `||` until clicked, as Discord's does.
    pub spoilers: bool,
    /// The address of the author's avatar; see [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
    /// The RGB colour of the author's name, from their highest coloured
    /// Discord role.
    pub color: Option<u32>,
    /// The author is a Discord bot or webhook.
    pub bot: bool,
    /// Forwarded on Discord: its body, files and embeds are the original's.
    pub forwarded: bool,
    pub body: String,
    /// The body is Markdown source whose sender also sent formatting.
    pub formatted: bool,
    pub time: String,
    /// Seconds since the Unix epoch; zero when unknown.
    pub timestamp: u64,
    pub own: bool,
    pub edited: bool,
    pub deleted: bool,
    /// From someone this account blocked: it shows collapsed until asked for.
    pub blocked: bool,
    /// Mentions this account or a group it's in, other than as a reply to
    /// its message. The timeline highlights it.
    pub mentions_me: bool,
    /// Replies to this account's message, which the timeline highlights
    /// while it's new; see [`Self::highlighted`].
    pub replies_to_me: bool,
    pub delivery: Delivery,
    pub reply: Option<Reply>,
    pub reactions: Vec<Reaction>,
    /// Files and images, in the order the sender attached them, then the
    /// pictures of `embeds`. Matrix messages carry at most one.
    pub attachments: Vec<Attachment>,
    pub embeds: Vec<Embed>,
    /// A Discord app's buttons and menus, or for a message made of
    /// components, all it shows.
    pub cards: Vec<Card>,
    /// Who and what the body mentions, which it links to.
    pub mentions: Vec<crate::mention::Mentioned>,
    /// The Discord custom emoji its body and reactions show.
    pub emoji: Vec<CustomEmoji>,
    pub can_reply: bool,
    pub can_edit: bool,
    pub can_delete: bool,
    pub can_react: bool,
    /// A sent message whose content can be sent again to another conversation.
    pub can_forward: bool,
    /// Among its conversation's pinned messages.
    pub pinned: bool,
    /// This account may pin it, or unpin it: [`Command::Pin`].
    pub can_pin: bool,
    /// The Discord thread it started, or that it tells of.
    pub thread: Option<ThreadLink>,
    /// The Discord poll it asks.
    pub poll: Option<Poll>,
    /// What it tells of, when it's a notice in place of a message, such as
    /// Discord's that someone pinned one; its body says it after its
    /// author's name.
    pub notice: Option<Notice>,
}

/// A notice in place of a message, as Discord's client marks it: with an
/// icon, its author's name, and what it tells of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    /// What follows the author's name, such as "joined the server."
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    /// Its author pinned a message, by item, which it links to.
    Pinned(Option<String>),
    Joined,
    ThreadStarted,
    Call,
    PollResult,
}

/// A Discord poll as a message shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Poll {
    pub question: String,
    pub answers: Vec<PollAnswer>,
    /// When it closes, in seconds since the Unix epoch.
    pub expiry: Option<u64>,
    /// It takes votes no more: Discord counted them for the last time.
    pub finalized: bool,
    /// One may pick several answers.
    pub multiselect: bool,
    /// This account may close it now: its author.
    pub can_end: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PollAnswer {
    pub id: u64,
    pub text: String,
    /// Its emoji, as a reaction's key: the emoji itself, or `:name:` for a
    /// custom one, among the message's [`ChatMessage::emoji`].
    pub emoji: Option<String>,
    pub votes: usize,
    pub mine: bool,
}

/// A Discord poll to post.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewPoll {
    pub question: String,
    pub answers: Vec<NewPollAnswer>,
    /// How long it takes votes, in hours.
    pub hours: u32,
    pub multiselect: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewPollAnswer {
    pub text: String,
    /// The emoji itself, or a custom one's `name:id`, as a reaction takes it.
    pub emoji: Option<String>,
}

impl Poll {
    /// Whether it takes votes no more at `now`, in seconds since the Unix epoch.
    pub fn closed(&self, now: u64) -> bool {
        self.finalized || self.expiry.is_some_and(|expiry| expiry <= now)
    }

    /// This account voted.
    pub fn voted(&self) -> bool {
        self.answers.iter().any(|answer| answer.mine)
    }
}

impl ChatMessage {
    /// Whether the timeline highlights it: a mention always, as Discord's
    /// client does, and a reply to this account's message while it's `new`.
    pub fn highlighted(&self, new: bool) -> bool {
        self.mentions_me || (self.replies_to_me && new)
    }
}

/// Where a timeline's new messages start: the index of the first after
/// `seen`, the message last read when it opened or its newest then. A
/// Discord message is after it by its snowflake, which holds even when
/// `seen` isn't loaded, and a Matrix one by its place. It's past the end
/// while `seen` isn't known, or the timeline doesn't have it.
pub fn first_new(messages: &[Arc<ChatMessage>], seen: Option<&str>) -> usize {
    let Some(seen) = seen else {
        return messages.len();
    };
    match crate::discord::item_order(seen) {
        Some(seen) => messages.iter().position(|message| {
            crate::discord::item_order(&message.item).is_some_and(|order| order > seen)
        }),
        None => messages
            .iter()
            .position(|message| message.item == seen)
            .map(|at| at + 1),
    }
    .unwrap_or(messages.len())
}

/// How loading history ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum History {
    /// It's in the timeline. With `start` the room's first message is
    /// loaded, so there's nothing before it; with `present` its newest is,
    /// so the timeline follows live messages. Without `present` it's a
    /// window of older messages, which live ones don't go in.
    Loaded { start: bool, present: bool },
    /// It failed, and [`Event::RoomError`] said why.
    Failed,
}

/// Which history a page loaded; see [`Event::HistoryLoaded`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// [`Command::Earlier`].
    Earlier,
    /// [`Command::Later`].
    Later,
    /// [`Command::Around`].
    Around,
    /// The newest page of a Discord channel, as it opens or loads again:
    /// what shows of it by then is all there is.
    Latest,
}

pub enum Start {
    Restore,
    Login {
        homeserver: String,
        username: String,
        password: String,
    },
}

/// How a message goes, besides what it says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SendOptions {
    /// A reply that doesn't notify the person it replies to.
    pub quiet_reply: bool,
    /// A message that notifies nobody, as Discord's `@silent` sends it.
    pub silent: bool,
}

/// What the UI asks of the networks and voice. [`Command::route`] decides
/// which of them carries out each one.
#[derive(Clone)]
pub enum Command {
    Voice(voice::Command),
    /// For the linked Discord account alone.
    Discord(DiscordCommand),
    /// Show a room. Every network hears it, so each closes its own open room
    /// when another network's room opens.
    Open(String),
    /// Show a Discord thread, by room id, beside the open room, as Discord
    /// opens a thread from its channel; or with none, stop. Its timeline
    /// comes as the open room's does.
    Aside(Option<String>),
    /// Close the open room, on every network.
    CloseRoom,
    /// Queue a Markdown message, optionally replying to an item, with the
    /// files at `files`. Matrix sends each file as its own message, then the
    /// text.
    Send {
        room: String,
        body: String,
        reply: Option<String>,
        options: SendOptions,
        files: Vec<OutgoingFile>,
    },
    Edit {
        room: String,
        item: String,
        body: String,
    },
    /// Forward an item of the open room to `target`, then send `note` there
    /// as a message of its own, if any. On Discord, `silent` notifies
    /// nobody of either, as `@silent` does.
    Forward {
        room: String,
        item: String,
        target: String,
        note: Option<String>,
        silent: bool,
    },
    React {
        room: String,
        item: String,
        key: String,
    },
    /// Who reacted to an item with `key`: up to `limit` of them, after the
    /// one with id `after`. [`Event::Reactors`] follows.
    Reactors {
        room: String,
        item: String,
        key: String,
        after: Option<String>,
        limit: u32,
    },
    /// Vote for `answers` of a Discord poll in place of this account's vote;
    /// none takes it back.
    Vote {
        room: String,
        item: String,
        answers: Vec<u64>,
    },
    /// Post a poll in a Discord conversation. [`Event::PollPosted`] follows.
    SendPoll {
        room: String,
        poll: NewPoll,
    },
    /// Close this account's Discord poll now.
    EndPoll {
        room: String,
        item: String,
    },
    /// Who voted for `answer` of a Discord poll, as [`Command::Reactors`]
    /// asks who reacted. [`Event::Reactors`] follows, with the answer's id
    /// as its key.
    Voters {
        room: String,
        item: String,
        answer: u64,
        after: Option<String>,
        limit: u32,
    },
    /// Remove someone else's reaction to an item, as a Discord moderator.
    RemoveReactor {
        room: String,
        item: String,
        key: String,
        user: String,
    },
    Delete {
        room: String,
        item: String,
    },
    /// Pin a Discord message to its channel's pinned messages, or unpin it.
    Pin {
        room: String,
        item: String,
        pinned: bool,
    },
    Retry {
        room: String,
        item: String,
    },
    Cancel {
        room: String,
        item: String,
    },
    /// Save the attachment at `index` in an item's attachments.
    SaveAttachment {
        room: String,
        item: String,
        index: usize,
        path: PathBuf,
    },
    /// Download the picture of the attachment at `index` in an item's
    /// attachments, for the clipboard; it arrives as [`Event::Copied`].
    CopyAttachment {
        room: String,
        item: String,
        index: usize,
    },
    /// Download an image attachment at viewing size; it arrives as
    /// [`Event::Image`].
    ViewImage {
        room: String,
        item: String,
        index: usize,
    },
    /// Play an attachment's video where its poster shows, at most `width`
    /// by `height` pixels, on `speakers`, stopping any other; it goes as
    /// [`Event::Video`].
    PlayVideo {
        room: String,
        item: String,
        index: usize,
        width: u32,
        height: u32,
        speakers: Option<String>,
    },
    /// Control the video playing.
    Video(VideoCommand),
    /// Look for people to start a DM with; each network answers with
    /// [`Event::People`].
    FindPeople {
        query: String,
    },
    /// Open the DM with someone [`Event::People`] found, starting it if there's none;
    /// [`Event::DmStarted`] follows.
    StartDm {
        id: String,
        discord: bool,
    },
    /// This account is typing in a room, or stopped. Backends tell others no more
    /// often than their network expects.
    Typing {
        room: String,
        typing: bool,
    },
    /// Mark a room read up to its newest message. Matrix sends a read
    /// receipt; Discord marks it read in Yapfast, and on Discord with `sync`.
    MarkRead {
        room: String,
        sync: bool,
    },
    /// Store a room's composer text in the encrypted session store; empty text clears it.
    SaveDraft {
        room: String,
        body: String,
    },
    /// Load the page of history before the room's oldest loaded message;
    /// [`Event::HistoryLoaded`] follows. A backend loads one page at a time
    /// per room, and answers at once when the first message is loaded.
    Earlier(String),
    /// Load the page after the newest message of a window of older
    /// messages, which [`Command::Around`] opened; [`Event::HistoryLoaded`]
    /// follows, and says when the window reached the newest message.
    Later(String),
    /// Show the messages around an item of the open room in place of its
    /// timeline: a window of older messages, unless it reaches the newest.
    /// [`Event::HistoryLoaded`] answers first; the timeline follows, the
    /// window or, when loading it failed, what was shown. Any page of
    /// history on its way is dropped.
    Around {
        room: String,
        item: String,
    },
    /// Show the room's newest messages again in place of a window of older
    /// ones, reusing those the backend kept where it can.
    Present(String),
    /// The timeline items in or near the viewport. Only their image previews
    /// need decoding; metadata for other loaded items stays available. While
    /// the timeline follows its newest message, `at_end`, a busy one lets go
    /// of its oldest; rows above go without moving it only then.
    VisibleMessages {
        room: String,
        items: Vec<String>,
        at_end: bool,
    },
    Recover(String),
    RefreshTrust,
    RetrySync,
    Logout,
}

/// A GIF Discord's picker found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Gif {
    /// The provider's id, which a pick is reported by.
    pub id: Option<String>,
    /// Its page, which a message sends and favourites are kept by.
    pub url: String,
    /// Its picture.
    pub src: String,
    /// The same picture as a `.gif`.
    pub gif_src: String,
    pub width: u32,
    pub height: u32,
}

/// A trending search term in the GIF picker, with a GIF of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GifCategory {
    pub name: String,
    pub src: String,
}

/// How often a Discord server or channel was used: in all, and when lately,
/// in Unix milliseconds, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaceUses {
    pub total: u32,
    pub recent: Vec<u64>,
}

/// A favourite GIF, as Discord's settings keep it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FavoriteGif {
    /// Its page, by which it's kept.
    pub url: String,
    /// The picture shown for it.
    pub src: String,
    /// The picture is a video rather than an image.
    pub video: bool,
    pub width: u32,
    pub height: u32,
    /// When it was added: one more than the newest before it.
    pub order: u32,
}

impl FavoriteGif {
    /// `gif` as a favourite after those `kept`, as Discord's client adds
    /// one: a video keeps its `.gif` instead, which every client shows.
    pub fn of(gif: &Gif, kept: &[Self]) -> Self {
        let video_src = gif.src.ends_with(".mp4") || gif.src.ends_with(".webm");
        let src = if video_src && !gif.gif_src.is_empty() && gif.gif_src != gif.src {
            gif.gif_src.clone()
        } else {
            gif.src.clone()
        };
        let image = [".webp", ".avif", ".gif"]
            .iter()
            .any(|extension| src.ends_with(extension));
        Self {
            url: gif.url.clone(),
            video: !image && video_src,
            src,
            width: gif.width,
            height: gif.height,
            order: kept.iter().map(|kept| kept.order).max().unwrap_or(0) + 1,
        }
    }
}

/// What an invite to a Discord server shows before joining by it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvitePreview {
    pub code: String,
    /// The server, by its server id, its name and the address of its icon.
    pub server: String,
    pub name: String,
    pub icon: Option<String>,
    /// The channel joining lands in, by its room id, its name, Discord's
    /// type of it, which joining names, and whether it's a voice channel.
    pub room: String,
    pub channel: String,
    pub channel_type: u64,
    pub voice: bool,
    /// How many members the server has, and how many are online, when
    /// Discord counts them.
    pub members: Option<u64>,
    pub online: Option<u64>,
}

/// A Discord server joined by an invite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinedServer {
    /// The server, by its server id, and the channel the invite lands in,
    /// by its room id.
    pub server: String,
    pub room: String,
    /// Whether the server asks new members to fill in its form, such as
    /// accepting its rules, before they may talk.
    pub form: bool,
}

/// What a Discord server's scheduled event shows from its link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventPreview {
    /// The event and its server, by their Discord ids.
    pub event: String,
    pub guild: String,
    pub name: String,
    /// What the event says about itself, when it says anything.
    pub description: Option<String>,
    /// When it starts and ends, in Unix seconds.
    pub starts_at: u64,
    pub ends_at: Option<u64>,
    /// The channel it happens in, by its room id, or where it happens
    /// when it happens somewhere else.
    pub room: Option<String>,
    pub location: Option<String>,
    /// Who made the event, when Discord says, and how many people are
    /// interested, when it counts them.
    pub creator: Option<String>,
    pub interested_count: Option<u64>,
    /// The event is going on now, or has ended or was canceled.
    pub active: bool,
    pub over: bool,
    /// This account is interested in the event.
    pub interested: bool,
}

/// What a Discord gift, such as Nitro, shows before it is redeemed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GiftPreview {
    /// What the gift grants, such as "Nitro", and the name of the
    /// subscription plan it grants, when it is one.
    pub name: String,
    pub plan: Option<String>,
    /// Who made the gift, by the name Discord shows for them, when it
    /// says.
    pub gifter: Option<String>,
    /// This account has redeemed the gift already.
    pub redeemed: bool,
    /// Every use of the gift has been redeemed, by anyone.
    pub spent: bool,
    /// Only Discord can redeem the gift: it needs a payment source, or
    /// it's from a promotion. Yapfast offers its link in the browser.
    pub in_discord: bool,
}

impl GiftPreview {
    /// Whether this account may still redeem the gift here.
    pub fn redeemable(&self) -> bool {
        !self.redeemed && !self.spent && !self.in_discord
    }
}

/// A server an application could be added to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OAuthGuild {
    /// The server, by its Discord id, its name and its icon's address.
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    /// This account may add applications to the server.
    pub can_add: bool,
}

/// What authorizing a Discord application from its link shows first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OAuthPreview {
    /// The link's query, which names the dialog the preview is for.
    /// Authorizing reads the link again, so it sends what this shows.
    pub query: String,
    /// The application, what it says about itself, and its icon.
    pub name: String,
    pub about: Option<String>,
    pub icon: Option<String>,
    /// The bot the link would add, when it adds one.
    pub bot: Option<String>,
    /// What each scope it asks for lets it do, in the words of Discord's
    /// authorization page, and a bot's permissions in the server.
    pub wants: Vec<&'static str>,
    pub permissions: Option<u64>,
    /// It asks to control the person's Discord client (the `rpc` scope).
    pub controls_client: bool,
    /// This account authorized it for these scopes before.
    pub authorized: bool,
    /// Where Discord sends the person afterwards, for a sign-in flow.
    pub redirect: Option<String>,
    /// It installs to a server, whose picking the link may lock to
    /// `wanted_guild`; the servers it could go to.
    pub guild_install: bool,
    pub guild_locked: bool,
    pub wanted_guild: Option<String>,
    pub guilds: Vec<OAuthGuild>,
}

/// Commands only the linked Discord account carries out.
#[derive(Clone)]
pub enum DiscordCommand {
    /// The GIF picker's home: trending terms with a GIF each, and a GIF for
    /// "Trending GIFs"; [`Event::GifCategories`] follows.
    GifCategories,
    /// GIFs for a search, or with `None` those trending; [`Event::Gifs`]
    /// follows.
    Gifs(Option<String>),
    /// Terms to search for instead of one; [`Event::GifSuggestions`] follows.
    GifSuggestions(String),
    /// Tell Discord's GIF provider a search for `query` led to the GIF `id`.
    GifPicked {
        id: String,
        query: String,
    },
    /// Fetch the account's favourite GIFs; [`Event::FavoriteGifs`] follows.
    FavoriteGifs,
    /// Fetch which servers and channels the account used, and when, as
    /// Discord's clients count them; [`Event::PlacesUsed`] follows.
    PlacesUsed,
    /// Keep these as the account's favourite GIFs, in place of the old.
    SetFavoriteGifs(Vec<FavoriteGif>),
    /// Download a GIF's picture and decode it to fit `width` by `height`;
    /// [`Event::GifPicture`] follows. A favourite's comes from a thumbnail
    /// kept on disk, made the first time.
    GifPicture {
        url: String,
        width: u32,
        height: u32,
        favorite: bool,
    },
    /// The members of a Discord role in the room's server, as far as they're known;
    /// [`Event::RoleMembers`] follows.
    RoleMembers {
        room: String,
        role: String,
    },
    /// Look for the room's members whose names start with `query`, beyond those
    /// listed; they arrive as [`Event::MemberMatches`]. Only Discord servers need it.
    SearchMembers {
        room: String,
        query: String,
    },
    /// Accept a Discord message request, or ignore it.
    AnswerRequest {
        room: String,
        accept: bool,
    },
    /// Decline a Discord DM call ringing this account.
    DeclineCall {
        room: String,
    },
    /// Send a Discord user a friend request or accept theirs, or with
    /// `add` false, ignore theirs or cancel this account's.
    Friend {
        user: String,
        add: bool,
    },
    /// Send a friend request to whoever has this Discord username;
    /// [`Event::FriendAdded`] follows.
    AddFriend {
        username: String,
    },
    /// Change how this account stands with a Discord user. The lists change
    /// when Discord confirms it; a failure comes back as [`Event::Error`].
    Relationship {
        user: String,
        change: RelationshipChange,
    },
    /// Fetch this account's note on a Discord user; [`Event::Note`] follows.
    Note {
        user: String,
    },
    /// Set this account's note on a Discord user, or the nickname it gives a
    /// friend; empty clears it. [`Event::PersonSaved`] follows.
    SetNote {
        user: String,
        note: String,
    },
    SetNickname {
        user: String,
        nickname: String,
    },
    /// Learn of a Discord thread, by room id, so it lists while it's open,
    /// in place of the open room or `beside` it: Discord tells a user
    /// account only of the threads it's in. [`Event::ThreadOpened`] follows.
    OpenThread {
        room: String,
        beside: bool,
    },
    /// Close a Discord DM; the DM list changes when Discord confirms it.
    CloseDm {
        room: String,
    },
    /// A Discord DM's or server channel's pinned messages; [`Event::Pins`]
    /// follows.
    Pins {
        room: String,
    },
    /// A page of a Discord server channel's threads, by its room id, as its
    /// list of threads shows them: the archived ones from the `offset`th,
    /// or with a `query`, those of any kind whose name it finds.
    /// [`Event::Threads`] follows.
    Threads {
        room: String,
        query: Option<String>,
        offset: u32,
    },
    /// The list of threads closed: forget what it found.
    CloseThreads,
    /// A Discord forum, by its room id, shows: hear of its posts as they
    /// come.
    ViewForum {
        room: String,
    },
    /// The first messages of a Discord forum's posts, by room id, as their
    /// cards show them, at most ten; [`Event::Posts`] follows.
    Posts {
        room: String,
        posts: Vec<String>,
    },
    /// Start a thread in a Discord server channel, by its room id: from its
    /// `message`, by item, or from none, a `private` one or not, from
    /// `location` in Discord's client, such as "Message".
    /// [`Event::ThreadStarted`] follows.
    StartThread {
        room: String,
        message: Option<String>,
        name: String,
        private: bool,
        location: &'static str,
    },
    /// Post in a Discord forum, by its room id: a post named `title` with
    /// the forum's tags by id, and a first message of `body` and `files`.
    /// [`Event::ForumPosted`] follows.
    PostInForum {
        room: String,
        title: String,
        tags: Vec<String>,
        body: String,
        files: Vec<OutgoingFile>,
    },
    /// Look for messages a search box's `query`, filters and all, finds: in
    /// a Discord DM, by its room id, or a server, by its server id; a page
    /// from the `offset`th found. [`Event::Found`] follows.
    Search {
        place: String,
        query: String,
        offset: u32,
    },
    /// Make a Discord group DM with these users, or with `room`, add them to
    /// that group; [`Event::GroupChanged`] follows.
    AddToGroup {
        room: Option<String>,
        users: Vec<String>,
    },
    /// Mute a Discord DM or group DM, or unmute it.
    Mute {
        room: String,
        muting: Muting,
    },
    /// Change this account's notification settings for a Discord server,
    /// by its server id, or a channel or category in it, by its room id.
    /// Snapshots show the change once Discord confirms it; a failure comes
    /// back as [`Event::Error`].
    Notifications {
        place: String,
        change: NotificationChange,
    },
    /// Join a Discord thread, by its room id, or leave it with `join`
    /// false, from the menu Discord's client names `location`; snapshots
    /// show it once Discord confirms it, and a failure comes back as
    /// [`Event::Error`].
    JoinThread {
        room: String,
        join: bool,
        location: &'static str,
    },
    /// Move a Discord server's channels and categories, by the server's id,
    /// as dragging them in its channel list does, locking the permissions
    /// of the channel `lock` names, by room id, to its new category's.
    /// Snapshots show them moved at once, and as they were again if Discord
    /// refuses, which comes back as [`Event::Error`].
    MoveChannels {
        server: String,
        placements: Vec<ChannelPlacement>,
        lock: Option<String>,
    },
    /// Set the flags of this account's override for a Discord server's
    /// channel, by its room id, which pins it to the top of the channel
    /// list or unpins it; snapshots show it once Discord confirms it, and a
    /// failure comes back as [`Event::Error`].
    PinChannel {
        room: String,
        flags: u64,
    },
    /// Set the flags of this account's override for a Discord server's
    /// channel or category, by its room id, which add it to the channel
    /// list or remove it; snapshots show it once Discord confirms it, and a
    /// failure comes back as [`Event::Error`].
    FollowChannel {
        room: String,
        flags: u64,
    },
    /// Set a Discord server's own flags in this account's settings for it,
    /// by its server id, which turn Show All Channels on or off, with the
    /// flags of the channels it `follow`s by id; snapshots show it once
    /// Discord confirms it, and a failure comes back as [`Event::Error`].
    ShowAllChannels {
        server: String,
        flags: u64,
        follow: Vec<(String, u64)>,
    },
    /// Make an invite to a Discord server's channel, by its room id, that
    /// lasts `max_age` seconds, 0 for good, and works `max_uses` times, 0
    /// for any number; [`Event::InviteMade`] follows.
    CreateInvite {
        room: String,
        max_age: u32,
        max_uses: u32,
    },
    /// Invite a Discord user to a server: make an invite to its channel,
    /// by its room id, that works once and lasts a day, and send its link
    /// in a message to their DM, opening the DM if need be;
    /// [`Event::InviteSent`] follows.
    SendInvite {
        user: String,
        room: String,
    },
    /// Look up an invite to a Discord server by its code, before joining
    /// by it; `typed` is what someone typed into Join a Server, if they
    /// did. [`Event::InviteResolved`] follows.
    ResolveInvite {
        code: String,
        typed: Option<String>,
    },
    /// Join a Discord server by the invite `preview` shows, from Join a
    /// Server, or from a clicked invite link with `from_link`.
    /// [`Event::JoinCaptcha`] follows for each captcha Discord asks for
    /// first, and then [`Event::ServerJoined`].
    JoinServer {
        // Boxed: it's the largest command by far.
        preview: Box<InvitePreview>,
        from_link: bool,
    },
    /// Look up a Discord server's scheduled event by its server and
    /// event ids, with whether this account is interested in it.
    /// [`Event::EventResolved`] follows.
    ResolveEvent {
        guild: String,
        event: String,
    },
    /// Mark this account interested in the scheduled event `preview`
    /// shows, or no longer interested in it.
    /// [`Event::EventInterestSet`] follows.
    SetEventInterest {
        preview: Box<EventPreview>,
        interested: bool,
    },
    /// Look up a Discord gift by its code, before offering to redeem
    /// it. [`Event::GiftResolved`] follows.
    ResolveGift {
        code: String,
    },
    /// Redeem the Discord gift with this code for this account, in the
    /// room `room` names when its link was opened in one.
    /// [`Event::GiftRedeemed`] follows.
    RedeemGift {
        code: String,
        room: Option<String>,
    },
    /// Look up what authorizing a Discord application from the link
    /// whose query this is would do. [`Event::OAuthResolved`] follows.
    ResolveOAuth {
        query: String,
    },
    /// Authorize the application the link whose query this is names,
    /// in the server by its Discord id when it installs to one.
    /// [`Event::OAuthAuthorized`] follows.
    AuthorizeOAuth {
        query: String,
        guild: Option<String>,
    },
    /// Make a channel or category in a Discord server, by its server id,
    /// inside the category `parent` names by its room id, or a copy of
    /// `copy_of`'s channel, by its room id, with these settings;
    /// [`Event::ChannelSaved`] follows with the new one's room id.
    CreateChannel {
        server: String,
        kind: ChannelKind,
        parent: Option<String>,
        copy_of: Option<String>,
        settings: Box<ChannelSettings>,
    },
    /// A Discord channel's or category's settings, by its room id, for its
    /// dialog; [`Event::ChannelLoaded`] follows.
    ChannelSettings {
        room: String,
    },
    /// Change a Discord channel's or category's settings, or delete it;
    /// [`Event::ChannelSaved`] follows.
    EditChannel {
        room: String,
        kind: ChannelKind,
        settings: Box<ChannelSettings>,
    },
    DeleteChannel {
        room: String,
    },
    /// Change a Discord channel's or category's overwrites, by its room id;
    /// [`Event::OverwritesChanged`] follows.
    ChangeOverwrites {
        room: String,
        change: OverwriteChange,
    },
    /// The names of members a channel's overwrites are for, by their user
    /// ids; [`Event::OverwriteNames`] follows.
    OverwriteNames {
        room: String,
        members: Vec<String>,
    },
    /// Leave a Discord server, by its server id; [`Event::Left`] follows.
    LeaveServer {
        server: String,
    },
    /// Leave a Discord group DM, by its room id, telling its other members
    /// or not; [`Event::Left`] follows, and its CHANNEL_DELETE takes it out
    /// of the DM list.
    LeaveGroup {
        room: String,
        silent: bool,
    },
    /// Delete a Discord server this account owns, confirming with
    /// two-factor authentication if Discord asks; [`Event::ServerDeleted`]
    /// follows.
    DeleteServer {
        server: String,
    },
    /// Give a Discord server this account owns to a member, by user id,
    /// with the code Discord emailed or none, confirming with two-factor
    /// authentication if Discord asks; [`Event::ServerTransferred`]
    /// follows.
    TransferServer {
        server: String,
        user: String,
        code: Option<String>,
    },
    /// Have Discord email the code that gives a server away;
    /// [`Event::ServerPincodeSent`] follows.
    SendServerPincode {
        server: String,
    },
    /// Set the Discord custom status, or clear it with `None`;
    /// [`Event::CustomStatusSet`] follows.
    SetCustomStatus(Option<CustomStatus>),
    /// Set the Discord status: `online`, `idle`, `dnd` or `invisible`;
    /// [`Event::StatusSet`] follows.
    SetStatus(String),
    /// Share the game the Discord account plays, or stop;
    /// [`Event::ShowGameSet`] follows.
    ShowGame(bool),
    /// Set how loud the soundboard plays, as Discord keeps it, from 0 to
    /// 100; [`Event::SoundboardVolumeSet`] follows.
    SoundboardVolume(f32),
    /// List every sound the account may join calls with;
    /// [`Event::SoundCatalog`] follows.
    SoundCatalog,
    /// Set the sound that plays as the account joins a voice channel of
    /// `server`, or of every server without one of its own, or take it
    /// away; `sound` is its id and its own server's, none for Discord's
    /// own. [`Event::JoinSoundSet`] follows.
    SetJoinSound {
        server: Option<String>,
        sound: Option<(String, Option<String>)>,
    },
    /// Nobody has used Yapfast since then, in Unix milliseconds, or someone is
    /// using it again; Discord goes AFK, and idle if `show_idle`, as its web
    /// client does.
    Inactive {
        since: Option<u64>,
        show_idle: bool,
    },
    /// Connect the Discord account that was just linked.
    Start,
    /// Forget the linked Discord account on this device, ending its session
    /// on Discord; [`Event::DiscordUnlinked`] follows.
    Unlink,
    /// Change a member of a Discord server, by its server id; a nickname,
    /// a timeout, a kick or a ban answers with [`Event::PersonSaved`], and
    /// a role with [`Event::Error`] when it fails.
    Member {
        server: String,
        user: String,
        change: MemberChange,
    },
    /// Fetch a Discord user's profile, as a member of a room's server or as
    /// the other person in a DM. It comes back as [`Event::Profile`].
    Profile {
        room: String,
        user: String,
    },
    /// Download server icons and avatars by address. Each that arrives comes
    /// back as [`Event::Picture`]; one that fails doesn't.
    Pictures(Vec<String>),
    /// This account's profile for its editor: in a Discord server, by its
    /// server id, or its main profile; [`Event::OwnProfile`] follows.
    OwnProfile {
        server: Option<String>,
    },
    /// Read the picture at `path` for a profile, as Discord takes it, with
    /// a preview; [`Event::ProfilePicture`] follows.
    ProfilePicture {
        kind: ProfilePicture,
        path: PathBuf,
    },
    /// Change this account's profile in a Discord server, by its server
    /// id, or its main profile; [`Event::ProfileSaved`] follows.
    SaveProfile {
        server: Option<String>,
        change: Box<ProfileChange>,
    },
    /// Confirm a request Discord held back for two-factor authentication,
    /// by its prompt's id, with a code, password or backup code;
    /// [`Event::MfaFinished`] follows, and the request goes again.
    FinishMfa {
        id: u64,
        method: MfaMethod,
        code: String,
    },
    /// Have Discord text a code for a prompt; [`Event::MfaTexted`] follows.
    TextMfa(u64),
    /// Give up on a prompt; the request waiting on it fails.
    CancelMfa(u64),
    /// A page of a Discord server's settings, by its server id, as it
    /// opens; [`Event::ServerSettingsLoaded`] follows.
    ServerSettings {
        server: String,
        section: SettingsSection,
    },
    /// Read the picture at `path` for a server, as Discord takes it, with a
    /// preview; [`Event::ServerPicture`] follows.
    ServerPicture {
        kind: ServerPicture,
        path: PathBuf,
    },
    /// Save what changed on a page of a Discord server's settings;
    /// [`Event::ServerSettingsSaved`] follows.
    SaveServerSettings {
        server: String,
        change: Box<ServerSettingsChange>,
    },
    /// Do something on Roles in a Discord server's settings that Discord's
    /// client does at once, without the bar.
    RoleAction {
        server: String,
        action: RoleAction,
    },
    /// Do something on Bans in a Discord server's settings.
    Bans {
        server: String,
        action: BansAction,
    },
    /// A page of a Discord server's audit log; [`Event::AuditLogLoaded`]
    /// follows.
    AuditLog {
        server: String,
        query: AuditQuery,
    },
    /// A chunk of a Discord server's members, as Members searches them;
    /// [`Event::MembersFound`] follows, after [`Event::MembersIndexing`]
    /// while Discord indexes the server.
    Members {
        server: String,
        query: MembersQuery,
    },
    /// What Invites asks of Discord; [`Event::InvitesLoaded`],
    /// [`Event::InviteRevoked`] or [`Event::ServerPaused`] follows.
    Invites {
        server: String,
        action: InvitesAction,
    },
    /// What Emoji asks of Discord; [`Event::EmojisChanged`] follows.
    Emoji {
        server: String,
        action: EmojiAction,
    },
    /// What Stickers asks of Discord; [`Event::StickersChanged`] follows.
    Stickers {
        server: String,
        /// Boxed: a new sticker's form and file would make every command
        /// bigger.
        action: Box<StickerAction>,
    },
}

/// What Emoji asks of Discord. Each is followed by the server's emoji
/// again, as Discord's client fetches them after GUILD_EMOJIS_UPDATE.
#[derive(Clone, Debug)]
pub enum EmojiAction {
    Reload,
    /// A new emoji: its name and its picture, read for it.
    Upload {
        name: String,
        picture: PickedPicture,
    },
    Rename {
        id: String,
        name: String,
    },
    Delete(String),
}

/// What Stickers asks of Discord; each is followed by the stickers again.
#[derive(Clone, Debug)]
pub enum StickerAction {
    /// A new sticker: what the form says, and its file, read for it.
    Upload {
        form: StickerForm,
        file: PickedPicture,
    },
    Edit {
        id: String,
        form: StickerForm,
    },
    Delete(String),
}

/// A sticker's name, related emoji, as a Unicode emoji or a custom emoji's
/// id, and description, which may be empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StickerForm {
    pub name: String,
    pub tags: String,
    pub description: String,
}

/// Which members Members asks Discord for: those named like `text`, in an
/// order, Discord's `sort`, 0 for its own, after a member's join time in
/// Unix milliseconds and user id, `limit` at most, 0 for Discord's chunk of
/// 250.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MembersQuery {
    pub text: String,
    pub sort: u8,
    pub after: Option<(u64, String)>,
    pub limit: usize,
}

impl MembersQuery {
    /// How many Discord is asked for.
    pub fn chunk(&self) -> usize {
        if self.limit == 0 {
            crate::application::MEMBERS_CHUNK
        } else {
            self.limit
        }
    }
}

/// What Invites asks of Discord.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvitesAction {
    /// The list again, as after making one.
    Reload,
    Revoke(String),
    /// Pause invites, DMs or both for some hours, or resume both.
    Pause {
        invites: bool,
        dms: bool,
        hours: u8,
    },
}

/// What Bans asks of Discord.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BansAction {
    /// The next batch, after a user's id; [`Event::BansLoaded`] follows.
    More(String),
    /// Bans of users named, or by id, as Discord's search finds them;
    /// [`Event::BansLoaded`] follows.
    Search(String),
    /// Lift a user's ban; [`Event::Unbanned`] follows.
    Unban(String),
}

/// What Roles does at once: each but [`RoleAction::Members`] and
/// [`RoleAction::FindMembers`] is followed by [`Event::ServerSettingsSaved`]
/// with the roles as they then stand.
#[derive(Clone, Debug)]
pub enum RoleAction {
    /// Make a role, "new role", and open it.
    Create,
    /// Copy a role, as its draft stands, as `name`, just below it, and
    /// open the copy.
    Duplicate {
        role: Box<RoleChange>,
        name: String,
    },
    Delete(String),
    /// The members holding a role; [`Event::RoleHolders`] follows.
    Members(String),
    /// Members whose names start with a text, for Add Members;
    /// [`Event::RoleCandidates`] follows.
    FindMembers(String),
    AddMembers {
        role: String,
        members: Vec<String>,
    },
    RemoveMember {
        role: String,
        member: String,
    },
}

/// A way to confirm with two-factor authentication, as Discord offers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MfaMethod {
    /// A passkey or security key, which takes Discord's app.
    Passkey,
    /// A code from an authenticator app.
    App,
    /// A code Discord texts.
    Sms,
    Password,
    /// One of the account's backup codes.
    Backup,
}

/// A page of a Discord server's settings, as its sidebar lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsSection {
    Profile,
    Engagement,
    BoostPerks,
    Emoji,
    Stickers,
    Members,
    Roles,
    Invites,
    Safety,
    AuditLog,
    Bans,
}

/// A picture of a Discord server's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerPicture {
    Icon,
    Banner,
    /// The invite background.
    Splash,
    /// The icon of the role open on Roles.
    RoleIcon,
    /// A new emoji's or sticker's.
    Emoji,
    Sticker,
}

/// What a page of a Discord server's settings shows, as Discord sent it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerSettingsPage {
    Profile(ServerProfile),
    /// Engagement's and Boost Perks', which the guild object holds.
    Guild(GuildSettings),
    Roles(ServerRoles),
    /// The first batch of the server's bans.
    Bans(Vec<Ban>),
    /// The audit log's newest page.
    AuditLog(AuditLog),
    /// The newest members, as Members first finds them, and how many there
    /// are in all.
    Members {
        members: Vec<ListedMember>,
        total: Option<u64>,
    },
    /// The server's invites, and its settings, which say whether invites
    /// are paused.
    Invites {
        invites: Vec<ServerInvite>,
        settings: GuildSettings,
    },
    /// The server's emoji or stickers, and its settings, which give its
    /// slots.
    Emoji {
        emojis: Vec<ServerEmoji>,
        settings: GuildSettings,
    },
    Stickers {
        stickers: Vec<ServerSticker>,
        settings: GuildSettings,
    },
}

/// A page of a Discord server's audit log, as Discord sends it: its
/// entries, newest first, and the names they refer to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditLog {
    pub entries: Vec<AuditEntry>,
    /// The users the entries name, by id.
    pub users: HashMap<String, AuditUser>,
    /// The names of the webhooks, integrations, threads, events, AutoMod
    /// rules and app commands the entries name, by id.
    pub names: HashMap<String, String>,
}

/// Something done in a Discord server, as its audit log records it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditEntry {
    /// A snowflake, which holds when.
    pub id: String,
    /// Discord's `action_type`, such as 20 for a kick.
    pub action: u16,
    pub target: Option<String>,
    /// Who did it, by user id.
    pub user: Option<String>,
    pub reason: Option<String>,
    pub changes: Vec<AuditChange>,
    /// What Discord adds for some actions, such as a count or a channel.
    pub options: serde_json::Value,
}

/// A field an entry changed, from and to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditChange {
    pub key: String,
    pub old: serde_json::Value,
    pub new: serde_json::Value,
}

/// A user an audit log names: their username, as Discord's log shows it,
/// and their avatar's address.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditUser {
    pub name: String,
    pub avatar: String,
}

/// Which audit log entries to fetch: before an entry's id, by a user, of
/// an action.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditQuery {
    pub before: Option<String>,
    pub user: Option<String>,
    pub action: Option<u16>,
}

/// Someone banned from a Discord server, as Bans lists them: their name,
/// and their username under it once they have a display name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ban {
    pub user: RoleMember,
    /// Why, as the moderator gave it; none from a ban made while Bans was
    /// open, as in Discord's client.
    pub reason: Option<String>,
}

/// What Roles shows: a server's roles and what they may have.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerRoles {
    /// Highest first, @everyone last.
    pub roles: Vec<RoleDetail>,
    /// The server's boost level or features give roles icons
    /// (`ROLE_ICONS`).
    pub role_icons: bool,
    /// A Community server, whose roles have Stage and Insights permissions.
    pub community: bool,
    /// A role to open, which the action that sent these just made.
    pub open: Option<String>,
}

/// A role of a Discord server in full, as Roles shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoleDetail {
    pub id: String,
    pub name: String,
    pub permissions: u64,
    /// Higher roles list first; @everyone is 0.
    pub position: i64,
    /// Its colour, 0 for none, and a gradient's others, which Yapfast
    /// keeps as they are.
    pub color: u32,
    pub secondary_color: Option<u32>,
    pub tertiary_color: Option<u32>,
    /// Listed apart from online members.
    pub hoist: bool,
    pub mentionable: bool,
    /// What gives it, when not people.
    pub managed: Option<RoleManager>,
    /// Its icon's address, or a Unicode emoji in its place.
    pub icon: Option<String>,
    pub unicode_emoji: Option<String>,
    /// How many members hold it.
    pub members: u64,
}

/// What gives a role, when people don't.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleManager {
    Bot,
    Booster,
    Integration,
}

/// A role as saving it sends it: every field, as Discord's client sends
/// them, and its icon only when it changes.
#[derive(Clone, Debug)]
pub struct RoleChange {
    pub id: String,
    pub name: String,
    pub permissions: u64,
    pub color: u32,
    pub secondary_color: Option<u32>,
    pub tertiary_color: Option<u32>,
    pub hoist: bool,
    pub mentionable: bool,
    /// A new icon, or `None` to remove it, which removes the emoji too.
    pub icon: Option<Option<PickedPicture>>,
    pub unicode_emoji: Option<String>,
}

/// A Discord channel's or category's change to what a role or member may
/// do there: the bits it allows and those it denies, over the server's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChannelOverwrite {
    /// The role's id, the server's for @everyone, or the member's user id.
    pub id: String,
    pub member: bool,
    pub allow: u64,
    pub deny: u64,
}

/// What Channel Permissions has Discord do with a channel's overwrites.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverwriteChange {
    /// Set these, one at a time: a save, or a role or member added.
    Put(Vec<ChannelOverwrite>),
    /// Take a role's or member's off.
    Delete(String),
    /// Replace them all with these, the category's, as Sync Now does.
    Sync(Vec<ChannelOverwrite>),
}

/// A member as Members lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedMember {
    /// Their name here, username and avatar.
    pub person: RoleMember,
    pub roles: Vec<String>,
    /// When they joined the server, in Unix milliseconds.
    pub joined: u64,
    /// Until when they're timed out, in Unix milliseconds; 0 for not.
    pub timed_out_until: u64,
    /// Discord's safety signals: unusual DMs lately, an account it
    /// suspects of spam, a name AutoMod quarantined.
    pub unusual_dms: bool,
    pub spammer: bool,
    pub quarantined: bool,
    pub joined_by: JoinMethod,
    /// A bot, which can't be given the server.
    pub bot: bool,
}

/// How a member joined a server, as Members' Join Method shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum JoinMethod {
    #[default]
    Unknown,
    Bot,
    /// An integration, such as `twitch` or `youtube`.
    Integration(Option<String>),
    Discovery,
    Hub,
    /// An invite's code.
    Invite(String),
    Vanity(String),
    /// Manual verification, with the invite's code if any.
    Manual(Option<String>),
    LinkedChannel,
}

/// A server's custom emoji, as Emoji lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerEmoji {
    pub id: String,
    pub name: String,
    pub animated: bool,
    /// Usable: a server that lost boosts keeps emoji past its slots,
    /// unavailable.
    pub available: bool,
    /// Who uploaded it, when Discord says.
    pub user: Option<RoleMember>,
    /// Its picture's address, the one the emoji picker shows.
    pub picture: String,
}

/// A server's sticker, as Stickers lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerSticker {
    pub id: String,
    pub form: StickerForm,
    /// Discord's `format_type`: 1 PNG, 2 APNG, 3 Lottie, 4 GIF.
    pub format: u8,
    pub available: bool,
    /// Who uploaded it, by id, and as a user when Discord says.
    pub user_id: Option<String>,
    pub user: Option<RoleMember>,
    /// Its picture's address; none for Lottie, which isn't drawn.
    pub picture: Option<String>,
}

/// An invite to a Discord server, as Invites lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerInvite {
    pub code: String,
    pub inviter: Option<RoleMember>,
    /// The channel it leads to, as `#name`.
    pub channel: String,
    pub uses: u64,
    /// Its limit, 0 for none.
    pub max_uses: u64,
    /// When it expires, in Unix seconds; none for never.
    pub expires: Option<u64>,
    /// The roles it gives, by id.
    pub roles: Vec<String>,
    /// New members skip join applications.
    pub bypass: bool,
}

/// A member holding a role, as Manage Members lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoleMember {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub avatar: String,
}

/// A Discord server's settings, as its Engagement and Boost Perks pages
/// show them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildSettings {
    /// The channel system messages go to, by its Discord id.
    pub system_channel: Option<String>,
    /// Which system messages Discord leaves out, each bit suppressing one:
    /// Discord's `system_channel_flags`.
    pub system_flags: u64,
    /// Whether members who set nothing are notified of every message, 0,
    /// or only of mentions, 1.
    pub default_notifications: u8,
    /// The voice channel idle members move to, by its Discord id, and
    /// after how many seconds.
    pub afk_channel: Option<String>,
    pub afk_timeout: u32,
    /// The addresses of its banner and invite background, when set.
    pub banner: Option<String>,
    pub splash: Option<String>,
    pub progress_bar: bool,
    /// Its boost level, 0 to 3, and Discord's features, which the level
    /// unlocks, such as `BANNER`.
    pub premium_tier: u8,
    pub features: Vec<String>,
    /// Safety Setup's: what members must have done before they may talk,
    /// 0 nothing to 4 a verified phone; whose images Discord filters, 0 no
    /// one, 1 those without roles, 2 everyone's; and whether moderators
    /// need two-factor authentication, 1, or not, 0.
    pub verification_level: u8,
    pub explicit_content_filter: u8,
    pub mfa_level: u8,
    /// When paused invites and DMs resume, in Unix seconds.
    pub invites_paused_until: Option<u64>,
    pub dms_paused_until: Option<u64>,
    /// The emoji slots its boosts add to each kind's 50, when Discord says.
    pub extra_emoji_slots: Option<u16>,
}

impl GuildSettings {
    /// Whether Discord gives the server a feature.
    pub fn has(&self, feature: &str) -> bool {
        self.features.iter().any(|known| known == feature)
    }
}

/// A Discord server's profile, as Server Profile shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerProfile {
    pub name: String,
    pub description: String,
    /// The address of its icon, when it has one; see
    /// [`DiscordCommand::Pictures`].
    pub icon: Option<String>,
}

/// What saving a page of a Discord server's settings changes: each field
/// that does.
#[derive(Clone, Debug)]
pub enum ServerSettingsChange {
    Profile {
        name: Option<String>,
        description: Option<String>,
        /// A new icon, or `None` to remove it.
        icon: Option<Option<PickedPicture>>,
    },
    /// Engagement's and Boost Perks', saved with `PATCH /guilds/{id}`.
    Guild {
        /// A channel by its Discord id, or `None` for none.
        system_channel: Option<Option<String>>,
        system_flags: Option<u64>,
        default_notifications: Option<u8>,
        afk_channel: Option<Option<String>>,
        afk_timeout: Option<u32>,
        banner: Option<Option<PickedPicture>>,
        splash: Option<Option<PickedPicture>>,
        progress_bar: Option<bool>,
        verification_level: Option<u8>,
        explicit_content_filter: Option<u8>,
    },
    /// Whether moderators need two-factor authentication, 1, or not, 0,
    /// saved with `POST /guilds/{id}/mfa`.
    MfaLevel(u8),
    /// Roles': the roles that move, as `(id, position)`, then the open
    /// role, if it changed.
    Roles {
        positions: Vec<(String, i64)>,
        role: Option<Box<RoleChange>>,
    },
}

/// This account's profile as its editor shows it, in a Discord server or
/// its main one: what it set, each empty where it goes by `fallback`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnProfile {
    /// The server nickname, or the display name.
    pub name: String,
    pub pronouns: String,
    pub bio: String,
    /// The addresses of its avatar and banner, when set; see
    /// [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
    pub banner: Option<String>,
    /// The main profile's banner colour; a server has none of its own.
    pub accent: Option<u32>,
    /// What shows where a field is empty: in a server, the main profile;
    /// in the main profile, the username and Discord's default avatar.
    pub fallback: Profile,
}

/// A picture of a profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfilePicture {
    Avatar,
    Banner,
}

/// A picture chosen for a profile: the file Discord takes, by its type and
/// bytes, and a preview at the size the editor shows it.
#[derive(Clone, Debug)]
pub struct PickedPicture {
    pub mime: &'static str,
    pub data: Arc<[u8]>,
    pub preview: Arc<Picture>,
}

/// What changes in this account's profile: each field that does, a
/// picture to `None` to go by [`OwnProfile::fallback`] again, as an empty
/// text does.
#[derive(Clone, Debug, Default)]
pub struct ProfileChange {
    /// The server nickname, or the display name.
    pub name: Option<String>,
    pub pronouns: Option<String>,
    pub bio: Option<String>,
    pub avatar: Option<Option<PickedPicture>>,
    pub banner: Option<Option<PickedPicture>>,
    /// The main profile's banner colour, `None` for Discord's default.
    pub accent: Option<Option<u32>>,
}

/// A change to how the Discord account stands with someone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationshipChange {
    Unfriend,
    Block,
    Unblock,
    /// Discord hides an ignored user's messages and quiets their DMs, and
    /// doesn't tell them.
    Ignore,
    Unignore,
}

/// A change to a member of a Discord server, which their server's
/// permissions and role hierarchy allow; see
/// [`crate::application::Application::member_choices`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemberChange {
    /// Their nickname in the server; empty resets it.
    Nickname(String),
    /// Give them a role, or take it away.
    Role {
        role: String,
        add: bool,
    },
    /// Time them out until then, in Unix milliseconds, or with none end
    /// their timeout. The reason goes to the server's audit log.
    Timeout {
        until_ms: Option<u64>,
        reason: String,
    },
    Kick {
        reason: String,
    },
    /// Ban them, deleting what they wrote in the last `delete_seconds`.
    Ban {
        delete_seconds: u32,
        reason: String,
    },
    /// Mute or deafen them in the server's voice channels, or undo it.
    ServerMute(bool),
    ServerDeafen(bool),
    /// Move them to another voice channel of the server, by its room id,
    /// or with none disconnect them from voice.
    Move(Option<String>),
}

/// How long a Discord DM, server, channel or category stays muted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Muting {
    Off,
    Minutes(u32),
    /// Until it's unmuted.
    Always,
}

/// Who carries out a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Voice,
    Matrix,
    Discord,
    /// Every network, each for its own rooms.
    Every,
}

impl Command {
    /// Which network a command is for: the one whose room or account it
    /// names, or every network when it changes which room is open.
    pub(crate) fn route(&self) -> Route {
        let network = |room: &str| match Network::of(room) {
            Network::Discord => Route::Discord,
            Network::Matrix => Route::Matrix,
        };
        match self {
            Command::Voice(_) => Route::Voice,
            // Only Discord's videos play for now.
            Command::Discord(_) | Command::Video(_) | Command::Aside(_) => Route::Discord,
            Command::Open(_) | Command::CloseRoom | Command::FindPeople { .. } => Route::Every,
            Command::Send { room, .. }
            | Command::Edit { room, .. }
            | Command::React { room, .. }
            | Command::Reactors { room, .. }
            | Command::Vote { room, .. }
            | Command::SendPoll { room, .. }
            | Command::EndPoll { room, .. }
            | Command::Voters { room, .. }
            | Command::RemoveReactor { room, .. }
            | Command::Delete { room, .. }
            | Command::Pin { room, .. }
            | Command::Retry { room, .. }
            | Command::Cancel { room, .. }
            | Command::SaveAttachment { room, .. }
            | Command::CopyAttachment { room, .. }
            | Command::ViewImage { room, .. }
            | Command::PlayVideo { room, .. }
            | Command::Typing { room, .. }
            | Command::MarkRead { room, .. }
            | Command::SaveDraft { room, .. }
            | Command::VisibleMessages { room, .. }
            | Command::Earlier(room)
            | Command::Later(room)
            | Command::Around { room, .. }
            | Command::Present(room) => network(room),
            // Forwarding doesn't cross networks; Discord says so.
            Command::Forward { room, target, .. } => match network(room) {
                Route::Matrix => network(target),
                route => route,
            },
            Command::StartDm { discord: true, .. } => Route::Discord,
            Command::StartDm { discord: false, .. } => Route::Matrix,
            // Signing out of a build without Matrix unlinks Discord instead.
            Command::Recover(_) | Command::RefreshTrust | Command::RetrySync | Command::Logout => {
                Route::Matrix
            }
        }
    }
}

/// What fell short of a forward to one conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForwardFailure {
    /// The message wasn't forwarded there, and why.
    Forward(String),
    /// It was, but the message added to it wasn't sent, and why.
    Note(String),
}

pub enum Event {
    /// The GIF picker's home, or why it couldn't load.
    GifCategories(Result<(Vec<GifCategory>, Option<Gif>), String>),
    /// GIFs for a search, or with `None` those trending, or why not.
    Gifs {
        query: Option<String>,
        result: Result<Vec<Gif>, String>,
    },
    GifSuggestions {
        query: String,
        terms: Vec<String>,
    },
    /// The account's favourite GIFs, all of them, as fetched or as another
    /// client changed them; or why they couldn't be fetched.
    FavoriteGifs(Result<Vec<FavoriteGif>, String>),
    /// The favourite GIFs as Yapfast last saw them, to show until Discord's
    /// come.
    KeptFavoriteGifs(Vec<FavoriteGif>),
    /// The Discord servers and channels the account used, by id, as fetched
    /// or as another client counted them; or why they couldn't be fetched.
    PlacesUsed(Result<Vec<(String, PlaceUses)>, String>),
    /// A GIF's picture, or none when it failed.
    GifPicture {
        url: String,
        picture: Option<Arc<Picture>>,
    },
    Voice(voice::State),
    /// A profile [`DiscordCommand::Profile`] asked for, or why Discord didn't give it.
    Profile {
        user: String,
        result: Result<Profile, String>,
    },
    /// The profile Yapfast kept of someone [`DiscordCommand::Profile`] asked
    /// for, to show until [`Event::Profile`] comes.
    KeptProfile {
        user: String,
        profile: Profile,
    },
    /// Discord's servers, voice channels and who is in them.
    #[cfg(feature = "discord")]
    Discord(Arc<yapfast_discord::service::Snapshot>),
    /// The video [`Command::PlayVideo`] plays, as it changes; none once it stops.
    Video(Option<Video>),
    /// An image [`Command::ViewImage`] asked for, or why it can't show.
    Image {
        room: String,
        item: String,
        index: usize,
        picture: Result<Arc<Picture>, &'static str>,
    },
    /// A new message to notify the person of.
    Notify(Notification),
    /// A role's name, colour and the members known to have it.
    RoleMembers {
        room: String,
        role: String,
        name: String,
        color: Option<u32>,
        members: Vec<ChatMember>,
    },
    /// People to start a DM with, from Discord or from Matrix.
    People {
        query: String,
        discord: bool,
        people: Vec<Contact>,
    },
    /// The DM [`Command::StartDm`] asked for, by room id, or why it couldn't start.
    DmStarted(Result<String, String>),
    /// Whether [`DiscordCommand::OpenThread`] learned of the thread, which
    /// lists once the next snapshot comes, or why not.
    ThreadOpened {
        room: String,
        result: Result<(), String>,
    },
    /// Whether [`DiscordCommand::AddFriend`]'s request went: to whom, or why not.
    FriendAdded(Result<String, String>),
    /// [`DiscordCommand::Note`]'s note, empty for none, or why it didn't come.
    Note {
        user: String,
        result: Result<String, String>,
    },
    /// Whether [`DiscordCommand::SetNote`] or [`DiscordCommand::SetNickname`]
    /// saved it, or why not.
    PersonSaved(Result<(), String>),
    /// Whether [`DiscordCommand::SetCustomStatus`] changed it, or why not.
    CustomStatusSet(Result<(), String>),
    /// Whether [`DiscordCommand::SetStatus`] changed it, or why not.
    StatusSet(Result<(), String>),
    /// Whether [`DiscordCommand::ShowGame`] changed it, or why not.
    ShowGameSet(Result<(), String>),
    /// Whether [`DiscordCommand::SoundboardVolume`] changed it, or why not.
    SoundboardVolumeSet(Result<(), String>),
    /// [`DiscordCommand::SoundCatalog`]'s sounds, in groups as the
    /// soundboard shows them, or why they didn't come.
    SoundCatalog(Result<Arc<[crate::voice::SoundGroup]>, String>),
    /// Whether [`DiscordCommand::SetJoinSound`] changed it, or why not.
    JoinSoundSet(Result<(), String>),
    /// [`DiscordCommand::Unlink`] forgot the account.
    DiscordUnlinked,
    /// [`DiscordCommand::Pins`]'s messages, the latest pinned first, or why
    /// they didn't come.
    Pins {
        room: String,
        result: Result<Vec<ChatMessage>, String>,
    },
    /// [`DiscordCommand::StartThread`]'s thread, by room id, in the channel
    /// `room`, or why it didn't start.
    ThreadStarted {
        room: String,
        result: Result<String, String>,
    },
    /// [`DiscordCommand::PostInForum`]'s post, by room id, in the forum
    /// `room`, or why it wasn't made.
    ForumPosted {
        room: String,
        result: Result<String, String>,
    },
    /// [`DiscordCommand::Threads`]'s page, and whether Discord has more
    /// after it, or why it didn't come.
    Threads {
        room: String,
        query: Option<String>,
        offset: u32,
        result: Result<(Vec<ListedThread>, bool), String>,
        /// In a forum, its posts' first messages.
        starters: PostStarters,
    },
    /// [`DiscordCommand::Posts`]'s first messages in the forum `room`, or
    /// why they didn't come.
    Posts {
        room: String,
        result: Result<PostStarters, String>,
    },
    /// What [`DiscordCommand::Search`] found: a page of messages, the newest
    /// first, and how many in all; or why it couldn't search.
    Found {
        place: String,
        query: String,
        offset: u32,
        result: Result<(u32, Vec<SearchHit>), String>,
    },
    /// The open Discord channel's topic, when it opens and when it changes;
    /// none when it has none.
    Topic {
        room: String,
        topic: Option<Arc<Topic>>,
    },
    /// The group [`DiscordCommand::AddToGroup`] made or added to, by room id,
    /// or why it couldn't.
    GroupChanged(Result<String, String>),
    /// A Discord channel's settings [`DiscordCommand::ChannelSettings`]
    /// asked for, by its room id, or why they didn't come.
    ChannelLoaded {
        room: String,
        result: Result<ChannelSettings, String>,
    },
    /// Whether Discord made, changed or deleted the channel a dialog asked
    /// it to: the room id of one it made, or why it didn't.
    ChannelSaved(Result<Option<String>, String>),
    /// Whether Discord took a [`DiscordCommand::ChangeOverwrites`], or why
    /// not.
    OverwritesChanged {
        room: String,
        result: Result<(), String>,
    },
    /// The members [`DiscordCommand::OverwriteNames`] asked about, named.
    OverwriteNames {
        room: String,
        members: Vec<RoleMember>,
    },
    /// This account's profile in a Discord server, by its server id, or
    /// its main profile, as [`DiscordCommand::OwnProfile`] asked, or why
    /// Discord didn't give it.
    OwnProfile {
        server: Option<String>,
        result: Result<OwnProfile, String>,
    },
    /// The picture [`DiscordCommand::ProfilePicture`] read, from `path`, or
    /// why it can't be one.
    ProfilePicture {
        kind: ProfilePicture,
        path: PathBuf,
        result: Result<PickedPicture, String>,
    },
    /// Whether Discord took the change to this account's profile in a
    /// server, or its main one, or why not.
    ProfileSaved {
        server: Option<String>,
        result: Result<(), String>,
    },
    /// Discord holds a request back until this account confirms with
    /// two-factor authentication, by one of `methods` in Discord's order;
    /// [`DiscordCommand::FinishMfa`] or [`DiscordCommand::CancelMfa`]
    /// answers it by `id`.
    MfaRequired {
        id: u64,
        methods: Vec<MfaMethod>,
    },
    /// Whether Discord took a confirmation, or why not; the prompt stays
    /// open for another try when it didn't.
    MfaFinished {
        id: u64,
        result: Result<(), String>,
    },
    /// Where Discord texted a code for a prompt, or why it didn't.
    MfaTexted {
        id: u64,
        result: Result<String, String>,
    },
    /// A page of a Discord server's settings, by its server id, as
    /// [`DiscordCommand::ServerSettings`] asked, or why Discord didn't give
    /// it.
    ServerSettingsLoaded {
        server: String,
        section: SettingsSection,
        result: Result<ServerSettingsPage, String>,
    },
    /// The picture [`DiscordCommand::ServerPicture`] read, from `path`, or
    /// why it can't be one.
    ServerPicture {
        kind: ServerPicture,
        path: PathBuf,
        result: Result<PickedPicture, String>,
    },
    /// A Discord server's settings changed, by its server id, as
    /// GUILD_UPDATE gives them, for its open settings to follow.
    ServerSettingsChanged {
        server: String,
        settings: Box<GuildSettings>,
    },
    /// The page Discord took the change to a server's settings on, as it
    /// now stands, or why Discord didn't take it.
    ServerSettingsSaved {
        server: String,
        result: Result<ServerSettingsPage, String>,
    },
    /// A role was made or changed elsewhere, or by this account, as
    /// GUILD_ROLE_CREATE and GUILD_ROLE_UPDATE give it, for an open Roles
    /// to follow; its member count isn't known.
    ServerRoleChanged {
        server: String,
        role: Box<RoleDetail>,
    },
    ServerRoleDeleted {
        server: String,
        role: String,
    },
    /// Bans Discord sent for [`BansAction::More`] or [`BansAction::Search`],
    /// or why not.
    BansLoaded {
        server: String,
        searched: bool,
        result: Result<Vec<Ban>, String>,
    },
    /// The audit log page [`DiscordCommand::AuditLog`] asked for, or why
    /// Discord didn't send it.
    AuditLogLoaded {
        server: String,
        query: AuditQuery,
        result: Result<AuditLog, String>,
    },
    /// The members [`DiscordCommand::Members`] asked for, and how many
    /// match in all, or why Discord didn't send them.
    MembersFound {
        server: String,
        query: MembersQuery,
        result: Result<(Vec<ListedMember>, Option<u64>), String>,
    },
    /// Discord is indexing the server before it can search its members.
    MembersIndexing {
        server: String,
        query: MembersQuery,
    },
    /// A server's invites again, or why not.
    InvitesLoaded {
        server: String,
        result: Result<Vec<ServerInvite>, String>,
    },
    /// Whether Discord revoked an invite.
    InviteRevoked {
        server: String,
        code: String,
        result: Result<(), String>,
    },
    /// When a server's paused invites and DMs now resume, or why Discord
    /// didn't pause or resume them.
    ServerPaused {
        server: String,
        result: Result<(Option<u64>, Option<u64>), String>,
    },
    /// Whether what Emoji asked of Discord went, uploading or not, and the
    /// server's emoji after it, when Discord sent them.
    EmojisChanged {
        server: String,
        upload: bool,
        result: Result<(), String>,
        emojis: Option<Vec<ServerEmoji>>,
    },
    /// The same for Stickers: `form` for an upload or an edit.
    StickersChanged {
        server: String,
        form: bool,
        result: Result<(), String>,
        stickers: Option<Vec<ServerSticker>>,
    },
    /// A server's stickers changed, as GUILD_STICKERS_UPDATE gives them,
    /// for an open Stickers.
    ServerStickers {
        server: String,
        stickers: Vec<ServerSticker>,
    },
    /// Whether Discord lifted a user's ban, or why not.
    Unbanned {
        server: String,
        user: String,
        result: Result<(), String>,
    },
    /// Someone was banned elsewhere, or unbanned, as GUILD_BAN_ADD and
    /// GUILD_BAN_REMOVE say, for an open Bans to follow.
    ServerBan {
        server: String,
        ban: Box<Ban>,
        banned: bool,
    },
    /// The members [`RoleAction::Members`] found holding a role, or why
    /// not.
    RoleHolders {
        server: String,
        role: String,
        result: Result<Vec<RoleMember>, String>,
        /// How many hold it, after a change to them.
        count: Option<u64>,
    },
    /// The members [`RoleAction::FindMembers`] found for a text.
    RoleCandidates {
        server: String,
        query: String,
        members: Vec<RoleMember>,
    },
    /// Whether this account left a Discord server or group DM, by its
    /// server or room id, or why not.
    Left {
        place: String,
        result: Result<(), String>,
    },
    /// Whether Discord deleted a server, or why not.
    ServerDeleted {
        server: String,
        result: Result<(), String>,
    },
    /// Whether Discord gave a server away, or why not.
    ServerTransferred {
        server: String,
        result: Result<(), String>,
    },
    /// Whether Discord emailed the code to give a server away, or why not.
    ServerPincodeSent {
        server: String,
        result: Result<(), String>,
    },
    /// The code of the invite [`DiscordCommand::CreateInvite`] made to a
    /// channel, by its room id, or why Discord made none.
    InviteMade {
        room: String,
        result: Result<String, String>,
    },
    /// Whether [`DiscordCommand::SendInvite`] sent a Discord user an
    /// invite to the channel by its room id, or why not.
    InviteSent {
        user: String,
        room: String,
        result: Result<(), String>,
    },
    /// What the invite [`DiscordCommand::ResolveInvite`] looked up by its
    /// code shows, or why Discord showed nothing.
    InviteResolved {
        code: String,
        result: Result<InvitePreview, String>,
    },
    /// The address of the page with a captcha Discord asked for before
    /// joining by the invite with this code.
    JoinCaptcha {
        code: String,
        page: String,
    },
    /// The server [`DiscordCommand::JoinServer`] joined by the invite with
    /// this code, or why it didn't.
    ServerJoined {
        code: String,
        result: Result<JoinedServer, String>,
    },
    /// What the scheduled event [`DiscordCommand::ResolveEvent`] looked
    /// up by its id shows, or why Discord showed nothing.
    EventResolved {
        event: String,
        result: Result<EventPreview, String>,
    },
    /// Whether [`DiscordCommand::SetEventInterest`] marked this account
    /// `interested` in the event with this id, or why it didn't.
    EventInterestSet {
        event: String,
        interested: bool,
        result: Result<(), String>,
    },
    /// What the gift [`DiscordCommand::ResolveGift`] looked up by its
    /// code shows, or why Discord showed nothing.
    GiftResolved {
        code: String,
        result: Result<GiftPreview, String>,
    },
    /// Whether [`DiscordCommand::RedeemGift`] redeemed the gift with
    /// this code, or why it didn't.
    GiftRedeemed {
        code: String,
        result: Result<(), String>,
    },
    /// What the authorization [`DiscordCommand::ResolveOAuth`] looked
    /// up by its link's query shows, or why Discord showed nothing.
    OAuthResolved {
        query: String,
        result: Result<OAuthPreview, String>,
    },
    /// The address [`DiscordCommand::AuthorizeOAuth`] sends the person
    /// to next, for the link with this query, or why it didn't.
    OAuthAuthorized {
        query: String,
        result: Result<String, String>,
    },
    /// The roles and groups a message in a room may mention.
    MentionGroups {
        room: String,
        groups: Vec<MentionGroup>,
    },
    /// Members whose names start with `query`, for mentioning.
    MemberMatches {
        room: String,
        query: String,
        members: Vec<ChatMember>,
    },
    /// Who else is typing in a room now, by name.
    Typing {
        room: String,
        names: Vec<String>,
    },
    /// A picture [`DiscordCommand::Pictures`] asked for, decoded.
    Picture {
        url: String,
        picture: Arc<Picture>,
    },
    Connected {
        user: String,
        trusted: bool,
        storage: &'static str,
    },
    Rooms(RoomIndex),
    Timeline {
        room: String,
        /// Shared, so a backend can send rows that didn't change again
        /// without copying them.
        messages: Vec<Arc<ChatMessage>>,
    },
    RoomError {
        room: String,
        message: String,
    },
    /// A page of history [`Command::Earlier`], [`Command::Later`] or
    /// [`Command::Around`] asked for, or a Discord channel's newest, is in
    /// the room's timeline, or why there's none.
    HistoryLoaded {
        room: String,
        direction: Direction,
        history: History,
    },
    /// The open room's joined members, by role and then name.
    Members {
        room: String,
        members: Result<Vec<ChatMember>, &'static str>,
    },
    /// The send queue accepted or refused a message; delivery shows on its timeline item.
    SendResult {
        room: String,
        body: String,
        result: Result<(), &'static str>,
    },
    EditResult {
        room: String,
        item: String,
        result: Result<(), String>,
    },
    /// [`Command::Reactors`]'s page, or [`Command::Voters`]', or why it
    /// didn't come.
    Reactors {
        room: String,
        item: String,
        key: String,
        after: Option<String>,
        limit: u32,
        result: Result<Vec<Reactor>, String>,
    },
    /// [`Command::SendPoll`]'s poll went up in `room`, or why it didn't.
    PollPosted {
        room: String,
        result: Result<(), String>,
    },
    /// A message was forwarded to `room`, with the message added to it, or
    /// what fell short.
    Forwarded {
        room: String,
        result: Result<(), ForwardFailure>,
    },
    /// A draft saved by an earlier run.
    Draft {
        room: String,
        body: String,
    },
    Saved {
        room: String,
        name: String,
        result: Result<(), &'static str>,
    },
    /// The picture [`Command::CopyAttachment`] downloaded: the file as it
    /// came, for the clipboard; or why it couldn't.
    Copied(Result<Vec<u8>, &'static str>),
    Trust {
        trusted: bool,
        error: Option<&'static str>,
    },
    Sync(bool),
    Error(String),
    LoggedOut,
}

pub fn sort_members(members: &mut [ChatMember]) {
    members.sort_by(|a, b| {
        a.role
            .cmp(&b.role)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Decode an image with dimension and allocation limits, then shrink it for the timeline.
pub(crate) fn decode_preview(data: &[u8]) -> Option<Picture> {
    decode_picture(data, PREVIEW_WIDTH, PREVIEW_HEIGHT)
}

/// Decode an image for the viewer, shrunk to fit a large window.
pub(crate) fn decode_view(data: &[u8]) -> Option<Picture> {
    decode_picture(data, VIEW_WIDTH, VIEW_HEIGHT)
}

/// Decode an image with dimension and allocation limits, shrunk to fit `width`
/// by `height`. A smaller image keeps its size: enlarging it would blur it
/// and cost memory, and the viewer sizes images from their own dimensions.
pub fn decode_picture(data: &[u8], width: u32, height: u32) -> Option<Picture> {
    decode_kept_picture(data, width, height, None)
}

/// The picture in `file`, as [`decode_picture`] decodes one, read where it
/// is: none of it is copied to memory, and an animation's frames come from
/// the file as they show.
pub fn decode_picture_at(file: &std::path::Path, width: u32, height: u32) -> Option<Picture> {
    let opened = std::fs::File::open(file).ok()?;
    // SAFETY: Yapfast's own files, which nothing changes while they're read.
    let data = unsafe { memmap2::Mmap::map(&opened) }.ok()?;
    decode_kept_picture(&data, width, height, Some(file))
}

/// [`decode_picture`], with an animation reading its frames from `kept`
/// when it's `data`'s file, rather than from a copy of `data`.
fn decode_kept_picture(
    data: &[u8],
    width: u32,
    height: u32,
    kept: Option<&std::path::Path>,
) -> Option<Picture> {
    #[cfg(feature = "discord")]
    if crate::video::is_video(data) {
        return crate::video::decode_kept_animation(data, width, height, kept);
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .ok()?;
    if let Some(format) = reader.format()
        && let Some(animation) = decode_animation(data, format, width, height, kept)
    {
        return Some(animation);
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let picture = if decoded.width() > width || decoded.height() > height {
        decoded.thumbnail(width, height)
    } else {
        decoded
    }
    .into_rgba8();
    Some(Picture {
        width: picture.width(),
        height: picture.height(),
        rgba: Pixels::new(picture.width(), picture.height(), picture.into_raw()),
        animation: None,
    })
}

/// An animated GIF, PNG or WebP, shrunk to fit `width` by `height`; none for
/// a still picture, or when its first frame fails. Only the first frame is
/// decoded: the others' timings come from their headers, so a long GIF opens
/// as fast as a short one, and a frame that fails later leaves the one
/// before it showing.
fn decode_animation(
    data: &[u8],
    format: image::ImageFormat,
    width: u32,
    height: u32,
    kept: Option<&std::path::Path>,
) -> Option<Picture> {
    use image::ImageFormat;
    let format = match format {
        ImageFormat::Gif => AnimationFormat::Gif,
        ImageFormat::Png => AnimationFormat::Png,
        ImageFormat::WebP => AnimationFormat::WebP,
        _ => return None,
    };
    let delays = frame_delays(data, format)?;
    (delays.len() > 1).then_some(())?;
    let ((full_width, full_height), mut frames) = image_frames(std::io::Cursor::new(data), format)?;
    let (width, height) = fit(full_width, full_height, width, height);
    let first = shrink(frames.next()?.ok()?.into_buffer(), width, height);
    Some(Picture {
        width,
        height,
        rgba: Pixels::new(width, height, first),
        animation: Some(Animation {
            delays,
            file: AnimationFile::of(data, kept),
            format,
        }),
    })
}

/// How long each frame of a GIF, an animated PNG or a WebP shows, in
/// milliseconds, read from the frames' headers without decoding a picture,
/// as the image crate gives them.
fn frame_delays(data: &[u8], format: AnimationFormat) -> Option<Vec<u32>> {
    let delays = match format {
        AnimationFormat::Gif => gif_delays(data),
        AnimationFormat::Png => apng_delays(data)?,
        AnimationFormat::WebP => webp_delays(data)?,
        #[cfg(feature = "discord")]
        AnimationFormat::Video => return None,
    };
    // Browsers show frames meant for under 20 ms for 100 ms.
    Some(
        delays
            .into_iter()
            .map(|delay| if delay < 20 { 100 } else { delay })
            .collect(),
    )
}

/// A GIF's frames' delays, which it gives in hundredths of a second, up to
/// the first that fails.
fn gif_delays(data: &[u8]) -> Vec<u32> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    options.skip_frame_decoding(true);
    let Ok(mut decoder) = options.read_info(data) else {
        return vec![];
    };
    let mut delays = Vec::new();
    while let Ok(Some(frame)) = decoder.read_next_frame() {
        delays.push(u32::from(frame.delay) * 10);
    }
    delays
}

/// An animated PNG's frames' delays, from their `fcTL` chunks, as fractions
/// of a second; as many as its `acTL` says it has.
fn apng_delays(data: &[u8]) -> Option<Vec<u32>> {
    (data.get(..8)? == b"\x89PNG\r\n\x1a\n").then_some(())?;
    let number = |bytes: &[u8]| u16::from_be_bytes([bytes[0], bytes[1]]);
    let mut count = None;
    let mut delays = Vec::new();
    let mut at = 8usize;
    while let Some(header) = data.get(at..at + 8) {
        let size = u32::from_be_bytes(header[..4].try_into().ok()?) as usize;
        let body = data.get(at + 8..(at + 8).checked_add(size)?)?;
        match &header[4..] {
            b"acTL" => count = Some(u32::from_be_bytes(body.get(..4)?.try_into().ok()?)),
            b"fcTL" => {
                let numerator = u32::from(number(body.get(20..22)?));
                let denominator = match number(body.get(22..24)?) {
                    // As the standard says.
                    0 => 100,
                    denominator => u32::from(denominator),
                };
                delays.push(numerator * 1000 / denominator);
            }
            b"IEND" => break,
            _ => {}
        }
        // Then the chunk's checksum.
        at += 12 + size;
    }
    delays.truncate(usize::try_from(count?).ok()?);
    Some(delays)
}

/// An animated WebP's frames' durations, in milliseconds, from their `ANMF`
/// chunks.
fn webp_delays(data: &[u8]) -> Option<Vec<u32>> {
    (data.get(..4)? == b"RIFF" && data.get(8..12)? == b"WEBP").then_some(())?;
    let mut delays = Vec::new();
    let mut at = 12usize;
    while let Some(header) = data.get(at..at + 8) {
        let size = u32::from_le_bytes(header[4..].try_into().ok()?) as usize;
        if &header[..4] == b"ANMF" {
            // After the frame's offset and size, three bytes each.
            let duration = data.get(at + 20..at + 23)?;
            delays.push(u32::from_le_bytes([
                duration[0],
                duration[1],
                duration[2],
                0,
            ]));
        }
        // Chunks are padded to an even size.
        at = at.checked_add(8 + size + size % 2)?;
    }
    Some(delays)
}

/// An animated GIF, PNG or WebP's size and frames, which decode as they're
/// taken; none for a still picture.
fn image_frames<'a>(
    cursor: impl std::io::BufRead + std::io::Seek + 'a,
    format: AnimationFormat,
) -> Option<((u32, u32), image::Frames<'a>)> {
    use image::{AnimationDecoder, ImageDecoder, codecs};
    // Every frame is drawn on a canvas of the full size, and a tiny file
    // can claim a huge one: these keep a sent animation from taking
    // gigabytes.
    const MAX_SIDE: u32 = 8192;
    const MAX_PIXELS: u64 = 4096 * 4096;
    let fits = |(w, h): (u32, u32)| {
        (w <= MAX_SIDE && h <= MAX_SIDE && u64::from(w) * u64::from(h) <= MAX_PIXELS)
            .then_some((w, h))
    };
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(256 * 1024 * 1024);
    match format {
        AnimationFormat::Gif => {
            let mut decoder = codecs::gif::GifDecoder::new(cursor).ok()?;
            let size = fits(decoder.dimensions())?;
            decoder.set_limits(limits).ok()?;
            Some((size, decoder.into_frames()))
        }
        AnimationFormat::Png => {
            let decoder = codecs::png::PngDecoder::with_limits(cursor, limits).ok()?;
            decoder.is_apng().ok()?.then_some(())?;
            let size = fits(decoder.dimensions())?;
            Some((size, decoder.apng().ok()?.into_frames()))
        }
        AnimationFormat::WebP => {
            let mut decoder = codecs::webp::WebPDecoder::new(cursor).ok()?;
            decoder.has_animation().then_some(())?;
            let size = fits(decoder.dimensions())?;
            decoder.set_limits(limits).ok()?;
            Some((size, decoder.into_frames()))
        }
        #[cfg(feature = "discord")]
        AnimationFormat::Video => None,
    }
}

/// A frame as RGBA at `width` by `height`, which it's no smaller than.
fn shrink(frame: image::RgbaImage, width: u32, height: u32) -> Vec<u8> {
    if frame.dimensions() == (width, height) {
        frame.into_raw()
    } else {
        image::imageops::thumbnail(&frame, width, height).into_raw()
    }
}

/// `width` by `height` shrunk to fit `max_width` by `max_height`, never
/// enlarged, keeping its shape.
pub fn fit(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(max_width) / f64::from(width),
            f64::from(max_height) / f64::from(height),
        ),
    );
    let side = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    (side(width), side(height))
}

#[cfg(feature = "discord")]
impl Animation {
    /// A video's, whose frames show for `delays`: `data`, read from `kept`
    /// when that's its file.
    pub(crate) fn video(data: &[u8], kept: Option<&std::path::Path>, delays: Vec<u32>) -> Self {
        Self {
            delays,
            file: AnimationFile::of(data, kept),
            format: AnimationFormat::Video,
        }
    }
}

/// The first line of a Markdown body as plain text, for reply quotes.
pub fn plain_snippet(markdown: &str) -> String {
    use pulldown_cmark::{Event, Parser, TagEnd};
    let mut text = String::new();
    for event in Parser::new(markdown) {
        match event {
            Event::Text(part) | Event::Code(part) => text.push_str(&part),
            Event::SoftBreak | Event::HardBreak | Event::End(TagEnd::Paragraph)
                if !text.is_empty() =>
            {
                break;
            }
            _ => {}
        }
    }
    text
}

/// A Markdown body as plain text for a notification: its paragraphs and list items on
/// lines of their own, cut at about 300 characters.
pub fn notification_text(markdown: &str) -> String {
    use pulldown_cmark::{Event, Parser, TagEnd};
    const LIMIT: usize = 300;
    let mut text = String::new();
    for event in Parser::new(markdown) {
        match event {
            Event::Text(part) | Event::Code(part) => text.push_str(&part),
            Event::SoftBreak
            | Event::HardBreak
            | Event::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::Heading(_))
                if !text.is_empty() && !text.ends_with('\n') =>
            {
                text.push('\n');
            }
            _ => {}
        }
    }
    let text = text.trim_end();
    match text.char_indices().nth(LIMIT) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text.to_owned(),
    }
}

/// Whether Yapfast can start without the sign-in screen: with a saved Matrix
/// session, or always in a build without Matrix, where Discord uses the
/// account linked on this device.
pub fn can_restore(root: &std::path::Path) -> bool {
    #[cfg(feature = "matrix")]
    return root.join("active.json").exists();
    #[cfg(all(not(feature = "matrix"), feature = "discord"))]
    return {
        let _ = root;
        yapfast_discord::token::load().is_ok()
    };
    #[cfg(not(any(feature = "matrix", feature = "discord")))]
    {
        let _ = root;
        true
    }
}

/// A new message that should notify the person, by their notification settings on its
/// network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    /// The conversation it came in.
    pub room: String,
    pub author: String,
    /// The message as plain text.
    pub body: String,
    /// The address of the author's picture; see [`DiscordCommand::Pictures`].
    pub avatar: Option<String>,
    /// It mentions this account, rather than notifying as any message there does.
    pub mention: bool,
}

/// A notification ready to show: its conversation, a title naming who wrote where, the
/// message, and the author's picture when Yapfast has it.
#[derive(Clone, Debug)]
pub struct Alert {
    pub room: String,
    pub title: String,
    pub body: String,
    pub picture: Option<Arc<Picture>>,
    /// A call ringing this account, rather than a message.
    pub call: bool,
    /// A message mentioning this account.
    pub mention: bool,
}

/// What woke the actor. Each network's own events are handled by its module.
// One lives at a time, on the actor's stack, so boxing would only add allocations.
#[allow(clippy::large_enum_variant)]
enum Wake {
    Command(Option<Command>),
    Voice(bool),
    #[cfg(feature = "matrix")]
    Matrix(crate::matrix_chat::Wake),
    #[cfg(feature = "discord")]
    Discord(crate::discord_text::Wake),
}

pub async fn run(
    root: PathBuf,
    start: Start,
    mut commands: UnboundedReceiver<Command>,
    events: UnboundedSender<Event>,
) {
    let remembered = root.join(voice::DEVICES);
    #[cfg(feature = "matrix")]
    let session = match crate::matrix_chat::open(root, start).await {
        Ok(session) => session,
        Err(error) => {
            let _ = events.send(Event::Error(error.to_string()));
            return;
        }
    };
    // Discord connects when this device has a linked account; see docs/discord/README.md.
    // Why a linked account isn't running answers the commands that need it.
    #[cfg(feature = "discord")]
    let (mut discord, mut discord_error) = match crate::discord_text::Chat::start() {
        Ok(discord) => (discord, None),
        Err(error) => (None, Some(discord_failed(&error))),
    };
    // Voice reaches Discord through this, so linking and unlinking reach it too.
    #[cfg(feature = "discord")]
    let linked =
        tokio::sync::watch::Sender::new(discord.as_ref().map(crate::discord_text::Chat::service));
    #[cfg(not(feature = "matrix"))]
    {
        let refused = match start {
            Start::Login { .. } => Some("This build of Yapfast has no Matrix sign-in."),
            #[cfg(feature = "discord")]
            Start::Restore if discord.is_none() => Some(
                discord_error
                    .as_deref()
                    .unwrap_or("Link your Discord account to sign in."),
            ),
            #[cfg(not(feature = "discord"))]
            Start::Restore => Some("This build of Yapfast has no networks."),
            #[allow(unreachable_patterns)]
            _ => None,
        };
        if let Some(refused) = refused {
            let _ = events.send(Event::Error(refused.into()));
            return;
        }
    }
    let calls = voice::Calls {
        #[cfg(feature = "matrix")]
        session: session.lease(),
        #[cfg(feature = "discord")]
        discord: linked.subscribe(),
    };
    let Ok(mut voice) = voice::Controller::start_remembering(calls, Some(remembered)) else {
        let _ = events.send(Event::Error(
            "Could not start the voice worker. Restart Yapfast and retry.".into(),
        ));
        return;
    };
    #[cfg(feature = "matrix")]
    let mut matrix = crate::matrix_chat::Matrix::connect(session, &events).await;
    #[cfg(feature = "discord")]
    if let Some(error) = &discord_error {
        let _ = events.send(Event::Error(error.clone()));
    }
    #[cfg(not(feature = "matrix"))]
    let _ = events.send(Event::Connected {
        user: "Discord".into(),
        trusted: true,
        storage: "The Discord token is stored in the OS credential store.",
    });

    // The actor waits on network events rather than polling the account or the UI.
    loop {
        #[cfg(feature = "matrix")]
        matrix.restart_sync(&events).await;
        let wake = {
            // Each network's wait is cancel safe, and handling happens outside it.
            #[cfg(feature = "matrix")]
            let matrix_wake = async { Wake::Matrix(matrix.wait().await) };
            #[cfg(not(feature = "matrix"))]
            let matrix_wake = std::future::pending::<Wake>();
            #[cfg(feature = "discord")]
            let discord_wake = async {
                match discord.as_mut() {
                    Some(discord) => Wake::Discord(discord.wait().await),
                    None => std::future::pending().await,
                }
            };
            #[cfg(not(feature = "discord"))]
            let discord_wake = std::future::pending::<Wake>();
            tokio::select! {
                command = commands.recv() => Wake::Command(command),
                changed = voice.states.changed(), if voice.states.has_changed().is_ok() => Wake::Voice(changed.is_ok()),
                wake = matrix_wake => wake,
                wake = discord_wake => wake,
            }
        };
        match wake {
            Wake::Command(None) => break,
            Wake::Command(Some(command)) => match command.route() {
                Route::Voice => {
                    if let Command::Voice(command) = command {
                        voice.command(command);
                    }
                }
                Route::Every => {
                    #[cfg(feature = "discord")]
                    if let Some(discord) = discord.as_mut() {
                        discord.command(command.clone(), &events);
                    }
                    #[cfg(feature = "matrix")]
                    if !matrix.command(command, &mut voice, &events).await {
                        break;
                    }
                }
                #[cfg(feature = "discord")]
                Route::Discord => match command {
                    Command::Discord(DiscordCommand::Start) => {
                        if discord.is_none() {
                            // Linking saved the login just before.
                            let started = crate::discord_text::Chat::start().and_then(|started| {
                                started.ok_or_else(|| anyhow::anyhow!("its login wasn't saved"))
                            });
                            match started {
                                Ok(started) => {
                                    discord = Some(started);
                                    discord_error = None;
                                }
                                Err(error) => {
                                    let error = discord_failed(&error);
                                    let _ = events.send(Event::Error(error.clone()));
                                    discord_error = Some(error);
                                }
                            }
                            linked.send_replace(
                                discord.as_ref().map(crate::discord_text::Chat::service),
                            );
                        }
                    }
                    Command::Discord(DiscordCommand::Unlink) => {
                        unlink_discord(&mut discord, &linked, &voice);
                        discord_error = None;
                        let _ = events.send(Event::DiscordUnlinked);
                    }
                    command => match discord.as_mut() {
                        Some(discord) => discord.command(command, &events),
                        None => crate::discord_text::without(
                            command,
                            discord_error
                                .as_deref()
                                .unwrap_or(crate::discord_text::NOT_LINKED),
                            &events,
                        ),
                    },
                },
                #[cfg(feature = "matrix")]
                Route::Matrix => {
                    if !matrix.command(command, &mut voice, &events).await {
                        break;
                    }
                }
                #[cfg(not(feature = "matrix"))]
                Route::Matrix => {
                    if let Command::Logout = command {
                        #[cfg(feature = "discord")]
                        unlink_discord(&mut discord, &linked, &voice);
                        let _ = events.send(Event::LoggedOut);
                        break;
                    }
                }
                // This build has no Discord to carry it out.
                #[cfg(not(feature = "discord"))]
                Route::Discord => {}
            },
            Wake::Voice(changed) => {
                if changed {
                    let _ = events.send(Event::Voice(voice.states.borrow_and_update().clone()));
                }
            }
            #[cfg(feature = "matrix")]
            Wake::Matrix(wake) => matrix.handle(wake, &voice, &events).await,
            #[cfg(feature = "discord")]
            Wake::Discord(wake) => {
                if let Some(discord) = discord.as_mut() {
                    discord.handle(wake, &events);
                }
            }
        }
    }
    voice.stop().await;
    #[cfg(feature = "matrix")]
    matrix.stop().await;
}

/// Logs why Discord couldn't start, and says so as the person sees it.
#[cfg(feature = "discord")]
fn discord_failed(error: &anyhow::Error) -> String {
    tracing::warn!(error = format!("{error:#}"), "could not start Discord");
    format!("Discord couldn't start: {error}")
}

/// Forgets the linked Discord account on this device: leaves its call, ends
/// its session on Discord, and stops it.
#[cfg(feature = "discord")]
fn unlink_discord(
    discord: &mut Option<crate::discord_text::Chat>,
    linked: &tokio::sync::watch::Sender<Option<yapfast_discord::service::Discord>>,
    voice: &voice::Controller,
) {
    if voice
        .states
        .borrow()
        .room
        .as_deref()
        .is_some_and(|room| Network::of(room) == Network::Discord)
    {
        voice.command(voice::Command::Leave);
    }
    linked.send_replace(None);
    if let Some(chat) = discord.take() {
        let service = chat.service();
        tokio::spawn(async move {
            if let Err(error) = service.logout().await {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "could not end the Discord session"
                );
            }
        });
    }
    if let Err(error) = yapfast_discord::token::forget() {
        tracing::warn!(%error, "could not forget the Discord login");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An animated GIF of `count` frames of one colour each, 50 ms apiece.
    fn gif(side: u32, count: u8) -> Vec<u8> {
        gif_sized(side, side, count)
    }

    fn gif_sized(width: u32, height: u32, count: u8) -> Vec<u8> {
        let mut data = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut data);
            for index in 0..count {
                let shade = image::Rgba([index.wrapping_mul(20), 0, 0, 255]);
                let frame = image::Frame::from_parts(
                    image::RgbaImage::from_pixel(width, height, shade),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(50, 1),
                );
                encoder.encode_frame(frame).unwrap();
            }
        }
        data
    }

    #[test]
    fn room_changes_reach_every_network_and_the_rest_their_own() {
        let (matrix, discord) = ("!room:test".to_owned(), "discord:10:21".to_owned());
        let send = |room: &String| Command::Send {
            room: room.clone(),
            body: "hi".into(),
            reply: None,
            options: Default::default(),
            files: vec![],
        };
        let forward = |room: &String, target: &String| Command::Forward {
            room: room.clone(),
            item: "1".into(),
            target: target.clone(),
            note: None,
            silent: false,
        };
        for (command, route) in [
            (Command::Open(matrix.clone()), Route::Every),
            (Command::Open(discord.clone()), Route::Every),
            (Command::CloseRoom, Route::Every),
            (send(&matrix), Route::Matrix),
            (send(&discord), Route::Discord),
            (Command::Earlier(discord.clone()), Route::Discord),
            (Command::Later(matrix.clone()), Route::Matrix),
            (
                Command::Around {
                    room: discord.clone(),
                    item: "d:1".into(),
                },
                Route::Discord,
            ),
            (Command::Present(matrix.clone()), Route::Matrix),
            (
                Command::VisibleMessages {
                    room: matrix.clone(),
                    items: vec![],
                    at_end: false,
                },
                Route::Matrix,
            ),
            (
                Command::VisibleMessages {
                    room: discord.clone(),
                    items: vec![],
                    at_end: false,
                },
                Route::Discord,
            ),
            (forward(&matrix, &matrix), Route::Matrix),
            (forward(&matrix, &discord), Route::Discord),
            (forward(&discord, &matrix), Route::Discord),
            (
                Command::MarkRead {
                    room: discord.clone(),
                    sync: true,
                },
                Route::Discord,
            ),
            (
                Command::MarkRead {
                    room: matrix.clone(),
                    sync: true,
                },
                Route::Matrix,
            ),
            (Command::Voice(voice::Command::Leave), Route::Voice),
            (Command::Logout, Route::Matrix),
        ] {
            assert_eq!(command.route(), route);
        }
    }

    #[test]
    fn decodes_animations_a_frame_at_a_time() {
        let picture = decode_picture(&gif(8, 3), 360, 270).unwrap();
        assert_eq!(picture.animation.as_ref().unwrap().delays, [50; 3]);
        let frames: Vec<_> = picture.frames().collect();
        assert_eq!(frames.len(), 3);
        assert_eq!(*picture.rgba, frames[0]);
        assert_eq!(frames[2][0], 40);
        // Shrunk to fit, keeping their shape; again from the start.
        let wide = decode_picture(&gif_sized(600, 300, 30), 360, 270).unwrap();
        assert_eq!((wide.width, wide.height), (360, 180));
        assert_eq!(wide.animation.as_ref().unwrap().delays.len(), 30);
        assert!(wide.frames().all(|frame| frame.len() == 360 * 180 * 4));
        assert_eq!(wide.frames().count(), 30);
        // A still picture has no frames.
        let still = decode_picture(&gif(8, 1), 360, 270).unwrap();
        assert!(still.animation.is_none() && still.frames().next().is_none());
    }

    #[test]
    fn reads_frame_timings_as_the_decoders_give_them() {
        let decoded = |data: &[u8], format: AnimationFormat| -> Vec<u32> {
            let (_, frames) = image_frames(std::io::Cursor::new(data), format).unwrap();
            frames
                .map(|frame| {
                    let (numerator, denominator) = frame.unwrap().delay().numer_denom_ms();
                    let delay = numerator / denominator;
                    if delay < 20 { 100 } else { delay }
                })
                .collect()
        };
        // A GIF's, some too short to be meant.
        let mut gif = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif);
            for (index, delay) in [0u32, 30, 70, 10, 1000].into_iter().enumerate() {
                let shade = image::Rgba([index as u8 * 40, 0, 0, 255]);
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(8, 6, shade),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(delay, 1),
                    ))
                    .unwrap();
            }
        }
        let delays = frame_delays(&gif, AnimationFormat::Gif).unwrap();
        assert_eq!(delays, [100, 30, 70, 100, 1000]);
        assert_eq!(delays, decoded(&gif, AnimationFormat::Gif));
        // An animated PNG's, as fractions of a second.
        let mut apng = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut apng, 4, 4);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_animated(3, 0).unwrap();
            let mut writer = encoder.write_header().unwrap();
            for (numerator, denominator) in [(1, 3), (7, 0), (1, 100)] {
                writer.set_frame_delay(numerator, denominator).unwrap();
                writer.write_image_data(&[90; 4 * 4 * 4]).unwrap();
            }
            writer.finish().unwrap();
        }
        let delays = frame_delays(&apng, AnimationFormat::Png).unwrap();
        assert_eq!(delays, [333, 70, 100]);
        assert_eq!(delays, decoded(&apng, AnimationFormat::Png));
        // A WebP's, from the fake's GIFs.
        let webp = include_bytes!("../../../testing/fixtures/gifs/wave.webp");
        let delays = frame_delays(webp, AnimationFormat::WebP).unwrap();
        assert_eq!(delays.len(), 8);
        assert_eq!(delays, decoded(webp, AnimationFormat::WebP));
        // Something else has none.
        assert!(frame_delays(b"RIFF", AnimationFormat::WebP).is_none());
        assert!(frame_delays(&gif, AnimationFormat::Png).is_none());
        assert!(
            frame_delays(b"GIF89a", AnimationFormat::Gif)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    #[cfg(feature = "discord")]
    fn animations_read_their_frames_from_the_kept_file() {
        use yapfast_discord::cache::KeptPicture;
        let data = gif(8, 3);
        let mut picture = decode_picture(&data, 360, 270).unwrap();
        let frames: Vec<_> = picture.frames().collect();
        let directory = tempfile::tempdir().unwrap();
        // Another file isn't the one it came from.
        let other = directory.path().join("other.gif");
        std::fs::write(&other, gif(8, 4)).unwrap();
        picture.read_from(KeptPicture::Plain(other));
        let memory = |picture: &Picture| {
            matches!(
                picture.animation.as_ref().unwrap().file,
                AnimationFile::Memory(_)
            )
        };
        assert!(memory(&picture));
        let kept = directory.path().join("kept.gif");
        std::fs::write(&kept, &data).unwrap();
        picture.read_from(KeptPicture::Plain(kept.clone()));
        assert!(!memory(&picture));
        assert_eq!(picture.frames().collect::<Vec<_>>(), frames);
        // Gone, it has no frames, and its first stays.
        std::fs::remove_file(&kept).unwrap();
        assert!(picture.frames().next().is_none());
    }

    #[test]
    fn sends_custom_emoji_in_discords_form() {
        let emoji = |name: &str, id: &str, animated| CustomEmoji {
            text: format!(":{name}:"),
            id: id.into(),
            animated,
            picture: String::new(),
        };
        let usable = [
            emoji("pog", "5", false),
            emoji("party", "6", true),
            emoji("pog", "7", false),
        ];
        assert_eq!(
            wire_emoji(
                "gg :pog: :party::pog: `:pog:` <:pog:5> :nope: 10:30:45",
                &usable
            ),
            "gg <:pog:5> <a:party:6><:pog:5> `:pog:` <:pog:5> :nope: 10:30:45"
        );
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut data = Vec::new();
        image::RgbaImage::new(width, height)
            .write_to(
                &mut std::io::Cursor::new(&mut data),
                image::ImageFormat::Png,
            )
            .unwrap();
        data
    }

    #[test]
    fn notification_text_keeps_lines_and_cuts_long_messages() {
        assert_eq!(
            notification_text("**Bold** start\n\n- one\n- `two`"),
            "Bold start\none\ntwo"
        );
        let long = "word ".repeat(100);
        let text = notification_text(&long);
        assert!(text.ends_with('…') && text.chars().count() <= 301);
    }

    #[test]
    fn pictures_shrink_to_fit_but_never_grow() {
        let small = decode_view(&png(800, 500)).unwrap();
        assert_eq!((small.width, small.height), (800, 500));
        let large = decode_view(&png(3840, 1200)).unwrap();
        assert_eq!((large.width, large.height), (1920, 600));
        let preview = decode_preview(&png(800, 500)).unwrap();
        assert_eq!((preview.width, preview.height), (360, 225));
    }

    #[test]
    fn reply_snippets_are_the_first_line_without_markdown() {
        assert_eq!(
            plain_snippet(
                "Welcome to **Yapfast**, with `code` and a [link](https://matrix.org)\nsecond line"
            ),
            "Welcome to Yapfast, with code and a link"
        );
        assert_eq!(plain_snippet("plain"), "plain");
        assert_eq!(plain_snippet(""), "");
    }
}
