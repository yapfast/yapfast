//! A Discord server to check a member's menu in, for this crate's tests and
//! other crates' through the `test-fixtures` feature.

use std::sync::RwLock;

use serde_json::json;
use yapfast_discord::{
    marked::{MarkedServer, ServerMarks},
    message::Names,
    ready::{self, permission},
    service::{SharedNames, Snapshot, Status},
};

use super::*;

/// Alice, "1", is Staff in a server Owen, "99", owns. Her roles let her
/// kick, ban, time out, rename and manage roles below Staff. Cara, a Lead,
/// is above her; Dave, her friend, has no role; Gus holds Wardens, a low
/// role with Administrator; Hal is timed out. Everyone may change their own
/// nickname, and Alice has #general open, "discord:10:11", in the server
/// "discord:10". "8" is a bot. `alice` names her roles: "30" for Staff,
/// who may also mute, deafen and move members in voice. Dave, server
/// muted, and Alice, on another client, are in the voice channel Lounge,
/// "discord:10:12"; Den, "13", is another, and Vault, "14", is closed to
/// everyone but Leads.
pub fn member_server(alice: &[&str]) -> Application {
    let bits = |bits: u64| bits.to_string();
    let staff = permission::KICK_MEMBERS
        | permission::BAN_MEMBERS
        | permission::MODERATE_MEMBERS
        | permission::MANAGE_ROLES
        | permission::MANAGE_NICKNAMES
        | permission::MUTE_MEMBERS
        | permission::DEAFEN_MEMBERS
        | permission::MOVE_MEMBERS;
    let server = ready::server(&json!({
        "id": "10", "owner_id": "99",
        "properties": {"name": "Test"},
        "roles": [
            {"id": "10", "name": "@everyone", "position": 0, "permissions": bits(
                permission::VIEW_CHANNEL | permission::SEND_MESSAGES | permission::CHANGE_NICKNAME
                | permission::CONNECT)},
            {"id": "60", "name": "Wardens", "position": 1, "permissions": bits(permission::ADMINISTRATOR)},
            {"id": "25", "name": "Bot", "position": 2, "permissions": "0", "managed": true},
            {"id": "20", "name": "Crew", "position": 3, "permissions": "0", "color": 0x3498db},
            {"id": "30", "name": "Staff", "position": 4, "permissions": bits(staff)},
            {"id": "40", "name": "Leads", "position": 5, "permissions": "0"},
        ],
        "channels": [
            {"id": "11", "type": 0, "name": "general", "position": 0},
            {"id": "12", "type": 2, "name": "Lounge", "position": 1},
            {"id": "13", "type": 2, "name": "Den", "position": 2},
            {"id": "14", "type": 2, "name": "Vault", "position": 3, "permission_overwrites": [
                {"id": "10", "type": 0, "allow": "0", "deny": bits(permission::CONNECT)},
                {"id": "40", "type": 0, "allow": bits(permission::CONNECT), "deny": "0"},
            ]},
        ],
    }))
    .unwrap();
    let marks = ServerMarks {
        roles: alice.iter().map(|role| (*role).to_owned()).collect(),
        ..Default::default()
    };
    let mut names = Names::default();
    let soon = chrono::Utc::now() + chrono::Duration::hours(1);
    for (user, name, roles, nick, until) in [
        ("1", "Alice", alice.to_vec(), Some("Al"), None),
        ("3", "Cara", vec!["40"], None, None),
        ("4", "Dave", vec!["20"], None, None),
        ("6", "Gus", vec!["60"], None, None),
        ("7", "Hal", vec![], None, Some(soon.to_rfc3339())),
        ("99", "Owen", vec![], None, None),
    ] {
        names.learn_member(
            "10",
            &json!({
                "user": {"id": user, "username": name.to_lowercase(), "global_name": name},
                "roles": roles, "nick": nick, "communication_disabled_until": until,
            }),
        );
    }
    names.learn_user(&json!({"id": "8", "username": "helper", "bot": true}));
    let mut app = Application::default();
    app.apply(Event::Discord(Arc::new(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        servers: vec![MarkedServer {
            server: Arc::new(server),
            marks: Arc::new(marks),
        }],
        friends: vec!["4".into()].into(),
        friend_list: vec![yapfast_discord::service::Friend {
            user_id: "4".into(),
            name: "Dave".into(),
            avatar: String::new(),
            nickname: None,
            presence: None,
            activity: None,
            activities: Default::default(),
        }]
        .into(),
        names: SharedNames::new(Arc::new(RwLock::new(names))),
        voice: [(
            "12".to_owned(),
            vec![
                yapfast_discord::service::VoiceMember {
                    user_id: "4".into(),
                    name: "Dave".into(),
                    avatar: String::new(),
                    muted: true,
                    deafened: false,
                    server_muted: true,
                    server_deafened: false,
                    streaming: false,
                    video: false,
                    elsewhere: false,
                },
                yapfast_discord::service::VoiceMember {
                    user_id: "1".into(),
                    name: "Alice".into(),
                    avatar: String::new(),
                    muted: false,
                    deafened: false,
                    server_muted: false,
                    server_deafened: false,
                    streaming: false,
                    video: false,
                    elsewhere: true,
                },
            ],
        )]
        .into(),
        ..Default::default()
    })));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:11".into()));
    let listed = |id: &str, name: &str| ChatMember {
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
    };
    app.apply(Event::Members {
        room: "discord:10:11".into(),
        members: Ok(["Al:1", "Cara:3", "Dave:4", "Gus:6", "Hal:7", "Owen:99"]
            .iter()
            .map(|entry| {
                let (name, id) = entry.split_once(':').unwrap();
                listed(id, name)
            })
            .collect()),
    });
    app
}

/// `app` with Alice free to invite to "Test" and in two more servers:
/// "Club", where she may invite but not to #staff, and whose #news comes
/// first by position though its category lists it after #lobby; and "Den",
/// where only #den's overwrite lets her invite.
pub fn with_clubs(app: &mut Application) {
    let bits = |bits: u64| bits.to_string();
    let invite = bits(permission::VIEW_CHANNEL | permission::CREATE_INSTANT_INVITE);
    let club = ready::server(&json!({
        "id": "50", "owner_id": "99", "properties": {"name": "Club"},
        "roles": [{"id": "50", "name": "@everyone", "position": 0, "permissions": invite}],
        "channels": [
            {"id": "51", "type": 0, "name": "staff", "position": 0, "permission_overwrites": [
                {"id": "50", "type": 0, "allow": "0", "deny": bits(permission::CREATE_INSTANT_INVITE)}]},
            {"id": "52", "type": 0, "name": "lobby", "position": 2},
            {"id": "53", "type": 4, "name": "Info", "position": 0},
            {"id": "54", "type": 0, "name": "news", "position": 1, "parent_id": "53"},
        ],
    }))
    .unwrap();
    let den = ready::server(&json!({
        "id": "60", "owner_id": "99", "properties": {"name": "Den"},
        "roles": [{"id": "60", "name": "@everyone", "position": 0,
                   "permissions": bits(permission::VIEW_CHANNEL)}],
        "channels": [{"id": "61", "type": 0, "name": "den", "position": 0, "permission_overwrites": [
            {"id": "60", "type": 0, "allow": bits(permission::CREATE_INSTANT_INVITE), "deny": "0"}]}],
    }))
    .unwrap();
    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    let mut test = ready::Server::clone(&snapshot.servers[0]);
    test.put_role(&json!({"id": "10", "name": "@everyone", "position": 0,
        "permissions": bits(permission::VIEW_CHANNEL | permission::SEND_MESSAGES
            | permission::CHANGE_NICKNAME | permission::CREATE_INSTANT_INVITE)}));
    snapshot.servers = vec![
        MarkedServer {
            server: Arc::new(test),
            marks: snapshot.servers[0].marks.clone(),
        },
        club.into(),
        den.into(),
    ];
    app.apply(Event::Discord(Arc::new(snapshot)));
}
