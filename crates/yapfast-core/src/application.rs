//! Shared client behavior. UI adapters dispatch actions and render the resulting state.
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use crate::{
    chat::{
        self, ChatMember, ChatMessage, ChatRoom, ChatServer, Command, Contact, DiscordCommand,
        Event, ForwardFailure, MentionGroup, Picture, Profile, ProfileRole, Reaction, Reply, Role,
    },
    discord,
    mention::{self, Candidate, Target},
    network::Network,
    voice::{self, JoinSound, Phase},
};

mod arrange;
mod aside;
mod browse;
mod calls;
mod channel_settings;
mod compose;
mod discord_account;
mod forum;
mod gifs;
mod header;
mod join;
mod members;
mod messages;
mod mfa;
mod new_post;
mod place_search;
mod places;
mod polls;
mod preview;
mod profile_editor;
mod profiles;
mod reactions;
mod rooms;
mod scheduled_event;
mod server;
mod server_settings;
mod session;
mod threads;

pub use arrange::{Landing, MoveSync};
pub use browse::{BrowseChannels, BrowsedChannel, BrowsedKind};
pub use channel_settings::{
    ChannelInput, ChannelSection, ChannelSettingsScreen, OverwriteConfirm, OverwriteRules,
    OverwriteState, Overwrites, channel_groups, channel_permission,
};
pub use forum::{ForumInfo, ForumPost, ForumPosts, ForumView};
pub use gifs::{GifPicker, GifPicture, GifView};
pub use join::JoinDialog;
pub use members::{BAN_HISTORY, RoleChoice, TIMEOUT_TIMES};
#[cfg(all(feature = "discord", any(test, feature = "test-fixtures")))]
pub use members::{member_server, with_clubs};
pub use mfa::MfaPrompt;
pub use new_post::{NewPost, POST_TAGS, POST_TITLE_LIMIT};
pub use place_search::Place;
pub use places::{
    ChannelAsk, ChannelDialog, INVITE_AGES, INVITE_USES, InviteDialog, LeaveDialog, ListedCategory,
    SLOWMODES,
};
pub use polls::{
    ANSWER_LIMIT, DraftAnswer, POLL_ANSWERS, POLL_HOURS, PollDraft, PollEdit, QUESTION_LIMIT,
};
pub use profile_editor::{
    BIO_LIMIT, NAME_LIMIT, PRONOUNS_LIMIT, PictureEdit, ProfileEditor, ProfileText,
};
pub use reactions::{ReactionTip, ReactionsWindow, TIP_PEOPLE};
pub use rooms::{MUTE_TIMES, MenuChoice, network_id, network_link};
pub use scheduled_event::EventDialog;
pub use server_settings::{
    ADD_MEMBERS_LIMIT, AFK_TIMEOUTS, AUDIT_ACTIONS, AUDIT_PAGE, AddMembers, AuditInput, AuditKind,
    AuditLine, AuditLogPage, AuditRow, AuditSpan, AuditWords, BANS_BATCH, BANS_PER_PAGE, BOOSTED,
    BanInput, BansPage, BoostPage, DEFAULT_ROLE_COLOR, DESCRIPTION_LIMIT, DeleteAsk,
    EMOJI_NAME_LIMIT, EmojiInput, EmojiPage, EmojiUpload, EngagementPage, ExpressionRules,
    FILTER_ALL, HIGHEST_VERIFICATION, InviteInput, InvitesPage, LEVEL_BOOSTS, MEMBER_PAGE_SIZES,
    MEMBER_SORTS, MEMBERS_CHUNK, MemberInput, MembersPage, MfaRule, Ownership, OwnershipInput,
    PAUSE_HOURS, Page as SettingsPage, PermissionGroup, PermissionInfo, PermissionLock,
    ProfilePage, ROLE_COLORS, ROLE_NAME_LIMIT, ROLE_PERMISSIONS, RoleConfirm, RoleDraft, RoleInput,
    RoleLock, RoleRules, RoleTab, RolesPage, SETUP_TIPS, STICKER_DESCRIPTION_LIMIT,
    STICKER_NAME_LIMIT, SafetyChoice, SafetyPage, SecurityDraft, ServerSettings, SettingsChoice,
    SettingsText, SettingsToggle, StickerCard, StickerDraft, StickerInput, StickersPage,
    TransferAsk, WELCOME, WELCOME_STICKER, emoji_name,
};
pub use threads::{
    NewThread, THREAD_NAME_LIMIT, ThreadFrom, ThreadList, ThreadSearch, ThreadSections,
    default_thread_name,
};

use calls::call_sounds;
use compose::has_markdown;
use preview::{preview_members, sample_actions};
use profiles::dm_profile;

/// How many people the composer suggests for an `@` word, and how many roles
/// and groups after them.
const MENTION_SUGGESTIONS: usize = 8;
const MENTION_GROUPS: usize = 4;

const STOPPED: &str = "The connection has stopped. Restart Yapfast to restore your session.";

/// Reactions offered before someone has used any, in order.
pub const DEFAULT_REACTIONS: [&str; 4] = ["👍", "❤️", "😂", "🎉"];

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Navigation {
    #[default]
    DirectMessages,
    Server(String),
}

/// The dialog for starting a DM, while it's open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewDm {
    pub query: String,
    /// Who each network found for the query.
    pub discord: Vec<Contact>,
    pub matrix: Vec<Contact>,
    /// A DM is starting.
    pub starting: bool,
    pub error: Option<String>,
}

/// Linking a Discord account by QR code or token, while it's under way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscordLink {
    /// Yapfast is asking Discord for a code.
    Starting,
    /// Discord is checking a pasted token.
    Checking,
    /// The code to scan, as the address it carries.
    Code(String),
    /// Someone scanned it, by their username, and confirms on their phone.
    Scanned(String),
    /// They confirmed, and Discord wants a captcha solved first, on the page
    /// at this address, open in the browser.
    Captcha(String),
    Failed(String),
}

/// The dialog that sends a friend request by Discord username.
#[derive(Clone, Debug, Default)]
pub struct AddFriend {
    pub sending: bool,
    /// The last request's outcome: who it went to, or why it didn't.
    pub outcome: Option<Result<String, String>>,
}

/// A dialog from a Discord DM's or member's menu about its person: writing
/// a note or a friend nickname, confirming an unfriending or a block, or in
/// a server, their nickname, a timeout, a kick or a ban.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonDialog {
    pub user: String,
    /// Their name, as the DM or the server shows it.
    pub name: String,
    pub ask: PersonAsk,
    /// The note, nickname or reason as it stands; `None` while the note is
    /// on its way.
    pub text: Option<String>,
    pub saving: bool,
    pub error: Option<String>,
    /// The Discord server, by its server id, for a member's dialog.
    pub server: Option<String>,
    /// Which of a timeout's [`TIMEOUT_TIMES`], or of a ban's
    /// [`BAN_HISTORY`], is chosen.
    pub choice: usize,
}

/// What a [`PersonDialog`] asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersonAsk {
    Note,
    /// The nickname this account gives a friend.
    Nickname,
    Unfriend,
    Block,
    // A server member's, with [`Action::AskMember`].
    /// Their nickname in the server, this account's own included.
    ServerNickname,
    Timeout,
    Kick,
    Ban,
}

/// The dialog that sets the Discord custom status.
#[derive(Clone, Debug, Default)]
pub struct StatusEditor {
    pub saving: bool,
    pub error: Option<String>,
}

/// When a custom status clears itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearAfter {
    Never,
    Minutes(u32),
    /// At the next local midnight.
    Today,
}

impl ClearAfter {
    /// Unix milliseconds when it clears, from `now`.
    fn at(self, now: chrono::DateTime<chrono::Local>) -> Option<u64> {
        let at = match self {
            ClearAfter::Never => return None,
            ClearAfter::Minutes(minutes) => now + chrono::Duration::minutes(minutes.into()),
            ClearAfter::Today => now
                .date_naive()
                .succ_opt()?
                .and_hms_opt(0, 0, 0)?
                .and_local_timezone(chrono::Local)
                .earliest()?,
        };
        u64::try_from(at.timestamp_millis()).ok()
    }
}

impl NewDm {
    /// Everyone found, Discord's first.
    pub fn people(&self) -> impl Iterator<Item = &Contact> {
        self.discord.iter().chain(&self.matrix)
    }
}

/// A DM's pinned messages, while their list is open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pins {
    pub room: String,
    /// The latest pinned first; `None` while they're on their way.
    pub messages: Option<Result<Vec<ChatMessage>, String>>,
}

/// A search, while its results show: of a DM, or of a whole Discord server,
/// which stays open while its channels do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Search {
    /// Where it looks: a DM's room id, or a Discord server's id.
    pub place: String,
    pub query: String,
    /// How many messages it found in all, of which `results` are loaded,
    /// the newest first.
    pub total: u32,
    pub results: Vec<chat::SearchHit>,
    /// Where the next page of results starts among them all.
    pub next: u32,
    /// A page of results is on its way.
    pub loading: bool,
    pub error: Option<String>,
}

impl Search {
    /// Found messages are still to load.
    pub fn more(&self) -> bool {
        self.next < self.total
    }
}

/// Adding friends to a DM, while the picker is open: to a one-to-one DM
/// it makes a new group, with the other person too.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddPeople {
    pub room: String,
    pub query: String,
    /// The friends chosen, by user id, in the order chosen.
    pub chosen: Vec<String>,
    pub adding: bool,
    pub error: Option<String>,
}

/// The most people in a Discord group DM, this account included.
pub const GROUP_LIMIT: usize = 10;

/// The most conversations a message is forwarded to at once, as Discord's
/// client allows.
pub const FORWARD_LIMIT: usize = 5;

/// A message being forwarded, while its dialog is open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Forwarding {
    /// The room the message is in, and its item there.
    pub room: String,
    pub item: String,
    /// What the dialog shows of it: its text, or its first file's name.
    pub quote: String,
    /// The conversations chosen, in the order chosen, up to
    /// [`FORWARD_LIMIT`].
    pub chosen: Vec<String>,
    /// The message added to it when it was last sent, which the dialog
    /// shows again after a failure.
    pub note: String,
    /// Why it couldn't be forwarded to those chosen, after a try that
    /// failed there.
    pub error: Option<String>,
    /// What the dialog's search looks for.
    pub query: String,
    /// Those chosen when the search last changed, which list first while
    /// it's empty; see [`AppState::forward_places`].
    pub pinned: Vec<String>,
}

/// The conversations this account used, which rank where a message is
/// forwarded and where the quick switcher goes; see
/// [`AppState::forward_places`] and [`AppState::switcher_places`].
#[derive(Default)]
pub struct PlacesUsed {
    /// By Discord server or channel id, as Discord's clients counted them,
    /// with this session's uses added.
    uses: HashMap<String, chat::PlaceUses>,
    /// This session's uses, by channel id, added again to each count that
    /// comes.
    pending: Vec<(String, u64)>,
    /// The conversations opened this session, of every network, the latest
    /// first, up to sixteen.
    visited: Vec<String>,
    /// Discord's counts came, or were asked for.
    fresh: bool,
    asked: bool,
    /// Counts changes to the counts and to those opened lately.
    changes: u64,
}

/// A forward sent, until each conversation it went to answers.
struct ForwardSent {
    forwarding: Forwarding,
    waiting: Vec<String>,
    /// Where it wasn't forwarded, and why.
    failed: Vec<(String, String)>,
    /// Where the message added to it wasn't sent, and why.
    notes_unsent: Vec<(String, String)>,
}

/// An image attachment shown large, and the message's other images to step to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Viewer {
    pub item: String,
    /// The attachment shown, by its place in the message.
    pub index: usize,
    /// Images at viewing size by attachment: `None` while one downloads,
    /// else the image or why it can't show.
    pub pictures: HashMap<usize, Option<Result<Arc<Picture>, &'static str>>>,
}

/// A person's profile card: beside the member list, or large over the window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileView {
    pub profile: Profile,
    pub large: bool,
    /// The rest of a Discord profile is on its way.
    pub loading: bool,
    /// It shows the profile Yapfast kept from before while Discord's comes.
    pub kept: bool,
    /// Why Discord didn't give the rest.
    pub note: Option<String>,
}

/// A screen the top of the DM list opens in place of a conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmScreen {
    /// The Discord friends list.
    Friends,
    /// Friend requests, message requests and spam.
    Requests,
}

/// A role someone clicked in a message: its name and the members known to have it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoleView {
    pub id: String,
    pub name: String,
    pub color: Option<u32>,
    /// `None` while they're on their way.
    pub members: Option<Vec<ChatMember>>,
}

/// What the composer's next send does in a room, besides sending a new message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Compose {
    Reply {
        item: String,
        author: String,
        body: String,
        /// Whether the reply notifies the person it replies to.
        ping: bool,
    },
    /// Editing replaces the draft; cancelling or finishing restores it.
    Edit { item: String, draft: String },
}

#[derive(Default)]
pub struct AppState {
    pub voice: voice::State,
    /// Discord is linked on this device and connected.
    pub discord_online: bool,
    /// The linked Discord account's user id.
    pub discord_user: Option<String>,
    /// Its status as its settings keep it: `online`, `idle`, `dnd` or
    /// `invisible`.
    pub discord_presence: Option<String>,
    /// Its friends, by user id, and friend requests waiting for an answer.
    pub discord_friends: Arc<[String]>,
    /// Those it blocked, by name, and those it ignores, by user id.
    pub discord_blocked: Vec<chat::Blocked>,
    pub discord_ignored: Arc<[String]>,
    /// DMs pinned to the top of the DM list on this device, by room id, the
    /// first pinned first.
    pub pinned: Vec<String>,
    pub person_dialog: Option<PersonDialog>,
    /// The invite dialog of a Discord server or channel, while it's open.
    pub invite: Option<InviteDialog>,
    /// Join a Server, while it's open.
    pub join: Option<JoinDialog>,
    /// The scheduled event a clicked event link opened, while its
    /// dialog is open.
    pub scheduled_event: Option<EventDialog>,
    /// The dialog that makes, changes or deletes a Discord channel or
    /// category, while it's open.
    pub channel_dialog: Option<ChannelDialog>,
    /// Asks before leaving a Discord server or group DM, while it's open.
    pub leave: Option<LeaveDialog>,
    /// Asks whether a channel dragged into a category takes its
    /// permissions, while it's open.
    pub move_sync: Option<MoveSync>,
    /// The editor of this account's Discord profile, its main one or one
    /// server's, while it's open; its pictures go when it closes.
    pub profile_editor: Option<ProfileEditor>,
    /// A Discord server's settings, while they're open: the open page's,
    /// and only while it is.
    pub server_settings: Option<ServerSettings>,
    /// A Discord channel's or category's settings, while they're open.
    pub channel_settings: Option<ChannelSettingsScreen>,
    /// Requests Discord holds back for two-factor authentication, the
    /// first one's prompt showing.
    pub mfa: Vec<MfaPrompt>,
    /// The same friends by name, as the friends list shows them.
    pub friends: Vec<chat::Friend>,
    pub friend_requests: Vec<chat::FriendRequest>,
    /// The Discord account's custom status, while it lasts.
    pub custom_status: Option<chat::CustomStatus>,
    /// Whether the Discord account shares the game it plays, and the game
    /// it shows.
    pub discord_show_game: bool,
    pub discord_playing: Option<chat::Playing>,
    /// How loud the Discord account's soundboard plays, as Discord keeps
    /// it: from 0 to 100, on its slider's curve.
    pub soundboard_volume: f32,
    /// The Discord account's join sounds: each one's server, none for
    /// every server without its own, then its sound's id and its sound's
    /// own server.
    pub join_sounds: Vec<JoinSound>,
    /// Every sound a join sound may be, once Settings asked for them.
    pub sound_catalog: Option<Arc<[voice::SoundGroup]>>,
    /// A Discord account is linked on this device, whether or not it's
    /// connected, and linking one, while that's under way.
    pub discord_linked: bool,
    pub discord_link: Option<DiscordLink>,
    pub add_friend: Option<AddFriend>,
    /// A screen of the DM list open in place of a conversation.
    pub dm_screen: Option<DmScreen>,
    /// Browse Channels, open in place of a server's conversation.
    pub browse: Option<BrowseChannels>,
    /// Whether a one-to-one DM shows its other person's profile beside it.
    pub profile_panel: bool,
    /// That profile, for the open DM.
    pub panel_profile: Option<ProfileView>,
    pub status_editor: Option<StatusEditor>,
    /// Whether Discord channels hidden from this account are listed, locked.
    pub show_hidden_channels: bool,
    /// Reading a Discord channel in Yapfast marks it read on Discord too.
    pub sync_discord_reads: bool,
    /// Whether a new reply notifies the person it replies to, until it's
    /// switched for that reply.
    pub reply_ping: bool,
    /// Tracking parameters come out of the links in sent messages.
    pub strip_tracking: bool,
    /// Messages from people this account blocked don't show at all, rather
    /// than collapsed.
    pub hide_blocked: bool,
    pub switch_voice: Option<String>,
    /// A stream to watch once its call connects and they show sharing in
    /// it, as clicking them in the channel list asks: the voice channel and
    /// their user id. Discord drops a watch made before.
    pub pending_watch: Option<(String, String)>,
    pub account: Option<String>,
    /// The account's name, avatar address and connection status, where the
    /// network gives them; a Discord-only build shows Discord's.
    pub account_name: Option<String>,
    pub account_avatar: Option<String>,
    pub account_status: Option<String>,
    pub preview: bool,
    pub busy: bool,
    pub trusted: bool,
    pub online: bool,
    pub storage: Option<&'static str>,
    pub notice: Option<String>,
    /// Shared with the networks' lists they're shown from.
    pub rooms: Vec<Arc<ChatRoom>>,
    pub servers: Vec<Arc<ChatServer>>,
    /// Who else is typing, by room.
    pub typing: HashMap<String, Vec<String>>,
    /// The `@` word being typed in the composer, if any.
    pub mention_query: Option<String>,
    /// What the selected room's messages may mention besides people: Discord's
    /// roles, @everyone and @here.
    pub mention_groups: Vec<MentionGroup>,
    /// Members of the selected room a search found for mentioning, beyond those
    /// listed.
    pub found_members: Vec<ChatMember>,
    /// Server icons and avatars that arrived, by address; those off screen
    /// go once many have.
    pub pictures: HashMap<String, Arc<Picture>>,
    /// Counts pictures arriving and going, so a client can tell when its own
    /// copies of them are out of date.
    pub picture_changes: u64,
    /// Counts changes to Discord servers' categories, and to the roles that
    /// decide who sees them, so a client can tell when
    /// [`Application::categories`] may list others.
    pub category_changes: u64,
    /// Counts changes to [`Self::rooms`] and [`Self::servers`], so a client
    /// can tell when its own copies of them are out of date. Whatever
    /// changes either counts it.
    pub list_changes: u64,
    pub navigation: Navigation,
    pub selected: Option<String>,
    /// The selected voice channel shows its chat in place of its call.
    pub voice_chat: bool,
    /// A Discord thread shown beside the selected channel.
    pub aside: Option<aside::Aside>,
    pub room_ready: bool,
    pub messages: Arc<Vec<Arc<ChatMessage>>>,
    /// The selected room's joined members, by role and then name; `None` while loading.
    pub members: Option<Result<Vec<ChatMember>, &'static str>>,
    /// A page of earlier history is on its way.
    pub loading_history: bool,
    /// The selected room's first message is loaded, so there's no earlier
    /// history to load.
    pub history_start: bool,
    /// Loading earlier history failed. It loads again only when asked
    /// with [`Action::Earlier`], rather than as the timeline scrolls.
    pub history_failed: bool,
    /// The timeline is a window of older messages, such as those around one
    /// jumped to, that doesn't reach the newest: live messages don't go in
    /// it, and the room counts them as unread. Newer pages load as it
    /// scrolls to its end, and [`Action::JumpToPresent`] leaves it.
    pub detached: bool,
    /// A page of newer messages, after the window, is on its way.
    pub loading_later: bool,
    /// Loading newer messages failed. They load again only when asked with
    /// [`Action::Later`], rather than as the timeline scrolls.
    pub later_failed: bool,
    /// The message the timeline should bring into view and highlight, and a
    /// count that changes with each jump, even to the same message.
    pub jump: Option<(String, u64)>,
    /// The selected room's first unread message as it was on opening the
    /// room, which the timeline draws the "New" divider above. It stays put
    /// while the room is open, as newer messages are read as they come.
    pub first_unread: Option<String>,
    /// The selected room's message that its new ones follow, as
    /// [`chat::first_new`] takes it: the last read on opening the room, or
    /// once its timeline came without that one, the newest it had. A reply
    /// to this account's message is highlighted only while it's new.
    pub seen: Option<String>,
    /// Changes each time the timeline should bring [`Self::first_unread`]
    /// to its top: once when it's found, and on [`Action::JumpToUnread`].
    pub unread_jump: u64,
    /// The newest message of the selected room is in view, with the window
    /// in front, as [`Action::Reading`] says; a Discord channel is read only
    /// then, as Discord's client reads it.
    pub reading: bool,
    /// The newest message was read since the "New" divider was placed, so
    /// the bar that jumps back to it is gone.
    pub caught_up: bool,
    pub sending: Option<String>,
    /// The selected room's message the user is asked to confirm deleting.
    pub confirm_delete: Option<String>,
    /// The message whose poll the user is asked to confirm closing now.
    pub confirm_end_poll: Option<String>,
    /// The poll being made, while its dialog is open.
    pub poll_draft: Option<PollDraft>,
    /// A message being forwarded, while its dialog is open.
    pub forwarding: Option<Forwarding>,
    pub places_used: PlacesUsed,
    /// What the quick switcher searches for, while it's open.
    pub switcher: Option<String>,
    pub new_dm: Option<NewDm>,
    /// The selected DM's pinned messages, a search of it, and adding friends
    /// to it, each while it's open.
    pub pins: Option<Pins>,
    pub search: Option<Search>,
    /// The open Discord channel's list of its threads.
    pub threads: Option<ThreadList>,
    /// The Discord forum on screen.
    pub forum: Option<ForumView>,
    /// The "New Thread" form beside the open Discord channel.
    pub new_thread: Option<NewThread>,
    /// The form for a post in a Discord forum, kept while other rooms show.
    pub new_post: Option<NewPost>,
    /// The reaction the pointer stayed over, by item and key, who the
    /// latest reactions hovered were, and the Reactions window.
    pub hovered_reaction: Option<(String, String)>,
    pub reaction_tips: Vec<ReactionTip>,
    pub reactions: Option<ReactionsWindow>,
    /// The GIF picker, and the account's favourite GIFs once fetched,
    /// counting each change for the stars on messages' GIFs.
    pub gifs: GifPicker,
    pub favorite_gifs: Option<Vec<chat::FavoriteGif>>,
    pub favorite_gifs_changed: u32,
    pub add_people: Option<AddPeople>,
    /// The selected Discord channel's topic, once the backend has said it.
    pub topic: Option<Arc<chat::Topic>>,
    /// The image viewer, open over the selected room.
    pub viewer: Option<Viewer>,
    /// The video playing in the selected room.
    pub video: Option<chat::Video>,
    /// The profile card that's open.
    pub profile: Option<ProfileView>,
    pub role: Option<RoleView>,
    /// Changes when selecting a room, including selecting the same room to retry opening it.
    pub selection_version: u64,
    /// Changes after logout so adapters can clear their inputs and focus sign-in.
    pub reset_version: u64,
    drafts: HashMap<String, String>,
    compose: HashMap<String, Compose>,
    /// Files waiting in each room's composer for the next message.
    staged: HashMap<String, Vec<chat::Staged>>,
}

impl AppState {
    /// The Discord account's status is Do Not Disturb, `dnd`, which silences
    /// its notifications and ringing.
    pub fn do_not_disturb(&self) -> bool {
        self.discord_presence.as_deref() == Some("dnd")
    }

    pub fn selected_room(&self) -> Option<&ChatRoom> {
        self.rooms
            .iter()
            .map(Arc::as_ref)
            .find(|room| Some(&room.id) == self.selected.as_ref())
    }

    /// Whether Yapfast can show the place one of Discord's links names, as
    /// the action it maps to would; the browser opens the rest, such as a
    /// server the account isn't in, or any link while Discord is offline.
    #[cfg(feature = "discord")]
    pub fn opens_discord_route(&self, route: &discord::DiscordRoute) -> bool {
        use discord::DiscordRoute;
        let server = |guild: &str| {
            let id = discord::server_id(guild);
            self.servers.iter().any(|server| server.id == id)
        };
        match route {
            DiscordRoute::Invite { .. } => self.discord_online || self.preview,
            // The event is looked up before it shows.
            DiscordRoute::Event { guild, .. } => {
                (self.discord_online || self.preview) && server(guild)
            }
            DiscordRoute::DirectMessages => self.discord_linked,
            DiscordRoute::Server { guild } => server(guild),
            DiscordRoute::Channel { guild, channel }
            | DiscordRoute::Message { guild, channel, .. } => {
                let room = discord::room_id(guild, channel);
                self.rooms.iter().any(|known| known.id == room)
                    // One not listed may be a thread, which opening asks for.
                    || (self.discord_online
                        && !self.preview
                        && guild != discord::DM_GUILD
                        && server(guild))
            }
            DiscordRoute::User { user } => {
                self.discord_online
                    || self.friends.iter().any(|friend| &friend.id == user)
                    || (self.selected_room().is_some() && self.person(user).is_some())
            }
        }
    }

    pub fn draft(&self) -> &str {
        self.selected
            .as_ref()
            .and_then(|id| self.drafts.get(id))
            .map_or("", String::as_str)
    }

    /// The files waiting in the selected room's composer.
    pub fn staged(&self) -> &[chat::Staged] {
        self.selected
            .as_ref()
            .and_then(|id| self.staged.get(id))
            .map_or(&[], Vec::as_slice)
    }

    fn editing(&self) -> bool {
        matches!(self.compose(), Some(Compose::Edit { .. }))
    }

    pub fn compose(&self) -> Option<&Compose> {
        self.selected.as_ref().and_then(|id| self.compose.get(id))
    }

    /// A message the selected room or the thread beside it shows.
    pub fn message(&self, item: &str) -> Option<&ChatMessage> {
        self.messages
            .iter()
            .chain(self.aside.iter().flat_map(|aside| aside.messages.iter()))
            .find(|message| message.item == item)
            .map(Arc::as_ref)
    }

    /// Whether this account's devices can write to the selected room.
    pub fn can_write(&self) -> bool {
        !self.busy
            && self.room_ready
            && self.shows_text()
            && self.selected_room().is_some_and(|room| self.writable(room))
    }

    /// Whether this device can write to a conversation: an encrypted Matrix
    /// room from a verified device, or a Discord channel that allows it while
    /// Discord is connected.
    pub fn writable(&self, room: &ChatRoom) -> bool {
        if !room.has_text() {
            false
        } else if room.network == Network::Discord {
            self.discord_online && !room.locked && !room.read_only
        } else {
            self.preview || (self.trusted && room.encrypted)
        }
    }

    /// Text channels and group DMs list their members; voice channels and
    /// one-to-one DMs don't.
    pub fn lists_members(&self) -> bool {
        self.selected_room()
            .is_some_and(|room| !room.voice && !room.forum && !(room.direct && room.members <= 2))
    }

    /// The open one-to-one DM, whose other person has a profile.
    pub fn one_to_one(&self) -> Option<&ChatRoom> {
        self.selected_room()
            .filter(|room| room.direct && room.members <= 2 && room.person.is_some())
    }

    /// A dialog that takes the keyboard is open, over which the quick
    /// switcher doesn't open.
    fn dialog_open(&self) -> bool {
        !self.mfa.is_empty()
            || self.forwarding.is_some()
            || self.new_dm.is_some()
            || self.add_friend.is_some()
            || self.status_editor.is_some()
            || self.person_dialog.is_some()
            || self.invite.is_some()
            || self.join.is_some()
            || self.scheduled_event.is_some()
            || self.channel_dialog.is_some()
            || self.leave.is_some()
            || self.move_sync.is_some()
            || self.profile_editor.is_some()
            || self.server_settings.is_some()
            || self.channel_settings.is_some()
            || self.poll_draft.is_some()
            || self.confirm_delete.is_some()
            || self.confirm_end_poll.is_some()
            || self.reactions.is_some()
            || self.add_people.is_some()
            || self.switch_voice.is_some()
            || self.viewer.is_some()
    }

    pub fn in_scope(&self, room: &ChatRoom) -> bool {
        match &self.navigation {
            Navigation::DirectMessages => room.direct,
            Navigation::Server(id) => {
                !room.direct
                    && self
                        .servers
                        .iter()
                        .any(|server| &server.id == id && server.channels.contains(&room.id))
            }
        }
    }

    /// Whether a muted channel stays out of the list, in a server set to
    /// hide them: the open one stays, and so does one that mentions this
    /// account, whose count the server shows on the rail.
    fn hidden_muted(&self, room: &ChatRoom) -> bool {
        room.muted && room.mentions == 0 && self.selected.as_ref() != Some(&room.id)
    }

    /// The rooms [`Self::in_scope`], in order, but for the muted channels a
    /// server hides, and the channels it leaves off a list of those this
    /// account follows, with their threads. It finds the server once, so it
    /// suits going through every room, as [`Self::in_scope`] for each
    /// doesn't with many servers.
    pub fn scoped_rooms(&self) -> impl Iterator<Item = &ChatRoom> {
        let server = match &self.navigation {
            Navigation::DirectMessages => None,
            Navigation::Server(id) => self.servers.iter().find(|server| &server.id == id),
        };
        let channels: Option<HashSet<&str>> = match &self.navigation {
            Navigation::DirectMessages => None,
            Navigation::Server(_) => Some(
                server
                    .map(|server| server.channels.iter().map(String::as_str).collect())
                    .unwrap_or_default(),
            ),
        };
        let hide_muted = server.is_some_and(|server| server.hide_muted);
        // A channel left off the list still shows while it, or a thread of
        // it, is open or mentions this account, as in Discord's client.
        let shown: HashSet<&str> = self
            .rooms
            .iter()
            .filter(|room| {
                room.unlisted && (room.mentions > 0 || self.selected.as_ref() == Some(&room.id))
            })
            .map(|room| room.parent.as_deref().unwrap_or(&room.id))
            .collect();
        self.rooms
            .iter()
            .map(Arc::as_ref)
            .filter(move |room| match &channels {
                None => room.direct,
                Some(channels) => {
                    !room.direct
                        && channels.contains(room.id.as_str())
                        && !(hide_muted && self.hidden_muted(room))
                        && !(room.unlisted
                            && !shown.contains(room.parent.as_deref().unwrap_or(&room.id)))
                }
            })
    }

    /// Who else is typing in the open conversation, as the composer says it; empty
    /// when nobody is.
    pub fn typing_line(&self) -> String {
        let names = self
            .selected
            .as_ref()
            .and_then(|room| self.typing.get(room))
            .map(Vec::as_slice)
            .unwrap_or_default();
        match names {
            [] => String::new(),
            [one] => format!("{one} is typing..."),
            [one, two] => format!("{one} and {two} are typing..."),
            [one, two, three] => format!("{one}, {two} and {three} are typing..."),
            _ => "Several people are typing...".into(),
        }
    }

    /// Someone in the selected room, as far as Yapfast knows them: listed, found by a
    /// search, holding a role on show, writing lately, or only mentioned.
    pub fn person(&self, user: &str) -> Option<ChatMember> {
        let known = self
            .members
            .iter()
            .flatten()
            .flatten()
            .chain(&self.found_members)
            .chain(
                self.role
                    .iter()
                    .flat_map(|role| role.members.iter().flatten()),
            )
            .find(|member| member.id == user);
        if let Some(member) = known {
            return Some(member.clone());
        }
        let sender = |message: &&Arc<ChatMessage>| {
            message.sender == user || discord::sender_user(&message.sender) == Some(user)
        };
        if let Some(message) = self.messages.iter().rev().find(sender) {
            return Some(ChatMember {
                id: user.to_owned(),
                name: message.author.clone(),
                username: None,
                role: Role::Member,
                avatar: message.avatar.clone(),
                group: None,
                presence: None,
                color: message.color,
                activity: None,
                activities: Default::default(),
                roles: Default::default(),
                timed_out_until: None,
            });
        }
        // Someone in a Discord voice channel, as it lists them, the open
        // one's first; their profile fills in the rest.
        if let Some(person) = self
            .selected_room()
            .into_iter()
            .chain(self.rooms.iter().map(Arc::as_ref))
            .flat_map(|room| &room.people)
            .find(|person| person.id == user)
        {
            return Some(ChatMember {
                id: user.to_owned(),
                name: person.name.clone(),
                username: None,
                role: Role::Member,
                avatar: person.avatar.clone(),
                group: None,
                presence: None,
                color: None,
                activity: None,
                activities: Default::default(),
                roles: Default::default(),
                timed_out_until: None,
            });
        }
        let target = Target::User(user.to_owned());
        let mentioned = self
            .messages
            .iter()
            .flat_map(|message| &message.mentions)
            .find(|mention| mention.target == target)
            .map(|mention| mention.text.trim_start_matches('@').to_owned());
        // Matrix links name no one; their id does.
        let name = mentioned.or_else(|| user.starts_with('@').then(|| user.to_owned()))?;
        Some(ChatMember {
            id: user.to_owned(),
            name,
            username: None,
            role: Role::Member,
            avatar: None,
            group: None,
            presence: None,
            color: None,
            activity: None,
            activities: Default::default(),
            roles: Default::default(),
            timed_out_until: None,
        })
    }

    /// Who and what a message in the selected room may mention: its members, those
    /// found by name and those who wrote lately, then its roles and groups. This
    /// account isn't one of them.
    pub fn mention_candidates(&self) -> Vec<Candidate> {
        let Some(room) = self.selected.as_deref() else {
            return vec![];
        };
        let on_discord = Network::of(room) == Network::Discord;
        let mut seen: HashSet<String> = [&self.discord_user, &self.account]
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        let mut candidates = Vec::new();
        let listed = self.members.iter().flatten().flatten();
        let people = listed.chain(&self.found_members).map(|member| {
            (
                member.id.as_str(),
                &member.name,
                member.username.as_ref(),
                &member.avatar,
                member.color,
            )
        });
        let authors = self.messages.iter().map(|message| {
            (
                message.sender.as_str(),
                &message.author,
                None,
                &message.avatar,
                message.color,
            )
        });
        for (id, name, username, avatar, color) in people.chain(authors) {
            let id = discord::sender_user(id).unwrap_or(id);
            if id.is_empty() || name.is_empty() || !seen.insert(id.to_owned()) {
                continue;
            }
            candidates.push(Candidate {
                name: name.clone(),
                // Matrix names people by their user id.
                username: if on_discord {
                    username.cloned()
                } else {
                    Some(id.to_owned())
                },
                mention: if on_discord {
                    format!("<@{id}>")
                } else {
                    let name = name.replace(['[', ']'], "");
                    format!("[{name}](https://matrix.to/#/{id})")
                },
                avatar: avatar.clone(),
                color,
                group: false,
            });
        }
        candidates.extend(self.mention_groups.iter().map(|group| Candidate {
            name: group.name.clone(),
            username: None,
            mention: group.mention.clone(),
            avatar: None,
            color: group.color,
            group: true,
        }));
        candidates
    }

    /// What the `@` word being typed may mean: people, best first, then roles
    /// and groups.
    pub fn mention_suggestions(&self) -> Vec<Candidate> {
        let Some(query) = &self.mention_query else {
            return vec![];
        };
        let (groups, people): (Vec<Candidate>, Vec<Candidate>) = self
            .mention_candidates()
            .into_iter()
            .partition(|candidate| candidate.group);
        let people = mention::matching(&people, query, MENTION_SUGGESTIONS);
        let groups = mention::matching(&groups, query, MENTION_GROUPS);
        people.into_iter().chain(groups).cloned().collect()
    }

    /// Text, or files to go with it; an edit changes only the text.
    pub fn can_send(&self) -> bool {
        self.can_write()
            && self.sending.is_none()
            && (!self.draft().trim().is_empty() || (!self.editing() && !self.staged().is_empty()))
    }

    pub fn can_attach(&self) -> bool {
        self.can_write()
            && !self.preview
            && !self.editing()
            && self.staged().len() < chat::MAX_FILES
    }

    /// Why files dropped on the window can't go with the next message, if
    /// they can't.
    pub fn drop_refusal(&self, paths: &[PathBuf]) -> Option<chat::Refusal> {
        let limit = self.selected_room().map_or(0, |room| room.upload_limit);
        chat::Refusal::of(paths, self.staged().len(), limit)
    }

    /// Where files dropped on the window go, as Discord's client names it:
    /// `#general`, `@Sam` for a DM, `"Plans"` for a thread, and a group DM
    /// or voice channel by its name alone.
    pub fn upload_destination(&self) -> String {
        self.selected_room()
            .map(|room| self.destination(room))
            .unwrap_or_default()
    }

    /// A conversation's name as Discord's client names it where a message
    /// goes: `#general`, `@Sam` for a DM, `"Plans"` for a thread, and a
    /// group DM or voice channel by its name alone.
    pub fn destination(&self, room: &ChatRoom) -> String {
        if room.parent.is_some() {
            format!("\"{}\"", room.name)
        } else if room.direct && room.members <= 2 && room.person.is_some() {
            format!("@{}", room.name)
        } else if room.direct || room.voice {
            room.name.clone()
        } else {
            format!("#{}", room.name)
        }
    }

    /// Whether the next message's files can be marked as spoilers.
    pub fn can_mark_spoilers(&self) -> bool {
        self.selected_room()
            .is_some_and(|room| room.capabilities.spoilers)
    }

    /// Whether a message of room `from` can be forwarded into this
    /// conversation from this device: one of the same network that it may
    /// write in, other than a forum, whose posts start with their own title.
    pub fn can_forward_to(&self, from: &str, room: &ChatRoom) -> bool {
        !self.busy
            && room.network == Network::of(from)
            && room.capabilities.forward
            && !room.forum
            && self.writable(room)
    }

    /// How this Discord account stands with a Discord user.
    pub fn relation(&self, user: &str) -> Relation {
        if self.discord_user.as_deref() == Some(user) {
            Relation::Me
        } else if self.discord_friends.iter().any(|friend| friend == user) {
            Relation::Friend
        } else {
            match self
                .friend_requests
                .iter()
                .find(|request| request.id == user)
            {
                Some(request) if request.incoming => Relation::Incoming,
                Some(_) => Relation::Outgoing,
                None => Relation::Stranger,
            }
        }
    }

    /// Whether this Discord account blocked a Discord user.
    pub fn is_blocked(&self, user: &str) -> bool {
        self.discord_blocked
            .iter()
            .any(|blocked| blocked.id == user)
    }

    /// The selected conversation, when it's a DM or group DM whose network
    /// lets it do what `can` asks, and not a message request.
    pub fn dm_that(
        &self,
        can: impl Fn(&crate::network::Capabilities) -> bool,
    ) -> Option<&ChatRoom> {
        self.selected_room()
            .filter(|room| room.direct && !room.request && can(&room.capabilities))
    }

    /// Where a search from the selected conversation looks: a DM itself,
    /// and a Discord server channel's whole server, as Discord's does.
    pub fn search_place(&self) -> Option<String> {
        let room = self.conversation_that(|can| can.search)?;
        Some(match discord::parse_room(&room.id) {
            Some((guild, _)) if !room.direct => discord::server_id(guild),
            _ => room.id.clone(),
        })
    }

    /// The search showing beside the selected conversation.
    pub fn search_shown(&self) -> Option<&Search> {
        let place = self.search_place()?;
        self.search.as_ref().filter(|search| search.place == place)
    }

    /// The selected conversation, when it shows messages this account may
    /// read, its network lets it do what `can` asks, and it isn't a message
    /// request.
    pub fn conversation_that(
        &self,
        can: impl Fn(&crate::network::Capabilities) -> bool,
    ) -> Option<&ChatRoom> {
        self.selected_room().filter(|room| {
            self.shows_text() && !room.locked && !room.request && can(&room.capabilities)
        })
    }

    /// The selected room shows messages: it isn't a voice room, or it's a
    /// voice channel showing its chat.
    pub fn shows_text(&self) -> bool {
        self.selected_room()
            .is_some_and(|room| !room.voice || self.voice_chat)
    }

    /// Who is in the selected DM besides this account, as far as Yapfast knows:
    /// the other person, or a group's members once they're listed.
    fn dm_people(&self) -> HashSet<&str> {
        let room = self.selected_room();
        room.and_then(|room| room.person.as_deref())
            .into_iter()
            .chain(
                self.members
                    .iter()
                    .flatten()
                    .flatten()
                    .map(|member| member.id.as_str()),
            )
            .collect()
    }

    /// How many more people the friends picker may choose: a group holds
    /// [`GROUP_LIMIT`].
    pub fn group_room(&self) -> usize {
        let members = self.selected_room().map_or(0, |room| room.members.max(2));
        GROUP_LIMIT.saturating_sub(members as usize)
    }

    /// The friends the picker offers: those not in the DM already, whose
    /// names start with its query, or a word of them.
    pub fn addable_friends(&self) -> Vec<&chat::Friend> {
        let Some(picker) = &self.add_people else {
            return vec![];
        };
        let query = picker.query.trim().to_lowercase();
        let present = self.dm_people();
        self.friends
            .iter()
            .filter(|friend| !present.contains(friend.id.as_str()))
            .filter(|friend| {
                let name = friend.name.to_lowercase();
                name.starts_with(&query)
                    || name.split_whitespace().any(|word| word.starts_with(&query))
            })
            .collect()
    }

    pub fn can_join_voice(&self, id: &str) -> bool {
        !matches!(self.voice.phase, Phase::Joining | Phase::Leaving)
            && self.rooms.iter().any(|room| {
                room.id == id
                    // A Discord DM or group DM has a call; a request waits for an answer.
                    && (room.voice
                        || (room.direct && room.capabilities.dm_calls && !room.request))
                    && !room.locked
                    && !room.join_locked
                    && if room.network == Network::Discord {
                        self.discord_online && !self.busy
                    } else {
                        self.preview
                            || (self.trusted && self.online && room.encrypted && !self.busy)
                    }
            })
    }
}

/// How the linked Discord account stands with someone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Me,
    Friend,
    /// They sent this account a friend request.
    Incoming,
    /// This account sent them one.
    Outgoing,
    Stranger,
}

pub enum Action {
    Navigate(Navigation),
    SelectRoom(String),
    /// Mark a listed conversation read, or every conversation of a listed
    /// server, as their menus do.
    MarkRead(String),
    /// Go to wherever a conversation is listed and open it.
    Open(String),
    /// [`Action::Open`], showing the conversation's messages: a voice
    /// channel's chat rather than its call, as a notification from it does.
    OpenChat(String),
    /// Show the selected voice channel's chat, or with `false` its call.
    ShowVoiceChat(bool),
    /// Show a Discord thread beside the open channel, as its link in a
    /// message does; anything else opens as [`Action::Open`] does.
    OpenAside(String),
    CloseAside,
    /// Show the thread beside the open channel in place of it.
    ExpandAside,
    /// Load the thread beside's earlier messages.
    AsideEarlier,
    SetAsideDraft(String),
    SendAside,
    /// Leave reply or edit mode in the thread beside.
    CancelAsideCompose,
    /// Add files to the thread beside's next message.
    StageAside(Vec<PathBuf>),
    /// Take the file at this index out of the thread beside's next message.
    UnstageAside(usize),
    /// Mark the file at this index of the thread beside's next message as a
    /// spoiler, or not.
    ToggleAsideSpoiler(usize),
    SetDraft(String),
    /// The `@` word being typed in the composer changed, or there's none now.
    MentionQuery(Option<String>),
    /// Open the dialog for starting a DM, look for people in it, start one, or close it.
    OpenNewDm,
    /// Edit this account's newest message that can be edited, as Up in an empty
    /// composer does.
    EditLast,
    NewDmQuery(String),
    StartDm(Contact),
    CloseNewDm,
    Send,
    Reply(String),
    Edit(String),
    CancelCompose,
    React {
        item: String,
        key: String,
    },
    RequestDelete(String),
    /// Vote for these answers of a Discord message's poll, by item, in
    /// place of this account's vote; none takes it back.
    Vote {
        item: String,
        answers: Vec<u64>,
    },
    /// Open the "Create a Poll" dialog for the open Discord conversation.
    OpenPollDraft,
    EditPoll(PollEdit),
    PostPoll,
    ClosePollDraft,
    /// Ask to make sure before closing this account's poll, by item.
    AskEndPoll(String),
    CancelEndPoll,
    /// Close the poll asked about now.
    EndPoll,
    /// Open the Reactions window on who voted in a message's poll, by
    /// item: for one answer, or the first.
    ShowVoters {
        item: String,
        answer: Option<u64>,
    },
    /// Pin a message of the selected room to its pinned messages, or
    /// unpin it if it's there.
    PinMessage(String),
    /// Open the forward dialog for a message of the selected room.
    StartForward(String),
    /// Choose a conversation to forward that message to, or leave it out.
    ChooseForward(String),
    /// Search the conversations it may go to.
    SearchForward(String),
    /// Forward it to those chosen, with this message added.
    Forward(String),
    CloseForward,
    /// Open the quick switcher, or close it if it's open.
    ToggleSwitcher,
    /// Search the places it goes to.
    SearchSwitcher(String),
    /// Go to a conversation or server it found, and close it.
    SwitchTo(String),
    CloseSwitcher,
    ConfirmDelete,
    CancelDelete,
    Retry(String),
    CancelSend(String),
    /// Add files to the selected room's next message.
    Stage(Vec<PathBuf>),
    /// Take the file at this index out of the next message.
    Unstage(usize),
    /// Mark the file at this index of the next message as a spoiler, or not.
    ToggleSpoiler(usize),
    /// Save the attachment at `index` of an item.
    SaveAttachment {
        item: String,
        index: usize,
        path: PathBuf,
    },
    /// Copy the picture of the attachment at `index` of an item.
    CopyAttachment {
        item: String,
        index: usize,
    },
    /// Show an item's image attachment large.
    ViewImage {
        item: String,
        index: usize,
    },
    /// Play an item's video attachment where it shows, at most `width` by
    /// `height` pixels.
    PlayVideo {
        item: String,
        index: usize,
        width: u32,
        height: u32,
    },
    /// Control the video playing.
    Video(chat::VideoCommand),
    /// Show the message's next image, or with -1 its previous one, wrapping around.
    StepImage(isize),
    CloseImage,
    DismissNotice,
    /// Load the page of history before the selected room's oldest loaded
    /// message, as the timeline does when scrolled near its top, unless one
    /// is on its way or the first message is loaded.
    Earlier,
    /// Items in or near the selected timeline's viewport, in timeline order,
    /// and whether it follows the newest message.
    VisibleMessages {
        room: String,
        items: Vec<String>,
        at_end: bool,
    },
    /// Show the selected room's message with this item; see
    /// [`AppState::jump`]. When the timeline doesn't have it, the messages
    /// around it load in its place; see [`AppState::detached`].
    Jump(String),
    /// Open a room, unless it's open, and show its message with this item,
    /// as [`Action::Jump`] does.
    JumpInRoom {
        room: String,
        item: String,
    },
    /// Show the selected room's first unread message; see
    /// [`AppState::first_unread`].
    JumpToUnread,
    /// Whether the newest message of the selected room is in view, with the
    /// window in front and no jump settling; see [`AppState::reading`].
    Reading(bool),
    /// Load the page after the newest message of a window of older ones,
    /// as the timeline does when scrolled near its end, unless one is on
    /// its way.
    Later,
    /// Show the selected room's newest messages again in place of a window
    /// of older ones.
    JumpToPresent,
    /// Download pictures that show outside the state, such as the emoji
    /// picker's; see [`DiscordCommand::Pictures`].
    LoadPictures(Vec<String>),
    Login {
        homeserver: String,
        username: String,
        password: String,
    },
    Restore,
    Recover(String),
    RefreshTrust,
    RetrySync,
    Logout,
    Preview,
    RequestJoinVoice(String),
    /// Answer a Discord DM call ringing this account: open the conversation
    /// and join, leaving any other call, or decline it.
    AcceptCall(String),
    DeclineCall(String),
    /// Send a Discord user a friend request, or accept theirs.
    Befriend(String),
    /// Show a screen of the DM list in place of the conversation.
    OpenDmScreen(DmScreen),
    /// Show Browse Channels for the Discord server navigated to in place
    /// of its conversation.
    BrowseChannels,
    /// Tick a channel's box in Browse Channels, by its room id, or a
    /// category's switch, or clear it with `follow` false.
    BrowseFollow {
        room: String,
        follow: bool,
    },
    /// Open the DM with someone, by Discord user id or Matrix ID, starting
    /// one with a Discord friend or a Matrix user if there's none.
    MessagePerson(String),
    /// Send someone a message from their profile card, in their DM, which
    /// opens, or starts as [`Action::MessagePerson`] starts one.
    MessageFromProfile {
        user: String,
        text: String,
    },
    /// Show or hide the other person's profile beside a one-to-one DM.
    ShowProfilePanel(bool),
    /// Ignore a Discord user's friend request, or cancel this account's.
    RemoveFriendRequest(String),
    /// Pin a DM to the top of the DM list, or unpin it.
    Pin {
        room: String,
        pinned: bool,
    },
    /// The DMs pinned on this device, as it remembers them.
    SetPinned(Vec<String>),
    /// Close a Discord DM; it comes back when either side writes.
    CloseDm(String),
    /// Show the selected DM's pinned messages, or stop.
    ShowPins(bool),
    /// The pointer went over a message's reaction, by item and key, or left
    /// it.
    HoverReaction(Option<(String, String)>),
    /// Open the GIF picker, or close it.
    OpenGifs,
    CloseGifs,
    /// The GIF picker's search field, after a pause in typing.
    SearchGifs(String),
    /// Show one of the GIF picker's views.
    ShowGifs(GifView),
    /// Search for a trending term from the GIF picker's home.
    SearchGifCategory(String),
    /// The pictures of the GIFs on screen or about to be, with the size to
    /// decode each at, and whether the list is scrolled near its end.
    GifsShown {
        shown: Vec<(String, u32, u32)>,
        near_end: bool,
    },
    /// Send a GIF, by its page, or with `insert` put it in the composer.
    PickGif {
        url: String,
        insert: bool,
    },
    /// Add a GIF to the favourites, by its page, or take it out.
    FavoriteGif(String),
    /// The same for a message's GIF, by item and attachment.
    FavoriteMessageGif {
        item: String,
        index: usize,
    },
    /// Load the favourite GIFs, if they haven't, as a star is about to be
    /// used.
    LoadFavoriteGifs,
    /// Open a message's Reactions window, by item, on the reaction `key`,
    /// or the one most reacted with.
    ShowReactions {
        item: String,
        key: Option<String>,
    },
    CloseReactions,
    /// Show who reacted with another of the window's reactions.
    ShowReactors(String),
    /// Load the window's next page of people.
    MoreReactors,
    /// Remove someone's reaction in the window, by user id: this
    /// account's, or as a moderator anyone's.
    RemoveReactor(String),
    /// Show the open Discord channel's list of its threads, or close it.
    ShowThreads(bool),
    /// Search the list's threads by name; an empty query shows the list.
    SearchThreads(String),
    /// Load the list's next page of archived threads.
    MoreThreads,
    /// Search the forum's posts by title; an empty query shows them all.
    SearchPosts(String),
    /// The forum's list of posts scrolled near its end: load more of them.
    MorePosts,
    /// Open the form for a post in the forum on screen.
    NewPost,
    /// Write the form's title or message.
    WriteNewPost {
        title: Option<String>,
        body: Option<String>,
    },
    /// Give the post a tag, by id, or take it away.
    TogglePostTag(String),
    /// Make the post the form describes.
    CreatePost,
    /// Close the form, asking first if something was written; close it
    /// anyway; or go back to it.
    CloseNewPost,
    LeaveNewPost,
    StayInNewPost,
    /// Open the "New Thread" form beside the open channel: from a message,
    /// by item, or from none.
    NewThread {
        message: Option<String>,
        from: ThreadFrom,
    },
    /// Write the form's name or first message, or choose a private thread.
    WriteNewThread {
        name: Option<String>,
        body: Option<String>,
    },
    SetThreadPrivate(bool),
    /// Start the thread the form describes.
    CreateThread,
    /// Close the form, asking first if something was written; close it
    /// anyway; or go back to it.
    CloseNewThread,
    LeaveNewThread,
    StayInNewThread,
    /// Search the selected DM's messages, or a server channel's server;
    /// an empty query ends the search.
    Search(String),
    /// Load the next page of the search's results.
    MoreResults,
    /// Open the picker that adds friends to the selected DM, look among
    /// them, choose a friend or unchoose them, add those chosen, or close it.
    OpenAddPeople,
    AddPeopleQuery(String),
    ChoosePerson(String),
    AddPeople,
    CloseAddPeople,
    /// Mute a Discord DM, or unmute it.
    MuteDm {
        room: String,
        muting: chat::Muting,
    },
    /// Change this account's notification settings for a Discord server, by
    /// its server id, or for a channel or category in it, by its room id.
    ChangeNotifications {
        place: String,
        change: chat::NotificationChange,
    },
    /// Pin a Discord server's channel, by its room id, to the top of its
    /// channel list, or unpin it with `pinned` false.
    PinChannel {
        room: String,
        pinned: bool,
    },
    /// Move a Discord server's channel or category, by its room id, where
    /// dropping it in the channel list lands it; see
    /// [`Application::landing`].
    MoveChannel {
        room: String,
        landing: Landing,
    },
    /// Answer whether a channel dragged into a category takes its
    /// permissions: with `Some(sync)` it moves, and with none it stays.
    AnswerMoveSync(Option<bool>),
    /// Add a Discord server's channel or category, by its room id, to the
    /// channel list that shows only those this account follows, or remove
    /// it with `follow` false.
    FollowChannel {
        room: String,
        follow: bool,
    },
    /// Show every channel of a Discord server, by its server id, or with
    /// `show` false only those this account follows.
    ShowAllChannels {
        server: String,
        show: bool,
    },
    /// Join a Discord thread, by its room id, or leave it with `join` false,
    /// from the menu Discord's client names `location`.
    JoinThread {
        room: String,
        join: bool,
        location: &'static str,
    },
    /// Open the invite dialog of a Discord server, by its server id, or of
    /// a channel, by its room id, which makes a link at once; make another
    /// that lasts `max_age` seconds, 0 for good, and works `max_uses`
    /// times, 0 for any number; or close it.
    OpenInvite(String),
    MakeInvite {
        max_age: u32,
        max_uses: u32,
    },
    CloseInvite,
    /// Send a Discord user an invite to a server, by its server id, from
    /// their menu's Invite to server submenu.
    InviteToServer {
        user: String,
        server: String,
    },
    /// Open Join a Server; look up the invite in what was typed or pasted
    /// into it; open it for a clicked invite link, by its code; join by
    /// the invite it shows, or open the server for a member; open the
    /// captcha Discord asked for again; open the server's form in the
    /// browser; or close it.
    OpenJoinServer,
    FindInvite(String),
    OpenInviteLink(String),
    JoinServer,
    OpenJoinCaptcha,
    OpenJoinForm,
    CloseJoin,
    /// Open a clicked scheduled-event link, by its server and event
    /// ids; mark this account interested in the event it shows, or no
    /// longer interested; or close its dialog.
    OpenEventLink {
        guild: String,
        event: String,
    },
    ToggleEventInterest,
    CloseEvent,
    /// Open the dialog that makes a channel of this kind, from a Discord
    /// server's menu, by its server id, or one like a channel in its
    /// category, from the channel's menu, by its room id.
    NewChannel {
        place: String,
        kind: chat::ChannelKind,
    },
    /// Open a Discord channel's or category's dialog from its menu, which
    /// copies or deletes it; carry it out with what was written, making one
    /// of `kind` where the dialog chooses; or close it.
    AskChannel(ChannelAsk),
    /// Open a Discord channel's or category's settings, by its room id;
    /// what they were given.
    OpenChannelSettings(String),
    ChannelSettings(ChannelInput),
    SaveChannel {
        kind: chat::ChannelKind,
        settings: chat::ChannelSettings,
    },
    CloseChannelDialog,
    /// Ask before leaving a Discord server, by its server id, or a group
    /// DM, by its room id; tick or untick leaving the group without telling
    /// its other members; leave the one asked about; or stay.
    AskLeaveServer(String),
    AskLeaveGroup(String),
    ToggleSilentLeave,
    Leave,
    CloseLeave,
    /// Open the editor of this account's main Discord profile, or of its
    /// profile in a Discord server, by its server id or one of its
    /// channels' room ids; change a text in it; read a picture chosen for
    /// its avatar or banner; go back to the fallback's; save what changed;
    /// open Discord's Nitro page, which some fields take; or close it.
    EditProfile,
    EditServerProfile(String),
    WriteProfile {
        field: ProfileText,
        text: String,
    },
    PickProfilePicture {
        kind: chat::ProfilePicture,
        path: PathBuf,
    },
    ResetProfilePicture(chat::ProfilePicture),
    /// Undo every change in the profile editor, as its bar's Reset does.
    UndoProfileEdits,
    SaveProfile,
    UnlockNitro,
    CloseProfileEditor,
    /// Open a Discord server's settings, by its server id or one of its
    /// channels' room ids; go to another page; change a text of the page,
    /// read a picture chosen for the server or remove one; undo or save
    /// what changed, as the bar's Reset and Save Changes do; or close
    /// them. Leaving a page with unsaved changes shakes the bar instead.
    OpenServerSettings(String),
    ServerSettingsSection(chat::SettingsSection),
    WriteServerSetting {
        field: SettingsText,
        text: String,
    },
    ToggleServerSetting(SettingsToggle),
    ChooseServerSetting(SettingsChoice),
    PickServerPicture {
        kind: chat::ServerPicture,
        path: PathBuf,
    },
    RemoveServerPicture(chat::ServerPicture),
    UndoServerSettings,
    SaveServerSettings,
    CloseServerSettings,
    /// Something done on Roles in a server's settings.
    Roles(RoleInput),
    /// Something done on Emoji in a server's settings.
    Emoji(EmojiInput),
    /// Something done on Stickers in a server's settings.
    Stickers(StickerInput),
    /// Something done on Members in a server's settings.
    Members(MemberInput),
    /// Something done in Delete Server's or Transfer Ownership's dialog.
    Ownership(OwnershipInput),
    /// Something done on Invites in a server's settings.
    Invites(InviteInput),
    /// Something done on Bans in a server's settings.
    Bans(BanInput),
    /// Something done on Audit Log in a server's settings.
    AuditLog(AuditInput),
    /// Answer the two-factor prompt: use another method, or with none
    /// list them; write the code; have Discord text it again; confirm; or
    /// cancel, which fails the request waiting on it.
    ChooseMfa(Option<chat::MfaMethod>),
    WriteMfa(String),
    ResendMfa,
    ConfirmMfa,
    CancelMfa,
    /// Change how the Discord account stands with someone. Unfriending and
    /// blocking ask first, through [`Action::AskPerson`].
    ChangeRelationship {
        user: String,
        change: chat::RelationshipChange,
    },
    /// Open a dialog about a Discord user, from their DM's menu; carry out
    /// what it asks for, with the note or nickname written; or close it.
    AskPerson {
        user: String,
        ask: PersonAsk,
    },
    AnswerPerson(String),
    ClosePersonDialog,
    // A server member's menu; see `members.rs`.
    /// Put a mention of someone in the open channel's composer.
    MentionPerson(String),
    /// Open the DM with someone and start its call, starting the DM first
    /// with a friend Yapfast has none with.
    CallPerson(String),
    /// Open a dialog about a member of a Discord server, by its server id
    /// or one of its channels' room ids, when this account may. With
    /// [`PersonAsk::ServerNickname`] and this account's own id it changes
    /// its own nickname there.
    AskMember {
        place: String,
        user: String,
        ask: PersonAsk,
    },
    /// Choose a timeout's length or how much a ban deletes, by its index in
    /// [`TIMEOUT_TIMES`] or [`BAN_HISTORY`].
    ChoosePersonOption(usize),
    /// Give a member a role or take it away, or end their timeout, when this
    /// account may; the rest of [`chat::MemberChange`] goes through
    /// [`Action::AskMember`].
    ChangeMember {
        place: String,
        user: String,
        change: chat::MemberChange,
    },
    /// Open the dialog that adds a friend by Discord username, send the
    /// request, or close it.
    OpenAddFriend,
    AddFriend(String),
    CloseAddFriend,
    /// Link a Discord account by QR code, or start again with a new code;
    /// link one with a pasted token; open the captcha Discord asked for
    /// again; stop linking; or forget the linked account on this device.
    LinkDiscord,
    LinkDiscordToken(String),
    OpenDiscordCaptcha,
    CancelDiscordLink,
    UnlinkDiscord,
    /// Open the custom status dialog, set the status, clear it, or close it.
    /// The emoji is a Unicode one or a custom one's `:name:`, which needs Nitro.
    OpenStatusEditor,
    SetCustomStatus {
        emoji: Option<String>,
        text: String,
        clear_after: ClearAfter,
    },
    ClearCustomStatus,
    CloseStatusEditor,
    /// Set the Discord status: `online`, `idle`, `dnd` or `invisible`.
    SetStatus(String),
    /// Share the game the Discord account plays, or stop.
    ShowGame(bool),
    /// Set how loud the Discord account's soundboard plays, on its slider
    /// from 0 to 100.
    SoundboardVolume(f32),
    /// List the sounds the Discord account may join calls with.
    LoadSoundCatalog,
    /// Set the Discord account's join sound for `server`, or for every
    /// server, to the sound with this id, or take it away.
    SetJoinSound {
        server: Option<String>,
        sound: Option<String>,
    },
    /// Nobody has used Yapfast since then, in Unix milliseconds, or someone is
    /// using it again; and whether that may show Discord idle.
    Inactive {
        since: Option<u64>,
        show_idle: bool,
    },
    ConfirmVoiceSwitch,
    CancelVoiceSwitch,
    /// Someone in a voice channel was clicked in the channel list, by
    /// user id: watch their stream, joining the call first, which asks if
    /// it isn't joined yet; or, with only a camera on, show the call.
    ShowVoicePerson {
        room: String,
        user: String,
    },
    DismissVoiceError,
    Voice(voice::Command),
    /// Turn the microphone on as a call connects, as a preference asks; quietly,
    /// since joining already made a sound.
    UnmuteOnJoin,
    /// Deafen as a call connects, as a preference asks; quietly too.
    DeafenOnJoin,
    /// List Discord channels this account can't see, locked.
    ShowHiddenChannels(bool),
    /// Whether new replies notify the person they reply to.
    ReplyPing(bool),
    /// Switch whether the reply being written notifies its author: in the
    /// thread beside's composer, or the selected room's.
    ToggleReplyPing {
        aside: bool,
    },
    /// Take tracking parameters out of links in sent messages.
    StripTracking(bool),
    /// Leave out messages from people this account blocked, rather than
    /// collapsing them.
    HideBlocked(bool),
    /// Mark Discord channels read on Discord when reading them in Yapfast.
    SyncDiscordReads(bool),
    /// Accept the selected Discord message request, or ignore it.
    AnswerRequest(bool),
    /// Show the profile of a member of the selected room, as a card beside
    /// the member list or large.
    ShowProfile {
        user: String,
        large: bool,
    },
    /// Show the profile of the other person in a one-to-one DM, large.
    ShowDmProfile(String),
    /// Show the profile of a friend a profile lists as mutual, by Discord
    /// user id: as a card, or large.
    ShowFriendProfile {
        user: String,
        large: bool,
    },
    /// Show the open profile card large.
    EnlargeProfile,
    CloseProfile,
    /// Show the members of a role mentioned in a message, or stop.
    ShowRole(String),
    CloseRole,
}

/// An adapter executes these effects with the shared backend, then applies its events.
pub enum Effect {
    Start(chat::Start),
    Backend(Command),
    /// Notify the person of a new message, if they aren't looking at Yapfast.
    Notify(chat::Alert),
    /// Play one of Yapfast's sounds, as the person set them.
    Sound(crate::sounds::Sound),
    /// List the system's audio devices for the preview, then report them
    /// through [`Application::preview_devices`].
    ListDevices,
    /// Link a Discord account by QR code, with `crate::discord_link::run`,
    /// reporting through `Application::linking`; this replaces any linking
    /// under way. The same with a pasted token. Stop linking.
    LinkDiscord,
    LinkDiscordToken(String),
    StopDiscordLink,
    /// Open this web page in the browser.
    OpenPage(String),
    /// Put this picture, a file as it was downloaded, on the clipboard.
    CopyPicture(Vec<u8>),
    /// Film for a preview call with the camera with this id, or the first,
    /// into this feed; or stop filming.
    PreviewCamera(Option<(Option<String>, crate::picture::Feed)>),
}

/// A timeline as the app state holds it, sharing its messages.
type TimelineArc = Arc<Vec<Arc<ChatMessage>>>;

#[derive(Default)]
pub struct Application {
    pub state: AppState,
    /// The lists from Matrix; `state` shows them with Discord's.
    matrix_rooms: Vec<Arc<ChatRoom>>,
    matrix_servers: Vec<Arc<ChatServer>>,
    #[cfg(feature = "discord")]
    discord: Option<Arc<yapfast_discord::service::Snapshot>>,
    /// Discord's lists, made from `discord`.
    #[cfg(feature = "discord")]
    discord_index: discord::Index,
    last_selected: HashMap<Navigation, String>,
    samples: HashMap<String, Vec<Arc<ChatMessage>>>,
    /// How often this device added each reaction.
    reaction_uses: HashMap<String, u32>,
    /// Pictures asked for once; they aren't asked for again.
    requested_pictures: HashSet<String>,
    /// Forwards sent, until each conversation they went to answers.
    forwards: Vec<ForwardSent>,
    /// The selected room's timeline with the blocked messages
    /// [`AppState::hide_blocked`] left out, to show them again.
    /// Kept with the timeline shown from it, so it's used only while that
    /// still shows.
    with_blocked: Option<(TimelineArc, TimelineArc)>,
    /// The timeline whose pictures were asked for, so an unchanged one isn't
    /// gone through again.
    pictured_timeline: Arc<Vec<Arc<ChatMessage>>>,
    visible_messages: Vec<String>,
    /// How many pictures other than custom emoji were kept the last time
    /// those off screen went.
    pictures_kept: usize,
    /// How many of the pictures kept are custom emoji, which stay.
    emoji_pictures: usize,
    /// Names searched for in the selected room, lowercased.
    searched: HashSet<String>,
    /// A conversation to open once it's listed, such as a DM just started.
    pending_open: Option<String>,
    /// A thread to show beside the selected channel once it's listed.
    pending_aside: Option<String>,
    /// A thread just started, whose first message, its draft, goes once
    /// it shows beside its channel.
    first_message: Option<String>,
    /// The items in or near the thread beside's view, as last reported.
    aside_visible: Vec<String>,
    /// The thread beside's timeline as its pictures were last asked for.
    pictured_aside: Arc<Vec<Arc<ChatMessage>>>,
    /// A DM with a friend is starting, to open once it has.
    starting_dm: bool,
    /// Its call starts once it opens, as [`Action::CallPerson`] asks.
    pending_call: bool,
    /// What [`Action::MessageFromProfile`] sends once its DM opens.
    pending_message: Option<PendingMessage>,
    /// The composer text of the message being sent, which `Event::SendResult` clears
    /// once sent; the backend has it with its mentions in the network's form.
    sent_draft: Option<String>,
    /// The files sent with it, which leave the composer once sent.
    sent_files: Vec<PathBuf>,
    /// The message [`Action::Jump`] or [`Action::JumpToUnread`] brings into
    /// view once the timeline has it.
    seeking: Option<Seeking>,
    /// The messages around one were asked for. Until they come, the
    /// selected room's timelines are of what showed before, and are left.
    focusing: bool,
    /// The last message read in the selected room when it was opened, while
    /// the first message after it is still to be found, and whether the
    /// messages around it were asked for.
    read_marker: Option<(String, bool)>,
    /// [`AppState::seen`] is still to be checked against the selected room's
    /// timeline, once it comes.
    placing_seen: bool,
    /// Discord channels shown lately, newest last: their timeline and
    /// member list as they were left, shown again at once on going back
    /// while the backend sends them afresh.
    shown: Vec<Shown>,
}

/// A message to someone, held until their DM is open and ready to send.
struct PendingMessage {
    user: String,
    text: String,
    /// Their DM showed; the message is dropped if something else shows
    /// before it sends.
    shown: bool,
}

struct Seeking {
    item: String,
    /// Bring it in view as the first unread message, without highlighting it.
    unread: bool,
}

struct Shown {
    room: String,
    messages: Arc<Vec<Arc<ChatMessage>>>,
    members: Option<Result<Vec<ChatMember>, &'static str>>,
}

/// Discord channels whose last view is kept, as many as the Discord side
/// keeps loaded.
const SHOWN: usize = discord::KEPT_CHANNELS;
/// DM pictures downloaded for the list. An account can have hundreds of
/// DMs, and the ones further down keep their initials.
const LISTED_PICTURES: usize = 150;
/// Server icons and avatars kept beyond those on screen. One is 64 KB
/// decoded, and the window keeps a copy of each it shows. The rail, the DMs
/// listed, a member list and a timeline show a few hundred at most.
const KEPT_PICTURES: usize = 400;
/// Timeline rows either side of those on screen whose avatars stay when
/// pictures go.
const PICTURED_AROUND: usize = 30;

/// Where the messages on screen are in `timeline`: from the first to the
/// last of `visible`, the items the timeline said show; none before it said,
/// or once they've gone.
fn visible_range(
    timeline: &[Arc<ChatMessage>],
    visible: &[String],
) -> Option<std::ops::RangeInclusive<usize>> {
    let (first, last) = visible.first().zip(visible.last())?;
    let first = timeline.iter().position(|message| &message.item == first)?;
    let last = timeline.iter().position(|message| &message.item == last)?;
    (first <= last).then_some(first..=last)
}

/// The server icons and avatars on screen: the rail, the first DMs listed,
/// the people in the listed voice channels, the call, `timeline` and the
/// member list, profiles and people to start a DM with.
fn pictures_on_screen<'a>(
    state: &'a AppState,
    timeline: &'a [Arc<ChatMessage>],
    list_avatars: bool,
) -> impl Iterator<Item = &'a str> {
    state
        .servers
        .iter()
        .filter(move |_| list_avatars)
        .filter_map(|server| server.icon.as_deref())
        .chain(state.account_avatar.as_deref())
        .chain(
            state
                .scoped_rooms()
                .filter(move |_| list_avatars)
                .filter_map(|room| room.avatar.as_deref())
                .take(LISTED_PICTURES),
        )
        .chain(
            state
                .selected_room()
                .and_then(|room| room.avatar.as_deref()),
        )
        .chain(
            state
                .friend_requests
                .iter()
                .filter(move |_| list_avatars)
                .filter(|_| state.navigation == Navigation::DirectMessages)
                .filter_map(|request| request.avatar.as_deref()),
        )
        .chain(
            state
                .friends
                .iter()
                .filter(move |_| list_avatars)
                .filter(|_| state.dm_screen == Some(DmScreen::Friends))
                .filter_map(|friend| friend.avatar.as_deref()),
        )
        .chain(state.server_settings.iter().flat_map(|settings| {
            let (first, second) = match &settings.page {
                Some(server_settings::Page::Profile(page)) => (page.original.icon.as_deref(), None),
                Some(server_settings::Page::BoostPerks(page)) => (
                    page.original.banner.as_deref(),
                    page.original.splash.as_deref(),
                ),
                Some(
                    server_settings::Page::Engagement(_)
                    | server_settings::Page::Emoji(_)
                    | server_settings::Page::Stickers(_)
                    | server_settings::Page::Members(_)
                    | server_settings::Page::Roles(_)
                    | server_settings::Page::Invites(_)
                    | server_settings::Page::Safety(_)
                    | server_settings::Page::AuditLog(_)
                    | server_settings::Page::Bans(_),
                )
                | None => (None, None),
            };
            // The page of Bans shown.
            let banned = match &settings.page {
                Some(server_settings::Page::Bans(page)) => page.on_page(),
                _ => vec![],
            }
            .into_iter()
            .map(|ban| ban.user.avatar.as_str());
            // The page of Members shown, and Invites' inviters.
            let listed = match &settings.page {
                Some(server_settings::Page::Members(page)) => page.on_page(),
                _ => &[],
            }
            .iter()
            .map(|member| member.person.avatar.as_str());
            let inviters = match &settings.page {
                Some(server_settings::Page::Invites(page)) => page.invites.as_slice(),
                _ => &[],
            }
            .iter()
            .filter_map(|invite| invite.inviter.as_ref())
            .map(|inviter| inviter.avatar.as_str());
            // Audit Log's moderators, beside its rows and in its user
            // filter.
            let logged = match &settings.page {
                Some(server_settings::Page::AuditLog(page)) => Some(page),
                _ => None,
            };
            let moderators = logged
                .into_iter()
                .flat_map(|page| page.actors.iter().map(|(_, user)| user.avatar.as_str()))
                .filter(|avatar| !avatar.is_empty());
            // Emoji and their uploaders; stickers and theirs.
            let emojis = match &settings.page {
                Some(server_settings::Page::Emoji(page)) => page.emojis.as_slice(),
                _ => &[],
            }
            .iter()
            .flat_map(|emoji| {
                let uploader = emoji.user.as_ref().map(|user| user.avatar.as_str());
                [Some(emoji.picture.as_str()), uploader]
            })
            .flatten()
            .filter(|url| !url.is_empty());
            let stickers = match &settings.page {
                Some(server_settings::Page::Stickers(page)) => page.stickers.as_slice(),
                _ => &[],
            }
            .iter()
            .flat_map(|sticker| {
                let uploader = sticker.user.as_ref().map(|user| user.avatar.as_str());
                [sticker.picture.as_deref(), uploader]
            })
            .flatten()
            .filter(|url| !url.is_empty());
            // Roles' icons, the open role's members and Add Members' finds.
            let roles = match &settings.page {
                Some(server_settings::Page::Roles(page)) => Some(page),
                _ => None,
            };
            let icons = roles
                .into_iter()
                .flat_map(|page| page.original.roles.iter())
                .filter_map(|role| role.icon.as_deref());
            let members = roles
                .into_iter()
                .flat_map(|page| page.shown_members())
                .map(|member| member.avatar.as_str());
            let found = roles
                .into_iter()
                .flat_map(|page| page.adding.iter())
                .flat_map(|adding| adding.found.iter().chain(&adding.chosen))
                .map(|member| member.avatar.as_str());
            first
                .into_iter()
                .chain(second)
                .chain(icons)
                .chain(emojis)
                .chain(stickers)
                .chain(members)
                .chain(found)
                .chain(banned)
                .chain(listed)
                .chain(inviters)
                .chain(moderators)
        }))
        .chain(state.profile_editor.iter().flat_map(|editor| {
            // The profile's pictures and its fallback's, which show where
            // the profile's are reset.
            editor.profile.iter().flat_map(|profile| {
                [
                    &profile.avatar,
                    &profile.banner,
                    &profile.fallback.avatar,
                    &profile.fallback.banner,
                ]
                .into_iter()
                .filter_map(Option::as_deref)
            })
        }))
        .chain(
            state
                .profile
                .iter()
                .chain(&state.panel_profile)
                .flat_map(|view| {
                    let profile = &view.profile;
                    [&profile.avatar, &profile.banner]
                        .into_iter()
                        .chain(
                            profile
                                .servers
                                .iter()
                                .flat_map(|server| [&server.icon, &server.banner]),
                        )
                        .chain(profile.friends.iter().map(|friend| &friend.avatar))
                        .filter_map(Option::as_deref)
                }),
        )
        .chain(
            state
                .scoped_rooms()
                .flat_map(|room| &room.people)
                .filter_map(|person| person.avatar.as_deref()),
        )
        .chain(
            state
                .voice
                .participants
                .iter()
                .filter_map(|person| person.avatar.as_deref()),
        )
        .chain(
            timeline
                .iter()
                .filter(move |_| list_avatars)
                .flat_map(|message| {
                    let reply = message
                        .reply
                        .as_ref()
                        .and_then(|reply| reply.avatar.as_deref());
                    message.avatar.as_deref().into_iter().chain(reply)
                })
                .chain(
                    timeline.iter().flat_map(|message| {
                        message.emoji.iter().map(|emoji| emoji.picture.as_str())
                    }),
                ),
        )
        .chain(
            state
                .members
                .iter()
                .filter(move |_| list_avatars)
                .flatten()
                .flatten()
                .chain(&state.found_members)
                .chain(
                    state
                        .role
                        .iter()
                        .filter(move |_| list_avatars)
                        .flat_map(|role| role.members.iter().flatten()),
                )
                .filter_map(|member| member.avatar.as_deref()),
        )
        .chain(
            state
                .new_dm
                .iter()
                .flat_map(NewDm::people)
                .filter_map(|person| person.avatar.as_deref()),
        )
}

/// Discord's CDN keeps custom emoji under /emojis/.
fn is_custom_emoji(url: &str) -> bool {
    url.contains("/emojis/")
}

/// A closed channel's recent message metadata, ready to show on reopening.
/// Preview ownership stays in the backend's shared byte-bounded cache.
/// Rows without previews stay shared with the timeline.
fn shelved(messages: &[Arc<ChatMessage>]) -> Vec<Arc<ChatMessage>> {
    let mut kept = messages[messages.len().saturating_sub(discord::KEPT_MESSAGES)..].to_vec();
    for message in &mut kept {
        if message
            .attachments
            .iter()
            .any(|file| file.preview.is_some())
        {
            for attachment in &mut Arc::make_mut(message).attachments {
                attachment.preview = None;
            }
        }
    }
    kept
}

impl Application {
    pub fn dispatch(&mut self, action: Action) -> Vec<Effect> {
        // A keystroke in the composer shows no picture that wasn't on screen.
        let composing = matches!(action, Action::SetDraft(_) | Action::MentionQuery(_));
        let mut effects = self.dispatch_action(action);
        effects.extend(self.send_pending_message());
        effects.extend(self.show_panel_profile());
        if !composing {
            effects.extend(self.request_pictures());
        }
        effects
    }

    pub fn apply(&mut self, event: Event) -> Vec<Effect> {
        let mut effects = self.apply_event(event);
        // A DM just started, or a thread asked for, opens once the lists
        // have it.
        if let Some(room) = self
            .pending_open
            .take_if(|room| self.state.rooms.iter().any(|listed| &listed.id == room))
        {
            effects.extend(self.dispatch(Action::Open(room.clone())));
            if std::mem::take(&mut self.pending_call) {
                effects.extend(self.dispatch(Action::RequestJoinVoice(room)));
            }
        }
        if let Some(room) = self
            .pending_aside
            .take_if(|room| self.state.rooms.iter().any(|listed| &listed.id == room))
        {
            effects.extend(self.dispatch(Action::OpenAside(room)));
        }
        effects.extend(self.send_pending_message());
        effects.extend(self.show_panel_profile());
        effects.extend(self.request_pictures());
        effects
    }

    /// Asks for the server icons and avatars on screen that weren't asked
    /// for before; see [`pictures_on_screen`]. The timeline, which can hold
    /// thousands of messages, is gone through only when it changed.
    fn request_pictures(&mut self) -> Option<Effect> {
        let state = &self.state;
        let timeline = if Arc::ptr_eq(&self.pictured_timeline, &state.messages) {
            &[][..]
        } else {
            visible_range(&state.messages, &self.visible_messages)
                .map_or(&[][..], |range| &state.messages[range])
        };
        let mut wanted = Vec::new();
        // Virtualized UI rows request their own avatars; this covers profiles,
        // calls and the custom emoji in nearby messages.
        for url in pictures_on_screen(state, timeline, false) {
            if !self.requested_pictures.contains(url) {
                self.requested_pictures.insert(url.to_owned());
                wanted.push(url.to_owned());
            }
        }
        self.pictured_timeline = self.state.messages.clone();
        // The thread beside is short, and shows its custom emoji whole.
        if let Some(aside) = &self.state.aside
            && !Arc::ptr_eq(&self.pictured_aside, &aside.messages)
        {
            for url in pictures_on_screen(state, &aside.messages, false) {
                if !self.requested_pictures.contains(url) {
                    self.requested_pictures.insert(url.to_owned());
                    wanted.push(url.to_owned());
                }
            }
            self.pictured_aside = aside.messages.clone();
        }
        (!wanted.is_empty()).then_some(Effect::Backend(Command::Discord(DiscordCommand::Pictures(
            wanted,
        ))))
    }

    /// Keeps a picture that arrived, and once more than [`KEPT_PICTURES`]
    /// have, forgets those off screen, which are asked for again when they
    /// show. So that pictures on screen don't make it go through them each
    /// time one arrives, it waits for twice as many as it kept last. Custom
    /// emoji stay: the emoji picker asks for hundreds at once, and they're
    /// small. They don't count towards when the others go, or the emoji of a
    /// few servers would let thousands of avatars pile up.
    fn keep_picture(&mut self, url: String, picture: Arc<Picture>) {
        let emoji = is_custom_emoji(&url);
        if self.state.pictures.insert(url, picture).is_none() && emoji {
            self.emoji_pictures += 1;
        }
        self.state.picture_changes += 1;
        let others = self.state.pictures.len() - self.emoji_pictures;
        if others <= KEPT_PICTURES.max(2 * self.pictures_kept) {
            return;
        }
        let aside = self
            .state
            .aside
            .as_ref()
            .map(|aside| aside.messages.as_slice());
        // Of a timeline, the rows on screen and those a short scroll away:
        // one that follows a busy channel can hold thousands, each with
        // someone else's avatar.
        let messages = &self.state.messages;
        let timeline =
            visible_range(messages, &self.visible_messages).map_or(&messages[..], |range| {
                let start = range.start().saturating_sub(PICTURED_AROUND);
                &messages[start..(range.end() + 1 + PICTURED_AROUND).min(messages.len())]
            });
        let shown: HashSet<String> = pictures_on_screen(&self.state, timeline, true)
            .chain(
                aside
                    .into_iter()
                    .flat_map(|aside| pictures_on_screen(&self.state, aside, true)),
            )
            .map(str::to_owned)
            .collect();
        let kept = |url: &String| shown.contains(url) || is_custom_emoji(url);
        self.state.pictures.retain(|url, _| kept(url));
        let pictures = &self.state.pictures;
        self.requested_pictures
            .retain(|url| kept(url) || pictures.contains_key(url));
        self.pictures_kept = self.state.pictures.len() - self.emoji_pictures;
    }

    fn dispatch_action(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::VisibleMessages {
                room,
                items,
                at_end,
            } if self
                .state
                .aside
                .as_ref()
                .is_some_and(|aside| aside.room == room) =>
            {
                if self.aside_visible == items {
                    return vec![];
                }
                self.aside_visible = items.clone();
                vec![Effect::Backend(Command::VisibleMessages {
                    room,
                    items,
                    at_end,
                })]
            }
            Action::VisibleMessages {
                room,
                items,
                at_end,
            } => {
                if self.state.selected.as_ref() != Some(&room) || self.visible_messages == items {
                    return vec![];
                }
                self.visible_messages = items.clone();
                self.pictured_timeline = Arc::new(Vec::new());
                vec![Effect::Backend(Command::VisibleMessages {
                    room,
                    items,
                    at_end,
                })]
            }
            Action::Navigate(navigation) => self.navigate(navigation),
            Action::OpenDmScreen(screen) => self.open_dm_screen(screen),
            Action::BrowseChannels => self.open_browse(),
            Action::BrowseFollow { room, follow } => self.browse_follow(room, follow),
            Action::MessagePerson(user) => self.message_person(user),
            Action::MessageFromProfile { user, text } => self.message_from_profile(user, text),
            Action::ShowProfilePanel(shown) => {
                self.state.profile_panel = shown;
                self.show_panel_profile()
            }
            Action::Open(id) => self.open(id, false),
            Action::OpenChat(id) => self.open(id, true),
            Action::ShowVoiceChat(chat) => self.show_voice_chat(chat),
            Action::OpenAside(id) => self.open_aside(id),
            Action::CloseAside => self.close_aside(),
            Action::ExpandAside => self.expand_aside(),
            Action::AsideEarlier => self.aside_earlier(),
            Action::SetAsideDraft(body) => match self.state.aside.as_ref() {
                Some(aside) => self.set_draft_in(aside.room.clone(), body),
                None => vec![],
            },
            Action::SendAside => self.send_aside(),
            Action::CancelAsideCompose => {
                if let Some(aside) = self.state.aside.as_ref().map(|aside| aside.room.clone()) {
                    self.finish_compose(&aside);
                }
                vec![]
            }
            Action::StageAside(paths) => {
                self.stage_aside(paths);
                vec![]
            }
            Action::UnstageAside(index) => match self.state.aside.as_ref() {
                Some(aside) => self.unstage_in(aside.room.clone(), index),
                None => vec![],
            },
            Action::ToggleAsideSpoiler(index) => match self.state.aside.as_ref() {
                Some(aside) => self.toggle_spoiler_in(aside.room.clone(), index),
                None => vec![],
            },
            Action::SelectRoom(id) => self.select_room(id),
            Action::MarkRead(id) => self.mark_read(id),
            Action::EditLast => self.edit_last(),
            Action::OpenNewDm => self.open_new_dm(),
            Action::NewDmQuery(query) => {
                let Some(new_dm) = &mut self.state.new_dm else {
                    return vec![];
                };
                new_dm.query = query.clone();
                new_dm.error = None;
                vec![Effect::Backend(Command::FindPeople { query })]
            }
            Action::StartDm(contact) => self.start_dm(contact),
            Action::CloseNewDm => {
                self.state.new_dm = None;
                vec![]
            }
            Action::MentionQuery(query) => self.mention_query(query),
            Action::SetDraft(body) => self.set_draft(body),
            Action::Send => self.send(),
            Action::Reply(item) => self.reply(item),
            Action::Edit(item) => self.edit(item),
            Action::CancelCompose => {
                if let Some(id) = self.state.selected.clone() {
                    self.finish_compose(&id);
                }
                vec![]
            }
            Action::React { item, key } => self.react(item, key),
            Action::RequestDelete(item) => self.request_delete(item),
            Action::Vote { item, answers } => self.vote(item, answers),
            Action::OpenPollDraft => self.open_poll_draft(),
            Action::EditPoll(edit) => self.edit_poll(edit),
            Action::PostPoll => self.post_poll(),
            Action::ClosePollDraft => {
                self.state.poll_draft = None;
                vec![]
            }
            Action::AskEndPoll(item) => self.ask_end_poll(item),
            Action::CancelEndPoll => {
                self.state.confirm_end_poll = None;
                vec![]
            }
            Action::EndPoll => self.end_poll(),
            Action::ShowVoters { item, answer } => self.show_voters(item, answer),
            Action::PinMessage(item) => self.pin_message(item),
            Action::StartForward(item) => self.start_forward(item),
            Action::ChooseForward(target) => self.choose_forward(target),
            Action::SearchForward(query) => self.search_forward(query),
            Action::Forward(note) => self.forward(note),
            Action::CloseForward => {
                self.state.forwarding = None;
                vec![]
            }
            Action::ToggleSwitcher => self.toggle_switcher(),
            Action::SearchSwitcher(query) => {
                if let Some(switcher) = &mut self.state.switcher {
                    *switcher = query;
                }
                vec![]
            }
            Action::SwitchTo(id) => self.switch_to(id),
            Action::CloseSwitcher => {
                self.state.switcher = None;
                vec![]
            }
            Action::CancelDelete => {
                self.state.confirm_delete = None;
                vec![]
            }
            Action::ConfirmDelete => self.confirm_delete(),
            Action::Retry(item) => {
                self.queue_action(item, |room, item| Command::Retry { room, item })
            }
            Action::CancelSend(item) => {
                self.queue_action(item, |room, item| Command::Cancel { room, item })
            }
            Action::Stage(paths) => {
                self.stage(paths);
                vec![]
            }
            Action::Unstage(index) => self.unstage(index),
            Action::ToggleSpoiler(index) => self.toggle_spoiler(index),
            Action::SaveAttachment { item, index, path } => self.save_attachment(item, index, path),
            Action::CopyAttachment { item, index } => self.copy_attachment(item, index),
            Action::ViewImage { item, index } => self.view_image(item, index),
            Action::PlayVideo {
                item,
                index,
                width,
                height,
            } => self.play_video(item, index, (width, height)),
            Action::Video(command) => {
                if command == chat::VideoCommand::Stop {
                    self.state.video = None;
                }
                vec![Effect::Backend(Command::Video(command))]
            }
            Action::StepImage(step) => self.step_image(step),
            Action::CloseImage => {
                self.state.viewer = None;
                vec![]
            }
            Action::DismissNotice => {
                self.state.notice = None;
                vec![]
            }
            Action::Jump(item) => self.jump(item, false),
            Action::JumpInRoom { room, item } => self.jump_in_room(room, item),
            Action::JumpToUnread => match self.state.first_unread.clone() {
                Some(item) => self.jump(item, true),
                None => vec![],
            },
            Action::Reading(reading) => {
                self.state.reading = reading;
                self.read_latest()
            }
            Action::Later => self.later(),
            Action::JumpToPresent => self.jump_to_present(),
            Action::LoadPictures(urls) => self.load_pictures(urls),
            Action::Earlier => self.earlier(),
            Action::Login {
                homeserver,
                username,
                password,
            } => self.login(homeserver, username, password),
            Action::Restore => {
                if self.state.busy || self.state.account.is_some() || self.state.preview {
                    vec![]
                } else {
                    self.start(chat::Start::Restore)
                }
            }
            Action::Recover(key) => self.recover(key),
            Action::RefreshTrust => {
                if self.state.busy || self.state.preview || self.state.account.is_none() {
                    return vec![];
                }
                self.state.busy = true;
                vec![Effect::Backend(Command::RefreshTrust)]
            }
            Action::RetrySync => {
                if self.state.busy || self.state.preview || self.state.account.is_none() {
                    vec![]
                } else {
                    vec![Effect::Backend(Command::RetrySync)]
                }
            }
            Action::Logout => self.logout(),
            Action::Preview => self.start_preview(),
            Action::RequestJoinVoice(id) => self.request_join_voice(id),
            Action::AcceptCall(id) => self.accept_call(id),
            Action::DeclineCall(room) => vec![Effect::Backend(Command::Discord(
                DiscordCommand::DeclineCall { room },
            ))],
            Action::Befriend(user) => match self.state.relation(&user) {
                Relation::Stranger | Relation::Incoming => {
                    vec![Effect::Backend(Command::Discord(DiscordCommand::Friend {
                        user,
                        add: true,
                    }))]
                }
                _ => vec![],
            },
            Action::RemoveFriendRequest(user) => match self.state.relation(&user) {
                Relation::Incoming | Relation::Outgoing => {
                    vec![Effect::Backend(Command::Discord(DiscordCommand::Friend {
                        user,
                        add: false,
                    }))]
                }
                _ => vec![],
            },
            Action::Pin { room, pinned } => self.pin(room, pinned),
            Action::SetPinned(pinned) => {
                self.state.pinned = pinned;
                vec![]
            }
            Action::CloseDm(room) => self.close_dm(room),
            Action::ShowPins(shown) => self.show_pins(shown),
            Action::HoverReaction(hovered) => self.hover_reaction(hovered),
            Action::OpenGifs => self.open_gifs(),
            Action::CloseGifs => self.close_gifs(),
            Action::SearchGifs(query) => self.search_gifs(query),
            Action::ShowGifs(view) => self.show_gifs(view),
            Action::SearchGifCategory(name) => self.search_gif_category(name),
            Action::GifsShown { shown, near_end } => self.gifs_shown(shown, near_end),
            Action::PickGif { url, insert } => self.pick_gif(url, insert),
            Action::FavoriteGif(url) => self.favorite_gif(url),
            Action::FavoriteMessageGif { item, index } => self.favorite_message_gif(item, index),
            Action::LoadFavoriteGifs => self.load_favorite_gifs(),
            Action::ShowReactions { item, key } => self.show_reactions(item, key),
            Action::CloseReactions => self.close_reactions(),
            Action::ShowReactors(key) => self.show_reactors(key),
            Action::MoreReactors => self.more_reactors(),
            Action::RemoveReactor(user) => self.remove_reactor(user),
            Action::ShowThreads(shown) => self.show_threads(shown),
            Action::SearchThreads(query) => self.search_threads(query),
            Action::MoreThreads => self.more_threads(),
            Action::SearchPosts(query) => self.search_posts(query),
            Action::MorePosts => self.more_posts(),
            Action::NewPost => {
                self.open_new_post();
                vec![]
            }
            Action::WriteNewPost { title, body } => {
                self.write_new_post(title, body);
                vec![]
            }
            Action::TogglePostTag(tag) => {
                self.toggle_post_tag(tag);
                vec![]
            }
            Action::CreatePost => self.create_post(),
            Action::CloseNewPost => {
                self.close_new_post(false);
                vec![]
            }
            Action::LeaveNewPost => {
                self.close_new_post(true);
                vec![]
            }
            Action::StayInNewPost => {
                if let Some(post) = &mut self.state.new_post {
                    post.leaving = false;
                }
                vec![]
            }
            Action::NewThread { message, from } => self.open_new_thread(message, from),
            Action::WriteNewThread { name, body } => {
                self.write_new_thread(name, body);
                vec![]
            }
            Action::SetThreadPrivate(private) => {
                self.set_thread_private(private);
                vec![]
            }
            Action::CreateThread => self.create_thread(),
            Action::CloseNewThread => {
                self.close_new_thread(false);
                vec![]
            }
            Action::LeaveNewThread => {
                self.close_new_thread(true);
                vec![]
            }
            Action::StayInNewThread => {
                if let Some(form) = &mut self.state.new_thread {
                    form.leaving = false;
                }
                vec![]
            }
            Action::Search(query) => self.search(query),
            Action::MoreResults => self.more_results(),
            Action::OpenAddPeople => self.open_add_people(),
            Action::AddPeopleQuery(query) => {
                if let Some(picker) = &mut self.state.add_people {
                    picker.query = query;
                }
                vec![]
            }
            Action::ChoosePerson(user) => self.choose_person(user),
            Action::AddPeople => self.add_people(),
            Action::CloseAddPeople => {
                self.state.add_people = None;
                vec![]
            }
            Action::MuteDm { room, muting } => self.mute_dm(room, muting),
            Action::ChangeNotifications { place, change } => {
                self.change_notifications(place, change)
            }
            Action::PinChannel { room, pinned } => self.pin_channel(room, pinned),
            Action::MoveChannel { room, landing } => self.move_channel(room, landing),
            Action::AnswerMoveSync(sync) => self.answer_move_sync(sync),
            Action::FollowChannel { room, follow } => self.follow_channel(room, follow),
            Action::ShowAllChannels { server, show } => self.show_all_channels(server, show),
            Action::JoinThread {
                room,
                join,
                location,
            } => self.join_thread(room, join, location),
            Action::OpenInvite(place) => self.open_invite(place),
            Action::MakeInvite { max_age, max_uses } => self.make_invite(max_age, max_uses),
            Action::CloseInvite => {
                self.state.invite = None;
                vec![]
            }
            Action::InviteToServer { user, server } => self.invite_to_server(user, server),
            Action::OpenJoinServer => self.open_join(),
            Action::FindInvite(typed) => self.find_invite(typed),
            Action::OpenInviteLink(code) => self.open_invite_link(code),
            Action::JoinServer => self.join_server(),
            Action::OpenJoinCaptcha => self.open_join_captcha(),
            Action::OpenJoinForm => self.open_join_form(),
            Action::CloseJoin => {
                self.state.join = None;
                vec![]
            }
            Action::OpenEventLink { guild, event } => self.open_event_link(guild, event),
            Action::ToggleEventInterest => self.toggle_event_interest(),
            Action::CloseEvent => {
                self.state.scheduled_event = None;
                vec![]
            }
            Action::NewChannel { place, kind } => self.new_channel(place, kind),
            Action::AskChannel(ask) => self.ask_channel(ask),
            Action::OpenChannelSettings(room) => self.open_channel_settings(room),
            Action::ChannelSettings(input) => self.channel_input(input),
            Action::SaveChannel { kind, settings } => self.save_channel(kind, settings),
            Action::CloseChannelDialog => {
                self.state.channel_dialog = None;
                vec![]
            }
            Action::AskLeaveServer(server) => self.ask_leave(server),
            Action::AskLeaveGroup(room) => self.ask_leave_group(room),
            Action::ToggleSilentLeave => {
                if let Some(dialog) = self
                    .state
                    .leave
                    .as_mut()
                    .filter(|dialog| dialog.group && !dialog.leaving)
                {
                    dialog.silent = !dialog.silent;
                }
                vec![]
            }
            Action::Leave => self.leave(),
            Action::CloseLeave => {
                self.state.leave = None;
                vec![]
            }
            Action::EditProfile => self.edit_profile(),
            Action::EditServerProfile(place) => self.edit_server_profile(place),
            Action::WriteProfile { field, text } => {
                self.write_profile(field, text);
                vec![]
            }
            Action::PickProfilePicture { kind, path } => self.pick_profile_picture(kind, path),
            Action::ResetProfilePicture(kind) => {
                self.reset_profile_picture(kind);
                vec![]
            }
            Action::UndoProfileEdits => {
                self.undo_profile_edits();
                vec![]
            }
            Action::SaveProfile => self.save_profile(),
            Action::UnlockNitro => self.unlock_nitro(),
            Action::CloseProfileEditor => {
                self.state.profile_editor = None;
                vec![]
            }
            Action::OpenServerSettings(place) => self.open_server_settings(place),
            Action::ServerSettingsSection(section) => self.server_settings_section(section),
            Action::WriteServerSetting { field, text } => {
                self.write_server_setting(field, text);
                vec![]
            }
            Action::ToggleServerSetting(toggle) => {
                self.toggle_server_setting(toggle);
                vec![]
            }
            Action::ChooseServerSetting(choice) => self.choose_server_setting(choice),
            Action::PickServerPicture { kind, path } => self.pick_server_picture(kind, path),
            Action::RemoveServerPicture(kind) => {
                self.remove_server_picture(kind);
                vec![]
            }
            Action::UndoServerSettings => {
                self.undo_server_settings();
                vec![]
            }
            Action::SaveServerSettings => self.save_server_settings(),
            Action::Roles(input) => self.role_input(input),
            Action::Emoji(input) => self.emoji_input(input),
            Action::Stickers(input) => self.sticker_input(input),
            Action::Members(input) => self.member_input(input),
            Action::Ownership(input) => self.ownership_input(input),
            Action::Invites(input) => self.invite_input(input),
            Action::Bans(input) => self.ban_input(input),
            Action::AuditLog(input) => self.audit_log_input(input),
            Action::CloseServerSettings => {
                self.close_server_settings();
                vec![]
            }
            Action::ChooseMfa(method) => self.choose_mfa(method),
            Action::WriteMfa(code) => {
                self.write_mfa(code);
                vec![]
            }
            Action::ResendMfa => self.resend_mfa(),
            Action::ConfirmMfa => self.confirm_mfa(),
            Action::CancelMfa => self.cancel_mfa(),
            Action::ChangeRelationship { user, change } => self.change_relationship(user, change),
            Action::AskPerson { user, ask } => self.ask_person(user, ask),
            Action::AnswerPerson(text) => self.answer_person(text),
            Action::ClosePersonDialog => {
                self.state.person_dialog = None;
                vec![]
            }
            Action::MentionPerson(user) => self.mention_person(user),
            Action::CallPerson(user) => self.call_person(user),
            Action::AskMember { place, user, ask } => self.ask_member(place, user, ask),
            Action::ChoosePersonOption(choice) => {
                if let Some(dialog) = &mut self.state.person_dialog {
                    dialog.choice = choice;
                }
                vec![]
            }
            Action::ChangeMember {
                place,
                user,
                change,
            } => self.change_member(place, user, change),
            Action::LinkDiscord => {
                if self.state.preview || self.state.discord_linked || !cfg!(feature = "discord") {
                    return vec![];
                }
                self.state.discord_link = Some(DiscordLink::Starting);
                vec![Effect::LinkDiscord]
            }
            Action::LinkDiscordToken(token) => {
                if self.state.preview || self.state.discord_linked || !cfg!(feature = "discord") {
                    return vec![];
                }
                if token.trim().is_empty() {
                    return vec![];
                }
                self.state.discord_link = Some(DiscordLink::Checking);
                vec![Effect::LinkDiscordToken(token)]
            }
            Action::OpenDiscordCaptcha => match &self.state.discord_link {
                Some(DiscordLink::Captcha(page)) => vec![Effect::OpenPage(page.clone())],
                _ => vec![],
            },
            Action::CancelDiscordLink => match self.state.discord_link.take() {
                Some(_) => vec![Effect::StopDiscordLink],
                None => vec![],
            },
            Action::UnlinkDiscord => {
                if self.state.discord_linked && self.state.account.is_some() {
                    vec![Effect::Backend(Command::Discord(DiscordCommand::Unlink))]
                } else {
                    vec![]
                }
            }
            Action::OpenAddFriend => {
                if self.state.discord_user.is_some() {
                    self.state.add_friend = Some(AddFriend::default());
                }
                vec![]
            }
            Action::AddFriend(username) => self.add_friend(username),
            Action::CloseAddFriend => {
                self.state.add_friend = None;
                vec![]
            }
            Action::OpenStatusEditor => {
                if self.state.discord_user.is_some() {
                    self.state.status_editor = Some(StatusEditor::default());
                }
                vec![]
            }
            Action::SetCustomStatus {
                emoji,
                text,
                clear_after,
            } => self.set_custom_status(emoji, text, clear_after),
            Action::ClearCustomStatus => self.save_status(None),
            Action::CloseStatusEditor => {
                self.state.status_editor = None;
                vec![]
            }
            Action::SetStatus(status) => {
                if self.state.preview
                    || self.state.discord_user.is_none()
                    || self.state.discord_presence.as_ref() == Some(&status)
                {
                    return vec![];
                }
                vec![Effect::Backend(Command::Discord(
                    DiscordCommand::SetStatus(status),
                ))]
            }
            Action::ShowGame(shown) => {
                if self.state.preview
                    || self.state.discord_user.is_none()
                    || self.state.discord_show_game == shown
                {
                    return vec![];
                }
                // The switch moves at once; a refusal moves it back.
                self.state.discord_show_game = shown;
                vec![Effect::Backend(Command::Discord(DiscordCommand::ShowGame(
                    shown,
                )))]
            }
            Action::SoundboardVolume(slider) => {
                // Discord keeps the volume on its slider's curve.
                let volume = crate::volume::from_slider(slider.clamp(0.0, 100.0)) * 100.0;
                if self.state.preview
                    || self.state.discord_user.is_none()
                    || (self.state.soundboard_volume - volume).abs() < 0.01
                {
                    return vec![];
                }
                self.state.soundboard_volume = volume;
                vec![Effect::Backend(Command::Discord(
                    DiscordCommand::SoundboardVolume(volume),
                ))]
            }
            Action::LoadSoundCatalog => {
                if self.state.preview || self.state.discord_user.is_none() {
                    return vec![];
                }
                vec![Effect::Backend(Command::Discord(
                    DiscordCommand::SoundCatalog,
                ))]
            }
            Action::SetJoinSound { server, sound } => {
                if self.state.preview || self.state.discord_user.is_none() {
                    return vec![];
                }
                // The sound's own server, from the sounds Settings offered.
                let sound = match sound {
                    None => None,
                    Some(id) => {
                        let found = self
                            .state
                            .sound_catalog
                            .iter()
                            .flat_map(|groups| groups.iter())
                            .flat_map(|group| &group.sounds)
                            .find(|sound| sound.id == id);
                        let Some(found) = found else { return vec![] };
                        Some((id, found.guild.clone()))
                    }
                };
                // Shown at once; the settings Discord sends back confirm it.
                self.state.join_sounds.retain(|join| join.server != server);
                if let Some((id, guild)) = &sound {
                    self.state.join_sounds.push(JoinSound {
                        server: server.clone(),
                        sound: id.clone(),
                        guild: guild.clone(),
                    });
                }
                vec![Effect::Backend(Command::Discord(
                    DiscordCommand::SetJoinSound { server, sound },
                ))]
            }
            Action::Inactive { since, show_idle } => {
                // Discord keeps it through reconnecting, so it goes even
                // before Discord connects. On the sign-in screen nothing
                // runs to hear it.
                let signed_out = self.state.account.is_none() && !self.state.busy;
                if self.state.preview || signed_out {
                    return vec![];
                }
                vec![Effect::Backend(Command::Discord(
                    DiscordCommand::Inactive { since, show_idle },
                ))]
            }
            Action::ConfirmVoiceSwitch => self.confirm_voice_switch(),
            Action::CancelVoiceSwitch => {
                self.state.switch_voice = None;
                self.state.pending_watch = None;
                vec![]
            }
            Action::ShowVoicePerson { room, user } => self.show_voice_person(room, user),
            Action::DismissVoiceError => {
                self.state.voice.error = None;
                vec![]
            }
            Action::ReplyPing(ping) => {
                self.state.reply_ping = ping;
                vec![]
            }
            Action::ToggleReplyPing { aside } => {
                let room = if aside {
                    self.state.aside.as_ref().map(|aside| &aside.room)
                } else {
                    self.state.selected.as_ref()
                };
                if let Some(room) = room.cloned()
                    && let Some(Compose::Reply { ping, .. }) = self.state.compose.get_mut(&room)
                {
                    *ping = !*ping;
                }
                vec![]
            }
            Action::StripTracking(strip) => {
                self.state.strip_tracking = strip;
                vec![]
            }
            Action::HideBlocked(hide) => {
                if self.state.hide_blocked == hide {
                    return vec![];
                }
                self.state.hide_blocked = hide;
                let all = match self.with_blocked.take() {
                    Some((all, shown)) if Arc::ptr_eq(&shown, &self.state.messages) => all,
                    _ => self.state.messages.clone(),
                };
                self.state.messages = self.without_blocked(all);
                vec![]
            }
            Action::ShowHiddenChannels(shown) => {
                if self.state.show_hidden_channels == shown {
                    return vec![];
                }
                self.state.show_hidden_channels = shown;
                self.list_discord()
            }
            Action::SyncDiscordReads(sync) => {
                self.state.sync_discord_reads = sync;
                vec![]
            }
            Action::ShowProfile { user, large } => self.show_profile(user, large),
            Action::ShowDmProfile(id) => self.show_dm_profile(id),
            Action::ShowFriendProfile { user, large } => self.show_friend_profile(user, large),
            Action::EnlargeProfile => {
                if let Some(view) = &mut self.state.profile {
                    view.large = true;
                }
                vec![]
            }
            Action::CloseProfile => {
                self.state.profile = None;
                vec![]
            }
            Action::ShowRole(role) => self.show_role(role),
            Action::CloseRole => {
                self.state.role = None;
                vec![]
            }
            Action::AnswerRequest(accept) => self.answer_request(accept),
            Action::Voice(voice::Command::Join(id)) => self.dispatch(Action::RequestJoinVoice(id)),
            Action::Voice(command) => self.voice_command(command),
            Action::UnmuteOnJoin => self.voice_command_quietly(voice::Command::Mute(false)),
            Action::DeafenOnJoin => self.voice_command_quietly(voice::Command::Deafen(true)),
        }
    }

    /// `messages` as the timeline shows them: without those from people
    /// this account blocked when they're hidden. The whole timeline is kept
    /// aside then, sharing its messages, to show them again.
    fn without_blocked(&mut self, messages: TimelineArc) -> TimelineArc {
        if !self.state.hide_blocked || !messages.iter().any(|message| message.blocked) {
            self.with_blocked = None;
            return messages;
        }
        let shown: TimelineArc = Arc::new(
            messages
                .iter()
                .filter(|message| !message.blocked)
                .cloned()
                .collect(),
        );
        self.with_blocked = Some((messages, shown.clone()));
        shown
    }

    /// A notification titled with who wrote where: the person alone in a DM, else with
    /// the group, the Matrix room, or the Discord channel and its server.
    fn alert(&self, notification: chat::Notification) -> chat::Alert {
        let chat::Notification {
            room,
            author,
            body,
            avatar,
            mention,
        } = notification;
        let place = self.state.rooms.iter().find(|known| known.id == room);
        let server = self
            .state
            .servers
            .iter()
            .find(|server| server.channels.contains(&room));
        let title = match (place, server) {
            (Some(place), _) if place.direct && place.person.is_some() => author,
            (Some(place), Some(server)) if server.discord => {
                format!("{author} in #{} ({})", place.name, server.name)
            }
            (Some(place), _) => format!("{author} in {}", place.name),
            (None, _) => author,
        };
        let picture = avatar.and_then(|avatar| self.state.pictures.get(&avatar).cloned());
        chat::Alert {
            room,
            title,
            body,
            picture,
            call: false,
            mention,
        }
    }

    fn apply_event(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Voice(state) => {
                let sounds = call_sounds(&self.state.voice, &state);
                // Someone moved the call to another channel, as a bot does
                // from one that makes rooms: its view goes along.
                if let (Some(from), Some(to)) = (&self.state.voice.room, &state.room)
                    && from != to
                    && self.state.voice.active()
                    && self.state.selected.as_ref() == Some(from)
                {
                    self.pending_open = Some(to.clone());
                }
                // The call a stream waited for failed to join, or was left.
                if let Some((room, _)) = &self.state.pending_watch
                    && self.state.voice.room.as_ref() == Some(room)
                    && state.room.as_ref() != Some(room)
                {
                    self.state.pending_watch = None;
                }
                self.state.voice = state;
                let mut effects = sounds;
                effects.extend(self.watch_when_connected());
                return effects;
            }
            Event::Connected {
                user,
                trusted,
                storage,
            } => {
                self.state.account = Some(user);
                self.state.trusted = trusted;
                self.state.storage = Some(storage);
                self.state.busy = false;
                self.state.notice = None;
            }
            Event::Rooms(index) => {
                // Those that didn't change stay shared with the state's.
                chat::share_list(&mut self.matrix_rooms, index.rooms);
                chat::share_list(&mut self.matrix_servers, index.servers);
                return self.show_lists();
            }
            Event::Image {
                room,
                item,
                index,
                picture,
            } => {
                // The room it came from: the selected one, or the thread
                // beside it.
                if self.state.room_of(&item) == Some(&room)
                    && let Some(viewer) = &mut self.state.viewer
                    && viewer.item == item
                {
                    viewer.pictures.insert(index, Some(picture));
                }
            }
            Event::Picture { url, picture } => self.keep_picture(url, picture),
            Event::Video(video) => {
                self.state.video =
                    video.filter(|video| self.state.selected.as_ref() == Some(&video.room));
            }
            // Do Not Disturb holds back message notifications, as in Discord.
            Event::Notify(_) if self.state.do_not_disturb() => {}
            Event::Notify(notification) => return vec![Effect::Notify(self.alert(notification))],
            Event::Typing { room, names } => {
                if names.is_empty() {
                    self.state.typing.remove(&room);
                } else {
                    self.state.typing.insert(room, names);
                }
            }
            #[cfg(feature = "discord")]
            Event::Discord(snapshot) => return self.show_snapshot(snapshot),
            Event::Timeline { room, messages }
                if self
                    .state
                    .aside
                    .as_ref()
                    .is_some_and(|aside| aside.room == room) =>
            {
                return self.aside_timeline(messages);
            }
            Event::Timeline { room, messages } => {
                // A window on its way replaces what showed before.
                if self.state.selected.as_ref() == Some(&room) && !self.focusing {
                    // A backend can drop its oldest messages, as Discord does
                    // after missing many; what came before them loads again.
                    if let Some(first) = self.state.messages.first()
                        && !messages.iter().any(|message| message.item == first.item)
                    {
                        self.state.history_start = false;
                    }
                    self.state.room_ready = true;
                    self.state.messages = self.without_blocked(Arc::new(messages));
                    let mut effects = self.place_unread();
                    effects.extend(self.seek());
                    effects.extend(self.read_latest());
                    return effects;
                }
            }
            Event::RoomError { room, message } => {
                if self.state.selected.as_ref() == Some(&room)
                    || self
                        .state
                        .aside
                        .as_ref()
                        .is_some_and(|aside| aside.room == room)
                {
                    self.state.notice = Some(message);
                }
            }
            Event::Profile { user, result } => self.show_fetched_profile(user, result),
            Event::KeptProfile { user, profile } => self.show_kept_profile(user, profile),
            Event::Members { room, members } => {
                // A list that came stays when a later check finds none came.
                let listed = members.is_err() && matches!(self.state.members, Some(Ok(_)));
                if self.state.selected.as_ref() == Some(&room) && !listed {
                    self.state.members = Some(members);
                }
            }
            Event::People {
                query,
                discord,
                people,
            } => {
                if let Some(new_dm) = &mut self.state.new_dm
                    && new_dm.query.trim() == query.trim()
                {
                    if discord {
                        new_dm.discord = people;
                    } else {
                        new_dm.matrix = people;
                    }
                }
            }
            Event::DmStarted(Ok(room)) => {
                if self.state.new_dm.is_some() || std::mem::take(&mut self.starting_dm) {
                    self.state.new_dm = None;
                    self.pending_open = Some(room);
                }
            }
            Event::ThreadOpened { room, result } => self.thread_opened(room, result),
            Event::Pins { room, result } => self.pins_came(room, result),
            Event::GifCategories(result) => self.gif_categories_came(result),
            Event::Gifs { query, result } => self.gifs_came(query, result),
            Event::GifSuggestions { query, terms } => self.gif_suggestions_came(query, terms),
            Event::FavoriteGifs(result) => return self.favorite_gifs_came(result),
            Event::KeptFavoriteGifs(favorites) => self.kept_favorite_gifs_came(favorites),
            Event::PlacesUsed(result) => self.state.places_used.came(result),
            Event::GifPicture { url, picture } => self.gif_picture_came(url, picture),
            Event::Reactors {
                item,
                key,
                after,
                limit,
                result,
                ..
            } => self.reactors_came(item, key, after, limit, result),
            Event::Threads {
                room,
                query,
                offset,
                result,
                starters,
            } => return self.threads_came(room, query, offset, result, starters),
            Event::Posts { room, result } => return self.posts_came(room, result),
            Event::ThreadStarted { room, result } => return self.thread_started(room, result),
            Event::ForumPosted { room, result } => return self.forum_posted(room, result),
            Event::Found {
                place,
                query,
                offset,
                result,
            } => self.found(place, query, offset, result),
            Event::Topic { room, topic } => {
                if self.state.selected.as_ref() == Some(&room) {
                    self.state.topic = topic;
                }
            }
            Event::GroupChanged(result) => self.group_changed(result),
            Event::InviteMade { room, result } => {
                let made = result.is_ok();
                self.invite_made(room.clone(), result);
                if made {
                    return self.invites_after(&room);
                }
            }
            Event::InviteSent { result, .. } => self.invite_sent(result),
            Event::InviteResolved { code, result } => return self.invite_resolved(code, result),
            Event::JoinCaptcha { code, page } => return self.join_captcha(code, page),
            Event::ServerJoined { code, result } => return self.server_joined(code, result),
            Event::EventResolved { event, result } => return self.event_resolved(event, result),
            Event::EventInterestSet {
                event,
                interested,
                result,
            } => return self.event_interest_set(event, interested, result),
            Event::ChannelLoaded { room, result } => self.channel_settings_loaded(&room, result),
            Event::ChannelSaved(result) => {
                if !self.channel_settings_saved(&result) {
                    self.channel_saved(result);
                }
            }
            Event::OverwritesChanged { room, result } => self.overwrites_changed(room, result),
            Event::OverwriteNames { room, members } => self.overwrite_names(room, members),
            Event::Left { place, result } => self.left(place, result),
            Event::ServerDeleted { server, result } => self.server_deleted(server, result),
            Event::ServerTransferred { server, result } => self.server_transferred(server, result),
            Event::ServerPincodeSent { server, result } => self.server_pincode_sent(server, result),
            Event::OwnProfile { server, result } => self.own_profile_loaded(server, result),
            Event::ProfilePicture { kind, path, result } => {
                self.profile_picture_read(kind, path, result);
            }
            Event::ProfileSaved { server, result } => self.profile_saved(server, result),
            Event::MfaRequired { id, methods } => return self.mfa_required(id, methods),
            Event::MfaFinished { id, result } => return self.mfa_finished(id, result),
            Event::MfaTexted { id, result } => self.mfa_texted(id, result),
            Event::ServerSettingsLoaded {
                server,
                section,
                result,
            } => self.server_settings_loaded(server, section, result),
            Event::ServerPicture { kind, path, result } => {
                self.server_picture_read(kind, path, result);
            }
            Event::ServerSettingsChanged { server, settings } => {
                self.server_settings_changed(server, *settings);
            }
            Event::ServerSettingsSaved { server, result } => {
                self.server_settings_saved(server, result);
            }
            Event::ServerRoleChanged { server, role } => {
                self.server_role_changed(server, Some(*role), None);
            }
            Event::ServerRoleDeleted { server, role } => {
                self.server_role_changed(server, None, Some(role));
            }
            Event::BansLoaded {
                server,
                searched,
                result,
            } => self.bans_loaded(server, searched, result),
            Event::Unbanned {
                server,
                user,
                result,
            } => self.unbanned(server, user, result),
            Event::AuditLogLoaded {
                server,
                query,
                result,
            } => self.audit_log_loaded(server, query, result),
            Event::MembersFound {
                server,
                query,
                result,
            } => return self.members_found(server, query, result),
            Event::MembersIndexing { server, query } => self.members_indexing(server, query),
            Event::InvitesLoaded { server, result } => self.invites_loaded(server, result),
            Event::InviteRevoked {
                server,
                code,
                result,
            } => self.invite_revoked(server, code, result),
            Event::ServerPaused { server, result } => self.server_paused(server, result),
            Event::EmojisChanged {
                server,
                upload,
                result,
                emojis,
            } => self.emojis_changed(server, upload, result, emojis),
            Event::StickersChanged {
                server,
                form,
                result,
                stickers,
            } => self.stickers_changed(server, form, result, stickers),
            Event::ServerStickers { server, stickers } => self.server_stickers(server, stickers),
            Event::ServerBan {
                server,
                ban,
                banned,
            } => self.server_ban(server, *ban, banned),
            Event::RoleHolders {
                server,
                role,
                result,
                count,
            } => self.role_holders(server, role, result, count),
            Event::RoleCandidates {
                server,
                query,
                members,
            } => {
                if !self.overwrite_candidates(&server, &query, &members) {
                    self.role_candidates(server, query, members);
                }
            }
            Event::FriendAdded(outcome) => {
                if let Some(dialog) = &mut self.state.add_friend {
                    dialog.sending = false;
                    dialog.outcome = Some(outcome);
                }
            }
            Event::DiscordUnlinked => return self.forget_discord(),
            Event::Note { user, result } => {
                if let Some(dialog) = self
                    .state
                    .person_dialog
                    .as_mut()
                    .filter(|dialog| dialog.user == user && dialog.ask == PersonAsk::Note)
                {
                    match result {
                        Ok(note) => dialog.text = Some(note),
                        Err(error) => {
                            dialog.text = Some(String::new());
                            dialog.error = Some(format!("Couldn't load your note: {error}"));
                        }
                    }
                }
            }
            Event::PersonSaved(result) => {
                if let Some(dialog) = &mut self.state.person_dialog {
                    match result {
                        // Members shows the change.
                        Ok(()) => {
                            self.state.person_dialog = None;
                            return self.members_again();
                        }
                        Err(error) => {
                            dialog.saving = false;
                            dialog.error = Some(error);
                        }
                    }
                }
            }
            Event::CustomStatusSet(Ok(())) => {
                self.state.status_editor = None;
            }
            Event::StatusSet(result) => {
                if let Err(error) = result {
                    self.state.notice = Some(format!("Couldn't change your status: {error}"));
                }
            }
            Event::SoundboardVolumeSet(Err(error)) => {
                self.state.notice = Some(format!("Couldn't change the soundboard volume: {error}"));
            }
            Event::JoinSoundSet(Err(error)) => {
                self.state.notice = Some(format!("Couldn't change your entrance sound: {error}"));
            }
            Event::SoundboardVolumeSet(Ok(())) | Event::JoinSoundSet(Ok(())) => {}
            Event::SoundCatalog(result) => match result {
                Ok(groups) => {
                    // The sounds' custom emoji, for the chooser.
                    let pictures = groups
                        .iter()
                        .flat_map(|group| &group.sounds)
                        .filter_map(|sound| sound.picture.clone())
                        .collect();
                    self.state.sound_catalog = Some(groups);
                    return self.load_pictures(pictures);
                }
                Err(error) => {
                    self.state.notice =
                        Some(format!("Couldn't list the soundboard's sounds: {error}"));
                }
            },
            Event::ShowGameSet(result) => {
                if let Err(error) = result {
                    self.state.discord_show_game = !self.state.discord_show_game;
                    self.state.notice = Some(format!(
                        "Couldn't change sharing the game you play: {error}"
                    ));
                }
            }
            Event::CustomStatusSet(Err(error)) => {
                if let Some(editor) = &mut self.state.status_editor {
                    editor.saving = false;
                    editor.error = Some(error);
                }
            }
            Event::DmStarted(Err(error)) => {
                if let Some(new_dm) = &mut self.state.new_dm {
                    new_dm.starting = false;
                    new_dm.error = Some(error);
                } else if std::mem::take(&mut self.starting_dm) {
                    self.pending_call = false;
                    self.pending_message = None;
                    self.state.notice = Some(error);
                }
            }
            Event::RoleMembers {
                room,
                role,
                name,
                color,
                members,
            } => {
                if self.state.selected.as_ref() == Some(&room)
                    && let Some(view) = self.state.role.as_mut().filter(|view| view.id == role)
                {
                    view.name = name;
                    view.color = color;
                    view.members = Some(members);
                }
            }
            Event::MentionGroups { room, groups } => {
                if self.state.selected.as_ref() == Some(&room) {
                    self.state.mention_groups = groups;
                }
            }
            Event::MemberMatches { room, members, .. } => {
                if self.state.selected.as_ref() == Some(&room) {
                    for member in members {
                        if !self
                            .state
                            .found_members
                            .iter()
                            .any(|known| known.id == member.id)
                        {
                            self.state.found_members.push(member);
                        }
                    }
                }
            }
            Event::HistoryLoaded {
                room,
                direction,
                history,
            } => {
                if self
                    .state
                    .aside
                    .as_ref()
                    .is_some_and(|aside| aside.room == room)
                {
                    self.aside_history(direction, history);
                } else if self.state.selected.as_ref() == Some(&room) {
                    return self.history_loaded(direction, history);
                }
            }
            Event::SendResult { room, body, result } => {
                if self.state.sending.as_ref() == Some(&room) {
                    self.state.sending = None;
                }
                let sent = self.sent_draft.take().unwrap_or(body);
                let files = std::mem::take(&mut self.sent_files);
                match result {
                    Ok(()) => {
                        if self.state.drafts.get(&room) == Some(&sent) {
                            self.state.drafts.remove(&room);
                        }
                        if let Some(staged) = self.state.staged.get_mut(&room) {
                            staged.retain(|file| !files.contains(&file.path));
                        }
                        if matches!(self.state.compose.get(&room), Some(Compose::Reply { .. })) {
                            self.state.compose.remove(&room);
                        }
                        self.state.notice = None;
                    }
                    Err(error) => self.state.notice = Some(error.into()),
                }
            }
            Event::EditResult { room, item, result } => {
                if self.state.sending.as_ref() == Some(&room) {
                    self.state.sending = None;
                }
                match result {
                    Ok(()) => {
                        if matches!(self.state.compose.get(&room), Some(Compose::Edit { item: target, .. }) if target == &item)
                        {
                            self.finish_compose(&room);
                        }
                        self.state.notice = None;
                    }
                    Err(error) => self.state.notice = Some(error),
                }
            }
            Event::Forwarded { room, result } => self.forwarded(room, result),
            Event::PollPosted { room, result } => self.poll_posted(room, result),
            Event::Draft { room, body } => {
                // Text typed since opening the room wins over the saved draft.
                if self.state.drafts.get(&room).is_none_or(String::is_empty)
                    && !self.state.compose.contains_key(&room)
                {
                    self.state.drafts.insert(room, body);
                }
            }
            Event::Saved { room, name, result } => {
                if self.state.selected.as_ref() == Some(&room) {
                    self.state.notice = Some(match result {
                        Ok(()) => format!("Saved {name}."),
                        Err(error) => error.into(),
                    });
                }
            }
            // Asked for just now, from the conversation or the thread beside it.
            Event::Copied(picture) => match picture {
                Ok(picture) => return vec![Effect::CopyPicture(picture)],
                Err(error) => self.state.notice = Some(error.into()),
            },
            Event::Trust { trusted, error } => {
                self.state.busy = false;
                self.state.trusted = trusted;
                self.state.notice = error.map(str::to_owned).or_else(|| {
                    (!trusted).then(|| "This device is still unverified. Verify it in another client or enter your recovery key.".into())
                });
            }
            Event::Sync(online) => self.state.online = online,
            Event::Error(error) => {
                self.state.busy = false;
                self.state.notice = Some(error);
            }
            Event::LoggedOut => self.reset(),
        }
        vec![]
    }

    fn start(&mut self, start: chat::Start) -> Vec<Effect> {
        self.state.busy = true;
        self.state.notice = Some(if cfg!(feature = "matrix") {
            "Connecting to Matrix...".into()
        } else {
            "Connecting to Discord...".into()
        });
        vec![Effect::Start(start)]
    }

    /// Whether the Discord account has Nitro, which join sounds need.
    pub fn discord_nitro(&self) -> bool {
        #[cfg(feature = "discord")]
        if let Some(snapshot) = &self.discord {
            return snapshot
                .account
                .as_ref()
                .is_some_and(|account| account.premium_type > 0);
        }
        false
    }

    /// The custom emoji this account may use in the selected room, by server, the
    /// room's own server first: with Nitro every server's, and without it only
    /// that server's, none animated.
    pub fn usable_emoji(&self) -> Vec<chat::EmojiSet> {
        #[cfg(feature = "discord")]
        if let Some(snapshot) = &self.discord {
            let here = self
                .state
                .selected
                .as_deref()
                .and_then(discord::parse_room)
                .map(|(guild, _)| guild);
            let nitro = snapshot
                .account
                .as_ref()
                .is_some_and(|account| account.premium_type > 0);
            let mut servers: Vec<_> = snapshot
                .servers
                .iter()
                .filter(|server| nitro || Some(server.id.as_str()) == here)
                .collect();
            servers.sort_by_key(|server| Some(server.id.as_str()) != here);
            return servers
                .into_iter()
                .map(|server| chat::EmojiSet {
                    server: server.name.clone(),
                    icon: server.icon.clone(),
                    emoji: server
                        .emojis
                        .iter()
                        .filter(|emoji| emoji.available && (nitro || !emoji.animated))
                        .map(|emoji| chat::CustomEmoji {
                            text: format!(":{}:", emoji.name),
                            id: emoji.id.clone(),
                            animated: emoji.animated,
                            picture: yapfast_discord::ready::custom_emoji(
                                &emoji.id,
                                false,
                                chat::EMOJI_PICTURE_SIZE,
                            ),
                        })
                        .collect(),
                })
                .filter(|set| !set.emoji.is_empty())
                .collect();
        }
        vec![]
    }

    /// The `count` reactions this device adds most, then the defaults.
    pub fn frequent_reactions(&self, count: usize) -> Vec<String> {
        let mut used: Vec<_> = self.reaction_uses.iter().collect();
        used.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let mut frequent: Vec<String> = Vec::with_capacity(count);
        for key in used
            .into_iter()
            .map(|(key, _)| key.as_str())
            .chain(DEFAULT_REACTIONS)
        {
            if frequent.len() == count {
                break;
            }
            if !frequent.iter().any(|known| known == key) {
                frequent.push(key.to_owned());
            }
        }
        frequent
    }

    /// How often this device added each reaction, for an adapter to keep across runs.
    pub fn reaction_uses(&self) -> &HashMap<String, u32> {
        &self.reaction_uses
    }

    /// Restore counts kept from [`Self::reaction_uses`], keeping any higher count.
    pub fn remember_reaction_uses(&mut self, uses: HashMap<String, u32>) {
        for (key, count) in uses {
            let known = self.reaction_uses.entry(key).or_default();
            *known = (*known).max(count);
        }
    }

    /// Keeps the Discord channel on screen as it is, to show again at once.
    fn remember_view(&mut self) {
        let Some(room) = self.state.selected.clone() else {
            return;
        };
        if !self.state.shows_text()
            || !self.state.room_ready
            || self.state.preview
            || Network::of(&room) != Network::Discord
        {
            return;
        }
        self.shown.retain(|shown| shown.room != room);
        // A window of older messages goes; the room shows its newest again.
        if self.state.detached || self.focusing {
            return;
        }
        self.shown.push(Shown {
            room,
            messages: Arc::new(shelved(&self.state.messages)),
            members: self.state.members.clone(),
        });
        if self.shown.len() > SHOWN {
            self.shown.remove(0);
        }
    }

    /// Each server's last selected channel, by server ID, for an adapter to keep
    /// across runs.
    pub fn last_channels(&self) -> HashMap<String, String> {
        self.last_selected
            .iter()
            .filter_map(|(navigation, room)| match navigation {
                Navigation::Server(server) => Some((server.clone(), room.clone())),
                Navigation::DirectMessages => None,
            })
            .collect()
    }

    /// Restore channels kept from [`Self::last_channels`]. A channel selected
    /// during this run wins, and a channel no longer joined is skipped on navigation.
    pub fn remember_channels(&mut self, channels: HashMap<String, String>) {
        for (server, room) in channels {
            self.last_selected
                .entry(Navigation::Server(server))
                .or_insert(room);
        }
    }
}

#[cfg(test)]
mod tests;
