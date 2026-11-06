//! What this account may do in a Discord server, and how the server notifies
//! it: what server, channel and member menus ask before offering a choice.
//! Each looks at the server the latest snapshot shares, rather than copying
//! roles and settings into every room or member.

use super::*;
use crate::chat::{
    DiscordCommand, NotificationChange, Notifications, Permission, ServerNotifications,
    ServerPreview, ServerRole,
};

#[cfg(feature = "discord")]
use crate::chat::NotifyLevel;
#[cfg(feature = "discord")]
use yapfast_discord::{marked::MarkedServer, notifications::Level, ready::permission};

/// The Discord server and, for a room id, the channel or category a place
/// names: a server id or a room id.
#[cfg(feature = "discord")]
fn place(place: &str) -> Option<(&str, Option<&str>)> {
    match discord::parse_room(place) {
        Some((guild, channel)) => Some((guild, Some(channel))),
        None => discord::parse_server(place).map(|guild| (guild, None)),
    }
}

#[cfg(feature = "discord")]
fn level(level: Level) -> NotifyLevel {
    match level {
        Level::All => NotifyLevel::All,
        Level::Mentions => NotifyLevel::Mentions,
        Level::Nothing => NotifyLevel::Nothing,
    }
}

#[cfg(feature = "discord")]
fn bits(permission: Permission) -> u64 {
    match permission {
        Permission::CreateInvite => permission::CREATE_INSTANT_INVITE,
        Permission::Kick => permission::KICK_MEMBERS,
        Permission::Ban => permission::BAN_MEMBERS,
        Permission::Moderate => permission::MODERATE_MEMBERS,
        Permission::ManageRoles => permission::MANAGE_ROLES,
        Permission::ManageChannels => permission::MANAGE_CHANNELS,
        Permission::ManageServer => permission::MANAGE_GUILD,
        Permission::ChangeNickname => permission::CHANGE_NICKNAME,
        Permission::ManageNicknames => permission::MANAGE_NICKNAMES,
        Permission::ViewChannel => permission::VIEW_CHANNEL,
        Permission::SendMessages => permission::SEND_MESSAGES,
        Permission::ReadHistory => permission::READ_MESSAGE_HISTORY,
        Permission::CreatePublicThreads => permission::CREATE_PUBLIC_THREADS,
        Permission::CreatePrivateThreads => permission::CREATE_PRIVATE_THREADS,
        Permission::ManageMessages => permission::MANAGE_MESSAGES,
        Permission::SendPolls => permission::SEND_POLLS,
        Permission::ManageThreads => permission::MANAGE_THREADS,
        Permission::Connect => permission::CONNECT,
        Permission::Settings => permission::SETTINGS,
        Permission::MuteMembers => permission::MUTE_MEMBERS,
        Permission::DeafenMembers => permission::DEAFEN_MEMBERS,
        Permission::MoveMembers => permission::MOVE_MEMBERS,
        Permission::ViewAuditLog => permission::VIEW_AUDIT_LOG,
        Permission::ManageExpressions => permission::MANAGE_GUILD_EXPRESSIONS,
        Permission::CreateExpressions => permission::CREATE_GUILD_EXPRESSIONS,
    }
}

impl Application {
    /// The Discord server a place names, its channel or category if it
    /// names one, and this account's id.
    #[cfg(feature = "discord")]
    pub(super) fn discord_place<'a>(
        &'a self,
        at: &'a str,
    ) -> Option<(&'a MarkedServer, Option<&'a str>, &'a str)> {
        let snapshot = self.discord.as_ref()?;
        let (guild, channel) = place(at)?;
        let server = snapshot.servers.iter().find(|server| server.id == guild)?;
        Some((server, channel, snapshot.user_id.as_deref()?))
    }

    /// A Discord server's roles, highest first, without @everyone; empty for
    /// a place that isn't one, or a server whose roles aren't known.
    pub fn server_roles(&self, place: &str) -> Vec<ServerRole> {
        #[cfg(feature = "discord")]
        if let Some((server, _, _)) = self.discord_place(place) {
            let mut roles: Vec<ServerRole> = server
                .role_names
                .iter()
                .filter(|(id, _)| **id != server.id)
                .map(|(id, name)| ServerRole {
                    id: id.clone(),
                    name: name.clone(),
                    position: server.role_positions.get(id).copied().unwrap_or_default(),
                    color: server.role_colors.get(id).map(|(_, color)| *color),
                    managed: server.managed_roles.contains(id),
                })
                .collect();
            roles.sort_by(|a, b| b.position.cmp(&a.position).then_with(|| a.id.cmp(&b.id)));
            return roles;
        }
        let _ = place;
        vec![]
    }

    /// Whether this account may do what `permission` names in a Discord
    /// server, by its server id, or in a channel or category of it, by its
    /// room id, as its roles and the channel's overwrites say.
    pub fn may(&self, place: &str, permission: Permission) -> bool {
        #[cfg(feature = "discord")]
        if let Some((server, id, me)) = self.discord_place(place) {
            let roles = &server.marks.roles;
            let channel = id.and_then(|id| server.server.channel(id));
            let held = match (channel, id) {
                (Some(channel), _) => server.channel_permissions(channel, me, roles),
                // A category, or a channel Yapfast doesn't know.
                (None, Some(id)) => server
                    .category_permissions(id, me, roles)
                    .unwrap_or_else(|| server.server_permissions(me, roles)),
                (None, None) => server.server_permissions(me, roles),
            };
            return held & bits(permission) != 0;
        }
        let _ = (place, permission);
        false
    }

    /// Whether this account outranks `user`, holding `roles`, in the Discord
    /// server a place names, as kicking, banning, timing out, renaming or
    /// changing the roles of someone requires; see
    /// [`yapfast_discord::ready::Server::outranks`].
    pub fn outranks(&self, place: &str, user: &str, roles: &[String]) -> bool {
        #[cfg(feature = "discord")]
        if let Some((server, _, me)) = self.discord_place(place) {
            return server.outranks(me, &server.marks.roles, user, roles);
        }
        let _ = (place, user, roles);
        false
    }

    /// The boost level of the Discord server a place names: 0 to 3.
    pub fn premium_tier(&self, place: &str) -> u8 {
        #[cfg(feature = "discord")]
        if let Some((server, _, _)) = self.discord_place(place) {
            return server.premium_tier;
        }
        let _ = place;
        0
    }

    /// Whether this account may give `role` to someone, or take it, in the
    /// Discord server a place names.
    pub fn may_grant(&self, place: &str, role: &str) -> bool {
        #[cfg(feature = "discord")]
        if let Some((server, _, me)) = self.discord_place(place) {
            return server.may_grant(me, &server.marks.roles, role);
        }
        let _ = (place, role);
        false
    }

    /// How a Discord server, by its server id, or a channel or category in
    /// it, by its room id, notifies this account.
    pub fn notifications(&self, place: &str) -> Option<Notifications> {
        #[cfg(feature = "discord")]
        if let Some((server, channel, _)) = self.discord_place(place) {
            let settings = &server.marks.notifications;
            let now = crate::clock::now_ms();
            let server_level = settings.level.unwrap_or(server.default_level);
            return Some(match channel {
                Some(channel) => {
                    let own = settings.of(channel);
                    let category = server
                        .text_channels
                        .iter()
                        .chain(&server.voice_channels)
                        .find(|known| known.id == channel)
                        .and_then(|known| known.parent.as_deref());
                    let inherited = category
                        .and_then(|category| settings.of(category).level)
                        .unwrap_or(server_level);
                    Notifications {
                        muted: own.mute.active(now),
                        muted_until: own.mute.until.filter(|_| own.mute.active(now)),
                        level: own.level.map(level),
                        inherited: level(inherited),
                    }
                }
                None => Notifications {
                    muted: settings.mute.active(now),
                    muted_until: settings.mute.until.filter(|_| settings.mute.active(now)),
                    level: settings.level.map(level),
                    inherited: level(server.default_level),
                },
            });
        }
        let _ = place;
        None
    }

    /// The room id of the category a Discord channel, by its room id, is
    /// listed under, for the menu of the category's heading.
    pub fn category_of(&self, room: &str) -> Option<String> {
        #[cfg(feature = "discord")]
        if let Some((server, Some(channel), _)) = self.discord_place(room) {
            return server
                .text_channels
                .iter()
                .chain(&server.voice_channels)
                .find(|known| known.id == channel)
                .and_then(|known| known.parent.as_deref())
                .filter(|parent| server.categories.contains_key(*parent))
                .map(|parent| discord::room_id(&server.id, parent));
        }
        let _ = room;
        None
    }

    /// How a Discord server notifies this account, with its switches.
    pub fn server_notifications(&self, server: &str) -> Option<ServerNotifications> {
        let own = self.notifications(server)?;
        #[cfg(feature = "discord")]
        if let Some((known, None, _)) = self.discord_place(server) {
            let settings = &known.marks.notifications;
            return Some(ServerNotifications {
                own,
                suppress_everyone: settings.suppress_everyone,
                suppress_roles: settings.suppress_roles,
                hide_muted: settings.hide_muted_channels,
                show_all: known.community.then_some(!settings.follows_only()),
            });
        }
        let _ = own;
        None
    }

    /// Asks Discord to change a server's, channel's or category's
    /// notification settings; snapshots show it once Discord confirms it.
    /// A server's own switches change only for a server, and only while
    /// Discord is connected.
    pub(super) fn change_notifications(
        &mut self,
        place: String,
        change: NotificationChange,
    ) -> Vec<Effect> {
        let server_only = matches!(
            change,
            NotificationChange::HideMuted(_)
                | NotificationChange::SuppressEveryone(_)
                | NotificationChange::SuppressRoles(_)
        );
        let known = discord::parse_room(&place)
            .filter(|(guild, _)| *guild != discord::DM_GUILD)
            .map(|_| false)
            .or_else(|| discord::parse_server(&place).map(|_| true));
        match known {
            Some(server) if self.state.discord_online && (server || !server_only) => {
                vec![Effect::Backend(Command::Discord(
                    DiscordCommand::Notifications { place, change },
                ))]
            }
            _ => vec![],
        }
    }

    /// Asks Discord what a server's card shows, for the card an emoji of it
    /// opens: how many of its members are online, and whether Discover lists
    /// it. A server's answer stands, so a card opened again asks nothing.
    pub(super) fn preview_server(&mut self, server: String) -> Vec<Effect> {
        if self.state.preview
            || !self.state.discord_online
            || self.state.server_previews.contains_key(&server)
            || Network::of(&server) != Network::Discord
        {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ServerPreview { server },
        ))]
    }

    /// Keeps what Discord said a server's card shows, for the cards that open
    /// again, or says it didn't answer.
    pub(super) fn show_server_preview(
        &mut self,
        server: String,
        result: Result<ServerPreview, String>,
    ) {
        match result {
            Ok(preview) => {
                self.state.server_previews.insert(server, preview);
            }
            Err(error) => {
                tracing::warn!(%error, "could not show what a server's card shows");
            }
        }
    }
}
