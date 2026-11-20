//! The application's tests, one file per area in `tests/`. What more than
//! one area builds on is here.

use super::{preview::preview_room, *};
use crate::{chat::RoomIndex, sounds::Sound};
#[cfg(feature = "discord")]
use yapfast_discord::marked::{ChannelMarks, MarkedServer, ServerMarks};

#[cfg(feature = "discord")]
fn discord_event(snapshot: yapfast_discord::service::Snapshot) -> Event {
    Event::Discord(Arc::new(snapshot))
}

/// Copies of the rooms or servers a list shares, as a network sends them.
fn owned<T: Clone>(list: &[Arc<T>]) -> Vec<T> {
    list.iter().map(|item| (**item).clone()).collect()
}

fn connected() -> Application {
    let mut app = Application::default();
    app.apply(Event::Connected {
        user: "@alice:test".into(),
        trusted: true,
        storage: "test",
    });
    app.apply(Event::Sync(true));
    app.apply(Event::Rooms(RoomIndex {
        rooms: vec![
            preview_room("dm", "DM", true, 2),
            preview_room("general", "general", false, 3),
            preview_room("games", "games", false, 3),
            ChatRoom {
                voice: true,
                ..preview_room("lounge", "Lounge", false, 3)
            },
            ChatRoom {
                voice: true,
                ..preview_room("party-voice", "Party", false, 3)
            },
        ],
        servers: vec![ChatServer {
            id: "friends".into(),
            name: "Friends".into(),
            channels: vec![
                "general".into(),
                "games".into(),
                "lounge".into(),
                "party-voice".into(),
            ],
            discord: false,
            muted: false,
            unavailable: false,
            icon: None,
            folder: None,
            hide_muted: false,
            community: false,
            pinned: Vec::new(),
            ..Default::default()
        }],
    }));
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![]),
    });
    app
}

fn send_command(effects: Vec<Effect>) -> Command {
    match effects.into_iter().next().expect("send effect") {
        Effect::Backend(command @ Command::Send { .. }) => command,
        _ => panic!("expected send command"),
    }
}

/// Rows as a backend sends them.
fn timeline(messages: Vec<ChatMessage>) -> Vec<Arc<ChatMessage>> {
    messages.into_iter().map(Arc::new).collect()
}

fn message(item: &str, own: bool) -> ChatMessage {
    ChatMessage {
        id: item.into(),
        item: item.into(),
        author: if own { "Alice" } else { "Bob" }.into(),
        body: format!("Body of {item}"),
        own,
        can_reply: true,
        can_edit: own,
        can_delete: own,
        can_react: true,
        ..Default::default()
    }
}

/// Messages in `direction` arrived; with `present` they reach the newest.
fn window(room: &str, direction: chat::Direction, present: bool) -> Event {
    Event::HistoryLoaded {
        room: room.into(),
        direction,
        history: chat::History::Loaded {
            start: false,
            present,
        },
    }
}

/// The first page of history arrived.
fn start(room: &str) -> Event {
    Event::HistoryLoaded {
        room: room.into(),
        direction: chat::Direction::Earlier,
        history: chat::History::Loaded {
            start: true,
            present: true,
        },
    }
}

fn member(id: &str, name: &str) -> ChatMember {
    ChatMember {
        bot: false,
        id: id.into(),
        name: name.into(),
        username: None,
        role: Role::Member,
        avatar: None,
        decoration: None,
        nameplate: None,
        style: None,
        group: None,
        presence: None,
        color: None,
        activity: None,
        activities: Default::default(),
        roles: Default::default(),
        timed_out_until: None,
        tag: None,
        group_icon: None,
        role_icon: None,
        boosting_since: None,
    }
}

/// A Discord server where Alice, "1", is a Mod: a category holding
/// #general and #news, where @everyone may not send, roles below and above
/// hers, and her notification settings, which hide muted channels.
#[cfg(feature = "discord")]
fn discord_server() -> Application {
    use yapfast_discord::{
        notifications::{Level, Mute, Notifications as Settings, Override},
        ready::{self, permission},
        service::{Snapshot, Status},
    };
    let bits = |bits: u64| bits.to_string();
    let server = ready::server(&serde_json::json!({
        "id": "10", "owner_id": "99",
        "properties": {"name": "Test", "default_message_notifications": 1},
        "roles": [
            {"id": "10", "name": "@everyone", "position": 0,
             "permissions": bits(permission::VIEW_CHANNEL | permission::SEND_MESSAGES)},
            {"id": "20", "name": "Mods", "position": 2, "color": 0x3498db,
             "permissions": bits(permission::KICK_MEMBERS | permission::MANAGE_ROLES)},
            {"id": "30", "name": "Helpers", "permissions": "0", "position": 1},
            {"id": "40", "name": "Bot", "permissions": "0", "position": 3, "managed": true},
        ],
        "channels": [
            {"id": "4", "type": 4, "name": "Talk", "position": 0},
            {"id": "11", "type": 0, "name": "general", "position": 0, "parent_id": "4"},
            {"id": "12", "type": 0, "name": "news", "position": 1, "parent_id": "4",
             "permission_overwrites": [{"id": "10", "type": 0, "allow": "0",
                                        "deny": bits(permission::SEND_MESSAGES)}]},
            {"id": "13", "type": 0, "name": "memes", "position": 2, "parent_id": "4"},
            {"id": "14", "type": 2, "name": "Lounge", "position": 3, "parent_id": "4"},
        ],
    }))
    .unwrap();
    let soon = crate::clock::now_ms() + 3_600_000;
    let notifications = Arc::new(Settings {
        hide_muted_channels: true,
        suppress_roles: true,
        overrides: [
            (
                "4".to_owned(),
                Override {
                    mute: Mute::default(),
                    level: Some(Level::All),
                    flags: 0,
                },
            ),
            (
                "12".to_owned(),
                Override {
                    mute: Mute {
                        muted: true,
                        until: Some(soon),
                    },
                    level: Some(Level::Nothing),
                    flags: 0,
                },
            ),
        ]
        .into(),
        ..Default::default()
    });
    // As the service marks them: #news is muted, #memes is muted and
    // mentions her since she read its message "d:3".
    let marks = ServerMarks {
        roles: vec!["20".into()],
        notifications,
        ..Default::default()
    };
    let server = MarkedServer::marked(server, marks, |channel| match channel.id.as_str() {
        "12" => ChannelMarks {
            muted: true,
            ..Default::default()
        },
        "13" => ChannelMarks {
            muted: true,
            mentions: 1,
            last_read: 3,
            ..Default::default()
        },
        _ => ChannelMarks::default(),
    });
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        servers: vec![server],
        ..Default::default()
    }));
    app
}

/// `app`'s Discord snapshot again, with its first server changed by `change`.
#[cfg(feature = "discord")]
fn change_server(app: &mut Application, change: impl FnOnce(&mut yapfast_discord::ready::Server)) {
    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    let listed = &mut snapshot.servers[0];
    // Each channel keeps its marks, wherever the change puts it.
    let was: HashMap<String, ChannelMarks> = listed
        .channels()
        .map(|channel| (channel.id.clone(), channel.marks))
        .collect();
    change(Arc::make_mut(&mut listed.server));
    let channels = listed
        .channels()
        .map(|channel| was.get(&channel.id).copied().unwrap_or_default())
        .collect();
    Arc::make_mut(&mut listed.marks).channels = channels;
    app.apply(discord_event(snapshot));
}

/// `server` with `marks` for the channels they name by id, and the
/// defaults for the rest.
#[cfg(feature = "discord")]
fn marked(server: yapfast_discord::ready::Server, marks: &[(&str, ChannelMarks)]) -> MarkedServer {
    MarkedServer::marked(server, ServerMarks::default(), |channel| {
        marks
            .iter()
            .find(|(id, _)| *id == channel.id)
            .map_or_else(ChannelMarks::default, |(_, marks)| *marks)
    })
}

/// Changes this account's marks in the first server, as a snapshot would.
#[cfg(feature = "discord")]
fn change_marks(
    app: &mut Application,
    change: impl FnOnce(&yapfast_discord::ready::Server, &mut ServerMarks),
) {
    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    let listed = &mut snapshot.servers[0];
    change(&listed.server, Arc::make_mut(&mut listed.marks));
    app.apply(discord_event(snapshot));
}

#[cfg(feature = "discord")]
mod arrange;
#[cfg(feature = "discord")]
mod audit_log;
mod calls;
#[cfg(feature = "discord")]
mod channel_list;
#[cfg(feature = "discord")]
mod channel_settings;
#[cfg(feature = "discord")]
mod commands;
mod composer;
mod direct_messages;
mod discord_account;
#[cfg(feature = "discord")]
mod expressions;
mod forwarding;
mod kept_pictures;
#[cfg(feature = "discord")]
mod links;
mod media;
mod mentions;
mod menus;
#[cfg(feature = "discord")]
mod moderation;
mod navigation;
#[cfg(feature = "discord")]
mod ownership;
#[cfg(feature = "discord")]
mod people;
mod pickers;
mod polls;
#[cfg(feature = "discord")]
mod profile_editor;
mod profiles;
#[cfg(feature = "discord")]
mod server_settings;
#[cfg(feature = "discord")]
mod servers;
mod session;
#[cfg(feature = "discord")]
mod slash_commands;
#[cfg(feature = "discord")]
mod storefront;
mod threads;
mod timeline;
mod unread;
