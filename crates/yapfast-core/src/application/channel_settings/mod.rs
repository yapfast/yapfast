//! A Discord channel's or category's settings, as Discord's client shows
//! them over the window: Overview, its name and, for a text channel, its
//! topic, slowmode and age restriction; and Permissions, its overwrites,
//! which allow or deny each permission to a role or member there. Each page
//! edits a draft against what Discord has, with Discord's unsaved changes
//! bar, as server settings do. See "Channel settings" in docs/discord/server-settings.md.

use super::*;
use crate::chat::{
    ChannelKind, ChannelOverwrite, ChannelSettings, OverwriteChange, Permission, RoleMember,
};

mod overwrites;
mod permissions;

pub use overwrites::{OverwriteConfirm, OverwriteRules, OverwriteState, Overwrites};
pub use permissions::{channel_groups, channel_permission};

/// The bit of Manage Permissions, which a channel's overwrites call
/// Discord's Manage Roles.
const MANAGE_ROLES: u32 = 28;
const VIEW_CHANNEL: u64 = 1 << 10;
const CONNECT: u64 = 1 << 20;

/// A page of a channel's settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChannelSection {
    #[default]
    Overview,
    Permissions,
}

/// A Discord channel's or category's settings, while they're open.
#[derive(Clone, Debug)]
pub struct ChannelSettingsScreen {
    /// Its room id.
    pub room: String,
    /// The server's id.
    pub server: String,
    pub kind: ChannelKind,
    pub section: ChannelSection,
    /// Overview's settings as Discord has them, and the draft; none while
    /// Discord sends a text channel's.
    pub overview: Option<(ChannelSettings, ChannelSettings)>,
    pub permissions: Overwrites,
    /// Discord is taking a change.
    pub saving: bool,
    pub error: Option<String>,
    /// How many times leaving was refused for unsaved changes.
    pub nudge: u32,
}

/// What a channel's settings were given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelInput {
    Section(ChannelSection),
    /// Overview's fields: the name, topic, slowmode in seconds, and age
    /// restriction.
    Name(String),
    Topic(String),
    Slowmode(u32),
    Nsfw(bool),
    /// Shows a role's or member's overwrite, by its id.
    Select(String),
    /// Sets a permission, by its bit number, in the shown overwrite.
    Set(u32, OverwriteState),
    /// Makes the channel private, or not.
    Private(bool),
    /// Opens the picker of roles and members to add, or closes it.
    Adding(bool),
    /// Members whose names start with this, for the picker.
    FindMembers(String),
    /// Adds a role, or a member with their name, at once.
    Add(String, Option<RoleMember>),
    /// Asks before taking the shown overwrite off.
    Remove,
    /// Asks before giving the channel its category's overwrites.
    Sync,
    Confirm,
    Cancel,
    Undo,
    Save,
    Close,
    /// Delete Channel, last in the sidebar: closes the settings and asks
    /// to delete the channel, as Discord's does.
    Delete,
}

impl ChannelSettingsScreen {
    /// Whether a page's draft differs from what Discord has.
    pub fn changed(&self) -> bool {
        self.overview_change().is_some() || self.permissions.changed()
    }

    /// Overview's draft, cleaned up as Discord would take it, if it changed.
    pub fn overview_change(&self) -> Option<ChannelSettings> {
        let (original, draft) = self.overview.as_ref()?;
        let draft = ChannelSettings {
            name: channel_name(&draft.name, self.kind),
            ..draft.clone()
        };
        (draft != *original).then_some(draft)
    }

    /// Why Overview's draft can't be saved as it is.
    pub fn problem(&self) -> Option<String> {
        let (_, draft) = self.overview.as_ref()?;
        let name = channel_name(&draft.name, self.kind);
        if name.is_empty() {
            return Some("Give it a name.".into());
        }
        if name.chars().count() > places::CHANNEL_NAME_LIMIT {
            return Some(format!(
                "Names can be up to {} characters.",
                places::CHANNEL_NAME_LIMIT
            ));
        }
        if draft.topic.chars().count() > places::CHANNEL_TOPIC_LIMIT {
            return Some(format!(
                "Topics can be up to {} characters.",
                places::CHANNEL_TOPIC_LIMIT
            ));
        }
        None
    }

    /// Whether the draft may be saved.
    pub fn ready(&self) -> bool {
        !self.saving && self.problem().is_none()
    }
}

/// A channel's name as Discord keeps it: trimmed, and for a text channel
/// lowercase with dashes for spaces.
fn channel_name(name: &str, kind: ChannelKind) -> String {
    let name = name.trim();
    if kind == ChannelKind::Text {
        name.to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-")
    } else {
        name.to_owned()
    }
}

impl Application {
    /// The pages of a channel's settings this account may open.
    pub fn channel_sections(&self, room: &str) -> Vec<ChannelSection> {
        let mut sections = vec![];
        if !self.state.discord_online {
            return sections;
        }
        if self.may(room, Permission::ManageChannels) {
            sections.push(ChannelSection::Overview);
        }
        if self.may(room, Permission::ManageRoles) {
            sections.push(ChannelSection::Permissions);
        }
        sections
    }

    /// Opens a Discord channel's or category's settings from its menu, at
    /// their first page.
    pub(super) fn open_channel_settings(&mut self, room: String) -> Vec<Effect> {
        if !self.menu_choices(&room).contains(&MenuChoice::EditChannel) {
            return vec![];
        }
        let Some(&section) = self.channel_sections(&room).first() else {
            return vec![];
        };
        let Some(server) = discord::parse_room(&room).map(|(guild, _)| discord::server_id(guild))
        else {
            return vec![];
        };
        let kind = self.channel_kind(&room);
        let overview = self
            .channel_sections(&room)
            .contains(&ChannelSection::Overview);
        let original = self.snapshot_overwrites(&room).unwrap_or_default();
        let mut screen = ChannelSettingsScreen {
            room: room.clone(),
            server,
            kind,
            section,
            overview: None,
            permissions: Overwrites::new(original, self.everyone_of(&room)),
            saving: false,
            error: None,
            nudge: 0,
        };
        // Only a text channel has more than a name to load, and only for
        // Overview.
        let load = overview && kind == ChannelKind::Text;
        if !load {
            let settings = ChannelSettings {
                name: self.channel_title(&room),
                ..Default::default()
            };
            screen.overview = Some((settings.clone(), settings));
        }
        let names = screen.permissions.unnamed();
        self.state.channel_settings = Some(screen);
        let mut effects = vec![];
        if load {
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::ChannelSettings { room: room.clone() },
            )));
        }
        if !names.is_empty() {
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::OverwriteNames {
                    room,
                    members: names,
                },
            )));
        }
        effects
    }

    /// What a room is: a text or voice channel, or a category.
    pub(super) fn channel_kind(&self, room: &str) -> ChannelKind {
        if self.state.is_category(room) {
            return ChannelKind::Category;
        }
        let voice = self
            .state
            .rooms
            .iter()
            .any(|known| known.id == room && known.voice);
        if voice {
            ChannelKind::Voice
        } else {
            ChannelKind::Text
        }
    }

    /// A channel's or category's name, as the sidebar lists it.
    pub fn channel_title(&self, room: &str) -> String {
        if let Some(listed) = self.state.rooms.iter().find(|known| known.id == room) {
            return listed.name.clone();
        }
        discord::parse_room(room)
            .and_then(|(guild, _)| {
                self.categories(&discord::server_id(guild))
                    .into_iter()
                    .find(|known| known.id == room)
            })
            .map(|category| category.name)
            .unwrap_or_default()
    }

    /// The Discord id of @everyone in the server a room is in: the server's.
    fn everyone_of(&self, room: &str) -> String {
        discord::parse_room(room)
            .map(|(guild, _)| guild.to_owned())
            .unwrap_or_default()
    }

    /// A channel's or category's overwrites, as the latest snapshot has
    /// them; none for one it doesn't know.
    pub(super) fn snapshot_overwrites(&self, room: &str) -> Option<Vec<ChannelOverwrite>> {
        #[cfg(feature = "discord")]
        if let Some((server, Some(id), _)) = self.discord_place(room) {
            let overwrites = match server.server.channel(id) {
                Some(channel) => &channel.overwrites,
                None => &server.categories.get(id)?.overwrites,
            };
            return Some(
                overwrites
                    .iter()
                    .map(|overwrite| ChannelOverwrite {
                        id: overwrite.id.clone(),
                        member: overwrite.member,
                        allow: overwrite.allow,
                        deny: overwrite.deny,
                    })
                    .collect(),
            );
        }
        let _ = room;
        None
    }

    /// What this account may change in a channel's overwrites, as Discord's
    /// client works it out.
    pub fn overwrite_rules(&self, room: &str) -> OverwriteRules {
        #[cfg(feature = "discord")]
        if let Some((server, Some(_), me)) = self.discord_place(room) {
            use yapfast_discord::ready::permission;
            let roles = &server.marks.roles;
            let held = server.server_permissions(me, roles);
            // Manage Permissions as the channel's own overwrites give it,
            // without the server's.
            let overwrites = self.snapshot_overwrites(room).unwrap_or_default();
            let mut given = 0u64;
            let mut apply = |overwrite: &ChannelOverwrite| {
                given = (given & !overwrite.deny) | overwrite.allow;
            };
            if let Some(everyone) = overwrites
                .iter()
                .find(|known| !known.member && known.id == server.id)
            {
                apply(everyone);
            }
            let (allow, deny) = overwrites
                .iter()
                .filter(|known| !known.member && roles.contains(&known.id))
                .fold((0, 0), |(allow, deny), known| {
                    (allow | known.allow, deny | known.deny)
                });
            apply(&ChannelOverwrite {
                allow,
                deny,
                ..Default::default()
            });
            if let Some(mine) = overwrites
                .iter()
                .find(|known| known.member && known.id == me)
            {
                apply(mine);
            }
            return OverwriteRules {
                me: me.to_owned(),
                all: held == permission::ALL,
                granted: given & (1 << MANAGE_ROLES) != 0,
                held,
            };
        }
        let _ = room;
        OverwriteRules::default()
    }

    /// Whether this account could still do what `bit` allows in a channel
    /// with `overwrites`; Discord's client refuses a change after which it
    /// couldn't.
    fn keeps(&self, room: &str, overwrites: &[ChannelOverwrite], bit: u32) -> bool {
        #[cfg(feature = "discord")]
        if let Some((server, Some(_), me)) = self.discord_place(room) {
            let overwrites: Vec<_> = overwrites
                .iter()
                .map(|overwrite| yapfast_discord::ready::Overwrite {
                    id: overwrite.id.clone(),
                    member: overwrite.member,
                    allow: overwrite.allow,
                    deny: overwrite.deny,
                })
                .collect();
            return server.overwritten(&overwrites, me, &server.marks.roles) & (1 << bit) != 0;
        }
        let _ = (room, overwrites, bit);
        true
    }

    /// The name an overwrite goes by: its role's, or its member's.
    pub fn overwrite_name(&self, room: &str, overwrite: &ChannelOverwrite) -> String {
        let Some(screen) = self.state.channel_settings.as_ref() else {
            return overwrite.id.clone();
        };
        if overwrite.member {
            return screen
                .permissions
                .names
                .iter()
                .find(|known| known.id == overwrite.id)
                .map_or_else(|| overwrite.id.clone(), |known| known.name.clone());
        }
        if overwrite.id == screen.permissions.everyone {
            return "@everyone".into();
        }
        self.server_roles(room)
            .into_iter()
            .find(|role| role.id == overwrite.id)
            .map_or_else(|| overwrite.id.clone(), |role| role.name)
    }

    /// The channel's overwrites as Channel Permissions lists them:
    /// @everyone's first, then roles' highest first, then members' by name.
    pub fn listed_overwrites(&self) -> Vec<ChannelOverwrite> {
        let Some(screen) = self.state.channel_settings.as_ref() else {
            return vec![];
        };
        let page = &screen.permissions;
        let roles = self.server_roles(&screen.room);
        let mut listed = page.draft();
        if !listed.iter().any(|known| page.is_everyone(known)) {
            listed.push(ChannelOverwrite {
                id: page.everyone.clone(),
                ..Default::default()
            });
        }
        let rank = |overwrite: &ChannelOverwrite| -> (u8, i64, String) {
            if page.is_everyone(overwrite) {
                return (0, 0, String::new());
            }
            if overwrite.member {
                let name = page
                    .names
                    .iter()
                    .find(|known| known.id == overwrite.id)
                    .map_or_else(|| overwrite.id.clone(), |known| known.name.to_lowercase());
                return (2, 0, name);
            }
            let position = roles
                .iter()
                .find(|role| role.id == overwrite.id)
                .map_or(0, |role| role.position);
            (1, -position, overwrite.id.clone())
        };
        listed.sort_by_key(rank);
        listed
    }

    /// The category a channel is in, if this account may change the
    /// category's permissions too, as Discord's sync notice needs: its room
    /// id and name.
    pub fn sync_category(&self, room: &str) -> Option<(String, String)> {
        let category = self.category_of(room)?;
        if !self.may(&category, Permission::ManageRoles) {
            return None;
        }
        let name = self.channel_title(&category);
        Some((category, name))
    }

    /// Whether a channel's overwrites match its category's, as Discord
    /// calls a channel synced; one that changes nothing, as Sync Now gives
    /// @everyone, doesn't count.
    pub fn synced(&self, room: &str) -> bool {
        self.category_of(room)
            .is_none_or(|category| self.same_overwrites(room, &category))
    }

    /// Whether two channels' or categories' overwrites match, by their room
    /// ids, as [`Self::synced`] compares them.
    pub(super) fn same_overwrites(&self, room: &str, other: &str) -> bool {
        let sorted = |mut overwrites: Vec<ChannelOverwrite>| {
            overwrites.retain(|known| known.allow != 0 || known.deny != 0);
            overwrites.sort_by(|a, b| a.id.cmp(&b.id));
            overwrites
        };
        let own = self.snapshot_overwrites(room).map(sorted);
        let theirs = self.snapshot_overwrites(other).map(sorted);
        own == theirs
    }

    pub(super) fn channel_input(&mut self, input: ChannelInput) -> Vec<Effect> {
        let Some(room) = self
            .state
            .channel_settings
            .as_ref()
            .map(|screen| screen.room.clone())
        else {
            return vec![];
        };
        if input == ChannelInput::Delete {
            return self.delete_from_channel_settings(room);
        }
        let sections = self.channel_sections(&room);
        let rules = self.overwrite_rules(&room);
        let synced = self.synced(&room);
        let category = self.category_of(&room);
        let category_overwrites = category
            .as_deref()
            .and_then(|category| self.snapshot_overwrites(category));
        // Discord's client refuses a change that would take a permission
        // from this account in the channel, unless it has every one.
        let refusal = match &input {
            ChannelInput::Set(bit, state) if !rules.all => {
                self.state.channel_settings.as_ref().and_then(|screen| {
                    let page = &screen.permissions;
                    let changed = page.with(&page.shown(), *bit, *state);
                    let mut draft = page.draft();
                    draft.retain(|known| known.id != changed.id);
                    draft.push(changed);
                    (!self.keeps(&room, &draft, *bit)).then(|| page.selected.clone())
                })
            }
            _ => None,
        };
        let refused_name = refusal.map(|id| {
            let shown = self
                .state
                .channel_settings
                .as_ref()
                .map(|screen| screen.permissions.shown())
                .unwrap_or(ChannelOverwrite {
                    id,
                    ..Default::default()
                });
            self.overwrite_name(&room, &shown)
        });
        let Some(screen) = self.state.channel_settings.as_mut() else {
            return vec![];
        };
        let changed = screen.changed();
        let command = |change: OverwriteChange| {
            vec![Effect::Backend(Command::Discord(
                DiscordCommand::ChangeOverwrites {
                    room: room.clone(),
                    change,
                },
            ))]
        };
        let editable = !screen.saving && sections.contains(&ChannelSection::Permissions);
        match input {
            ChannelInput::Section(section) => {
                if section == screen.section || !sections.contains(&section) {
                    return vec![];
                }
                if changed || screen.saving {
                    screen.nudge = screen.nudge.wrapping_add(1);
                    return vec![];
                }
                screen.section = section;
                screen.error = None;
            }
            ChannelInput::Name(text) => {
                if let Some((_, draft)) = &mut screen.overview {
                    draft.name = text;
                }
            }
            ChannelInput::Topic(text) => {
                if let Some((_, draft)) = &mut screen.overview {
                    draft.topic = text;
                }
            }
            ChannelInput::Slowmode(seconds) => {
                if let Some((_, draft)) = &mut screen.overview
                    && places::SLOWMODES.contains(&seconds)
                {
                    draft.slowmode = seconds;
                }
            }
            ChannelInput::Nsfw(on) => {
                if let Some((_, draft)) = &mut screen.overview {
                    draft.nsfw = on;
                }
            }
            ChannelInput::Select(id) => {
                let page = &mut screen.permissions;
                if page.draft().iter().any(|known| known.id == id) || id == page.everyone {
                    page.selected = id;
                }
            }
            ChannelInput::Set(bit, state) => {
                if !editable || rules.locked(bit) {
                    return vec![];
                }
                if let Some(name) = refused_name {
                    screen.permissions.confirm = Some(OverwriteConfirm::DenySelf(name));
                    return vec![];
                }
                let page = &mut screen.permissions;
                let changed = page.with(&page.shown(), bit, state);
                page.edit(changed);
            }
            ChannelInput::Private(on) => {
                let access = match screen.kind {
                    ChannelKind::Voice => VIEW_CHANNEL | CONNECT,
                    _ => VIEW_CHANNEL,
                };
                let locked = (0..64).any(|bit| access & (1 << bit) != 0 && rules.locked(bit));
                if !editable || locked {
                    return vec![];
                }
                screen.permissions.make_private(on, access, &rules);
            }
            ChannelInput::Adding(open) => {
                if !editable {
                    return vec![];
                }
                screen.permissions.adding = open.then(Default::default);
            }
            ChannelInput::FindMembers(query) => {
                let Some(adding) = &mut screen.permissions.adding else {
                    return vec![];
                };
                adding.query = query.clone();
                adding.found.clear();
                if query.trim().is_empty() {
                    return vec![];
                }
                return vec![Effect::Backend(Command::Discord(
                    DiscordCommand::RoleAction {
                        server: screen.server.clone(),
                        action: crate::chat::RoleAction::FindMembers(query),
                    },
                ))];
            }
            ChannelInput::Add(id, member) => {
                let page = &mut screen.permissions;
                if !editable
                    || page.adding.is_none()
                    || page.draft().iter().any(|known| known.id == id)
                {
                    return vec![];
                }
                let overwrite = ChannelOverwrite {
                    id: id.clone(),
                    member: member.is_some(),
                    ..Default::default()
                };
                if let Some(member) = member
                    && !page.names.iter().any(|known| known.id == member.id)
                {
                    page.names.push(member);
                }
                page.adding = None;
                page.pending = Some(OverwriteChange::Put(vec![overwrite.clone()]));
                page.selected = id;
                screen.saving = true;
                screen.error = None;
                return command(OverwriteChange::Put(vec![overwrite]));
            }
            ChannelInput::Remove => {
                let page = &mut screen.permissions;
                if editable && page.selected != page.everyone && page.has(&page.selected) {
                    page.confirm = Some(OverwriteConfirm::Remove(page.selected.clone()));
                }
            }
            ChannelInput::Sync => {
                if editable && !synced && category_overwrites.is_some() {
                    screen.permissions.confirm = Some(OverwriteConfirm::Sync);
                }
            }
            ChannelInput::Cancel => screen.permissions.confirm = None,
            ChannelInput::Confirm => {
                // Asked before Discord went away: it waits for Discord.
                if !sections.contains(&ChannelSection::Permissions) {
                    return vec![];
                }
                let confirm = screen.permissions.confirm.take();
                let change = match confirm {
                    Some(OverwriteConfirm::Remove(id)) => OverwriteChange::Delete(id),
                    Some(OverwriteConfirm::Sync) => {
                        let mut theirs = category_overwrites.unwrap_or_default();
                        let everyone = screen.permissions.everyone.clone();
                        if !theirs
                            .iter()
                            .any(|known| !known.member && known.id == everyone)
                        {
                            theirs.push(ChannelOverwrite {
                                id: everyone,
                                ..Default::default()
                            });
                        }
                        OverwriteChange::Sync(theirs)
                    }
                    Some(OverwriteConfirm::DenySelf(_)) | None => return vec![],
                };
                if changed || screen.saving {
                    screen.nudge = screen.nudge.wrapping_add(1);
                    return vec![];
                }
                screen.permissions.pending = Some(change.clone());
                screen.saving = true;
                screen.error = None;
                return command(change);
            }
            ChannelInput::Undo => {
                if screen.saving {
                    return vec![];
                }
                if let Some((original, draft)) = &mut screen.overview {
                    *draft = original.clone();
                }
                screen.permissions.undo();
                screen.error = None;
            }
            ChannelInput::Save => return self.save_channel_settings(),
            ChannelInput::Close => {
                if changed || screen.saving {
                    screen.nudge = screen.nudge.wrapping_add(1);
                    return vec![];
                }
                self.state.channel_settings = None;
            }
            ChannelInput::Delete => {}
        }
        vec![]
    }

    /// Whether channel settings show Delete Channel: as the channel's menu
    /// offers it.
    pub fn may_delete_channel(&self, room: &str) -> bool {
        self.menu_choices(room).contains(&MenuChoice::DeleteChannel)
    }

    fn delete_from_channel_settings(&mut self, room: String) -> Vec<Effect> {
        let may = self.may_delete_channel(&room);
        let Some(screen) = self.state.channel_settings.as_mut() else {
            return vec![];
        };
        if screen.changed() || screen.saving {
            screen.nudge = screen.nudge.wrapping_add(1);
            return vec![];
        }
        if !may {
            return vec![];
        }
        self.state.channel_settings = None;
        self.dispatch(Action::AskChannel(ChannelAsk::Delete(room)))
    }

    /// Saves the open page's draft: Overview's fields, or the overwrites
    /// changed, one at a time.
    fn save_channel_settings(&mut self) -> Vec<Effect> {
        let Some(room) = self
            .state
            .channel_settings
            .as_ref()
            .map(|screen| &screen.room)
        else {
            return vec![];
        };
        let me = self.overwrite_rules(room).me;
        let online = self.state.discord_online;
        let Some(screen) = self.state.channel_settings.as_mut() else {
            return vec![];
        };
        if !screen.ready() || !online {
            return vec![];
        }
        let room = screen.room.clone();
        if let Some(settings) = screen.overview_change() {
            screen.saving = true;
            screen.error = None;
            return vec![Effect::Backend(Command::Discord(
                DiscordCommand::EditChannel {
                    room,
                    kind: screen.kind,
                    settings: Box::new(settings),
                },
            ))];
        }
        if screen.permissions.changed() {
            let change = OverwriteChange::Put(screen.permissions.to_save(&me));
            screen.permissions.pending = Some(change.clone());
            screen.saving = true;
            screen.error = None;
            return vec![Effect::Backend(Command::Discord(
                DiscordCommand::ChangeOverwrites { room, change },
            ))];
        }
        vec![]
    }

    /// Takes a text channel's settings for Overview, if they're open.
    pub(super) fn channel_settings_loaded(
        &mut self,
        room: &str,
        result: Result<ChannelSettings, String>,
    ) {
        let Some(screen) = self
            .state
            .channel_settings
            .as_mut()
            .filter(|screen| screen.room == room && screen.overview.is_none())
        else {
            return;
        };
        match result {
            Ok(settings) => screen.overview = Some((settings.clone(), settings)),
            Err(error) => screen.error = Some(error),
        }
    }

    /// Takes Discord's answer to Overview's save, if it was the settings
    /// that saved. Whether it was.
    pub(super) fn channel_settings_saved(
        &mut self,
        result: &Result<Option<String>, String>,
    ) -> bool {
        let Some(screen) = self
            .state
            .channel_settings
            .as_mut()
            .filter(|screen| screen.saving && screen.permissions.pending.is_none())
        else {
            return false;
        };
        screen.saving = false;
        match result {
            Ok(_) => {
                if let Some(saved) = screen.overview_change()
                    && let Some((original, draft)) = &mut screen.overview
                {
                    *draft = saved.clone();
                    *original = saved;
                }
            }
            Err(error) => screen.error = Some(error.clone()),
        }
        true
    }

    pub(super) fn overwrites_changed(&mut self, room: String, result: Result<(), String>) {
        let Some(screen) = self
            .state
            .channel_settings
            .as_mut()
            .filter(|screen| screen.room == room && screen.saving)
        else {
            return;
        };
        screen.saving = false;
        let change = screen.permissions.pending.take();
        match (result, change) {
            (Ok(()), Some(change)) => screen.permissions.took(change),
            (Ok(()), None) => {}
            (Err(error), _) => {
                screen.error = Some(error);
                // A role or member Add put in isn't there after all.
                let page = &mut screen.permissions;
                if !page.has(&page.selected) {
                    page.selected = page.everyone.clone();
                }
            }
        }
    }

    pub(super) fn overwrite_names(&mut self, room: String, members: Vec<RoleMember>) {
        if let Some(screen) = self
            .state
            .channel_settings
            .as_mut()
            .filter(|screen| screen.room == room)
        {
            for member in members {
                let page = &mut screen.permissions;
                page.names.retain(|known| known.id != member.id);
                page.names.push(member);
            }
        }
    }

    /// Takes members the picker found, if it's still asking for them.
    /// Whether it was.
    pub(super) fn overwrite_candidates(
        &mut self,
        server: &str,
        query: &str,
        members: &[RoleMember],
    ) -> bool {
        let Some(adding) = self
            .state
            .channel_settings
            .as_mut()
            .filter(|screen| screen.server == server)
            .and_then(|screen| screen.permissions.adding.as_mut())
            .filter(|adding| adding.query == query)
        else {
            return false;
        };
        adding.found = members.to_vec();
        true
    }

    /// Keeps the open settings to what this account may still change, as
    /// a new snapshot says, and takes overwrites changed elsewhere.
    #[cfg(feature = "discord")]
    pub(super) fn follow_channel_settings(&mut self) -> Vec<Effect> {
        // While Discord is away nothing may change, but the draft waits
        // for it rather than closing with the settings.
        if !self.state.discord_online {
            return vec![];
        }
        let Some(room) = self
            .state
            .channel_settings
            .as_ref()
            .map(|screen| screen.room.clone())
        else {
            return vec![];
        };
        let sections = self.channel_sections(&room);
        let overwrites = self.snapshot_overwrites(&room);
        let Some(screen) = self.state.channel_settings.as_mut() else {
            return vec![];
        };
        let (Some(overwrites), true) = (overwrites, sections.contains(&screen.section)) else {
            self.state.channel_settings = None;
            return vec![];
        };
        screen.permissions.follow(overwrites);
        let members = screen.permissions.unnamed();
        if members.is_empty() {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::OverwriteNames { room, members },
        ))]
    }
}
