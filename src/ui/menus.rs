//! The menus a right click opens: what they offer, and the choices made.

use yapfast_core::{
    application::{ChannelAsk, MUTE_TIMES, MenuChoice, OwnershipInput, network_id, network_link},
    chat::{
        ChannelKind, MemberChange, Muting, NotificationChange, Notifications, NotifyLevel,
        RelationshipChange,
    },
};

use super::*;

/// How a menu target names a Discord server's header, whose dropdown lists
/// some of the server menu's choices: this, then the server's id.
pub(super) const HEADER: &str = "header:";

/// How a Discord thread header's More button names its menu, before the
/// thread's room id: the header in full view, and the one beside its channel.
pub(super) const TOOLBAR: &str = "thread-toolbar:";
pub(super) const SIDEBAR: &str = "thread-sidebar:";

/// The thread a header's More menu is about, and where Discord's client says
/// a choice in it came from.
fn thread_more(id: &str) -> Option<(&str, &'static str)> {
    id.strip_prefix(TOOLBAR)
        .map(|room| (room, "Toolbar Overflow"))
        .or_else(|| {
            id.strip_prefix(SIDEBAR)
                .map(|room| (room, "Sidebar Overflow"))
        })
}

/// A menu's rows, and the submenu each opens by the row's index: empty for a
/// row without one, and none at all when no row has one, as the window's
/// ContextMenu takes them.
pub(super) type Menu = (Vec<MenuRow>, Vec<Vec<MenuRow>>);

/// What a server's, conversation's or person's menu offers; nothing for one
/// Yapfast doesn't know.
pub(super) fn menu(adapter: &Adapter, id: &str, person: bool) -> Menu {
    if let Some(place) = id.strip_prefix(BELL) {
        return Notify::of(adapter, place)
            .map_or_else(Menu::default, |notify| notify.bell(adapter));
    }
    let icons = &adapter.icons;
    let app = &adapter.app;
    let state = &app.state;
    let in_voice = voice_target(id);
    let thread = thread_more(id);
    let choices = if let Some((room, user)) = in_voice {
        app.voice_person_choices(room, user)
    } else if let Some((room, _)) = thread {
        app.thread_more_choices(room)
    } else if person {
        app.person_choices(id)
    } else if let Some(server) = id.strip_prefix(HEADER) {
        app.header_choices(server)
    } else {
        app.menu_choices(id)
    };
    let id = match (in_voice, thread) {
        (Some((_, user)), _) => user,
        (None, Some((room, _))) => room,
        (None, None) => id.strip_prefix(HEADER).unwrap_or(id),
    };
    let voice_person = in_voice.and_then(|(room, user)| app.voice_person(room, user));
    // Moderation names whom it's about, as Discord's does.
    let named = std::cell::OnceCell::new();
    let name = || {
        named
            .get_or_init(|| {
                state
                    .person(id)
                    .map_or_else(|| "them".into(), |person| person.name)
            })
            .as_str()
    };
    let (room, user) = subjects(state, id, person);
    let in_call = state
        .rooms
        .iter()
        .any(|found| Some(&found.id) == room.as_ref() && !found.people.is_empty());
    let server = state.servers.iter().any(|server| server.id == id);
    let category = !server && state.is_category(id);
    let voice = state
        .rooms
        .iter()
        .any(|found| found.id == id && found.voice);
    let mut group = None;
    let mut rows: Vec<MenuRow> = choices
        .into_iter()
        .map(|choice| {
            let (text, icon) = match choice {
                MenuChoice::Timeout => (format!("Time out {}", name()), &icons.timeout),
                MenuChoice::Kick => (format!("Kick {}", name()), &icons.kick),
                MenuChoice::Ban => (format!("Ban {}", name()), &icons.ban),
                MenuChoice::Invite if server => ("Invite to server".into(), &icons.befriend),
                MenuChoice::CreateChannel if server => ("Create channel".into(), &icons.create),
                MenuChoice::CreateChannel if voice => {
                    ("Create voice channel".into(), &icons.create)
                }
                MenuChoice::EditChannel if category => ("Edit category".into(), &icons.edit),
                MenuChoice::DeleteChannel if category => ("Delete category".into(), &icons.delete),
                choice => {
                    let (text, icon) = label(icons, choice, in_call);
                    (text.to_owned(), icon)
                }
            };
            // A line goes between groups: what it does, then how this
            // account stands with someone, then notifications, roles,
            // copying, and moderation; a channel's management, then deleting it.
            let starts = match choice {
                MenuChoice::ServerNickname
                | MenuChoice::InviteToServer
                | MenuChoice::AddFriend
                | MenuChoice::AcceptFriend
                | MenuChoice::CancelFriendRequest
                | MenuChoice::Unfriend
                | MenuChoice::Ignore
                | MenuChoice::Unignore
                | MenuChoice::Block
                | MenuChoice::Unblock => 1,
                MenuChoice::Mute
                | MenuChoice::Unmute
                | MenuChoice::ChannelMute
                | MenuChoice::ChannelNotifications
                | MenuChoice::MuteServer
                | MenuChoice::UnmuteServer
                | MenuChoice::ServerNotifications
                | MenuChoice::ShowAllChannels
                | MenuChoice::HideMuted => 2,
                MenuChoice::CopyLink
                | MenuChoice::CopyId
                | MenuChoice::JoinThread
                | MenuChoice::LeaveThread
                | MenuChoice::Invite
                | MenuChoice::PinChannel
                | MenuChoice::UnpinChannel
                | MenuChoice::AddToChannelList
                | MenuChoice::RemoveFromChannelList
                | MenuChoice::UnfollowCategory => 3,
                MenuChoice::Roles | MenuChoice::MoveTo => 4,
                MenuChoice::LocalMute
                | MenuChoice::SoundboardMute
                | MenuChoice::SelfMute
                | MenuChoice::SelfDeafen => 8,
                MenuChoice::ServerMute
                | MenuChoice::ServerDeafen
                | MenuChoice::Disconnect
                | MenuChoice::Timeout
                | MenuChoice::RemoveTimeout
                | MenuChoice::Kick
                | MenuChoice::Ban => 5,
                // Its own group, as in Discord's.
                MenuChoice::TransferOwnership => 9,
                MenuChoice::ServerSettings
                | MenuChoice::EditServerProfile
                | MenuChoice::CreateChannel
                | MenuChoice::CreateCategory
                | MenuChoice::EditChannel
                | MenuChoice::DuplicateChannel => 6,
                MenuChoice::DeleteChannel | MenuChoice::LeaveServer | MenuChoice::LeaveGroup => 7,
                _ => 0,
            };
            let separated = group.replace(starts).is_some_and(|last| last != starts);
            let notifications = || app.server_notifications(id);
            let subtitle = match choice {
                MenuChoice::UnmuteServer => notifications()
                    .and_then(|notifications| notifications.own.muted_until)
                    .map(|until| muted_until(until / 1000, adapter.layout.twelve_hour_clock))
                    .unwrap_or_default(),
                MenuChoice::ServerNotifications => notifications()
                    .map(|notifications| {
                        let own = notifications.own;
                        level_name(own.level.unwrap_or(own.inherited)).to_owned()
                    })
                    .unwrap_or_default(),
                _ => String::new(),
            };
            let checkbox = matches!(
                choice,
                MenuChoice::ShowAllChannels
                    | MenuChoice::HideMuted
                    | MenuChoice::LocalMute
                    | MenuChoice::SoundboardMute
                    | MenuChoice::SelfMute
                    | MenuChoice::SelfDeafen
                    | MenuChoice::ServerMute
                    | MenuChoice::ServerDeafen
            );
            let checked = match choice {
                MenuChoice::ShowAllChannels => {
                    notifications().is_some_and(|known| known.show_all == Some(true))
                }
                MenuChoice::HideMuted => notifications().is_some_and(|known| known.hide_muted),
                MenuChoice::LocalMute => yapfast_core::volume::muted(id),
                MenuChoice::SoundboardMute => yapfast_core::volume::soundboard_muted(id),
                MenuChoice::SelfMute => state.voice.muted,
                MenuChoice::SelfDeafen => state.voice.deafened,
                MenuChoice::ServerMute => voice_person.is_some_and(|person| person.server_muted),
                MenuChoice::ServerDeafen => {
                    voice_person.is_some_and(|person| person.server_deafened)
                }
                _ => false,
            };
            MenuRow {
                id: choice_id(choice).into(),
                text: text.into(),
                subtitle: subtitle.into(),
                icon: icon.clone(),
                danger: matches!(
                    choice,
                    MenuChoice::Unfriend
                        | MenuChoice::Block
                        | MenuChoice::Timeout
                        | MenuChoice::Kick
                        | MenuChoice::Ban
                        | MenuChoice::ServerMute
                        | MenuChoice::ServerDeafen
                        | MenuChoice::Disconnect
                        | MenuChoice::DeleteChannel
                        | MenuChoice::LeaveServer
                        | MenuChoice::LeaveGroup
                        | MenuChoice::TransferOwnership
                ),
                separated,
                checkbox,
                checked,
                ..Default::default()
            }
        })
        .collect();
    let notify = Notify::of(adapter, id);
    if let Some(notify) = &notify {
        for row in &mut rows {
            notify.dress(adapter, row);
        }
    }
    let submenus = rows
        .iter()
        .map(|row| {
            if row.id == choice_id(MenuChoice::Mute) || row.id == choice_id(MenuChoice::MuteServer)
            {
                mute_times()
            } else if row.id == choice_id(MenuChoice::ServerNotifications) {
                notification_rows(app, id)
            } else if row.id == choice_id(MenuChoice::Roles) {
                role_rows(adapter, id)
            } else if row.id == choice_id(MenuChoice::InviteToServer) {
                invite_rows(app, user.as_deref())
            } else if let (true, Some((room, user))) =
                (row.id == choice_id(MenuChoice::MoveTo), in_voice)
            {
                move_rows(app, room, user)
            } else if let Some(notify) = &notify {
                notify.submenu(&row.id)
            } else {
                vec![]
            }
        })
        .collect();
    (rows, submenus)
}

/// What a menu row says for a choice, and its icon.
fn label(icons: &Icons, choice: MenuChoice, in_call: bool) -> (&'static str, &slint::Image) {
    match choice {
        MenuChoice::MarkRead => ("Mark as read", &icons.read),
        MenuChoice::JoinVoice => ("Join voice", &icons.call),
        MenuChoice::OpenChat => ("Open chat", &icons.chat),
        MenuChoice::Profile => ("Profile", &icons.profile),
        MenuChoice::Message => ("Message", &icons.dm),
        MenuChoice::AddFriend => ("Add friend", &icons.befriend),
        MenuChoice::AcceptFriend => ("Accept friend request", &icons.befriend),
        MenuChoice::CancelFriendRequest => ("Cancel friend request", &icons.cancel),
        MenuChoice::Pin => ("Pin", &icons.pin),
        MenuChoice::Unpin => ("Unpin", &icons.unpin),
        MenuChoice::Call if in_call => ("Join call", &icons.call),
        MenuChoice::Call => ("Start call", &icons.call),
        MenuChoice::AddNote => ("Add note", &icons.note),
        MenuChoice::AddNickname => ("Add friend nickname", &icons.nickname),
        MenuChoice::EditNickname => ("Change friend nickname", &icons.nickname),
        MenuChoice::CloseDm => ("Close DM", &icons.cancel),
        MenuChoice::LeaveGroup => ("Leave group", &icons.leave),
        MenuChoice::InviteToServer => ("Invite to server", &icons.befriend),
        MenuChoice::Unfriend => ("Remove friend", &icons.unfriend),
        MenuChoice::Ignore => ("Ignore", &icons.ignore),
        MenuChoice::Unignore => ("Stop ignoring", &icons.ignore),
        MenuChoice::Block => ("Block", &icons.block),
        MenuChoice::Unblock => ("Unblock", &icons.block),
        MenuChoice::Mute => ("Mute", &icons.mute),
        MenuChoice::Unmute => ("Unmute", &icons.unmute),
        MenuChoice::CopyLink => ("Copy link", &icons.copy),
        MenuChoice::CopyId => ("Copy ID", &icons.text),
        MenuChoice::Mention => ("Mention", &icons.mention),
        MenuChoice::ServerNickname => ("Change nickname", &icons.rename),
        MenuChoice::Roles => ("Roles", &icons.roles),
        MenuChoice::RemoveTimeout => ("Remove timeout", &icons.timeout),
        MenuChoice::Timeout => ("Time out", &icons.timeout),
        MenuChoice::Kick => ("Kick", &icons.kick),
        MenuChoice::Ban => ("Ban", &icons.ban),
        MenuChoice::TransferOwnership => ("Transfer Ownership", &icons.crown),
        MenuChoice::LocalMute | MenuChoice::SelfMute => ("Mute", &icons.mic_off),
        MenuChoice::SoundboardMute => ("Mute Soundboard", &icons.soundboard),
        MenuChoice::SelfDeafen => ("Deafen", &icons.deafen),
        MenuChoice::MoveTo => ("Move to", &icons.move_to),
        MenuChoice::ServerMute => ("Server mute", &icons.mic_off),
        MenuChoice::ServerDeafen => ("Server deafen", &icons.deafen),
        MenuChoice::Disconnect => ("Disconnect", &icons.disconnect),
        // Notify::dress names these.
        MenuChoice::ChannelMute => ("Mute", &icons.mute),
        MenuChoice::ChannelNotifications => ("Notification settings", &icons.unmute),
        MenuChoice::MuteServer => ("Mute server", &icons.mute),
        MenuChoice::UnmuteServer => ("Unmute server", &icons.unmute),
        MenuChoice::ServerNotifications => ("Notification settings", &icons.unmute),
        MenuChoice::ShowAllChannels => ("Show all channels", &icons.show),
        MenuChoice::HideMuted => ("Hide muted channels", &icons.ignore),
        // A server's says so above.
        MenuChoice::Invite => ("Invite to channel", &icons.befriend),
        MenuChoice::PinChannel => ("Pin channel to top", &icons.pin),
        MenuChoice::UnpinChannel => ("Unpin channel", &icons.unpin),
        MenuChoice::AddToChannelList => ("Add to channel list", &icons.show),
        MenuChoice::RemoveFromChannelList => ("Remove from channel list", &icons.ignore),
        MenuChoice::UnfollowCategory => ("Unfollow channel category", &icons.ignore),
        // A server's and a voice channel's say so above.
        MenuChoice::CreateChannel => ("Create text channel", &icons.create),
        MenuChoice::CreateCategory => ("Create category", &icons.create_category),
        // A category's say so above.
        MenuChoice::EditChannel => ("Edit channel", &icons.edit),
        MenuChoice::DuplicateChannel => ("Duplicate channel", &icons.duplicate),
        MenuChoice::DeleteChannel => ("Delete channel", &icons.delete),
        MenuChoice::LeaveServer => ("Leave server", &icons.leave),
        MenuChoice::JoinThread => ("Join thread", &icons.thread),
        MenuChoice::LeaveThread => ("Leave thread", &icons.leave),
        MenuChoice::EditServerProfile => ("Edit server profile", &icons.rename),
        MenuChoice::ServerSettings => ("Server settings", &icons.settings),
    }
}

/// A server member's roles submenu, built as it opens: every role of the
/// server, highest first, ticked for those they hold, and greyed for those
/// this account may not give or take, as Discord shows them.
fn role_rows(adapter: &Adapter, user: &str) -> Vec<MenuRow> {
    let Some(place) = adapter.app.state.selected.as_deref() else {
        return vec![];
    };
    adapter
        .app
        .role_choices(place, user)
        .into_iter()
        .map(|choice| MenuRow {
            id: format!("{ROLE}{}", choice.role.id).into(),
            text: choice.role.name.into(),
            checkbox: true,
            checked: choice.held,
            disabled: !choice.grantable,
            ..Default::default()
        })
        .collect()
}

/// What a role's row in the roles submenu reports, before the role's id.
const ROLE: &str = "role:";

/// Someone's Invite to server submenu, built as it opens: the servers they
/// may be invited to, by name, in the rail's order. A DM's is its person's.
fn invite_rows(app: &Application, user: Option<&str>) -> Vec<MenuRow> {
    let Some(user) = user else {
        return vec![];
    };
    app.invite_servers(user)
        .into_iter()
        .map(|server| MenuRow {
            id: format!("{INVITE_TO}{}", server.id).into(),
            text: server.name.as_str().into(),
            ..Default::default()
        })
        .collect()
}

/// What a server's row in the Invite to server submenu reports, before the
/// server's id.
const INVITE_TO: &str = "invite-to:";

/// How someone's menu in a voice channel names its target: this, the
/// channel's room id, `/`, and their id, which has no `/`.
const IN_VOICE: &str = "in-voice:";

/// The voice channel and the person a voice channel's menu is about.
fn voice_target(target: &str) -> Option<(&str, &str)> {
    target.strip_prefix(IN_VOICE)?.rsplit_once('/')
}

/// Someone's Move to submenu, built as it opens: the voice channels they
/// may be moved to, by name, in the channel list's order.
fn move_rows(app: &Application, room: &str, user: &str) -> Vec<MenuRow> {
    app.move_targets(room, user)
        .into_iter()
        .map(|(room, name)| MenuRow {
            id: format!("{MOVE_TO}{room}").into(),
            text: name.into(),
            ..Default::default()
        })
        .collect()
}

/// What a channel's row in the Move to submenu reports, before its room id.
const MOVE_TO: &str = "move-to:";
/// The header's bell names its menu by the open channel's room id after this.
const BELL: &str = "notifications:";

/// The ids of the notification levels a channel's or category's choices offer.
const LEVELS: [(&str, Option<NotifyLevel>); 4] = [
    ("level-default", None),
    ("level-all", Some(NotifyLevel::All)),
    ("level-mentions", Some(NotifyLevel::Mentions)),
    ("level-nothing", Some(NotifyLevel::Nothing)),
];

/// A Discord server channel's or category's notification choices, which its
/// menu and the header's bell build alike: muting it for a time, or
/// unmuting it, and which of its messages notify.
pub(super) struct Notify {
    category: bool,
    /// A channel listed under a category, whose default is the category's.
    in_category: bool,
    settings: Notifications,
}

impl Notify {
    /// Nothing for a place that isn't a Discord server channel or category.
    pub(super) fn of(adapter: &Adapter, place: &str) -> Option<Self> {
        let app = &adapter.app;
        let settings = app.notifications(place)?;
        discord::parse_room(place)?;
        Some(Self {
            category: !app.state.rooms.iter().any(|room| room.id == place),
            in_category: app.category_of(place).is_some(),
            settings,
        })
    }

    fn kind(&self) -> &'static str {
        if self.category { "category" } else { "channel" }
    }

    /// The bell's menu: muting, then the levels.
    fn bell(&self, adapter: &Adapter) -> Menu {
        let mut rows = vec![self.mute_row(adapter)];
        let mut levels = self.levels();
        if let Some(first) = levels.first_mut() {
            first.separated = true;
        }
        rows.extend(levels);
        let submenus = rows.iter().map(|row| self.submenu(&row.id)).collect();
        (rows, submenus)
    }

    /// Mute for a time, in a submenu, or while muted, unmute, saying till when.
    fn mute_row(&self, adapter: &Adapter) -> MenuRow {
        let icons = &adapter.icons;
        let muted = self.settings.muted;
        MenuRow {
            id: choice_id(MenuChoice::ChannelMute).into(),
            text: format!("{} {}", if muted { "Unmute" } else { "Mute" }, self.kind()).into(),
            subtitle: self
                .settings
                .muted_until
                .map(|until| muted_until(until / 1000, adapter.layout.twelve_hour_clock))
                .unwrap_or_default()
                .into(),
            icon: if muted {
                icons.unmute.clone()
            } else {
                icons.mute.clone()
            },
            ..Default::default()
        }
    }

    /// The levels, the one chosen checked; following the category's or the
    /// server's says which that is.
    fn levels(&self) -> Vec<MenuRow> {
        LEVELS
            .into_iter()
            .map(|(id, level)| MenuRow {
                id: id.into(),
                text: match level {
                    None if self.in_category => "Use category default",
                    None => "Use server default",
                    Some(level) => level_name(level),
                }
                .into(),
                subtitle: match level {
                    None => level_name(self.settings.inherited).into(),
                    Some(_) => SharedString::default(),
                },
                checked: self.settings.level == level,
                ..Default::default()
            })
            .collect()
    }

    /// Names a channel's or category's own rows in its menu.
    fn dress(&self, adapter: &Adapter, row: &mut MenuRow) {
        if row.id == choice_id(MenuChoice::ChannelMute) {
            let separated = row.separated;
            *row = MenuRow {
                separated,
                ..self.mute_row(adapter)
            };
        } else if row.id == choice_id(MenuChoice::ChannelNotifications) {
            row.subtitle =
                level_name(self.settings.level.unwrap_or(self.settings.inherited)).into();
        }
    }

    /// What choosing a row of its choices, by its id, changes.
    pub(super) fn change(&self, id: &str) -> Option<NotificationChange> {
        if let Some((_, level)) = LEVELS.into_iter().find(|(level, _)| *level == id) {
            Some(NotificationChange::Level(level))
        } else if let Some(muting) = MUTE_TIMES.into_iter().find(|&muting| mute_id(muting) == id) {
            Some(NotificationChange::Mute(muting))
        } else if id == choice_id(MenuChoice::ChannelMute) && self.settings.muted {
            Some(NotificationChange::Mute(Muting::Off))
        } else {
            None
        }
    }

    /// The submenu of a row of its choices: how long to mute for, unless
    /// muted, or the levels.
    fn submenu(&self, id: &str) -> Vec<MenuRow> {
        if id == choice_id(MenuChoice::ChannelMute) && !self.settings.muted {
            mute_times()
        } else if id == choice_id(MenuChoice::ChannelNotifications) {
            self.levels()
        } else {
            vec![]
        }
    }
}

fn level_name(level: NotifyLevel) -> &'static str {
    match level {
        NotifyLevel::All => "All messages",
        NotifyLevel::Mentions => "Only @mentions",
        NotifyLevel::Nothing => "Nothing",
    }
}

/// The levels a server's notification submenu offers, the one that
/// applies checked, then its switches.
fn notification_rows(app: &Application, server: &str) -> Vec<MenuRow> {
    let Some(notifications) = app.server_notifications(server) else {
        return vec![];
    };
    let own = notifications.own;
    let current = own.level.unwrap_or(own.inherited);
    // A server has no default above it.
    let levels = LEVELS.into_iter().filter_map(|(id, level)| {
        let level = level?;
        Some(MenuRow {
            id: id.into(),
            text: level_name(level).into(),
            checked: level == current,
            ..Default::default()
        })
    });
    let switch = |id: &str, text: &str, on: bool, separated: bool| MenuRow {
        id: id.into(),
        text: text.into(),
        checkbox: true,
        checked: on,
        separated,
        ..Default::default()
    };
    levels
        .chain([
            switch(
                SUPPRESS_EVERYONE,
                "Suppress @everyone and @here",
                notifications.suppress_everyone,
                true,
            ),
            switch(
                SUPPRESS_ROLES,
                "Suppress all role @mentions",
                notifications.suppress_roles,
                false,
            ),
        ])
        .collect()
}

const SUPPRESS_EVERYONE: &str = "suppress-everyone";
const SUPPRESS_ROLES: &str = "suppress-roles";

/// When a mute ends, as its Unmute row says it.
fn muted_until(seconds: u64, twelve_hour: bool) -> String {
    format!("Muted until {}", until(seconds, twelve_hour))
}

/// When something ends, a Unix time: the time today, or the day too.
pub(super) fn until(seconds: u64, twelve_hour: bool) -> String {
    let now = yapfast_core::clock::now_ms() / 1000;
    let time = yapfast_core::clock::time_on(seconds, twelve_hour);
    let on = yapfast_core::clock::day(seconds);
    if on == yapfast_core::clock::day(now) {
        time
    } else {
        format!("{on}, {time}")
    }
}

/// How long [`MenuChoice::Mute`] mutes for, as its submenu offers.
fn mute_times() -> Vec<MenuRow> {
    MUTE_TIMES
        .into_iter()
        .map(|muting| MenuRow {
            id: mute_id(muting).into(),
            text: match muting {
                Muting::Minutes(15) => "For 15 minutes".into(),
                Muting::Minutes(60) => "For 1 hour".into(),
                Muting::Minutes(minutes) if minutes % 60 == 0 => {
                    format!("For {} hours", minutes / 60).into()
                }
                Muting::Minutes(minutes) => format!("For {minutes} minutes").into(),
                Muting::Always | Muting::Off => "Until I turn it back on".into(),
            },
            ..Default::default()
        })
        .collect()
}

/// Shows a menu in the window, its submenus only when a row has one.
pub(super) fn show(window: &YapfastWindow, (rows, submenus): Menu) {
    let submenus: Vec<ModelRc<MenuRow>> = if submenus.iter().all(Vec::is_empty) {
        vec![]
    } else {
        submenus
            .into_iter()
            .map(|rows| {
                if rows.is_empty() {
                    ModelRc::default()
                } else {
                    ModelRc::new(VecModel::from(rows))
                }
            })
            .collect()
    };
    window.set_menu_rows(ModelRc::new(VecModel::from(rows)));
    window.set_menu_submenus(ModelRc::new(VecModel::from(submenus)));
}

/// Ticks or unticks the checkbox row `id` in the menu showing, as choosing
/// it does while the menu stays open; whether it's ticked now, when there
/// was one.
pub(super) fn toggle(window: &YapfastWindow, id: &str) -> Option<bool> {
    let flip = |rows: ModelRc<MenuRow>| {
        let index = rows.iter().position(|row| row.checkbox && row.id == id)?;
        let mut row = rows.row_data(index)?;
        row.checked = !row.checked;
        let checked = row.checked;
        rows.set_row_data(index, row);
        Some(checked)
    };
    std::iter::once(window.get_menu_rows())
        .chain(window.get_menu_submenus().iter())
        .find_map(flip)
}

/// Carries out the choice a menu reports by its row's id.
pub(super) fn choose(
    adapter: &mut Adapter,
    window: &YapfastWindow,
    target: String,
    person: bool,
    id: &str,
) {
    // A checkbox shows its new state at once, as the menu stays open.
    let ticked = toggle(window, id);
    // Someone's menu in a voice channel acts on them in its server.
    let in_voice = voice_target(&target).map(|(room, user)| (room.to_owned(), user.to_owned()));
    let target = match &in_voice {
        Some((_, user)) => user.clone(),
        None => target,
    };
    let target = match target.strip_prefix(BELL) {
        Some(place) => place.to_owned(),
        None => target,
    };
    let (target, location) = match thread_more(&target) {
        Some((room, location)) => (room.to_owned(), location),
        None => (target, "Context Menu"),
    };
    let notify = Notify::of(adapter, &target);
    let change = |change| Action::ChangeNotifications {
        place: target.clone(),
        change,
    };
    if let Some(chosen) = notify.and_then(|notify| notify.change(id)) {
        adapter.action(window, change(chosen));
        return;
    }
    let target = target
        .strip_prefix(HEADER)
        .map(str::to_owned)
        .unwrap_or(target);
    let server = adapter
        .app
        .state
        .servers
        .iter()
        .any(|server| server.id == target);
    if server && let Some(change) = server_change(&adapter.app, &target, id) {
        adapter.action(
            window,
            Action::ChangeNotifications {
                place: target,
                change,
            },
        );
        return;
    }
    let (room, user) = subjects(&adapter.app.state, &target, person);
    // A member's choices act in the server of the channel they're in, or
    // of the settings listing them.
    #[cfg(feature = "discord")]
    let listed = adapter.app.member_place(&target).map(str::to_owned);
    #[cfg(not(feature = "discord"))]
    let listed = None;
    let place = in_voice
        .map(|(room, _)| room)
        .or(listed)
        .or_else(|| adapter.app.state.selected.clone());
    let me = adapter.app.state.discord_user.as_deref() == Some(target.as_str());
    let voice = |change| {
        place.clone().map(|place| Action::ChangeMember {
            place,
            user: target.clone(),
            change,
        })
    };
    if let Some(to) = id.strip_prefix(MOVE_TO) {
        // This account moves by joining there.
        let action = if me {
            Some(Action::RequestJoinVoice(to.to_owned()))
        } else {
            voice(MemberChange::Move(Some(to.to_owned())))
        };
        if let Some(action) = action {
            adapter.action(window, action);
        }
        return;
    }
    if let (Some(role), Some(add), Some(place)) = (id.strip_prefix(ROLE), ticked, place.clone()) {
        let change = MemberChange::Role {
            role: role.to_owned(),
            add,
        };
        adapter.action(
            window,
            Action::ChangeMember {
                place,
                user: target,
                change,
            },
        );
        return;
    }
    if let (Some(server), Some(user)) = (id.strip_prefix(INVITE_TO), user.clone()) {
        let server = server.to_owned();
        adapter.action(window, Action::InviteToServer { user, server });
        return;
    }
    if let Some(muting) = MUTE_TIMES.into_iter().find(|&muting| mute_id(muting) == id) {
        if let Some(room) = room {
            adapter.action(window, Action::MuteDm { room, muting });
        }
        return;
    }
    let Some(choice) = CHOICES.into_iter().find(|&choice| choice_id(choice) == id) else {
        return;
    };
    let ask = |ask| user.clone().map(|user| Action::AskPerson { user, ask });
    let change = |change| {
        user.clone()
            .map(|user| Action::ChangeRelationship { user, change })
    };
    let ask_member = |ask| {
        place.clone().map(|place| Action::AskMember {
            place,
            user: target.clone(),
            ask,
        })
    };
    let action = match choice {
        MenuChoice::MarkRead => Some(Action::MarkRead(target)),
        MenuChoice::JoinVoice => Some(Action::RequestJoinVoice(target)),
        MenuChoice::OpenChat => Some(Action::OpenChat(target)),
        MenuChoice::Call if person => Some(Action::CallPerson(target.clone())),
        MenuChoice::Call => room.map(Action::RequestJoinVoice),
        MenuChoice::Profile if person => Some(Action::ShowProfile {
            user: target,
            large: false,
        }),
        MenuChoice::Profile => Some(Action::ShowDmProfile(target)),
        MenuChoice::Message => Some(Action::MessagePerson(target)),
        MenuChoice::AddFriend | MenuChoice::AcceptFriend => user.map(Action::Befriend),
        MenuChoice::CancelFriendRequest => user.map(Action::RemoveFriendRequest),
        MenuChoice::Pin | MenuChoice::Unpin => room.map(|room| Action::Pin {
            room,
            pinned: choice == MenuChoice::Pin,
        }),
        MenuChoice::AddNote => ask(PersonAsk::Note),
        MenuChoice::AddNickname | MenuChoice::EditNickname => ask(PersonAsk::Nickname),
        MenuChoice::CloseDm => room.map(Action::CloseDm),
        MenuChoice::LeaveGroup => room.map(Action::AskLeaveGroup),
        MenuChoice::Unfriend => ask(PersonAsk::Unfriend),
        MenuChoice::Block => ask(PersonAsk::Block),
        MenuChoice::Ignore => change(RelationshipChange::Ignore),
        MenuChoice::Unignore => change(RelationshipChange::Unignore),
        MenuChoice::Unblock => change(RelationshipChange::Unblock),
        // Its times are its submenu's.
        MenuChoice::Mute => None,
        MenuChoice::Unmute => room.map(|room| Action::MuteDm {
            room,
            muting: Muting::Off,
        }),
        MenuChoice::CopyLink => {
            window.invoke_copy_text(network_link(&target).into());
            None
        }
        MenuChoice::CopyId => {
            window.invoke_copy_text(network_id(&target).into());
            None
        }
        MenuChoice::Mention => {
            adapter.action(window, Action::MentionPerson(target));
            let end = adapter.app.state.draft().len();
            window.invoke_place_composer_caret(i32::try_from(end).unwrap_or(i32::MAX));
            window.invoke_focus_composer();
            None
        }
        MenuChoice::ServerNickname => ask_member(PersonAsk::ServerNickname),
        MenuChoice::Timeout => ask_member(PersonAsk::Timeout),
        MenuChoice::Kick => ask_member(PersonAsk::Kick),
        MenuChoice::Ban => ask_member(PersonAsk::Ban),
        MenuChoice::TransferOwnership => {
            Some(Action::Ownership(OwnershipInput::Transfer(target.clone())))
        }
        MenuChoice::RemoveTimeout => place.clone().map(|place| Action::ChangeMember {
            place,
            user: target.clone(),
            change: MemberChange::Timeout {
                until_ms: None,
                reason: String::new(),
            },
        }),
        // Its roles are its submenu's, as are the servers to invite to and
        // the channels to move to.
        MenuChoice::Roles | MenuChoice::InviteToServer | MenuChoice::MoveTo => None,
        MenuChoice::LocalMute => {
            if let Some(muted) = ticked {
                yapfast_core::volume::mute(&target, muted);
                let people = &mut adapter.layout.muted_people;
                if muted {
                    people.insert(target.clone());
                } else {
                    people.remove(&target);
                }
                adapter.layout.write(&adapter.root);
            }
            None
        }
        MenuChoice::SoundboardMute => {
            if let Some(muted) = ticked {
                yapfast_core::volume::mute_soundboard(&target, muted);
                let people = &mut adapter.layout.soundboard_muted_people;
                if muted {
                    people.insert(target.clone());
                } else {
                    people.remove(&target);
                }
                adapter.layout.write(&adapter.root);
            }
            None
        }
        MenuChoice::SelfMute => ticked.map(|muted| Action::Voice(voice::Command::Mute(muted))),
        MenuChoice::SelfDeafen => {
            ticked.map(|deafened| Action::Voice(voice::Command::Deafen(deafened)))
        }
        MenuChoice::ServerMute => ticked.and_then(|mute| voice(MemberChange::ServerMute(mute))),
        MenuChoice::ServerDeafen => ticked.and_then(|deaf| voice(MemberChange::ServerDeafen(deaf))),
        MenuChoice::Disconnect => voice(MemberChange::Move(None)),
        // Notify's choices, above.
        MenuChoice::ChannelMute | MenuChoice::ChannelNotifications => None,
        MenuChoice::Invite => Some(Action::OpenInvite(target)),
        MenuChoice::PinChannel | MenuChoice::UnpinChannel => Some(Action::PinChannel {
            room: target,
            pinned: choice == MenuChoice::PinChannel,
        }),
        MenuChoice::AddToChannelList | MenuChoice::RemoveFromChannelList => {
            Some(Action::FollowChannel {
                room: target,
                follow: choice == MenuChoice::AddToChannelList,
            })
        }
        // The channel's category goes, with every channel it lists.
        MenuChoice::UnfollowCategory => {
            adapter
                .app
                .category_of(&target)
                .map(|category| Action::FollowChannel {
                    room: category,
                    follow: false,
                })
        }
        MenuChoice::ShowAllChannels => ticked.map(|show| Action::ShowAllChannels {
            server: target,
            show,
        }),
        MenuChoice::CreateChannel => Some(Action::NewChannel {
            place: target,
            kind: ChannelKind::Text,
        }),
        MenuChoice::CreateCategory => Some(Action::NewChannel {
            place: target,
            kind: ChannelKind::Category,
        }),
        MenuChoice::EditChannel => Some(Action::OpenChannelSettings(target)),
        MenuChoice::DuplicateChannel => Some(Action::AskChannel(ChannelAsk::Duplicate(target))),
        MenuChoice::DeleteChannel => Some(Action::AskChannel(ChannelAsk::Delete(target))),
        MenuChoice::LeaveServer => Some(Action::AskLeaveServer(target)),
        MenuChoice::JoinThread | MenuChoice::LeaveThread => Some(Action::JoinThread {
            room: target,
            join: choice == MenuChoice::JoinThread,
            location,
        }),
        // From a server's menu, or one's own in its member list.
        MenuChoice::EditServerProfile => {
            if person {
                place.clone().map(Action::EditServerProfile)
            } else {
                Some(Action::EditServerProfile(target))
            }
        }
        MenuChoice::ServerSettings => Some(Action::OpenServerSettings(target)),
        // A server's notifications change above; their rows are submenus.
        MenuChoice::MuteServer
        | MenuChoice::UnmuteServer
        | MenuChoice::ServerNotifications
        | MenuChoice::HideMuted => None,
    };
    let Some(action) = action else {
        return;
    };
    let pins = matches!(action, Action::Pin { .. } | Action::CloseDm(_));
    adapter.action(window, action);
    if pins {
        adapter.layout.pinned = adapter.app.state.pinned.clone();
        adapter.layout.write(&adapter.root);
    }
}

/// The change to a Discord server's notification settings a choice from
/// its menu makes: a mute, a level or a switch, each switch the opposite of
/// what Discord last said.
pub(super) fn server_change(
    app: &Application,
    server: &str,
    id: &str,
) -> Option<NotificationChange> {
    if let Some(muting) = MUTE_TIMES.into_iter().find(|&muting| mute_id(muting) == id) {
        return Some(NotificationChange::Mute(muting));
    }
    if let Some((_, Some(level))) = LEVELS.into_iter().find(|(level, _)| *level == id) {
        return Some(NotificationChange::Level(Some(level)));
    }
    let known = app.server_notifications(server)?;
    match id {
        SUPPRESS_EVERYONE => Some(NotificationChange::SuppressEveryone(
            !known.suppress_everyone,
        )),
        SUPPRESS_ROLES => Some(NotificationChange::SuppressRoles(!known.suppress_roles)),
        _ if id == choice_id(MenuChoice::HideMuted) => {
            Some(NotificationChange::HideMuted(!known.hide_muted))
        }
        _ if id == choice_id(MenuChoice::UnmuteServer) => {
            Some(NotificationChange::Mute(Muting::Off))
        }
        _ => None,
    }
}

const CHOICES: [MenuChoice; 65] = [
    MenuChoice::MarkRead,
    MenuChoice::JoinVoice,
    MenuChoice::OpenChat,
    MenuChoice::Profile,
    MenuChoice::Message,
    MenuChoice::AddFriend,
    MenuChoice::AcceptFriend,
    MenuChoice::CancelFriendRequest,
    MenuChoice::Pin,
    MenuChoice::Unpin,
    MenuChoice::Call,
    MenuChoice::AddNote,
    MenuChoice::AddNickname,
    MenuChoice::EditNickname,
    MenuChoice::CloseDm,
    MenuChoice::LeaveGroup,
    MenuChoice::InviteToServer,
    MenuChoice::Unfriend,
    MenuChoice::Ignore,
    MenuChoice::Unignore,
    MenuChoice::Block,
    MenuChoice::Unblock,
    MenuChoice::Mute,
    MenuChoice::Unmute,
    MenuChoice::CopyLink,
    MenuChoice::CopyId,
    MenuChoice::Mention,
    MenuChoice::ServerNickname,
    MenuChoice::Roles,
    MenuChoice::Timeout,
    MenuChoice::RemoveTimeout,
    MenuChoice::Kick,
    MenuChoice::Ban,
    MenuChoice::TransferOwnership,
    MenuChoice::ChannelMute,
    MenuChoice::ChannelNotifications,
    MenuChoice::MuteServer,
    MenuChoice::UnmuteServer,
    MenuChoice::ServerNotifications,
    MenuChoice::HideMuted,
    MenuChoice::Invite,
    MenuChoice::PinChannel,
    MenuChoice::UnpinChannel,
    MenuChoice::AddToChannelList,
    MenuChoice::RemoveFromChannelList,
    MenuChoice::UnfollowCategory,
    MenuChoice::ShowAllChannels,
    MenuChoice::CreateChannel,
    MenuChoice::CreateCategory,
    MenuChoice::EditChannel,
    MenuChoice::DuplicateChannel,
    MenuChoice::DeleteChannel,
    MenuChoice::LeaveServer,
    MenuChoice::JoinThread,
    MenuChoice::LeaveThread,
    MenuChoice::EditServerProfile,
    MenuChoice::ServerSettings,
    MenuChoice::LocalMute,
    MenuChoice::SoundboardMute,
    MenuChoice::SelfMute,
    MenuChoice::SelfDeafen,
    MenuChoice::MoveTo,
    MenuChoice::ServerMute,
    MenuChoice::ServerDeafen,
    MenuChoice::Disconnect,
];

fn choice_id(choice: MenuChoice) -> &'static str {
    match choice {
        MenuChoice::MarkRead => "mark-read",
        MenuChoice::JoinVoice => "join-voice",
        MenuChoice::OpenChat => "open-chat",
        MenuChoice::Profile => "profile",
        MenuChoice::Message => "message",
        MenuChoice::AddFriend => "add-friend",
        MenuChoice::AcceptFriend => "accept-friend",
        MenuChoice::CancelFriendRequest => "cancel-friend-request",
        MenuChoice::Pin => "pin",
        MenuChoice::Unpin => "unpin",
        MenuChoice::Call => "call",
        MenuChoice::AddNote => "add-note",
        MenuChoice::AddNickname => "add-nickname",
        MenuChoice::EditNickname => "edit-nickname",
        MenuChoice::CloseDm => "close-dm",
        MenuChoice::LeaveGroup => "leave-group",
        MenuChoice::InviteToServer => "invite-to-server",
        MenuChoice::Unfriend => "unfriend",
        MenuChoice::Ignore => "ignore",
        MenuChoice::Unignore => "unignore",
        MenuChoice::Block => "block",
        MenuChoice::Unblock => "unblock",
        MenuChoice::Mute => "mute",
        MenuChoice::Unmute => "unmute",
        MenuChoice::CopyLink => "copy-link",
        MenuChoice::CopyId => "copy-id",
        MenuChoice::Mention => "mention",
        MenuChoice::ServerNickname => "server-nickname",
        MenuChoice::Roles => "roles",
        MenuChoice::Timeout => "timeout",
        MenuChoice::RemoveTimeout => "remove-timeout",
        MenuChoice::Kick => "kick",
        MenuChoice::Ban => "ban",
        MenuChoice::TransferOwnership => "transfer-ownership",
        MenuChoice::ChannelMute => "channel-mute",
        MenuChoice::ChannelNotifications => "channel-notifications",
        MenuChoice::MuteServer => "mute-server",
        MenuChoice::UnmuteServer => "unmute-server",
        MenuChoice::ServerNotifications => "server-notifications",
        MenuChoice::HideMuted => "hide-muted",
        MenuChoice::Invite => "invite",
        MenuChoice::PinChannel => "pin-channel",
        MenuChoice::UnpinChannel => "unpin-channel",
        MenuChoice::AddToChannelList => "add-to-channel-list",
        MenuChoice::RemoveFromChannelList => "remove-from-channel-list",
        MenuChoice::UnfollowCategory => "unfollow-category",
        MenuChoice::ShowAllChannels => "show-all-channels",
        MenuChoice::CreateChannel => "create-channel",
        MenuChoice::CreateCategory => "create-category",
        MenuChoice::EditChannel => "edit-channel",
        MenuChoice::DuplicateChannel => "duplicate-channel",
        MenuChoice::DeleteChannel => "delete-channel",
        MenuChoice::LeaveServer => "leave-server",
        MenuChoice::JoinThread => "join-thread",
        MenuChoice::LeaveThread => "leave-thread",
        MenuChoice::EditServerProfile => "edit-server-profile",
        MenuChoice::ServerSettings => "server-settings",
        MenuChoice::LocalMute => "local-mute",
        MenuChoice::SoundboardMute => "soundboard-mute",
        MenuChoice::SelfMute => "self-mute",
        MenuChoice::SelfDeafen => "self-deafen",
        MenuChoice::MoveTo => "move-to",
        MenuChoice::ServerMute => "server-mute",
        MenuChoice::ServerDeafen => "server-deafen",
        MenuChoice::Disconnect => "disconnect",
    }
}

/// The conversation and the person a menu is about: a person's one-to-one
/// DM, or a DM's other person.
fn subjects(state: &AppState, target: &str, person: bool) -> (Option<String>, Option<String>) {
    if person {
        let room = state
            .rooms
            .iter()
            .find(|room| room.direct && room.person.as_deref() == Some(target))
            .map(|room| room.id.clone());
        (room, Some(target.to_owned()))
    } else {
        let user = state
            .rooms
            .iter()
            .find(|room| room.id == target)
            .and_then(|room| room.person.clone());
        (Some(target.to_owned()), user)
    }
}

fn mute_id(muting: Muting) -> String {
    match muting {
        Muting::Minutes(minutes) => format!("mute-{minutes}"),
        Muting::Always => "mute-always".into(),
        Muting::Off => "unmute".into(),
    }
}
