//! Yapfast's Discord connection: one gateway session kept open while Yapfast runs.
//! It tracks the account's servers, their channels and who is in voice,
//! passes on live message events, and hands the voice server details for a
//! join to the voice worker.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

use crate::{
    activity,
    api::{self, Api, PAGE, Span},
    cache, forum, gift, invite, limits,
    marked::MarkedServer,
    member_list::{ListedMember, Presence},
    message::{self, Message, Names, Page},
    mfa,
    notifications::Change,
    profile::{self, Profile},
    ready::{self, PRIVATE, Server},
    scheduled_event, search, server_settings,
    settings::{self, CustomStatus},
    soundboard,
    stream::StreamServer,
};

mod live;
mod marks;
mod member_lists;
mod member_search;
mod publisher;
mod read_acks;
mod session;
mod stream_openings;
mod tracker;
mod voice_join;

use member_lists::MemberLists;
use session::Shared;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Connecting,
    Online,
    /// Disconnected, with a reason a person can act on.
    Offline(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceMember {
    pub user_id: String,
    pub name: String,
    /// The address of the avatar Discord shows for them on this server.
    pub avatar: String,
    /// Muted or deafened, by themselves or by the server.
    pub muted: bool,
    pub deafened: bool,
    /// Muted or deafened by the server, which a moderator chose, and
    /// Discord's client shows in red.
    pub server_muted: bool,
    pub server_deafened: bool,
    /// Sharing their screen with Go Live.
    pub streaming: bool,
    /// Their camera is on.
    pub video: bool,
    /// This account, in the call on another client, such as Discord's app
    /// on a phone, rather than on this gateway session.
    pub elsewhere: bool,
}

/// The linked account, as Discord shows it outside any server.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Account {
    pub name: String,
    /// The address of its avatar.
    pub avatar: String,
    /// Its Nitro: 0 none, 1 Nitro Classic, 2 Nitro, 3 Nitro Basic; see
    /// [`crate::api::upload_limit`].
    pub premium_type: u8,
    /// It has two-factor authentication on, which a server owner needs to
    /// change whether moderators do.
    #[serde(default)]
    pub mfa_enabled: bool,
    /// It has an email address, to which Discord sends the code that gives
    /// a server away when two-factor authentication is off; only whether,
    /// not the address.
    #[serde(default)]
    pub has_email: bool,
}

/// A DM or group DM, as the DM list shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Conversation {
    pub id: String,
    pub name: String,
    /// The other person's avatar in a DM, or a group DM's icon.
    pub avatar: Option<String>,
    pub group: bool,
    /// Everyone in it, this account included.
    pub people: usize,
    /// A message request, waiting to be accepted or ignored. Requests count
    /// no mentions, and spam isn't unread either.
    pub request: bool,
    /// A request Discord thinks is spam.
    pub spam: bool,
    /// The other person in a DM; none in a group.
    pub person: Option<String>,
    pub unread: bool,
    /// Unread messages, as every message in a DM counts as a mention.
    pub mentions: u64,
    /// The snowflake of the last message this account read here, or 0.
    #[serde(default)]
    pub last_read: u64,
    /// Muted in this account's notification settings, for now or for good.
    #[serde(default)]
    pub muted: bool,
    /// A call is going on here. Who is in it is in [`Snapshot::voice`].
    pub call: bool,
    /// Whom Discord is ringing for the call, this account perhaps among them.
    #[serde(skip)]
    pub ringing: Vec<VoiceMember>,
    /// A one-to-one DM's other person: whether they're around, when Discord
    /// says so of a friend, and their custom status or what they're doing.
    /// Not saved, since it's stale by the next start.
    #[serde(skip)]
    pub presence: Option<Presence>,
    #[serde(skip)]
    pub activity: Option<String>,
    #[serde(skip)]
    pub activities: crate::activity::Activities,
    /// A message request's latest message, as the requests list shows it.
    #[serde(default)]
    pub preview: Option<Preview>,
}

/// A message request's latest message, in short.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Preview {
    /// Its snowflake, which says when it came.
    pub id: String,
    /// Discord Markdown, its mentions as names.
    pub text: String,
    /// How many files it carries, said when it has no text.
    pub files: usize,
}

/// A friend request, sent to this account or by it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FriendRequest {
    pub user_id: String,
    pub name: String,
    pub avatar: String,
    /// Sent to this account, rather than by it.
    pub incoming: bool,
    /// Discord took it for spam.
    pub spam: bool,
}

/// Someone this account blocked, as the blocked list shows them.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlockedUser {
    pub user_id: String,
    pub name: String,
    /// Their username, such as `ann`, when their name is a display name.
    pub username: Option<String>,
    pub avatar: String,
}

/// A friend as the friends list shows them.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Friend {
    pub user_id: String,
    /// The nickname this account gave them, else their name.
    pub name: String,
    pub avatar: String,
    /// The nickname this account gave them.
    #[serde(default)]
    pub nickname: Option<String>,
    /// Whether they're around and their custom status or what they're doing,
    /// when Discord says. Not saved, since it's stale by the next start.
    #[serde(skip)]
    pub presence: Option<Presence>,
    #[serde(skip)]
    pub activity: Option<String>,
    #[serde(skip)]
    pub activities: crate::activity::Activities,
}

/// Someone this account can start a DM with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub avatar: String,
    /// A Discord friend, rather than someone known from a server or a message.
    pub friend: bool,
}

/// A page of a channel's list of threads, in the order Discord gave it:
/// each has [`ready::Channel::thread`], and its access set, and those this
/// account may not see are left out.
#[derive(Debug, Default)]
pub struct ThreadPage {
    pub threads: Vec<ready::Channel>,
    /// Discord has more after these.
    pub more: bool,
    /// In a forum, each post's first message, by post, as its card shows
    /// it; none where it was deleted.
    pub starters: Starters,
}

/// Forum posts' first messages, by post id; none for one deleted.
pub type Starters = Vec<(String, Option<forum::Starter>)>;

/// Someone who reacted to a message, as a list of them shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reactor {
    pub id: String,
    /// Their name where the message is: a server nickname, a display name
    /// or a username.
    pub name: String,
    pub username: String,
    pub avatar: String,
    /// It's this account.
    pub me: bool,
}

/// Messages a search found: one page, the newest first, and how many it
/// found in all.
#[derive(Debug, Default)]
pub struct Found {
    pub total: u32,
    pub messages: Vec<Message>,
}

/// The longest a search waits in all while Discord indexes what it
/// searches, asking again as often as Discord says.
const SEARCH_PATIENCE: Duration = Duration::from_secs(30);

/// How long to wait before searching again, as Discord's answer that it's
/// still indexing asks, or `None` once that would wait past
/// [`SEARCH_PATIENCE`] in all.
fn index_wait(answer: &Value, waited: &mut Duration) -> Option<Duration> {
    let wait = answer["retry_after"].as_f64().unwrap_or(1.0);
    let wait = crate::limits::sent_seconds(wait, Duration::from_millis(500), SEARCH_PATIENCE);
    *waited += wait;
    (*waited <= SEARCH_PATIENCE).then_some(wait)
}
/// How many minutes of quiet archive a thread, unless its channel says.
const DEFAULT_ARCHIVE: u16 = 4320;
/// Discord's answer to a search of a DM it hasn't indexed yet.
const INDEX_NOT_READY: u64 = 110000;

/// What the account looks like now. [`crate::cache`] keeps all but the live
/// parts, the connection and voice, to show on the next start.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    #[serde(skip)]
    pub status: Status,
    pub user_id: Option<String>,
    pub account: Option<Account>,
    /// Each server and this account's marks in it, each shared between
    /// snapshots while unchanged, so a change of the marks alone copies
    /// none of the server.
    pub servers: Vec<MarkedServer>,
    /// The folders in the account's server list. Their servers sit together
    /// in [`Self::servers`], in the folder's order.
    #[serde(default)]
    pub folders: Vec<settings::Folder>,
    /// DMs and group DMs, the latest conversation first. Each DM, and each
    /// list of people below, is shared between snapshots while it stays the
    /// same.
    pub conversations: Vec<Arc<Conversation>>,
    /// The account's friends, by user id.
    pub friends: Arc<[String]>,
    /// The same friends by name, as the friends list shows them.
    #[serde(default)]
    pub friend_list: Arc<[Friend]>,
    /// Those this account blocked, by user id. Their messages show collapsed,
    /// and they don't notify.
    pub blocked: Arc<[String]>,
    /// The same people by name, as the blocked list shows them.
    #[serde(default)]
    pub blocked_list: Arc<[BlockedUser]>,
    /// Those this account ignores, by user id.
    #[serde(default)]
    pub ignored: Arc<[String]>,
    /// This account's status: `online`, `idle`, `dnd` or `invisible`.
    pub presence: String,
    /// This account's custom status, until it ends.
    pub custom_status: Option<CustomStatus>,
    /// Whether this account shares the game it plays, as its settings say:
    /// not before Discord has said.
    #[serde(default)]
    pub show_game: bool,
    /// The game this account shows it plays now.
    #[serde(skip)]
    pub playing: Option<Arc<crate::own_activity::Playing>>,
    /// The soundboard's volume, from 0 to 100 on the power curve of the
    /// other volumes, and the sounds that play as this account joins
    /// voice, as its settings keep them; the volume is none until they say,
    /// which is [`soundboard::DEFAULT_VOLUME`].
    #[serde(skip)]
    pub soundboard_volume: Option<f32>,
    #[serde(skip)]
    pub join_sounds: Arc<[soundboard::JoinSound]>,
    /// Friend requests waiting for an answer: those sent to this account
    /// first, then those it sent, the newest first in each.
    pub friend_requests: Arc<[FriendRequest]>,
    /// Who is in each voice channel, by channel id and then by name.
    #[serde(skip)]
    pub voice: HashMap<String, Vec<VoiceMember>>,
    /// The channel whose member list is on screen, and how many people can
    /// read it.
    #[serde(skip)]
    pub listed: Option<(String, u64)>,
    /// The server channel or thread on screen, and a thread beside it, by
    /// id: a thread this account isn't in lists while it shows.
    #[serde(skip)]
    pub viewing: Vec<String>,
    /// Who is who, as the service knows them; see [`SharedNames`].
    #[serde(skip)]
    pub names: SharedNames,
}

/// The service's [`Names`], shared with each snapshot rather than copied
/// into it, so a menu can read a member's roles and timeout when it opens.
#[derive(Clone, Default)]
pub struct SharedNames(Arc<RwLock<Names>>);

impl SharedNames {
    pub fn new(names: Arc<RwLock<Names>>) -> Self {
        Self(names)
    }

    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, Names> {
        self.0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl std::fmt::Debug for SharedNames {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedNames")
    }
}

/// A message event from the gateway, for any channel.
#[derive(Clone, Debug)]
pub enum Live {
    Created(Message),
    /// A new message that should notify this account, by its notification settings,
    /// in a server or, with none, a DM.
    Notify {
        guild: Option<String>,
        message: Message,
    },
    Updated(Message),
    Deleted {
        channel: String,
        id: String,
    },
    Reaction {
        channel: String,
        message: String,
        key: String,
        emoji: String,
        me: bool,
        added: bool,
    },
    /// Someone voted for an answer of a poll, or took their vote back.
    Vote {
        channel: String,
        message: String,
        answer: u64,
        me: bool,
        added: bool,
    },
    /// Every reaction, or every one with `key`, was removed.
    ReactionsCleared {
        channel: String,
        message: String,
        key: Option<String>,
    },
    /// Someone else started typing; it lasts about ten seconds unless they type on.
    Typing {
        guild: Option<String>,
        channel: String,
        user: String,
        name: String,
    },
    /// Names were learned, so mentions and authors may render differently.
    Names,
    /// The member list of the channel last viewed, as it stands now.
    Members {
        channel: String,
        members: Vec<ListedMember>,
    },
    /// The account's favourite GIFs, all of them, as another client or this
    /// one changed them.
    FavoriteGifs(Vec<crate::gifs::Favorite>),
    /// The servers and channels this account used, as another client counted
    /// them; see [`crate::frecency`].
    PlacesUsed(Vec<(String, crate::frecency::Uses)>),
    /// Who watches a stream this account is in, and whether it's paused.
    StreamUpdated {
        stream_key: String,
        viewers: Vec<String>,
        paused: bool,
    },
    /// A stream this account shared or watched ended, with Discord's reason
    /// such as "user_requested" or "stream_full".
    StreamEnded {
        stream_key: String,
        reason: Option<String>,
    },
    /// This account is in `channel`, a voice channel of `guild`: where it
    /// joined, or where someone such as a bot moved it.
    VoiceChannel {
        guild: String,
        channel: String,
    },
    /// This account joined voice in `guild`, or with [`PRIVATE`] a DM call,
    /// on another client, such as Discord's app on a phone. That takes over
    /// from a call Yapfast has, which ends without leaving: leaving would end
    /// the other client's call too.
    VoiceElsewhere {
        guild: String,
    },
    /// Discord moved this account's call in `guild`, or with [`PRIVATE`] its
    /// DM call, to another voice server, which the call connects to.
    VoiceMoved {
        guild: String,
        server: VoiceServer,
    },
    /// Discord moved a stream this account shares or watches to another
    /// server.
    StreamMoved(StreamServer),
    /// Discord holds a request back until this account confirms with
    /// two-factor authentication, by one of `methods`; the prompt is
    /// answered by [`Discord::finish_mfa`] or [`Discord::cancel_mfa`] with
    /// its id.
    MfaRequired {
        id: u64,
        methods: Vec<mfa::Method>,
    },
    /// A server's settings changed, as GUILD_UPDATE gives them, for its
    /// settings' pages while they're open.
    ServerSettings {
        guild: String,
        settings: Box<server_settings::GuildSettings>,
    },
    /// A role was made or changed, as GUILD_ROLE_CREATE and
    /// GUILD_ROLE_UPDATE give it, for Roles while it's open.
    ServerRole {
        guild: String,
        role: Box<server_settings::Role>,
    },
    /// A role was deleted, by id.
    ServerRoleDeleted {
        guild: String,
        role: String,
    },
    /// Someone was banned, as GUILD_BAN_ADD gives them, without a reason,
    /// or unbanned, for Bans while it's open.
    ServerBan {
        guild: String,
        ban: Box<server_settings::Ban>,
        banned: bool,
    },
    /// A server's stickers, as GUILD_STICKERS_UPDATE gives them all, for
    /// Stickers while it's open.
    ServerStickers {
        guild: String,
        stickers: Vec<server_settings::GuildSticker>,
    },
    /// A server's soundboard sounds, all of them: Discord's answer to
    /// [`Discord::request_soundboard`], or GUILD_SOUNDBOARD_SOUNDS_UPDATE.
    SoundboardSounds {
        guild: String,
        sounds: Vec<soundboard::Sound>,
    },
    /// A server's sound was added or changed.
    SoundboardSound(soundboard::Sound),
    /// A server's sound was deleted, by id.
    SoundboardSoundDeleted {
        guild: String,
        sound: String,
    },
    /// Someone played a sound in a call this account is in, this account
    /// among them.
    SoundPlayed(soundboard::Played),
    /// The account's favourite sounds, all of them in order, as another
    /// client or this one changed them.
    FavoriteSounds(Vec<String>),
}

/// What a call needs to open the voice connection Discord assigned.
#[derive(Clone)]
pub struct VoiceServer {
    pub user_id: String,
    pub session_id: String,
    pub token: String,
    pub endpoint: String,
}

/// Leaves out the token, which no log should show.
impl std::fmt::Debug for VoiceServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceServer")
            .field("user_id", &self.user_id)
            .field("session_id", &self.session_id)
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

enum Request {
    /// A thread about to open, from its channel object, for the tracker to
    /// know and list: in place of the channel on screen, or beside it.
    OpenThread {
        guild: String,
        id: String,
        /// Its channel object; none for one the server's snapshot has.
        thread: Option<Value>,
        beside: bool,
    },
    /// The thread shown beside the channel on screen, or none.
    Beside {
        guild: String,
        thread: Option<String>,
    },
    Join {
        guild: String,
        channel: String,
        reply: oneshot::Sender<Result<VoiceServer>>,
    },
    Leave {
        guild: String,
    },
    SelfState {
        guild: String,
        channel: String,
        mute: bool,
        deaf: bool,
        video: bool,
    },
    View {
        guild: String,
        channel: Option<String>,
    },
    /// Asks for these members' server profiles; with `reply`, says when
    /// they arrived.
    Members {
        guild: String,
        users: Vec<String>,
        reply: Option<oneshot::Sender<Vec<String>>>,
    },
    Search {
        guild: String,
        query: String,
        reply: oneshot::Sender<Vec<String>>,
    },
    MarkRead {
        guild: String,
        channel: String,
        sync: bool,
    },
    GoLive {
        guild: String,
        channel: String,
        reply: oneshot::Sender<Result<StreamServer>>,
    },
    Watch {
        stream_key: String,
        reply: oneshot::Sender<Result<StreamServer>>,
    },
    EndStream {
        stream_key: String,
    },
    PauseStream {
        stream_key: String,
        paused: bool,
    },
    /// The custom status changed in the settings; this session shows it.
    CustomStatus(Option<CustomStatus>),
    /// The status changed in the settings; this session shows it.
    Status(String),
    /// Sharing the game played changed in the settings.
    ShowGame(bool),
    /// Nobody has used Yapfast since then, in Unix milliseconds, or someone
    /// is using it again; and whether it may show idle.
    Inactive(Option<u64>, bool),
    /// This gateway session's id, from READY.
    Session(oneshot::Sender<Option<String>>),
    /// Asks for these servers' soundboard sounds.
    Soundboard {
        guilds: Vec<String>,
    },
    /// Channels and categories moved, shown before Discord confirms it;
    /// `reply` gives where they were.
    Place {
        guild: String,
        placements: Vec<ready::Placement>,
        reply: Option<oneshot::Sender<Vec<ready::Placement>>>,
    },
}

/// What a channel's dialog edits; see [`Discord::channel_settings`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChannelSettings {
    pub name: String,
    pub topic: String,
    pub slowmode: u32,
    pub nsfw: bool,
}

impl ChannelSettings {
    fn of(channel: &Value) -> Self {
        Self {
            name: channel["name"].as_str().unwrap_or_default().to_owned(),
            topic: channel["topic"].as_str().unwrap_or_default().to_owned(),
            slowmode: channel["rate_limit_per_user"]
                .as_u64()
                .and_then(|seconds| u32::try_from(seconds).ok())
                .unwrap_or_default(),
            nsfw: channel["nsfw"].as_bool().unwrap_or(false),
        }
    }
}

/// A handle to the connection. It ends when every handle is dropped.
#[derive(Clone)]
pub struct Discord {
    requests: mpsc::UnboundedSender<Request>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    live: broadcast::Sender<Live>,
    api: Arc<Api>,
    names: Arc<RwLock<Names>>,
    /// Profiles fetched lately, oldest first.
    profiles: Arc<Mutex<Vec<Kept>>>,
    /// The threads a list of a channel's threads found, by id, while it's
    /// open: one opens from what Discord gave rather than asking again.
    listed_threads: Arc<Mutex<HashMap<String, Value>>>,
    /// New DMs started lately, which Discord limits; see [`Self::dm_with`].
    new_dms: Arc<Mutex<limits::Window>>,
    /// The addresses of the icons of applications activities showed, by
    /// id; none for one without an icon.
    application_icons: Arc<Mutex<HashMap<String, Option<String>>>>,
    /// Requests held back for two-factor authentication, waiting for the
    /// person; see [`Self::stepping_up`].
    mfa: Arc<mfa::Prompts>,
    /// Discord's own soundboard sounds, once a call asked for them: Discord's
    /// client asks once a session too.
    default_sounds: Arc<tokio::sync::Mutex<Option<Arc<[soundboard::Sound]>>>>,
}

/// Discord allows starting 10 new DMs with people in 10 minutes, and treats
/// many more as spam; Yapfast keeps a margin.
const NEW_DMS: usize = 8;
const NEW_DM_SPAN: Duration = Duration::from_secs(10 * 60);

/// A profile fetched lately, for a user in a server.
struct Kept {
    user: String,
    guild: String,
    at: Instant,
    /// Their server nickname and avatar when it was fetched: once
    /// GUILD_MEMBER_UPDATE changes either, it's fetched again, with the
    /// rest of what they changed in their server profile.
    member: (Option<String>, String),
    profile: Profile,
}

const LIVE_BACKLOG: usize = 256;
/// Profiles are fetched again after this long; Discord limits how often.
const PROFILE_AGE: Duration = Duration::from_secs(5 * 60);
const PROFILES: usize = 32;

impl Discord {
    /// Connects on the current Tokio runtime and keeps reconnecting until
    /// Discord rejects the token.
    pub fn start(token: String) -> Result<Self> {
        let api = Arc::new(Api::new(token.clone())?);
        let (requests, receiver) = mpsc::unbounded_channel();
        // The account as last seen shows while Yapfast connects.
        let key = crate::token::cache_key()
            .inspect_err(|error| tracing::warn!(%error, "Discord's files won't be kept"))
            .ok();
        let cache::Loaded {
            snapshot: cached,
            names,
            lists,
        } = cache::load(&token, key);
        let presence = settings::presence(
            if cached.presence.is_empty() {
                "online"
            } else {
                &cached.presence
            },
            cached.custom_status.as_ref(),
            now_ms(),
            settings::Away::default(),
        );
        let names = Arc::new(RwLock::new(names));
        let mut cached = cached;
        cached.names = SharedNames::new(names.clone());
        let (snapshots, watcher) = watch::channel(Arc::new(cached));
        let (live, _) = broadcast::channel(LIVE_BACKLOG);
        // Sharing waits for READY unless an earlier session's settings said.
        let (show_game, shown) = watch::channel(watcher.borrow().show_game);
        let activity = crate::own_activity::start(api.clone(), watcher.clone(), shown);
        tokio::spawn(cache::keep(watcher.clone(), names.clone()));
        tokio::spawn(session::run(
            token,
            receiver,
            Shared {
                snapshots,
                live: live.clone(),
                names: names.clone(),
                api: api.clone(),
                lists: MemberLists::new(
                    lists,
                    live.clone(),
                    names.clone(),
                    Presence::parse(&presence),
                ),
                presence,
                activity,
                show_game,
                inactive: None,
                viewing: None,
                beside: None,
            },
        ));
        Ok(Self {
            requests,
            snapshots: watcher,
            live,
            api,
            names,
            profiles: Arc::default(),
            listed_threads: Arc::default(),
            new_dms: Arc::new(Mutex::new(limits::Window::new(NEW_DMS, NEW_DM_SPAN))),
            application_icons: Arc::default(),
            mfa: Arc::default(),
            default_sounds: Arc::default(),
        })
    }

    pub fn snapshots(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.snapshots.clone()
    }

    /// Message events for every channel, from now on.
    pub fn live(&self) -> broadcast::Receiver<Live> {
        self.live.subscribe()
    }

    /// Tells Discord a server's channel is on screen, so its messages arrive
    /// live. With a text `channel`, its member list arrives as [`Live::Members`].
    /// DMs arrive live anyway; with [`PRIVATE`] as the server, a group DM's
    /// members arrive the same way.
    pub fn view(&self, guild: &str, channel: Option<&str>) {
        let _ = self.requests.send(Request::View {
            guild: guild.into(),
            channel: channel.map(str::to_owned),
        });
    }

    /// Marks a channel read in Yapfast, and on Discord too with `sync`.
    pub fn mark_read(&self, guild: &str, channel: &str, sync: bool) {
        let _ = self.requests.send(Request::MarkRead {
            guild: guild.into(),
            channel: channel.into(),
            sync,
        });
    }

    /// A page of a channel's messages.
    pub async fn history(&self, guild: &str, channel: &str, span: Span<'_>) -> Result<Page> {
        let around = match span {
            Span::Around(id) => id.parse::<u64>().ok(),
            _ => None,
        };
        let values = self.api.messages(channel, span).await?;
        // The open channel shows them again once their authors arrive.
        let (mut messages, _) = self.learn(guild, &values, false);
        messages.sort_by_key(Message::order);
        let ids = || {
            values
                .iter()
                .filter_map(|value| value["id"].as_str()?.parse::<u64>().ok())
        };
        // Discord sends up to half a page from before the snowflake and
        // half from it on. Its client takes fewer than that before it, less
        // one, as the channel's start, and fewer from it on as its newest.
        let (full, newest) = match around {
            Some(around) => {
                let before = ids().filter(|&id| id < around).count();
                (
                    before + 1 >= PAGE.div_ceil(2),
                    values.len() - before < PAGE / 2,
                )
            }
            None => (values.len() >= PAGE, false),
        };
        Ok(Page {
            messages,
            oldest: ids().min(),
            full,
            newest,
        })
    }

    /// Messages from a server's channel, or with [`PRIVATE`] a DM, learning
    /// their authors. Messages fetched over HTTP carry no server profiles,
    /// so the nicknames, server avatars and roles of authors not known yet
    /// are asked for; they arrive as [`Live::Names`], and with `wait` the
    /// receiver returned, if any, hears when.
    fn learn(
        &self,
        guild: &str,
        values: &[serde_json::Value],
        wait: bool,
    ) -> (Vec<Message>, Option<oneshot::Receiver<Vec<String>>>) {
        let server = server(guild);
        let mut unknown = HashSet::new();
        {
            let mut names = self.write_names();
            for value in values {
                names.learn_message(server, value);
                if let (Some(guild), Some(author)) = (server, value["author"]["id"].as_str())
                    && !names.knows_member(guild, author)
                {
                    unknown.insert(author.to_owned());
                }
            }
        }
        let mut arrived = None;
        if !unknown.is_empty() {
            let reply = wait.then(|| {
                let (reply, receiver) = oneshot::channel();
                arrived = Some(receiver);
                reply
            });
            let _ = self.requests.send(Request::Members {
                guild: guild.into(),
                users: unknown.into_iter().collect(),
                reply,
            });
        }
        (values.iter().filter_map(message::parse).collect(), arrived)
    }

    /// [`Self::learn`], waiting a while for the authors' server profiles,
    /// for messages shown once rather than kept up to date.
    async fn learn_authors(&self, guild: &str, values: &[serde_json::Value]) -> Vec<Message> {
        let (messages, arrived) = self.learn(guild, values, true);
        if let Some(arrived) = arrived {
            let _ = tokio::time::timeout(SEARCH_TIMEOUT, arrived).await;
        }
        messages
    }

    /// Who reacted to the message `id` with `emoji`, in the API's form: up
    /// to `limit`, by user id, after the user `after`, named as
    /// [`Self::people`] names them.
    pub async fn reactors(
        &self,
        guild: &str,
        channel: &str,
        id: &str,
        emoji: &str,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Reactor>> {
        let users = self.api.reactors(channel, id, emoji, after, limit).await?;
        Ok(self.people(guild, &users).await)
    }

    /// Who voted for `answer` of the poll of the message `id`, as
    /// [`Self::reactors`] lists who reacted.
    pub async fn voters(
        &self,
        guild: &str,
        channel: &str,
        id: &str,
        answer: u64,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Reactor>> {
        let users = self.api.voters(channel, id, answer, after, limit).await?;
        Ok(self.people(guild, &users).await)
    }

    /// Votes for `answers` of a message's poll; none takes the vote back.
    pub async fn vote(&self, channel: &str, id: &str, answers: &[u64]) -> Result<()> {
        self.api.vote(channel, id, answers).await
    }

    /// Closes this account's poll now.
    pub async fn end_poll(&self, channel: &str, id: &str) -> Result<()> {
        self.api.end_poll(channel, id).await
    }

    /// Users as a list of people shows them, by their names in `guild`. In
    /// a server, the nicknames and avatars of those not known there are
    /// asked for first, waiting a while for them.
    async fn people(&self, guild: &str, users: &[serde_json::Value]) -> Vec<Reactor> {
        let in_server = server(guild);
        let unknown: Vec<String> = {
            let mut names = self.write_names();
            for user in users {
                names.learn_user(user);
            }
            users
                .iter()
                .filter_map(|user| user["id"].as_str())
                .filter(|user| in_server.is_some_and(|guild| !names.knows_member(guild, user)))
                .map(str::to_owned)
                .collect()
        };
        if !unknown.is_empty() {
            let (reply, arrived) = oneshot::channel();
            let _ = self.requests.send(Request::Members {
                guild: guild.into(),
                users: unknown,
                reply: Some(reply),
            });
            let _ = tokio::time::timeout(SEARCH_TIMEOUT, arrived).await;
        }
        let me = self.snapshots.borrow().user_id.clone();
        let names = self.read_names();
        users
            .iter()
            .filter_map(|user| {
                let id = user["id"].as_str()?;
                let username = names.username(id).unwrap_or_default().to_owned();
                Some(Reactor {
                    id: id.to_owned(),
                    name: in_server
                        .and_then(|guild| names.name(guild, id))
                        .or_else(|| names.user_name(id))
                        .map_or_else(|| username.clone(), str::to_owned),
                    username,
                    avatar: names.avatar(in_server, id),
                    me: me.as_deref() == Some(id),
                })
            })
            .collect()
    }

    /// A channel's pinned messages, the latest pinned first: a server's
    /// channel, or with [`PRIVATE`] a DM.
    pub async fn pins(&self, guild: &str, channel: &str) -> Result<Vec<Message>> {
        let values = self.api.pins(channel).await?;
        Ok(self.learn_authors(guild, &values).await)
    }

    /// A page of the messages a search finds, the newest first, from the
    /// `offset`th: in the DM `channel` with [`PRIVATE`], or across a server.
    /// `text` is what the search box took, filters and all; see
    /// [`search::Query`]. Discord indexes a DM or a server the first time
    /// it's searched, and asks to be asked again until it has.
    pub async fn search(
        &self,
        guild: &str,
        channel: Option<&str>,
        text: &str,
        offset: u32,
    ) -> Result<Found> {
        let query = search::Query::parse(text).map_err(|why| anyhow!(why))?;
        if query.is_empty() {
            bail!("Type something to search for.");
        }
        let in_server = server(guild);
        let channel = match (in_server, channel) {
            (Some(_), _) => "",
            (None, Some(channel)) => channel,
            (None, None) => bail!("Yapfast can't tell which DM to search."),
        };
        let mut params = query.params();
        for name in &query.channels {
            let Some(guild) = in_server else {
                bail!("in: picks a server's channels, and a DM has none.");
            };
            let id = self
                .channel_named(guild, name)
                .ok_or_else(|| anyhow!("This server has no channel called #{name}."))?;
            params.push(("channel_id", id));
        }
        for name in &query.from {
            params.push(("author_id", self.person_named(guild, name).await?));
        }
        for name in &query.mentions {
            params.push(("mentions", self.person_named(guild, name).await?));
        }
        let mut waited = Duration::ZERO;
        loop {
            let mut answer = self.api.search(in_server, channel, &params, offset).await?;
            if let serde_json::Value::Array(pages) = answer["messages"].take() {
                // Each result came with the messages around it, which Discord
                // no longer sends; the first is the one found.
                let values: Vec<_> = pages
                    .into_iter()
                    .filter_map(|mut page| page.get_mut(0).map(serde_json::Value::take))
                    .collect();
                return Ok(Found {
                    total: answer["total_results"].as_u64().unwrap_or_default() as u32,
                    messages: self.learn_authors(guild, &values).await,
                });
            }
            if answer["code"].as_u64() != Some(INDEX_NOT_READY) {
                break;
            }
            let Some(wait) = index_wait(&answer, &mut waited) else {
                break;
            };
            tokio::time::sleep(wait).await;
        }
        Err(anyhow!(
            "Discord is still getting these messages ready to search. Try again in a moment."
        ))
    }

    /// The id of a server's text channel by its name, the same letters
    /// first, as `in:` names it.
    fn channel_named(&self, guild: &str, name: &str) -> Option<String> {
        let snapshot = self.snapshots.borrow().clone();
        let server = snapshot.servers.iter().find(|server| server.id == guild)?;
        let channels = &server.text_channels;
        channels
            .iter()
            .find(|channel| channel.name == name)
            .or_else(|| {
                channels
                    .iter()
                    .find(|channel| channel.name.eq_ignore_ascii_case(name))
            })
            .map(|channel| channel.id.clone())
    }

    /// The id of someone a search's `from:` or `mentions:` names: in a
    /// server, the member Discord finds by it, as its search box asks; in
    /// DMs, someone Yapfast knows by it. One called exactly that comes first.
    async fn person_named(&self, guild: &str, name: &str) -> Result<String> {
        let found = match server(guild) {
            Some(guild) => self.search_members(guild, name).await?,
            None => self.read_names().find_users(name),
        };
        let called = |id: &String| {
            [self.name(guild, id), self.username(id)]
                .into_iter()
                .flatten()
                .any(|known| known.eq_ignore_ascii_case(name))
        };
        found
            .iter()
            .find(|id| called(id))
            .or(found.first())
            .cloned()
            .ok_or_else(|| anyhow!("Yapfast can't find anyone called {name} here."))
    }

    /// Makes a group DM with `users`; returns its channel id. The DM list
    /// has it once Discord's CHANNEL_CREATE arrives.
    pub async fn create_group(&self, users: &[String]) -> Result<String> {
        self.api.create_group(users).await
    }

    /// Adds `users` to a group DM, one at a time, as Discord's client does.
    pub async fn add_to_group(&self, channel: &str, users: &[String]) -> Result<()> {
        for user in users {
            self.api.add_recipient(channel, user).await?;
        }
        Ok(())
    }

    /// Sends a message, or a reply in the same channel as `options` says,
    /// with files given as their names and contents.
    pub async fn send(
        &self,
        guild: &str,
        channel: &str,
        content: &str,
        options: crate::api::Sending<'_>,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<Message> {
        let uploads = self.upload(channel, files).await?;
        let sent = self
            .api
            .send(server(guild), channel, content, options, &uploads)
            .await;
        if sent.is_err() {
            self.discard(&uploads).await;
        }
        let value = sent?;
        self.write_names().learn_message(server(guild), &value);
        message::parse(&value).ok_or_else(|| anyhow!("could not understand Discord's answer"))
    }

    /// Uploads files for a message in `channel`, given as their names and
    /// contents, all at once as Discord's client does; if one fails, the
    /// others are thrown away.
    async fn upload(
        &self,
        channel: &str,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<Vec<crate::api::Upload>> {
        let results = futures_util::future::join_all(
            files
                .into_iter()
                .map(|(name, data)| async move { self.api.upload(channel, &name, data).await }),
        )
        .await;
        let mut uploads = Vec::with_capacity(results.len());
        let mut failed = None;
        for result in results {
            match result {
                Ok(upload) => uploads.push(upload),
                Err(error) => failed = failed.or(Some(error)),
            }
        }
        match failed {
            Some(error) => {
                self.discard(&uploads).await;
                Err(error)
            }
            None => Ok(uploads),
        }
    }

    /// Throws away uploads no message took.
    async fn discard(&self, uploads: &[crate::api::Upload]) {
        futures_util::future::join_all(uploads.iter().map(|upload| self.api.discard(upload))).await;
    }

    /// Posts in a server's forum, as Discord's client does: a post named
    /// `title` with the forum's tags by id, archiving after the forum's
    /// default quiet, or three days, and a first message of `content` and
    /// files, given as their names and contents, uploaded to the forum.
    /// Returns the post's id; it lists once Discord's THREAD_CREATE comes.
    pub async fn post_in_forum(
        &self,
        guild: &str,
        forum: &str,
        title: &str,
        tags: &[String],
        content: &str,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<String> {
        let archive_after = {
            let snapshot = self.snapshots.borrow();
            let forum = snapshot
                .servers
                .iter()
                .find(|server| server.id == guild)
                .and_then(|server| server.channel(forum))
                .ok_or_else(|| anyhow!("Yapfast doesn't know that forum."))?;
            match forum.archive_after {
                0 => DEFAULT_ARCHIVE,
                minutes => minutes,
            }
        };
        let uploads = self.upload(forum, files).await?;
        let posted = self
            .api
            .create_post(forum, title, archive_after, tags, content, &uploads)
            .await;
        if posted.is_err() {
            self.discard(&uploads).await;
        }
        posted?["id"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("could not understand Discord's answer"))
    }

    /// Posts a poll in `channel`, of `guild`. The gateway shows it.
    pub async fn send_poll(
        &self,
        guild: &str,
        channel: &str,
        poll: &crate::api::NewPoll,
    ) -> Result<()> {
        let nonce = crate::message::nonce();
        let sent = self.api.send_poll(channel, poll, &nonce).await?;
        self.write_names().learn_message(server(guild), &sent);
        Ok(())
    }

    /// Forwards message `id` of `channel`, in `guild`, to `target`, in
    /// `target_guild`, as Discord's client does; `silent` notifies nobody.
    /// The gateway shows it.
    pub async fn forward(
        &self,
        (guild, channel, id): (&str, &str, &str),
        (target_guild, target): (&str, &str),
        silent: bool,
    ) -> Result<()> {
        let nonce = crate::message::nonce();
        let sent = self
            .api
            .forward(server(guild), channel, id, target, &nonce, silent)
            .await?;
        self.write_names()
            .learn_message(server(target_guild), &sent);
        Ok(())
    }

    /// Sends the text added to a forward after it, as a message of its own:
    /// Discord takes no text with a forward.
    pub async fn send_forward_note(
        &self,
        (guild, channel): (&str, &str),
        note: &str,
        silent: bool,
    ) -> Result<()> {
        let nonce = crate::message::nonce();
        let sent = self
            .api
            .send_from("forwarding", channel, note, &nonce, silent)
            .await?;
        self.write_names().learn_message(server(guild), &sent);
        Ok(())
    }

    pub async fn edit(&self, channel: &str, id: &str, content: &str) -> Result<Message> {
        let value = self.api.edit(channel, id, content).await?;
        message::parse(&value).ok_or_else(|| anyhow!("could not understand Discord's answer"))
    }

    pub async fn delete(&self, channel: &str, id: &str) -> Result<()> {
        self.api.delete(channel, id).await
    }

    /// Pins a message, or unpins it. The gateway's MESSAGE_UPDATE says so too.
    pub async fn pin(&self, channel: &str, id: &str, pinned: bool) -> Result<()> {
        self.api.pin(channel, id, pinned).await
    }

    /// A user's profile, in a server or with [`PRIVATE`] outside any. One
    /// fetched in the last few minutes is given again. Otherwise `kept` gets
    /// the one fetched last, if there's one, even before Yapfast started,
    /// to show while Discord answers.
    pub async fn profile(
        &self,
        guild: &str,
        user: &str,
        kept: impl FnOnce(Profile),
    ) -> Result<Profile> {
        let same = |kept: &&Kept| kept.user == user && kept.guild == guild;
        let member = self.member_face(guild, user);
        let earlier = {
            let profiles = self.lock_profiles();
            let earlier = profiles.iter().find(same);
            if let Some(fresh) =
                earlier.filter(|fresh| fresh.at.elapsed() < PROFILE_AGE && fresh.member == member)
            {
                return Ok(fresh.profile.clone());
            }
            earlier.map(|earlier| earlier.profile.clone())
        };
        let earlier = match earlier {
            Some(earlier) => Some(earlier),
            None => {
                let (guild, user) = (guild.to_owned(), user.to_owned());
                tokio::task::spawn_blocking(move || cache::profile(&guild, &user))
                    .await
                    .ok()
                    .flatten()
            }
        };
        if let Some(earlier) = earlier {
            kept(earlier);
        }
        let value = self.api.profile(user, server(guild)).await?;
        self.write_names().learn_user(&value["user"]);
        let profile = profile::parse(&value, server(guild))
            .ok_or_else(|| anyhow!("could not understand Discord's answer"))?;
        {
            let mut profiles = self.lock_profiles();
            profiles.retain(|old| !same(&old));
            profiles.push(Kept {
                user: user.to_owned(),
                guild: guild.to_owned(),
                at: Instant::now(),
                member: self.member_face(guild, user),
                profile: profile.clone(),
            });
            if profiles.len() > PROFILES {
                profiles.remove(0);
            }
        }
        let (guild, user, saving) = (guild.to_owned(), user.to_owned(), profile.clone());
        tokio::task::spawn_blocking(move || cache::save_profile(&guild, &user, &saving));
        Ok(profile)
    }

    /// A member's nickname and avatar in a server, as Names knows them.
    fn member_face(&self, guild: &str, user: &str) -> (Option<String>, String) {
        let names = self.read_names();
        (
            names.nickname(guild, user).map(str::to_owned),
            names.avatar(server(guild), user),
        )
    }

    /// Does `work`, and when Discord holds it back for two-factor
    /// authentication, asks the person with [`Live::MfaRequired`] and does
    /// it again once they confirm, as Discord's client does. Cancelling
    /// fails it. `work` may send several requests; each carries the token.
    pub async fn stepping_up<T, F, Work>(&self, mut work: Work) -> Result<T>
    where
        Work: FnMut() -> F,
        F: std::future::Future<Output = Result<T>>,
    {
        let mut token = None;
        loop {
            let result = match token.take() {
                Some(token) => mfa::with_token(token, work()).await,
                None => work().await,
            };
            let error = match result {
                Ok(done) => return Ok(done),
                Err(error) => error,
            };
            let Some(required) = error.downcast_ref::<mfa::Required>() else {
                return Err(error);
            };
            let (id, answered) = self.mfa.ask(required.ticket.clone());
            let _ = self.live.send(Live::MfaRequired {
                id,
                methods: required.methods.clone(),
            });
            match answered.await {
                Ok(mfa::Answer::Token(confirmed)) => token = Some(confirmed),
                Ok(mfa::Answer::Cancelled) | Err(_) => {
                    bail!("Two-factor authentication was cancelled.")
                }
            }
        }
    }

    /// Confirms a prompt of [`Live::MfaRequired`] with a code, password or
    /// backup code; the request waiting on it goes again. A confirmation
    /// Discord refuses fails here and leaves the prompt open.
    pub async fn finish_mfa(&self, id: u64, method: mfa::Method, data: &str) -> Result<()> {
        let ticket = self
            .mfa
            .ticket(id)
            .context("That confirmation is no longer waiting.")?;
        let token = self.api.finish_mfa(&ticket, method, data).await?;
        self.mfa.answer(id, mfa::Answer::Token(token));
        Ok(())
    }

    /// Has Discord text a code for a prompt; the phone number it went to,
    /// partly hidden, comes back.
    pub async fn send_mfa_sms(&self, id: u64) -> Result<String> {
        let ticket = self
            .mfa
            .ticket(id)
            .context("That confirmation is no longer waiting.")?;
        self.api.send_mfa_sms(&ticket).await
    }

    /// Gives up on a prompt; the request waiting on it fails.
    pub fn cancel_mfa(&self, id: u64) {
        self.mfa.answer(id, mfa::Answer::Cancelled);
    }

    /// This account's own profile, for its editor, fetched afresh: in
    /// `guild`, or its main one.
    pub async fn own_profile(&self, guild: Option<&str>) -> Result<profile::OwnProfile> {
        let me = self.snapshots.borrow().user_id.clone();
        let me = me.ok_or_else(|| anyhow!("Discord hasn't said who this account is yet"))?;
        let value = self.api.profile(&me, guild).await?;
        match guild {
            Some(guild) => profile::parse_server(&value, guild),
            None => profile::parse_main(&value),
        }
        .ok_or_else(|| anyhow!("could not understand Discord's answer"))
    }

    /// Changes this account's profile in a server as Discord's client
    /// does: `member`'s fields, its nickname and avatar, go to its member
    /// object, then `profile`'s, its banner, bio and pronouns, to its
    /// server profile, each only when it has any. GUILD_MEMBER_UPDATE
    /// brings the member's change back; the rest has no gateway event.
    pub async fn save_server_profile(
        &self,
        guild: &str,
        member: &Value,
        profile: &Value,
    ) -> Result<()> {
        if some(member) {
            self.api.modify_member_me(guild, member).await?;
        }
        if some(profile) {
            self.api.modify_guild_profile(guild, profile).await?;
        }
        self.forget_own_profile(Some(guild));
        Ok(())
    }

    /// A server's profile, for its settings' Server Profile page.
    pub async fn guild_profile(&self, guild: &str) -> Result<server_settings::ServerProfile> {
        let profile = self.api.server_profile(guild).await?;
        Ok(server_settings::parse_profile(guild, &profile))
    }

    /// Changes a server's profile with `body`'s fields, as Server Profile's
    /// Save Changes does, giving it as it now stands; GUILD_UPDATE follows.
    pub async fn save_guild_profile(
        &self,
        guild: &str,
        body: &Value,
    ) -> Result<server_settings::ServerProfile> {
        let profile = self.api.modify_server_profile(guild, body).await?;
        Ok(server_settings::parse_profile(guild, &profile))
    }

    /// A server's settings, for its Engagement and Boost Perks pages.
    pub async fn guild_settings(&self, guild: &str) -> Result<server_settings::GuildSettings> {
        let settings = self.api.guild(guild).await?;
        Ok(server_settings::parse_guild(&settings))
    }

    /// Changes a server's settings with `body`'s fields, giving them as
    /// they now stand; GUILD_UPDATE follows.
    pub async fn save_guild_settings(
        &self,
        guild: &str,
        body: &Value,
    ) -> Result<server_settings::GuildSettings> {
        let settings = self.api.modify_guild(guild, body).await?;
        Ok(server_settings::parse_guild(&settings))
    }

    /// Up to [`api::BANS_PAGE`] of a server's bans, after a user's id.
    pub async fn bans(
        &self,
        guild: &str,
        after: Option<&str>,
    ) -> Result<Vec<server_settings::Ban>> {
        let bans = self.api.bans(guild, after).await?;
        Ok(parse_bans(&bans))
    }

    /// Bans' search: the first name in `query` and every user id in it,
    /// split at commas, as Discord's client splits it.
    pub async fn search_bans(&self, guild: &str, query: &str) -> Result<Vec<server_settings::Ban>> {
        let (ids, names): (Vec<&str>, Vec<&str>) = query
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .partition(|part| {
                (17..=19).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_digit())
            });
        let ids: Vec<String> = ids.into_iter().map(str::to_owned).collect();
        let bans = self
            .api
            .search_bans(guild, names.first().copied(), &ids)
            .await?;
        Ok(parse_bans(&bans))
    }

    /// A page of a server's audit log, as Discord sends it; see
    /// [`Api::audit_log`].
    pub async fn audit_log(
        &self,
        guild: &str,
        before: Option<&str>,
        user: Option<&str>,
        action: Option<u16>,
    ) -> Result<Value> {
        self.api.audit_log(guild, before, user, action).await
    }

    /// A chunk of Members' search; see [`crate::api::Api::search_members`].
    pub async fn members_search(
        &self,
        guild: &str,
        body: &Value,
    ) -> Result<crate::api::MembersSearch> {
        self.api.search_members(guild, body).await
    }

    pub async fn guild_invites(&self, guild: &str) -> Result<Vec<server_settings::Invite>> {
        let invites = self.api.guild_invites(guild).await?;
        Ok(invites
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(server_settings::parse_invite)
            .collect())
    }

    pub async fn revoke_invite(&self, code: &str) -> Result<()> {
        self.api.revoke_invite(code).await
    }

    pub async fn guild_emojis(&self, guild: &str) -> Result<Vec<server_settings::GuildEmoji>> {
        let emojis = self.api.guild_emojis(guild).await?;
        Ok(emojis
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(server_settings::parse_emoji)
            .collect())
    }

    /// Adds a custom emoji, `image` a data URI; gives back the emoji.
    pub async fn create_emoji(
        &self,
        guild: &str,
        name: &str,
        image: &str,
    ) -> Result<server_settings::GuildEmoji> {
        let emoji = self.api.create_emoji(guild, name, image).await?;
        server_settings::parse_emoji(&emoji).context("could not understand Discord's answer")
    }

    pub async fn rename_emoji(&self, guild: &str, emoji: &str, name: &str) -> Result<()> {
        self.api.rename_emoji(guild, emoji, name).await.map(drop)
    }

    pub async fn delete_emoji(&self, guild: &str, emoji: &str) -> Result<()> {
        self.api.delete_emoji(guild, emoji).await
    }

    pub async fn guild_stickers(&self, guild: &str) -> Result<Vec<server_settings::GuildSticker>> {
        let stickers = self.api.guild_stickers(guild).await?;
        Ok(stickers
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(server_settings::parse_sticker)
            .collect())
    }

    /// Uploads a sticker: its name, related emoji and description, and the
    /// file by its name, type and bytes.
    pub async fn create_sticker(
        &self,
        guild: &str,
        fields: [(&str, &str); 3],
        file: (&str, &str, &[u8]),
    ) -> Result<()> {
        self.api.create_sticker(guild, fields, file).await.map(drop)
    }

    pub async fn edit_sticker(
        &self,
        guild: &str,
        sticker: &str,
        name: &str,
        tags: &str,
        description: &str,
    ) -> Result<()> {
        let body = serde_json::json!({"name": name, "tags": tags, "description": description});
        self.api.edit_sticker(guild, sticker, &body).await.map(drop)
    }

    pub async fn delete_sticker(&self, guild: &str, sticker: &str) -> Result<()> {
        self.api.delete_sticker(guild, sticker).await
    }

    /// Pauses invites and DMs for `hours`, or resumes them; gives back when
    /// each pause ends, in Unix seconds.
    pub async fn pause_server(
        &self,
        guild: &str,
        invites: bool,
        dms: bool,
        hours: u8,
    ) -> Result<(Option<u64>, Option<u64>)> {
        let until = (invites || dms).then(|| {
            let end = std::time::SystemTime::now()
                + std::time::Duration::from_secs(u64::from(hours) * 3600);
            chrono::DateTime::<chrono::Utc>::from(end)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        });
        let body = serde_json::json!({
            "invites_disabled_until": until.as_ref().filter(|_| invites),
            "dms_disabled_until": until.as_ref().filter(|_| dms),
            "lockdown_duration_hours": (invites || dms).then_some(hours),
        });
        let incidents = self.api.incident_actions(guild, &body).await?;
        Ok(server_settings::parse_incidents(&incidents))
    }

    pub async fn unban(&self, guild: &str, user: &str) -> Result<()> {
        self.api.unban(guild, user).await
    }

    /// Sets whether a server's moderators need two-factor authentication,
    /// giving the server's settings as they then stand.
    pub async fn set_mfa_level(
        &self,
        guild: &str,
        level: u8,
    ) -> Result<server_settings::GuildSettings> {
        self.api.set_mfa_level(guild, level).await?;
        self.guild_settings(guild).await
    }

    /// Deletes a server, as its owner.
    pub async fn delete_guild(&self, guild: &str) -> Result<()> {
        self.api.delete_guild(guild).await
    }

    /// Gives a server to another member, as its owner: with the code
    /// Discord emailed, or none when two-factor authentication confirms
    /// it.
    pub async fn transfer_guild(&self, guild: &str, owner: &str, code: Option<&str>) -> Result<()> {
        self.api.transfer_guild(guild, owner, code).await
    }

    /// Has Discord email this account the code that gives a server away.
    pub async fn send_guild_pincode(&self, guild: &str) -> Result<()> {
        self.api.send_guild_pincode(guild).await
    }

    /// What Roles shows: the server's roles in full and how many members
    /// hold each.
    pub async fn roles_page(&self, guild: &str) -> Result<server_settings::Roles> {
        let (settings, counts) =
            tokio::join!(self.api.guild(guild), self.api.role_member_counts(guild));
        Ok(server_settings::parse_roles_page(&settings?, &counts?))
    }

    /// How many members hold each role, by role id.
    pub async fn role_member_counts(&self, guild: &str) -> Result<HashMap<String, u64>> {
        let counts = self.api.role_member_counts(guild).await?;
        Ok(server_settings::parse_roles_page(&Value::Null, &counts).counts)
    }

    /// Moves roles to new positions, as `(id, position)`, then changes one
    /// role with `body`'s fields, as Roles' Save Changes does, in that
    /// order. GUILD_ROLE_UPDATE follows each.
    pub async fn save_roles(
        &self,
        guild: &str,
        positions: &[(String, i64)],
        role: Option<(&str, &Value)>,
    ) -> Result<()> {
        if !positions.is_empty() {
            let positions: Vec<Value> = positions
                .iter()
                .map(|(id, position)| serde_json::json!({"id": id, "position": position}))
                .collect();
            self.api.move_roles(guild, &Value::Array(positions)).await?;
        }
        if let Some((role, body)) = role {
            self.api.modify_role(guild, role, body).await?;
        }
        Ok(())
    }

    /// Makes a role with `body`'s fields, giving its id; GUILD_ROLE_CREATE
    /// follows.
    pub async fn create_role(&self, guild: &str, body: &Value) -> Result<String> {
        let role = self.api.create_role(guild, body).await?;
        role["id"]
            .as_str()
            .map(str::to_owned)
            .context("Discord gave no id for the role")
    }

    /// Deletes a role; GUILD_ROLE_DELETE follows.
    pub async fn delete_role(&self, guild: &str, role: &str) -> Result<()> {
        self.api.delete_role(guild, role).await
    }

    /// Up to 100 members holding a role, by id, whose names, avatars and
    /// roles are known once this answers.
    pub async fn role_members(&self, guild: &str, role: &str) -> Result<Vec<String>> {
        let ids = self.api.role_member_ids(guild, role).await?;
        let ids: Vec<String> = ids
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|id| Some(id.as_str()?.to_owned()))
            .collect();
        self.ask_members(guild, ids.iter().map(String::as_str))
            .await;
        Ok(ids)
    }

    /// Gives a role to these members at once.
    pub async fn add_role_members(
        &self,
        guild: &str,
        role: &str,
        members: &[String],
    ) -> Result<()> {
        self.api
            .add_role_members(guild, role, members)
            .await
            .map(drop)
    }

    /// Takes a role from a member, as Manage Members does: by setting the
    /// member's other roles.
    /// Their roles must be known, or the rest would go too.
    pub async fn remove_role_member(&self, guild: &str, role: &str, user: &str) -> Result<()> {
        self.ask_members(guild, std::iter::once(user)).await;
        let held = self.roles(guild, user);
        if !held.iter().any(|known| known == role) {
            bail!("Yapfast doesn't know that member's roles yet. Try again.");
        }
        let kept: Vec<String> = held.iter().filter(|held| *held != role).cloned().collect();
        self.api.set_member_roles(guild, user, &kept).await
    }

    /// Changes this account's main profile as Discord's client does:
    /// `user`'s fields, its display name and avatar, go to the user, then
    /// `profile`'s, its bio, pronouns, banner and banner colour, to its
    /// profile, each only when it has any. USER_UPDATE follows both.
    pub async fn save_main_profile(&self, user: &Value, profile: &Value) -> Result<()> {
        if some(user) {
            self.api.modify_me(user).await?;
        }
        if some(profile) {
            self.api.modify_profile_me(profile).await?;
        }
        self.forget_own_profile(None);
        Ok(())
    }

    /// Drops this account's kept profiles, so they're fetched afresh: in
    /// `guild`, or everywhere once the main profile changed.
    fn forget_own_profile(&self, guild: Option<&str>) {
        let me = self.snapshots.borrow().user_id.clone();
        self.lock_profiles().retain(|kept| {
            Some(&kept.user) != me.as_ref() || guild.is_some_and(|guild| kept.guild != guild)
        });
    }

    fn lock_profiles(&self) -> std::sync::MutexGuard<'_, Vec<Kept>> {
        self.profiles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Accepts a message request, or ignores it. The DM list changes when
    /// Discord confirms it.
    pub async fn answer_request(&self, channel: &str, accept: bool) -> Result<()> {
        if accept {
            self.api.accept_request(channel).await
        } else {
            self.api.ignore_request(channel).await
        }
    }

    /// Removes someone's reaction, as a moderator; `emoji` is in the API's
    /// form.
    pub async fn remove_reaction(
        &self,
        channel: &str,
        id: &str,
        emoji: &str,
        user: &str,
    ) -> Result<()> {
        self.api.remove_reaction(channel, id, emoji, user).await
    }

    /// Adds or removes this account's reaction; `emoji` is in the API's form.
    pub async fn react(&self, channel: &str, id: &str, emoji: &str, add: bool) -> Result<()> {
        self.api.react(channel, id, emoji, add).await
    }

    /// Shows others that this account is typing in `channel`, for about ten seconds.
    pub async fn typing(&self, channel: &str) -> Result<()> {
        self.api.typing(channel).await
    }

    /// Rings the others in a DM or group DM call this account started, as
    /// Discord's client does: in a DM, a friend at once, and anyone else only
    /// if Discord says they may be rung. Returns whether it rang.
    pub async fn ring(&self, channel: &str) -> Result<bool> {
        let stranger = {
            let snapshot = self.snapshots.borrow();
            snapshot
                .conversations
                .iter()
                .find(|conversation| conversation.id == channel)
                .and_then(|conversation| conversation.person.clone())
                .is_some_and(|person| !snapshot.friends.contains(&person))
        };
        if stranger && !self.api.ringable(channel).await? {
            return Ok(false);
        }
        self.api.ring(channel).await?;
        Ok(true)
    }

    /// Ends this login's session on Discord; the token stops working.
    pub async fn logout(&self) -> Result<()> {
        self.api.logout().await
    }

    /// Sends a friend request to whoever has this username.
    pub async fn add_friend_by_username(&self, username: &str) -> Result<()> {
        self.api.add_friend_by_username(username).await
    }

    /// Sets or clears the custom status, everywhere this account is signed
    /// in, and shows it from this session.
    pub async fn set_custom_status(&self, custom: Option<CustomStatus>) -> Result<()> {
        self.api.set_custom_status(custom.as_ref()).await?;
        let _ = self.requests.send(Request::CustomStatus(custom));
        Ok(())
    }

    /// Sets the status, `online`, `idle`, `dnd` or `invisible`, everywhere
    /// this account is signed in, and shows it from this session.
    pub async fn set_status(&self, status: &str) -> Result<()> {
        self.api.set_status(status).await?;
        let _ = self.requests.send(Request::Status(status.to_owned()));
        Ok(())
    }

    /// Shares the game this account plays, everywhere it's signed in, or
    /// stops sharing it. Stopping ends game detection and Rich Presence.
    pub async fn set_show_game(&self, shown: bool) -> Result<()> {
        self.api.set_show_game(shown).await?;
        let _ = self.requests.send(Request::ShowGame(shown));
        Ok(())
    }

    /// Tells the session nobody has used Yapfast since `since`, in Unix
    /// milliseconds, or `None` once someone does. As in the web client, the
    /// session then goes AFK after the account's AFK timeout, so Discord
    /// sends notifications to the phone, and shows idle after 10 minutes
    /// if `show_idle`. Only this session's presence changes, not the status
    /// setting.
    pub fn set_inactive(&self, since: Option<u64>, show_idle: bool) {
        let _ = self.requests.send(Request::Inactive(since, show_idle));
    }

    /// The GIF picker's home: trending terms with a GIF each.
    pub async fn gifs_trending(&self) -> Result<crate::gifs::Trending> {
        Ok(crate::gifs::Trending::parse(
            &self.api.gifs_trending().await?,
        ))
    }

    /// The GIFs trending now.
    pub async fn gifs_trending_gifs(&self) -> Result<Vec<crate::gifs::Gif>> {
        Ok(crate::gifs::parse_list(
            &self.api.gifs_trending_gifs().await?,
        ))
    }

    /// GIFs for `query`.
    pub async fn gifs_search(&self, query: &str) -> Result<Vec<crate::gifs::Gif>> {
        Ok(crate::gifs::parse_list(&self.api.gifs_search(query).await?))
    }

    /// Up to `limit` terms to try instead of `query`.
    pub async fn gifs_suggest(&self, query: &str, limit: u32) -> Result<Vec<String>> {
        let terms = self.api.gifs_suggest(query, limit).await?;
        Ok(terms
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|term| term.as_str().map(str::to_owned))
            .collect())
    }

    /// Tells the provider a search for `query` led to the GIF `id`.
    pub async fn gif_select(&self, id: &str, query: &str) -> Result<()> {
        self.api.gif_select(id, query).await
    }

    /// The account's favourite GIFs as Yapfast last saw them, to show until
    /// [`Self::favorite_gifs`] answers.
    pub async fn kept_favorite_gifs(&self) -> Option<Vec<crate::gifs::Favorite>> {
        tokio::task::spawn_blocking(cache::favorite_gifs)
            .await
            .ok()
            .flatten()
    }

    /// The account's favourite GIFs, kept on disk too.
    pub async fn favorite_gifs(&self) -> Result<Vec<crate::gifs::Favorite>> {
        let settings = self.api.frecency_settings().await?;
        let favorites = crate::gifs::read_favorites(&settings).unwrap_or_default();
        keep_favorite_gifs(favorites.clone()).await;
        Ok(favorites)
    }

    /// The servers and channels this account used, by id, as Discord's
    /// clients counted them; see [`crate::frecency`].
    pub async fn places_used(&self) -> Result<Vec<(String, crate::frecency::Uses)>> {
        let settings = self.api.frecency_settings().await?;
        Ok(crate::frecency::read_places(&settings).unwrap_or_default())
    }

    /// Keeps `favorites` as the account's favourite GIFs, in place of the old.
    pub async fn set_favorite_gifs(&self, favorites: &[crate::gifs::Favorite]) -> Result<()> {
        self.api
            .set_frecency_settings(&crate::gifs::write_favorites(favorites))
            .await?;
        keep_favorite_gifs(favorites.to_vec()).await;
        Ok(())
    }

    /// Sends someone a friend request from their profile, or accepts theirs.
    /// The lists change when Discord confirms it.
    pub async fn befriend(&self, user: &str) -> Result<()> {
        let incoming = self
            .snapshots
            .borrow()
            .friend_requests
            .iter()
            .any(|request| request.user_id == user && request.incoming);
        // Discord's client accepts from its friends list and sends from a profile.
        let place = if incoming { "Friends" } else { "User Profile" };
        self.api.add_friend(user, place).await
    }

    /// Ignores a friend request to this account, or cancels one it sent.
    pub async fn remove_friend_request(&self, user: &str) -> Result<()> {
        self.api.remove_relationship(user, "Friends").await
    }

    /// Ends a friendship. The lists change when Discord confirms it.
    pub async fn unfriend(&self, user: &str) -> Result<()> {
        self.api.remove_relationship(user, "ContextMenu").await
    }

    /// Blocks someone, or unblocks them.
    pub async fn block(&self, user: &str, block: bool) -> Result<()> {
        if block {
            self.api.block(user, "ContextMenu").await
        } else {
            self.api.remove_relationship(user, "ContextMenu").await
        }
    }

    /// Ignores someone, or stops ignoring them.
    pub async fn ignore(&self, user: &str, ignore: bool) -> Result<()> {
        self.api.ignore(user, ignore).await
    }

    /// Sets the nickname this account gives a friend; empty clears it.
    pub async fn set_friend_nickname(&self, user: &str, nickname: &str) -> Result<()> {
        self.api.set_friend_nickname(user, nickname).await
    }

    /// This account's note on someone; empty when it has none.
    pub async fn note(&self, user: &str) -> Result<String> {
        self.api.note(user).await
    }

    /// Sets this account's note on someone; empty removes it.
    pub async fn set_note(&self, user: &str, note: &str) -> Result<()> {
        self.api.set_note(user, note).await
    }

    /// Sets the nickname of `user` in a server, this account's own included;
    /// empty resets it. Names change when Discord confirms it.
    pub async fn set_member_nick(&self, guild: &str, user: &str, nick: &str) -> Result<()> {
        let me = self.snapshots.borrow().user_id.as_deref() == Some(user);
        self.api
            .set_member_nick(guild, (!me).then_some(user), nick)
            .await
    }

    /// Gives a member a role, or takes it away.
    pub async fn member_role(&self, guild: &str, user: &str, role: &str, add: bool) -> Result<()> {
        self.api.member_role(guild, user, role, add).await
    }

    /// Times a member out until `until_ms`, in Unix milliseconds, or with
    /// none ends their timeout; `reason` goes to the audit log.
    pub async fn time_out(
        &self,
        guild: &str,
        user: &str,
        until_ms: Option<u64>,
        reason: &str,
    ) -> Result<()> {
        let until = until_ms
            .and_then(|ms| chrono::DateTime::from_timestamp_millis(i64::try_from(ms).ok()?))
            .map(|until| until.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        self.api
            .time_out(guild, user, until.as_deref(), reason)
            .await
    }

    /// Mutes a member in the server's voice channels, or unmutes them.
    pub async fn server_mute(&self, guild: &str, user: &str, mute: bool) -> Result<()> {
        let body = serde_json::json!({ "mute": mute });
        self.api.modify_member_voice(guild, user, &body).await
    }

    /// Deafens a member in the server's voice channels, or undeafens them.
    pub async fn server_deafen(&self, guild: &str, user: &str, deaf: bool) -> Result<()> {
        let body = serde_json::json!({ "deaf": deaf });
        self.api.modify_member_voice(guild, user, &body).await
    }

    /// Moves a member to another voice channel of the server, or with none
    /// disconnects them from voice.
    pub async fn move_member(&self, guild: &str, user: &str, channel: Option<&str>) -> Result<()> {
        let body = serde_json::json!({ "channel_id": channel });
        self.api.modify_member_voice(guild, user, &body).await
    }

    /// Removes a member from a server.
    pub async fn kick(&self, guild: &str, user: &str, reason: &str) -> Result<()> {
        self.api.kick(guild, user, reason).await
    }

    /// Bans a member, deleting what they wrote in the last `delete_seconds`.
    pub async fn ban(
        &self,
        guild: &str,
        user: &str,
        delete_seconds: u32,
        reason: &str,
    ) -> Result<()> {
        self.api.ban(guild, user, delete_seconds, reason).await
    }

    /// Closes a DM; the DM list changes when Discord confirms it.
    pub async fn close_dm(&self, channel: &str) -> Result<()> {
        self.api.close_dm(channel).await
    }

    /// Mutes a DM or group DM for `seconds`, or for good with none, or
    /// unmutes it; the DM list changes when Discord confirms it.
    pub async fn mute_private(
        &self,
        channel: &str,
        muted: bool,
        seconds: Option<u64>,
    ) -> Result<()> {
        self.change_notifications(PRIVATE, Some(channel), Change::Mute { muted, seconds })
            .await
    }

    /// Changes this account's notification settings for a server, or with
    /// `channel` for a channel or category in it, or with [`PRIVATE`] for a
    /// DM. Snapshots show it when Discord confirms it.
    pub async fn change_notifications(
        &self,
        guild: &str,
        channel: Option<&str>,
        change: Change,
    ) -> Result<()> {
        let body = change
            .body(channel)
            .ok_or_else(|| anyhow!("Only a server has that setting."))?;
        self.api.guild_settings(guild, &body).await
    }

    /// Sets the `flags` of a server's channel's override, which pin it to
    /// the top of the channel list or unpin it, see
    /// [`crate::notifications::Override::pin_flags`], and add it to the
    /// list or remove it, see [`crate::notifications::Override::follow_flags`].
    /// Snapshots show it when Discord confirms it.
    pub async fn channel_flags(&self, guild: &str, channel: &str, flags: u64) -> Result<()> {
        let body = crate::notifications::flags_body(guild, channel, flags);
        self.api.guilds_settings(&body).await
    }

    /// Turns a server's Show All Channels on or off by setting its own
    /// `flags`, following the channels `follow` names with theirs; see
    /// [`crate::notifications::show_all_body`]. Snapshots show it when
    /// Discord confirms it.
    pub async fn show_all_channels(
        &self,
        guild: &str,
        flags: u64,
        follow: &[(String, u64)],
    ) -> Result<()> {
        let body = crate::notifications::show_all_body(flags, follow);
        self.api.guild_settings(guild, &body).await
    }

    /// Makes an invite to a server's channel; its code. See
    /// [`Api::create_invite`].
    pub async fn create_invite(
        &self,
        channel: &str,
        max_age: u32,
        max_uses: u32,
    ) -> Result<String> {
        self.api.create_invite(channel, max_age, max_uses).await
    }

    /// Invites `user` to a server as Discord's Invite to Server does: makes
    /// an invite to its `channel` that works once and sends its link in a
    /// message to their DM, which opening finds or starts. The DM's and the
    /// message's gateway events show both.
    pub async fn send_invite(&self, user: &str, channel: &str) -> Result<()> {
        let code = self.api.create_invite_once(channel).await?;
        let dm = self.dm_with(user).await?;
        let link = format!("https://discord.gg/{code}");
        let nonce = crate::message::nonce();
        let sent = self
            .api
            .send_from("context_menu", &dm, &link, &nonce, false)
            .await?;
        self.write_names().learn_message(None, &sent);
        Ok(())
    }

    /// What an invite shows before joining by it, by its code; see
    /// [`Api::resolve_invite`]. An invite that isn't to a server, which
    /// Yapfast doesn't join, is refused.
    pub async fn invite(&self, code: &str, typed: Option<&str>) -> Result<invite::Preview> {
        let invite = self.api.resolve_invite(code, typed).await?;
        invite::Preview::of(&invite).context("This invite isn't to a server.")
    }

    /// Joins a server by the invite `preview` shows, as Discord's client
    /// does from `from`, showing each captcha Discord asks for first:
    /// `captcha` hears its page's address. The server's GUILD_CREATE
    /// follows, and a snapshot lists it.
    pub async fn join_invite(
        &self,
        preview: &invite::Preview,
        from: invite::JoinFrom,
        captcha: impl FnMut(&str),
    ) -> Result<invite::Joined> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::Session(reply))
            .map_err(|_| anyhow!("Discord isn't connected. Restart Yapfast and try again."))?;
        let session = answer
            .await
            .ok()
            .flatten()
            .context("Discord isn't connected yet. Try again in a moment.")?;
        let (api, session, context) = (&self.api, &session, &preview.context(from));
        let answer = crate::captcha::solving(
            |solved| async move {
                let solved = solved
                    .as_ref()
                    .map(|(captcha, solution)| (captcha, solution.as_str()));
                api.accept_invite(&preview.code, session, context, solved)
                    .await
            },
            captcha,
        )
        .await?;
        Ok(invite::Joined::of(&answer, preview))
    }

    /// What a server's scheduled event shows from its link, and
    /// whether this account is interested in it; see
    /// [`Api::scheduled_event`].
    pub async fn scheduled_event(
        &self,
        guild: &str,
        event: &str,
    ) -> Result<scheduled_event::Preview> {
        let found = self.api.scheduled_event(guild, event).await?;
        let own = self.api.own_scheduled_events(guild).await?;
        let interested = scheduled_event::interested_in(&own, event);
        scheduled_event::Preview::of(&found, interested)
            .context("This event is not in that server.")
    }

    /// Marks this account interested in a server's scheduled event, or
    /// no longer interested in it.
    pub async fn set_event_interest(
        &self,
        guild: &str,
        event: &str,
        interested: bool,
    ) -> Result<()> {
        self.api
            .set_scheduled_event_interest(guild, event, interested)
            .await
    }

    /// What a gift shows before it is redeemed, by its code; see
    /// [`Api::gift_code`]. An answer without a gift code is refused.
    pub async fn gift(&self, code: &str) -> Result<gift::Preview> {
        let gift = self.api.gift_code(code).await?;
        gift::Preview::of(&gift).context("Discord didn't answer with a gift.")
    }

    /// Redeems the gift with this code for this account; `channel` is
    /// where its link was opened, when it was a channel. Discord sends
    /// the entitlement it grants over the gateway.
    pub async fn redeem_gift(&self, code: &str, channel: Option<&str>) -> Result<()> {
        self.api.redeem_gift_code(code, channel).await.map(drop)
    }

    /// A channel's name, topic, slowmode in seconds and whether it's
    /// age-restricted, as its dialog edits them.
    pub async fn channel_settings(&self, channel: &str) -> Result<ChannelSettings> {
        let channel = self.api.channel(channel).await?;
        Ok(ChannelSettings::of(&channel))
    }

    /// Makes a channel of `kind`, `0` for text, `2` for voice or `4` for a
    /// category, in a server, in the category `parent`, or copying the
    /// permissions, topic, slowmode, age restriction and voice settings of
    /// `copy_of`, as Discord's Duplicate Channel does; its id. Snapshots
    /// show it once its CHANNEL_CREATE comes.
    pub async fn create_channel(
        &self,
        guild: &str,
        kind: u8,
        parent: Option<&str>,
        copy_of: Option<&str>,
        settings: &ChannelSettings,
    ) -> Result<String> {
        let mut body = json!({"name": settings.name, "type": kind, "permission_overwrites": []});
        if let Some(source) = copy_of {
            let source = self.api.channel(source).await?;
            for key in [
                "type",
                "permission_overwrites",
                "parent_id",
                "topic",
                "nsfw",
                "rate_limit_per_user",
                "bitrate",
                "user_limit",
                "rtc_region",
            ] {
                if !source[key].is_null() {
                    body[key] = source[key].clone();
                }
            }
        } else if kind == 0 {
            body["topic"] = json!(Some(settings.topic.as_str()).filter(|topic| !topic.is_empty()));
            body["rate_limit_per_user"] = json!(settings.slowmode);
            body["nsfw"] = json!(settings.nsfw);
        }
        if let Some(parent) = parent.filter(|_| copy_of.is_none()) {
            body["parent_id"] = json!(parent);
        }
        let made = self.api.create_channel(guild, &body).await?;
        made["id"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("Discord made no channel."))
    }

    /// Renames a channel or category, and for a text channel sets its
    /// topic, slowmode and age restriction too.
    pub async fn edit_channel(
        &self,
        channel: &str,
        text: bool,
        settings: &ChannelSettings,
    ) -> Result<()> {
        let mut body = json!({"name": settings.name});
        if text {
            body["topic"] = json!(settings.topic);
            body["rate_limit_per_user"] = json!(settings.slowmode);
            body["nsfw"] = json!(settings.nsfw);
        }
        self.api.edit_channel(channel, &body).await
    }

    /// Moves a server's channels and categories, as dragging them in the
    /// channel list does, locking the permissions of the channel `lock`
    /// names to its new category's. The list shows them moved at once,
    /// and as they were again if Discord refuses.
    pub async fn move_channels(
        &self,
        guild: &str,
        placements: Vec<ready::Placement>,
        lock: Option<&str>,
    ) -> Result<()> {
        let body: Vec<Value> = placements
            .iter()
            .map(|placement| {
                let mut entry = json!({"id": placement.id, "position": placement.position});
                if let Some(parent) = &placement.parent {
                    entry["parent_id"] = json!(parent);
                }
                if lock == Some(placement.id.as_str()) {
                    entry["lock_permissions"] = json!(true);
                }
                entry
            })
            .collect();
        let (reply, was) = oneshot::channel();
        let _ = self.requests.send(Request::Place {
            guild: guild.into(),
            placements,
            reply: Some(reply),
        });
        let result = self.api.move_channels(guild, &Value::Array(body)).await;
        if result.is_err()
            && let Ok(placements) = was.await
        {
            let _ = self.requests.send(Request::Place {
                guild: guild.into(),
                placements,
                reply: None,
            });
        }
        result
    }

    /// Deletes a server's channel or category for good.
    pub async fn delete_channel(&self, channel: &str) -> Result<()> {
        self.api.delete_channel(channel).await
    }

    /// Sets these overwrites on a channel or category, one request each,
    /// one after another, as Channel Permissions saves them; the first
    /// Discord refuses stops the rest.
    pub async fn put_overwrites(
        &self,
        channel: &str,
        overwrites: &[ready::Overwrite],
    ) -> Result<()> {
        for overwrite in overwrites {
            self.api
                .put_overwrite(channel, &overwrite.id, &overwrite_body(overwrite))
                .await?;
        }
        Ok(())
    }

    /// Takes a role's or member's overwrite off a channel or category.
    pub async fn delete_overwrite(&self, channel: &str, id: &str) -> Result<()> {
        self.api.delete_overwrite(channel, id).await
    }

    /// Gives a channel exactly these overwrites, its category's, as Sync
    /// Now does.
    pub async fn sync_overwrites(
        &self,
        channel: &str,
        overwrites: &[ready::Overwrite],
    ) -> Result<()> {
        let overwrites: Vec<Value> = overwrites.iter().map(overwrite_body).collect();
        self.api
            .edit_channel(channel, &json!({"permission_overwrites": overwrites}))
            .await
    }

    /// Learns of a thread of a server, by id, to open in place of the
    /// channel on screen, or `beside` it: snapshots list it while it's on
    /// screen, as Discord tells a user account only of the threads it
    /// joined. One the server's snapshot or an open list of threads has
    /// opens from that; Yapfast asks Discord for the rest.
    /// `listed` is what [`Self::listed_thread`] gave for it.
    pub async fn open_thread(
        &self,
        guild: &str,
        thread: &str,
        listed: Option<Value>,
        beside: bool,
    ) -> Result<()> {
        let known = self
            .snapshots
            .borrow()
            .servers
            .iter()
            .find(|server| server.id == guild)
            .is_some_and(|server| server.threads.iter().any(|known| known.id == thread));
        let value = match (known, listed) {
            (true, _) => None,
            (false, Some(value)) => Some(value),
            (false, None) => Some(self.api.channel(thread).await?),
        };
        if let Some(value) = &value
            && (!ready::is_thread(value) || value["guild_id"] != guild)
        {
            bail!("That isn't a thread of this server.");
        }
        let _ = self.requests.send(Request::OpenThread {
            guild: guild.into(),
            id: thread.into(),
            thread: value,
            beside,
        });
        Ok(())
    }

    /// A page of a server channel's threads, as its list of threads shows
    /// them: the archived ones, [`api::THREAD_PAGE`] from the `offset`th,
    /// those with the newest messages first; or with a `name`, the active
    /// and archived ones Discord finds by it. Their owners' names are
    /// learnt, waiting a while for them. Discord indexes a channel the
    /// first time it's searched, and asks to be asked again until it has.
    pub async fn threads(
        &self,
        guild: &str,
        channel: &str,
        name: Option<&str>,
        offset: u32,
    ) -> Result<ThreadPage> {
        let mut waited = Duration::ZERO;
        loop {
            let answer = self.api.search_threads(channel, name, offset).await?;
            if let Some(found) = answer["threads"].as_array() {
                return Ok(self.thread_page(guild, &answer, found).await);
            }
            if answer["code"].as_u64() != Some(INDEX_NOT_READY) {
                bail!("could not understand Discord's answer");
            }
            let Some(wait) = index_wait(&answer, &mut waited) else {
                break;
            };
            tokio::time::sleep(wait).await;
        }
        Err(anyhow!(
            "Discord is still getting these threads ready to search. Try again in a moment."
        ))
    }

    async fn thread_page(&self, guild: &str, answer: &Value, found: &[Value]) -> ThreadPage {
        let threads = self.listed_page(guild, answer, found);
        // A forum's posts come with their first messages.
        let forum = {
            let snapshot = self.snapshots.borrow();
            let parent = threads.first().and_then(|thread| thread.parent.as_deref());
            snapshot
                .servers
                .iter()
                .find(|server| server.id == guild)
                .and_then(|server| server.channel(parent?))
                .is_some_and(|parent| parent.forum.is_some())
        };
        let starters = match forum {
            true => {
                let first = answer["first_messages"].as_array();
                let posts = threads.iter().map(|thread| {
                    let message = first
                        .into_iter()
                        .flatten()
                        .find(|message| message["channel_id"].as_str() == Some(thread.id.as_str()));
                    (thread.id.as_str(), message)
                });
                self.starters(guild, posts)
            }
            false => vec![],
        };
        let people = threads
            .iter()
            .filter_map(|thread| thread.thread.as_ref()?.owner.as_deref())
            .chain(
                starters
                    .iter()
                    .filter_map(|(_, starter)| Some(starter.as_ref()?.author.as_str())),
            );
        self.ask_members(guild, people).await;
        ThreadPage {
            threads,
            more: answer["has_more"].as_bool().unwrap_or(false),
            starters,
        }
    }

    /// The first messages of a forum's posts, by id, at most
    /// [`api::POSTS_PER_ASK`] of them, as their cards show them; the names
    /// of those who wrote them come first, if they come soon.
    pub async fn posts(&self, guild: &str, forum: &str, posts: &[&str]) -> Result<Starters> {
        let posts = &posts[..posts.len().min(api::POSTS_PER_ASK)];
        let answer = self.api.post_data(forum, posts).await?;
        let Some(found) = answer["threads"].as_object() else {
            bail!("could not understand Discord's answer");
        };
        {
            let mut names = self.write_names();
            for data in found.values() {
                if data["owner"].is_object() {
                    names.learn_member(guild, &data["owner"]);
                }
            }
        }
        let starters = self.starters(
            guild,
            posts.iter().map(|post| {
                let message =
                    Some(&found[*post]["first_message"]).filter(|first| first.is_object());
                (*post, message)
            }),
        );
        let authors = starters
            .iter()
            .filter_map(|(_, starter)| Some(starter.as_ref()?.author.as_str()));
        self.ask_members(guild, authors).await;
        Ok(starters)
    }

    /// Posts' first messages as their cards show them, learning their
    /// authors' names.
    fn starters<'a>(
        &self,
        guild: &str,
        posts: impl Iterator<Item = (&'a str, Option<&'a Value>)>,
    ) -> Starters {
        let mut names = self.write_names();
        posts
            .map(|(post, message)| {
                let starter = message.and_then(|message| {
                    names.learn_message(Some(guild), message);
                    forum::Starter::of(message)
                });
                (post.to_owned(), starter)
            })
            .collect()
    }

    /// Learns the names of these members of a server, asking the gateway
    /// for those Yapfast doesn't know yet.
    pub async fn learn_members(&self, guild: &str, users: &[String]) {
        self.ask_members(guild, users.iter().map(String::as_str))
            .await;
    }

    /// Asks Discord for the server profiles of those of `people` Yapfast
    /// doesn't know there, waiting a while for them.
    async fn ask_members<'a>(&self, guild: &str, people: impl Iterator<Item = &'a str>) {
        let unknown: HashSet<String> = {
            let names = self.read_names();
            people
                .filter(|person| !names.knows_member(guild, person))
                .map(str::to_owned)
                .collect()
        };
        if unknown.is_empty() {
            return;
        }
        let (reply, arrived) = oneshot::channel();
        let _ = self.requests.send(Request::Members {
            guild: guild.into(),
            users: unknown.into_iter().collect(),
            reply: Some(reply),
        });
        let _ = tokio::time::timeout(SEARCH_TIMEOUT, arrived).await;
    }

    /// The threads of a page Discord gave, this account's thread members
    /// in them and its access set, without those it may not see; kept to
    /// open from while the list is open.
    fn listed_page(&self, guild: &str, answer: &Value, found: &[Value]) -> Vec<ready::Channel> {
        let snapshot = self.snapshots.borrow().clone();
        let server = snapshot.servers.iter().find(|server| server.id == guild);
        let me = snapshot.user_id.as_deref().unwrap_or_default();
        let members = answer["members"].as_array();
        let mut threads = Vec::with_capacity(found.len());
        let mut listed = self.lock_listed();
        for value in found {
            let Some(mut thread) = ready::parse_thread(value) else {
                continue;
            };
            if let Some(member) = members
                .into_iter()
                .flatten()
                .find(|member| member["id"].as_str() == Some(thread.id.as_str()))
                && let Some(details) = thread.thread.as_deref_mut()
            {
                details.join(member);
            }
            if let Some(server) = server
                && server.access(&thread, me, &server.marks.roles) == ready::Access::Hidden
            {
                continue;
            }
            listed.insert(thread.id.clone(), value.clone());
            threads.push(thread);
        }
        threads
    }

    /// Starts a thread in a server's text or announcement channel, from
    /// one of its messages or from none, as Discord's client does: a
    /// private one, an announcement thread in an announcement channel, else
    /// a public one, archiving after the channel's default quiet, or three
    /// days. `location` is where in the client it started, such as
    /// "Message". Returns the thread's id; it lists once Discord's
    /// THREAD_CREATE and THREAD_MEMBERS_UPDATE come.
    pub async fn start_thread(
        &self,
        guild: &str,
        channel: &str,
        message: Option<&str>,
        name: &str,
        private: bool,
        location: &str,
    ) -> Result<String> {
        let (announcement, archive_after) = {
            let snapshot = self.snapshots.borrow();
            let parent = snapshot
                .servers
                .iter()
                .find(|server| server.id == guild)
                .and_then(|server| server.channel(channel))
                .ok_or_else(|| anyhow!("Yapfast doesn't know that channel."))?;
            (parent.announcement, parent.archive_after)
        };
        let kind = match (private, announcement) {
            (true, false) => ready::PRIVATE_THREAD,
            (true, true) => bail!("An announcement channel has no private threads."),
            (false, true) => ready::ANNOUNCEMENT_THREAD,
            (false, false) => ready::PUBLIC_THREAD,
        };
        let body = json!({
            "name": name,
            "type": kind,
            "auto_archive_duration": match archive_after {
                0 => DEFAULT_ARCHIVE,
                minutes => minutes,
            },
            "location": location,
        });
        let thread = self.api.create_thread(channel, message, &body).await?;
        thread["id"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("could not understand Discord's answer"))
    }

    /// A thread an open list of threads found, by id, as Discord gave it,
    /// to open with [`Self::open_thread`]; ask before the list closes.
    pub fn listed_thread(&self, thread: &str) -> Option<Value> {
        self.lock_listed().get(thread).cloned()
    }

    /// Forgets the threads a list of threads found, as it closes.
    pub fn forget_listed_threads(&self) {
        let mut listed = self.lock_listed();
        listed.clear();
        listed.shrink_to_fit();
    }

    fn lock_listed(&self) -> std::sync::MutexGuard<'_, HashMap<String, Value>> {
        self.listed_threads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Shows a thread of a server beside the channel on screen, or with
    /// none, stops; snapshots list it meanwhile.
    pub fn beside(&self, guild: &str, thread: Option<&str>) {
        let _ = self.requests.send(Request::Beside {
            guild: guild.into(),
            thread: thread.map(str::to_owned),
        });
    }

    /// Joins a thread, or leaves it with `join` false; `location` is the
    /// menu it was chosen from, as Discord's client names it, such as
    /// "Context Menu". Snapshots show it once Discord's
    /// THREAD_MEMBERS_UPDATE comes; one left stays open, as in Discord's
    /// client.
    pub async fn join_thread(&self, thread: &str, join: bool, location: &str) -> Result<()> {
        self.api.thread_member(thread, join, location).await
    }

    /// Leaves a server this account doesn't own. Snapshots drop it once its
    /// GUILD_DELETE comes.
    pub async fn leave_server(&self, guild: &str) -> Result<()> {
        self.api.leave_guild(guild).await
    }

    /// Leaves a group DM, `silent`ly or not; see [`Api::leave_group`].
    /// Snapshots drop it once its CHANNEL_DELETE comes.
    pub async fn leave_group(&self, channel: &str, silent: bool) -> Result<()> {
        self.api.leave_group(channel, silent).await
    }

    /// Declines a call ringing this account; the call goes on without it.
    pub async fn decline(&self, channel: &str) -> Result<()> {
        let me = self.snapshots.borrow().user_id.clone().unwrap_or_default();
        self.api.stop_ringing(channel, &me).await
    }

    /// Uploads the preview Discord shows of this account's live stream: a
    /// JPEG as a data URI.
    pub async fn stream_preview(&self, stream_key: &str, thumbnail: &str) -> Result<()> {
        self.api.stream_preview(stream_key, thumbnail).await
    }

    /// Downloads a file from Discord's CDN, or a picture kept on disk.
    pub async fn download(&self, url: &str, limit: usize) -> Result<Vec<u8>> {
        self.fetch(url, limit, true).await
    }

    /// Downloads a file as [`Self::download`] does, without waiting behind
    /// the pictures downloading: one someone opened, saved or copied, or
    /// one whose caller keeps turns of its own, as avatars and icons do.
    pub async fn download_now(&self, url: &str, limit: usize) -> Result<Vec<u8>> {
        self.fetch(url, limit, false).await
    }

    /// The picture at `url` as it was kept on disk, if it was and it's
    /// within `limit`: what [`Self::download`] finds without downloading.
    pub async fn kept_picture(&self, url: &str, limit: usize) -> Option<Vec<u8>> {
        cache::picture(url).await.filter(|kept| kept.len() <= limit)
    }

    async fn fetch(&self, url: &str, limit: usize, queued: bool) -> Result<Vec<u8>> {
        // An activity names a game's icon by the game until Discord says
        // which picture it is.
        let icon;
        let url = match activity::icon_application(url) {
            Some(application) => {
                icon = self
                    .application_icon(application)
                    .await?
                    .context("the application has no icon")?;
                icon.as_str()
            }
            None => url,
        };
        if let Some(kept) = cache::picture(url).await.filter(|kept| kept.len() <= limit) {
            return Ok(kept);
        }
        let data = if queued {
            self.api.download(url, limit).await?
        } else {
            self.api.download_now(url, limit).await?
        };
        cache::keep_picture(url, data)
            .await
            .context("the picture was lost while it was kept")
    }

    /// The address of an application's icon, asked of Discord once.
    async fn application_icon(&self, application: &str) -> Result<Option<String>> {
        let lock = || {
            self.application_icons
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        };
        if let Some(known) = lock().get(application) {
            return Ok(known.clone());
        }
        let found = self.api.public_applications(&[application]).await?;
        let icon = found
            .as_array()
            .into_iter()
            .flatten()
            .find(|found| found["id"] == application)
            .and_then(activity::icon_url);
        lock().insert(application.to_owned(), icon.clone());
        Ok(icon)
    }

    /// Downloads a file from Discord's CDN into `file` as it arrives,
    /// returning its size; a video plays while it does.
    pub async fn download_into(
        &self,
        url: &str,
        file: &mut std::fs::File,
        limit: u64,
    ) -> Result<u64> {
        self.api.download_into(url, file, limit).await
    }

    /// Changes whenever a name, avatar or role that messages show changes.
    pub fn names_version(&self) -> u64 {
        self.read_names().version()
    }

    /// A user's name in a server: their nickname, display name or username.
    pub fn name(&self, guild: &str, user: &str) -> Option<String> {
        self.read_names().name(guild, user).map(str::to_owned)
    }

    /// A user's username, such as `ann`.
    pub fn username(&self, user: &str) -> Option<String> {
        self.read_names().username(user).map(str::to_owned)
    }

    /// The address of a user's avatar, in a server or in DMs.
    pub fn avatar(&self, guild: Option<&str>, user: &str) -> String {
        self.read_names().avatar(guild.and_then(server), user)
    }

    /// A member's roles in a server by id, as far as they're known, shared
    /// with what learned them.
    pub fn roles(&self, guild: &str, user: &str) -> Arc<[String]> {
        self.read_names()
            .shared_roles(guild, user)
            .unwrap_or_default()
    }

    /// The ids of up to ten server members whose names start with `query`, whose
    /// names, avatars and roles are known once it answers.
    pub async fn search_members(&self, guild: &str, query: &str) -> Result<Vec<String>> {
        let (reply, found) = oneshot::channel();
        self.requests
            .send(Request::Search {
                guild: guild.into(),
                query: query.into(),
                reply,
            })
            .map_err(|_| anyhow!("Discord is offline."))?;
        tokio::time::timeout(SEARCH_TIMEOUT, found)
            .await
            .map_err(|_| anyhow!("Discord didn't answer the search."))?
            .map_err(|_| anyhow!("Discord disconnected during the search."))
    }

    /// Opens the DM with `user`, or finds the one already open; returns its channel
    /// id. The DM list has it once Discord's CHANNEL_CREATE arrives.
    pub async fn open_dm(&self, user: &str) -> Result<String> {
        self.dm_with(user).await
    }

    /// The DM with `user`: the one already listed, or one Discord opens,
    /// which may be new. Discord limits how many new DMs with people (not
    /// bots) an account starts, and flags one that starts many as a spammer.
    async fn dm_with(&self, user: &str) -> Result<String> {
        let listed = self
            .snapshots
            .borrow()
            .conversations
            .iter()
            .find(|conversation| {
                !conversation.group && conversation.person.as_deref() == Some(user)
            })
            .map(|conversation| conversation.id.clone());
        if let Some(dm) = listed {
            return Ok(dm);
        }
        let person = !self.read_names().is_bot(user);
        if person && let Some(wait) = self.lock_new_dms().full(Instant::now()) {
            bail!(
                "You started {NEW_DMS} new DMs in the last few minutes, and Discord treats more as spam. Try again in {} min.",
                wait.as_secs().div_ceil(60)
            );
        }
        let dm = self.api.open_dm(user).await?;
        if person {
            self.lock_new_dms().push(Instant::now());
        }
        Ok(dm)
    }

    fn lock_new_dms(&self) -> std::sync::MutexGuard<'_, limits::Window> {
        self.new_dms
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// People to start a DM with whose names start with `query`, friends first; with
    /// an empty query, the friends alone.
    pub fn find_people(&self, query: &str) -> Vec<Person> {
        let snapshot = self.snapshots.borrow().clone();
        let me = snapshot.user_id.clone().unwrap_or_default();
        let names = self.read_names();
        let person = |id: &String, friend: bool| Person {
            id: id.clone(),
            name: names.user_name(id).unwrap_or("Discord user").to_owned(),
            avatar: names.avatar(None, id),
            friend,
        };
        let matches = |name: &str| {
            let (name, query) = (name.to_lowercase(), query.to_lowercase());
            name.starts_with(&query) || name.split_whitespace().any(|word| word.starts_with(&query))
        };
        let mut people: Vec<Person> = snapshot
            .friends
            .iter()
            .map(|id| person(id, true))
            .filter(|person| matches(&person.name))
            .collect();
        people.sort_by_key(|person| person.name.to_lowercase());
        if !query.is_empty() {
            for id in names.find_users(query) {
                if id != me && !snapshot.friends.contains(&id) {
                    people.push(person(&id, false));
                }
            }
        }
        people
    }

    /// The RGB colour of a member's name in a server: their highest coloured
    /// role's, once their roles are known.
    pub fn color(&self, server: &Server, user: &str) -> Option<u32> {
        server.color(self.read_names().roles(&server.id, user))
    }

    /// [`Discord::render`], with the users and roles the message mentions and
    /// where.
    pub fn render_mentions(
        &self,
        server: Option<&Server>,
        content: &str,
    ) -> (String, Vec<message::Mention>) {
        self.read_names().render_mentions(server, content)
    }

    /// The members known to hold `role` in a server, by user id.
    pub fn members_with_role(&self, guild: &str, role: &str) -> Vec<String> {
        self.read_names().members_with_role(guild, role)
    }

    /// Discord Markdown for Yapfast, with mentions, custom emoji and times as text.
    pub fn render(&self, server: Option<&Server>, content: &str) -> String {
        self.read_names().render(server, content)
    }

    fn read_names(&self) -> std::sync::RwLockReadGuard<'_, Names> {
        self.names
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_names(&self) -> std::sync::RwLockWriteGuard<'_, Names> {
        self.names
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Asks Discord to join a voice channel, muted like every Yapfast call at
    /// first, and waits for the voice server it assigns.
    pub async fn join(&self, guild: &str, channel: &str) -> Result<VoiceServer> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::Join {
                guild: guild.into(),
                channel: channel.into(),
                reply,
            })
            .map_err(|_| anyhow!("Discord isn't connected. Restart Yapfast and try again."))?;
        answer
            .await
            .map_err(|_| anyhow!("Discord disconnected while joining voice."))?
    }

    pub fn leave(&self, guild: &str) {
        let _ = self.requests.send(Request::Leave {
            guild: guild.into(),
        });
    }

    /// Shows others whether this account is muted or deafened, and whether
    /// its camera is on.
    pub fn set_self(&self, guild: &str, channel: &str, mute: bool, deaf: bool, video: bool) {
        let _ = self.requests.send(Request::SelfState {
            guild: guild.into(),
            channel: channel.into(),
            mute,
            deaf,
            video,
        });
    }

    /// Starts sharing to the voice channel this account is in, and waits for
    /// the server Discord assigns the stream.
    pub async fn go_live(&self, guild: &str, channel: &str) -> Result<StreamServer> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::GoLive {
                guild: guild.into(),
                channel: channel.into(),
                reply,
            })
            .map_err(|_| anyhow!("Discord isn't connected. Restart Yapfast and try again."))?;
        answer
            .await
            .map_err(|_| anyhow!("Discord disconnected while starting the stream."))?
    }

    /// Starts watching a stream in the voice channel this account is in.
    pub async fn watch(&self, stream_key: &str) -> Result<StreamServer> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::Watch {
                stream_key: stream_key.into(),
                reply,
            })
            .map_err(|_| anyhow!("Discord isn't connected. Restart Yapfast and try again."))?;
        answer
            .await
            .map_err(|_| anyhow!("Discord disconnected while opening the stream."))?
    }

    /// Ends this account's stream, or stops watching someone else's.
    pub fn end_stream(&self, stream_key: &str) {
        let _ = self.requests.send(Request::EndStream {
            stream_key: stream_key.into(),
        });
    }

    pub fn pause_stream(&self, stream_key: &str, paused: bool) {
        let _ = self.requests.send(Request::PauseStream {
            stream_key: stream_key.into(),
            paused,
        });
    }

    /// Asks for these servers' soundboard sounds, which come as
    /// [`Live::SoundboardSounds`], one for each.
    pub fn request_soundboard(&self, guilds: Vec<String>) {
        if !guilds.is_empty() {
            let _ = self.requests.send(Request::Soundboard { guilds });
        }
    }

    /// Discord's own soundboard sounds, asked for the first time only.
    pub async fn default_sounds(&self) -> Result<Arc<[soundboard::Sound]>> {
        let mut kept = self.default_sounds.lock().await;
        if let Some(sounds) = kept.as_ref() {
            return Ok(sounds.clone());
        }
        let sounds: Arc<[soundboard::Sound]> = self.api.default_soundboard_sounds().await?.into();
        *kept = Some(sounds.clone());
        Ok(sounds)
    }

    /// Plays `sound` for everyone in the call `channel`; see
    /// [`Api::send_soundboard_sound`].
    pub async fn send_sound(&self, channel: &str, sound: &soundboard::Sound) -> Result<()> {
        self.api.send_soundboard_sound(channel, sound).await
    }

    /// The account's favourite sounds, in the order the picker shows them.
    pub async fn favorite_sounds(&self) -> Result<Vec<String>> {
        let settings = self.api.frecency_settings().await?;
        Ok(soundboard::read_favorites(&settings).unwrap_or_default())
    }

    /// Keeps `favorites` as the account's favourite sounds, in place of
    /// the old.
    pub async fn set_favorite_sounds(&self, favorites: &[String]) -> Result<()> {
        self.api
            .set_frecency_settings(&soundboard::write_favorites(favorites))
            .await
    }

    /// Sets how loud the soundboard plays, from 0 to 100, for every client
    /// of the account. The settings come back in USER_SETTINGS_PROTO_UPDATE.
    pub async fn set_soundboard_volume(&self, volume: f32) -> Result<()> {
        let current = self.api.user_settings().await?;
        self.api
            .set_user_settings(&soundboard::write_volume(&current, volume))
            .await
    }

    /// Sets the sound that plays as this account joins a voice channel of
    /// `server`, or of any server without one of its own, or takes it away.
    /// `sound` is its id and its own server's, none for Discord's own.
    pub async fn set_join_sound(
        &self,
        server: Option<&str>,
        sound: Option<(&str, Option<&str>)>,
    ) -> Result<()> {
        let current = self.api.user_settings().await?;
        self.api
            .set_user_settings(&soundboard::write_join_sound(&current, server, sound))
            .await
    }

    /// Has Discord play this account's join sound for the others in the
    /// call `channel`; see [`Api::custom_call_sound`].
    pub async fn play_join_sound(&self, channel: &str) -> Result<()> {
        let animation = rand::random_range(10..=15);
        self.api.custom_call_sound(channel, animation).await
    }

    /// A sound's file, MP3 or Ogg, at once rather than behind the pictures
    /// queued: it plays as soon as it's here.
    pub async fn sound_file(&self, id: &str) -> Result<Vec<u8>> {
        self.api
            .download_now(&soundboard::file(id), SOUND_FILE_LIMIT)
            .await
    }
}

/// Discord takes sounds up to 512 KiB.
const SOUND_FILE_LIMIT: usize = 1 << 20;

/// The server a channel is in, or none for DMs.
fn server(guild: &str) -> Option<&str> {
    (guild != PRIVATE).then_some(guild)
}

/// Whether a request body has any fields to send.
fn some(body: &Value) -> bool {
    body.as_object().is_some_and(|fields| !fields.is_empty())
}

/// The status that silences notifications, as [`Snapshot::presence`] gives it.
pub const DO_NOT_DISTURB: &str = "dnd";
/// How long a member search waits for Discord.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Unix milliseconds now.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Keeps the favourite GIFs on disk, off the async threads.
pub async fn keep_favorite_gifs(favorites: Vec<crate::gifs::Favorite>) {
    let _ = tokio::task::spawn_blocking(move || cache::keep_favorite_gifs(&favorites)).await;
}

/// The bans in a list of them, skipping any without a user.
fn parse_bans(bans: &Value) -> Vec<server_settings::Ban> {
    bans.as_array()
        .into_iter()
        .flatten()
        .filter_map(server_settings::parse_ban)
        .collect()
}

/// An overwrite as Discord's client sends it: type 0 for a role and 1 for
/// a member, and the bits as decimal strings.
fn overwrite_body(overwrite: &ready::Overwrite) -> Value {
    json!({
        "id": overwrite.id,
        "type": u8::from(overwrite.member),
        "allow": overwrite.allow.to_string(),
        "deny": overwrite.deny.to_string(),
    })
}
