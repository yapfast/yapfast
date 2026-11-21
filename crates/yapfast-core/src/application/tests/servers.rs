//! Discord servers: permissions, notifications, pins, search, invites and leaving.

use super::*;

#[test]
#[cfg(feature = "discord")]
fn discord_servers_say_what_this_account_may_do() {
    use chat::Permission;
    let app = discord_server();
    let roles = app.server_roles("discord:10");
    let listed: Vec<_> = roles
        .iter()
        .map(|role| (role.name.as_str(), role.managed))
        .collect();
    assert_eq!(listed, [("Bot", true), ("Mods", false), ("Helpers", false)]);
    assert_eq!(roles[1].color, Some(0x3498db));
    assert_eq!(
        app.server_roles("discord:10:11"),
        roles,
        "a channel's server"
    );

    assert!(app.may("discord:10", Permission::Kick));
    assert!(app.may("discord:10", Permission::ManageRoles));
    assert!(!app.may("discord:10", Permission::Ban));
    assert!(app.may("discord:10:11", Permission::SendMessages));
    assert!(
        !app.may("discord:10:12", Permission::SendMessages),
        "#news's overwrite"
    );
    assert!(!app.may("discord:@me:30", Permission::Kick));
    assert!(!app.may("discord:20", Permission::ViewChannel), "unknown");

    assert!(app.outranks("discord:10", "2", &["30".into()]));
    assert!(!app.outranks("discord:10", "3", &["40".into()]));
    assert!(!app.outranks("discord:10", "99", &[]), "the owner");
    assert!(!app.outranks("discord:10", "1", &[]), "herself");
    assert!(app.may_grant("discord:10", "30"));
    assert!(!app.may_grant("discord:10", "20"), "her own level");
    assert!(!app.may_grant("discord:10", "40"), "managed");
    assert!(!app.may_grant("discord:10", "10"), "@everyone");
}

#[test]
#[cfg(feature = "discord")]
fn discord_servers_and_channels_say_how_they_notify() {
    let app = discord_server();
    let server = app.server_notifications("discord:10").unwrap();
    assert_eq!(
        server.own,
        chat::Notifications {
            muted: false,
            muted_until: None,
            level: None,
            inherited: chat::NotifyLevel::Mentions,
        },
        "the server's default"
    );
    assert!(server.hide_muted && server.suppress_roles && !server.suppress_everyone);
    let news = app.notifications("discord:10:12").unwrap();
    assert!(news.muted && news.muted_until.is_some());
    assert_eq!(news.level, Some(chat::NotifyLevel::Nothing));
    assert_eq!(news.inherited, chat::NotifyLevel::All, "its category's");
    let general = app.notifications("discord:10:11").unwrap();
    assert_eq!(
        (general.muted, general.level, general.inherited),
        (false, None, chat::NotifyLevel::All)
    );
    let category = app.notifications("discord:10:4").unwrap();
    assert_eq!(
        (category.level, category.inherited),
        (Some(chat::NotifyLevel::All), chat::NotifyLevel::Mentions)
    );
    assert!(app.server_notifications("discord:10:11").is_none());
    assert!(app.notifications("discord:20").is_none());

    // A muted channel shows faint.
    let muted = |id: &str| {
        app.state
            .rooms
            .iter()
            .find(|room| room.id == id)
            .unwrap()
            .muted
    };
    assert!(muted("discord:10:12"));
    assert!(!muted("discord:10:11"));
}

#[test]
#[cfg(feature = "discord")]
fn discord_notification_changes_go_to_servers_and_channels() {
    use chat::{Muting, NotificationChange};
    let mut app = discord_server();
    let mut change = |place: &str, change| {
        app.dispatch(Action::ChangeNotifications {
            place: place.into(),
            change,
        })
    };
    let mute = NotificationChange::Mute(Muting::Minutes(15));
    assert!(matches!(
        change("discord:10", mute).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Notifications { place, change }))]
            if place == "discord:10" && *change == mute
    ));
    assert_eq!(
        change("discord:10:11", NotificationChange::Level(None)).len(),
        1
    );
    assert_eq!(
        change("discord:10", NotificationChange::HideMuted(false)).len(),
        1
    );
    assert!(
        change("discord:10:11", NotificationChange::SuppressRoles(true)).is_empty(),
        "the server's own"
    );
    assert!(
        change("discord:@me:30", mute).is_empty(),
        "a DM mutes from its menu"
    );
    assert!(change("!room:example.org", mute).is_empty());
    app.apply(discord_event(yapfast_discord::service::Snapshot {
        status: yapfast_discord::service::Status::Connecting,
        ..Default::default()
    }));
    assert!(
        app.dispatch(Action::ChangeNotifications {
            place: "discord:10".into(),
            change: mute,
        })
        .is_empty(),
        "while Discord is away"
    );
}

#[test]
#[cfg(feature = "discord")]
fn a_server_set_to_hide_muted_channels_lists_only_those_it_needs() {
    let mut app = discord_server();
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    let listed = |app: &Application| {
        app.state
            .scoped_rooms()
            .map(|room| room.name.clone())
            .collect::<Vec<_>>()
    };
    // #news is muted; #memes is too, but mentions her.
    app.dispatch(Action::SelectRoom("discord:10:11".into()));
    assert_eq!(listed(&app), ["general", "memes", "Lounge"]);
    // The open one stays.
    app.dispatch(Action::SelectRoom("discord:10:12".into()));
    assert_eq!(listed(&app), ["general", "news", "memes", "Lounge"]);
}

#[test]
#[cfg(feature = "discord")]
fn server_channels_list_their_pins_and_search_them() {
    let mut app = discord_server();
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    let general = "discord:10:11".to_owned();
    app.dispatch(Action::SelectRoom(general.clone()));
    assert!(matches!(
        app.dispatch(Action::ShowPins(true)).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Pins { room }))] if *room == general
    ));
    // A search looks through the whole server, as Discord's does.
    assert!(matches!(
        app.dispatch(Action::Search("plan".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Search { place, offset: 0, .. }))]
            if place == "discord:10"
    ));
    // A voice channel has neither.
    app.dispatch(Action::Open("discord:10:14".into()));
    assert!(app.state.conversation_that(|can| can.pins).is_none());
    assert!(app.dispatch(Action::ShowPins(true)).is_empty());
}

#[test]
#[cfg(feature = "discord")]
fn server_channels_and_categories_offer_their_notifications() {
    let app = discord_server();
    let notifying = [MenuChoice::ChannelMute, MenuChoice::ChannelNotifications];
    let offers = |id: &str| {
        let choices = app.state.menu_choices(id);
        notifying.iter().all(|choice| choices.contains(choice))
    };
    assert!(offers("discord:10:11"));
    assert!(
        offers("discord:10:12"),
        "a channel muted with its own level"
    );
    assert!(!offers("discord:10:14"), "a voice channel");
    assert_eq!(
        app.state.menu_choices("discord:10:4"),
        notifying,
        "a category, by the room id its heading's menu names"
    );
    assert!(!offers("discord:10"), "the server");
    assert!(app.state.menu_choices("discord:20:4").is_empty(), "unknown");
    assert_eq!(
        app.category_of("discord:10:11").as_deref(),
        Some("discord:10:4")
    );
    assert_eq!(
        app.category_of("discord:10:4"),
        None,
        "a category is in none"
    );
    assert_eq!(app.category_of("discord:@me:30"), None);
}

#[test]
#[cfg(feature = "discord")]
fn the_open_channel_keeps_its_topic_till_another_opens() {
    let mut app = discord_server();
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:11".into()));
    let topic = Arc::new(chat::Topic {
        body: "Plans, and **rules**".into(),
        ..Default::default()
    });
    app.apply(Event::Topic {
        room: "discord:10:13".into(),
        topic: Some(topic.clone()),
    });
    assert!(app.state.topic.is_none(), "another channel's");
    app.apply(Event::Topic {
        room: "discord:10:11".into(),
        topic: Some(topic.clone()),
    });
    assert!(
        app.state
            .topic
            .as_ref()
            .is_some_and(|shown| Arc::ptr_eq(shown, &topic))
    );
    app.apply(Event::Topic {
        room: "discord:10:11".into(),
        topic: None,
    });
    assert!(app.state.topic.is_none(), "taken away");
    app.apply(Event::Topic {
        room: "discord:10:11".into(),
        topic: Some(topic),
    });
    app.dispatch(Action::SelectRoom("discord:10:13".into()));
    assert!(app.state.topic.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn a_server_search_stays_open_in_its_channels_and_jumps_into_them() {
    let mut app = discord_server();
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:11".into()));
    app.dispatch(Action::Search("plan".into()));
    let hit = |room: &str, item: &str| chat::SearchHit {
        room: room.into(),
        message: message(item, false),
        thread: None,
    };
    app.apply(Event::Found {
        place: "discord:10".into(),
        query: "plan".into(),
        sort: chat::SearchSort::Newest,
        offset: 0,
        result: Ok(chat::Searched::Page {
            total: 2,
            hits: vec![hit("discord:10:13", "d:7"), hit("discord:10:11", "d:5")],
            words: "plan".into(),
            indexing_older: false,
        }),
    });
    assert_eq!(app.state.search_shown().unwrap().results.len(), 2);

    // A result in #general, open, jumps there at once.
    app.apply(Event::Timeline {
        room: "discord:10:11".into(),
        messages: vec![message("d:5", false).into()],
    });
    app.dispatch(Action::JumpInRoom {
        room: "discord:10:11".into(),
        item: "d:5".into(),
    });
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:11"));
    assert_eq!(
        app.state.jump.as_ref().map(|(item, _)| item.as_str()),
        Some("d:5")
    );

    // One in #memes, unread since "d:3", opens it on the messages around
    // the result, the search staying.
    let effects = app.dispatch(Action::JumpInRoom {
        room: "discord:10:13".into(),
        item: "d:8".into(),
    });
    assert!(matches!(
        effects.as_slice(),
        [.., Effect::Backend(Command::Around { room, item })]
            if room == "discord:10:13" && item == "d:8"
    ));
    assert!(
        app.state.search_shown().is_some(),
        "the server's search stays"
    );
    let unread_jumps = app.state.unread_jump;
    let around = |app: &mut Application, items: &[&str]| {
        app.apply(window("discord:10:13", chat::Direction::Around, false));
        let messages = items.iter().map(|item| message(item, false)).collect();
        app.apply(Event::Timeline {
            room: "discord:10:13".into(),
            messages: timeline(messages),
        })
    };
    // A window that doesn't reach the last read message stays: the result
    // shows rather than the messages around "d:3".
    let effects = around(&mut app, &["d:7", "d:8", "d:9"]);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::Around { .. })))
    );
    assert_eq!(
        app.state.jump.as_ref().map(|(item, _)| item.as_str()),
        Some("d:8")
    );
    assert_eq!(app.state.first_unread.as_deref(), Some("d:7"));
    // One that reaches it shows the "New" divider without scrolling to it.
    app.dispatch(Action::SelectRoom("discord:10:11".into()));
    app.dispatch(Action::JumpInRoom {
        room: "discord:10:13".into(),
        item: "d:8".into(),
    });
    around(&mut app, &["d:3", "d:4", "d:8", "d:9"]);
    assert_eq!(
        app.state.jump.as_ref().map(|(item, _)| item.as_str()),
        Some("d:8")
    );
    assert_eq!(app.state.first_unread.as_deref(), Some("d:4"));
    assert_eq!(
        app.state.unread_jump, unread_jumps,
        "the \"New\" divider doesn't take the view from it"
    );

    // A voice channel can't search, so it closes the search.
    app.dispatch(Action::Open("discord:10:14".into()));
    assert!(app.state.search.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn a_discord_servers_menu_and_header_offer_its_notifications() {
    use MenuChoice::*;
    let mut app = discord_server();
    assert_eq!(
        app.menu_choices("discord:10"),
        [
            MarkRead,
            MuteServer,
            ServerNotifications,
            HideMuted,
            // Her role manages roles.
            ServerSettings,
            EditServerProfile,
            LeaveServer,
            CopyLink,
            CopyId
        ]
    );
    assert_eq!(
        app.header_choices("discord:10"),
        [
            ServerSettings,
            ServerNotifications,
            EditServerProfile,
            HideMuted,
            LeaveServer,
            CopyId
        ],
        "the header's dropdown lists the menu's choices in its own order"
    );
    assert!(app.header_choices("discord:10:11").is_empty());
    assert_eq!(
        app.state.menu_choices("discord:10:11"),
        [ChannelMute, ChannelNotifications, CopyLink, CopyId]
    );
    assert_eq!(
        app.menu_choices("discord:10:11"),
        [
            PinChannel,
            CopyLink,
            ChannelMute,
            ChannelNotifications,
            // Her role may manage its permissions.
            EditChannel,
            CopyId
        ],
        "a channel's menu is a conversation's, with pinning it, and its \
         notifications after its link"
    );

    change_marks(&mut app, |_, marks| {
        let mut settings =
            yapfast_discord::notifications::Notifications::clone(&marks.notifications);
        settings.mute = yapfast_discord::notifications::Mute {
            muted: true,
            until: None,
        };
        marks.notifications = Arc::new(settings);
    });
    assert_eq!(app.menu_choices("discord:10")[1], UnmuteServer);

    // Discord takes no changes while it's away, so they aren't offered.
    app.apply(discord_event(yapfast_discord::service::Snapshot {
        status: yapfast_discord::service::Status::Connecting,
        ..yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap())
    }));
    assert_eq!(app.menu_choices("discord:10"), [MarkRead, CopyLink, CopyId]);
    assert_eq!(
        app.header_choices("discord:10"),
        [CopyId],
        "the header's dropdown keeps Copy Server ID, which changes nothing"
    );
}

#[test]
#[cfg(feature = "discord")]
fn a_discord_category_has_a_menu_that_marks_its_channels_read() {
    use MenuChoice::*;
    let mut app = discord_server();
    assert!(app.state.is_category("discord:10:4"));
    assert!(!app.state.is_category("discord:10:11"));
    assert!(!app.state.is_category("discord:10"));
    assert_eq!(
        app.category_of("discord:10:12").as_deref(),
        Some("discord:10:4")
    );
    assert_eq!(app.category_of("discord:10:4"), None);
    // #memes mentions her; the header's notification choices sit between.
    assert_eq!(
        app.menu_choices("discord:10:4"),
        [
            MarkRead,
            ChannelMute,
            ChannelNotifications,
            EditChannel,
            CopyId
        ]
    );
    assert!(matches!(
        app.dispatch(Action::MarkRead("discord:10:4".into())).as_slice(),
        [Effect::Backend(Command::MarkRead { room, .. })] if room == "discord:10:13"
    ));
    change_marks(&mut app, |server, marks| {
        for channel in &server.text_channels {
            marks.channel_mut(server, &channel.id).unwrap().mentions = 0;
        }
    });
    assert_eq!(
        app.menu_choices("discord:10:4"),
        [ChannelMute, ChannelNotifications, EditChannel, CopyId]
    );
    assert!(
        app.dispatch(Action::MarkRead("discord:10:4".into()))
            .is_empty()
    );
}

#[test]
#[cfg(feature = "discord")]
fn discord_invites_go_to_a_channel_this_account_may_invite_to() {
    use MenuChoice::*;
    use yapfast_discord::ready::permission;
    let mut app = discord_server();
    assert!(
        !app.menu_choices("discord:10").contains(&Invite),
        "nobody may"
    );
    assert!(
        app.dispatch(Action::OpenInvite("discord:10".into()))
            .is_empty()
    );
    change_server(&mut app, |server| {
        let bits = permission::VIEW_CHANNEL
            | permission::SEND_MESSAGES
            | permission::CREATE_INSTANT_INVITE;
        server.put_role(
            &serde_json::json!({"id": "10", "name": "@everyone", "position": 0,
                                            "permissions": bits.to_string()}),
        );
    });
    assert_eq!(app.menu_choices("discord:10")[1], Invite);
    assert_eq!(app.header_choices("discord:10")[0], Invite);
    assert_eq!(
        app.menu_choices("discord:10:11"),
        [
            Invite,
            PinChannel,
            CopyLink,
            ChannelMute,
            ChannelNotifications,
            // Her role may manage its permissions.
            EditChannel,
            CopyId
        ],
        "inviting to a channel comes before pinning it and its link"
    );
    assert!(
        !app.menu_choices("discord:10:4").contains(&Invite),
        "a category"
    );

    // The server's first channel, or the open one.
    assert_eq!(
        app.invite_channel("discord:10").as_deref(),
        Some("discord:10:11")
    );
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:12".into()));
    assert_eq!(
        app.invite_channel("discord:10").as_deref(),
        Some("discord:10:12")
    );

    let effects = app.dispatch(Action::OpenInvite("discord:10".into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::CreateInvite { room, max_age: 604_800, max_uses: 0 }))]
            if room == "discord:10:12"
    ));
    let invite = app.state.invite.clone().unwrap();
    assert_eq!(
        (invite.server.as_str(), invite.channel.as_str()),
        ("Test", "news")
    );
    assert!(invite.making && invite.link.is_none());
    assert!(
        app.dispatch(Action::MakeInvite {
            max_age: 3600,
            max_uses: 5
        })
        .is_empty(),
        "one link at a time"
    );
    app.apply(Event::InviteMade {
        room: "discord:10:11".into(),
        result: Ok("elsewhere".into()),
    });
    assert!(
        app.state.invite.as_ref().unwrap().link.is_none(),
        "another channel's"
    );
    app.apply(Event::InviteMade {
        room: "discord:10:12".into(),
        result: Ok("abc123".into()),
    });
    let invite = app.state.invite.clone().unwrap();
    assert_eq!(invite.link.as_deref(), Some("https://discord.gg/abc123"));
    assert!(!invite.making);

    assert!(
        app.dispatch(Action::MakeInvite {
            max_age: 99,
            max_uses: 0
        })
        .is_empty(),
        "only Discord's choices"
    );
    assert!(matches!(
        app.dispatch(Action::MakeInvite {
            max_age: 3600,
            max_uses: 5
        })
        .as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::CreateInvite {
                max_age: 3600,
                max_uses: 5,
                ..
            }
        ))]
    ));
    app.apply(Event::InviteMade {
        room: "discord:10:12".into(),
        result: Err("Could not make an invite: no".into()),
    });
    let invite = app.state.invite.clone().unwrap();
    assert_eq!(
        invite.error.as_deref(),
        Some("Could not make an invite: no")
    );
    assert_eq!(
        invite.link.as_deref(),
        Some("https://discord.gg/abc123"),
        "the last one stays"
    );
    app.dispatch(Action::CloseInvite);
    assert!(app.state.invite.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn leaving_a_discord_server_asks_first_and_is_not_for_its_owner() {
    use MenuChoice::*;
    let mut app = discord_server();
    let server = "discord:10".to_owned();
    app.dispatch(Action::Navigate(Navigation::Server(server.clone())));
    assert!(
        app.dispatch(Action::AskLeaveServer(server.clone()))
            .is_empty()
    );
    let dialog = app.state.leave.clone().unwrap();
    assert_eq!(
        (dialog.place.as_str(), dialog.name.as_str()),
        ("discord:10", "Test")
    );
    assert!(matches!(
        app.dispatch(Action::Leave).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::LeaveServer { server }))] if server == "discord:10"
    ));
    assert!(app.dispatch(Action::Leave).is_empty(), "once");
    app.apply(Event::Left {
        place: server.clone(),
        result: Err("Couldn't leave the server: no".into()),
    });
    let dialog = app.state.leave.clone().unwrap();
    assert!(!dialog.leaving);
    assert_eq!(
        dialog.error.as_deref(),
        Some("Couldn't leave the server: no")
    );
    app.dispatch(Action::Leave);
    app.apply(Event::Left {
        place: server.clone(),
        result: Ok(()),
    });
    assert!(app.state.leave.is_none());
    // Its GUILD_DELETE takes it out of the list, and Yapfast back to the DMs.
    app.apply(discord_event(yapfast_discord::service::Snapshot {
        servers: vec![],
        ..yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap())
    }));
    assert_eq!(app.state.navigation, Navigation::DirectMessages);
    assert!(app.state.servers.iter().all(|known| known.id != server));

    // Its owner may not; Discord asks them to hand it on or delete it.
    let mut app = discord_server();
    change_server(&mut app, |known| known.owner_id = Some("1".into()));
    assert!(!app.menu_choices("discord:10").contains(&LeaveServer));
    assert!(!app.header_choices("discord:10").contains(&LeaveServer));
    assert!(
        app.dispatch(Action::AskLeaveServer("discord:10".into()))
            .is_empty()
    );
    assert!(app.state.leave.is_none());
}
