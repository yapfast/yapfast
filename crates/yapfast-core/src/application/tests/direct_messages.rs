//! Discord's direct messages, groups and friends.

use super::*;

#[test]
fn new_dms_find_people_on_both_networks_and_open_once_listed() {
    let mut app = connected();
    assert!(matches!(
        app.dispatch(Action::OpenNewDm).as_slice(),
        [Effect::Backend(Command::FindPeople { query })] if query.is_empty()
    ));
    app.dispatch(Action::NewDmQuery("sa".into()));
    let contact = |id: &str, name: &str, discord: bool| Contact {
        id: id.into(),
        name: name.into(),
        avatar: None,
        detail: String::new(),
        discord,
    };
    app.apply(Event::People {
        query: "sa".into(),
        discord: false,
        people: vec![contact("@sam:test", "Sam", false)],
    });
    app.apply(Event::People {
        query: "sa".into(),
        discord: true,
        people: vec![contact("5", "Sasha", true)],
    });
    // An answer to an older query is dropped.
    app.apply(Event::People {
        query: "s".into(),
        discord: true,
        people: vec![contact("6", "Sid", true)],
    });
    let new_dm = app.state.new_dm.clone().unwrap();
    let names: Vec<_> = new_dm.people().map(|person| person.name.as_str()).collect();
    assert_eq!(names, ["Sasha", "Sam"]);
    assert!(matches!(
        app.dispatch(Action::StartDm(new_dm.matrix[0].clone())).as_slice(),
        [Effect::Backend(Command::StartDm { id, discord: false })] if id == "@sam:test"
    ));
    assert!(
        app.dispatch(Action::StartDm(new_dm.matrix[0].clone()))
            .is_empty(),
        "one at a time"
    );
    app.apply(Event::DmStarted(Err("offline".into())));
    let new_dm = app.state.new_dm.as_ref().unwrap();
    assert!(!new_dm.starting && new_dm.error.as_deref() == Some("offline"));
    app.dispatch(Action::StartDm(contact("@sam:test", "Sam", false)));
    // The room isn't listed yet; it opens when it is.
    app.apply(Event::DmStarted(Ok("!sam".into())));
    assert!(app.state.new_dm.is_none());
    assert_eq!(app.state.selected.as_deref(), Some("dm"));
    let mut rooms = owned(&app.state.rooms);
    rooms.push(preview_room("!sam", "Sam", true, 2));
    let effects = app.apply(Event::Rooms(RoomIndex {
        rooms,
        servers: owned(&app.state.servers),
    }));
    assert_eq!(app.state.selected.as_deref(), Some("!sam"));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::Open(id)) if id == "!sam"))
    );
}

#[test]
#[cfg(feature = "discord")]
fn discord_text_channels_write_without_matrix_trust_and_read_once_read() {
    use yapfast_discord::{
        ready::{Access, Channel, Server},
        service::{Snapshot, Status},
    };
    let snapshot = |unread: bool| Snapshot {
        status: Status::Online,
        servers: vec![marked(
            Server {
                id: "10".into(),
                name: "Test".into(),
                text_channels: vec![
                    Channel {
                        id: "21".into(),
                        name: "general".into(),
                        ..Default::default()
                    },
                    Channel {
                        id: "22".into(),
                        name: "news".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            &[
                (
                    "21",
                    ChannelMarks {
                        unread,
                        ..Default::default()
                    },
                ),
                (
                    "22",
                    ChannelMarks {
                        access: Access::Locked,
                        ..Default::default()
                    },
                ),
            ],
        )],
        ..Default::default()
    };
    let mut app = connected();
    app.state.trusted = false;
    app.dispatch(Action::SyncDiscordReads(true));
    app.apply(discord_event(snapshot(true)));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    let effects = app.dispatch(Action::SelectRoom("discord:10:21".into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Open(id))] if id == "discord:10:21"
    ));
    let effects = app.apply(Event::Timeline {
        room: "discord:10:21".into(),
        messages: timeline(vec![]),
    });
    assert!(effects.is_empty(), "shown, but not read yet");
    // Read once its newest message is in view, with the window in front.
    assert!(matches!(
        app.dispatch(Action::Reading(true)).as_slice(),
        [Effect::Backend(Command::MarkRead { room, sync: true })] if room == "discord:10:21"
    ));
    assert!(app.state.can_write(), "Discord doesn't need Matrix trust");
    assert!(app.state.can_attach());
    assert!(
        app.apply(discord_event(snapshot(false))).is_empty(),
        "a read channel isn't marked again"
    );

    app.apply(Event::Timeline {
        room: "discord:10:21".into(),
        messages: timeline(vec![ChatMessage {
            item: "d:1".into(),
            body: "kept".into(),
            ..Default::default()
        }]),
    });
    app.apply(Event::Members {
        room: "discord:10:21".into(),
        members: Ok(vec![]),
    });
    app.dispatch(Action::SelectRoom("discord:10:22".into()));
    assert!(!app.state.room_ready, "a channel not shown before loads");
    app.apply(Event::Timeline {
        room: "discord:10:22".into(),
        messages: timeline(vec![ChatMessage {
            item: "d:2".into(),
            body: "Out now".into(),
            can_react: true,
            ..Default::default()
        }]),
    });
    assert!(!app.state.can_write(), "read-only channel");
    // It takes reactions all the same, as Add Reactions isn't sending.
    assert_eq!(app.state.reacts_in("discord:10:22"), chat::Reacts::Any);
    let effects = app.dispatch(Action::React {
        item: "d:2".into(),
        key: "🎉".into(),
    });
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::React { room, .. })] if room == "discord:10:22"
    ));
    // Back to general: shown as it was left at once, while the backend
    // sends it afresh.
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    assert!(app.state.room_ready);
    assert_eq!(app.state.messages[0].body, "kept");
    assert_eq!(app.state.members, Some(Ok(Arc::new(vec![]))));
    app.apply(Event::Timeline {
        room: "discord:10:21".into(),
        messages: timeline(vec![]),
    });
    app.apply(discord_event(Snapshot {
        status: Status::Offline("Reconnecting".into()),
        ..snapshot(false)
    }));
    assert!(!app.state.can_write(), "sending waits for Discord");
    assert_eq!(app.state.reacts_in("discord:10:21"), chat::Reacts::None);
}

#[test]
#[cfg(feature = "discord")]
fn discord_dms_list_with_their_pictures_and_read_once_read() {
    use yapfast_discord::service::{Conversation, Snapshot, Status};
    let mut app = connected();
    app.state.trusted = false;
    let effects = app.apply(discord_event(Snapshot {
        status: Status::Online,
        conversations: vec![
            Arc::new(Conversation {
                id: "30".into(),
                name: "Ann".into(),
                avatar: Some("ann.png".into()),
                people: 2,
                unread: true,
                mentions: 2,
                ..Default::default()
            }),
            Arc::new(Conversation {
                id: "31".into(),
                name: "Weekend".into(),
                group: true,
                people: 3,
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    assert!(
        !effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls))) if urls.iter().any(|url| url == "ann.png")
        )),
        "off-screen list avatars wait for the UI"
    );
    assert!(matches!(
        app.dispatch(Action::LoadPictures(vec!["ann.png".into()])).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls)))] if urls == &["ann.png"]
    ));
    let listed: Vec<_> = app
        .state
        .scoped_rooms()
        .filter(|room| room.network == Network::Discord)
        .map(|room| (room.id.as_str(), room.members, room.mentions))
        .collect();
    assert_eq!(
        listed,
        [("discord:@me:30", 2, 2), ("discord:@me:31", 3, 0)],
        "Discord's order, after the Matrix DMs"
    );
    let effects = app.dispatch(Action::SelectRoom("discord:@me:30".into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Open(id))] if id == "discord:@me:30"
    ));
    app.dispatch(Action::Reading(true));
    let effects = app.apply(Event::Timeline {
        room: "discord:@me:30".into(),
        messages: timeline(vec![]),
    });
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::Backend(Command::MarkRead { room, .. })] if room == "discord:@me:30"
        ),
        "read as it shows, its newest message in view"
    );
    assert!(app.state.can_write(), "Discord DMs don't need Matrix trust");
    assert!(!app.state.lists_members(), "a DM lists no members");
    app.dispatch(Action::SelectRoom("discord:@me:31".into()));
    assert!(app.state.lists_members(), "a group DM does");
}

#[test]
#[cfg(feature = "discord")]
fn discord_friends_are_listed_and_messaged() {
    use yapfast_discord::{
        member_list::Presence as Discord,
        service::{Conversation, Friend, Snapshot, Status},
    };
    let friend = |id: &str, name: &str, presence| Friend {
        decoration: None,
        user_id: id.into(),
        name: name.into(),
        avatar: format!("{id}.png"),
        nickname: None,
        since: None,
        presence,
        activity: None,
        activities: Default::default(),
    };
    let snapshot = |dms: &[(&str, &str)]| Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        friends: vec!["2".into(), "3".into()].into(),
        friend_list: vec![
            Friend {
                decoration: None,
                activity: Some("making tea".into()),
                activities: Default::default(),
                // Friends since 2026-09-01, in milliseconds.
                since: Some(1_788_253_200_000),
                ..friend("2", "Ann", Some(Discord::Idle))
            },
            friend("3", "Bob", None),
        ]
        .into(),
        conversations: dms
            .iter()
            .map(|(id, person)| Conversation {
                id: (*id).into(),
                name: "DM".into(),
                person: Some((*person).into()),
                people: 2,
                ..Default::default()
            })
            .map(Arc::new)
            .collect(),
        ..Default::default()
    };
    let mut app = Application::default();
    app.apply(discord_event(snapshot(&[("30", "2")])));
    assert_eq!(
        app.state.friends[0],
        chat::Friend {
            decoration: None,
            id: "2".into(),
            name: "Ann".into(),
            nickname: None,
            avatar: Some("2.png".into()),
            presence: Some(chat::Presence::Idle),
            activity: Some("making tea".into()),
            activities: Default::default(),
            since: Some(1_788_253_200_000),
        }
    );
    let effects = app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    assert_eq!(app.state.selected, None);
    assert!(
        !effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls))) if urls.iter().any(|url| url == "3.png")
        )),
        "off-screen list avatars wait for the UI"
    );
    assert!(matches!(
        app.dispatch(Action::LoadPictures(vec!["3.png".into()])).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls)))] if urls == &["3.png"]
    ));

    // With no conversation open, only a friend's DM has their profile.
    assert_eq!(
        app.state.person_choices("2"),
        [
            MenuChoice::Profile,
            MenuChoice::Message,
            MenuChoice::Block,
            MenuChoice::CopyId
        ]
    );
    assert_eq!(
        app.state.person_choices("3"),
        [MenuChoice::Message, MenuChoice::Block, MenuChoice::CopyId]
    );

    // A friend with a DM opens it; one without has it started.
    app.dispatch(Action::MessagePerson("2".into()));
    assert_eq!(app.state.selected.as_deref(), Some("discord:@me:30"));
    assert_eq!(app.state.dm_screen, None);
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    assert!(matches!(
        app.dispatch(Action::MessagePerson("3".into())).as_slice(),
        [Effect::Backend(Command::StartDm { id, discord: true })] if id == "3"
    ));
    app.apply(Event::DmStarted(Ok("discord:@me:31".into())));
    app.apply(discord_event(snapshot(&[("30", "2"), ("31", "3")])));
    assert_eq!(app.state.selected.as_deref(), Some("discord:@me:31"));
    assert!(
        app.dispatch(Action::MessagePerson("9".into())).is_empty(),
        "only friends"
    );

    // A DM that couldn't start says why.
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    app.apply(discord_event(snapshot(&[])));
    app.dispatch(Action::MessagePerson("2".into()));
    app.apply(Event::DmStarted(Err("Could not start the DM".into())));
    assert_eq!(app.state.notice.as_deref(), Some("Could not start the DM"));
}

#[test]
#[cfg(feature = "discord")]
fn discord_message_requests_are_answered_not_written_to() {
    use yapfast_discord::service::{Conversation, Snapshot, Status};
    let snapshot = |request: bool| Snapshot {
        status: Status::Online,
        conversations: vec![Arc::new(Conversation {
            id: "40".into(),
            name: "Erin".into(),
            people: 2,
            request,
            unread: true,
            ..Default::default()
        })],
        ..Default::default()
    };
    let mut app = Application::default();
    app.apply(discord_event(snapshot(true)));
    assert_eq!(app.state.selected, None, "a request opens only when chosen");
    let room = &app.state.rooms[0];
    assert!(room.request && room.read_only && room.category == discord::REQUESTS);
    app.dispatch(Action::SelectRoom("discord:@me:40".into()));
    app.apply(Event::Timeline {
        room: "discord:@me:40".into(),
        messages: timeline(vec![]),
    });
    assert!(!app.state.can_write(), "replying waits for accepting");
    let effects = app.dispatch(Action::AnswerRequest(true));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::AnswerRequest { room, accept: true }))] if room == "discord:@me:40"
    ));
    app.apply(discord_event(snapshot(false)));
    assert!(app.state.can_write(), "an accepted request is a DM");
    assert!(
        app.dispatch(Action::AnswerRequest(false)).is_empty(),
        "only requests are answered"
    );
}

#[test]
#[cfg(feature = "discord")]
fn discord_friend_requests_are_sent_answered_and_cancelled() {
    use yapfast_discord::service::{FriendRequest, Snapshot, Status};
    let request = |id: &str, name: &str, incoming: bool| FriendRequest {
        user_id: id.into(),
        name: name.into(),
        avatar: format!("{id}.png"),
        incoming,
        spam: false,
    };
    let mut app = Application::default();
    let effects = app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        friends: vec!["2".into()].into(),
        friend_requests: vec![request("3", "Erin", true), request("4", "Dave", false)].into(),
        ..Default::default()
    }));
    assert!(
        !effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls))) if urls.iter().any(|url| url == "3.png")
        )),
        "off-screen list avatars wait for the UI"
    );
    assert!(matches!(
        app.dispatch(Action::LoadPictures(vec!["3.png".into()])).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls)))] if urls == &["3.png"]
    ));
    let relations: Vec<_> = ["1", "2", "3", "4", "5"]
        .into_iter()
        .map(|user| app.state.relation(user))
        .collect();
    assert_eq!(
        relations,
        [
            Relation::Me,
            Relation::Friend,
            Relation::Incoming,
            Relation::Outgoing,
            Relation::Stranger
        ]
    );
    // Someone's menu offers what fits how they stand with this account.
    use MenuChoice::*;
    let choices: Vec<_> = ["1", "2", "3", "4", "5"]
        .into_iter()
        .map(|user| app.state.person_choices(user))
        .collect();
    assert_eq!(
        choices,
        [
            vec![CopyId],
            vec![Message, Block, CopyId],
            vec![AcceptFriend, Block, CopyId],
            vec![CancelFriendRequest, Block, CopyId],
            vec![AddFriend, Block, CopyId],
        ]
    );
    let sent = |effects: Vec<Effect>| -> Option<(String, bool)> {
        match effects.as_slice() {
            [Effect::Backend(Command::Discord(DiscordCommand::Friend { user, add }))] => {
                Some((user.clone(), *add))
            }
            _ => None,
        }
    };
    assert_eq!(
        sent(app.dispatch(Action::Befriend("5".into()))),
        Some(("5".into(), true))
    );
    assert_eq!(
        sent(app.dispatch(Action::Befriend("3".into()))),
        Some(("3".into(), true))
    );
    assert_eq!(
        sent(app.dispatch(Action::Befriend("4".into()))),
        None,
        "already sent"
    );
    assert_eq!(
        sent(app.dispatch(Action::Befriend("2".into()))),
        None,
        "already friends"
    );
    assert_eq!(
        sent(app.dispatch(Action::RemoveFriendRequest("4".into()))),
        Some(("4".into(), false))
    );
    assert_eq!(
        sent(app.dispatch(Action::RemoveFriendRequest("2".into()))),
        None,
        "not unfriending"
    );
}

/// Alice's Discord DMs: one with Annie, and a group with Annie and Ben. Her
/// friends are Annie, Ben and Cara.
#[cfg(feature = "discord")]
fn discord_dms(extra: Vec<yapfast_discord::service::Conversation>) -> Application {
    use yapfast_discord::service::{Conversation, Friend, Snapshot, Status};
    let friend = |id: &str, name: &str| Friend {
        decoration: None,
        user_id: id.into(),
        name: name.into(),
        avatar: String::new(),
        nickname: None,
        since: None,
        presence: None,
        activity: None,
        activities: Default::default(),
    };
    let mut conversations = vec![
        Conversation {
            id: "30".into(),
            name: "Annie".into(),
            person: Some("2".into()),
            people: 2,
            ..Default::default()
        },
        Conversation {
            id: "31".into(),
            name: "Hikers".into(),
            group: true,
            people: 3,
            ..Default::default()
        },
    ];
    conversations.extend(extra);
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        friends: vec!["2".into(), "3".into(), "4".into()].into(),
        friend_list: vec![
            friend("2", "Annie"),
            friend("3", "Ben"),
            friend("4", "Cara"),
        ]
        .into(),
        conversations: conversations.into_iter().map(Arc::new).collect(),
        ..Default::default()
    }));
    app
}

#[test]
#[cfg(feature = "discord")]
fn leaving_a_group_dm_asks_first_and_may_tell_no_one() {
    use MenuChoice::*;
    let mut app = discord_dms(vec![]);
    let group = "discord:@me:31".to_owned();
    // After muting, as in Discord's menu; a one-to-one DM closes instead.
    assert_eq!(
        app.state.menu_choices(&group),
        [Pin, Call, Mute, LeaveGroup, CopyLink, CopyId]
    );
    assert!(
        !app.state
            .menu_choices("discord:@me:30")
            .contains(&LeaveGroup)
    );
    assert!(
        app.dispatch(Action::AskLeaveGroup("discord:@me:30".into()))
            .is_empty()
            && app.state.leave.is_none()
    );

    app.dispatch(Action::SelectRoom(group.clone()));
    assert!(
        app.dispatch(Action::AskLeaveGroup(group.clone()))
            .is_empty()
    );
    let dialog = app.state.leave.clone().unwrap();
    assert_eq!(
        (dialog.place.as_str(), dialog.name.as_str()),
        (group.as_str(), "Hikers")
    );
    assert!(
        dialog.group && !dialog.silent,
        "telling the others, unless ticked"
    );
    app.dispatch(Action::ToggleSilentLeave);
    assert!(app.state.leave.as_ref().unwrap().silent);
    assert!(matches!(
        app.dispatch(Action::Leave).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::LeaveGroup { room, silent: true }))]
            if *room == group
    ));
    app.dispatch(Action::ToggleSilentLeave);
    assert!(
        app.state.leave.as_ref().unwrap().silent,
        "not while it's leaving"
    );
    app.apply(Event::Left {
        place: group.clone(),
        result: Err("Couldn't leave the group: no".into()),
    });
    let dialog = app.state.leave.clone().unwrap();
    assert!(!dialog.leaving && dialog.silent);
    assert_eq!(
        dialog.error.as_deref(),
        Some("Couldn't leave the group: no")
    );
    app.dispatch(Action::Leave);
    app.apply(Event::Left {
        place: group.clone(),
        result: Ok(()),
    });
    assert!(app.state.leave.is_none());
    // Its CHANNEL_DELETE takes it out of the DM list, and off the screen.
    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    snapshot.conversations.retain(|known| known.id != "31");
    app.apply(discord_event(snapshot));
    assert!(app.state.rooms.iter().all(|room| room.id != group));
    assert_ne!(app.state.selected.as_deref(), Some(group.as_str()));

    // Discord takes it only while connected.
    let mut app = discord_dms(vec![]);
    app.state.discord_online = false;
    assert!(!app.state.menu_choices(&group).contains(&LeaveGroup));
}

#[test]
#[cfg(feature = "discord")]
fn a_dms_pins_load_when_shown_and_close_with_the_conversation() {
    let mut app = discord_dms(vec![]);
    let dm = "discord:@me:30".to_owned();
    app.dispatch(Action::SelectRoom(dm.clone()));
    assert!(app.state.conversation_that(|can| can.pins).is_some());
    assert!(matches!(
        app.dispatch(Action::ShowPins(true)).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Pins { room }))] if *room == dm
    ));
    assert_eq!(
        app.state.pins.as_ref().unwrap().messages,
        None,
        "on their way"
    );
    app.apply(Event::Pins {
        room: "discord:@me:31".into(),
        result: Ok(vec![]),
    });
    assert_eq!(
        app.state.pins.as_ref().unwrap().messages,
        None,
        "another DM's pins don't show"
    );
    app.apply(Event::Pins {
        room: dm,
        result: Ok(vec![message("d:9", false)]),
    });
    let pinned = app.state.pins.as_ref().unwrap().messages.clone();
    assert_eq!(pinned.unwrap().unwrap()[0].item, "d:9");

    app.dispatch(Action::ShowPins(false));
    assert!(app.state.pins.is_none());
    app.dispatch(Action::ShowPins(true));
    app.dispatch(Action::SelectRoom("discord:@me:31".into()));
    assert!(app.state.pins.is_none(), "another conversation closes them");
}

#[test]
#[cfg(feature = "discord")]
fn a_dm_search_loads_its_results_a_page_at_a_time() {
    let mut app = discord_dms(vec![]);
    let dm = "discord:@me:30".to_owned();
    app.dispatch(Action::SelectRoom(dm.clone()));
    assert!(matches!(
        app.dispatch(Action::Search("  trip ".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Search {
            place,
            query,
            sort: chat::SearchSort::Newest,
            offset: 0,
        }))] if *place == dm && query == "trip"
    ));
    let page = |from: u32, count: u32| -> Vec<chat::SearchHit> {
        (from..from + count)
            .map(|n| chat::SearchHit {
                room: dm.clone(),
                message: message(&format!("d:{n}"), false),
                thread: None,
            })
            .collect()
    };
    let found_in =
        |sort, query: &str, offset: u32, result: Result<(u32, Vec<chat::SearchHit>), String>| {
            Event::Found {
                place: dm.clone(),
                query: query.into(),
                sort,
                offset,
                result: result.map(|(total, hits)| chat::Searched::Page {
                    total,
                    hits,
                    words: "trip".into(),
                    indexing_older: false,
                }),
            }
        };
    let found = |query: &str, offset: u32, result| {
        found_in(chat::SearchSort::Newest, query, offset, result)
    };
    // While Discord indexes the DM, the search goes on, then shows that.
    let indexing = |again| Event::Found {
        place: dm.clone(),
        query: "trip".into(),
        sort: chat::SearchSort::Newest,
        offset: 0,
        result: Ok(chat::Searched::Indexing { again }),
    };
    app.apply(indexing(true));
    let search = app.state.search.as_ref().unwrap();
    assert!(search.indexing && search.loading);
    app.apply(found("old", 0, Ok((1, page(0, 1)))));
    assert!(
        app.state.search.as_ref().unwrap().results.is_empty(),
        "an earlier search's results don't show"
    );
    app.apply(found("trip", 0, Ok((30, page(0, 25)))));
    let search = app.state.search.as_ref().unwrap();
    assert!(!search.loading && !search.indexing);
    assert_eq!(
        (search.total, search.results.len(), search.pages()),
        (30, 25, 2)
    );
    assert_eq!(search.words, "trip");

    // Pages replace each other, one asked for at a time.
    assert!(matches!(
        app.dispatch(Action::SearchPage(1)).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Search {
            offset: 25,
            ..
        }))]
    ));
    assert!(
        app.dispatch(Action::SearchPage(0)).is_empty(),
        "one page at a time"
    );
    app.apply(found("trip", 25, Err("Discord is busy.".into())));
    let search = app.state.search.as_ref().unwrap();
    assert_eq!(search.error.as_deref(), Some("Discord is busy."));
    app.dispatch(Action::SearchPage(1));
    app.apply(found("trip", 25, Ok((30, page(25, 5)))));
    let search = app.state.search.as_ref().unwrap();
    assert_eq!((search.page, search.results.len()), (1, 5));
    assert!(search.error.is_none());
    assert!(
        app.dispatch(Action::SearchPage(2)).is_empty(),
        "no third page"
    );

    // Another order starts again from the first page; the same order does
    // nothing.
    assert!(
        app.dispatch(Action::SearchSort(chat::SearchSort::Newest))
            .is_empty()
    );
    assert!(matches!(
        app.dispatch(Action::SearchSort(chat::SearchSort::Oldest))
            .as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Search {
            sort: chat::SearchSort::Oldest,
            offset: 0,
            ..
        }))]
    ));
    app.apply(found("trip", 0, Ok((30, page(0, 25)))));
    assert!(
        app.state.search.as_ref().unwrap().loading,
        "the newest page came late"
    );
    app.apply(found_in(
        chat::SearchSort::Oldest,
        "trip",
        0,
        Ok((30, page(5, 25))),
    ));
    let search = app.state.search.as_ref().unwrap();
    assert_eq!(
        (search.page, search.results[0].message.item.as_str()),
        (0, "d:5")
    );
    // A search from the box is the newest first again, and keeps its
    // count meanwhile only in the same order.
    app.dispatch(Action::Search("trip".into()));
    let search = app.state.search.as_ref().unwrap();
    assert_eq!((search.sort, search.total), (chat::SearchSort::Newest, 0));
    app.apply(found("trip", 0, Ok((30, page(0, 25)))));
    app.dispatch(Action::Search("trip".into()));
    assert_eq!(app.state.search.as_ref().unwrap().total, 30);
    app.dispatch(Action::ShowHiddenResults(true));
    assert!(app.state.search.as_ref().unwrap().show_hidden);

    // A box holding only keys asks nothing, so nothing changes, as in
    // Discord's.
    assert!(app.dispatch(Action::Search("from: has:".into())).is_empty());
    assert_eq!(app.state.search.as_ref().unwrap().query, "trip");
    app.dispatch(Action::Search(" ".into()));
    assert!(app.state.search.is_none(), "an empty search ends it");

    // Discord stopped being asked while it indexes: the search shows that.
    app.dispatch(Action::Search("trip".into()));
    app.apply(indexing(false));
    let search = app.state.search.as_ref().unwrap();
    assert!(search.indexing && !search.loading && search.results.is_empty());
}

#[test]
#[cfg(feature = "discord")]
fn friends_added_to_a_dm_make_a_group_that_opens() {
    use yapfast_discord::service::Conversation;
    let mut app = discord_dms(vec![]);
    let dm = "discord:@me:30".to_owned();
    app.dispatch(Action::SelectRoom(dm));
    app.dispatch(Action::OpenAddPeople);
    let names = |app: &Application| -> Vec<String> {
        app.state
            .addable_friends()
            .iter()
            .map(|friend| friend.name.clone())
            .collect()
    };
    assert_eq!(names(&app), ["Ben", "Cara"], "Annie is here already");
    app.dispatch(Action::AddPeopleQuery("c".into()));
    assert_eq!(names(&app), ["Cara"]);
    assert!(
        app.dispatch(Action::AddPeople).is_empty(),
        "nobody is chosen"
    );
    app.dispatch(Action::ChoosePerson("4".into()));
    app.dispatch(Action::ChoosePerson("3".into()));
    app.dispatch(Action::ChoosePerson("3".into()));
    assert_eq!(app.state.add_people.as_ref().unwrap().chosen, ["4"]);
    assert!(matches!(
        app.dispatch(Action::AddPeople).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::AddToGroup { room: None, users }))]
            if users == &["2", "4"]
    ));
    app.dispatch(Action::ChoosePerson("3".into()));
    assert_eq!(
        app.state.add_people.as_ref().unwrap().chosen,
        ["4"],
        "the choice holds while adding"
    );

    // The group opens once Discord lists it.
    app.apply(Event::GroupChanged(Ok("discord:@me:50".into())));
    assert!(app.state.add_people.is_none());
    let group = Conversation {
        id: "50".into(),
        name: "Annie, Cara".into(),
        group: true,
        people: 3,
        ..Default::default()
    };
    let snapshot = discord_dms(vec![group]).discord.unwrap();
    app.apply(Event::Discord(snapshot));
    assert_eq!(app.state.selected.as_deref(), Some("discord:@me:50"));
}

#[test]
#[cfg(feature = "discord")]
fn friends_added_to_a_group_fill_it_to_ten() {
    let mut app = discord_dms(vec![]);
    let group = "discord:@me:31".to_owned();
    app.dispatch(Action::SelectRoom(group.clone()));
    assert_eq!(app.state.group_room(), 7, "three of ten are there");
    app.dispatch(Action::OpenAddPeople);
    app.dispatch(Action::ChoosePerson("4".into()));
    assert!(matches!(
        app.dispatch(Action::AddPeople).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::AddToGroup { room: Some(room), users }))]
            if *room == group && users == &["4"]
    ));
    app.apply(Event::GroupChanged(Err("Could not add them: no.".into())));
    let picker = app.state.add_people.as_ref().unwrap();
    assert!(!picker.adding);
    assert_eq!(picker.error.as_deref(), Some("Could not add them: no."));
    app.dispatch(Action::AddPeople);
    app.apply(Event::GroupChanged(Ok(group.clone())));
    assert!(app.state.add_people.is_none());
    assert_eq!(
        app.state.selected.as_ref(),
        Some(&group),
        "the group stays open"
    );
}

#[test]
#[cfg(feature = "discord")]
fn a_dm_search_closes_in_another_dm() {
    let mut app = discord_dms(vec![]);
    app.dispatch(Action::SelectRoom("discord:@me:30".into()));
    app.dispatch(Action::Search("trip".into()));
    assert_eq!(app.state.search_place().as_deref(), Some("discord:@me:30"));
    app.dispatch(Action::SelectRoom("discord:@me:31".into()));
    assert!(app.state.search.is_none());
}
