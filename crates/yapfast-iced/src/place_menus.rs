//! The menus of places: a server's (from its name over the channels), a
//! channel's, a category's or a DM's (a right click on its row), built
//! from the core's `menu_choices`: Discord's wording, each row with its
//! icon, a line between groups, and Copy ID only with Developer Mode on.
//! Muting opens the times it mutes for (a click on the row itself
//! mutes until it's turned back on), notification settings their levels,
//! and a server's switches tick in place. The header's bell opens the open
//! channel's notification choices.

use iced::{Point, Task, widget::operation};
use yapfast_core::{
    application::{Action, ChannelAsk, MUTE_TIMES, MenuChoice, network_id, network_link},
    chat::{ChannelKind, MemberChange, Muting, NotificationChange, Notifications, NotifyLevel},
};

use crate::{
    app::{App, Message, NOBODY},
    controls::Check,
    menus::{Context, Menu, MenuAct, MenuRow},
    pictures::Icon,
};

/// Opens the menu of the place `id` at `at`: the server's header menu
/// when `header`.
pub fn open_place_menu(app: &mut App, id: &str, at: Point, header: bool) -> Task<Message> {
    let core = &app.client.app;
    let choices = if header {
        core.header_choices(id)
    } else {
        core.menu_choices(id)
    };
    place_menu(app, id, at, header, choices)
}

/// A thread's header's More, beside its channel or in full view: its
/// menu without Mark as read.
pub fn open_thread_more(app: &mut App, room: &str, at: Point) -> Task<Message> {
    let choices = app.client.app.thread_more_choices(room);
    place_menu(app, room, at, false, choices)
}

fn place_menu(
    app: &mut App,
    id: &str,
    at: Point,
    header: bool,
    mut choices: Vec<MenuChoice>,
) -> Task<Message> {
    let core = &app.client.app;
    let state = &core.state;
    let server = state.servers.iter().any(|server| server.id == id);
    let folder = !server && state.is_folder(id);
    let category = !server && !folder && state.is_category(id);
    let voice = state.rooms.iter().any(|room| room.id == id && room.voice);
    // A DM with people in its call offers to join it rather than start one.
    let in_call = state
        .rooms
        .iter()
        .any(|room| room.id == id && !room.people.is_empty());
    // A conversation's and a server's menu starts with Mark as read,
    // greyed when there's nothing to read.
    let mut unread = !folder || state.folder_unread(id);
    if !header
        && !state.preview
        && (server || state.rooms.iter().any(|room| room.id == id))
        && !choices.contains(&MenuChoice::MarkRead)
    {
        choices.insert(0, MenuChoice::MarkRead);
        unread = false;
    }
    let developer = app.layout.developer_mode;
    let twelve = app.layout.prefs.twelve_hour_clock;
    let server_notifications = if server {
        core.server_notifications(id)
    } else {
        None
    };
    // A channel's or category's own notification choices.
    let notify = (!server)
        .then(|| core.notifications(id))
        .flatten()
        .filter(|_| state.rooms.iter().any(|room| room.id == id && !room.direct) || category)
        .map(|settings| Notify {
            category,
            in_category: core.category_of(id).is_some(),
            settings,
        });
    let person = state
        .rooms
        .iter()
        .find(|room| room.id == id)
        .and_then(|room| room.person.as_deref());
    // How each of a folder's Discord servers notifies, in the rail's order.
    let folder_known: Vec<(String, Option<yapfast_core::chat::ServerNotifications>)> = if folder {
        core.folder_notifications(id)
            .map(|(server, known)| (server.to_owned(), known))
            .collect()
    } else {
        Vec::new()
    };
    let mut group = None;
    let mut rows: Vec<MenuRow> = choices
        .into_iter()
        .filter(|choice| developer || *choice != MenuChoice::CopyId)
        .map(|choice| {
            let label = match choice {
                MenuChoice::MarkRead if folder => "Mark folder as read",
                MenuChoice::Invite if server => "Invite to server",
                MenuChoice::CreateChannel if server => "Create channel",
                MenuChoice::CreateChannel if voice => "Create voice channel",
                MenuChoice::EditChannel if category => "Edit category",
                MenuChoice::DeleteChannel if category => "Delete category",
                MenuChoice::Call if in_call => "Join call",
                choice => label(choice),
            };
            let starts = group_of(choice);
            let separated = group.replace(starts).is_some_and(|last| last != starts);
            let mut row = MenuRow {
                icon: Some(icon(choice)),
                danger: crate::person_menu::danger(choice),
                separated,
                disabled: choice == MenuChoice::MarkRead && !unread,
                ..MenuRow::plain(MenuAct::Place(choice), label)
            };
            match choice {
                MenuChoice::MuteServer => {
                    row.submenu =
                        mute_times(|muting| MenuAct::Notify(NotificationChange::Mute(muting)));
                    row.click = Some(MenuAct::Notify(NotificationChange::Mute(Muting::Always)));
                }
                MenuChoice::UnmuteServer => {
                    row.act = MenuAct::Notify(NotificationChange::Mute(Muting::Off));
                    row.subtitle = server_notifications
                        .and_then(|known| known.own.muted_until)
                        .map(|until| muted_until(until / 1000, twelve).into());
                }
                MenuChoice::ServerNotifications => {
                    if let Some(known) = server_notifications {
                        let own = known.own;
                        row.subtitle = Some(level_name(own.level.unwrap_or(own.inherited)).into());
                        row.submenu = server_levels(&known);
                    }
                }
                MenuChoice::HideMuted => {
                    let hidden = server_notifications.is_some_and(|known| known.hide_muted);
                    row.act = MenuAct::Notify(NotificationChange::HideMuted(!hidden));
                    row.check = Check::Box(hidden);
                }
                MenuChoice::ShowAllChannels => {
                    row.act = MenuAct::ShowAll;
                    row.check = Check::Box(
                        server_notifications.is_some_and(|known| known.show_all == Some(true)),
                    );
                }
                MenuChoice::ChannelMute => {
                    if let Some(notify) = &notify {
                        row = MenuRow {
                            separated,
                            ..notify.mute_row(twelve)
                        };
                    }
                }
                MenuChoice::ChannelNotifications => {
                    if let Some(notify) = &notify {
                        let settings = notify.settings;
                        row.subtitle =
                            Some(level_name(settings.level.unwrap_or(settings.inherited)).into());
                        row.submenu = notify.levels();
                    }
                }
                // A DM's, about its person.
                MenuChoice::Mute | MenuChoice::InviteToServer | MenuChoice::Apps => {
                    crate::person_menu::submenu(core, &mut row, choice, person, Some(id), None);
                }
                MenuChoice::Unmute => row.act = MenuAct::MuteDm(Muting::Off),
                // A folder's, for all its servers at once.
                MenuChoice::MuteFolder => {
                    row.submenu =
                        mute_times(|muting| MenuAct::Notify(NotificationChange::Mute(muting)));
                    row.click = Some(MenuAct::Notify(NotificationChange::Mute(Muting::Always)));
                }
                MenuChoice::UnmuteFolder => {
                    row.act = MenuAct::Notify(NotificationChange::Mute(Muting::Off));
                    row.subtitle = folder_muted_until(&folder_known)
                        .map(|until| muted_until(until / 1000, twelve).into());
                }
                MenuChoice::FolderNotifications => {
                    row.submenu = folder_levels(&folder_known);
                }
                _ => {}
            }
            row
        })
        .collect();
    if folder && !header {
        rows.extend(folder_rows(app, id, &folder_known, twelve));
    }
    if rows.is_empty() {
        return Task::none();
    }
    app.win.menu = Some(Menu {
        place: Some(id.to_owned()),
        header,
        ..Menu::new(at, rows).width(crate::menus::FIELD_WIDTH)
    });
    operation::focus(NOBODY)
}

/// The header's bell: the open channel's mute and levels, under the
/// button.
pub fn open_bell(app: &mut App, at: Point) -> Task<Message> {
    let core = &app.client.app;
    let Some(room) = core.state.selected.clone() else {
        return Task::none();
    };
    let Some(settings) = core.notifications(&room) else {
        return Task::none();
    };
    let notify = Notify {
        category: false,
        in_category: core.category_of(&room).is_some(),
        settings,
    };
    let mut rows = vec![notify.mute_row(app.layout.prefs.twelve_hour_clock)];
    let mut levels = notify.levels();
    if let Some(first) = levels.first_mut() {
        first.separated = true;
    }
    rows.extend(levels);
    app.win.menu = Some(Menu {
        place: Some(room),
        ..Menu::new(at, rows).width(crate::menus::FIELD_WIDTH)
    });
    operation::focus(NOBODY)
}

/// The server's scheduled events, under its events row: each one's name,
/// with when it starts (or "Happening now") and where under it; "No
/// upcoming events", greyed, for a server with none, as Discord's opens
/// all the same.
pub fn open_events_menu(app: &mut App, server: &str, at: Point) -> Task<Message> {
    let twelve = app.layout.prefs.twelve_hour_clock;
    let mut rows: Vec<MenuRow> = app
        .client
        .state()
        .servers
        .iter()
        .find(|found| found.id == server)
        .into_iter()
        .flat_map(|server| &server.events)
        .map(|event| {
            let when = match u64::try_from(event.start) {
                Ok(_) if event.active => "Happening now".to_owned(),
                Ok(start) => until(start, twelve),
                Err(_) => String::new(),
            };
            let subtitle = match &event.location {
                Some(place) if !when.is_empty() => format!("{when} · {place}"),
                Some(place) => place.clone(),
                None => when,
            };
            MenuRow {
                subtitle: (!subtitle.is_empty()).then(|| subtitle.into()),
                ..MenuRow::plain(MenuAct::Event, event.name.clone())
            }
        })
        .collect();
    if rows.is_empty() {
        rows.push(MenuRow {
            disabled: true,
            ..MenuRow::plain(MenuAct::Event, "No upcoming events")
        });
    }
    app.win.menu = Some(Menu {
        place: Some(server.to_owned()),
        ..Menu::new(at, rows).width(crate::menus::FIELD_WIDTH)
    });
    operation::focus(NOBODY)
}

/// A place's menu row was chosen.
pub fn choose_place(app: &mut App, place: String, choice: MenuChoice) -> Task<Message> {
    let action = match choice {
        MenuChoice::MarkRead => Action::MarkRead(place),
        MenuChoice::JoinVoice => Action::RequestJoinVoice(place),
        MenuChoice::OpenChat => Action::OpenChat(place),
        MenuChoice::Profile => Action::ShowDmProfile(place),
        MenuChoice::Call => Action::RequestJoinVoice(place),
        MenuChoice::Pin | MenuChoice::Unpin => Action::Pin {
            room: place,
            pinned: choice == MenuChoice::Pin,
        },
        MenuChoice::CloseDm => Action::CloseDm(place),
        MenuChoice::LeaveGroup => Action::AskLeaveGroup(place),
        MenuChoice::EditServerProfile => Action::EditServerProfile(place),
        MenuChoice::ServerSettings => Action::OpenServerSettings(place),
        MenuChoice::EditChannel => Action::OpenChannelSettings(place),
        MenuChoice::Invite => Action::OpenInvite(place),
        MenuChoice::PinChannel | MenuChoice::UnpinChannel => Action::PinChannel {
            room: place,
            pinned: choice == MenuChoice::PinChannel,
        },
        MenuChoice::AddToChannelList | MenuChoice::RemoveFromChannelList => Action::FollowChannel {
            room: place,
            follow: choice == MenuChoice::AddToChannelList,
        },
        // The channel's category goes, with every channel it lists.
        MenuChoice::UnfollowCategory => {
            let Some(category) = app.client.app.category_of(&place) else {
                return Task::none();
            };
            Action::FollowChannel {
                room: category,
                follow: false,
            }
        }
        MenuChoice::CreateChannel => Action::NewChannel {
            place,
            kind: ChannelKind::Text,
        },
        MenuChoice::CreateCategory => Action::NewChannel {
            place,
            kind: ChannelKind::Category,
        },
        MenuChoice::DuplicateChannel => Action::AskChannel(ChannelAsk::Duplicate(place)),
        MenuChoice::DeleteChannel => Action::AskChannel(ChannelAsk::Delete(place)),
        MenuChoice::LeaveServer => Action::AskLeaveServer(place),
        MenuChoice::JoinThread | MenuChoice::LeaveThread => Action::JoinThread {
            room: place,
            join: choice == MenuChoice::JoinThread,
            location: "Context Menu",
        },
        MenuChoice::CopyLink => return iced::clipboard::write(network_link(&place)),
        MenuChoice::CopyId => return iced::clipboard::write(network_id(&place).to_owned()),
        // The rest of a DM's are about its person, as their own menu.
        choice => {
            let person = app
                .client
                .state()
                .rooms
                .iter()
                .find(|room| room.id == place)
                .and_then(|room| room.person.clone());
            return match person {
                Some(user) => crate::person_menu::choose_person(
                    app,
                    user,
                    crate::person_menu::CONTEXT_MENU,
                    choice,
                    None,
                ),
                None => Task::none(),
            };
        }
    };
    app.client.dispatch(action);
    crate::person_menu::keep_pins(app);
    app.sync()
}

/// What a submenu's row, or a row that carries what it's about, does:
/// `ticked` is a checkbox's new state.
pub fn sub_act(
    app: &mut App,
    menu: &Context,
    row: &MenuRow,
    ticked: Option<bool>,
) -> Task<Message> {
    let core = &app.client.app;
    let state = &core.state;
    let place = menu.place.clone().unwrap_or_default();
    // A conversation's menu is about it and its person; someone's
    // about them and their DM.
    let listed = state.rooms.iter().find(|room| room.id == place);
    let (room, user) = match (&menu.voice, listed) {
        (Some((_, user)), _) => (None, Some(user.clone())),
        (None, Some(listed)) => (Some(place.clone()), listed.person.clone()),
        (None, None) => {
            let dm = state
                .rooms
                .iter()
                .find(|room| room.direct && room.person.as_deref() == Some(place.as_str()))
                .map(|room| room.id.clone());
            (dm, Some(place.clone()))
        }
    };
    // A member's choices act in the server of the channel they're in,
    // or of the settings listing them.
    let member_place = |user: &str| match &menu.voice {
        Some((room, _)) => Some(room.clone()),
        None => core
            .member_place(user)
            .map(str::to_owned)
            .or_else(|| state.selected.clone()),
    };
    let action = match row.act {
        MenuAct::Notify(change) => {
            // A switch says what it's turned to now.
            let change = match (change, ticked) {
                (NotificationChange::HideMuted(_), Some(on)) => NotificationChange::HideMuted(on),
                (NotificationChange::SuppressEveryone(_), Some(on)) => {
                    NotificationChange::SuppressEveryone(on)
                }
                (NotificationChange::SuppressRoles(_), Some(on)) => {
                    NotificationChange::SuppressRoles(on)
                }
                (change, _) => change,
            };
            Some(Action::ChangeNotifications { place, change })
        }
        // A folder's server's own, by the server in the row's key.
        MenuAct::NotifyServer(change) => {
            let change = match (change, ticked) {
                (NotificationChange::SuppressEveryone(_), Some(on)) => {
                    NotificationChange::SuppressEveryone(on)
                }
                (NotificationChange::SuppressRoles(_), Some(on)) => {
                    NotificationChange::SuppressRoles(on)
                }
                (change, _) => change,
            };
            Some(Action::ChangeNotifications {
                place: row.key.clone(),
                change,
            })
        }
        // A DM's own menu, or its person's.
        MenuAct::MuteDm(muting) => room.map(|room| Action::MuteDm { room, muting }),
        MenuAct::ShowAll => Some(Action::ShowAllChannels {
            server: place,
            show: ticked.unwrap_or(true),
        }),
        MenuAct::Role => user.as_deref().and_then(|user| {
            Some(Action::ChangeMember {
                place: member_place(user)?,
                user: user.to_owned(),
                change: MemberChange::Role {
                    role: row.key.clone(),
                    add: ticked.unwrap_or(true),
                },
            })
        }),
        MenuAct::InviteTo => user.map(|user| Action::InviteToServer {
            user,
            server: row.key.clone(),
        }),
        // On a message, by its item; on someone, by their id.
        MenuAct::App => menu
            .item
            .clone()
            .or(user)
            .map(|target| Action::RunAppCommand {
                command: row.key.clone(),
                target: Some(target),
            }),
        // This account moves by joining there.
        MenuAct::MoveTo => menu
            .voice
            .as_ref()
            .and_then(|(room, user)| core.move_to(room, user, &row.key)),
        _ => None,
    };
    match action {
        Some(action) => {
            app.client.dispatch(action);
            app.sync()
        }
        None => Task::none(),
    }
}

/// A Discord channel's or category's notification choices, which its
/// menu and the header's bell build alike.
struct Notify {
    category: bool,
    /// A channel listed under a category, whose default is the category's.
    in_category: bool,
    settings: Notifications,
}

impl Notify {
    fn kind(&self) -> &'static str {
        if self.category { "category" } else { "channel" }
    }

    /// Mute for a time, in a submenu (a click on the row itself mutes
    /// until it's turned back on), or while muted, unmute, saying till
    /// when.
    fn mute_row(&self, twelve: bool) -> MenuRow {
        let muted = self.settings.muted;
        let label = format!("{} {}", if muted { "Unmute" } else { "Mute" }, self.kind());
        let mut row = MenuRow {
            icon: Some(if muted { Icon::Bell } else { Icon::BellOff }),
            subtitle: self
                .settings
                .muted_until
                .map(|until| muted_until(until / 1000, twelve).into()),
            ..MenuRow::plain(
                MenuAct::Notify(NotificationChange::Mute(Muting::Off)),
                label,
            )
        };
        if !muted {
            row.submenu = mute_times(|muting| MenuAct::Notify(NotificationChange::Mute(muting)));
            row.click = Some(MenuAct::Notify(NotificationChange::Mute(Muting::Always)));
        }
        row
    }

    /// The levels, the one chosen checked; following the category's or the
    /// server's says which that is.
    fn levels(&self) -> Vec<MenuRow> {
        [
            None,
            Some(NotifyLevel::All),
            Some(NotifyLevel::Mentions),
            Some(NotifyLevel::Nothing),
        ]
        .into_iter()
        .map(|level| {
            let label = match level {
                None if self.in_category => "Use category default",
                None => "Use server default",
                Some(level) => level_name(level),
            };
            MenuRow {
                subtitle: level
                    .is_none()
                    .then(|| level_name(self.settings.inherited).into()),
                check: if self.settings.level == level {
                    Check::Current
                } else {
                    Check::None
                },
                ..MenuRow::plain(MenuAct::Notify(NotificationChange::Level(level)), label)
            }
        })
        .collect()
    }
}

/// The levels a server's notification submenu offers, the one that
/// applies checked, then its switches.
fn server_levels(known: &yapfast_core::chat::ServerNotifications) -> Vec<MenuRow> {
    let current = known.own.level.unwrap_or(known.own.inherited);
    let mut rows: Vec<MenuRow> = [
        NotifyLevel::All,
        NotifyLevel::Mentions,
        NotifyLevel::Nothing,
    ]
    .into_iter()
    .map(|level| MenuRow {
        check: if level == current {
            Check::Current
        } else {
            Check::None
        },
        ..MenuRow::plain(
            MenuAct::Notify(NotificationChange::Level(Some(level))),
            level_name(level),
        )
    })
    .collect();
    rows.push(MenuRow {
        separated: true,
        check: Check::Box(known.suppress_everyone),
        ..MenuRow::plain(
            MenuAct::Notify(NotificationChange::SuppressEveryone(
                !known.suppress_everyone,
            )),
            "Suppress @everyone and @here",
        )
    });
    rows.push(MenuRow {
        check: Check::Box(known.suppress_roles),
        ..MenuRow::plain(
            MenuAct::Notify(NotificationChange::SuppressRoles(!known.suppress_roles)),
            "Suppress all role @mentions",
        )
    });
    rows
}

/// A folder's notification levels and switches, for all its servers: a
/// level checked when they all have it, a switch ticked when they all have
/// it on, which turns it on for all of them, or off when it was.
fn folder_levels(
    known: &[(String, Option<yapfast_core::chat::ServerNotifications>)],
) -> Vec<MenuRow> {
    let known: Vec<_> = known.iter().filter_map(|(_, known)| *known).collect();
    let shared = |level: NotifyLevel| {
        !known.is_empty()
            && known
                .iter()
                .all(|known| known.own.level.unwrap_or(known.own.inherited) == level)
    };
    let mut rows: Vec<MenuRow> = [
        NotifyLevel::All,
        NotifyLevel::Mentions,
        NotifyLevel::Nothing,
    ]
    .into_iter()
    .map(|level| MenuRow {
        check: if shared(level) {
            Check::Current
        } else {
            Check::None
        },
        ..MenuRow::plain(
            MenuAct::Notify(NotificationChange::Level(Some(level))),
            level_name(level),
        )
    })
    .collect();
    let everyone = !known.is_empty() && known.iter().all(|known| known.suppress_everyone);
    let roles = !known.is_empty() && known.iter().all(|known| known.suppress_roles);
    rows.push(MenuRow {
        separated: true,
        check: Check::Box(everyone),
        ..MenuRow::plain(
            MenuAct::Notify(NotificationChange::SuppressEveryone(!everyone)),
            "Suppress @everyone and @here",
        )
    });
    rows.push(MenuRow {
        check: Check::Box(roles),
        ..MenuRow::plain(
            MenuAct::Notify(NotificationChange::SuppressRoles(!roles)),
            "Suppress all role @mentions",
        )
    });
    rows
}

/// When the last of a folder's timed mutes ends, if every server is muted
/// for a time; none while one is muted for good.
fn folder_muted_until(
    known: &[(String, Option<yapfast_core::chat::ServerNotifications>)],
) -> Option<u64> {
    known
        .iter()
        .map(|(_, known)| known.and_then(|known| known.own.muted_until))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .max()
}

/// A folder's menu's last rows: Servers, a list of its servers beside the
/// menu, each saying how it notifies, which a click opens; and Close all
/// folders while one is open, as in Discord.
fn folder_rows(
    app: &App,
    folder: &str,
    known: &[(String, Option<yapfast_core::chat::ServerNotifications>)],
    twelve: bool,
) -> Vec<MenuRow> {
    let state = app.client.state();
    let servers: Vec<MenuRow> = state
        .folder_servers(folder)
        .map(|server| {
            let notifies = known
                .iter()
                .find(|(id, _)| *id == server.id)
                .and_then(|(_, known)| *known)
                .map(|known| {
                    let own = known.own;
                    if own.muted {
                        own.muted_until.map_or_else(
                            || "Muted".to_owned(),
                            |until| muted_until(until / 1000, twelve),
                        )
                    } else {
                        level_name(own.level.unwrap_or(own.inherited)).to_owned()
                    }
                });
            let own = known
                .iter()
                .find(|(id, _)| *id == server.id)
                .and_then(|(_, known)| *known);
            MenuRow {
                subtitle: notifies.map(Into::into),
                side: own
                    .map(|own| server_choices(&server.id, &own, twelve))
                    .unwrap_or_default(),
                ..MenuRow::plain(MenuAct::GoServer, server.name.clone()).key(server.id.clone())
            }
        })
        .collect();
    let mut rows = Vec::new();
    if !servers.is_empty() {
        let count = servers.len();
        rows.push(MenuRow {
            icon: Some(Icon::Folder),
            separated: true,
            subtitle: Some(
                if count == 1 {
                    "1 server".to_owned()
                } else {
                    format!("{count} servers")
                }
                .into(),
            ),
            submenu: servers,
            ..MenuRow::plain(MenuAct::Event, "Servers")
        });
    }
    if !app.layout.open_folders.is_empty() {
        rows.push(MenuRow {
            icon: Some(Icon::ChevronUp),
            separated: rows.is_empty(),
            ..MenuRow::plain(MenuAct::CloseFolders, "Close all folders")
        });
    }
    rows
}

/// A folder's server's own choices, which a right click on it in the
/// folder's Servers list opens beside it, as its own menu has them in one
/// list: mute for each time (or Unmute server while it's muted, saying
/// till when), its levels and its two switches. Each changes that server
/// alone, by its id in the rows' keys.
fn server_choices(
    server: &str,
    known: &yapfast_core::chat::ServerNotifications,
    twelve: bool,
) -> Vec<MenuRow> {
    let own = known.own;
    let mut rows: Vec<MenuRow> = if own.muted {
        vec![MenuRow {
            icon: Some(Icon::Bell),
            subtitle: own
                .muted_until
                .map(|until| muted_until(until / 1000, twelve).into()),
            ..MenuRow::plain(
                MenuAct::NotifyServer(NotificationChange::Mute(Muting::Off)),
                "Unmute server",
            )
        }]
    } else {
        mute_times(|muting| MenuAct::NotifyServer(NotificationChange::Mute(muting)))
    };
    let current = own.level.unwrap_or(own.inherited);
    for (at, level) in [
        NotifyLevel::All,
        NotifyLevel::Mentions,
        NotifyLevel::Nothing,
    ]
    .into_iter()
    .enumerate()
    {
        rows.push(MenuRow {
            separated: at == 0,
            check: if level == current {
                Check::Current
            } else {
                Check::None
            },
            ..MenuRow::plain(
                MenuAct::NotifyServer(NotificationChange::Level(Some(level))),
                level_name(level),
            )
        });
    }
    rows.push(MenuRow {
        separated: true,
        check: Check::Box(known.suppress_everyone),
        ..MenuRow::plain(
            MenuAct::NotifyServer(NotificationChange::SuppressEveryone(
                !known.suppress_everyone,
            )),
            "Suppress @everyone and @here",
        )
    });
    rows.push(MenuRow {
        check: Check::Box(known.suppress_roles),
        ..MenuRow::plain(
            MenuAct::NotifyServer(NotificationChange::SuppressRoles(!known.suppress_roles)),
            "Suppress all role @mentions",
        )
    });
    rows.into_iter()
        .map(|row| row.key(server.to_owned()))
        .collect()
}

/// What a level is called, as Discord's client words it.
fn level_name(level: NotifyLevel) -> &'static str {
    match level {
        NotifyLevel::All => "All messages",
        NotifyLevel::Mentions => "Only @mentions",
        NotifyLevel::Nothing => "Nothing",
    }
}

/// How long a mute lasts, as its submenu offers: each time, as `act` makes
/// it.
pub fn mute_times(act: fn(Muting) -> MenuAct) -> Vec<MenuRow> {
    MUTE_TIMES
        .into_iter()
        .map(|muting| {
            let label: String = match muting {
                Muting::Minutes(15) => "For 15 minutes".into(),
                Muting::Minutes(60) => "For 1 hour".into(),
                Muting::Minutes(minutes) if minutes % 60 == 0 => {
                    format!("For {} hours", minutes / 60)
                }
                Muting::Minutes(minutes) => format!("For {minutes} minutes"),
                Muting::Always | Muting::Off => "Until I turn it back on".into(),
            };
            MenuRow::plain(act(muting), label)
        })
        .collect()
}

/// When a mute ends, as its Unmute row says it.
fn muted_until(seconds: u64, twelve_hour: bool) -> String {
    format!("Muted until {}", until(seconds, twelve_hour))
}

/// When something ends, a Unix time: the time today, or the day too.
pub fn until(seconds: u64, twelve_hour: bool) -> String {
    use yapfast_core::clock;
    let now = clock::now_ms() / 1000;
    let time = clock::time_on(seconds, twelve_hour);
    let on = clock::day(seconds);
    if on == clock::day(now) {
        time
    } else {
        format!("{on}, {time}")
    }
}

/// Which group a choice is in, for the lines drawn between them: what it
/// does; how this account stands with someone; notifications; copying and
/// the channel list; roles; moderation; a channel's management; deleting
/// or leaving.
pub fn group_of(choice: MenuChoice) -> u8 {
    match choice {
        MenuChoice::InviteToServer
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
        | MenuChoice::MuteFolder
        | MenuChoice::UnmuteFolder
        | MenuChoice::FolderNotifications
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
        MenuChoice::ServerNickname | MenuChoice::Roles | MenuChoice::MoveTo => 4,
        MenuChoice::Apps => 10,
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
        MenuChoice::TransferOwnership => 9,
        MenuChoice::ServerSettings
        | MenuChoice::EditServerProfile
        | MenuChoice::CreateChannel
        | MenuChoice::CreateCategory
        | MenuChoice::EditChannel
        | MenuChoice::DuplicateChannel => 6,
        MenuChoice::DeleteChannel | MenuChoice::LeaveServer | MenuChoice::LeaveGroup => 7,
        _ => 0,
    }
}

/// A choice's icon, in every menu that offers it.
pub fn icon(choice: MenuChoice) -> Icon {
    use MenuChoice as C;
    match choice {
        C::MarkRead => Icon::Check,
        C::JoinVoice | C::Call => Icon::Phone,
        C::OpenChat => Icon::ForumPost,
        C::Profile => Icon::User,
        C::Message => Icon::Dm,
        C::AddFriend | C::AcceptFriend | C::InviteToServer | C::Invite => Icon::UserPlus,
        C::CancelFriendRequest | C::CloseDm => Icon::X,
        C::Pin | C::PinChannel => Icon::Pin,
        C::Unpin | C::UnpinChannel => Icon::PinOff,
        C::AddNote => Icon::StickyNote,
        C::AddNickname | C::EditNickname => Icon::UserPen,
        C::LeaveGroup | C::LeaveServer | C::LeaveThread => Icon::LogOut,
        C::Apps => Icon::Slash,
        C::ServerSettings => Icon::Settings,
        C::Unfriend => Icon::UserMinus,
        C::Ignore | C::Unignore | C::HideMuted | C::RemoveFromChannelList | C::UnfollowCategory => {
            Icon::EyeOff
        }
        C::ShowAllChannels | C::AddToChannelList => Icon::Eye,
        C::Block | C::Unblock => Icon::Ban,
        C::Mute | C::ChannelMute | C::MuteServer | C::MuteFolder => Icon::BellOff,
        C::Unmute
        | C::ChannelNotifications
        | C::UnmuteServer
        | C::ServerNotifications
        | C::UnmuteFolder
        | C::FolderNotifications => Icon::Bell,
        C::CopyLink => Icon::Copy,
        C::CopyId => Icon::Id,
        C::Mention => Icon::AtSign,
        C::ServerNickname | C::EditChannel | C::EditServerProfile => Icon::Pencil,
        C::Roles => Icon::Shield,
        C::Timeout | C::RemoveTimeout => Icon::Clock,
        C::Kick => Icon::UserX,
        C::Ban => Icon::Gavel,
        C::TransferOwnership => Icon::Crown,
        C::LocalMute | C::SelfMute | C::ServerMute => Icon::MicOff,
        C::SoundboardMute => Icon::Soundboard,
        C::SelfDeafen | C::ServerDeafen => Icon::HeadphoneOff,
        C::MoveTo => Icon::Forward,
        C::Disconnect => Icon::PhoneOff,
        C::CreateChannel => Icon::CirclePlus,
        C::CreateCategory => Icon::FolderPlus,
        C::DuplicateChannel => Icon::CopyPlus,
        C::DeleteChannel => Icon::Trash,
        C::JoinThread => Icon::Thread,
    }
}

/// What a menu row says for a choice.
pub fn label(choice: MenuChoice) -> &'static str {
    match choice {
        MenuChoice::MarkRead => "Mark as read",
        MenuChoice::JoinVoice => "Join voice",
        MenuChoice::OpenChat => "Open chat",
        MenuChoice::Profile => "Profile",
        MenuChoice::Message => "Message",
        MenuChoice::AddFriend => "Add friend",
        MenuChoice::AcceptFriend => "Accept friend request",
        MenuChoice::CancelFriendRequest => "Cancel friend request",
        MenuChoice::Pin => "Pin",
        MenuChoice::Unpin => "Unpin",
        MenuChoice::Call => "Start call",
        MenuChoice::AddNote => "Add note",
        MenuChoice::AddNickname => "Add friend nickname",
        MenuChoice::EditNickname => "Change friend nickname",
        MenuChoice::CloseDm => "Close DM",
        MenuChoice::LeaveGroup => "Leave group",
        MenuChoice::InviteToServer => "Invite to server",
        MenuChoice::Apps => "Apps",
        MenuChoice::Unfriend => "Remove friend",
        MenuChoice::Ignore => "Ignore",
        MenuChoice::Unignore => "Stop ignoring",
        MenuChoice::Block => "Block",
        MenuChoice::Unblock => "Unblock",
        MenuChoice::Mute => "Mute",
        MenuChoice::Unmute => "Unmute",
        MenuChoice::CopyLink => "Copy link",
        MenuChoice::CopyId => "Copy ID",
        MenuChoice::Mention => "Mention",
        MenuChoice::ServerNickname => "Change nickname",
        MenuChoice::Roles => "Roles",
        MenuChoice::RemoveTimeout => "Remove timeout",
        MenuChoice::Timeout => "Time out",
        MenuChoice::Kick => "Kick",
        MenuChoice::Ban => "Ban",
        MenuChoice::TransferOwnership => "Transfer Ownership",
        MenuChoice::LocalMute | MenuChoice::SelfMute => "Mute",
        MenuChoice::SoundboardMute => "Mute Soundboard",
        MenuChoice::SelfDeafen => "Deafen",
        MenuChoice::MoveTo => "Move to",
        MenuChoice::ServerMute => "Server mute",
        MenuChoice::ServerDeafen => "Server deafen",
        MenuChoice::Disconnect => "Disconnect",
        MenuChoice::ChannelMute => "Mute",
        MenuChoice::ChannelNotifications => "Notification settings",
        MenuChoice::MuteServer => "Mute server",
        MenuChoice::UnmuteServer => "Unmute server",
        MenuChoice::ServerNotifications | MenuChoice::FolderNotifications => {
            "Notification settings"
        }
        MenuChoice::MuteFolder => "Mute folder",
        MenuChoice::UnmuteFolder => "Unmute folder",
        MenuChoice::ShowAllChannels => "Show all channels",
        MenuChoice::HideMuted => "Hide muted channels",
        MenuChoice::Invite => "Invite to channel",
        MenuChoice::PinChannel => "Pin channel to top",
        MenuChoice::UnpinChannel => "Unpin channel",
        MenuChoice::AddToChannelList => "Add to channel list",
        MenuChoice::RemoveFromChannelList => "Remove from channel list",
        MenuChoice::UnfollowCategory => "Unfollow channel category",
        MenuChoice::CreateChannel => "Create text channel",
        MenuChoice::CreateCategory => "Create category",
        MenuChoice::EditChannel => "Edit channel",
        MenuChoice::DuplicateChannel => "Duplicate channel",
        MenuChoice::DeleteChannel => "Delete channel",
        MenuChoice::LeaveServer => "Leave server",
        MenuChoice::JoinThread => "Join thread",
        MenuChoice::LeaveThread => "Leave thread",
        MenuChoice::EditServerProfile => "Edit server profile",
        MenuChoice::ServerSettings => "Server settings",
    }
}
