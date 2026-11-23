//! Threads and forums.

use super::*;

#[test]
#[cfg(feature = "discord")]
fn discord_threads_open_once_listed() {
    use yapfast_discord::{
        ready::{Channel, Server, Thread},
        service::{Snapshot, Status},
    };
    let snapshot = |threads: &[&str], joined: &[&str], viewing: Option<&str>| Snapshot {
        status: Status::Online,
        servers: vec![MarkedServer::from(Server {
            id: "10".into(),
            name: "Test".into(),
            text_channels: vec![Channel {
                id: "21".into(),
                name: "general".into(),
                ..Default::default()
            }],
            threads: threads
                .iter()
                .map(|id| Channel {
                    id: (*id).into(),
                    name: "Plans".into(),
                    parent: Some("21".into()),
                    thread: Some(Box::new(Thread {
                        joined: joined.contains(id),
                        ..Default::default()
                    })),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })],
        viewing: viewing.into_iter().map(Into::into).collect(),
        ..Default::default()
    };
    let mut app = connected();
    app.apply(discord_event(snapshot(&[], &[], None)));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    // A thread this account isn't in isn't listed: Yapfast asks for it.
    let effects = app.dispatch(Action::Open("discord:10:30".into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::OpenThread { room, beside: false }))]
            if room == "discord:10:30"
    ));
    app.apply(Event::ThreadOpened {
        room: "discord:10:30".into(),
        result: Ok(()),
    });
    let effects = app.apply(discord_event(snapshot(&["30"], &[], Some("30"))));
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:30"));
    assert!(effects.iter().any(
        |effect| matches!(effect, Effect::Backend(Command::Open(id)) if id == "discord:10:30")
    ));
    let thread = app.state.selected_room().unwrap();
    assert_eq!(thread.parent.as_deref(), Some("discord:10:21"));
    // Its menu has no channel's notifications, invites or managing, and
    // joins it.
    let choices = app.menu_choices("discord:10:30");
    assert_eq!(
        choices,
        [
            MenuChoice::JoinThread,
            MenuChoice::CopyLink,
            MenuChoice::CopyId
        ]
    );
    let join = |join| Action::JoinThread {
        room: "discord:10:30".into(),
        join,
        location: "Context Menu",
    };
    assert!(matches!(
        app.dispatch(join(true)).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::JoinThread {
            room,
            join: true,
            location: "Context Menu",
        }))] if room == "discord:10:30"
    ));
    // Its header's More menu offers the same, less marking it read.
    assert_eq!(
        app.thread_more_choices("discord:10:30"),
        [
            MenuChoice::JoinThread,
            MenuChoice::CopyLink,
            MenuChoice::CopyId
        ]
    );
    assert!(app.dispatch(join(false)).is_empty(), "it isn't in it");

    // One Discord won't give says why, and doesn't open later.
    app.dispatch(Action::Open("discord:10:31".into()));
    app.apply(Event::ThreadOpened {
        room: "discord:10:31".into(),
        result: Err("Couldn't open the thread: Missing Access".into()),
    });
    assert_eq!(
        app.state.notice.as_deref(),
        Some("Couldn't open the thread: Missing Access")
    );
    app.apply(discord_event(snapshot(&["30", "31"], &["31"], Some("30"))));
    assert!(
        app.state
            .rooms
            .iter()
            .any(|room| room.id == "discord:10:31")
    );
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:30"));
    // One it's in, it leaves.
    let choices = app.menu_choices("discord:10:31");
    assert!(choices.contains(&MenuChoice::LeaveThread));
    assert!(!choices.contains(&MenuChoice::JoinThread));
}

#[test]
#[cfg(feature = "discord")]
fn discord_threads_show_beside_their_channel() {
    use yapfast_discord::{
        ready::{Channel, Server, Thread},
        service::{Snapshot, Status},
    };
    let snapshot = |threads: &[&str], viewing: &[&str]| Snapshot {
        status: Status::Online,
        servers: vec![MarkedServer::from(Server {
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
            threads: threads
                .iter()
                .map(|id| Channel {
                    id: (*id).into(),
                    name: "Plans".into(),
                    parent: Some("21".into()),
                    thread: Some(Box::new(Thread {
                        joined: *id == "30",
                        ..Default::default()
                    })),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })],
        viewing: viewing.iter().map(|id| (*id).into()).collect(),
        ..Default::default()
    };
    let mut app = connected();
    app.apply(discord_event(snapshot(&["30"], &[])));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    let effects = app.dispatch(Action::OpenAside("discord:10:30".into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Aside(Some(room)))] if room == "discord:10:30"
    ));
    app.apply(Event::Timeline {
        room: "discord:10:30".into(),
        messages: timeline(vec![ChatMessage {
            item: "d:5".into(),
            body: "Saturday?".into(),
            attachments: vec![chat::Attachment {
                name: "map.png".into(),
                size: None,
                image: true,
                dimensions: None,
                preview: None,
                preview_failed: false,
                placement: chat::Placement::Message,
                gif: None,
                spoiler: false,
                video: false,
                url: None,
                description: None,
            }],
            can_react: true,
            can_reply: true,
            ..Default::default()
        }]),
    });
    let aside = app.state.aside.as_ref().unwrap();
    assert!(aside.ready && aside.messages.len() == 1);
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:21"));

    // Its images open from the thread, and show once they come from there.
    let effects = app.dispatch(Action::ViewImage {
        item: "d:5".into(),
        index: 0,
    });
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::ViewImage { room, .. })] if room == "discord:10:30"
    ));
    app.apply(Event::Image {
        room: "discord:10:30".into(),
        item: "d:5".into(),
        index: 0,
        picture: Err("gone"),
    });
    assert_eq!(
        app.state.viewer.as_ref().unwrap().pictures[&0],
        Some(Err("gone"))
    );
    app.dispatch(Action::CloseImage);

    // Its messages act in the thread, and its composer writes there.
    let effects = app.dispatch(Action::React {
        item: "d:5".into(),
        key: "👍".into(),
    });
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::React { room, .. })] if room == "discord:10:30"
    ));
    app.dispatch(Action::Reply("d:5".into()));
    assert!(matches!(
        app.state.compose.get("discord:10:30"),
        Some(Compose::Reply { item, .. }) if item == "d:5"
    ));
    assert!(
        app.state.compose().is_none(),
        "the channel's composer stays"
    );
    app.dispatch(Action::SetAsideDraft("Yes".into()));
    let effects = app.dispatch(Action::SendAside);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Send { room, body, reply: Some(reply), .. })]
            if room == "discord:10:30" && body == "Yes" && reply == "d:5"
    ));
    app.apply(Event::SendResult {
        room: "discord:10:30".into(),
        body: "Yes".into(),
        result: Ok(()),
    });

    // Files go with its next message, not the channel's.
    let folder = tempfile::tempdir().unwrap();
    let file = |name: &str| {
        let path = folder.path().join(name);
        std::fs::write(&path, b"png").unwrap();
        path
    };
    let (picture, notes) = (file("map.png"), file("notes.txt"));
    assert!(app.state.aside_can_attach());
    app.dispatch(Action::StageAside(vec![picture.clone(), notes]));
    assert!(
        app.state.staged().is_empty(),
        "the channel's composer stays"
    );
    assert_eq!(app.state.aside_staged().len(), 2);
    app.dispatch(Action::ToggleAsideSpoiler(0));
    app.dispatch(Action::UnstageAside(1));
    assert!(app.state.can_send_in("discord:10:30"), "files alone send");
    let effects = app.dispatch(Action::SendAside);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Send { room, files, .. })]
            if room == "discord:10:30"
                && matches!(files.as_slice(), [chat::OutgoingFile { path, spoiler: true }] if *path == picture)
    ));
    app.apply(Event::SendResult {
        room: "discord:10:30".into(),
        body: String::new(),
        result: Ok(()),
    });
    assert!(app.state.aside_staged().is_empty(), "sent with the message");

    // Expanding it shows it in place of its channel.
    let effects = app.dispatch(Action::ExpandAside);
    assert!(app.state.aside.is_none());
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:30"));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Aside(None)), .., Effect::Backend(Command::Open(room))]
            if room == "discord:10:30"
    ));

    // One that isn't listed shows once Discord gives it, and goes with
    // its channel.
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    let effects = app.dispatch(Action::OpenAside("discord:10:31".into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::OpenThread { room, beside: true }))]
            if room == "discord:10:31"
    ));
    let effects = app.apply(discord_event(snapshot(&["30", "31"], &["21", "31"])));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::Aside(Some(room))) if room == "discord:10:31"
    )));
    let effects = app.dispatch(Action::SelectRoom("discord:10:22".into()));
    assert!(matches!(
        effects.first(),
        Some(Effect::Backend(Command::Aside(None)))
    ));
    assert!(app.state.aside.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn a_channels_threads_list_as_discords_client_lists_them() {
    use chat::ListedThread;
    use yapfast_discord::{
        ready::{self, Access, permission},
        service::{Snapshot, Status},
    };
    let bits = |bits: u64| bits.to_string();
    let thread = |id: &str, name: &str, parent: &str, last: &str, archived: bool, joined: bool| {
        let mut thread = serde_json::json!({
            "id": id, "type": 11, "name": name, "parent_id": parent, "guild_id": "10",
            "last_message_id": last, "member_count": 3, "member_ids_preview": ["2", "3"],
            "thread_metadata": {"archived": archived, "locked": false},
        });
        if joined {
            thread["member"] = serde_json::json!({"id": id, "user_id": "1", "flags": 1});
        }
        thread
    };
    let mut server = ready::server(&serde_json::json!({
        "id": "10", "owner_id": "99", "properties": {"name": "Test"},
        "roles": [{"id": "10", "name": "@everyone", "position": 0, "permissions": bits(
            permission::VIEW_CHANNEL | permission::SEND_MESSAGES
                | permission::READ_MESSAGE_HISTORY | permission::SEND_MESSAGES_IN_THREADS)}],
        "channels": [
            {"id": "11", "type": 0, "name": "general", "position": 0},
            {"id": "12", "type": 5, "name": "news", "position": 1,
             "permission_overwrites": [{"id": "10", "type": 0, "allow": "0",
                                        "deny": bits(permission::READ_MESSAGE_HISTORY)}]},
            {"id": "13", "type": 2, "name": "Lounge", "position": 2},
        ],
        "threads": [
            thread("30", "Plans", "11", "300", false, true),
            thread("31", "Book club", "11", "310", false, false),
            thread("32", "Old", "11", "200", true, false),
            thread("33", "Secret", "11", "320", false, false),
            thread("34", "Elsewhere", "12", "330", false, false),
        ],
    }))
    .unwrap();
    // READY lists only those she's in; the rest came with THREAD_LIST_SYNC.
    // As the service publishes them, a private one she isn't in is hidden.
    for channel in &mut server.threads {
        channel.thread.as_mut().unwrap().joined = channel.id == "30";
    }
    let server = marked(
        server,
        &[(
            "33",
            ChannelMarks {
                access: Access::Hidden,
                ..Default::default()
            },
        )],
    );
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        servers: vec![server],
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    let general = "discord:10:11".to_owned();
    app.dispatch(Action::SelectRoom(general.clone()));
    assert_eq!(app.state.threads_channel(), Some(general.as_str()));

    // Opening it asks for the first page of archived threads, and only that.
    assert!(matches!(
        app.dispatch(Action::ShowThreads(true)).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Threads { room, query: None, offset: 0 }))]
            if *room == general
    ));
    let rooms = |threads: &[ListedThread]| {
        threads
            .iter()
            .map(|thread| {
                thread
                    .room
                    .as_str()
                    .trim_start_matches("discord:10:")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };
    let listed = |id: &str, last: u64| ListedThread {
        room: format!("discord:10:{id}"),
        name: format!("Thread {id}"),
        archived: true,
        last,
        ..Default::default()
    };
    // Until the first page answers, the active ones show, and none of the
    // archived ones the server knows.
    let sections = app.thread_sections().unwrap();
    assert_eq!(rooms(&sections.joined), ["30"]);
    assert!(sections.older.is_empty());
    app.apply(Event::Threads {
        room: general.clone(),
        query: None,
        offset: 0,
        result: Ok((vec![listed("40", 250), listed("41", 150)], true)),
        starters: vec![],
    });
    let sections = app.thread_sections().unwrap();
    assert_eq!(rooms(&sections.joined), ["30"]);
    assert_eq!(rooms(&sections.other), ["31"], "the hidden one stays out");
    // Those the server knows were archived join the pages they fall within.
    assert_eq!(rooms(&sections.older), ["40", "32", "41"]);
    let plans = &sections.joined[0];
    assert_eq!((plans.last, plans.members), (300, 3));
    assert_eq!(plans.member_avatars.len(), 2);

    // The next page loads once, from where the last ended.
    assert!(matches!(
        app.dispatch(Action::MoreThreads).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Threads {
            query: None,
            offset: 25,
            ..
        }))]
    ));
    assert!(app.dispatch(Action::MoreThreads).is_empty());
    app.apply(Event::Threads {
        room: general.clone(),
        query: None,
        offset: 25,
        result: Ok((vec![listed("42", 100)], false)),
        starters: vec![],
    });
    assert_eq!(
        rooms(&app.thread_sections().unwrap().older),
        ["40", "32", "41", "42"]
    );
    assert!(app.dispatch(Action::MoreThreads).is_empty(), "none left");

    // A search stands in for the list until it's cleared.
    assert!(matches!(
        app.dispatch(Action::SearchThreads(" book ".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Threads { query: Some(query), offset: 0, .. }))]
            if query == "book"
    ));
    app.apply(Event::Threads {
        room: general.clone(),
        query: Some("book".into()),
        offset: 0,
        result: Ok((vec![listed("31", 310)], false)),
        starters: vec![],
    });
    let search = app.state.thread_list().unwrap().search.as_ref().unwrap();
    assert!(matches!(&search.found, Some(Ok(found)) if found.len() == 1));
    assert!(
        app.dispatch(Action::SearchThreads(String::new()))
            .is_empty()
    );
    assert!(app.state.thread_list().unwrap().search.is_none());

    // Closing it, or opening another room, forgets what it found.
    assert!(matches!(
        app.dispatch(Action::ShowThreads(false)).as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::CloseThreads
        ))]
    ));
    assert!(app.state.threads.is_none());
    app.dispatch(Action::ShowThreads(true));
    let effects = app.dispatch(Action::SelectRoom("discord:10:30".into()));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::Discord(DiscordCommand::CloseThreads))
    )));
    assert!(app.state.threads.is_none());
    // A thread lists its channel's threads.
    assert_eq!(app.state.threads_channel(), Some(general.as_str()));

    // Without its history, a channel lists only its active threads, as
    // Discord's client does.
    app.dispatch(Action::SelectRoom("discord:10:12".into()));
    assert!(app.dispatch(Action::ShowThreads(true)).is_empty());
    let sections = app.thread_sections().unwrap();
    assert_eq!(rooms(&sections.other), ["34"]);
    // A voice channel has none.
    app.dispatch(Action::Open("discord:10:13".into()));
    assert_eq!(app.state.threads_channel(), None);
}

#[test]
#[cfg(feature = "discord")]
fn a_thread_starts_from_discords_new_thread_form() {
    use yapfast_discord::{
        ready::{self, permission},
        service::{Snapshot, Status},
    };
    let bits = |bits: u64| bits.to_string();
    let server = |threads: Vec<serde_json::Value>| {
        let mut server = ready::server(&serde_json::json!({
            "id": "10", "owner_id": "99", "properties": {"name": "Test"},
            "roles": [{"id": "10", "name": "@everyone", "position": 0, "permissions": bits(
                permission::VIEW_CHANNEL | permission::SEND_MESSAGES
                    | permission::READ_MESSAGE_HISTORY | permission::SEND_MESSAGES_IN_THREADS
                    | permission::CREATE_PUBLIC_THREADS | permission::CREATE_PRIVATE_THREADS)}],
            "channels": [
                {"id": "11", "type": 0, "name": "general", "position": 0},
                {"id": "12", "type": 5, "name": "news", "position": 1},
                {"id": "14", "type": 0, "name": "quiet", "position": 2,
                 "permission_overwrites": [{"id": "10", "type": 0, "allow": "0",
                                            "deny": bits(permission::CREATE_PUBLIC_THREADS)}]},
            ],
            "threads": threads,
        }))
        .unwrap();
        for channel in &mut server.threads {
            channel.thread.as_mut().unwrap().joined = true;
        }
        Event::Discord(Arc::new(Snapshot {
            status: Status::Online,
            user_id: Some("1".into()),
            servers: vec![MarkedServer::from(server)],
            ..Default::default()
        }))
    };
    let mut app = Application::default();
    app.apply(server(vec![]));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    let general = "discord:10:11".to_owned();
    app.dispatch(Action::SelectRoom(general.clone()));
    app.apply(Event::Timeline {
        room: general.clone(),
        messages: timeline(vec![ChatMessage {
            body: "## Plan for the weekend trip to the mountains with everyone\nDetails".into(),
            ..message("d:500", false)
        }]),
    });
    assert!(app.can_start_threads());
    assert_eq!(app.thread_starts(&general), (true, true));

    // From a message: public, named after it unless named, and the
    // channel's draft becomes its first message.
    app.dispatch(Action::SetDraft("Who's driving?".into()));
    let effects = app.dispatch(Action::NewThread {
        message: Some("d:500".into()),
        from: ThreadFrom::Message,
    });
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::SaveDraft { room, body }) if *room == general && body.is_empty()
    )));
    let form = app.state.new_thread().unwrap();
    assert_eq!(form.default_name, "Plan for the weekend trip to the...");
    assert_eq!(form.body, "Who's driving?");
    assert!(!form.can_private);
    assert_eq!(app.state.draft(), "");
    assert!(matches!(
        app.dispatch(Action::CreateThread).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::StartThread {
            room, message: Some(message), name, private: false, location: "Message",
        }))] if *room == general && message == "d:500" && name == "Plan for the weekend trip to the..."
    ));
    // Discord tells of the thread before it answers; it opens beside the
    // channel, and its first message goes once its timeline comes.
    app.apply(server(vec![serde_json::json!({
        "id": "500", "type": 11, "name": "Plan for the weekend trip to the...",
        "parent_id": "11", "guild_id": "10", "member_count": 1,
        "thread_metadata": {"archived": false, "locked": false},
    })]));
    let thread = "discord:10:500".to_owned();
    assert!(
        app.apply(Event::ThreadStarted {
            room: general.clone(),
            result: Ok(thread.clone()),
        })
        .iter()
        .any(|effect| matches!(effect, Effect::Backend(Command::Aside(Some(room))) if *room == thread))
    );
    assert!(app.state.new_thread.is_none());
    let effects = app.apply(Event::Timeline {
        room: thread.clone(),
        messages: timeline(vec![]),
    });
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::Send { room, body, .. }) if *room == thread && body == "Who's driving?"
    )));

    // From the composer's menu: it needs a name and a first message, and
    // may be private.
    app.dispatch(Action::CloseAside);
    app.dispatch(Action::NewThread {
        message: None,
        from: ThreadFrom::PlusButton,
    });
    let form = app.state.new_thread().unwrap();
    assert!(form.can_private && !form.only_private && !form.private);
    app.dispatch(Action::CreateThread);
    assert_eq!(
        app.state.new_thread().unwrap().error.as_deref(),
        Some("Thread Name is required")
    );
    app.dispatch(Action::WriteNewThread {
        name: Some(format!("Snacks\n{}", "a".repeat(150))),
        body: None,
    });
    let form = app.state.new_thread().unwrap();
    assert_eq!(form.error, None);
    assert_eq!(form.name.chars().count(), THREAD_NAME_LIMIT);
    assert!(form.name.starts_with("Snacks a"));
    app.dispatch(Action::CreateThread);
    assert_eq!(
        app.state.new_thread().unwrap().error.as_deref(),
        Some("Starter Message is required")
    );
    app.dispatch(Action::SetThreadPrivate(true));
    app.dispatch(Action::WriteNewThread {
        name: Some("Snacks".into()),
        body: Some("Chips?".into()),
    });
    assert!(matches!(
        app.dispatch(Action::CreateThread).as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::StartThread {
                message: None,
                private: true,
                location: "Plus Button",
                ..
            }
        ))]
    ));
    // A refusal leaves the form as it was, saying why.
    app.apply(Event::ThreadStarted {
        room: general,
        result: Err("Couldn't start the thread: no".into()),
    });
    let form = app.state.new_thread().unwrap();
    assert!(!form.starting);
    assert_eq!(form.error.as_deref(), Some("Couldn't start the thread: no"));
    // Closing what was written asks first.
    app.dispatch(Action::CloseNewThread);
    assert!(app.state.new_thread().unwrap().leaving);
    app.dispatch(Action::StayInNewThread);
    assert!(!app.state.new_thread().unwrap().leaving);
    app.dispatch(Action::CloseNewThread);
    app.dispatch(Action::LeaveNewThread);
    assert!(app.state.new_thread.is_none());

    // Files wait in the form and go with its first message once the
    // thread has started, as in Discord's form; they alone make one, and
    // count as written. The first thread's message went meanwhile.
    app.apply(Event::SendResult {
        room: thread,
        body: "Who's driving?".into(),
        result: Ok(()),
    });
    let folder = tempfile::tempdir().unwrap();
    let picture = folder.path().join("map.png");
    std::fs::write(&picture, b"png").unwrap();
    app.dispatch(Action::NewThread {
        message: None,
        from: ThreadFrom::PlusButton,
    });
    assert!(app.state.can_attach_first());
    app.dispatch(Action::StageFirst(vec![picture.clone()]));
    app.dispatch(Action::ToggleFirstSpoiler(0));
    assert!(
        app.state.staged().is_empty(),
        "the channel's composer stays"
    );
    app.dispatch(Action::CloseNewThread);
    assert!(app.state.new_thread().unwrap().leaving);
    app.dispatch(Action::StayInNewThread);
    app.dispatch(Action::WriteNewThread {
        name: Some("Maps".into()),
        body: None,
    });
    assert!(matches!(
        app.dispatch(Action::CreateThread).as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::StartThread { .. }
        ))]
    ));
    app.apply(server(vec![serde_json::json!({
        "id": "501", "type": 11, "name": "Maps",
        "parent_id": "11", "guild_id": "10", "member_count": 1,
        "thread_metadata": {"archived": false, "locked": false},
    })]));
    let maps = "discord:10:501".to_owned();
    app.apply(Event::ThreadStarted {
        room: "discord:10:11".into(),
        result: Ok(maps.clone()),
    });
    let effects = app.apply(Event::Timeline {
        room: maps.clone(),
        messages: timeline(vec![]),
    });
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::Send { room, body, files, .. })
            if *room == maps
                && body.is_empty()
                && matches!(files.as_slice(), [chat::OutgoingFile { path, spoiler: true }] if *path == picture)
    )));
    app.dispatch(Action::CloseAside);

    // An announcement channel has no private threads; where only private
    // ones may start, the form starts one, and messages offer none.
    app.dispatch(Action::SelectRoom("discord:10:12".into()));
    app.dispatch(Action::NewThread {
        message: None,
        from: ThreadFrom::PlusButton,
    });
    assert!(!app.state.new_thread().unwrap().can_private);
    app.dispatch(Action::SelectRoom("discord:10:14".into()));
    assert!(app.state.new_thread.is_none(), "another room closes it");
    assert!(!app.can_start_threads());
    app.dispatch(Action::NewThread {
        message: None,
        from: ThreadFrom::PlusButton,
    });
    let form = app.state.new_thread().unwrap();
    assert!(form.only_private && form.private);
    app.dispatch(Action::SetThreadPrivate(false));
    assert!(app.state.new_thread().unwrap().private);
}

#[test]
fn a_threads_default_name_is_discords() {
    let named = |body: &str| {
        default_thread_name(&ChatMessage {
            body: body.into(),
            ..Default::default()
        })
    };
    assert_eq!(named("- # Short\nmore"), "Short");
    assert_eq!(named(""), "Thread");
    assert_eq!(named(&"x".repeat(50)), format!("{}...", "x".repeat(40)));
}

#[test]
#[cfg(feature = "discord")]
fn a_forum_shows_its_posts_as_discords_client_does() {
    use chat::{ListedThread, PostStarter};
    use yapfast_discord::{
        ready::{self, permission},
        service::{Snapshot, Status},
    };
    let bits = |bits: u64| bits.to_string();
    let post = |id: &str, name: &str, last: &str, flags: u64, tags: &[&str]| {
        serde_json::json!({
            "id": id, "type": 11, "name": name, "parent_id": "15", "guild_id": "10",
            "last_message_id": last, "flags": flags, "applied_tags": tags, "message_count": 2,
            "thread_metadata": {"archived": false, "locked": false},
        })
    };
    let snapshot = |posts: &[serde_json::Value]| {
        let mut server = ready::server(&serde_json::json!({
            "id": "10", "owner_id": "99", "properties": {"name": "Test"},
            "roles": [{"id": "10", "name": "@everyone", "position": 0, "permissions": bits(
                permission::VIEW_CHANNEL | permission::SEND_MESSAGES
                    | permission::READ_MESSAGE_HISTORY | permission::SEND_MESSAGES_IN_THREADS)}],
            "channels": [
                {"id": "11", "type": 0, "name": "general", "position": 0},
                {"id": "15", "type": 15, "name": "ideas", "position": 1,
                 "available_tags": [
                     {"id": "7", "name": "Bug", "emoji_id": null, "emoji_name": "🐛"},
                     {"id": "8", "name": "Idea", "emoji_id": "9", "emoji_name": "idea"},
                 ],
                 "default_reaction_emoji": {"emoji_id": null, "emoji_name": "👍"}},
            ],
            "threads": posts,
        }))
        .unwrap();
        // Read up to the joined post, which she opened; the newest she never
        // opened.
        for channel in &mut server.threads {
            if let Some(thread) = channel.thread.as_deref_mut() {
                thread.joined = channel.id == "45";
            }
        }
        let server = MarkedServer::marked(server, ServerMarks::default(), |channel| ChannelMarks {
            last_read: 45,
            opened: channel.thread.is_some() && channel.id == "45",
            ..Default::default()
        });
        discord_event(Snapshot {
            status: Status::Online,
            user_id: Some("1".into()),
            servers: vec![server],
            ..Default::default()
        })
    };
    let posts = vec![
        post("40", "Welcome", "41", 2, &[]),
        post("45", "Joined", "60", 0, &["7"]),
        post("50", "Newest", "50", 0, &["8", "7"]),
    ];
    let mut app = Application::default();
    app.apply(snapshot(&posts));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    let ideas = "discord:10:15".to_owned();
    let room = |id: &str| format!("discord:10:{id}");
    assert!(
        app.state
            .rooms
            .iter()
            .any(|listed| listed.id == ideas && listed.forum)
    );

    // Opening it shows its posts in place of messages: it asks for older
    // ones and for the first messages of the cards on top, pinned first.
    let effects = app.dispatch(Action::SelectRoom(ideas.clone()));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::Open(_))))
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::Discord(DiscordCommand::ViewForum { room })) if *room == ideas
    )));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::Discord(DiscordCommand::Threads { room, query: None, offset: 0 }))
            if *room == ideas
    )));
    let asked: Vec<_> = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Backend(Command::Discord(DiscordCommand::Posts { posts, .. })) => {
                Some(posts.clone())
            }
            _ => None,
        })
        .expect("first messages asked for");
    assert_eq!(asked, [room("40"), room("45"), room("50")]);
    assert!(app.state.room_ready);
    // A forum has no list of threads, pins or member list.
    assert_eq!(app.state.threads_channel(), None);
    assert!(!app.state.lists_members());

    let shown = app.forum_posts().unwrap();
    let names: Vec<_> = shown
        .active
        .iter()
        .map(|post| post.thread.name.as_str())
        .collect();
    assert_eq!(names, ["Welcome", "Joined", "Newest"]);
    let news: Vec<_> = shown.active.iter().map(|post| post.new).collect();
    assert_eq!(news, [false, false, true], "only one made since, unopened");
    let tags: Vec<_> = shown.active[2]
        .tags
        .iter()
        .map(|tag| tag.name.as_str())
        .collect();
    assert_eq!(tags, ["Bug", "Idea"], "in the forum's order");
    assert!(shown.active[0].thread.pinned);

    // Their first messages come; one Discord didn't answer for was deleted,
    // and nothing more is asked for.
    let starter = |text: &str| PostStarter {
        text: text.into(),
        ..Default::default()
    };
    let effects = app.apply(Event::Posts {
        room: ideas.clone(),
        result: Ok(vec![
            (room("40"), Some(starter("Hello"))),
            (room("45"), Some(starter("Idea"))),
        ]),
    });
    assert!(effects.is_empty());
    let shown = app.forum_posts().unwrap();
    assert_eq!(shown.active[0].starter, Some(Some(starter("Hello"))));
    assert_eq!(shown.active[2].starter, Some(None));

    // Older posts come with their own first messages.
    app.apply(Event::Threads {
        room: ideas,
        query: None,
        offset: 0,
        result: Ok((
            vec![ListedThread {
                room: room("30"),
                name: "Old".into(),
                archived: true,
                last: 30,
                ..Default::default()
            }],
            true,
        )),
        starters: vec![(room("30"), Some(starter("Long ago")))],
    });
    let shown = app.forum_posts().unwrap();
    assert_eq!(shown.older.len(), 1);
    assert_eq!(shown.older[0].starter, Some(Some(starter("Long ago"))));
    // Scrolled near the end, the next page of older posts loads.
    assert!(
        app.dispatch(Action::MorePosts)
            .iter()
            .any(|effect| matches!(
                effect,
                Effect::Backend(Command::Discord(DiscordCommand::Threads { offset: 25, .. }))
            ))
    );

    // A post made while it shows wants its first message.
    let mut more = posts;
    more.push(post("55", "Brand new", "55", 0, &[]));
    let effects = app.apply(snapshot(&more));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::Discord(DiscordCommand::Posts { posts, .. })) if *posts == [room("55")]
    )));

    // Another room forgets it all.
    let effects = app.dispatch(Action::SelectRoom(room("11")));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::Discord(DiscordCommand::CloseThreads))
    )));
    assert!(app.state.forum.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn a_forum_post_needs_a_title_a_message_and_a_tag_the_forum_requires() {
    use yapfast_discord::{
        ready::{self, permission},
        service::{Snapshot, Status},
    };
    let backend = |effects: Vec<Effect>| -> Vec<Command> {
        effects
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Backend(command) => Some(command),
                _ => None,
            })
            .collect()
    };
    let folder = tempfile::tempdir().unwrap();
    let crash = folder.path().join("crash.png");
    std::fs::File::create(&crash).unwrap().set_len(3).unwrap();
    let server = ready::server(&serde_json::json!({
        "id": "10", "owner_id": "99", "properties": {"name": "Test"},
        "roles": [{"id": "10", "name": "@everyone", "position": 0, "permissions": (
            permission::VIEW_CHANNEL | permission::SEND_MESSAGES
                | permission::READ_MESSAGE_HISTORY).to_string()}],
        "channels": [
            {"id": "11", "type": 0, "name": "general", "position": 0},
            {"id": "15", "type": 15, "name": "ideas", "position": 1, "flags": 16,
             "available_tags": [
                 {"id": "7", "name": "Bug", "emoji_id": null, "emoji_name": "🐛"},
                 {"id": "8", "name": "Idea", "emoji_id": null, "emoji_name": "💡"},
                 {"id": "9", "name": "Done", "moderated": true, "emoji_id": null, "emoji_name": "✅"},
             ]},
        ],
    }))
    .unwrap();
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        servers: vec![MarkedServer::from(server)],
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    let ideas = "discord:10:15".to_owned();
    app.dispatch(Action::SelectRoom(ideas.clone()));
    assert!(app.can_post(), "Send Messages is enough");
    // A moderated tag is for those who manage threads.
    let tags: Vec<_> = app
        .post_tags(&ideas)
        .into_iter()
        .map(|tag| tag.id)
        .collect();
    assert_eq!(tags, ["7", "8"]);

    app.dispatch(Action::NewPost);
    let error = |app: &Application| app.state.new_post().and_then(|post| post.error.clone());
    assert!(backend(app.dispatch(Action::CreatePost)).is_empty());
    assert_eq!(error(&app).as_deref(), Some("A title is required"));

    // A title is one line of at most 100 characters.
    app.dispatch(Action::WriteNewPost {
        title: Some(format!("Crash\n{}", "x".repeat(120))),
        body: None,
    });
    let title = app.state.new_post().unwrap().title.clone();
    assert!(title.starts_with("Crash x") && title.chars().count() == POST_TITLE_LIMIT);
    app.dispatch(Action::WriteNewPost {
        title: Some("Crash when pasting".into()),
        body: None,
    });
    assert!(backend(app.dispatch(Action::CreatePost)).is_empty());
    assert_eq!(error(&app).as_deref(), Some("A message is required"));

    // A file is a message too, and the forum requires a tag.
    app.dispatch(Action::Stage(vec![crash.clone()]));
    assert!(backend(app.dispatch(Action::CreatePost)).is_empty());
    assert_eq!(
        error(&app).as_deref(),
        Some("A tag is required to create a post in this channel")
    );
    for tag in ["9", "8", "7", "8"] {
        app.dispatch(Action::TogglePostTag(tag.into()));
    }
    assert_eq!(app.state.new_post().unwrap().tags, ["7"]);
    app.dispatch(Action::WriteNewPost {
        title: None,
        body: Some("It closes.\n".into()),
    });
    let posted = backend(app.dispatch(Action::CreatePost));
    assert!(matches!(
        posted.as_slice(),
        [Command::Discord(DiscordCommand::PostInForum { room, title, tags, body, files })]
            if *room == ideas && title == "Crash when pasting" && tags == &["7"]
                && body == "It closes." && files.len() == 1 && files[0].path == crash
    ));
    // Nothing changes while it's on its way, and a refusal says why.
    app.dispatch(Action::TogglePostTag("8".into()));
    assert_eq!(app.state.new_post().unwrap().tags, ["7"]);
    app.apply(Event::ForumPosted {
        room: ideas.clone(),
        result: Err("Couldn't create the post: Missing Access".into()),
    });
    assert!(!app.state.new_post().unwrap().posting);
    assert!(error(&app).is_some_and(|error| error.contains("Missing Access")));

    // Once made, the form and its files go.
    assert_eq!(backend(app.dispatch(Action::CreatePost)).len(), 1);
    app.apply(Event::ForumPosted {
        room: ideas.clone(),
        result: Ok("discord:10:70".into()),
    });
    assert!(app.state.new_post.is_none());
    assert!(app.state.staged().is_empty());

    // What was written stays while another room shows, and closing asks
    // first.
    app.dispatch(Action::NewPost);
    app.dispatch(Action::WriteNewPost {
        title: Some("Later".into()),
        body: None,
    });
    app.dispatch(Action::SelectRoom("discord:10:11".into()));
    assert!(app.state.new_post().is_none());
    app.dispatch(Action::SelectRoom(ideas));
    assert_eq!(app.state.new_post().unwrap().title, "Later");
    app.dispatch(Action::CloseNewPost);
    assert!(app.state.new_post().unwrap().leaving);
    app.dispatch(Action::StayInNewPost);
    assert!(!app.state.new_post().unwrap().leaving);
    app.dispatch(Action::LeaveNewPost);
    assert!(app.state.new_post.is_none());

    // Files dropped on the forum open the form.
    app.dispatch(Action::Stage(vec![crash]));
    assert!(app.state.new_post().is_some());
    assert_eq!(app.state.staged().len(), 1);
}

#[test]
#[cfg(feature = "discord")]
fn split_view_shows_a_dm_beside_a_channel_and_keeps_it() {
    use yapfast_discord::{
        ready::{Channel, Server},
        service::{Conversation, Snapshot, Status},
    };
    let mut app = connected();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        servers: vec![MarkedServer::from(Server {
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
        })],
        conversations: vec![Arc::new(Conversation {
            id: "30".into(),
            name: "Annie".into(),
            person: Some("2".into()),
            people: 2,
            unread: true,
            ..Default::default()
        })],
        ..Default::default()
    }));
    let dm = "discord:@me:30";
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));

    // Off, as it starts, menus don't offer it, and a Ctrl-click opens the
    // room as a click does.
    assert!(!app.menu_choices(dm).contains(&MenuChoice::OpenInSplit));
    app.dispatch(Action::OpenSplit("discord:10:22".into()));
    assert!(app.state.aside.is_none());
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:22"));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));

    // On, any Discord room but the open one offers it.
    app.dispatch(Action::SplitView(true));
    assert!(app.menu_choices(dm).contains(&MenuChoice::OpenInSplit));
    assert!(
        app.menu_choices("discord:10:22")
            .contains(&MenuChoice::OpenInSplit)
    );
    assert!(
        !app.menu_choices("discord:10:21")
            .contains(&MenuChoice::OpenInSplit)
    );
    let effects = app.dispatch(Action::OpenSplit(dm.into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Aside(Some(room)))] if room == dm
    ));
    assert!(app.state.aside.as_ref().is_some_and(|aside| aside.split));
    assert!(
        !app.menu_choices(dm).contains(&MenuChoice::OpenInSplit),
        "it shows already"
    );

    // It's read as it shows, as the open room is, and its composer writes
    // there.
    let effects = app.apply(Event::Timeline {
        room: dm.into(),
        messages: timeline(vec![ChatMessage {
            item: "d:5".into(),
            body: "Lunch?".into(),
            can_reply: true,
            ..Default::default()
        }]),
    });
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::MarkRead { room, .. }) if room == dm
    )));
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:21"));
    app.dispatch(Action::SetAsideDraft("Sure".into()));
    let effects = app.dispatch(Action::SendAside);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Send { room, body, .. })] if room == dm && body == "Sure"
    ));
    app.apply(Event::SendResult {
        room: dm.into(),
        body: "Sure".into(),
        result: Ok(()),
    });

    // It stays while the conversation goes to another channel, unlike a
    // thread beside it.
    let effects = app.dispatch(Action::SelectRoom("discord:10:22".into()));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::Aside(_))))
    );
    assert!(
        app.state
            .aside
            .as_ref()
            .is_some_and(|aside| aside.room == dm)
    );

    // Opening its room in the conversation takes it out of split view.
    let effects = app.dispatch(Action::Open(dm.into()));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::Aside(None))))
    );
    assert!(app.state.aside.is_none());
    assert_eq!(app.state.selected.as_deref(), Some(dm));

    // Turning it off closes what it shows.
    app.dispatch(Action::OpenSplit("discord:10:21".into()));
    assert!(app.state.aside.as_ref().is_some_and(|aside| aside.split));
    let effects = app.dispatch(Action::SplitView(false));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Aside(None))]
    ));
    assert!(app.state.aside.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn split_view_swaps_sides_and_comes_back_after_a_restart() {
    use yapfast_discord::{
        ready::{Channel, Server},
        service::{Conversation, Snapshot, Status},
    };
    let snapshot = || Snapshot {
        status: Status::Online,
        servers: vec![MarkedServer::from(Server {
            id: "10".into(),
            name: "Test".into(),
            text_channels: vec![Channel {
                id: "21".into(),
                name: "general".into(),
                ..Default::default()
            }],
            ..Default::default()
        })],
        conversations: vec![Arc::new(Conversation {
            id: "30".into(),
            name: "Annie".into(),
            person: Some("2".into()),
            people: 2,
            ..Default::default()
        })],
        ..Default::default()
    };
    let dm = "discord:@me:30";
    let mut app = connected();
    app.dispatch(Action::SplitView(true));
    app.apply(discord_event(snapshot()));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    app.dispatch(Action::OpenSplit(dm.into()));
    assert!(app.state.can_swap_split());

    // Swapping shows the DM in the conversation and the channel beside it,
    // each still open, so neither loads again.
    let effects = app.dispatch(Action::SwapSplit);
    assert_eq!(app.state.selected.as_deref(), Some(dm));
    assert!(
        app.state
            .aside
            .as_ref()
            .is_some_and(|aside| aside.split && aside.room == "discord:10:21")
    );
    let commands: Vec<String> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Backend(Command::Aside(None)) => Some("close".to_owned()),
            Effect::Backend(Command::Aside(Some(room))) => Some(format!("beside {room}")),
            Effect::Backend(Command::Open(room)) => Some(format!("open {room}")),
            _ => None,
        })
        .collect();
    assert_eq!(commands.first().map(String::as_str), Some("close"));
    assert!(
        commands
            .iter()
            .any(|command| command == "open discord:@me:30")
    );
    assert_eq!(
        commands.last().map(String::as_str),
        Some("beside discord:10:21")
    );

    // With split view closed there is nothing to swap.
    app.dispatch(Action::CloseAside);
    assert!(!app.state.can_swap_split());

    // After a restart, the room split view showed comes back once it's
    // listed and a room is open.
    let mut app = connected();
    app.dispatch(Action::SplitView(true));
    assert!(
        app.dispatch(Action::RestoreSplit(dm.into())).is_empty(),
        "not listed yet"
    );
    assert!(app.state.selected.is_some());
    let effects = app.apply(discord_event(snapshot()));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::Aside(Some(room))) if room == dm
    )));
    assert!(app.state.aside.as_ref().is_some_and(|aside| aside.split));
}

#[test]
#[cfg(feature = "discord")]
fn the_open_room_moves_to_split_view_and_the_conversation_goes_back() {
    use yapfast_discord::{
        ready::{Channel, Server},
        service::{Conversation, Snapshot, Status},
    };
    let mut app = connected();
    app.dispatch(Action::SplitView(true));
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        servers: vec![MarkedServer::from(Server {
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
        })],
        conversations: vec![Arc::new(Conversation {
            id: "30".into(),
            name: "Annie".into(),
            person: Some("2".into()),
            people: 2,
            ..Default::default()
        })],
        ..Default::default()
    }));
    let dm = "discord:@me:30";
    let split = |app: &Application| {
        app.state
            .aside
            .as_ref()
            .filter(|aside| aside.split)
            .map(|aside| aside.room.clone())
    };

    // Back to the room shown before it.
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    app.dispatch(Action::SelectRoom("discord:10:22".into()));
    app.dispatch(Action::OpenSplit("discord:10:22".into()));
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:21"));
    assert_eq!(split(&app).as_deref(), Some("discord:10:22"));

    // The room before it in split view: they change places.
    app.dispatch(Action::OpenSplit("discord:10:21".into()));
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:22"));
    assert_eq!(split(&app).as_deref(), Some("discord:10:21"));

    // Without one, a channel's server's first text channel.
    app.dispatch(Action::CloseAside);
    app.previous_room = None;
    app.dispatch(Action::OpenSplit("discord:10:22".into()));
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:21"));
    assert_eq!(split(&app).as_deref(), Some("discord:10:22"));

    // The room before is the one that showed, not one the way there
    // passed: going to a DM from a channel goes back to the channel.
    app.dispatch(Action::CloseAside);
    app.dispatch(Action::Open(dm.into()));
    app.dispatch(Action::OpenSplit(dm.into()));
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:21"));
    assert_eq!(split(&app).as_deref(), Some(dm));

    // And a DM's, the friends list, where split view stays.
    app.dispatch(Action::CloseAside);
    app.dispatch(Action::Open(dm.into()));
    app.previous_room = None;
    app.dispatch(Action::OpenSplit(dm.into()));
    assert_eq!(app.state.selected, None);
    assert_eq!(app.state.dm_screen, Some(DmScreen::Friends));
    assert_eq!(split(&app).as_deref(), Some(dm));
    // Opening a conversation from there keeps it.
    app.dispatch(Action::SelectRoom("dm".into()));
    assert_eq!(split(&app).as_deref(), Some(dm));
}
