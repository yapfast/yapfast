//! The menus of Discord servers, their categories and their channels: what
//! they offer beyond a conversation's menu, by what this account may do
//! there and how the server notifies it. A server's header opens the same
//! choices as its menu in the rail, in the order Discord's dropdown has.

use super::*;
use crate::chat::{ChannelKind, ChannelSettings, Permission};

/// A Discord category as the channel list heads its channels with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedCategory {
    /// Its room id, which its menu takes.
    pub id: String,
    pub name: String,
    /// No channel sits in it, so it shows as its heading alone.
    pub empty: bool,
}

/// The server menu's choices its header's dropdown lists, in its order.
const HEADER: [MenuChoice; 9] = [
    MenuChoice::Invite,
    MenuChoice::ServerSettings,
    MenuChoice::CreateChannel,
    MenuChoice::CreateCategory,
    MenuChoice::ServerNotifications,
    MenuChoice::EditServerProfile,
    MenuChoice::ShowAllChannels,
    MenuChoice::HideMuted,
    MenuChoice::LeaveServer,
];

/// Asks before leaving a Discord server or group DM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeaveDialog {
    /// The server's or group's id and name.
    pub place: String,
    pub name: String,
    /// A group DM, which its dialog offers to leave `silent`ly, without
    /// telling its other members.
    pub group: bool,
    pub silent: bool,
    pub leaving: bool,
    pub error: Option<String>,
}

/// The slowmodes Discord's channel settings offer, in seconds: off, then
/// 5 seconds to 6 hours.
pub const SLOWMODES: [u32; 14] = [
    0, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 21_600,
];
/// The longest name and topic Discord's channel settings take.
pub(super) const CHANNEL_NAME_LIMIT: usize = 100;
pub(super) const CHANNEL_TOPIC_LIMIT: usize = 1024;

/// A dialog that makes, changes or deletes a Discord channel or category.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelDialog {
    pub ask: ChannelAsk,
    /// The server's id and name.
    pub server: String,
    pub server_name: String,
    /// The name of what it's about: the channel or category, or for one to
    /// make, the category it goes in; empty for none.
    pub name: String,
    /// What it makes, changes or deletes.
    pub kind: ChannelKind,
    /// Its settings as they stand; none while Discord sends them.
    pub settings: Option<ChannelSettings>,
    pub saving: bool,
    pub error: Option<String>,
}

/// What a [`ChannelDialog`] does, with the room id of the channel or
/// category it's about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelAsk {
    /// Make one, inside this category, if any.
    Create {
        parent: Option<String>,
    },
    /// Make a copy of this channel.
    Duplicate(String),
    Delete(String),
}

/// How long an invite lasts, in seconds, as Discord's invite dialog offers:
/// 30 minutes, an hour, 6 and 12 hours, a day, 7 days, and for good.
pub const INVITE_AGES: [u32; 7] = [1800, 3600, 21_600, 43_200, 86_400, 604_800, 0];
/// How many times an invite works, any number first.
pub const INVITE_USES: [u32; 7] = [0, 1, 5, 10, 25, 50, 100];
/// What a new invite dialog's link starts with, as in Discord: 7 days, any
/// number of times.
const FIRST_INVITE: (u32, u32) = (604_800, 0);

/// An invite dialog: where it invites to, and the link Discord made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteDialog {
    /// The server's name.
    pub server: String,
    /// The channel the link lands people in, by its room id, and its name.
    pub room: String,
    pub channel: String,
    pub voice: bool,
    /// How long the link lasts, in seconds, 0 for good, and how many times
    /// it works, 0 for any number.
    pub max_age: u32,
    pub max_uses: u32,
    /// The link, once Discord made it.
    pub link: Option<String>,
    pub making: bool,
    pub error: Option<String>,
}

impl Application {
    /// What the menu of a listed server, Discord category or conversation
    /// offers, in order; nothing for one Yapfast doesn't know. A Discord
    /// server's and its categories' menus offer what this account may do
    /// there, as Discord's do.
    pub fn menu_choices(&self, id: &str) -> Vec<MenuChoice> {
        let listed = self.state.menu_choices(id);
        if self.state.preview {
            return listed;
        }
        if self.discord_server(id) {
            return self.discord_server_choices(id, &listed);
        }
        if self.state.is_category(id) {
            return self.category_choices(id, &listed);
        }
        if self.discord_channel(id) {
            return self.discord_channel_choices(id, &listed);
        }
        let mut listed = listed;
        // A one-to-one Discord DM's menu invites its person.
        if let Some(person) = self.state.rooms.iter().find_map(|room| {
            (room.id == id && room.direct && !room.request && room.network == Network::Discord)
                .then_some(room.person.as_deref())
                .flatten()
        }) {
            self.offer_invite(person, &mut listed);
        }
        listed
    }

    /// What a Discord server's header opens: the choices of its menu that
    /// Discord's dropdown lists, in that order, so the two agree.
    pub fn header_choices(&self, server: &str) -> Vec<MenuChoice> {
        if !self.discord_server(server) {
            return vec![];
        }
        let offered = self.menu_choices(server);
        HEADER
            .into_iter()
            .filter(|choice| offered.contains(choice))
            .collect()
    }

    /// A listed Discord server.
    fn discord_server(&self, id: &str) -> bool {
        self.state
            .servers
            .iter()
            .any(|server| server.discord && server.id == id)
    }

    /// A listed channel of a Discord server.
    fn discord_channel(&self, id: &str) -> bool {
        discord::parse_room(id).is_some_and(|(guild, _)| guild != discord::DM_GUILD)
            && self
                .state
                .rooms
                .iter()
                .any(|room| room.id == id && !room.direct)
    }

    /// The server menu: marking it read, inviting, its notifications, then
    /// copying.
    fn discord_server_choices(&self, id: &str, listed: &[MenuChoice]) -> Vec<MenuChoice> {
        let mut choices = vec![];
        if listed.contains(&MenuChoice::MarkRead) {
            choices.push(MenuChoice::MarkRead);
        }
        if self.state.discord_online && self.invite_channel(id).is_some() {
            choices.push(MenuChoice::Invite);
        }
        // Discord takes changes only while it's connected.
        if self.state.discord_online
            && let Some(notifications) = self.server_notifications(id)
        {
            choices.push(if notifications.own.muted {
                MenuChoice::UnmuteServer
            } else {
                MenuChoice::MuteServer
            });
            choices.push(MenuChoice::ServerNotifications);
            if notifications.show_all.is_some() {
                choices.push(MenuChoice::ShowAllChannels);
            }
            choices.push(MenuChoice::HideMuted);
        }
        if self.state.discord_online && !self.settings_sections(id).is_empty() {
            choices.push(MenuChoice::ServerSettings);
        }
        // One's own profile there, which anyone may change something of.
        if self.state.discord_online && self.state.discord_user.is_some() {
            choices.push(MenuChoice::EditServerProfile);
        }
        if self.manages(id) {
            choices.extend([MenuChoice::CreateChannel, MenuChoice::CreateCategory]);
        }
        if self.state.discord_online && self.may_leave(id) {
            choices.push(MenuChoice::LeaveServer);
        }
        choices.extend([MenuChoice::CopyLink, MenuChoice::CopyId]);
        choices
    }

    /// Whether this account may leave a Discord server: any but one it
    /// owns, which Discord asks to hand on or delete instead.
    fn may_leave(&self, id: &str) -> bool {
        #[cfg(feature = "discord")]
        if let Some((server, None, me)) = self.discord_place(id) {
            return server.owner_id.as_deref() != Some(me);
        }
        let _ = id;
        false
    }

    /// Asks before leaving a Discord server, from its menu.
    pub(super) fn ask_leave(&mut self, server: String) -> Vec<Effect> {
        if !self
            .menu_choices(&server)
            .contains(&MenuChoice::LeaveServer)
        {
            return vec![];
        }
        let name = self
            .state
            .servers
            .iter()
            .find(|known| known.id == server)
            .map(|known| known.name.clone())
            .unwrap_or_default();
        self.state.leave = Some(LeaveDialog {
            place: server,
            name,
            group: false,
            silent: false,
            leaving: false,
            error: None,
        });
        vec![]
    }

    /// Leaves the server or group DM the dialog asked about. A server's
    /// GUILD_DELETE takes it out of the list, and Yapfast to the DMs if it
    /// was open; a group's CHANNEL_DELETE takes it out of the DM list.
    pub(super) fn leave(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.state.leave.as_mut().filter(|dialog| !dialog.leaving) else {
            return vec![];
        };
        dialog.leaving = true;
        dialog.error = None;
        let place = dialog.place.clone();
        let command = if dialog.group {
            DiscordCommand::LeaveGroup {
                room: place,
                silent: dialog.silent,
            }
        } else {
            DiscordCommand::LeaveServer { server: place }
        };
        vec![Effect::Backend(Command::Discord(command))]
    }

    pub(super) fn left(&mut self, place: String, result: Result<(), String>) {
        let Some(dialog) = self
            .state
            .leave
            .as_mut()
            .filter(|dialog| dialog.leaving && dialog.place == place)
        else {
            return;
        };
        match result {
            Ok(()) => self.state.leave = None,
            Err(error) => {
                dialog.leaving = false;
                dialog.error = Some(error);
            }
        }
    }

    /// Whether this account may create a channel in a category, for the "+" on
    /// its heading.
    pub fn may_create_channel(&self, category: &str) -> bool {
        !category.is_empty()
            && !self.state.preview
            && self.state.is_category(category)
            && self.manages(category)
    }

    /// Whether this account may make, change and delete channels in a
    /// Discord server, category or channel, while Discord takes it.
    fn manages(&self, place: &str) -> bool {
        self.state.discord_online && self.may(place, Permission::ManageChannels)
    }

    /// Whether a channel's or category's settings have a page this account
    /// may change: Overview with Manage Channels, Permissions with Manage
    /// Permissions there.
    fn edits(&self, place: &str) -> bool {
        !self.channel_sections(place).is_empty()
    }

    /// A Discord channel's menu: a conversation's, with inviting to it and
    /// pinning it before its link, and its notifications after, as in
    /// Discord's menu.
    fn discord_channel_choices(&self, id: &str, listed: &[MenuChoice]) -> Vec<MenuChoice> {
        let mut choices = listed.to_vec();
        // A thread is neither invited to nor managed as a channel is. Its
        // menu joins or leaves it, after marking it read, as Discord's does.
        if self
            .state
            .rooms
            .iter()
            .any(|room| room.id == id && room.parent.is_some())
        {
            if self.state.discord_online
                && let Some(joined) = self.thread_joined(id)
            {
                let at = usize::from(choices.first() == Some(&MenuChoice::MarkRead));
                choices.insert(
                    at,
                    if joined {
                        MenuChoice::LeaveThread
                    } else {
                        MenuChoice::JoinThread
                    },
                );
            }
            return choices;
        }
        if self.state.discord_online
            && self.invite_channel(id).is_some()
            && let Some(at) = choices
                .iter()
                .position(|choice| *choice == MenuChoice::CopyLink)
        {
            choices.insert(at, MenuChoice::Invite);
        }
        // Pinning it goes between inviting to it and its link, as in
        // Discord's menu; Discord takes it only while connected.
        if self.state.discord_online
            && let Some(at) = choices
                .iter()
                .position(|choice| *choice == MenuChoice::CopyLink)
        {
            choices.insert(
                at,
                if self.channel_pinned(id) {
                    MenuChoice::UnpinChannel
                } else {
                    MenuChoice::PinChannel
                },
            );
        }
        // Then adding it to the channel list or taking it off, while the
        // list shows only the channels followed.
        if let Some(choice) = self.list_choice(id)
            && let Some(at) = choices
                .iter()
                .position(|choice| *choice == MenuChoice::CopyLink)
        {
            choices.insert(at, choice);
        }
        let notifying: Vec<MenuChoice> = choices
            .extract_if(.., |choice| {
                matches!(
                    choice,
                    MenuChoice::ChannelMute | MenuChoice::ChannelNotifications
                )
            })
            .collect();
        let at = choices
            .iter()
            .position(|choice| *choice == MenuChoice::CopyLink)
            .map_or(choices.len(), |at| at + 1);
        choices.splice(at..at, notifying);
        // Managing it goes after its notifications, before its id.
        let mut managing = vec![];
        if self.edits(id) {
            managing.push(MenuChoice::EditChannel);
        }
        if self.manages(id) {
            managing.push(MenuChoice::DuplicateChannel);
        }
        let around = self
            .category_of(id)
            .or_else(|| discord::parse_room(id).map(|(guild, _)| discord::server_id(guild)));
        if around.is_some_and(|place| self.manages(&place)) {
            managing.push(MenuChoice::CreateChannel);
        }
        if self.manages(id) {
            managing.push(MenuChoice::DeleteChannel);
        }
        let at = choices
            .iter()
            .position(|choice| *choice == MenuChoice::CopyId)
            .unwrap_or(choices.len());
        choices.splice(at..at, managing);
        choices
    }

    /// Whether this account is in a Discord thread, by its room id; none for
    /// a room that isn't a thread Yapfast knows.
    fn thread_joined(&self, room: &str) -> Option<bool> {
        #[cfg(feature = "discord")]
        {
            let (guild, channel) = discord::parse_room(room)?;
            let snapshot = self.discord.as_ref()?;
            let server = snapshot.servers.iter().find(|server| server.id == guild)?;
            server
                .channel(channel)?
                .thread
                .as_ref()
                .map(|thread| thread.joined)
        }
        #[cfg(not(feature = "discord"))]
        {
            let _ = room;
            None
        }
    }

    /// What a Discord thread header's More menu offers, by its room id, as
    /// Discord's does: the thread's menu, less marking it read.
    pub fn thread_more_choices(&self, room: &str) -> Vec<MenuChoice> {
        let mut choices = self.menu_choices(room);
        choices.retain(|choice| *choice != MenuChoice::MarkRead);
        choices
    }

    /// Joins a Discord thread from a menu, or leaves it. It lists, or
    /// stops listing, once Discord confirms it; one left stays open, as in
    /// Discord's client.
    pub(super) fn join_thread(
        &mut self,
        room: String,
        join: bool,
        location: &'static str,
    ) -> Vec<Effect> {
        let choice = if join {
            MenuChoice::JoinThread
        } else {
            MenuChoice::LeaveThread
        };
        if !self.menu_choices(&room).contains(&choice) {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::JoinThread {
                room,
                join,
                location,
            },
        ))]
    }

    /// What a Discord channel's or category's menu, by its room id, offers
    /// for the channel list while it shows only the channels this account
    /// follows, as Discord's client offers it: to stop following the
    /// category a channel is followed through, else to add the channel or
    /// category, or to remove it. None while Discord is away, and for a
    /// thread.
    fn list_choice(&self, id: &str) -> Option<MenuChoice> {
        #[cfg(feature = "discord")]
        if self.state.discord_online
            && let Some((server, Some(channel), _)) = self.discord_place(id)
            && server.follows_only()
        {
            let known = server.server.channel(channel);
            if known.is_some_and(|known| known.thread.is_some()) {
                return None;
            }
            let settings = &server.marks.notifications;
            let category = known.and_then(|known| known.parent.as_deref());
            return Some(
                if category.is_some_and(|category| settings.of(category).followed()) {
                    MenuChoice::UnfollowCategory
                } else if settings.of(channel).followed() {
                    MenuChoice::RemoveFromChannelList
                } else {
                    MenuChoice::AddToChannelList
                },
            );
        }
        let _ = id;
        None
    }

    /// Asks Discord to add a channel or category to the channel list that
    /// shows only those this account follows, or to remove it, from its
    /// menu: its override's flags, as Discord's client writes them. The
    /// list changes when Discord confirms it.
    pub(super) fn follow_channel(&mut self, room: String, follow: bool) -> Vec<Effect> {
        let choice = if follow {
            MenuChoice::AddToChannelList
        } else {
            MenuChoice::RemoveFromChannelList
        };
        if self.list_choice(&room) != Some(choice) {
            return vec![];
        }
        #[cfg(feature = "discord")]
        if let Some((server, Some(channel), _)) = self.discord_place(&room) {
            let flags = server.marks.notifications.of(channel).follow_flags(follow);
            return vec![Effect::Backend(Command::Discord(
                DiscordCommand::FollowChannel { room, flags },
            ))];
        }
        vec![]
    }

    /// Asks Discord to show every channel of a server in its channel list,
    /// or only those this account follows, from the server's menu. Turning
    /// Show All Channels off with nothing followed follows every channel
    /// this account sees, as Discord's client does, so the list doesn't
    /// empty. The list changes when Discord confirms it.
    pub(super) fn show_all_channels(&mut self, server: String, show: bool) -> Vec<Effect> {
        if !self
            .menu_choices(&server)
            .contains(&MenuChoice::ShowAllChannels)
            || self
                .server_notifications(&server)
                .and_then(|known| known.show_all)
                == Some(show)
        {
            return vec![];
        }
        #[cfg(feature = "discord")]
        if let Some((known, None, _)) = self.discord_place(&server) {
            let settings = &known.marks.notifications;
            let flags = settings.show_all_flags(show);
            let follow = if show || settings.follows_any() {
                vec![]
            } else {
                known
                    .text_channels()
                    .chain(known.voice_channels())
                    .filter(|channel| {
                        channel.marks.access != yapfast_discord::ready::Access::Hidden
                    })
                    .map(|channel| {
                        let flags = settings.of(&channel.id).follow_flags(true);
                        (channel.id.clone(), flags)
                    })
                    .collect()
            };
            return vec![Effect::Backend(Command::Discord(
                DiscordCommand::ShowAllChannels {
                    server,
                    flags,
                    follow,
                },
            ))];
        }
        vec![]
    }

    /// Whether this account pinned a Discord channel, by its room id, to
    /// the top of its server's channel list.
    pub fn channel_pinned(&self, room: &str) -> bool {
        self.state
            .servers
            .iter()
            .any(|server| server.pinned.iter().any(|pinned| pinned == room))
    }

    /// Asks Discord to pin a channel to the top of its server's channel
    /// list, or to unpin it, from its menu: its override's flags with the
    /// pin set or cleared, as Discord's client writes them. The list
    /// changes when Discord confirms it.
    pub(super) fn pin_channel(&mut self, room: String, pin: bool) -> Vec<Effect> {
        let choice = if pin {
            MenuChoice::PinChannel
        } else {
            MenuChoice::UnpinChannel
        };
        if !self.menu_choices(&room).contains(&choice) {
            return vec![];
        }
        #[cfg(feature = "discord")]
        if let Some((server, Some(channel), _)) = self.discord_place(&room) {
            let flags = server.marks.notifications.of(channel).pin_flags(pin);
            return vec![Effect::Backend(Command::Discord(
                DiscordCommand::PinChannel { room, flags },
            ))];
        }
        vec![]
    }

    /// The channel an invite from a Discord server's or channel's menu
    /// lands people in, by its room id: the channel itself, or for the
    /// server the open channel, else the first in Discord's order, as
    /// Discord's client picks; each only if this account may invite there.
    pub fn invite_channel(&self, place: &str) -> Option<String> {
        let invitable = |room: &str| {
            self.may(room, chat::Permission::ViewChannel)
                && self.may(room, chat::Permission::CreateInvite)
                && !self.state.is_category(room)
        };
        let server = self
            .state
            .servers
            .iter()
            .find(|server| server.discord && server.id == place);
        let Some(server) = server else {
            return invitable(place).then(|| place.to_owned());
        };
        self.state
            .selected
            .iter()
            .filter(|open| server.channels.contains(open))
            .chain(&server.channels)
            .find(|room| invitable(room))
            .cloned()
    }

    pub(super) fn open_invite(&mut self, place: String) -> Vec<Effect> {
        if !self.menu_choices(&place).contains(&MenuChoice::Invite) {
            return vec![];
        }
        let Some(room) = self.invite_channel(&place) else {
            return vec![];
        };
        let listed = self.state.rooms.iter().find(|known| known.id == room);
        let server = self
            .state
            .servers
            .iter()
            .find(|server| server.channels.contains(&room));
        self.state.invite = Some(InviteDialog {
            server: server.map(|server| server.name.clone()).unwrap_or_default(),
            channel: listed.map(|room| room.name.clone()).unwrap_or_default(),
            voice: listed.is_some_and(|room| room.voice),
            room,
            max_age: FIRST_INVITE.0,
            max_uses: FIRST_INVITE.1,
            link: None,
            making: false,
            error: None,
        });
        self.make_invite(FIRST_INVITE.0, FIRST_INVITE.1)
    }

    /// Asks Discord for a new link for the invite dialog, unless one is on
    /// its way or the choices are none Discord's dialog offers.
    pub(super) fn make_invite(&mut self, max_age: u32, max_uses: u32) -> Vec<Effect> {
        let offered = INVITE_AGES.contains(&max_age) && INVITE_USES.contains(&max_uses);
        let Some(invite) = self
            .state
            .invite
            .as_mut()
            .filter(|invite| !invite.making && offered)
        else {
            return vec![];
        };
        invite.max_age = max_age;
        invite.max_uses = max_uses;
        invite.making = true;
        invite.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::CreateInvite {
                room: invite.room.clone(),
                max_age,
                max_uses,
            },
        ))]
    }

    pub(super) fn invite_made(&mut self, room: String, result: Result<String, String>) {
        let Some(invite) = self
            .state
            .invite
            .as_mut()
            .filter(|invite| invite.room == room && invite.making)
        else {
            return;
        };
        invite.making = false;
        match result {
            Ok(code) => invite.link = Some(format!("https://discord.gg/{code}")),
            Err(error) => invite.error = Some(error),
        }
    }

    /// The Discord servers someone's Invite to server submenu lists, in the
    /// rail's order, as Discord's client lists them: those where this
    /// account may create invites, and has a channel to invite to, but not
    /// the server whose member they're shown as. Discord doesn't leave out
    /// other servers they're in. None for oneself, a bot, or someone
    /// blocked or ignored. Built when their menu opens.
    pub fn invite_servers(&self, user: &str) -> Vec<&ChatServer> {
        #[cfg(feature = "discord")]
        {
            let state = &self.state;
            let listed = |list: &[String]| list.iter().any(|listed| listed == user);
            let bot = self
                .discord
                .as_ref()
                .is_some_and(|snapshot| snapshot.names.read().is_bot(user));
            let person = !state.preview
                && state.discord_online
                && !user.starts_with('@')
                && state.relation(user) != Relation::Me
                && !bot
                && !state.is_blocked(user)
                && !listed(&state.discord_ignored);
            if !person {
                return vec![];
            }
            let shown_in = self
                .member_place(user)
                .and_then(discord::parse_room)
                .map(|(guild, _)| guild);
            state
                .servers
                .iter()
                .map(Arc::as_ref)
                .filter(|server| server.discord && !server.unavailable)
                .filter(|server| discord::parse_server(&server.id) != shown_in)
                .filter(|server| self.may(&server.id, chat::Permission::CreateInvite))
                .filter(|server| self.person_invite_channel(&server.id).is_some())
                .collect()
        }
        #[cfg(not(feature = "discord"))]
        {
            let _ = user;
            vec![]
        }
    }

    /// Puts Invite to server in someone's menu where Discord's has it,
    /// before how this account stands with them, when it lists a server.
    pub(super) fn offer_invite(&self, user: &str, choices: &mut Vec<MenuChoice>) {
        if self.invite_servers(user).is_empty() {
            return;
        }
        let at = choices
            .iter()
            .position(|choice| {
                matches!(
                    choice,
                    MenuChoice::AddFriend
                        | MenuChoice::AcceptFriend
                        | MenuChoice::CancelFriendRequest
                        | MenuChoice::Unfriend
                        | MenuChoice::Ignore
                        | MenuChoice::Unignore
                        | MenuChoice::Block
                        | MenuChoice::Unblock
                        | MenuChoice::CopyId
                )
            })
            .unwrap_or(choices.len());
        choices.insert(at, MenuChoice::InviteToServer);
    }

    /// The channel an invite from someone's menu lands them in, as Discord's
    /// client picks it: of the text and announcement channels this account
    /// may see and invite to, the first by position, else such a voice
    /// channel. Not the server's order in the channel list, which goes by
    /// category first.
    #[cfg(feature = "discord")]
    fn person_invite_channel<'a>(
        &'a self,
        server: &'a str,
    ) -> Option<&'a yapfast_discord::ready::Channel> {
        use yapfast_discord::ready::permission::{CREATE_INSTANT_INVITE, VIEW_CHANNEL};
        let (server, None, me) = self.discord_place(server)? else {
            return None;
        };
        let wanted = VIEW_CHANNEL | CREATE_INSTANT_INVITE;
        let first = |channels: &'a [yapfast_discord::ready::Channel]| {
            channels
                .iter()
                .filter(|channel| {
                    server.channel_permissions(channel, me, &server.marks.roles) & wanted == wanted
                })
                .min_by_key(|channel| (channel.position, &channel.id))
        };
        first(&server.text_channels).or_else(|| first(&server.voice_channels))
    }

    /// Sends someone an invite to a server their menu's submenu lists.
    pub(super) fn invite_to_server(&mut self, user: String, server: String) -> Vec<Effect> {
        #[cfg(feature = "discord")]
        if self
            .invite_servers(&user)
            .iter()
            .any(|listed| listed.id == server)
            && let Some(channel) = self.person_invite_channel(&server)
            && let Some(guild) = discord::parse_server(&server)
        {
            let room = discord::room_id(guild, &channel.id);
            return vec![Effect::Backend(Command::Discord(
                DiscordCommand::SendInvite { user, room },
            ))];
        }
        let _ = (user, server);
        vec![]
    }

    /// Says why an invite didn't go. One that went shows in the DM, which
    /// lists it, as Discord's client shows it.
    pub(super) fn invite_sent(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            self.state.notice = Some(error);
        }
    }

    /// A category's menu: marking its channels read, then its notification
    /// choices, then copying its id, as Discord's has them.
    fn category_choices(&self, id: &str, listed: &[MenuChoice]) -> Vec<MenuChoice> {
        let mut choices = vec![];
        if !self.unread_in_category(id).is_empty() {
            choices.push(MenuChoice::MarkRead);
        }
        // Following it comes before its notifications, as in Discord's menu.
        choices.extend(self.list_choice(id));
        choices.extend_from_slice(listed);
        if self.edits(id) {
            choices.push(MenuChoice::EditChannel);
        }
        if self.manages(id) {
            choices.push(MenuChoice::DeleteChannel);
        }
        choices.push(MenuChoice::CopyId);
        choices
    }

    /// The room ids of a Discord category's listed channels with messages to read.
    pub(super) fn unread_in_category(&self, id: &str) -> Vec<String> {
        #[cfg(feature = "discord")]
        if let Some((server, Some(category), _)) = self.discord_place(id) {
            let inside: HashSet<String> = server
                .text_channels
                .iter()
                .chain(&server.voice_channels)
                .filter(|channel| channel.parent.as_deref() == Some(category))
                .map(|channel| discord::room_id(&server.id, &channel.id))
                .collect();
            return self
                .state
                .rooms
                .iter()
                .filter(|room| inside.contains(&room.id) && rooms::unread(room))
                .map(|room| room.id.clone())
                .collect();
        }
        let _ = id;
        vec![]
    }

    /// A Discord server's categories this account may see, in Discord's
    /// order; none for any other server.
    pub fn categories(&self, server: &str) -> Vec<ListedCategory> {
        #[cfg(feature = "discord")]
        if let Some((known, None, me)) = self.discord_place(server) {
            let roles = &known.marks.roles;
            let mut categories: Vec<_> = known
                .categories
                .iter()
                .filter(|(id, _)| {
                    known
                        .category_permissions(id, me, roles)
                        .is_some_and(|held| {
                            held & yapfast_discord::ready::permission::VIEW_CHANNEL != 0
                        })
                })
                .collect();
            categories.sort_by_key(|(id, category)| (category.position, id.as_str()));
            return categories
                .into_iter()
                .map(|(id, category)| ListedCategory {
                    id: discord::room_id(&known.id, id),
                    name: category.name.clone(),
                    empty: !known
                        .text_channels
                        .iter()
                        .chain(&known.voice_channels)
                        .any(|channel| channel.parent.as_ref() == Some(id)),
                })
                .collect();
        }
        let _ = server;
        vec![]
    }

    /// Opens the dialog that makes a channel of `kind` from a Discord
    /// server's menu, or from a channel's, one like it in its category.
    pub(super) fn new_channel(&mut self, place: String, kind: ChannelKind) -> Vec<Effect> {
        let choice = if kind == ChannelKind::Category {
            MenuChoice::CreateCategory
        } else {
            MenuChoice::CreateChannel
        };
        if !self.menu_choices(&place).contains(&choice) {
            return vec![];
        }
        let parent = self.category_of(&place);
        let name = parent
            .as_deref()
            .and_then(|category| {
                self.categories(&self.server_of(category)?)
                    .into_iter()
                    .find(|known| known.id == category)
            })
            .map(|category| category.name)
            .unwrap_or_default();
        let kind = match self.state.rooms.iter().find(|room| room.id == place) {
            Some(room) if room.voice => ChannelKind::Voice,
            Some(_) => ChannelKind::Text,
            None => kind,
        };
        self.open_channel_dialog(
            &place,
            ChannelAsk::Create { parent },
            name,
            kind,
            Some(ChannelSettings::default()),
        );
        vec![]
    }

    /// Opens a Discord channel's or category's dialog from its menu: to
    /// copy it, or to make sure before deleting it.
    pub(super) fn ask_channel(&mut self, ask: ChannelAsk) -> Vec<Effect> {
        let (room, choice) = match &ask {
            ChannelAsk::Duplicate(room) => (room.clone(), MenuChoice::DuplicateChannel),
            ChannelAsk::Delete(room) => (room.clone(), MenuChoice::DeleteChannel),
            ChannelAsk::Create { .. } => return vec![],
        };
        if !self.menu_choices(&room).contains(&choice) {
            return vec![];
        }
        let kind = self.channel_kind(&room);
        let name = self.channel_title(&room);
        let settings = ChannelSettings {
            name: if matches!(ask, ChannelAsk::Delete(_)) {
                String::new()
            } else {
                name.clone()
            },
            ..Default::default()
        };
        self.open_channel_dialog(&room, ask, name, kind, Some(settings));
        vec![]
    }

    /// The server id of the Discord server a room id names.
    fn server_of(&self, room: &str) -> Option<String> {
        discord::parse_room(room).map(|(guild, _)| discord::server_id(guild))
    }

    fn open_channel_dialog(
        &mut self,
        place: &str,
        ask: ChannelAsk,
        name: String,
        kind: ChannelKind,
        settings: Option<ChannelSettings>,
    ) {
        let server = self.server_of(place).unwrap_or_else(|| place.to_owned());
        let server_name = self
            .state
            .servers
            .iter()
            .find(|known| known.id == server)
            .map(|known| known.name.clone())
            .unwrap_or_default();
        self.state.channel_dialog = Some(ChannelDialog {
            ask,
            server,
            server_name,
            name,
            kind,
            settings,
            saving: false,
            error: None,
        });
    }

    /// Carries out the channel dialog with what it was given: of `kind`, text
    /// or voice, for a channel it makes.
    pub(super) fn save_channel(
        &mut self,
        kind: ChannelKind,
        settings: ChannelSettings,
    ) -> Vec<Effect> {
        let Some(dialog) = self
            .state
            .channel_dialog
            .as_mut()
            .filter(|dialog| !dialog.saving && dialog.settings.is_some())
        else {
            return vec![];
        };
        let kind = match dialog.ask {
            // Making a channel chooses text or voice; a category stays one,
            // and a channel stays what it was.
            ChannelAsk::Create { .. } if dialog.kind != ChannelKind::Category => {
                if kind == ChannelKind::Category {
                    ChannelKind::Text
                } else {
                    kind
                }
            }
            _ => dialog.kind,
        };
        let mut settings = settings;
        settings.name = settings.name.trim().to_owned();
        // Discord's text channel names are lower case, with dashes for spaces.
        if kind == ChannelKind::Text {
            settings.name = settings
                .name
                .to_lowercase()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join("-");
        }
        let deleting = matches!(dialog.ask, ChannelAsk::Delete(_));
        let problem = if deleting {
            None
        } else if settings.name.is_empty() {
            Some("Give it a name.".to_owned())
        } else if settings.name.chars().count() > CHANNEL_NAME_LIMIT {
            Some(format!(
                "Names can be up to {CHANNEL_NAME_LIMIT} characters."
            ))
        } else if settings.topic.chars().count() > CHANNEL_TOPIC_LIMIT {
            Some(format!(
                "Topics can be up to {CHANNEL_TOPIC_LIMIT} characters."
            ))
        } else if !SLOWMODES.contains(&settings.slowmode) {
            Some("Pick one of the slowmodes.".to_owned())
        } else {
            None
        };
        if let Some(problem) = problem {
            dialog.error = Some(problem);
            return vec![];
        }
        dialog.saving = true;
        dialog.error = None;
        let command = match dialog.ask.clone() {
            ChannelAsk::Create { parent } => DiscordCommand::CreateChannel {
                server: dialog.server.clone(),
                kind,
                parent,
                copy_of: None,
                settings: Box::new(settings),
            },
            ChannelAsk::Duplicate(room) => DiscordCommand::CreateChannel {
                server: dialog.server.clone(),
                kind,
                parent: None,
                copy_of: Some(room),
                settings: Box::new(settings),
            },
            ChannelAsk::Delete(room) => DiscordCommand::DeleteChannel { room },
        };
        dialog.kind = kind;
        vec![Effect::Backend(Command::Discord(command))]
    }

    /// Closes the channel dialog once Discord did what it asked, opening a
    /// text channel it made, as Discord's client does; or says why not.
    pub(super) fn channel_saved(&mut self, result: Result<Option<String>, String>) {
        let Some(dialog) = self
            .state
            .channel_dialog
            .as_mut()
            .filter(|dialog| dialog.saving)
        else {
            return;
        };
        match result {
            Ok(made) => {
                if dialog.kind == ChannelKind::Text {
                    self.pending_open = made;
                }
                self.state.channel_dialog = None;
            }
            Err(error) => {
                dialog.saving = false;
                dialog.error = Some(error);
            }
        }
    }
}
