//! Moving between conversations, servers and screens, and back and forward.

use super::*;

#[test]
fn the_requests_screen_stands_in_for_a_conversation() {
    let mut app = connected();
    assert_eq!(app.state.selected.as_deref(), Some("dm"));
    assert!(matches!(
        app.dispatch(Action::OpenDmScreen(DmScreen::Requests))
            .as_slice(),
        [Effect::Backend(Command::CloseRoom)]
    ));
    assert_eq!(app.state.dm_screen, Some(DmScreen::Requests));
    assert_eq!(app.state.selected, None);
    // The lists changing doesn't pick a conversation over it.
    let rooms = owned(&app.state.rooms);
    let servers = owned(&app.state.servers);
    app.apply(Event::Rooms(RoomIndex { rooms, servers }));
    assert_eq!(app.state.dm_screen, Some(DmScreen::Requests));
    assert_eq!(app.state.selected, None);
    // The friends list takes its place, with no conversation to close.
    assert!(
        app.dispatch(Action::OpenDmScreen(DmScreen::Friends))
            .is_empty()
    );
    assert_eq!(app.state.dm_screen, Some(DmScreen::Friends));
    // Choosing a conversation, or a server, closes it.
    app.dispatch(Action::SelectRoom("dm".into()));
    assert_eq!(app.state.dm_screen, None);
    app.dispatch(Action::OpenDmScreen(DmScreen::Requests));
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert_eq!(app.state.dm_screen, None);
    assert!(
        app.dispatch(Action::OpenDmScreen(DmScreen::Requests))
            .is_empty(),
        "only among DMs"
    );
}

#[test]
fn locked_channels_show_without_asking_for_their_messages() {
    let mut app = Application::default();
    app.apply(Event::Connected {
        user: "@alice:test".into(),
        trusted: true,
        storage: "test",
    });
    app.apply(Event::Rooms(RoomIndex {
        rooms: vec![
            preview_room("general", "general", false, 3),
            ChatRoom {
                locked: true,
                ..preview_room("secret", "secret", false, 3)
            },
        ],
        servers: vec![ChatServer {
            id: "friends".into(),
            name: "Friends".into(),
            channels: vec!["general".into(), "secret".into()],
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
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert!(matches!(
        app.dispatch(Action::SelectRoom("secret".into())).as_slice(),
        [Effect::Backend(Command::CloseRoom)]
    ));
    assert!(app.state.room_ready);
    assert!(app.state.notice.is_some());
}

#[test]
fn notifications_name_the_place_and_open_it() {
    let mut app = connected();
    let notify = |app: &mut Application, room: &str| {
        let effects = app.apply(Event::Notify(chat::Notification {
            room: room.into(),
            author: "Sam".into(),
            body: "Thinking co-op".into(),
            avatar: None,
            mention: false,
            replied: None,
        }));
        match effects.as_slice() {
            [Effect::Notify(alert)] => (alert.room.clone(), alert.title.clone()),
            _ => panic!("expected one notification"),
        }
    };
    assert_eq!(
        notify(&mut app, "games"),
        ("games".into(), "Sam in games".into())
    );
    assert_eq!(
        notify(&mut app, "unknown"),
        ("unknown".into(), "Sam".into())
    );
    // Opening a channel from DMs goes to its server first.
    assert_eq!(app.state.selected.as_deref(), Some("dm"));
    app.dispatch(Action::Open("games".into()));
    assert_eq!(app.state.navigation, Navigation::Server("friends".into()));
    assert_eq!(app.state.selected.as_deref(), Some("games"));
    app.dispatch(Action::Open("dm".into()));
    assert_eq!(app.state.navigation, Navigation::DirectMessages);
    assert_eq!(app.state.selected.as_deref(), Some("dm"));
}

/// A reply's notification says whom it answers only once that's asked for;
/// until then, and for a message that isn't a reply, the title is as ever.
#[test]
fn a_replys_title_says_whom_it_replies_to_when_asked() {
    let mut app = connected();
    let title = |app: &mut Application, room: &str, replied: Option<chat::Replied>| {
        let effects = app.apply(Event::Notify(chat::Notification {
            room: room.into(),
            author: "Sam".into(),
            body: "Thinking co-op".into(),
            avatar: None,
            mention: false,
            replied,
        }));
        match effects.as_slice() {
            [Effect::Notify(alert)] => alert.title.clone(),
            _ => panic!("expected one notification"),
        }
    };
    let lena = || Some(chat::Replied::Person("Lena".into()));
    assert!(!app.state.reply_titles, "off until asked for");
    assert_eq!(
        title(&mut app, "games", Some(chat::Replied::You)),
        "Sam in games"
    );
    assert_eq!(title(&mut app, "unknown", lena()), "Sam");
    app.dispatch(Action::ReplyTitles(true));
    assert_eq!(
        title(&mut app, "games", Some(chat::Replied::You)),
        "Sam replied to you in games"
    );
    assert_eq!(title(&mut app, "unknown", lena()), "Sam replied to Lena");
    assert_eq!(title(&mut app, "games", None), "Sam in games");
    app.dispatch(Action::ReplyTitles(false));
    assert_eq!(title(&mut app, "unknown", lena()), "Sam");
}

#[test]
fn navigation_restores_each_scope_selection_and_draft() {
    let mut app = connected();
    app.dispatch(Action::SetDraft("DM draft".into()));
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    app.dispatch(Action::SelectRoom("games".into()));
    app.dispatch(Action::SetDraft("Games draft".into()));
    app.dispatch(Action::Navigate(Navigation::DirectMessages));
    assert_eq!(app.state.selected.as_deref(), Some("dm"));
    assert_eq!(app.state.draft(), "DM draft");
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert_eq!(app.state.selected.as_deref(), Some("games"));
    assert_eq!(app.state.draft(), "Games draft");
    assert!(app.dispatch(Action::SelectRoom("dm".into())).is_empty());
    assert_eq!(app.state.selected.as_deref(), Some("games"));
}

#[test]
fn servers_open_their_last_channel_across_runs_or_the_top_one() {
    let mut app = connected();
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert_eq!(
        app.state.selected.as_deref(),
        Some("general"),
        "top channel"
    );
    app.dispatch(Action::SelectRoom("games".into()));
    let kept = app.last_channels();
    assert_eq!(kept.get("friends").map(String::as_str), Some("games"));

    let mut next = connected();
    next.remember_channels(kept);
    next.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert_eq!(next.state.selected.as_deref(), Some("games"));

    let mut stale = connected();
    stale.remember_channels(HashMap::from([("friends".into(), "gone".into())]));
    stale.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert_eq!(stale.state.selected.as_deref(), Some("general"));
}

#[test]
fn the_quick_switcher_finds_places_as_discords_does() {
    // Matrix rooms, with a Space as the server.
    let mut app = connected();
    app.state.rooms.extend(
        [
            ChatRoom {
                locked: true,
                ..preview_room("staff", "staff", false, 3)
            },
            ChatRoom {
                category: crate::discord::REQUESTS.into(),
                request: true,
                ..preview_room("request", "Ronja", true, 2)
            },
        ]
        .map(Arc::new),
    );
    app.dispatch(Action::Open("dm".into()));
    app.dispatch(Action::Open("games".into()));
    app.dispatch(Action::Open("general".into()));
    let listed = |app: &Application| -> Vec<String> {
        app.state
            .switcher_places()
            .iter()
            .map(|place| place.name().to_owned())
            .collect()
    };
    assert!(listed(&app).is_empty(), "closed");
    app.dispatch(Action::ToggleSwitcher);
    // Those opened lately, but not the one open.
    assert_eq!(listed(&app), ["games", "DM"]);
    // A search finds servers too, those as good by name, but not what this
    // account can't read, nor message requests.
    app.dispatch(Action::SearchSwitcher("s".into()));
    assert_eq!(listed(&app), ["Friends", "games"]);
    app.dispatch(Action::SearchSwitcher("g".into()));
    assert_eq!(listed(&app), ["games", "general", "Lounge"]);
    app.dispatch(Action::SearchSwitcher("ronja".into()));
    assert!(listed(&app).is_empty());
    app.dispatch(Action::SearchSwitcher("#g".into()));
    assert_eq!(listed(&app), ["games", "general"]);
    app.dispatch(Action::SearchSwitcher("!".into()));
    assert_eq!(listed(&app), ["Lounge", "Party"]);
    app.dispatch(Action::SearchSwitcher("*f".into()));
    assert_eq!(listed(&app), ["Friends"]);
    // Going to a place closes it; a server shows its channel shown last.
    app.dispatch(Action::SwitchTo("dm".into()));
    assert!(app.state.switcher.is_none());
    assert_eq!(app.state.navigation, Navigation::DirectMessages);
    assert_eq!(app.state.selected.as_deref(), Some("dm"));
    app.dispatch(Action::ToggleSwitcher);
    app.dispatch(Action::SwitchTo("friends".into()));
    assert_eq!(app.state.navigation, Navigation::Server("friends".into()));
    assert_eq!(app.state.selected.as_deref(), Some("general"));
    app.dispatch(Action::ToggleSwitcher);
    app.dispatch(Action::ToggleSwitcher);
    assert!(app.state.switcher.is_none());
    // A dialog keeps the keyboard.
    app.dispatch(Action::OpenNewDm);
    assert!(app.state.new_dm.is_some());
    app.dispatch(Action::ToggleSwitcher);
    assert!(app.state.switcher.is_none());
}

#[test]
fn room_membership_loss_moves_selection_and_server_loss_returns_to_dms() {
    let mut app = connected();
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    app.dispatch(Action::SelectRoom("games".into()));
    app.dispatch(Action::SetDraft("Keep me".into()));
    let rooms = app
        .state
        .rooms
        .iter()
        .filter(|room| room.id != "games")
        .map(|room| (**room).clone())
        .collect();
    let servers = owned(&app.state.servers);
    app.apply(Event::Rooms(RoomIndex { rooms, servers }));
    assert_eq!(app.state.selected.as_deref(), Some("general"));
    let rooms = owned(&app.state.rooms);
    app.apply(Event::Rooms(RoomIndex {
        rooms,
        servers: vec![],
    }));
    assert_eq!(app.state.navigation, Navigation::DirectMessages);
    assert_eq!(app.state.selected.as_deref(), Some("dm"));
}

#[test]
fn rooms_and_servers_link_and_name_themselves_as_their_networks_do() {
    assert_eq!(
        network_link("discord:10:21"),
        "https://discord.com/channels/10/21"
    );
    assert_eq!(
        network_link("discord:@me:30"),
        "https://discord.com/channels/@me/30"
    );
    assert_eq!(
        network_link("discord:10"),
        "https://discord.com/channels/10"
    );
    assert_eq!(
        network_link("!abc:example.org"),
        "https://matrix.to/#/!abc:example.org?via=example.org"
    );
    assert_eq!(network_link("!abc"), "https://matrix.to/#/!abc");
    assert_eq!(network_id("discord:10:21"), "21");
    assert_eq!(network_id("discord:10"), "10");
    assert_eq!(network_id("!abc:example.org"), "!abc:example.org");
}

#[test]
#[cfg(feature = "discord")]
fn discord_channels_join_unless_locked_and_hidden_ones_show_on_request() {
    use yapfast_discord::{
        ready::{Access, Channel, Server},
        service::{Snapshot, Status},
    };
    let mut app = connected();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        servers: vec![marked(
            Server {
                id: "10".into(),
                name: "Test".into(),
                voice_channels: vec![
                    Channel {
                        id: "11".into(),
                        name: "Lounge".into(),
                        ..Default::default()
                    },
                    Channel {
                        id: "12".into(),
                        name: "Staff".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            &[(
                "12",
                ChannelMarks {
                    access: Access::Hidden,
                    ..Default::default()
                },
            )],
        )],
        ..Default::default()
    }));
    assert!(app.state.servers.iter().any(|server| server.discord));
    assert!(app.state.can_join_voice("discord:10:11"));
    assert!(
        !app.state
            .rooms
            .iter()
            .any(|room| room.id == "discord:10:12")
    );

    app.dispatch(Action::ShowHiddenChannels(true));
    assert!(
        app.state
            .rooms
            .iter()
            .any(|room| room.id == "discord:10:12" && room.locked)
    );
    assert!(!app.state.can_join_voice("discord:10:12"));
    assert!(
        app.dispatch(Action::RequestJoinVoice("discord:10:12".into()))
            .is_empty()
    );
    let effects = app.dispatch(Action::RequestJoinVoice("discord:10:11".into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Voice(voice::Command::Join(id)))] if id == "discord:10:11"
    ));
}

#[test]
fn back_and_forward_walk_the_conversations_visited_without_recording_the_walk() {
    let mut app = connected();
    app.state.selected = None;
    app.dispatch(Action::Open("general".into()));
    app.state.nav_back.clear();
    assert!(!app.state.can_go_back() && !app.state.can_go_forward());
    app.dispatch(Action::Open("games".into()));
    app.dispatch(Action::Open("dm".into()));
    assert_eq!(app.state.selected.as_deref(), Some("dm"));
    assert!(app.state.can_go_back() && !app.state.can_go_forward());

    app.dispatch(Action::NavigateBack);
    assert_eq!(app.state.selected.as_deref(), Some("games"));
    app.dispatch(Action::NavigateBack);
    assert_eq!(app.state.selected.as_deref(), Some("general"));
    assert!(!app.state.can_go_back() && app.state.can_go_forward());
    // Nowhere further back.
    assert!(app.dispatch(Action::NavigateBack).is_empty());
    assert_eq!(app.state.selected.as_deref(), Some("general"));

    app.dispatch(Action::NavigateForward);
    assert_eq!(app.state.selected.as_deref(), Some("games"));
    // Opening something new drops what was forward, as a browser does.
    app.dispatch(Action::Open("lounge".into()));
    assert!(!app.state.can_go_forward());
    app.dispatch(Action::NavigateBack);
    assert_eq!(app.state.selected.as_deref(), Some("games"));
    app.dispatch(Action::NavigateBack);
    assert_eq!(app.state.selected.as_deref(), Some("general"));
}

#[test]
fn back_and_forward_return_to_the_screens_of_the_dm_list() {
    let mut app = connected();
    app.state.nav_back.clear();
    let screen = |app: &Application| app.state.dm_screen;
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    // Opening the screen already open records nothing.
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    app.dispatch(Action::OpenDmScreen(DmScreen::Requests));
    assert_eq!(
        app.state.nav_back,
        [
            HistoryPlace::Room("dm".into()),
            HistoryPlace::Dm(DmScreen::Friends)
        ]
    );
    assert!(app.state.can_go_back() && !app.state.can_go_forward());

    app.dispatch(Action::NavigateBack);
    assert_eq!(screen(&app), Some(DmScreen::Friends));
    assert_eq!(app.state.selected, None);
    assert!(app.state.can_go_back() && app.state.can_go_forward());
    app.dispatch(Action::NavigateBack);
    assert_eq!(screen(&app), None);
    assert_eq!(app.state.selected.as_deref(), Some("dm"));
    assert!(!app.state.can_go_back() && app.state.can_go_forward());

    // Forward goes through the screens again, and the walk records nothing.
    app.dispatch(Action::NavigateForward);
    assert_eq!(screen(&app), Some(DmScreen::Friends));
    app.dispatch(Action::NavigateForward);
    assert_eq!(screen(&app), Some(DmScreen::Requests));
    assert!(!app.state.can_go_forward());
    assert!(app.state.nav_forward.is_empty());
    assert_eq!(app.state.nav_back.len(), 2);

    // A new visit drops what was forward.
    app.dispatch(Action::NavigateBack);
    app.dispatch(Action::SelectRoom("dm".into()));
    assert!(!app.state.can_go_forward());
    app.dispatch(Action::NavigateBack);
    assert_eq!(screen(&app), Some(DmScreen::Friends));
}

#[test]
fn back_from_a_server_returns_to_the_friends_list_among_the_direct_messages() {
    let mut app = connected();
    app.state.nav_back.clear();
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert_eq!(app.state.dm_screen, None);
    assert!(app.state.selected.is_some());
    app.dispatch(Action::NavigateBack);
    assert_eq!(app.state.navigation, Navigation::DirectMessages);
    assert_eq!(app.state.dm_screen, Some(DmScreen::Friends));
    assert_eq!(app.state.selected, None);
    app.dispatch(Action::NavigateForward);
    assert_eq!(app.state.navigation, Navigation::Server("friends".into()));
    assert_eq!(app.state.dm_screen, None);
}

#[test]
fn a_place_that_is_gone_is_skipped_and_the_history_keeps_fifty() {
    let mut app = connected();
    app.state.nav_back.clear();
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    app.dispatch(Action::SelectRoom("dm".into()));
    app.dispatch(Action::Open("games".into()));
    // Behind "games" are "dm", the friends list and "dm" again; with "dm"
    // gone the arrow still has the screen to go to.
    app.state.rooms.retain(|room| room.id != "dm");
    assert!(app.state.can_go_back());
    app.dispatch(Action::NavigateBack);
    assert_eq!(app.state.dm_screen, Some(DmScreen::Friends));
    assert!(!app.state.can_go_back());

    let mut app = connected();
    app.state.nav_back.clear();
    for turn in 0..80 {
        let screen = if turn % 2 == 0 {
            DmScreen::Friends
        } else {
            DmScreen::Requests
        };
        app.dispatch(Action::OpenDmScreen(screen));
        app.dispatch(Action::SelectRoom("dm".into()));
    }
    assert_eq!(app.state.nav_back.len(), 50);
}

#[test]
#[cfg(feature = "discord")]
fn back_and_forward_return_to_browse_channels() {
    let mut app = discord_server();
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    change_server(&mut app, |server| server.community = true);
    app.dispatch(Action::SelectRoom("discord:10:11".into()));
    app.state.nav_back.clear();
    app.dispatch(Action::BrowseChannels);
    assert!(app.state.browse.is_some());
    app.dispatch(Action::SelectRoom("discord:10:13".into()));
    assert!(app.state.browse.is_none());
    assert_eq!(
        app.state.nav_back,
        [
            HistoryPlace::Room("discord:10:11".into()),
            HistoryPlace::Browse("discord:10".into())
        ]
    );

    app.dispatch(Action::NavigateBack);
    let browse = app.state.browse.as_ref().expect("Browse Channels");
    assert_eq!(browse.server, "discord:10");
    assert_eq!(app.state.selected, None);
    assert!(app.state.can_go_back() && app.state.can_go_forward());
    app.dispatch(Action::NavigateBack);
    assert!(app.state.browse.is_none());
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:11"));
    assert!(!app.state.can_go_back() && app.state.can_go_forward());
    app.dispatch(Action::NavigateForward);
    assert!(app.state.browse.is_some());
    app.dispatch(Action::NavigateForward);
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:13"));
    assert!(!app.state.can_go_forward());

    // A server that stops offering it takes it out of the arrows' reach.
    change_server(&mut app, |server| server.community = false);
    assert!(app.state.can_go_back(), "the first channel is still there");
    app.dispatch(Action::NavigateBack);
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:11"));
    assert!(!app.state.can_go_back());
}

#[test]
fn the_quick_switcher_lists_the_mentions_not_listed_yet_before_a_search() {
    let mut app = connected();
    app.state.rooms.extend(
        [
            ChatRoom {
                mentions: 2,
                ..preview_room("ping", "ping", false, 3)
            },
            ChatRoom {
                mentions: 5,
                ..preview_room("loud", "loud", false, 3)
            },
            ChatRoom {
                mentions: 9,
                locked: true,
                ..preview_room("staff", "staff", false, 3)
            },
        ]
        .map(Arc::new),
    );
    app.dispatch(Action::Open("games".into()));
    app.dispatch(Action::Open("general".into()));
    let mentioned = |app: &Application| -> Vec<String> {
        app.state
            .switcher_mentions()
            .iter()
            .map(|room| room.name.clone())
            .collect()
    };
    assert!(mentioned(&app).is_empty(), "closed");
    app.dispatch(Action::ToggleSwitcher);
    assert_eq!(mentioned(&app), ["loud", "ping"], "most mentions first");
    // One opened lately is under Previous Channels already.
    app.dispatch(Action::ToggleSwitcher);
    app.state.places_used.visit("ping", 1);
    app.dispatch(Action::ToggleSwitcher);
    assert_eq!(mentioned(&app), ["loud"]);
    // A search lists none.
    app.dispatch(Action::SearchSwitcher("l".into()));
    assert!(mentioned(&app).is_empty());
}
