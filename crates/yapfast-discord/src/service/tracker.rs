//! The account as the gateway session sees it: servers, voice, DMs, friends
//! and settings, kept current from dispatched events.

use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, RwLock},
};

use serde_json::Value;

use super::{
    Account, BlockedUser, Conversation, DO_NOT_DISTURB, Friend, FriendRequest, Live, Preview,
    Snapshot, Status, VoiceMember, VoiceServer, marks::Published, now_ms, server,
};
use crate::{
    activity::{self, Activities},
    event::Event,
    marked::{MarkedServer, ServerMarks},
    member_list::{self, ListedMember, Presence},
    message::{Message, Names},
    ready::{self, PRIVATE, Private, Server},
    settings::{self, CustomStatus},
    stream::{self, StreamServer},
    unread::Unreads,
};

/// Relationship types: a friend, someone blocked, and friend requests to and
/// from this account.
const FRIEND: u64 = 1;
const BLOCKED: u64 = 2;
const INCOMING_REQUEST: u64 = 3;
const OUTGOING_REQUEST: u64 = 4;

/// The lists of people the last snapshot gave, each kept with a fingerprint
/// of what made it.
#[derive(Default)]
struct Kept {
    /// By DM id.
    conversations: HashMap<String, (u64, Arc<Conversation>)>,
    friends: Option<(u64, Arc<[String]>)>,
    friend_list: Option<(u64, Arc<[Friend]>)>,
    blocked: Option<(u64, Arc<[String]>)>,
    blocked_list: Option<(u64, Arc<[BlockedUser]>)>,
    ignored: Option<(u64, Arc<[String]>)>,
    friend_requests: Option<(u64, Arc<[FriendRequest]>)>,
}

/// The lists of people in a snapshot.
struct People {
    conversations: Vec<Arc<Conversation>>,
    friends: Arc<[String]>,
    friend_list: Arc<[Friend]>,
    blocked: Arc<[String]>,
    blocked_list: Arc<[BlockedUser]>,
    ignored: Arc<[String]>,
    friend_requests: Arc<[FriendRequest]>,
}

/// Whether a DM is muted and unread, its mentions and the last message
/// read, as [`Unreads::private_marks`] gives them.
type Marks = (bool, bool, u64, u64);

/// The list `kept` holds when `inputs`, the fingerprint of what makes it,
/// is the same as when it was made; otherwise a new one.
fn shared<T: ?Sized>(
    kept: &mut Option<(u64, Arc<T>)>,
    inputs: u64,
    make: impl FnOnce() -> Arc<T>,
) -> Arc<T> {
    match kept {
        Some((last, list)) if *last == inputs => list.clone(),
        _ => kept.insert((inputs, make())).1.clone(),
    }
}

/// A hash of what makes a list, to tell whether it changed.
fn fingerprint(value: &impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// What Discord says of a friend or a DM's person.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
struct Known {
    presence: Presence,
    /// Their custom status, else what they're doing, on one line.
    activity: Option<String>,
    activities: Activities,
}

struct VoiceEntry {
    channel: String,
    /// The voice session, which a stream connection reuses.
    session: Option<String>,
    /// Muted and deafened by themselves, and by the server.
    muted: bool,
    deafened: bool,
    server_muted: bool,
    server_deafened: bool,
    streaming: bool,
    video: bool,
}

/// The account's servers and voice channels, kept current from gateway events.
pub(super) struct Tracker {
    pub(super) me: String,
    /// This gateway session, from READY; a voice state of this account with
    /// another is on another client.
    pub(super) session: Option<String>,
    /// This account's Nitro, as [`Account::premium_type`].
    premium: u8,
    /// Whether this account has two-factor authentication on.
    mfa: bool,
    /// Whether it has an email address, as [`Account::has_email`].
    email: bool,
    /// Changed through [`Self::server_mut`], which copies one a snapshot
    /// still shares.
    pub(super) servers: Vec<Arc<Server>>,
    /// Each server as the last snapshot marked it, by id.
    published: HashMap<String, Published>,
    /// The map [`Self::published`] was before the last snapshot, emptied,
    /// for the next to fill.
    emptied: HashMap<String, Published>,
    /// The lists of people the last snapshot gave.
    kept: Kept,
    names: Arc<RwLock<Names>>,
    /// Where each user is in voice, by server and user id; DM and group DM
    /// calls go under [`PRIVATE`].
    voice: HashMap<(String, String), VoiceEntry>,
    /// The calls in DMs and group DMs, by channel id, with whom each rings.
    calls: HashMap<String, Vec<String>>,
    /// This account's roles, by server id.
    my_roles: HashMap<String, Vec<String>>,
    pub(super) unreads: Unreads,
    /// DMs and group DMs.
    private: Vec<Private>,
    /// What friends and DM people are doing: whether they're around, the
    /// line under their name, and their activities in full.
    presences: HashMap<String, Known>,
    /// The account's friends, by user id.
    friends: Vec<String>,
    /// Those this account blocked, by user id.
    blocked: Vec<String>,
    /// Those this account ignores, by user id.
    ignored: Vec<String>,
    /// The nicknames this account gave its friends, by user id.
    nicknames: HashMap<String, String>,
    /// The server list's order and folders, from the settings.
    order: Vec<String>,
    folders: Vec<settings::Folder>,
    /// This account's status and custom status, from its settings.
    pub(super) status: String,
    pub(super) custom_status: Option<CustomStatus>,
    /// How long nobody uses Yapfast before it's away, in seconds, from the
    /// settings.
    pub(super) afk_timeout: u64,
    /// Whether the account shares the game it plays, from the settings, and
    /// the game it shows.
    pub(super) show_game: bool,
    pub(super) playing: Option<Arc<crate::own_activity::Playing>>,
    /// The soundboard's volume and join sounds, from the settings.
    soundboard_volume: Option<f32>,
    join_sounds: Arc<[crate::soundboard::JoinSound]>,
    /// Friend requests by user id: whether each came to this account, whether
    /// Discord took it for spam, and when it was sent.
    requests: HashMap<String, (bool, bool, String)>,
    /// Each message request's latest message, by channel id, and the
    /// requests whose message was asked of Discord.
    previews: HashMap<String, Preview>,
    previews_asked: HashSet<String>,
    /// The RTC server of each stream this account shares or watches, by key.
    streams: HashMap<String, String>,
    /// The channel or thread on screen, and a thread beside it, each of
    /// which lists while it shows though this account isn't in it.
    viewing: Option<String>,
    beside: Option<String>,
}

/// A user's Nitro from their user object, as [`Account::premium_type`].
fn premium_type(user: &Value) -> u8 {
    user["premium_type"]
        .as_u64()
        .and_then(|premium| u8::try_from(premium).ok())
        .unwrap_or_default()
}

impl Tracker {
    pub(super) fn from_ready(me: String, ready: &Value, names: Arc<RwLock<Names>>) -> Self {
        let mut tracker = Self {
            me,
            session: ready["session_id"].as_str().map(str::to_owned),
            premium: premium_type(&ready["user"]),
            mfa: ready["user"]["mfa_enabled"].as_bool().unwrap_or_default(),
            email: ready["user"]["email"].is_string(),
            servers: ready::servers(ready).into_iter().map(Arc::new).collect(),
            published: HashMap::new(),
            emptied: HashMap::new(),
            kept: Kept::default(),
            names,
            voice: HashMap::new(),
            calls: HashMap::new(),
            my_roles: HashMap::new(),
            unreads: Unreads::from_ready(ready),
            private: Vec::new(),
            presences: HashMap::new(),
            friends: Vec::new(),
            blocked: Vec::new(),
            ignored: Vec::new(),
            nicknames: HashMap::new(),
            order: Vec::new(),
            folders: Vec::new(),
            status: "online".into(),
            custom_status: None,
            afk_timeout: settings::AFK_TIMEOUT,
            // As in Discord's apps, until the settings say otherwise.
            show_game: true,
            playing: None,
            soundboard_volume: None,
            join_sounds: Arc::from([]),
            requests: HashMap::new(),
            previews: HashMap::new(),
            previews_asked: HashSet::new(),
            streams: HashMap::new(),
            viewing: None,
            beside: None,
        };
        tracker.sort_servers();
        if let Some(encoded) = ready["user_settings_proto"].as_str() {
            tracker.settings(encoded);
        }
        for relationship in ready["relationships"].as_array().into_iter().flatten() {
            tracker.relationship(relationship);
        }
        tracker.learn_user(&ready["user"]);
        for user in ready["users"].as_array().into_iter().flatten() {
            tracker.learn_user(user);
        }
        for channel in ready["private_channels"].as_array().into_iter().flatten() {
            tracker.put_private(channel);
        }
        tracker.learn_members(&ready["guilds"], &ready["merged_members"]);
        tracker
    }

    /// Keeps what `earlier` knew of servers READY gave as unavailable, as it
    /// gives only their ids.
    pub(super) fn keep_unavailable(&mut self, earlier: &[MarkedServer]) {
        for server in self.servers.iter_mut().filter(|server| server.unavailable) {
            if let Some(known) = earlier.iter().find(|known| known.id == server.id) {
                let mut known = Server::clone(&known.server);
                known.unavailable = true;
                // The snapshot's copy of a server carries a timeout.
                known.my_timeout = None;
                *server = Arc::new(known);
            }
        }
        self.sort_servers();
    }

    /// Keeps the thread `channel` of `guild`, on screen, if `earlier` knew
    /// it and READY, which lists only the threads this account is in,
    /// didn't.
    pub(super) fn keep_thread(&mut self, earlier: &[MarkedServer], guild: &str, channel: &str) {
        let Some(thread) = earlier
            .iter()
            .find(|server| server.id == guild)
            .and_then(|server| server.threads.iter().find(|thread| thread.id == channel))
        else {
            return;
        };
        if let Some(server) = self.server_mut(guild)
            && server.channel(channel).is_none()
        {
            let mut thread = thread.clone();
            if let Some(details) = thread.thread.as_deref_mut() {
                details.leave();
            }
            server.threads.push(thread);
            server
                .threads
                .sort_by_key(|thread| std::cmp::Reverse(thread.id.parse::<u64>().unwrap_or(0)));
        }
    }

    /// Learns of a thread from its channel object.
    pub(super) fn put_thread(&mut self, guild: &str, thread: &Value) {
        self.unreads.learn_thread(thread);
        if let Some(server) = self.server_mut(guild) {
            server.put_thread(thread);
        }
    }

    /// Puts a server's channels and categories where `placements` say,
    /// giving where they were.
    pub(super) fn place(
        &mut self,
        guild: &str,
        placements: &[ready::Placement],
    ) -> Vec<ready::Placement> {
        self.server_mut(guild)
            .map(|server| server.place(placements))
            .unwrap_or_default()
    }

    /// Applies one dispatched event; returns whether anything visible changed.
    pub(super) fn apply(&mut self, event: &Event, data: &Value) -> bool {
        let unreads = self.unreads.apply(event, data, &self.me, &self.my_roles);
        self.apply_servers(event, data) || unreads
    }

    fn apply_servers(&mut self, event: &Event, data: &Value) -> bool {
        match event {
            Event::ReadySupplemental => {
                // The DMs left out of READY to make it smaller.
                for channel in data["lazy_private_channels"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    self.unreads.learn_channel(channel);
                    self.put_private(channel);
                }
                for presence in data["merged_presences"]["friends"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    self.presence(presence["user_id"].as_str(), presence);
                }
                // Servers' presences, for the people of DMs who aren't
                // friends but share a server, as Discord's DM list shows them.
                for presence in data["merged_presences"]["guilds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_array)
                    .flatten()
                {
                    let user = presence["user_id"].as_str();
                    if user.is_some_and(|user| self.in_private(user)) {
                        self.presence(user, presence);
                    }
                }
                for guild in data["guilds"].as_array().into_iter().flatten() {
                    let Some(id) = guild["id"].as_str() else {
                        continue;
                    };
                    for state in guild["voice_states"].as_array().into_iter().flatten() {
                        self.voice_state(id, state);
                    }
                }
                self.learn_members(&data["guilds"], &data["merged_members"]);
                true
            }
            Event::VoiceStateUpdate => {
                match data["guild_id"].as_str() {
                    Some(guild) => {
                        self.learn_member(guild, &data["member"]);
                        self.voice_state(guild, data);
                    }
                    // In a DM or group DM call.
                    None => self.voice_state(PRIVATE, data),
                }
                true
            }
            // Calls in DMs and group DMs. Discord sends one for each call
            // going on when Yapfast connects, as its capabilities ask.
            Event::CallCreate | Event::CallUpdate => {
                let Some(channel) = data["channel_id"].as_str() else {
                    return false;
                };
                let ringing = data["ringing"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|user| user.as_str().map(str::to_owned))
                    .collect();
                self.calls.insert(channel.to_owned(), ringing);
                for state in data["voice_states"].as_array().into_iter().flatten() {
                    self.voice_state(PRIVATE, state);
                }
                true
            }
            Event::CallDelete => {
                let Some(channel) = data["channel_id"].as_str() else {
                    return false;
                };
                self.calls.remove(channel);
                self.voice
                    .retain(|(guild, _), entry| guild != PRIVATE || entry.channel != channel);
                true
            }
            Event::GuildCreate => {
                let Some(server) = ready::server(data) else {
                    return false;
                };
                // Still out: what was known of it stays.
                if server.unavailable
                    && let Some(known) = self.server_mut(&server.id)
                {
                    known.unavailable = true;
                    return true;
                }
                for state in data["voice_states"].as_array().into_iter().flatten() {
                    self.voice_state(&server.id, state);
                }
                for member in data["members"].as_array().into_iter().flatten() {
                    self.learn_member(&server.id, member);
                }
                self.servers.retain(|known| known.id != server.id);
                self.servers.push(Arc::new(server));
                self.sort_servers();
                true
            }
            Event::RelationshipAdd | Event::RelationshipUpdate => {
                self.relationship(data);
                true
            }
            Event::RelationshipRemove => {
                let id = data["id"].as_str().or_else(|| data["user_id"].as_str());
                self.friends.retain(|friend| Some(friend.as_str()) != id);
                self.blocked.retain(|blocked| Some(blocked.as_str()) != id);
                self.ignored.retain(|ignored| Some(ignored.as_str()) != id);
                if let Some(id) = id {
                    self.requests.remove(id);
                    self.nicknames.remove(id);
                }
                true
            }
            Event::GuildMemberUpdate => {
                let Some(guild) = data["guild_id"].as_str() else {
                    return false;
                };
                self.learn_member(guild, data);
                true
            }
            // A server this account left goes with its GUILD_DELETE.
            Event::GuildMemberRemove => {
                let (Some(guild), Some(user)) =
                    (data["guild_id"].as_str(), data["user"]["id"].as_str())
                else {
                    return false;
                };
                self.write_names().forget_member(guild, user);
                false
            }
            Event::GuildRoleCreate | Event::GuildRoleUpdate | Event::GuildRoleDelete => {
                let Some(server) = data["guild_id"]
                    .as_str()
                    .and_then(|guild| self.server_mut(guild))
                else {
                    return false;
                };
                if *event == Event::GuildRoleDelete {
                    if let Some(role) = data["role_id"].as_str() {
                        server.remove_role(role);
                    }
                } else {
                    server.put_role(&data["role"]);
                }
                true
            }
            // The settings of type 1 hold the status.
            Event::UserSettingsProtoUpdate if data["settings"]["type"] == 1 => {
                let Some(encoded) = data["settings"]["proto"].as_str() else {
                    return false;
                };
                self.settings(encoded);
                true
            }
            Event::GuildEmojisUpdate => {
                let Some(server) = data["guild_id"]
                    .as_str()
                    .and_then(|guild| self.server_mut(guild))
                else {
                    return false;
                };
                server.emojis = std::sync::Arc::new(ready::emojis(&data["emojis"]));
                true
            }
            // Carries the server's own fields, without its channels or members.
            Event::GuildUpdate => {
                let (Some(update), Some(known)) = (
                    ready::server(data),
                    data["id"].as_str().and_then(|id| self.server_mut(id)),
                ) else {
                    return false;
                };
                known.name = update.name;
                known.icon = update.icon;
                known.banner = update.banner;
                // Every GUILD_UPDATE has its owner; a transfer changes it.
                if update.owner_id.is_some() {
                    known.owner_id = update.owner_id;
                }
                known.premium_tier = update.premium_tier;
                known.mfa_level = update.mfa_level;
                known.default_level = update.default_level;
                known.forwarding_disabled = update.forwarding_disabled;
                known.verified = update.verified;
                known.community = update.community;
                self.sort_servers();
                true
            }
            // This account's Nitro, name and avatar can change while
            // connected, the last two from its profile editor.
            Event::UserUpdate if data["id"].as_str() == Some(self.me.as_str()) => {
                let premium = premium_type(data);
                let shown = |tracker: &Self| {
                    let names = tracker.read_names();
                    (
                        names.user_name(&tracker.me).map(str::to_owned),
                        names.avatar(None, &tracker.me),
                    )
                };
                let before = shown(self);
                self.learn_user(data);
                let mfa = data["mfa_enabled"].as_bool().unwrap_or(self.mfa);
                // Only this account's own USER_UPDATE has `email`, null
                // for none.
                let email = data.get("email").map_or(self.email, Value::is_string);
                let changed = premium != self.premium
                    || mfa != self.mfa
                    || email != self.email
                    || shown(self) != before;
                self.premium = premium;
                self.mfa = mfa;
                self.email = email;
                changed
            }
            // An outage, with `unavailable`, keeps the server as last known
            // until its GUILD_CREATE; without, this account left it.
            Event::GuildDelete => {
                let Some(id) = data["id"].as_str() else {
                    return false;
                };
                if data["unavailable"].as_bool() == Some(true) {
                    if let Some(server) = self.server_mut(id) {
                        server.unavailable = true;
                    }
                } else {
                    self.servers.retain(|server| server.id != id);
                }
                self.voice.retain(|(guild, _), _| guild != id);
                true
            }
            Event::ChannelCreate | Event::ChannelUpdate | Event::ChannelDelete
                if data["guild_id"].is_null() =>
            {
                let Some(id) = data["id"].as_str() else {
                    return false;
                };
                self.private.retain(|private| private.id != id);
                if *event != Event::ChannelDelete {
                    self.put_private(data);
                }
                true
            }
            Event::ChannelRecipientAdd | Event::ChannelRecipientRemove => {
                let (Some(channel), Some(user)) =
                    (data["channel_id"].as_str(), data["user"]["id"].as_str())
                else {
                    return false;
                };
                self.learn_user(&data["user"]);
                let Some(private) = self
                    .private
                    .iter_mut()
                    .find(|private| private.id == channel)
                else {
                    return false;
                };
                private.recipients.retain(|known| known != user);
                if *event == Event::ChannelRecipientAdd {
                    private.recipients.push(user.to_owned());
                }
                true
            }
            // A DM moves to the top of the list with each new message.
            Event::MessageCreate if data["guild_id"].is_null() => {
                let channel = data["channel_id"].as_str().unwrap_or_default();
                self.private.iter().any(|private| private.id == channel)
            }
            // Friends' presences, and a server's for the people of DMs; the
            // member list takes servers' presences too.
            Event::PresenceUpdate => {
                let user = data["user"]["id"].as_str();
                if !data["guild_id"].is_null() && !user.is_some_and(|user| self.in_private(user)) {
                    return false;
                }
                self.presence(user, data) && self.shows_presence(user)
            }
            // A member list's rows carry presences, which the DMs of those
            // people show too.
            Event::GuildMemberListUpdate => {
                let mut changed = false;
                for op in data["ops"].as_array().into_iter().flatten() {
                    let items = op["items"].as_array().map_or(&[][..], Vec::as_slice);
                    for member in items
                        .iter()
                        .chain([&op["item"]])
                        .map(|item| &item["member"])
                    {
                        let user = member["user"]["id"].as_str();
                        if member["presence"].is_object()
                            && user.is_some_and(|user| self.in_private(user))
                        {
                            changed |= self.presence(user, &member["presence"])
                                && self.shows_presence(user);
                        }
                    }
                }
                changed
            }
            Event::ChannelCreate | Event::ChannelUpdate | Event::ChannelDelete => {
                let (Some(guild), Some(id)) = (data["guild_id"].as_str(), data["id"].as_str())
                else {
                    return false;
                };
                let Some(server) = self.server_mut(guild) else {
                    return false;
                };
                if *event == Event::ChannelDelete {
                    server.remove_channel(id);
                } else {
                    server.put_channel(data);
                }
                true
            }
            Event::ThreadCreate | Event::ThreadUpdate | Event::ThreadDelete => {
                let (Some(guild), Some(id)) = (data["guild_id"].as_str(), data["id"].as_str())
                else {
                    return false;
                };
                let Some(server) = self.server_mut(guild) else {
                    return false;
                };
                if *event == Event::ThreadDelete {
                    server.remove_channel(id);
                } else {
                    server.put_thread(data);
                }
                true
            }
            Event::ThreadListSync => {
                let Some(server) = data["guild_id"]
                    .as_str()
                    .and_then(|guild| self.server_mut(guild))
                else {
                    return false;
                };
                for thread in data["threads"].as_array().into_iter().flatten() {
                    server.put_thread(thread);
                }
                for member in data["members"].as_array().into_iter().flatten() {
                    if let Some(thread) = member["id"].as_str().and_then(|id| server.thread_mut(id))
                    {
                        thread.join(member);
                    }
                }
                // Each thread's newest message, which its row in a list of
                // threads shows.
                let recent = data["most_recent_messages"].as_array();
                for message in recent.into_iter().flatten() {
                    if let Some(thread) = message["channel_id"]
                        .as_str()
                        .and_then(|id| server.thread_mut(id))
                    {
                        thread.sent(message);
                    }
                }
                let guild = data["guild_id"].as_str();
                let mut names = self
                    .names
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                for message in recent.into_iter().flatten() {
                    names.learn_message(guild, message);
                }
                true
            }
            Event::ThreadMemberUpdate => {
                let (Some(guild), Some(id)) = (data["guild_id"].as_str(), data["id"].as_str())
                else {
                    return false;
                };
                let Some(thread) = self
                    .server_mut(guild)
                    .and_then(|server| server.thread_mut(id))
                else {
                    return false;
                };
                thread.join(data);
                true
            }
            // Whether this account joined or left; others' only change
            // the thread's member count, which Yapfast doesn't show.
            Event::ThreadMembersUpdate => {
                let (Some(guild), Some(id)) = (data["guild_id"].as_str(), data["id"].as_str())
                else {
                    return false;
                };
                let me = self.me.clone();
                let added = data["added_members"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|member| member["user_id"] == me.as_str());
                let removed = data["removed_member_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|user| *user == me.as_str());
                let count = data["member_count"]
                    .as_u64()
                    .and_then(|count| u32::try_from(count).ok());
                let preview = ready::member_preview(&data["member_ids_preview"]);
                let known = self
                    .servers
                    .iter()
                    .find(|server| server.id == guild)
                    .and_then(|server| server.threads.iter().find(|thread| thread.id == id))
                    .and_then(|thread| thread.thread.as_deref());
                // Someone else coming or going changes only the count a
                // list of threads shows.
                let recounted = known.is_some_and(|known| {
                    count.is_some_and(|count| count != known.members)
                        || preview
                            .as_ref()
                            .is_some_and(|preview| *preview != known.member_preview)
                });
                if added.is_none() && !removed && !recounted {
                    return false;
                }
                if let Some(member) = added {
                    self.unreads.joined_thread(id, member);
                }
                let Some(thread) = self
                    .server_mut(guild)
                    .and_then(|server| server.thread_mut(id))
                else {
                    return false;
                };
                if let Some(count) = count {
                    thread.members = count;
                }
                if let Some(preview) = preview {
                    thread.member_preview = preview;
                }
                match added {
                    Some(member) => thread.join(member),
                    None if removed => thread.leave(),
                    None => {}
                }
                true
            }
            // A thread counts its messages, which its first message in its
            // channel shows; Discord doesn't send the thread again for it.
            Event::MessageCreate | Event::MessageDelete => {
                let (Some(guild), Some(channel)) =
                    (data["guild_id"].as_str(), data["channel_id"].as_str())
                else {
                    return false;
                };
                let known = self
                    .servers
                    .iter()
                    .find(|server| server.id == guild)
                    .is_some_and(|server| server.threads.iter().any(|thread| thread.id == channel));
                if !known {
                    return false;
                }
                let Some(thread) = self
                    .server_mut(guild)
                    .and_then(|server| server.thread_mut(channel))
                else {
                    return false;
                };
                // Its first message isn't counted: one pointing at the
                // message it was started from, or a forum post's own, which
                // has the post's id.
                let starter = data["type"] == 21;
                let counted = !starter && data["id"].as_str() != Some(channel);
                if *event == Event::MessageCreate {
                    thread.messages += u64::from(counted);
                    if !starter {
                        thread.sent(data);
                    }
                    // A message opens an archived thread again.
                    thread.archived = false;
                } else if counted {
                    thread.messages = thread.messages.saturating_sub(1);
                }
                true
            }
            // Large servers report voice in these until subscribed.
            Event::PassiveUpdateV1 | Event::PassiveUpdateV2 => {
                let Some(guild) = data["guild_id"].as_str() else {
                    return false;
                };
                let members = data["members"]
                    .as_array()
                    .or_else(|| data["updated_members"].as_array());
                for member in members.into_iter().flatten() {
                    self.learn_member(guild, member);
                }
                if *event == Event::PassiveUpdateV1 {
                    self.voice.retain(|(server, _), _| server != guild);
                }
                let states = data["voice_states"]
                    .as_array()
                    .or_else(|| data["updated_voice_states"].as_array());
                for state in states.into_iter().flatten() {
                    self.voice_state(guild, state);
                }
                for user in data["removed_voice_states"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    if let Some(user) = user.as_str() {
                        self.voice.remove(&(guild.to_owned(), user.to_owned()));
                    }
                }
                true
            }
            Event::GuildMembersChunk => {
                let Some(guild) = data["guild_id"].as_str() else {
                    return false;
                };
                for member in data["members"].as_array().into_iter().flatten() {
                    self.learn_member(guild, member);
                }
                true
            }
            // Discord sends STREAM_CREATE to the streamer and those who
            // watch, naming the stream's server again when it moves.
            Event::StreamCreate | Event::StreamUpdate | Event::StreamDelete => {
                let Some(key) = data["stream_key"].as_str() else {
                    return false;
                };
                let rtc = data["rtc_server_id"].as_str().map(str::to_owned);
                match (event, rtc) {
                    (Event::StreamDelete, _) => {
                        self.streams.remove(key);
                    }
                    (Event::StreamCreate, Some(rtc)) => {
                        self.streams.insert(key.to_owned(), rtc);
                    }
                    (Event::StreamUpdate, Some(rtc)) => {
                        if let Some(known) = self.streams.get_mut(key) {
                            *known = rtc;
                        }
                    }
                    _ => {}
                }
                false
            }
            _ => false,
        }
    }

    /// Where a call or stream this account is in went, from a server update
    /// that no join or stream waited for: Discord moved it to another server.
    pub(super) fn moved(&self, event: &Event, data: &Value) -> Option<Live> {
        // The endpoint is null while Discord is still allocating a server.
        let (token, endpoint) = (data["token"].as_str()?, data["endpoint"].as_str()?);
        match event {
            Event::VoiceServerUpdate => {
                let guild = data["guild_id"].as_str().unwrap_or(PRIVATE);
                let entry = self.voice.get(&(guild.to_owned(), self.me.clone()))?;
                // A DM call's update names its channel instead.
                if (guild == PRIVATE && data["channel_id"] != entry.channel.as_str())
                    || self.elsewhere(entry)
                {
                    return None;
                }
                Some(Live::VoiceMoved {
                    guild: guild.to_owned(),
                    server: VoiceServer {
                        user_id: self.me.clone(),
                        session_id: entry.session.clone()?,
                        token: token.to_owned(),
                        endpoint: endpoint.to_owned(),
                    },
                })
            }
            Event::StreamServerUpdate => {
                let key = data["stream_key"].as_str()?;
                let (guild, _, _) = stream::parse(key)?;
                Some(Live::StreamMoved(StreamServer {
                    stream_key: key.to_owned(),
                    user_id: self.me.clone(),
                    session_id: self.voice_session(guild)?.to_owned(),
                    rtc_server_id: self.streams.get(key)?.clone(),
                    token: token.to_owned(),
                    endpoint: endpoint.to_owned(),
                }))
            }
            _ => None,
        }
    }

    /// The voice channel this account is in, in `guild`.
    pub(super) fn voice_channel(&self, guild: &str) -> Option<&str> {
        self.voice
            .get(&(guild.to_owned(), self.me.clone()))
            .map(|entry| entry.channel.as_str())
    }

    pub(super) fn voice_session(&self, guild: &str) -> Option<&str> {
        self.voice
            .get(&(guild.to_owned(), self.me.clone()))
            .and_then(|entry| entry.session.as_deref())
    }

    /// Whether this account's voice state in `guild` belongs to this
    /// session, so that leaving there ends this session's call rather than
    /// another client's.
    pub(super) fn in_voice_here(&self, guild: &str) -> bool {
        self.voice
            .get(&(guild.to_owned(), self.me.clone()))
            .is_some_and(|entry| !self.elsewhere(entry))
    }

    /// Whether a voice state of this account is another client's.
    fn elsewhere(&self, entry: &VoiceEntry) -> bool {
        self.session.is_some() && entry.session.is_some() && entry.session != self.session
    }

    fn voice_state(&mut self, guild: &str, state: &Value) {
        let Some(user) = state["user_id"].as_str() else {
            return;
        };
        let key = (guild.to_owned(), user.to_owned());
        match state["channel_id"].as_str() {
            Some(channel) => {
                let flag = |name: &str| state[name].as_bool().unwrap_or(false);
                self.voice.insert(
                    key,
                    VoiceEntry {
                        channel: channel.to_owned(),
                        session: state["session_id"].as_str().map(str::to_owned),
                        muted: flag("self_mute"),
                        deafened: flag("self_deaf"),
                        server_muted: flag("mute"),
                        server_deafened: flag("deaf"),
                        streaming: flag("self_stream"),
                        video: flag("self_video"),
                    },
                );
            }
            None => {
                self.voice.remove(&key);
            }
        }
    }

    fn learn_user(&mut self, user: &Value) {
        self.write_names().learn_user(user);
    }

    /// Adds a DM or group DM from a channel object, learning the users it
    /// carries.
    fn put_private(&mut self, channel: &Value) {
        let Some(private) = ready::private_channel(channel) else {
            return;
        };
        for user in channel["recipients"].as_array().into_iter().flatten() {
            self.learn_user(user);
        }
        self.private.retain(|known| known.id != private.id);
        self.private.push(private);
    }

    /// Whether the snapshot shows `user`'s presence: the DM list shows the
    /// person of each one-to-one DM, and the friends list each friend.
    fn shows_presence(&self, user: Option<&str>) -> bool {
        self.friends
            .iter()
            .any(|friend| Some(friend.as_str()) == user)
            || self.private.iter().any(|private| {
                !private.group && private.recipients.first().map(String::as_str) == user
            })
    }

    /// Whether `user` is in a DM or group DM with this account.
    fn in_private(&self, user: &str) -> bool {
        self.private
            .iter()
            .any(|private| private.recipients.iter().any(|known| known == user))
    }

    /// Message requests whose latest message is neither known nor asked
    /// for, which count as asked for now. Previews of what's no longer a
    /// request go.
    pub(super) fn previews_wanted(&mut self) -> Vec<String> {
        let requests: HashSet<&str> = self
            .private
            .iter()
            .filter(|private| private.request)
            .map(|private| private.id.as_str())
            .collect();
        self.previews.retain(|id, _| requests.contains(id.as_str()));
        let wanted: Vec<String> = requests
            .into_iter()
            .filter(|id| !self.previews.contains_key(*id) && !self.previews_asked.contains(*id))
            .map(str::to_owned)
            .collect();
        self.previews_asked.extend(wanted.iter().cloned());
        wanted
    }

    /// Takes in Discord's supplemental data for message requests,
    /// `[{channel_id, message_preview}]`; returns whether a preview changed.
    pub(super) fn put_previews(&mut self, found: &Value) -> bool {
        let mut changed = false;
        for entry in found.as_array().into_iter().flatten() {
            if let Some(message) = crate::message::parse(&entry["message_preview"]) {
                changed |= self.preview(&message);
            }
        }
        changed
    }

    /// Keeps a message in a message request as its preview, unless a newer
    /// one is; returns whether the preview changed.
    pub(super) fn preview(&mut self, message: &Message) -> bool {
        let channel = &message.channel_id;
        if !self
            .private
            .iter()
            .any(|private| private.id == *channel && private.request)
        {
            return false;
        }
        if self
            .previews
            .get(channel)
            .is_some_and(|known| known.id.parse::<u64>().ok() > message.id.parse().ok())
        {
            return false;
        }
        let text = self
            .names
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .render(None, &message.content);
        let preview = Preview {
            id: message.id.clone(),
            text,
            files: message.attachments.len(),
        };
        self.previews.insert(channel.clone(), preview.clone()) != Some(preview)
    }

    /// A message request's preview was deleted: it's asked for again.
    pub(super) fn unpreview(&mut self, channel: &str, id: &str) -> bool {
        if !self
            .previews
            .get(channel)
            .is_some_and(|known| known.id == id)
        {
            return false;
        }
        self.previews.remove(channel);
        self.previews_asked.remove(channel);
        true
    }

    /// Learns someone's presence; returns whether it changed.
    fn presence(&mut self, user: Option<&str>, data: &Value) -> bool {
        let Some(user) = user else {
            return false;
        };
        let presence = Known {
            presence: Presence::parse(data),
            activity: member_list::activity(&data["activities"]),
            activities: activity::activities(&data["activities"]),
        };
        self.presences.insert(user.to_owned(), presence.clone()) != Some(presence)
    }

    /// A group DM's members by name, as Discord lists them.
    pub(super) fn group_members(&self, channel: &str) -> Option<Vec<ListedMember>> {
        let group = self
            .private
            .iter()
            .find(|private| private.id == channel && private.group)?;
        let names = self
            .names
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let people = std::iter::once(&self.me).chain(&group.recipients);
        let heading = format!("Members — {}", group.recipients.len() + 1);
        let mut members: Vec<_> = people
            .map(|user| {
                // MemberLists gives this account its own presence.
                let known = self.presences.get(user).cloned().unwrap_or_default();
                ListedMember {
                    user_id: user.clone(),
                    name: names.user_name(user).unwrap_or("Discord user").to_owned(),
                    username: names.username(user).map(str::to_owned),
                    avatar: names.avatar(None, user),
                    group: heading.clone(),
                    presence: known.presence,
                    color: None,
                    activity: known.activity,
                    activities: known.activities,
                    roles: Arc::default(),
                    timed_out_until: None,
                }
            })
            .collect();
        members.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.user_id.cmp(&b.user_id))
        });
        Some(members)
    }

    /// Whether a new message should notify this account. Message requests and
    /// spam don't, and nothing does while its status is Do Not Disturb.
    pub(super) fn notifies(&self, message: &Value) -> bool {
        if self.status == DO_NOT_DISTURB {
            return false;
        }
        let channel = message["channel_id"].as_str().unwrap_or_default();
        let server = message["guild_id"]
            .as_str()
            .and_then(|guild| self.servers.iter().find(|server| server.id == guild));
        let known = server.and_then(|server| server.channel(channel));
        let thread = known.and_then(|known| known.thread.as_deref());
        // A thread notifies as its channel does, unless it says otherwise.
        let place = match (thread, known) {
            (Some(_), Some(known)) => known.parent.as_deref().unwrap_or(channel),
            _ => channel,
        };
        let category = server
            .and_then(|server| server.channel(place))
            .filter(|known| known.thread.is_none())
            .and_then(|known| known.parent.as_deref());
        let author = message["author"]["id"].as_str().unwrap_or_default();
        let unwanted = self.blocked.iter().any(|blocked| blocked == author)
            || self
                .private
                .iter()
                .any(|private| private.id == channel && (private.request || private.spam));
        !unwanted
            && self
                .unreads
                .notifies(message, &self.me, &self.my_roles, place, category, thread)
    }

    /// Learns the status from settings, all or some of them, in base64.
    fn settings(&mut self, encoded: &str) {
        let read = settings::read(encoded);
        if let Some(status) = read.status.filter(|status| !status.is_empty()) {
            self.status = status;
        }
        if let Some(custom) = read.custom {
            self.custom_status = custom;
        }
        if let Some(timeout) = read.afk_timeout {
            self.afk_timeout = timeout;
        }
        if let Some(shown) = read.show_game {
            self.show_game = shown;
        }
        let soundboard = crate::soundboard::read_settings(encoded);
        if let Some(volume) = soundboard.volume {
            self.soundboard_volume = Some(volume);
        }
        if let Some(join_sounds) = soundboard.join_sounds {
            self.join_sounds = join_sounds.into();
        }
        if let Some(list) = settings::server_list(encoded) {
            self.order = list.order;
            self.folders = list.folders;
            self.sort_servers();
        }
    }

    /// A server to change, copied first if a snapshot shares it.
    fn server_mut(&mut self, id: &str) -> Option<&mut Server> {
        self.servers
            .iter_mut()
            .find(|server| server.id == id)
            .map(Arc::make_mut)
    }

    /// Orders the servers as the account's server list does. Those its
    /// settings don't list yet go first, the latest joined first, as Discord
    /// shows a server just joined.
    fn sort_servers(&mut self) {
        let place: HashMap<&str, usize> = self
            .order
            .iter()
            .enumerate()
            .map(|(index, id)| (id.as_str(), index))
            .collect();
        self.servers.sort_by(|a, b| {
            match (place.get(a.id.as_str()), place.get(b.id.as_str())) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (None, None) => b.joined_at.cmp(&a.joined_at),
            }
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
        });
    }

    /// Takes in the channel on screen; returns whether what lists changed,
    /// which it does when a thread comes or goes.
    pub(super) fn view(&mut self, guild: &str, channel: Option<&str>) -> bool {
        let changed = self.is_thread(self.viewing.as_deref()) || self.is_thread(channel);
        self.viewing = channel.filter(|_| guild != PRIVATE).map(str::to_owned);
        changed
    }

    /// Takes in the thread shown beside the channel on screen, or that
    /// none is; returns whether what lists changed.
    pub(super) fn beside(&mut self, thread: Option<&str>) -> bool {
        let changed = self.beside.as_deref() != thread;
        self.beside = thread.map(str::to_owned);
        changed
    }

    fn is_thread(&self, id: Option<&str>) -> bool {
        id.is_some_and(|id| {
            self.servers
                .iter()
                .any(|server| server.threads.iter().any(|thread| thread.id == id))
        })
    }

    /// The channel whose member list shows for a channel: a thread's
    /// channel, as Discord sends no list for a thread.
    pub(super) fn listed_channel(&self, guild: &str, channel: String) -> String {
        let parent = self
            .servers
            .iter()
            .find(|server| server.id == guild)
            .and_then(|server| {
                let known = server.channel(&channel)?;
                known.thread.as_ref()?;
                known.parent.clone()
            });
        parent.unwrap_or(channel)
    }

    /// Who can read a server's channel; see [`ready::Server::readers`].
    pub(super) fn readers(&self, guild: &str, channel: &str) -> String {
        self.servers
            .iter()
            .find(|server| server.id == guild)
            .map_or_else(
                || format!("channel:{channel}"),
                |server| server.readers(channel),
            )
    }

    /// The presence this session shows, as the settings say, and `away`.
    pub(super) fn own_presence(&self, away: settings::Away) -> Value {
        settings::presence(&self.status, self.custom_status.as_ref(), now_ms(), away)
    }

    /// Learns a relationship object: a friend, or someone who no longer is one.
    fn relationship(&mut self, relationship: &Value) {
        if relationship["user"].is_object() {
            self.learn_user(&relationship["user"]);
        }
        let Some(id) = relationship["id"]
            .as_str()
            .or_else(|| relationship["user_id"].as_str())
        else {
            return;
        };
        // An update may leave out what hasn't changed.
        if let Some(ignored) = relationship["user_ignored"].as_bool() {
            self.ignored.retain(|user| user != id);
            if ignored {
                self.ignored.push(id.to_owned());
            }
        }
        if relationship.get("nickname").is_some() {
            match relationship["nickname"]
                .as_str()
                .filter(|nickname| !nickname.is_empty())
            {
                Some(nickname) => self.nicknames.insert(id.to_owned(), nickname.to_owned()),
                None => self.nicknames.remove(id),
            };
        }
        let Some(kind) = relationship["type"].as_u64() else {
            return;
        };
        self.friends.retain(|friend| friend != id);
        self.blocked.retain(|blocked| blocked != id);
        self.requests.remove(id);
        match kind {
            FRIEND => self.friends.push(id.to_owned()),
            BLOCKED => self.blocked.push(id.to_owned()),
            INCOMING_REQUEST | OUTGOING_REQUEST => {
                let spam = relationship["is_spam_request"].as_bool().unwrap_or(false);
                let since = relationship["since"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                self.requests
                    .insert(id.to_owned(), (kind == INCOMING_REQUEST, spam, since));
            }
            _ => {}
        }
    }

    /// The lists of people a snapshot gives: those that stayed the same
    /// since the last are shared with it.
    fn people(&self, names: &Names, kept: &mut Kept) -> People {
        let friends = shared(&mut kept.friends, fingerprint(&self.friends), || {
            self.friends.as_slice().into()
        });
        let blocked = shared(&mut kept.blocked, fingerprint(&self.blocked), || {
            self.blocked.as_slice().into()
        });
        let ignored = shared(&mut kept.ignored, fingerprint(&self.ignored), || {
            self.ignored.as_slice().into()
        });
        // What each list shows of a person, found as building it would, so
        // the names it uses stay remembered.
        let shown = |hasher: &mut DefaultHasher, user: &String| {
            (user, names.user_name(user), names.avatar_hash(user)).hash(hasher);
        };
        let mut hasher = DefaultHasher::new();
        for user in &self.friends {
            shown(&mut hasher, user);
            (self.nicknames.get(user), self.presences.get(user)).hash(&mut hasher);
        }
        let friend_list = shared(&mut kept.friend_list, hasher.finish(), || {
            self.friend_list(names).into()
        });
        let mut hasher = DefaultHasher::new();
        for user in &self.blocked {
            shown(&mut hasher, user);
            names.username(user).hash(&mut hasher);
        }
        let blocked_list = shared(&mut kept.blocked_list, hasher.finish(), || {
            self.blocked_list(names).into()
        });
        let mut hasher = DefaultHasher::new();
        for (user, request) in &self.requests {
            shown(&mut hasher, user);
            request.hash(&mut hasher);
        }
        let friend_requests = shared(&mut kept.friend_requests, hasher.finish(), || {
            self.friend_requests(names).into()
        });
        People {
            conversations: self.conversations(names, &mut kept.conversations),
            friends,
            friend_list,
            blocked,
            blocked_list,
            ignored,
            friend_requests,
        }
    }

    /// Friend requests waiting for an answer, incoming then outgoing, the
    /// newest first.
    fn friend_requests(&self, names: &Names) -> Vec<FriendRequest> {
        let mut requests: Vec<_> = self.requests.iter().collect();
        requests.sort_by(|(_, a), (_, b)| b.0.cmp(&a.0).then_with(|| b.2.cmp(&a.2)));
        requests
            .into_iter()
            .map(|(user, (incoming, spam, _))| FriendRequest {
                user_id: user.clone(),
                name: names.user_name(user).unwrap_or("Discord user").to_owned(),
                avatar: names.avatar(None, user),
                incoming: *incoming,
                spam: *spam,
            })
            .collect()
    }

    /// The friends by name, with what Discord says of each.
    fn friend_list(&self, names: &Names) -> Vec<Friend> {
        let mut friends: Vec<Friend> = self
            .friends
            .iter()
            .map(|user| {
                let known = self.presences.get(user);
                let nickname = self.nicknames.get(user).cloned();
                Friend {
                    user_id: user.clone(),
                    name: nickname.clone().unwrap_or_else(|| {
                        names.user_name(user).unwrap_or("Discord user").to_owned()
                    }),
                    nickname,
                    avatar: names.avatar(None, user),
                    presence: known.map(|known| known.presence),
                    activity: known.and_then(|known| known.activity.clone()),
                    activities: known
                        .map(|known| known.activities.clone())
                        .unwrap_or_default(),
                }
            })
            .collect();
        friends.sort_by_cached_key(|friend| (friend.name.to_lowercase(), friend.user_id.clone()));
        friends
    }

    /// Those this account blocked, by name.
    fn blocked_list(&self, names: &Names) -> Vec<BlockedUser> {
        let mut blocked: Vec<BlockedUser> = self
            .blocked
            .iter()
            .map(|user| {
                let name = names.user_name(user).unwrap_or("Discord user");
                BlockedUser {
                    user_id: user.clone(),
                    name: name.to_owned(),
                    // Under their name only when it's a display name.
                    username: names
                        .username(user)
                        .filter(|username| *username != name)
                        .map(str::to_owned),
                    avatar: names.avatar(None, user),
                }
            })
            .collect();
        blocked.sort_by_cached_key(|user| (user.name.to_lowercase(), user.user_id.clone()));
        blocked
    }

    /// DMs and group DMs, the latest conversation first. Each one the
    /// last snapshot gave the same is shared with it.
    fn conversations(
        &self,
        names: &Names,
        kept: &mut HashMap<String, (u64, Arc<Conversation>)>,
    ) -> Vec<Arc<Conversation>> {
        let mut private: Vec<&Private> = self.private.iter().collect();
        // Discord orders by the newest message, or when the DM opened.
        private.sort_by_cached_key(|private| {
            let opened = private.id.parse().unwrap_or_default();
            std::cmp::Reverse(self.unreads.last_message(&private.id).max(opened))
        });
        let conversations: Vec<_> = private
            .into_iter()
            .map(|private| {
                let marks = self.unreads.private_marks(&private.id);
                let inputs = self.conversation_inputs(private, marks, names);
                let made = || Arc::new(self.conversation(private, marks, names));
                match kept.get_mut(private.id.as_str()) {
                    Some((last, conversation)) if *last == inputs => conversation.clone(),
                    Some(slot) => {
                        *slot = (inputs, made());
                        slot.1.clone()
                    }
                    None => {
                        let conversation = made();
                        kept.insert(private.id.clone(), (inputs, conversation.clone()));
                        conversation
                    }
                }
            })
            .collect();
        // Each DM closed leaves one more kept than listed.
        if kept.len() > conversations.len() {
            let open: HashSet<&str> = self.private.iter().map(|private| &*private.id).collect();
            kept.retain(|id, _| open.contains(id.as_str()));
        }
        conversations
    }

    /// A fingerprint of all [`Self::conversation`] reads, found as it
    /// finds them, so the names it uses stay remembered.
    fn conversation_inputs(&self, private: &Private, marks: Marks, names: &Names) -> u64 {
        let mut hasher = DefaultHasher::new();
        (private, marks).hash(&mut hasher);
        for user in &private.recipients {
            names.user_name(user).hash(&mut hasher);
        }
        if let Some(user) = private.recipients.first() {
            (
                names.avatar_hash(user),
                self.nicknames.get(user),
                self.presences.get(user),
            )
                .hash(&mut hasher);
        }
        let ringing = self.calls.get(&private.id);
        ringing.is_some().hash(&mut hasher);
        for user in ringing.into_iter().flatten() {
            (user, names.user_name(user), names.avatar_hash(user)).hash(&mut hasher);
        }
        if private.request {
            self.previews.get(&private.id).hash(&mut hasher);
        }
        hasher.finish()
    }

    /// A DM or group DM as the DM list shows it.
    fn conversation(&self, private: &Private, marks: Marks, names: &Names) -> Conversation {
        let (muted, unread, mentions, last_read) = marks;
        let name = |user: &String| names.user_name(user).unwrap_or("Discord user");
        // A friend's DM goes by the nickname this account gave them.
        let nickname = |user: &String| self.nicknames.get(user).map(String::as_str);
        let title = match (&private.name, private.recipients.as_slice()) {
            (Some(name), _) => name.clone(),
            (None, []) if private.group => "Unnamed group".into(),
            (None, recipients) if private.group => {
                recipients.iter().map(name).collect::<Vec<_>>().join(", ")
            }
            (None, [user, ..]) => nickname(user).unwrap_or_else(|| name(user)).to_owned(),
            (None, []) => "Discord user".into(),
        };
        let avatar = if private.group {
            private.icon.clone()
        } else {
            private
                .recipients
                .first()
                .map(|user| names.avatar(None, user))
        };
        let call = self.calls.contains_key(&private.id);
        let known = (!private.group)
            .then(|| private.recipients.first())
            .flatten()
            .and_then(|user| self.presences.get(user));
        let ringing = self
            .calls
            .get(&private.id)
            .into_iter()
            .flatten()
            .map(|user| VoiceMember {
                user_id: user.clone(),
                name: name(user).to_owned(),
                avatar: names.avatar(None, user),
                muted: false,
                deafened: false,
                server_muted: false,
                server_deafened: false,
                streaming: false,
                video: false,
                elsewhere: false,
            })
            .collect();
        let preview = private
            .request
            .then(|| self.previews.get(&private.id).cloned())
            .flatten();
        Conversation {
            id: private.id.clone(),
            name: title,
            avatar,
            group: private.group,
            people: private.recipients.len() + 1,
            request: private.request,
            spam: private.spam,
            person: (!private.group)
                .then(|| private.recipients.first().cloned())
                .flatten(),
            unread: unread && !private.spam,
            mentions: if private.request { 0 } else { mentions },
            last_read,
            muted,
            call,
            ringing,
            presence: known.map(|known| known.presence),
            activity: known.and_then(|known| known.activity.clone()),
            activities: known
                .map(|known| known.activities.clone())
                .unwrap_or_default(),
            preview,
        }
    }

    fn learn_member(&mut self, guild: &str, member: &Value) {
        self.write_names().learn_member(guild, member);
        let user = member["user_id"]
            .as_str()
            .or_else(|| member["user"]["id"].as_str());
        if user == Some(self.me.as_str()) {
            self.unreads.learn_own_member(guild, member);
        }
        if user == Some(self.me.as_str())
            && let Some(roles) = member["roles"].as_array()
        {
            let roles = roles
                .iter()
                .filter_map(|role| role.as_str().map(str::to_owned))
                .collect();
            self.my_roles.insert(guild.to_owned(), roles);
        }
    }

    /// `merged_members` lists members per guild, in the same order as `guilds`.
    fn learn_members(&mut self, guilds: &Value, merged: &Value) {
        let guilds = guilds.as_array().into_iter().flatten();
        let merged = merged.as_array().into_iter().flatten();
        for (guild, members) in guilds.zip(merged) {
            let Some(guild) = guild["id"].as_str() else {
                continue;
            };
            for member in members.as_array().into_iter().flatten() {
                self.learn_member(guild, member);
            }
        }
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

    /// When the next thread this account is in goes quiet after `now`, or
    /// its next timeout ends, in Unix milliseconds: a snapshot then stops
    /// listing the thread under its channel, or gives back what the
    /// timeout took.
    pub(super) fn next_quiet(&self, now: u64) -> Option<u64> {
        let names = self.read_names();
        let timeouts = self
            .servers
            .iter()
            .filter_map(|server| names.timed_out_until(&server.id, &self.me));
        self.servers
            .iter()
            .flat_map(|server| &server.threads)
            .filter_map(|thread| {
                let details = thread.thread.as_deref()?;
                let at = details.quiet_at(&thread.id);
                (details.joined && !details.archived && at != u64::MAX).then_some(at)
            })
            .chain(timeouts)
            .filter(|at| *at > now)
            .min()
    }

    /// The account as it is now, but for [`Snapshot::listed`], which the
    /// member lists know.
    pub(super) fn snapshot(&mut self) -> Snapshot {
        let names = self
            .names
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut voice: HashMap<String, Vec<VoiceMember>> = HashMap::new();
        for ((guild, user), entry) in &self.voice {
            voice
                .entry(entry.channel.clone())
                .or_default()
                .push(VoiceMember {
                    user_id: user.clone(),
                    name: names.name(guild, user).unwrap_or("Discord user").to_owned(),
                    avatar: names.avatar(server(guild), user),
                    muted: entry.muted || entry.server_muted,
                    deafened: entry.deafened || entry.server_deafened,
                    server_muted: entry.server_muted,
                    server_deafened: entry.server_deafened,
                    streaming: entry.streaming,
                    video: entry.video,
                    elsewhere: *user == self.me && self.elsewhere(entry),
                });
        }
        for members in voice.values_mut() {
            members.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then_with(|| a.user_id.cmp(&b.user_id))
            });
        }
        // Mark what this account may do in each channel, reusing the servers
        // and marks the last snapshot published the same. Each moves from
        // the last snapshot's map to this one's with its id.
        let mut published = std::mem::take(&mut self.emptied);
        published.reserve(self.servers.len());
        let mut servers = Vec::with_capacity(self.servers.len());
        let now = now_ms();
        for server in &self.servers {
            let roles = self.my_roles.get(&server.id).map_or(&[][..], Vec::as_slice);
            let timeout = names
                .timed_out_until(&server.id, &self.me)
                .filter(|until| *until > now);
            let (id, last) = match self.published.remove_entry(&server.id) {
                Some((id, last)) => (id, Some(last)),
                None => (server.id.clone(), None),
            };
            let last = last.filter(|last| {
                Arc::ptr_eq(&last.source, server)
                    && last.listed.marks.roles == roles
                    && last.timeout == timeout
            });
            // What the account may do stays while the server, its roles
            // and its timeout do.
            let listed = match last {
                Some(last) => {
                    let MarkedServer { server, marks } = last.listed;
                    match marks.unread(&server, &self.unreads, now) {
                        Some(marks) => MarkedServer {
                            server,
                            marks: Arc::new(marks),
                        },
                        None => MarkedServer { server, marks },
                    }
                }
                None => {
                    // A timeout in force takes what the account may do
                    // away; a copy of the server carries it while it lasts.
                    let server = match timeout {
                        Some(until) => Arc::new(Server {
                            my_timeout: Some(ready::Timeout {
                                user: self.me.clone(),
                                until,
                            }),
                            ..(**server).clone()
                        }),
                        None => server.clone(),
                    };
                    let marks = ServerMarks::new(&server, &self.me, roles, &self.unreads, now);
                    MarkedServer {
                        server,
                        marks: Arc::new(marks),
                    }
                }
            };
            servers.push(listed.clone());
            published.insert(
                id,
                Published {
                    source: server.clone(),
                    timeout,
                    listed,
                },
            );
        }
        // What's left is of servers this account left.
        self.published.clear();
        self.emptied = std::mem::replace(&mut self.published, published);
        let mut kept = std::mem::take(&mut self.kept);
        let people = self.people(&names, &mut kept);
        self.kept = kept;
        Snapshot {
            status: Status::Online,
            user_id: Some(self.me.clone()),
            account: names.user_name(&self.me).map(|name| Account {
                name: name.to_owned(),
                avatar: names.avatar(None, &self.me),
                premium_type: self.premium,
                mfa_enabled: self.mfa,
                has_email: self.email,
            }),
            servers,
            folders: self.folders.clone(),
            conversations: people.conversations,
            friends: people.friends,
            friend_list: people.friend_list,
            blocked: people.blocked,
            blocked_list: people.blocked_list,
            ignored: people.ignored,
            presence: self.status.clone(),
            custom_status: self
                .custom_status
                .clone()
                .filter(|custom| !custom.expired(now_ms())),
            show_game: self.show_game,
            soundboard_volume: self.soundboard_volume,
            join_sounds: self.join_sounds.clone(),
            playing: self.playing.clone(),
            friend_requests: people.friend_requests,
            voice,
            listed: None,
            viewing: self.viewing.iter().chain(&self.beside).cloned().collect(),
            names: super::SharedNames::new(self.names.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn tracks_voice_channels_and_names() {
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "users": [{"id": "2", "username": "ann", "global_name": "Ann", "avatar": "abc"}],
            "guilds": [{"id": "10", "properties": {"name": "Test"}, "channels": [
                {"id": "11", "name": "Lounge", "type": 2, "position": 0},
            ]}],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        tracker.apply(
            &Event::ReadySupplemental,
            &json!({
                "guilds": [{"id": "10", "voice_states": [
                    {"user_id": "2", "channel_id": "11", "self_mute": true},
                ]}],
                "merged_members": [[{"user_id": "2", "nick": "Annie"}]],
            }),
        );
        let snapshot = tracker.snapshot();
        let lounge = &snapshot.voice["11"];
        assert_eq!(lounge.len(), 1);
        assert_eq!(lounge[0].name, "Annie");
        assert!(lounge[0].avatar.ends_with("/avatars/2/abc.png?size=128"));
        assert!(lounge[0].muted);

        tracker.apply(
            &Event::VoiceStateUpdate,
            &json!({"guild_id": "10", "user_id": "2", "channel_id": null}),
        );
        assert!(tracker.snapshot().voice.is_empty());

        tracker.apply(
            &Event::ChannelCreate,
            &json!({"guild_id": "10", "id": "12", "name": "Games", "type": 2, "position": 1}),
        );
        let names: Vec<_> = tracker.servers[0]
            .voice_channels
            .iter()
            .map(|channel| channel.name.as_str())
            .collect();
        assert_eq!(names, ["Lounge", "Games"]);

        tracker.apply(
            &Event::GuildUpdate,
            &json!({"id": "10", "name": "Renamed", "icon": "ghi"}),
        );
        let server = &tracker.servers[0];
        assert_eq!(server.name, "Renamed");
        assert!(
            server
                .icon
                .as_ref()
                .is_some_and(|icon| icon.contains("/icons/10/ghi.png"))
        );
        assert_eq!(server.voice_channels.len(), 2, "the channels stay");
    }

    #[test]
    fn keeps_servers_through_an_outage() {
        let guild = |id: &str, name: &str| {
            json!({"id": id, "properties": {"name": name}, "channels": [
                {"id": format!("{id}1"), "name": "general", "type": 0, "position": 0},
            ]})
        };
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "guilds": [guild("10", "Test"), guild("20", "Other")],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let listed = |tracker: &mut Tracker| -> Vec<(String, bool, usize)> {
            let mut servers: Vec<_> = tracker
                .snapshot()
                .servers
                .iter()
                .map(|server| {
                    let channels = server.text_channels.len();
                    (server.name.clone(), server.unavailable, channels)
                })
                .collect();
            servers.sort();
            servers
        };

        tracker.apply(
            &Event::GuildDelete,
            &json!({"id": "10", "unavailable": true}),
        );
        assert_eq!(
            listed(&mut tracker),
            [("Other".into(), false, 1), ("Test".into(), true, 1)],
            "an outage keeps the server as last known"
        );
        // READY in the outage gives only its id; what was shown stays.
        let earlier = tracker.snapshot().servers;
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "guilds": [{"id": "10", "unavailable": true}, guild("20", "Other")],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        tracker.keep_unavailable(&earlier);
        assert_eq!(
            listed(&mut tracker),
            [("Other".into(), false, 1), ("Test".into(), true, 1)]
        );

        tracker.apply(&Event::GuildCreate, &guild("10", "Test"));
        assert_eq!(
            listed(&mut tracker),
            [("Other".into(), false, 1), ("Test".into(), false, 1)],
            "it's back with its GUILD_CREATE"
        );

        // Without `unavailable`, this account left the server.
        tracker.apply(&Event::GuildDelete, &json!({"id": "10"}));
        assert_eq!(listed(&mut tracker), [("Other".into(), false, 1)]);
    }

    #[test]
    fn tells_a_server_mute_from_ones_own() {
        let ready = json!({"user": {"id": "1", "username": "me"}});
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let flags = |tracker: &mut Tracker| {
            let member = tracker.snapshot().voice["11"][0].clone();
            (
                member.muted,
                member.server_muted,
                member.deafened,
                member.server_deafened,
            )
        };
        let state = |extra: Value| {
            let mut state = json!({"guild_id": "10", "user_id": "2", "channel_id": "11"});
            state
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            state
        };
        tracker.apply(&Event::VoiceStateUpdate, &state(json!({"self_mute": true})));
        assert_eq!(flags(&mut tracker), (true, false, false, false));
        tracker.apply(
            &Event::VoiceStateUpdate,
            &state(json!({"mute": true, "deaf": true})),
        );
        assert_eq!(
            flags(&mut tracker),
            (true, true, true, true),
            "the server's"
        );
    }

    #[test]
    fn marks_this_account_in_voice_on_another_client() {
        let ready = json!({"session_id": "here", "user": {"id": "1", "username": "me"}});
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        tracker.apply(
            &Event::ReadySupplemental,
            &json!({"guilds": [{"id": "10", "voice_states": [
                {"user_id": "1", "channel_id": "11", "session_id": "phone"},
                {"user_id": "2", "channel_id": "11", "session_id": "theirs"},
            ]}]}),
        );
        let elsewhere = |tracker: &mut Tracker| {
            let snapshot = tracker.snapshot();
            let mut members: Vec<_> = snapshot.voice["11"]
                .iter()
                .map(|member| (member.user_id.clone(), member.elsewhere))
                .collect();
            members.sort();
            members
        };
        assert_eq!(
            elsewhere(&mut tracker),
            [("1".into(), true), ("2".into(), false)],
            "only this account's state is another client's"
        );
        assert!(
            !tracker.in_voice_here("10"),
            "leaving would end the phone's call"
        );
        let update = json!({"guild_id": "10", "token": "t", "endpoint": "e"});
        assert!(tracker.moved(&Event::VoiceServerUpdate, &update).is_none());

        tracker.apply(
            &Event::VoiceStateUpdate,
            &json!({"guild_id": "10", "user_id": "1", "channel_id": "11", "session_id": "here"}),
        );
        assert_eq!(elsewhere(&mut tracker)[0], ("1".into(), false));
        assert!(tracker.in_voice_here("10"));
    }

    #[test]
    fn tracks_dm_calls_and_whom_they_ring() {
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "users": [{"id": "2", "username": "ann", "global_name": "Ann", "avatar": "abc"}],
            "private_channels": [{"id": "20", "type": 1, "recipient_ids": ["2"]}],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        tracker.apply(
            &Event::CallCreate,
            &json!({"channel_id": "20", "message_id": "30", "region": "rotterdam",
            "ringing": ["1"], "voice_states": [
                {"user_id": "2", "channel_id": "20", "session_id": "s", "self_mute": true},
            ]}),
        );
        let snapshot = tracker.snapshot();
        let call = &snapshot.voice["20"];
        assert_eq!((call[0].name.as_str(), call[0].muted), ("Ann", true));
        assert!(call[0].avatar.ends_with("/avatars/2/abc.png?size=128"));
        let ringing: Vec<_> = snapshot.conversations[0]
            .ringing
            .iter()
            .map(|person| (person.user_id.as_str(), person.name.as_str()))
            .collect();
        assert_eq!(ringing, [("1", "me")]);

        // Answering: this account joins, without a server, and stops ringing.
        tracker.apply(
            &Event::VoiceStateUpdate,
            &json!({"user_id": "1", "channel_id": "20", "session_id": "mine"}),
        );
        tracker.apply(
            &Event::CallUpdate,
            &json!({"channel_id": "20", "message_id": "30", "ringing": []}),
        );
        assert_eq!(tracker.voice_channel(PRIVATE), Some("20"));
        assert_eq!(tracker.voice_session(PRIVATE), Some("mine"));
        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.voice["20"].len(), 2);
        assert!(snapshot.conversations[0].ringing.is_empty());

        tracker.apply(&Event::CallDelete, &json!({"channel_id": "20"}));
        assert!(tracker.snapshot().voice.is_empty());
    }

    #[test]
    fn dms_show_their_persons_presence() {
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "users": [
                {"id": "2", "username": "ann", "global_name": "Ann"},
                {"id": "3", "username": "cy"},
            ],
            "private_channels": [
                {"id": "20", "type": 1, "recipient_ids": ["2"]},
                {"id": "30", "type": 3, "recipient_ids": ["2", "3"], "name": "Group"},
            ],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        assert_eq!(
            tracker.snapshot().conversations[0].presence,
            None,
            "unknown until Discord says"
        );
        let away = json!({"user": {"id": "2"}, "status": "idle",
            "activities": [{"type": 4, "name": "Custom Status", "state": "making tea"}]});
        assert!(
            tracker.apply(&Event::PresenceUpdate, &away),
            "Ann's DM shows it"
        );
        assert!(
            !tracker.apply(&Event::PresenceUpdate, &away),
            "the same again changes nothing"
        );
        let snapshot = tracker.snapshot();
        let dm = snapshot
            .conversations
            .iter()
            .find(|dm| dm.id == "20")
            .unwrap();
        assert_eq!(dm.presence, Some(Presence::Idle));
        assert_eq!(dm.activity.as_deref(), Some("making tea"));
        let group = snapshot
            .conversations
            .iter()
            .find(|dm| dm.id == "30")
            .unwrap();
        assert_eq!(group.presence, None, "a group has no one person");
        // Someone with no one-to-one DM doesn't change the list.
        assert!(!tracker.apply(
            &Event::PresenceUpdate,
            &json!({"user": {"id": "3"}, "status": "online"})
        ));
    }

    #[test]
    fn a_dm_shows_the_presence_of_a_shared_server() {
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "users": [{"id": "2", "username": "ann"}, {"id": "3", "username": "cy"}],
            "private_channels": [{"id": "20", "type": 1, "recipient_ids": ["2"]}],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let presence = |tracker: &mut Tracker| tracker.snapshot().conversations[0].presence;
        tracker.apply(
            &Event::ReadySupplemental,
            &json!({"merged_presences": {"friends": [], "guilds": [[
                {"user_id": "2", "status": "idle"},
                {"user_id": "3", "status": "online"},
            ]]}}),
        );
        assert_eq!(presence(&mut tracker), Some(Presence::Idle));
        assert!(
            !tracker.presences.contains_key("3"),
            "nobody keeps the presence of a member with no DM"
        );
        assert!(tracker.apply(
            &Event::PresenceUpdate,
            &json!({"guild_id": "10", "user": {"id": "2"}, "status": "dnd"})
        ));
        assert_eq!(presence(&mut tracker), Some(Presence::DoNotDisturb));
        assert!(
            !tracker.apply(
                &Event::PresenceUpdate,
                &json!({"guild_id": "11", "user": {"id": "2"}, "status": "dnd"})
            ),
            "another server's copy changes nothing"
        );
        assert!(!tracker.apply(
            &Event::PresenceUpdate,
            &json!({"guild_id": "10", "user": {"id": "3"}, "status": "idle"})
        ));
        assert!(!tracker.presences.contains_key("3"));
        let row = |user: &str, status: &str| json!({"member": {"user": {"id": user}, "presence": {"status": status}}});
        assert!(
            tracker.apply(
                &Event::GuildMemberListUpdate,
                &json!({"guild_id": "10", "id": "everyone", "ops": [
                    {"op": "SYNC", "range": [0, 99], "items": [{"group": {"id": "online"}}, row("3", "online")]},
                    {"op": "UPDATE", "index": 4, "item": row("2", "online")},
                ]}),
            ),
            "a member list's row shows in the DM"
        );
        assert_eq!(presence(&mut tracker), Some(Presence::Online));
        assert!(!tracker.presences.contains_key("3"));
    }

    #[test]
    fn orders_servers_as_the_settings_do() {
        let guild = |id: &str, name: &str, joined: &str| json!({"id": id, "name": name, "joined_at": joined, "channels": [], "roles": []});
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "guilds": [
                guild("10", "Alpha", "2026-01-01T00:00:00+00:00"),
                guild("20", "Bravo", "2026-02-01T00:00:00+00:00"),
                guild("30", "Charlie", "2026-03-01T00:00:00+00:00"),
                guild("40", "Delta", "2026-04-01T00:00:00+00:00"),
            ],
            // Charlie on its own, then a folder of Alpha and Delta; Bravo is
            // missing, as a server joined from another client may be.
            "user_settings_proto": settings::write_server_order(&[&[30], &[10, 40]]),
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let order = |tracker: &Tracker| -> Vec<String> {
            tracker
                .servers
                .iter()
                .map(|server| server.name.clone())
                .collect()
        };
        assert_eq!(order(&tracker), ["Bravo", "Charlie", "Alpha", "Delta"]);

        // A server just joined goes first, until the settings place it.
        tracker.apply(
            &Event::GuildCreate,
            &guild("50", "Echo", "2026-05-01T00:00:00+00:00"),
        );
        assert_eq!(
            order(&tracker),
            ["Echo", "Bravo", "Charlie", "Alpha", "Delta"]
        );
        tracker.apply(
            &Event::UserSettingsProtoUpdate,
            &json!({"settings": {"type": 1, "proto":
                settings::write_server_order(&[&[40], &[50], &[30], &[20], &[10]])}}),
        );
        assert_eq!(
            order(&tracker),
            ["Delta", "Echo", "Charlie", "Bravo", "Alpha"]
        );

        // Changing only the status leaves the order alone.
        tracker.apply(
            &Event::UserSettingsProtoUpdate,
            &json!({"settings": {"type": 1, "proto": settings::write_custom(None)}, "partial": true}),
        );
        assert_eq!(
            order(&tracker),
            ["Delta", "Echo", "Charlie", "Bravo", "Alpha"]
        );
        assert!(tracker.snapshot().folders.is_empty());

        // A folder keeps its servers together, in its order.
        let games = settings::Folder {
            id: "7".into(),
            name: Some("Games".into()),
            color: None,
            servers: vec!["40".into(), "10".into()],
        };
        tracker.apply(
            &Event::UserSettingsProtoUpdate,
            &json!({"settings": {"type": 1, "proto": settings::write_server_list(&[
                settings::Folder { servers: vec!["20".into()], ..Default::default() },
                games.clone(),
                settings::Folder { servers: vec!["30".into()], ..Default::default() },
            ])}}),
        );
        assert_eq!(
            order(&tracker),
            ["Echo", "Bravo", "Delta", "Alpha", "Charlie"]
        );
        assert_eq!(tracker.snapshot().folders, [games]);
    }

    #[test]
    fn shows_the_status_the_settings_hold() {
        let away = CustomStatus {
            text: "Fixing the bridge".into(),
            emoji_name: Some("🔧".into()),
            ..Default::default()
        };
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "user_settings_proto": settings::write_custom(Some(&away)),
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        assert_eq!(tracker.snapshot().custom_status, Some(away));
        assert_eq!(tracker.snapshot().presence, "online");
        assert_eq!(
            tracker.own_presence(settings::Away::default())["activities"][0]["state"],
            "Fixing the bridge"
        );

        // Settings of another type say nothing of it; clearing it does.
        let cleared = settings::write_custom(None);
        assert!(!tracker.apply(
            &Event::UserSettingsProtoUpdate,
            &json!({"settings": {"type": 2, "proto": cleared}, "partial": true}),
        ));
        assert!(tracker.snapshot().custom_status.is_some());
        assert!(tracker.apply(
            &Event::UserSettingsProtoUpdate,
            &json!({"settings": {"type": 1, "proto": cleared}, "partial": true}),
        ));
        assert_eq!(tracker.snapshot().custom_status, None);
        assert_eq!(
            tracker.own_presence(settings::Away::default())["activities"],
            json!([])
        );
    }

    #[test]
    fn tracks_friend_requests_both_ways() {
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "relationships": [
                {"id": "2", "type": 1, "user": {"id": "2", "username": "ann"}},
                {"id": "3", "type": 3, "since": "2026-10-01T10:00:00+00:00",
                    "user": {"id": "3", "username": "bob"}},
                {"id": "4", "type": 4, "since": "2026-10-02T10:00:00+00:00",
                    "user": {"id": "4", "username": "cat"}},
                {"id": "5", "type": 3, "since": "2026-10-03T10:00:00+00:00",
                    "is_spam_request": true, "user": {"id": "5", "username": "spam"}},
            ],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let listed = |tracker: &mut Tracker| -> Vec<(String, bool, bool)> {
            tracker
                .snapshot()
                .friend_requests
                .iter()
                .map(|request| (request.name.clone(), request.incoming, request.spam))
                .collect()
        };
        assert_eq!(
            listed(&mut tracker),
            [
                ("spam".to_owned(), true, true),
                ("bob".to_owned(), true, false),
                ("cat".to_owned(), false, false),
            ],
            "those sent to this account first, the newest first"
        );

        // Accepting Bob's makes him a friend; Cat's is cancelled.
        tracker.apply(
            &Event::RelationshipAdd,
            &json!({"id": "3", "type": 1, "user": {"id": "3", "username": "bob"}}),
        );
        tracker.apply(&Event::RelationshipRemove, &json!({"id": "4", "type": 4}));
        tracker.apply(
            &Event::RelationshipUpdate,
            &json!({"id": "5", "nickname": "x"}),
        );
        assert_eq!(listed(&mut tracker), [("spam".to_owned(), true, true)]);
        assert_eq!(*tracker.snapshot().friends, ["2", "3"]);
        let friends: Vec<_> = tracker
            .snapshot()
            .friend_list
            .iter()
            .map(|friend| (friend.name.clone(), friend.presence))
            .collect();
        assert_eq!(
            friends,
            [("ann".to_owned(), None), ("bob".to_owned(), None)],
            "by name"
        );
        // A friend's presence changes the friends list, though there's no DM with them.
        assert!(tracker.apply(
            &Event::PresenceUpdate,
            &json!({"user": {"id": "3"}, "status": "idle", "activities": []}),
        ));
        assert_eq!(
            tracker.snapshot().friend_list[1].presence,
            Some(Presence::Idle)
        );

        // Blocking Ann ends the friendship, and her messages stop notifying.
        tracker.apply(&Event::RelationshipAdd, &json!({"id": "2", "type": 2}));
        assert_eq!(*tracker.snapshot().friends, ["3"]);
        assert_eq!(*tracker.snapshot().blocked, ["2"]);
        let blocked = &tracker.snapshot().blocked_list[0];
        assert_eq!(
            (blocked.name.as_str(), blocked.username.as_deref()),
            ("ann", None),
            "the blocked list names her, her username once"
        );
        let from =
            |author: &str| json!({"channel_id": "9", "author": {"id": author}, "content": "hi"});
        assert!(!tracker.notifies(&from("2")));
        tracker.apply(&Event::RelationshipRemove, &json!({"id": "2", "type": 2}));
        assert!(tracker.snapshot().blocked.is_empty());
        assert!(tracker.notifies(&from("2")));

        // Do Not Disturb silences everyone, until the status changes.
        tracker.status = DO_NOT_DISTURB.into();
        assert!(!tracker.notifies(&from("3")));
        tracker.status = "idle".into();
        assert!(tracker.notifies(&from("3")));
    }

    #[test]
    fn follows_calls_and_streams_to_new_servers() {
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "guilds": [{"id": "10", "properties": {"name": "Test"}, "channels": [
                {"id": "11", "name": "Lounge", "type": 2, "position": 0},
            ]}],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let update =
            json!({"guild_id": "10", "token": "new", "endpoint": "moved.discord.media:443"});
        assert!(
            tracker.moved(&Event::VoiceServerUpdate, &update).is_none(),
            "not in a call there"
        );
        tracker.apply(
            &Event::VoiceStateUpdate,
            &json!({"guild_id": "10", "user_id": "1", "channel_id": "11", "session_id": "s"}),
        );
        let Some(Live::VoiceMoved { guild, server }) =
            tracker.moved(&Event::VoiceServerUpdate, &update)
        else {
            panic!("the call moved");
        };
        assert_eq!(guild, "10");
        assert_eq!(
            (
                server.session_id.as_str(),
                server.token.as_str(),
                server.endpoint.as_str()
            ),
            ("s", "new", "moved.discord.media:443")
        );
        let allocating = json!({"guild_id": "10", "token": "new", "endpoint": null});
        assert!(
            tracker
                .moved(&Event::VoiceServerUpdate, &allocating)
                .is_none()
        );

        // A stream moves where its last STREAM_CREATE said.
        let key = stream::key("10", "11", "1");
        let server_update =
            json!({"stream_key": key, "token": "t2", "endpoint": "moved.discord.media:443"});
        assert!(
            tracker
                .moved(&Event::StreamServerUpdate, &server_update)
                .is_none(),
            "a stream it isn't in"
        );
        tracker.apply(
            &Event::StreamCreate,
            &json!({"stream_key": key, "rtc_server_id": "rtc"}),
        );
        tracker.apply(
            &Event::StreamCreate,
            &json!({"stream_key": key, "rtc_server_id": "rtc2"}),
        );
        let Some(Live::StreamMoved(server)) =
            tracker.moved(&Event::StreamServerUpdate, &server_update)
        else {
            panic!("the stream moved");
        };
        assert_eq!(
            (
                server.rtc_server_id.as_str(),
                server.session_id.as_str(),
                server.token.as_str()
            ),
            ("rtc2", "s", "t2")
        );
        tracker.apply(
            &Event::StreamDelete,
            &json!({"stream_key": key, "reason": "user_requested"}),
        );
        assert!(
            tracker
                .moved(&Event::StreamServerUpdate, &server_update)
                .is_none()
        );
    }

    #[test]
    fn lists_dms_newest_first_with_unreads() {
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "users": [
                {"id": "2", "username": "ann", "global_name": "Ann", "avatar": "abc"},
                {"id": "3", "username": "bob"},
            ],
            "private_channels": [
                {"id": "20", "type": 1, "recipient_ids": ["2"], "last_message_id": "500"},
                {"id": "21", "type": 3, "recipient_ids": ["2", "3"], "last_message_id": "600"},
                {"id": "22", "type": 3, "name": "Plans", "icon": "def", "recipient_ids": ["3"]},
                {"id": "23", "type": 1, "recipient_ids": ["3"], "is_message_request": true},
            ],
            "read_state": {"entries": [{"id": "21", "last_message_id": "550", "mention_count": 1}]},
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let listed = |tracker: &mut Tracker| -> Vec<(String, bool, u64)> {
            tracker
                .snapshot()
                .conversations
                .into_iter()
                .filter(|dm| !dm.request)
                .map(|dm| (dm.name.clone(), dm.unread, dm.mentions))
                .collect()
        };
        assert_eq!(
            listed(&mut tracker),
            [
                ("Ann, bob".into(), true, 1),
                ("Ann".into(), false, 0),
                ("Plans".into(), false, 0),
            ],
            "a DM without a read state is read"
        );
        let snapshot = tracker.snapshot();
        let dm = |id: &str| {
            snapshot
                .conversations
                .iter()
                .find(|dm| dm.id == id)
                .unwrap()
        };
        let ann = dm("20");
        assert!(!ann.group && ann.people == 2);
        assert!(
            ann.avatar
                .as_deref()
                .unwrap()
                .ends_with("/avatars/2/abc.png?size=128")
        );
        assert!(
            dm("22")
                .avatar
                .as_deref()
                .unwrap()
                .contains("/channel-icons/22/def.png")
        );
        assert_eq!(dm("21").avatar, None);

        // Ann writes: her DM comes first, and each message counts.
        let message = |id: &str| json!({"id": id, "channel_id": "20", "author": {"id": "2"}});
        assert!(tracker.apply(&Event::MessageCreate, &message("700")));
        tracker.apply(&Event::MessageCreate, &message("701"));
        assert_eq!(listed(&mut tracker)[0], ("Ann".into(), true, 2));
        assert_eq!(
            tracker.unreads.mark_read(PRIVATE, "20").as_deref(),
            Some("701")
        );
        assert_eq!(listed(&mut tracker)[0], ("Ann".into(), false, 0));

        // A muted DM shows nothing.
        tracker.apply(
            &Event::UserGuildSettingsUpdate,
            &json!({"guild_id": null, "channel_overrides": [{"channel_id": "20", "muted": true}]}),
        );
        tracker.apply(&Event::MessageCreate, &message("702"));
        assert_eq!(listed(&mut tracker)[0], ("Ann".into(), false, 0));

        // DMs open, close and change their people.
        tracker.apply(
            &Event::ChannelCreate,
            &json!({"id": "800", "type": 1, "recipients": [{"id": "4", "username": "cat"}]}),
        );
        assert_eq!(listed(&mut tracker)[0].0, "cat");
        tracker.apply(&Event::ChannelDelete, &json!({"id": "800", "type": 1}));
        tracker.apply(
            &Event::ChannelRecipientAdd,
            &json!({"channel_id": "22", "user": {"id": "4", "username": "cat"}}),
        );
        tracker.apply(
            &Event::ChannelRecipientRemove,
            &json!({"channel_id": "22", "user": {"id": "3"}}),
        );
        tracker.apply(
            &Event::PresenceUpdate,
            &json!({"user": {"id": "4"}, "status": "idle", "activities": []}),
        );
        let members: Vec<_> = tracker
            .group_members("22")
            .unwrap()
            .into_iter()
            .map(|member| (member.name, member.group, member.presence))
            .collect();
        assert_eq!(
            members,
            [
                ("cat".into(), "Members — 2".into(), Presence::Idle),
                // MemberLists gives this account its own presence.
                ("me".into(), "Members — 2".into(), Presence::Offline),
            ]
        );
        assert!(
            tracker.group_members("20").is_none(),
            "a DM lists no members"
        );
        assert_eq!(
            tracker.snapshot().conversations.len(),
            4,
            "with the request"
        );
    }

    #[test]
    fn message_requests_wait_to_be_answered() {
        let ready = json!({
            "user": {"id": "1", "username": "me"},
            "users": [{"id": "2", "username": "ann"}, {"id": "3", "username": "spam"}],
            "private_channels": [
                {"id": "20", "type": 1, "recipient_ids": ["2"], "is_message_request": true},
                {"id": "21", "type": 1, "recipient_ids": ["3"], "is_message_request": true, "is_spam": true},
            ],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        for (id, channel, author) in [("500", "20", "2"), ("501", "21", "3")] {
            tracker.apply(
                &Event::MessageCreate,
                &json!({"id": id, "channel_id": channel, "author": {"id": author}}),
            );
        }
        let listed: Vec<_> = tracker
            .snapshot()
            .conversations
            .into_iter()
            .map(|dm| (dm.name.clone(), dm.request, dm.spam, dm.unread, dm.mentions))
            .collect();
        assert_eq!(
            listed,
            [
                ("spam".into(), true, true, false, 0),
                ("ann".into(), true, false, true, 0)
            ],
            "requests count no mentions, and spam isn't unread"
        );

        // Each request is asked for its message once, and shows the newest
        // it knows; a deleted one is asked for again.
        let mut wanted = tracker.previews_wanted();
        wanted.sort();
        assert_eq!(wanted, ["20", "21"]);
        assert!(tracker.previews_wanted().is_empty());
        let message = |id: &str, content: &str| {
            crate::message::parse(&json!({
                "id": id, "channel_id": "20", "content": content,
                "author": {"id": "2", "username": "ann"},
            }))
            .unwrap()
        };
        assert!(tracker.preview(&message("502", "Later")));
        assert!(
            tracker.put_previews(&json!([{"channel_id": "20", "message_preview": {
                "id": "503", "channel_id": "20", "content": "Latest", "author": {"id": "2"},
            }}]))
        );
        assert!(
            !tracker.preview(&message("500", "Hello")),
            "an older message stays out"
        );
        let previewed = |tracker: &mut Tracker| {
            let snapshot = tracker.snapshot();
            let dm = snapshot
                .conversations
                .iter()
                .find(|dm| dm.id == "20")
                .unwrap();
            dm.preview.as_ref().map(|preview| preview.text.clone())
        };
        assert_eq!(previewed(&mut tracker).as_deref(), Some("Latest"));
        assert!(tracker.unpreview("20", "503"));
        assert_eq!(previewed(&mut tracker), None);
        assert_eq!(tracker.previews_wanted(), ["20"]);
        assert!(tracker.preview(&message("504", "Again")));

        // Accepting makes it a DM; ignoring removes it.
        tracker.apply(
            &Event::ChannelUpdate,
            &json!({"id": "20", "type": 1, "recipient_ids": ["2"], "is_message_request": false}),
        );
        tracker.apply(&Event::ChannelDelete, &json!({"id": "21", "type": 1}));
        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.conversations.len(), 1);
        assert!(!snapshot.conversations[0].request);
        assert_eq!(snapshot.conversations[0].mentions, 1);
        assert_eq!(
            snapshot.conversations[0].preview, None,
            "a DM shows no request's message"
        );
        assert!(tracker.previews_wanted().is_empty());
        assert!(!tracker.preview(&message("505", "Hi")));
    }

    #[test]
    fn marks_voice_channels_this_account_can_join() {
        let hidden = r#"[{"id": "10", "type": 0, "allow": "0", "deny": "1024"},
                         {"id": "20", "type": 0, "allow": "1024", "deny": "0"}]"#;
        let ready = json!({
            "user": {"id": "1"},
            "guilds": [{"id": "10", "properties": {"name": "Test", "owner_id": "9"},
                "roles": [{"id": "10", "permissions": "1049600"}, {"id": "20", "permissions": "0"}],
                "channels": [
                    {"id": "11", "name": "Lounge", "type": 2, "position": 0},
                    {"id": "12", "name": "Staff", "type": 2, "position": 1,
                     "permission_overwrites": serde_json::from_str::<Value>(hidden).unwrap()},
                ]}],
            "merged_members": [[{"user_id": "1", "roles": []}]],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let channels = |tracker: &mut Tracker| -> Vec<String> {
            tracker.snapshot().servers[0]
                .voice_channels()
                .filter(|channel| channel.marks.access == ready::Access::Join)
                .map(|channel| channel.name.clone())
                .collect()
        };
        assert_eq!(channels(&mut tracker), ["Lounge"]);
        tracker.apply(
            &Event::GuildMemberUpdate,
            &json!({"guild_id": "10", "user": {"id": "1"}, "roles": ["20"]}),
        );
        assert_eq!(channels(&mut tracker), ["Lounge", "Staff"]);
    }

    #[test]
    fn a_timeout_leaves_this_account_reading_until_it_ends() {
        let now = now_ms() / 1000 * 1000;
        let iso = |ms: u64| {
            chrono::DateTime::from_timestamp_millis(ms as i64)
                .unwrap()
                .to_rfc3339()
        };
        let until = now + 3_600_000;
        let ready = json!({
            "user": {"id": "1"},
            "guilds": [{"id": "10", "properties": {"name": "Test", "owner_id": "9"},
                // Viewing, sending and connecting.
                "roles": [{"id": "10", "permissions": "1051648"}],
                "channels": [
                    {"id": "11", "name": "general", "type": 0, "position": 0},
                    {"id": "12", "name": "Lounge", "type": 2, "position": 1},
                ]}],
            "merged_members": [[{"user_id": "1", "roles": [],
                "communication_disabled_until": iso(until)}]],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let snapshot = tracker.snapshot();
        let server = &snapshot.servers[0];
        assert_eq!(
            server.my_timeout,
            Some(ready::Timeout {
                user: "1".into(),
                until
            })
        );
        let access = |server: &MarkedServer| {
            server
                .text_channels()
                .chain(server.voice_channels())
                .map(|channel| channel.marks.access)
                .collect::<Vec<_>>()
        };
        assert_eq!(access(server), [ready::Access::Locked; 2]);
        // The copy that carries the timeout is kept while it lasts.
        assert_eq!(&tracker.snapshot().servers[0], server);
        assert!(Arc::ptr_eq(
            &server.server,
            &tracker.snapshot().servers[0].server
        ));
        assert_eq!(tracker.next_quiet(now), Some(until), "published again then");

        // Once it ends, what it took comes back.
        tracker.apply(
            &Event::GuildMemberUpdate,
            &json!({"guild_id": "10", "user": {"id": "1"}, "roles": [],
                "communication_disabled_until": iso(now - 1000)}),
        );
        let snapshot = tracker.snapshot();
        let server = &snapshot.servers[0];
        assert_eq!(server.my_timeout, None);
        assert_eq!(access(server), [ready::Access::Join; 2]);
        assert_eq!(tracker.next_quiet(now), None);
    }

    #[test]
    fn keeps_what_a_list_of_threads_shows_of_each() {
        let ready = json!({
            "user": {"id": "1"},
            "guilds": [{"id": "10", "properties": {"name": "Test", "owner_id": "9"},
                "roles": [{"id": "10", "permissions": "0"}],
                "channels": [{"id": "11", "name": "general", "type": 0, "position": 0}],
                "threads": []}],
            "merged_members": [[{"user_id": "1", "roles": []}]],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        // Subscribing syncs the server's active threads with their newest
        // messages, whose authors' names are learnt.
        let sync = json!({"guild_id": "10",
            "threads": [{"id": "300", "type": 11, "name": "Book club", "parent_id": "11",
                "guild_id": "10", "last_message_id": "350", "member_count": 2,
                "member_ids_preview": ["2", "3"],
                "thread_metadata": {"archived": false, "locked": false}}],
            "members": [],
            "most_recent_messages": [{"id": "350", "channel_id": "300", "type": 0,
                "content": "Chapter 3 by Friday?",
                "author": {"id": "2", "username": "bob", "global_name": "Bob"}}]});
        assert!(tracker.apply(&Event::ThreadListSync, &sync));
        let details = |tracker: &mut Tracker| {
            tracker.snapshot().servers[0].threads[0]
                .thread
                .as_deref()
                .unwrap()
                .clone()
        };
        let known = details(&mut tracker);
        assert_eq!((known.last_message, known.members), (350, 2));
        assert_eq!(known.member_preview, ["2", "3"]);
        let recent = known.recent.unwrap();
        assert_eq!(
            (recent.author.as_str(), recent.content.as_str()),
            ("2", "Chapter 3 by Friday?")
        );
        assert_eq!(tracker.names.read().unwrap().user_name("2"), Some("Bob"));

        // A later message is its newest; someone else joining changes its count.
        let message = json!({"id": "360", "channel_id": "300", "guild_id": "10", "type": 0,
            "content": "Yes", "author": {"id": "3", "username": "carol"}});
        assert!(tracker.apply(&Event::MessageCreate, &message));
        let joined = json!({"id": "300", "guild_id": "10", "member_count": 3,
            "added_members": [{"user_id": "4"}], "removed_member_ids": []});
        assert!(tracker.apply(&Event::ThreadMembersUpdate, &joined));
        assert!(
            !tracker.apply(&Event::ThreadMembersUpdate, &joined),
            "nothing changed"
        );
        let known = details(&mut tracker);
        assert_eq!((known.last_message, known.members), (360, 3));
        assert!(!known.joined);
        assert_eq!(known.recent.unwrap().content, "Yes");
        // An update without its newest message keeps it.
        let update = json!({"id": "300", "type": 11, "name": "Book club!", "parent_id": "11",
            "guild_id": "10", "last_message_id": "350",
            "thread_metadata": {"archived": false, "locked": false}});
        assert!(tracker.apply(&Event::ThreadUpdate, &update));
        let known = details(&mut tracker);
        assert_eq!(known.last_message, 360);
        assert_eq!(known.member_preview, ["2", "3"]);
    }

    #[test]
    fn a_thread_goes_quiet_after_its_archive_duration_without_activity() {
        use crate::message::snowflake_at;
        let now = now_ms() / 1000 * 1000;
        let hour = 3_600_000;
        let iso = |ms: u64| {
            chrono::DateTime::from_timestamp_millis(ms as i64)
                .unwrap()
                .to_rfc3339()
        };
        let thread = |id: u64, last: u64, opened: u64| {
            json!({"id": id.to_string(), "type": 11, "name": "Plans", "parent_id": "11",
                "guild_id": "10", "last_message_id": last.to_string(),
                "thread_metadata": {"archived": false, "locked": false,
                    "auto_archive_duration": 1440, "archive_timestamp": iso(opened)},
                "member": {"join_timestamp": iso(opened), "flags": 0}})
        };
        let ready = json!({
            "user": {"id": "1"},
            "guilds": [{"id": "10", "properties": {"name": "Test", "owner_id": "1"},
                "channels": [{"id": "11", "name": "general", "type": 0, "position": 0}],
                "threads": [
                    // Its last message was two days ago.
                    thread(snowflake_at(now - 72 * hour), snowflake_at(now - 48 * hour), now - 72 * hour),
                    // Opened again an hour ago, a day after its last message.
                    thread(snowflake_at(now - 96 * hour), snowflake_at(now - 25 * hour), now - hour),
                ]}],
            "merged_members": [[{"user_id": "1", "roles": []}]],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let quiet = |tracker: &mut Tracker| -> Vec<bool> {
            tracker.snapshot().servers[0]
                .threads()
                .map(|thread| thread.marks.quiet)
                .collect()
        };
        assert_eq!(quiet(&mut tracker), [true, false]);
        // The one opened again goes quiet a day after.
        assert_eq!(tracker.next_quiet(now), Some(now + 23 * hour));

        // A message wakes the quiet one for another day.
        let id = snowflake_at(now - 72 * hour).to_string();
        let message = snowflake_at(now).to_string();
        tracker.apply(
            &Event::MessageCreate,
            &json!({"id": message, "channel_id": id, "guild_id": "10", "author": {"id": "2"}}),
        );
        assert_eq!(quiet(&mut tracker), [false, false]);
        assert_eq!(tracker.next_quiet(now), Some(now + 23 * hour));
    }

    #[test]
    fn a_media_channel_lists_as_a_forum() {
        let ready = json!({
            "user": {"id": "1"},
            "guilds": [{"id": "10", "properties": {"name": "Test", "owner_id": "1"},
                "channels": [{"id": "16", "name": "mods", "type": 16, "position": 0}]}],
            "merged_members": [[{"user_id": "1", "roles": []}]],
        });
        let tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let channels = &tracker.servers[0].text_channels;
        assert_eq!(channels.len(), 1);
        assert!(channels[0].forum.is_some());
    }

    #[test]
    fn a_forum_posts_first_message_isnt_a_reply() {
        let ready = json!({
            "user": {"id": "1"},
            "guilds": [{"id": "10", "properties": {"name": "Test", "owner_id": "1"},
                "channels": [{"id": "15", "name": "ideas", "type": 15, "position": 0}]}],
            "merged_members": [[{"user_id": "1", "roles": []}]],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let post = json!({"id": "600", "type": 11, "name": "Idea", "parent_id": "15",
            "guild_id": "10", "owner_id": "2", "message_count": 0,
            "thread_metadata": {"archived": false, "locked": false}});
        assert!(tracker.apply(&Event::ThreadCreate, &post));
        let message = |id: &str| {
            json!({"id": id, "type": 0, "channel_id": "600", "guild_id": "10",
                   "author": {"id": "2"}, "content": "An idea"})
        };
        let counted = |tracker: &Tracker| {
            let details = tracker.servers[0].threads[0].thread.as_deref().unwrap();
            (details.messages, details.last_message)
        };
        // Its first message has its id: the newest, but not a reply.
        tracker.apply(&Event::MessageCreate, &message("600"));
        assert_eq!(counted(&tracker), (0, 600));
        tracker.apply(&Event::MessageCreate, &message("601"));
        assert_eq!(counted(&tracker), (1, 601));
        tracker.apply(&Event::MessageDelete, &message("600"));
        assert_eq!(counted(&tracker).0, 1);
    }

    #[test]
    fn follows_threads_this_account_is_in_and_views() {
        let may = (ready::permission::VIEW_CHANNEL
            | ready::permission::SEND_MESSAGES
            | ready::permission::SEND_MESSAGES_IN_THREADS)
            .to_string();
        let ready = json!({
            "user": {"id": "1"},
            "guilds": [{"id": "10", "properties": {"name": "Test", "owner_id": "9"},
                "roles": [{"id": "10", "permissions": may}],
                "channels": [{"id": "11", "name": "general", "type": 0, "position": 0}],
                "threads": [{"id": "300", "type": 11, "name": "Plans", "parent_id": "11",
                    "guild_id": "10", "last_message_id": "350", "message_count": 2,
                    "thread_metadata": {"archived": false, "locked": false},
                    "member": {"join_timestamp": "2026-01-01T00:00:00+00:00", "flags": 0}}]}],
            "merged_members": [[{"user_id": "1", "roles": []}]],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let threads = |tracker: &mut Tracker| -> Vec<(String, bool, ready::Access, bool, u64)> {
            tracker.snapshot().servers[0]
                .threads()
                .map(|thread| {
                    let details = thread.thread.as_deref().unwrap();
                    (
                        thread.id.clone(),
                        details.joined,
                        thread.marks.access,
                        thread.marks.unread,
                        details.messages,
                    )
                })
                .collect()
        };
        use ready::Access::{Hidden, Join, Locked};
        // Read up to when it joined.
        assert_eq!(
            threads(&mut tracker),
            [("300".into(), true, Join, false, 2)]
        );

        // Someone else's thread is known, the newest first, without this
        // account in it; its messages count, but not its first.
        let made = json!({"id": "400", "type": 11, "name": "Later", "parent_id": "11",
            "guild_id": "10", "owner_id": "2", "thread_metadata": {"archived": false, "locked": false}});
        assert!(tracker.apply(&Event::ThreadCreate, &made));
        let message = |channel: &str, id: &str, kind: u64| {
            json!({"id": id, "type": kind, "channel_id": channel, "guild_id": "10",
                   "author": {"id": "2"}, "content": "hi"})
        };
        tracker.apply(
            &Event::MessageCreate,
            &message("400", "1500000000000000000", 21),
        );
        tracker.apply(
            &Event::MessageCreate,
            &message("400", "1500000000000000001", 0),
        );
        tracker.apply(
            &Event::MessageCreate,
            &message("300", "1500000000000000002", 0),
        );
        assert_eq!(
            threads(&mut tracker),
            [
                ("400".into(), false, Join, true, 1),
                ("300".into(), true, Join, true, 3),
            ]
        );

        // Joining and leaving it, as Discord tells this account.
        let members = |added: Value, removed: Value| {
            json!({"id": "400", "guild_id": "10", "member_count": 2,
                   "added_members": added, "removed_member_ids": removed})
        };
        tracker.apply(
            &Event::ThreadMembersUpdate,
            &members(
                json!([{"user_id": "1", "flags": 0, "muted": true}]),
                json!([]),
            ),
        );
        let joined = tracker.servers[0].threads[0].thread.clone().unwrap();
        assert!(joined.joined && joined.mute.muted);
        // An update without the member keeps it.
        let locked = json!({"id": "400", "type": 11, "name": "Later", "parent_id": "11",
            "guild_id": "10", "thread_metadata": {"archived": false, "locked": true}});
        tracker.apply(&Event::ThreadUpdate, &locked);
        assert_eq!(
            threads(&mut tracker)[0],
            ("400".into(), true, Locked, false, 1)
        );
        tracker.apply(
            &Event::ThreadMembersUpdate,
            &members(json!([]), json!(["1"])),
        );
        assert!(
            !tracker.servers[0].threads[0]
                .thread
                .as_ref()
                .unwrap()
                .joined
        );

        // A private thread it isn't in stays hidden.
        let private = json!({"id": "500", "type": 12, "name": "Secret", "parent_id": "11",
            "guild_id": "10", "thread_metadata": {"archived": false, "locked": false}});
        tracker.put_thread("10", &private);
        assert_eq!(threads(&mut tracker)[0].2, Hidden);

        // Viewing a thread lists it, so what lists changes.
        assert!(tracker.view("10", Some("400")));
        assert_eq!(tracker.snapshot().viewing, ["400"]);
        assert_eq!(tracker.listed_channel("10", "400".into()), "11");
        assert!(tracker.view("10", Some("11")));
        assert!(!tracker.view("10", Some("11")));
        // Or beside it.
        assert!(tracker.beside(Some("400")));
        assert_eq!(tracker.snapshot().viewing, ["11", "400"]);
        assert!(tracker.beside(None));

        // Its channel takes its threads along when deleted.
        tracker.apply(
            &Event::ChannelDelete,
            &json!({"id": "11", "guild_id": "10", "type": 0}),
        );
        assert!(threads(&mut tracker).is_empty());
    }

    #[test]
    fn snapshots_share_the_servers_that_stay_the_same() {
        let ready = json!({
            "user": {"id": "1"},
            "guilds": [
                {"id": "10", "properties": {"name": "One"}, "joined_at": "2026-01-02T00:00:00+00:00",
                 "channels": [{"id": "11", "name": "general", "type": 0, "position": 0}]},
                {"id": "20", "properties": {"name": "Two"}, "joined_at": "2026-01-01T00:00:00+00:00",
                 "channels": [{"id": "21", "name": "general", "type": 0, "position": 0}]},
            ],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let shared = |one: &MarkedServer, other: &MarkedServer| {
            (
                Arc::ptr_eq(&one.server, &other.server),
                Arc::ptr_eq(&one.marks, &other.marks),
            )
        };
        let first = tracker.snapshot();
        let same = tracker.snapshot();
        assert_eq!(shared(&first.servers[0], &same.servers[0]), (true, true));
        assert_eq!(shared(&first.servers[1], &same.servers[1]), (true, true));

        // A message marks one server's channel unread: only its marks are
        // new, and the other server's stay shared.
        tracker.apply(
            &Event::MessageCreate,
            &json!({"id": "900", "channel_id": "21", "guild_id": "20", "author": {"id": "2"}}),
        );
        let unread = tracker.snapshot();
        assert_eq!(shared(&first.servers[0], &unread.servers[0]), (true, true));
        assert_eq!(shared(&first.servers[1], &unread.servers[1]), (true, false));
        let general = |server: &MarkedServer| server.text_channels().next().unwrap().marks.unread;
        assert!(general(&unread.servers[1]));
        assert!(!general(&first.servers[1]), "published ones don't change");

        // Changing a server copies it rather than the one a snapshot holds.
        tracker.apply(&Event::GuildUpdate, &json!({"id": "10", "name": "Renamed"}));
        let renamed = tracker.snapshot();
        assert_eq!(renamed.servers[0].name, "Renamed");
        assert_eq!(unread.servers[0].name, "One");
        assert_eq!(
            shared(&unread.servers[1], &renamed.servers[1]),
            (true, true)
        );
    }

    #[test]
    fn snapshots_share_the_dms_and_lists_that_stay_the_same() {
        let ready = json!({
            "user": {"id": "1"},
            "users": [{"id": "2", "username": "ann"}, {"id": "3", "username": "bob"}],
            "relationships": [{"id": "2", "type": 1}, {"id": "3", "type": 1}],
            "private_channels": [
                {"id": "20", "type": 1, "recipient_ids": ["2"]},
                {"id": "30", "type": 1, "recipient_ids": ["3"]},
            ],
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let dm = |snapshot: &Snapshot, id: &str| {
            let found = snapshot.conversations.iter().find(|dm| dm.id == id);
            found.unwrap().clone()
        };
        let first = tracker.snapshot();
        let same = tracker.snapshot();
        assert!(Arc::ptr_eq(&dm(&first, "20"), &dm(&same, "20")));
        assert!(Arc::ptr_eq(&dm(&first, "30"), &dm(&same, "30")));
        assert!(Arc::ptr_eq(&first.friends, &same.friends));
        assert!(Arc::ptr_eq(&first.friend_list, &same.friend_list));
        assert!(Arc::ptr_eq(&first.friend_requests, &same.friend_requests));

        // Bob going idle changes his DM and the friends list, nothing else.
        tracker.apply(
            &Event::PresenceUpdate,
            &json!({"user": {"id": "3"}, "status": "idle", "activities": []}),
        );
        let idle = tracker.snapshot();
        assert_eq!(dm(&idle, "30").presence, Some(Presence::Idle));
        assert!(!Arc::ptr_eq(&dm(&first, "30"), &dm(&idle, "30")));
        assert!(!Arc::ptr_eq(&first.friend_list, &idle.friend_list));
        assert!(Arc::ptr_eq(&dm(&first, "20"), &dm(&idle, "20")));
        assert!(Arc::ptr_eq(&first.friends, &idle.friends));

        // A message from Ann marks her DM unread, and only hers.
        tracker.apply(
            &Event::MessageCreate,
            &json!({"id": "900", "channel_id": "20", "author": {"id": "2"}}),
        );
        let unread = tracker.snapshot();
        assert!(dm(&unread, "20").unread);
        assert!(!dm(&idle, "20").unread, "published ones don't change");
        assert!(Arc::ptr_eq(&dm(&idle, "30"), &dm(&unread, "30")));
        assert!(Arc::ptr_eq(&idle.friend_list, &unread.friend_list));

        // A DM closed is forgotten.
        tracker.apply(
            &Event::ChannelDelete,
            &json!({"id": "30", "type": 1, "recipient_ids": ["3"]}),
        );
        assert_eq!(tracker.snapshot().conversations.len(), 1);
        assert_eq!(tracker.kept.conversations.len(), 1);
    }

    #[test]
    fn a_voice_channels_chat_is_unread_as_a_text_channel_is() {
        let ready = json!({
            "user": {"id": "1"},
            "guilds": [{"id": "10", "properties": {"name": "One"},
                "channels": [{"id": "11", "name": "Lounge", "type": 2, "position": 0,
                              "last_message_id": "800"}]}],
            "read_state": {"entries": [{"id": "11", "last_message_id": "700", "mention_count": 1}]},
        });
        let mut tracker = Tracker::from_ready("1".into(), &ready, Default::default());
        let marks = |tracker: &mut Tracker| {
            let snapshot = tracker.snapshot();
            let voice = snapshot.servers[0].voice_channels().next().unwrap().marks;
            (voice.unread, voice.mentions, voice.last_read)
        };
        assert_eq!(marks(&mut tracker), (true, 1, 700));
        tracker.apply(
            &Event::MessageCreate,
            &json!({"id": "900", "channel_id": "11", "guild_id": "10",
                    "author": {"id": "2"}, "mentions": [{"id": "1"}]}),
        );
        assert_eq!(marks(&mut tracker), (true, 2, 700));
        tracker.apply(
            &Event::MessageAck,
            &json!({"channel_id": "11", "message_id": "900", "mention_count": 0}),
        );
        assert_eq!(marks(&mut tracker), (false, 0, 900));
    }
}
