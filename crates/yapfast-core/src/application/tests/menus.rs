//! The menus of conversations, servers, folders and people.

use super::*;

#[test]
fn a_folder_marks_its_servers_read() {
    let mut app = connected();
    let folder = chat::ServerFolder {
        id: "discord-folder:7".into(),
        name: None,
        color: None,
    };
    let mut rooms = owned(&app.state.rooms);
    rooms.push(preview_room("standup", "standup", false, 3));
    let mut servers = owned(&app.state.servers);
    servers[0].folder = Some(folder.clone());
    servers.push(ChatServer {
        id: "work".into(),
        name: "Work".into(),
        channels: vec!["standup".into()],
        folder: Some(folder),
        ..servers[0].clone()
    });
    app.apply(Event::Rooms(RoomIndex {
        rooms: rooms.clone(),
        servers: servers.clone(),
    }));
    assert!(
        app.state.menu_choices("discord-folder:7").is_empty(),
        "nothing unread"
    );

    for (id, unread) in [("general", 4), ("standup", 1)] {
        rooms.iter_mut().find(|room| room.id == id).unwrap().unread = unread;
    }
    app.apply(Event::Rooms(RoomIndex { rooms, servers }));
    assert_eq!(
        app.state.menu_choices("discord-folder:7"),
        [MenuChoice::MarkRead]
    );
    let marked: Vec<String> = app
        .dispatch(Action::MarkRead("discord-folder:7".into()))
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Backend(Command::MarkRead { room, .. }) => Some(room),
            _ => None,
        })
        .collect();
    assert_eq!(marked, ["general", "standup"]);
}

#[test]
fn menus_offer_what_fits_their_conversation_or_server() {
    use MenuChoice::*;
    let mut app = connected();
    let mut rooms = owned(&app.state.rooms);
    rooms
        .iter_mut()
        .find(|room| room.id == "general")
        .unwrap()
        .unread = 4;
    rooms
        .iter_mut()
        .find(|room| room.id == "games")
        .unwrap()
        .mentions = 1;
    let servers = owned(&app.state.servers);
    app.apply(Event::Rooms(RoomIndex { rooms, servers }));
    assert_eq!(
        app.state.menu_choices("dm"),
        [Profile, Pin, CopyLink, CopyId]
    );
    assert_eq!(
        app.state.menu_choices("general"),
        [MarkRead, CopyLink, CopyId]
    );
    assert_eq!(
        app.state.menu_choices("lounge"),
        [JoinVoice, CopyLink, CopyId]
    );
    assert_eq!(
        app.state.menu_choices("friends"),
        [MarkRead, CopyLink, CopyId]
    );
    assert!(app.state.menu_choices("gone").is_empty());

    let marked = |effects: Vec<Effect>| -> Vec<String> {
        effects
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Backend(Command::MarkRead { room, .. }) => Some(room),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        marked(app.dispatch(Action::MarkRead("general".into()))),
        ["general"]
    );
    assert!(
        marked(app.dispatch(Action::MarkRead("lounge".into()))).is_empty(),
        "nothing unread"
    );
    // A server marks each of its unread channels read.
    assert_eq!(
        marked(app.dispatch(Action::MarkRead("friends".into()))),
        ["general", "games"]
    );

    // The preview's rooms are on no network, and its calls join at once.
    let mut app = Application::default();
    app.dispatch(Action::Preview);
    assert_eq!(app.state.menu_choices("lounge"), [JoinVoice]);
    assert!(app.state.menu_choices("friends").is_empty());
    app.dispatch(Action::RequestJoinVoice("lounge".into()));
    assert!(app.state.menu_choices("lounge").is_empty(), "already in it");

    // A Discord channel's people and messages offer its apps' user and
    // message commands under Apps, once a menu opening asked for its index.
    #[cfg(feature = "discord")]
    {
        use yapfast_discord::commands::CommandIndex;
        const GENERAL: &str = "discord:10:11";
        let mut app = discord_server();
        app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
        app.dispatch(Action::SelectRoom(GENERAL.into()));
        assert!(!app.person_choices("2").contains(&Apps), "no index yet");
        let asked = app.dispatch(Action::AskAppCommands);
        assert!(asked.iter().any(|effect| matches!(effect,
            Effect::Backend(Command::Discord(DiscordCommand::CommandIndex { room })) if room == GENERAL)));
        // Status Bot's Wave needs Use Application Commands, which Alice
        // lacks in "Test"; Notes is installed for her, so its work anywhere.
        let server = CommandIndex::parse(
            &serde_json::json!({
                "applications": [{"id": "7", "name": "Status Bot", "bot_id": "7", "flags": "0"}],
                "application_commands": [
                    {"id": "6", "type": 2, "application_id": "7", "name": "Wave", "description": "", "version": "1"}],
            }),
            false,
        );
        let own = CommandIndex::parse(
            &serde_json::json!({
                "applications": [{"id": "8", "name": "Notes", "flags": "0"}],
                "application_commands": [
                    {"id": "1", "type": 1, "application_id": "8", "name": "note", "description": "Keeps a note",
                     "integration_types": [1], "contexts": [0, 1, 2], "version": "1"},
                    {"id": "4", "type": 3, "application_id": "8", "name": "Save note", "description": "",
                     "integration_types": [1], "contexts": [0, 1, 2], "version": "1"},
                    {"id": "5", "type": 2, "application_id": "8", "name": "Note about", "description": "",
                     "integration_types": [1], "contexts": [0, 1, 2], "version": "1"}],
            }),
            true,
        );
        app.apply(Event::CommandIndex {
            room: GENERAL.into(),
            index: Ok(Arc::new(own.merged(&server))),
        });
        let names = |target| -> Vec<String> {
            app.commands_for_target(GENERAL, target)
                .into_iter()
                .map(|command| command.name)
                .collect()
        };
        assert_eq!(names(CommandTarget::Message), ["Save note"]);
        assert_eq!(names(CommandTarget::User), ["Note about"]);
        let choices = app.person_choices("2");
        assert!(choices.contains(&Apps));
        // Before how this account stands with them, as Discord's has it.
        if let Some(befriend) = choices.iter().position(|choice| *choice == AddFriend) {
            assert!(choices.iter().position(|choice| *choice == Apps) < Some(befriend));
        }
        assert!(
            !app.person_choices("@bob:matrix.org").contains(&Apps),
            "not on Discord"
        );

        let ran = |effects: Vec<Effect>| -> Vec<(String, Option<String>, serde_json::Value)> {
            effects
                .into_iter()
                .filter_map(|effect| match effect {
                    Effect::Backend(Command::Discord(DiscordCommand::RunCommand(run))) => Some((
                        run.command["name"].as_str().unwrap_or_default().to_owned(),
                        run.target,
                        run.options,
                    )),
                    _ => None,
                })
                .collect()
        };
        let message = |item: &str, flags: u64| {
            Arc::new(ChatMessage {
                id: item.into(),
                item: item.into(),
                flags,
                ..Default::default()
            })
        };
        app.state.messages = Arc::new(vec![message("d:5", 0), message("d:6", 1 << 6)]);
        let run = |app: &mut Application, command: &str, target: &str| {
            ran(app.dispatch(Action::RunAppCommand {
                command: command.into(),
                target: Some(target.into()),
            }))
        };
        // On the message by its Discord id, and on the person, with no
        // options; each shows as sending.
        assert_eq!(
            run(&mut app, "4", "d:5"),
            [(
                "Save note".to_owned(),
                Some("5".to_owned()),
                serde_json::json!([])
            )]
        );
        assert_eq!(
            run(&mut app, "5", "2"),
            [(
                "Note about".to_owned(),
                Some("2".to_owned()),
                serde_json::json!([])
            )]
        );
        let sending = app
            .state
            .messages
            .iter()
            .filter(|message| message.item.starts_with("c:"))
            .count();
        assert_eq!(sending, 2);
        // Not on an answer only Alice sees, not a user command on a
        // message or a message command on a person, nor one she can't use.
        assert!(run(&mut app, "4", "d:6").is_empty());
        assert!(run(&mut app, "5", "d:5").is_empty());
        assert!(run(&mut app, "4", "2").is_empty());
        assert!(run(&mut app, "6", "2").is_empty());
    }
}

#[test]
#[cfg(feature = "discord")]
fn someones_menu_blocks_and_unblocks_them_but_never_this_account() {
    use MenuChoice::*;
    use chat::{Profile, RelationshipChange};
    use yapfast_discord::service::{BlockedUser, Friend, Snapshot, Status};
    let profile = "user profile overflow menu";
    let snapshot = |eve_blocked: bool| Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        friends: vec!["2".into()].into(),
        friend_list: vec![Friend {
            decoration: None,
            user_id: "2".into(),
            name: "Annie".into(),
            avatar: String::new(),
            nickname: None,
            since: None,
            presence: None,
            activity: None,
            activities: Default::default(),
        }]
        .into(),
        blocked: if eve_blocked {
            vec!["5".into()].into()
        } else {
            vec![].into()
        },
        blocked_list: if eve_blocked {
            vec![BlockedUser {
                user_id: "5".into(),
                name: "Eve".into(),
                username: None,
                avatar: String::new(),
            }]
            .into()
        } else {
            vec![].into()
        },
        ..Default::default()
    };
    let mut app = Application::default();
    app.apply(discord_event(snapshot(false)));
    // Away from any conversation, as a mutual friend's or a linked profile
    // opens: a friend, a stranger, and this account, which can't block itself.
    assert_eq!(app.state.person_choices("2"), [Message, Block, CopyId]);
    assert_eq!(app.state.person_choices("5"), [AddFriend, Block, CopyId]);
    assert_eq!(app.state.person_choices("1"), [CopyId]);
    app.apply(discord_event(snapshot(true)));
    assert_eq!(
        app.state.person_choices("5"),
        [Unblock, CopyId],
        "no friend request to someone blocked"
    );
    app.apply(discord_event(snapshot(false)));

    // Blocking from a profile asks first, naming them as their profile does,
    // and then says it came from the profile's menu.
    app.state.profile = Some(Arc::new(ProfileView {
        profile: Profile {
            id: "5".into(),
            name: "Eve".into(),
            ..Default::default()
        },
        ..Default::default()
    }));
    assert!(
        app.dispatch(Action::AskPerson {
            user: "5".into(),
            ask: PersonAsk::Block,
            location: profile,
        })
        .is_empty()
    );
    let dialog = app.state.person_dialog.as_ref().unwrap();
    assert_eq!((dialog.name.as_str(), dialog.location), ("Eve", profile));
    assert!(matches!(
        app.dispatch(Action::AnswerPerson(String::new())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Relationship {
            user,
            change: RelationshipChange::Block,
            location: "user profile overflow menu",
        }))] if user == "5"
    ));
    // Not this account, whatever asks, even with its own profile open.
    app.state.profile = Some(Arc::new(ProfileView {
        profile: Profile {
            id: "1".into(),
            name: "Alice".into(),
            ..Default::default()
        },
        ..Default::default()
    }));
    assert!(
        app.dispatch(Action::AskPerson {
            user: "1".into(),
            ask: PersonAsk::Block,
            location: profile,
        })
        .is_empty()
    );
    assert_eq!(app.state.person_dialog, None);
    // Nor a request to someone blocked, whatever asks.
    app.apply(discord_event(snapshot(true)));
    assert!(app.dispatch(Action::Befriend("5".into())).is_empty());
}

#[test]
#[cfg(feature = "discord")]
fn a_group_dms_creator_is_its_owner_and_a_dm_has_none() {
    use yapfast_discord::service::{Conversation, Snapshot, Status};
    let mut app = connected();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        conversations: vec![
            Arc::new(Conversation {
                id: "30".into(),
                person: Some("2".into()),
                people: 2,
                ..Default::default()
            }),
            Arc::new(Conversation {
                id: "31".into(),
                group: true,
                people: 3,
                owner: Some("2".into()),
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    assert_eq!(app.server_owner("discord:@me:31").as_deref(), Some("2"));
    assert_eq!(app.server_owner("discord:@me:30"), None);
}

#[test]
#[cfg(feature = "discord")]
fn a_discord_dms_menu_manages_its_person_and_notifications() {
    use MenuChoice::*;
    use chat::{Muting, RelationshipChange};
    use yapfast_discord::service::{BlockedUser, Conversation, Friend, Snapshot, Status};
    let snapshot = |friends: bool, muted: bool, blocked: bool, ignored: bool| Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        friends: if friends {
            vec!["2".into()].into()
        } else {
            vec![].into()
        },
        friend_list: if friends {
            vec![Friend {
                decoration: None,
                user_id: "2".into(),
                name: "Annie".into(),
                avatar: String::new(),
                nickname: Some("Annie".into()),
                since: None,
                presence: None,
                activity: None,
                activities: Default::default(),
            }]
            .into()
        } else {
            vec![].into()
        },
        blocked: if blocked {
            vec!["2".into()].into()
        } else {
            vec![].into()
        },
        blocked_list: if blocked {
            vec![BlockedUser {
                user_id: "2".into(),
                name: "Annie".into(),
                username: None,
                avatar: String::new(),
            }]
            .into()
        } else {
            vec![].into()
        },
        ignored: if ignored {
            vec!["2".into()].into()
        } else {
            vec![].into()
        },
        conversations: vec![
            Arc::new(Conversation {
                id: "30".into(),
                name: "Annie".into(),
                person: Some("2".into()),
                people: 2,
                muted,
                ..Default::default()
            }),
            Arc::new(Conversation {
                id: "31".into(),
                name: "Group".into(),
                group: true,
                people: 3,
                ..Default::default()
            }),
        ],
        ..Default::default()
    };
    let mut app = Application::default();
    app.apply(discord_event(snapshot(true, false, false, false)));
    let dm = "discord:@me:30";
    assert_eq!(
        app.state.menu_choices(dm),
        [
            Profile,
            Pin,
            Call,
            AddNote,
            EditNickname,
            CloseDm,
            Unfriend,
            Ignore,
            Block,
            Mute,
            CopyLink,
            CopyId
        ]
    );
    // A group has no one person to manage.
    assert_eq!(
        app.state.menu_choices("discord:@me:31"),
        [Pin, Call, Mute, LeaveGroup, CopyLink, CopyId]
    );
    app.apply(discord_event(snapshot(false, true, true, true)));
    assert_eq!(
        app.state.menu_choices(dm),
        [
            Profile, Pin, Call, AddNote, CloseDm, Unignore, Unblock, Unmute, CopyLink, CopyId
        ],
        "no nickname nor unfriending for someone who isn't a friend, nor a request to someone blocked"
    );
    app.apply(discord_event(snapshot(false, false, false, false)));
    assert!(app.state.menu_choices(dm).contains(&AddFriend));
    // In the DM, its person's menu, as their avatar or name opens it, is the
    // DM's, less what's about the conversation in the list.
    app.dispatch(Action::Open(dm.into()));
    let mut in_dm = app.state.menu_choices(dm);
    in_dm.retain(|choice| *choice != CopyLink);
    assert_eq!(app.state.person_choices("2"), in_dm);
    app.apply(discord_event(snapshot(false, true, true, true)));

    // Pinning puts it first on this device, until it's unpinned or closed.
    app.dispatch(Action::Pin {
        room: dm.into(),
        pinned: true,
    });
    assert_eq!(app.state.pinned, [dm]);
    assert_eq!(app.state.menu_choices(dm)[1], Unpin);
    assert!(matches!(
        app.dispatch(Action::CloseDm(dm.into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::CloseDm { room }))] if room == dm
    ));
    assert!(app.state.pinned.is_empty());

    // Muting goes to Discord; a menu that offers unmuting doesn't mute.
    assert!(
        app.dispatch(Action::MuteDm {
            room: dm.into(),
            muting: Muting::Minutes(15),
        })
        .is_empty()
    );
    assert!(matches!(
        app.dispatch(Action::MuteDm { room: dm.into(), muting: Muting::Off }).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Mute { room, muting: Muting::Off }))] if room == dm
    ));

    // A note loads, then saves; the dialog closes once Discord has it.
    app.apply(discord_event(snapshot(true, false, false, false)));
    assert!(matches!(
        app.dispatch(Action::AskPerson { user: "2".into(), ask: PersonAsk::Note, location: "ContextMenu" }).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Note { user }))] if user == "2"
    ));
    assert_eq!(app.state.person_dialog.as_ref().unwrap().text, None);
    assert!(
        app.dispatch(Action::AnswerPerson("early".into()))
            .is_empty(),
        "not before the note is here"
    );
    app.apply(Event::Note {
        user: "2".into(),
        result: Ok("Likes tea".into()),
    });
    assert_eq!(
        app.state.person_dialog.as_ref().unwrap().text.as_deref(),
        Some("Likes tea")
    );
    assert!(matches!(
        app.dispatch(Action::AnswerPerson(" Likes coffee ".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::SetNote { user, note }))]
            if user == "2" && note == "Likes coffee"
    ));
    app.apply(Event::PersonSaved(Err("Discord refused".into())));
    let dialog = app.state.person_dialog.as_ref().unwrap();
    assert!(!dialog.saving && dialog.error.as_deref() == Some("Discord refused"));
    app.dispatch(Action::AnswerPerson("Likes coffee".into()));
    app.apply(Event::PersonSaved(Ok(())));
    assert_eq!(app.state.person_dialog, None);

    // The nickname starts from the one there is.
    app.dispatch(Action::AskPerson {
        user: "2".into(),
        ask: PersonAsk::Nickname,
        location: "ContextMenu",
    });
    assert_eq!(
        app.state.person_dialog.as_ref().unwrap().text.as_deref(),
        Some("Annie")
    );
    app.dispatch(Action::ClosePersonDialog);

    // Blocking asks first; ignoring doesn't.
    assert!(
        app.dispatch(Action::AskPerson {
            user: "2".into(),
            ask: PersonAsk::Block,
            location: "ContextMenu",
        })
        .is_empty()
    );
    assert!(matches!(
        app.dispatch(Action::AnswerPerson(String::new())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Relationship {
            user,
            change: RelationshipChange::Block,
            location: "ContextMenu",
        }))] if user == "2"
    ));
    assert_eq!(app.state.person_dialog, None);
    assert!(matches!(
        app.dispatch(Action::ChangeRelationship {
            user: "2".into(),
            change: RelationshipChange::Ignore,
            location: "ContextMenu",
        })
        .as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::Relationship { .. }
        ))]
    ));
}

#[test]
fn matrix_peoples_menus_offer_a_dm_but_not_to_this_account() {
    use MenuChoice::*;
    let mut app = connected();
    // In an open room, a Matrix ID is enough for a profile card.
    assert_eq!(app.state.person_choices("@alice:test"), [Profile, CopyId]);
    // In their own DM, someone's menu is the DM's.
    assert_eq!(
        app.state.person_choices("@dm:preview.local"),
        [Profile, Pin, CopyId]
    );
    assert_eq!(
        app.state.person_choices("@bob:test"),
        [Profile, Message, CopyId]
    );
    // An existing DM opens; a new one starts.
    app.dispatch(Action::SelectRoom("general".into()));
    app.dispatch(Action::MessagePerson("@dm:preview.local".into()));
    assert_eq!(app.state.selected.as_deref(), Some("dm"));
    assert!(matches!(
        app.dispatch(Action::MessagePerson("@bob:test".into())).as_slice(),
        [Effect::Backend(Command::StartDm { id, discord: false })] if id == "@bob:test"
    ));
}
