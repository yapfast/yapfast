//! A server's channel list: pinned and followed channels, Browse Channels, and making and changing channels.

use super::*;

#[test]
#[cfg(feature = "discord")]
fn server_channels_pin_to_the_top_as_discords_client_pins_them() {
    use MenuChoice::*;
    use yapfast_discord::notifications::Override;
    let mut app = discord_server();
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
        "pinning comes before the link, and notifications after"
    );
    assert!(
        app.menu_choices("discord:10:14").contains(&PinChannel),
        "a voice channel"
    );
    assert!(
        !app.menu_choices("discord:10:4").contains(&PinChannel),
        "a category"
    );
    assert!(
        app.dispatch(Action::PinChannel {
            room: "discord:10:11".into(),
            pinned: false,
        })
        .is_empty(),
        "it isn't pinned"
    );

    // A channel without an override pins with the opt-in flag too; one with
    // keeps its other flags.
    let pin = |app: &mut Application, room: &str, pinned| {
        let effects = app.dispatch(Action::PinChannel {
            room: room.into(),
            pinned,
        });
        match effects.as_slice() {
            [
                Effect::Backend(Command::Discord(DiscordCommand::PinChannel { room: sent, flags })),
            ] if sent == room => *flags,
            other => panic!("{} effects", other.len()),
        }
    };
    assert_eq!(pin(&mut app, "discord:10:13", true), 6144);
    assert!(
        !app.channel_pinned("discord:10:13"),
        "not before Discord says so"
    );

    // Discord's word: Lounge, a voice channel, lists after #memes and
    // #news, by position, though it came first.
    change_marks(&mut app, |_, marks| {
        let mut settings =
            yapfast_discord::notifications::Notifications::clone(&marks.notifications);
        for (channel, flags) in [("14", 6144), ("13", 6144), ("12", 2048 | 1024)] {
            let known = settings.overrides.entry(channel.into()).or_default();
            *known = Override { flags, ..*known };
        }
        marks.notifications = Arc::new(settings);
    });
    assert_eq!(
        app.state.servers[0].pinned,
        ["discord:10:12", "discord:10:13", "discord:10:14"]
    );
    assert!(app.menu_choices("discord:10:13").contains(&UnpinChannel));
    assert_eq!(pin(&mut app, "discord:10:13", false), 4096);
    assert_eq!(
        pin(&mut app, "discord:10:12", false),
        1024,
        "its other flags stay"
    );
    assert!(
        app.dispatch(Action::PinChannel {
            room: "discord:10:13".into(),
            pinned: true,
        })
        .is_empty(),
        "already pinned"
    );
}

#[test]
#[cfg(feature = "discord")]
fn a_community_server_lists_only_the_channels_followed_as_discords_client_does() {
    use MenuChoice::*;
    use yapfast_discord::notifications::Override;
    let mut app = discord_server();
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:11".into()));
    let listed = |app: &Application| {
        app.state
            .scoped_rooms()
            .map(|room| room.name.clone())
            .collect::<Vec<_>>()
    };
    let settings = |app: &mut Application, server_flags: u64, flags: &[(&str, u64)]| {
        let flags: Vec<(String, u64)> = flags
            .iter()
            .map(|(channel, flags)| ((*channel).to_owned(), *flags))
            .collect();
        change_server(app, |server| server.community = true);
        change_marks(app, move |_, marks| {
            let mut settings =
                yapfast_discord::notifications::Notifications::clone(&marks.notifications);
            // Muted channels show, to see only what following hides.
            settings.hide_muted_channels = false;
            settings.flags = server_flags;
            for (channel, flags) in flags {
                let known = settings.overrides.entry(channel).or_default();
                *known = Override { flags, ..*known };
            }
            marks.notifications = Arc::new(settings);
        });
    };
    let follow = |app: &mut Application, room: &str, follow| match app
        .dispatch(Action::FollowChannel {
            room: room.into(),
            follow,
        })
        .as_slice()
    {
        [
            Effect::Backend(Command::Discord(DiscordCommand::FollowChannel { room: sent, flags })),
        ] if sent == room => Some(*flags),
        [] => None,
        other => panic!("{} effects", other.len()),
    };

    // Outside a Community server, nothing to choose.
    assert_eq!(
        app.server_notifications("discord:10").unwrap().show_all,
        None
    );
    assert!(!app.menu_choices("discord:10").contains(&ShowAllChannels));

    // A Community server shows every channel until told otherwise.
    settings(&mut app, 0, &[]);
    assert_eq!(listed(&app), ["general", "news", "memes", "Lounge"]);
    assert_eq!(
        app.server_notifications("discord:10").unwrap().show_all,
        Some(true)
    );
    let choices = app.menu_choices("discord:10");
    let at = choices.iter().position(|choice| *choice == ShowAllChannels);
    assert_eq!(at.map(|at| choices[at + 1]), Some(HideMuted));
    assert!(app.header_choices("discord:10").contains(&ShowAllChannels));
    assert_eq!(
        follow(&mut app, "discord:10:11", true),
        None,
        "not while all show"
    );
    // Turning it off with none followed follows every channel she sees.
    assert!(matches!(
        app.dispatch(Action::ShowAllChannels { server: "discord:10".into(), show: false }).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ShowAllChannels { server, flags: 16384, follow }))]
            if server == "discord:10"
                && *follow == [("11".into(), 4096), ("12".into(), 4096), ("13".into(), 4096), ("14".into(), 4096)]
    ));

    // Following #general alone: #news goes; #memes stays, mentioning her,
    // and Lounge, a voice channel, as every voice channel does.
    settings(&mut app, 16384 | 1, &[("11", 4096)]);
    assert_eq!(
        app.server_notifications("discord:10").unwrap().show_all,
        Some(false)
    );
    assert_eq!(listed(&app), ["general", "memes", "Lounge"]);
    app.dispatch(Action::SelectRoom("discord:10:12".into()));
    assert_eq!(
        listed(&app),
        ["general", "news", "memes", "Lounge"],
        "the open one stays"
    );
    assert_eq!(
        app.menu_choices("discord:10:11"),
        [
            PinChannel,
            RemoveFromChannelList,
            CopyLink,
            ChannelMute,
            ChannelNotifications,
            EditChannel,
            CopyId
        ],
        "after pinning, before the link"
    );
    assert!(
        app.menu_choices("discord:10:12")
            .contains(&AddToChannelList)
    );
    assert_eq!(
        app.menu_choices("discord:10:4").first(),
        Some(&MarkRead),
        "#memes is unread"
    );
    assert_eq!(app.menu_choices("discord:10:4")[1], AddToChannelList);
    assert_eq!(follow(&mut app, "discord:10:12", true), Some(4096));
    assert_eq!(follow(&mut app, "discord:10:11", false), Some(0));
    assert_eq!(follow(&mut app, "discord:10:11", true), None, "followed");
    // Turning it back on keeps what she follows; off again follows nothing more.
    assert!(matches!(
        app.dispatch(Action::ShowAllChannels { server: "discord:10".into(), show: true }).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ShowAllChannels { flags: 1, follow, .. }))]
            if follow.is_empty()
    ));
    assert!(
        app.dispatch(Action::ShowAllChannels {
            server: "discord:10".into(),
            show: false
        })
        .is_empty(),
        "already off"
    );

    // Following the category follows each channel in it; a channel's menu
    // then unfollows the category, and a pinned channel shows unfollowed.
    app.dispatch(Action::SelectRoom("discord:10:11".into()));
    settings(&mut app, 16384, &[("11", 0), ("4", 4096)]);
    assert_eq!(listed(&app), ["general", "news", "memes", "Lounge"]);
    assert!(
        app.menu_choices("discord:10:12")
            .contains(&UnfollowCategory)
    );
    assert!(
        app.menu_choices("discord:10:4")
            .contains(&RemoveFromChannelList)
    );
    assert_eq!(follow(&mut app, "discord:10:4", false), Some(0));
    settings(&mut app, 16384, &[("4", 0), ("12", 2048)]);
    assert_eq!(listed(&app), ["general", "news", "memes", "Lounge"]);
    app.dispatch(Action::SelectRoom("discord:10:13".into()));
    assert_eq!(listed(&app), ["news", "memes", "Lounge"]);
    assert_eq!(
        follow(&mut app, "discord:10:12", true),
        Some(2048 | 4096),
        "keeps the pin"
    );
}

#[test]
#[cfg(feature = "discord")]
fn browse_channels_ticks_the_channels_followed_as_discords_client_does() {
    use yapfast_discord::notifications::Override;
    let mut app = discord_server();
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:11".into()));
    let settings = |app: &mut Application, server_flags: u64, flags: &[(&str, u64)]| {
        let flags: Vec<(String, u64)> = flags
            .iter()
            .map(|(channel, flags)| ((*channel).to_owned(), *flags))
            .collect();
        change_server(app, |server| server.community = true);
        change_marks(app, move |_, marks| {
            let mut settings =
                yapfast_discord::notifications::Notifications::clone(&marks.notifications);
            settings.flags = server_flags;
            for (channel, flags) in flags {
                let known = settings.overrides.entry(channel).or_default();
                *known = Override { flags, ..*known };
            }
            marks.notifications = Arc::new(settings);
        });
    };
    fn ticked(app: &Application) -> Vec<(&str, bool, bool)> {
        app.state
            .browse
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .map(|row| (row.name.as_str(), row.followed, row.by_category))
            .collect()
    }
    let tick = |app: &mut Application, room: &str, follow| {
        app.dispatch(Action::BrowseFollow {
            room: room.into(),
            follow,
        })
    };
    app.dispatch(Action::BrowseChannels);
    assert!(app.state.browse.is_none(), "not a Community server");

    // With Show All Channels on and none followed, every box is ticked.
    settings(&mut app, 0, &[]);
    app.dispatch(Action::BrowseChannels);
    assert_eq!(app.state.selected, None, "in place of the conversation");
    let browse = app.state.browse.as_ref().unwrap();
    assert_eq!(browse.server, "discord:10");
    let kinds: Vec<_> = browse
        .rows
        .iter()
        .map(|row| (row.id.as_str(), row.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("discord:10:4", BrowsedKind::Category),
            ("discord:10:11", BrowsedKind::Text),
            ("discord:10:12", BrowsedKind::Text),
            ("discord:10:13", BrowsedKind::Text),
            ("discord:10:14", BrowsedKind::Voice),
        ]
    );
    assert!(ticked(&app).iter().all(|(_, followed, _)| *followed));
    // Clearing one follows the rest and lists only those.
    assert!(matches!(
        tick(&mut app, "discord:10:12", false).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ShowAllChannels { server, flags: 16384, follow }))]
            if server == "discord:10"
                && *follow == [("11".into(), 4096), ("13".into(), 4096), ("14".into(), 4096)]
    ));
    assert!(
        tick(&mut app, "discord:10:11", true).is_empty(),
        "already ticked"
    );

    // Discord's word shows; ticking a channel or the category follows it.
    settings(&mut app, 16384, &[("11", 4096), ("13", 4096), ("14", 4096)]);
    assert_eq!(
        ticked(&app),
        [
            ("Talk", false, false),
            ("general", true, false),
            ("news", false, false),
            ("memes", true, false),
            ("Lounge", true, false),
        ]
    );
    assert!(matches!(
        tick(&mut app, "discord:10:4", true).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::FollowChannel { room, flags: 4096 }))]
            if room == "discord:10:4"
    ));
    // A followed category's channels change only with it.
    settings(&mut app, 16384, &[("4", 4096)]);
    assert_eq!(ticked(&app)[2], ("news", true, true));
    assert!(tick(&mut app, "discord:10:12", false).is_empty());

    // With Show All Channels on and some followed, ticking one turns it off.
    settings(&mut app, 0, &[("4", 0), ("11", 4096), ("13", 0), ("14", 0)]);
    assert!(matches!(
        tick(&mut app, "discord:10:12", true).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ShowAllChannels { flags: 16384, follow, .. }))]
            if *follow == [("12".into(), 4096)]
    ));

    // A conversation, or another place, takes its place.
    app.dispatch(Action::SelectRoom("discord:10:11".into()));
    assert!(app.state.browse.is_none());
    app.dispatch(Action::BrowseChannels);
    app.dispatch(Action::Navigate(Navigation::DirectMessages));
    assert!(app.state.browse.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn discord_channels_and_categories_are_made_changed_and_deleted_from_their_menus() {
    use MenuChoice::*;
    use chat::{ChannelKind, ChannelSettings};
    use yapfast_discord::ready::permission;
    let mut app = discord_server();
    assert!(
        !app.menu_choices("discord:10").contains(&CreateChannel),
        "nobody may manage channels"
    );
    assert!(
        app.dispatch(Action::NewChannel {
            place: "discord:10".into(),
            kind: ChannelKind::Text
        })
        .is_empty()
    );
    assert!(app.state.channel_dialog.is_none());
    assert!(
        !app.may_create_channel("discord:10:4"),
        "the \"+\" on a category heading needs Manage Channels"
    );
    change_server(&mut app, |server| {
        let bits =
            permission::VIEW_CHANNEL | permission::SEND_MESSAGES | permission::MANAGE_CHANNELS;
        server.put_role(
            &serde_json::json!({"id": "10", "name": "@everyone", "position": 0,
                                            "permissions": bits.to_string()}),
        );
    });
    assert!(app.may_create_channel("discord:10:4"));
    assert!(
        !app.may_create_channel("discord:10:11"),
        "a channel is no category"
    );
    assert!(!app.may_create_channel(""));
    assert_eq!(
        app.menu_choices("discord:10")[6..],
        [CreateChannel, CreateCategory, LeaveServer, CopyLink, CopyId]
    );
    assert_eq!(
        app.header_choices("discord:10"),
        [
            ServerSettings,
            CreateChannel,
            CreateCategory,
            ServerNotifications,
            EditServerProfile,
            HideMuted,
            LeaveServer,
            CopyId
        ]
    );
    assert_eq!(
        app.menu_choices("discord:10:11"),
        [
            PinChannel,
            CopyLink,
            ChannelMute,
            ChannelNotifications,
            EditChannel,
            DuplicateChannel,
            CreateChannel,
            DeleteChannel,
            CopyId
        ]
    );
    assert_eq!(
        app.menu_choices("discord:10:4"),
        [
            MarkRead,
            ChannelMute,
            ChannelNotifications,
            EditChannel,
            DeleteChannel,
            CopyId
        ]
    );

    // From the server: text or voice, outside any category.
    app.dispatch(Action::NewChannel {
        place: "discord:10".into(),
        kind: ChannelKind::Text,
    });
    let dialog = app.state.channel_dialog.clone().unwrap();
    assert_eq!(dialog.ask, ChannelAsk::Create { parent: None });
    assert_eq!(
        (dialog.server.as_str(), dialog.server_name.as_str()),
        ("discord:10", "Test")
    );
    let named = |name: &str| ChannelSettings {
        name: name.into(),
        ..Default::default()
    };
    assert!(
        app.dispatch(Action::SaveChannel {
            kind: ChannelKind::Text,
            settings: named("  ")
        })
        .is_empty()
    );
    assert_eq!(
        app.state.channel_dialog.as_ref().unwrap().error.as_deref(),
        Some("Give it a name.")
    );
    assert!(matches!(
        app.dispatch(Action::SaveChannel { kind: ChannelKind::Text, settings: named(" Plans  For Later ") }).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::CreateChannel {
            server, kind: ChannelKind::Text, parent: None, copy_of: None, settings
        }))] if server == "discord:10" && settings.name == "plans-for-later"
    ));
    assert!(
        app.dispatch(Action::SaveChannel {
            kind: ChannelKind::Text,
            settings: named("again")
        })
        .is_empty(),
        "one save at a time"
    );
    app.apply(Event::ChannelSaved(Err(
        "Couldn't make the channel: no".into()
    )));
    let dialog = app.state.channel_dialog.clone().unwrap();
    assert!(!dialog.saving);
    assert_eq!(
        dialog.error.as_deref(),
        Some("Couldn't make the channel: no")
    );
    assert!(matches!(
        app.dispatch(Action::SaveChannel { kind: ChannelKind::Voice, settings: named("Hang Out") }).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::CreateChannel {
            kind: ChannelKind::Voice, settings, ..
        }))] if settings.name == "Hang Out"
    ));
    app.apply(Event::ChannelSaved(Ok(Some("discord:10:15".into()))));
    assert!(app.state.channel_dialog.is_none());

    // From a channel: one like it, in its category.
    app.dispatch(Action::NewChannel {
        place: "discord:10:14".into(),
        kind: ChannelKind::Text,
    });
    let dialog = app.state.channel_dialog.clone().unwrap();
    assert_eq!(
        dialog.ask,
        ChannelAsk::Create {
            parent: Some("discord:10:4".into())
        }
    );
    assert_eq!(
        (dialog.name.as_str(), dialog.kind),
        ("Talk", ChannelKind::Voice)
    );
    app.dispatch(Action::CloseChannelDialog);

    app.dispatch(Action::NewChannel {
        place: "discord:10".into(),
        kind: ChannelKind::Category,
    });
    assert!(
        matches!(
            app.dispatch(Action::SaveChannel { kind: ChannelKind::Voice, settings: named("Games Night") }).as_slice(),
            [Effect::Backend(Command::Discord(DiscordCommand::CreateChannel {
                kind: ChannelKind::Category, settings, ..
            }))] if settings.name == "Games Night"
        ),
        "a category stays one, and keeps its capitals"
    );
    app.dispatch(Action::CloseChannelDialog);

    // A text channel's settings open over the window, loading Overview
    // first.
    let general = "discord:10:11".to_owned();
    assert!(matches!(
        app.dispatch(Action::OpenChannelSettings(general.clone())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ChannelSettings { room }))] if *room == general
    ));
    let input = |app: &mut Application, input| app.dispatch(Action::ChannelSettings(input));
    assert!(
        input(&mut app, ChannelInput::Save).is_empty(),
        "not before they're here"
    );
    let loaded = ChannelSettings {
        name: "general".into(),
        topic: "Say hi".into(),
        slowmode: 10,
        nsfw: false,
    };
    app.apply(Event::ChannelLoaded {
        room: "discord:10:12".into(),
        result: Ok(ChannelSettings::default()),
    });
    let screen = |app: &Application| app.state.channel_settings.clone().unwrap();
    assert!(screen(&app).overview.is_none(), "another channel's");
    app.apply(Event::ChannelLoaded {
        room: general.clone(),
        result: Ok(loaded.clone()),
    });
    assert_eq!(screen(&app).overview.unwrap().1, loaded);
    input(&mut app, ChannelInput::Slowmode(7));
    assert!(!screen(&app).changed(), "only Discord's slowmodes");
    input(&mut app, ChannelInput::Slowmode(30));
    input(&mut app, ChannelInput::Name("General Chat".into()));
    assert!(screen(&app).changed());
    assert!(
        input(&mut app, ChannelInput::Close).is_empty() && screen(&app).nudge == 1,
        "closing waits for the bar"
    );
    assert!(matches!(
        input(&mut app, ChannelInput::Save).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::EditChannel {
            room, kind: ChannelKind::Text, settings
        }))] if *room == general && settings.slowmode == 30 && settings.topic == "Say hi"
            && settings.name == "general-chat"
    ));
    app.apply(Event::ChannelSaved(Ok(None)));
    assert!(!screen(&app).saving && !screen(&app).changed());
    input(&mut app, ChannelInput::Close);
    assert!(app.state.channel_settings.is_none());

    // A voice channel and a category have only their names.
    assert!(
        app.dispatch(Action::OpenChannelSettings("discord:10:4".into()))
            .is_empty()
    );
    assert_eq!(screen(&app).kind, ChannelKind::Category);
    assert_eq!(screen(&app).overview.unwrap().0.name, "Talk");
    input(&mut app, ChannelInput::Close);

    app.dispatch(Action::AskChannel(ChannelAsk::Duplicate(general.clone())));
    assert_eq!(
        app.state
            .channel_dialog
            .as_ref()
            .unwrap()
            .settings
            .as_ref()
            .unwrap()
            .name,
        "general"
    );
    assert!(matches!(
        app.dispatch(Action::SaveChannel { kind: ChannelKind::Text, settings: named("general") }).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::CreateChannel {
            copy_of: Some(copy), kind: ChannelKind::Text, ..
        }))] if *copy == general
    ));

    app.dispatch(Action::AskChannel(ChannelAsk::Delete(
        "discord:10:4".into(),
    )));
    assert!(matches!(
        app.dispatch(Action::SaveChannel { kind: ChannelKind::Text, settings: ChannelSettings::default() }).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::DeleteChannel { room }))] if room == "discord:10:4"
    ));
    app.apply(Event::ChannelSaved(Ok(None)));
    assert!(app.state.channel_dialog.is_none());

    // Discord takes no changes while it's away.
    app.apply(discord_event(yapfast_discord::service::Snapshot {
        status: yapfast_discord::service::Status::Connecting,
        ..yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap())
    }));
    assert!(!app.menu_choices("discord:10:11").contains(&EditChannel));
}
