use super::{preview::preview_room, *};

#[cfg(feature = "discord")]
fn discord_event(snapshot: yapfast_discord::service::Snapshot) -> Event {
    Event::Discord(Arc::new(snapshot))
}
use crate::{chat::RoomIndex, sounds::Sound};
#[cfg(feature = "discord")]
use yapfast_discord::marked::{ChannelMarks, MarkedServer, ServerMarks};

/// The sounds among effects, in order.
/// Copies of the rooms or servers a list shares, as a network sends them.
fn owned<T: Clone>(list: &[Arc<T>]) -> Vec<T> {
    list.iter().map(|item| (**item).clone()).collect()
}

fn sounds(effects: &[Effect]) -> Vec<Sound> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Sound(sound) => Some(*sound),
            _ => None,
        })
        .collect()
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
        }],
    }));
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![]),
    });
    app
}

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

fn send_command(effects: Vec<Effect>) -> Command {
    match effects.into_iter().next().expect("send effect") {
        Effect::Backend(command @ Command::Send { .. }) => command,
        _ => panic!("expected send command"),
    }
}

#[test]
fn typing_shows_for_the_open_conversation() {
    let mut app = connected();
    let typing = |app: &mut Application, room: &str, names: &[&str]| {
        app.apply(Event::Typing {
            room: room.into(),
            names: names.iter().map(|name| (*name).to_owned()).collect(),
        });
        app.state.typing_line()
    };
    assert_eq!(typing(&mut app, "dm", &["Sam"]), "Sam is typing...");
    assert_eq!(
        typing(&mut app, "dm", &["Sam", "Riley"]),
        "Sam and Riley are typing..."
    );
    assert_eq!(
        typing(&mut app, "dm", &["A", "B", "C", "D"]),
        "Several people are typing..."
    );
    // Typing elsewhere doesn't show here.
    assert_eq!(
        typing(&mut app, "games", &["Sam"]),
        "Several people are typing..."
    );
    assert_eq!(typing(&mut app, "dm", &[]), "");
    // Clearing the draft says this account stopped.
    app.dispatch(Action::SetDraft("Hi".into()));
    assert!(matches!(
        app.dispatch(Action::SetDraft(String::new())).as_slice(),
        [Effect::Backend(Command::Typing { typing: false, .. }), ..]
    ));
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
fn members_belong_to_the_selected_text_room() {
    let mut app = connected();
    assert!(!app.state.lists_members(), "one-to-one DMs list no members");
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert!(app.state.lists_members() && app.state.members.is_none());
    let member = ChatMember {
        id: "@bob:test".into(),
        name: "Bob".into(),
        username: None,
        role: Role::Member,
        avatar: None,
        group: None,
        presence: None,
        color: None,
        activity: None,
        activities: Default::default(),
        roles: Default::default(),
        timed_out_until: None,
    };
    app.apply(Event::Members {
        room: "games".into(),
        members: Ok(vec![member.clone()]),
    });
    assert!(
        app.state.members.is_none(),
        "another room's members are stale"
    );
    app.apply(Event::Members {
        room: "general".into(),
        members: Ok(vec![member]),
    });
    assert!(matches!(&app.state.members, Some(Ok(members)) if members.len() == 1));
    let late = "Discord didn't send this channel's member list.";
    app.apply(Event::Members {
        room: "general".into(),
        members: Err(late),
    });
    assert!(
        matches!(&app.state.members, Some(Ok(members)) if members.len() == 1),
        "the list that came stays"
    );
    app.dispatch(Action::SelectRoom("games".into()));
    assert!(app.state.members.is_none());
    app.apply(Event::Members {
        room: "games".into(),
        members: Err(late),
    });
    assert_eq!(app.state.members, Some(Err(late)), "loading ends");
    app.dispatch(Action::SelectRoom("lounge".into()));
    assert!(!app.state.lists_members());

    let mut preview = Application::default();
    preview.dispatch(Action::Preview);
    let Some(Ok(members)) = &preview.state.members else {
        panic!("the preview lists sample members");
    };
    assert_eq!(members[0].role, Role::Admin);
    assert_eq!(
        members.len() as u64,
        preview.state.selected_room().unwrap().members
    );
}

#[test]
fn frequent_reactions_follow_use_and_fill_with_defaults() {
    let mut app = Application::default();
    app.dispatch(Action::Preview);
    assert_eq!(app.frequent_reactions(3), ["👍", "❤️", "😂"]);
    let item = app.state.messages[0].item.clone();
    for key in ["🔥", "🔥", "👀"] {
        app.dispatch(Action::React {
            item: item.clone(),
            key: key.into(),
        });
        // Removing a reaction is not a use.
        if key == "🔥" {
            app.dispatch(Action::React {
                item: item.clone(),
                key: key.into(),
            });
        }
    }
    assert_eq!(app.frequent_reactions(4), ["🔥", "👀", "👍", "❤️"]);
    app.dispatch(Action::Logout);
    assert_eq!(
        app.frequent_reactions(1),
        ["🔥"],
        "counts outlive the session"
    );
    let mut next = Application::default();
    next.remember_reaction_uses(app.reaction_uses().clone());
    assert_eq!(next.frequent_reactions(2), ["🔥", "👀"]);
}

#[test]
fn forwarding_sends_a_message_to_the_conversations_chosen() {
    let mut app = Application::default();
    app.dispatch(Action::Preview);
    let item = app.state.messages[1].item.clone();
    let body = app.state.messages[1].body.clone();
    app.dispatch(Action::StartForward(item.clone()));
    // Choosing again leaves it out; a voice channel can't be chosen.
    app.dispatch(Action::ChooseForward("games".into()));
    app.dispatch(Action::ChooseForward("general".into()));
    app.dispatch(Action::ChooseForward("general".into()));
    app.dispatch(Action::ChooseForward("lounge".into()));
    assert_eq!(app.state.forwarding.as_ref().unwrap().chosen, ["games"]);
    // With one chosen, the dialog closes and the conversation opens, with
    // the forward and the message added after it.
    app.dispatch(Action::Forward("  for you ".into()));
    assert!(app.state.forwarding.is_none());
    assert_eq!(app.state.selected.as_deref(), Some("games"));
    let [.., forward, note] = app.state.messages.as_slice() else {
        panic!("no forward");
    };
    assert!(forward.forwarded && forward.body == body);
    assert_eq!(note.body, "for you");
    assert_eq!(app.state.notice.as_deref(), Some("Message forwarded."));

    let mut app = connected();
    let mut sent = message("e:$bob", false);
    sent.can_forward = true;
    sent.body = "Hello there".into();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![sent]),
    });
    app.dispatch(Action::StartForward("e:$bob".into()));
    assert_eq!(app.state.forwarding.as_ref().unwrap().quote, "Hello there");
    // Up to five, within the message's network, and nothing goes until Send.
    let general = (*app.state.rooms[1]).clone();
    for id in ["c1", "c2", "c3", "c4", "c5"] {
        app.state.rooms.push(Arc::new(ChatRoom {
            id: id.into(),
            ..general.clone()
        }));
    }
    app.state.rooms.push(Arc::new(ChatRoom {
        id: crate::discord::room_id("10", "11"),
        network: Network::Discord,
        ..general.clone()
    }));
    assert!(
        !app.state
            .can_forward_to("dm", app.state.rooms.last().unwrap())
    );
    for id in ["general", "c1", "c2", "c3", "c4", "c5"] {
        assert!(app.dispatch(Action::ChooseForward(id.into())).is_empty());
    }
    assert_eq!(
        app.state.forwarding.as_ref().unwrap().chosen,
        ["general", "c1", "c2", "c3", "c4"]
    );
    for id in ["c2", "c3", "c4"] {
        app.dispatch(Action::ChooseForward(id.into()));
    }
    let effects = app.dispatch(Action::Forward("look".into()));
    let targets: Vec<_> = effects
        .iter()
        .map(|effect| match effect {
            Effect::Backend(Command::Forward {
                room,
                item,
                target,
                note,
                silent: false,
            }) if room == "dm" && item == "e:$bob" && note.as_deref() == Some("look") => {
                target.as_str()
            }
            _ => panic!("not a forward"),
        })
        .collect();
    assert_eq!(targets, ["general", "c1"]);
    assert_eq!(app.state.selected.as_deref(), Some("dm"), "two stay closed");
    // Once both answer, the dialog opens again for where it failed, with
    // the message added and why.
    app.apply(Event::Forwarded {
        room: "general".into(),
        result: Err(ForwardFailure::Forward("refused".into())),
    });
    assert!(app.state.forwarding.is_none());
    app.apply(Event::Forwarded {
        room: "c1".into(),
        result: Ok(()),
    });
    let forwarding = app.state.forwarding.as_ref().unwrap();
    assert_eq!(forwarding.chosen, ["general"]);
    assert_eq!(forwarding.note, "look");
    assert_eq!(
        forwarding.error.as_deref(),
        Some("The message couldn't be forwarded to #general: refused")
    );
    // The message added falling short says so, and doesn't forward again.
    let effects = app.dispatch(Action::Forward(String::new()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Forward { note: None, .. }), ..]
    ));
    assert_eq!(app.state.selected.as_deref(), Some("general"));
    app.apply(Event::Forwarded {
        room: "general".into(),
        result: Err(ForwardFailure::Note("too fast".into())),
    });
    assert!(app.state.forwarding.is_none());
    assert_eq!(
        app.state.notice.as_deref(),
        Some("Forwarded, but the message you added didn't go to #general: too fast")
    );
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert!(app.state.forwarding.is_none());
}

#[test]
fn discord_forwards_skip_the_note_in_slowmode_and_take_silent() {
    let mut app = connected();
    let channel = |id: &str, slowed| ChatRoom {
        id: crate::discord::room_id("10", id),
        network: Network::Discord,
        capabilities: Network::Discord.capabilities(),
        slowed,
        ..preview_room("x", id, false, 3)
    };
    app.state.rooms = [
        channel("11", false),
        channel("12", true),
        channel("13", false),
    ]
    .map(Arc::new)
    .into();
    app.state.discord_online = true;
    app.state.selected = Some(app.state.rooms[0].id.clone());
    let mut sent = message("d:5", false);
    sent.can_forward = true;
    app.state.messages = Arc::new(timeline(vec![sent]));
    // A Discord message forwards without Matrix's verified device.
    app.state.trusted = false;
    app.dispatch(Action::StartForward("d:5".into()));
    app.dispatch(Action::ChooseForward(app.state.rooms[1].id.clone()));
    app.dispatch(Action::ChooseForward(app.state.rooms[2].id.clone()));
    let notes: Vec<_> = app
        .dispatch(Action::Forward("@silent see this".into()))
        .into_iter()
        .map(|effect| match effect {
            Effect::Backend(Command::Forward {
                note, silent: true, ..
            }) => note,
            _ => panic!("not a forward"),
        })
        .collect();
    assert_eq!(notes, [None, Some("see this".into())]);
}

#[test]
fn discord_forwards_list_places_as_discords_dialog_does() {
    let mut app = connected();
    let place = |guild: &str, id: &str, name: &str| ChatRoom {
        id: crate::discord::room_id(guild, id),
        name: name.into(),
        network: Network::Discord,
        capabilities: Network::Discord.capabilities(),
        direct: guild == "@me",
        ..preview_room("x", id, false, 3)
    };
    app.state.rooms = [
        ChatRoom {
            person: Some("1".into()),
            ..place("@me", "20", "Bob")
        },
        place("@me", "21", "weekend plans"),
        place("10", "11", "general"),
        place("10", "12", "games"),
        place("10", "13", "game-night"),
        place("10", "14", "chat"),
    ]
    .map(Arc::new)
    .into();
    let id = |channel| crate::discord::room_id("10", channel);
    let (games, chat) = (id("12"), id("14"));
    app.state.discord_online = true;
    app.state.places_used.visit(&games, crate::clock::now_ms());
    app.state.places_used.visit(&chat, crate::clock::now_ms());
    app.state.selected = Some(chat);
    let mut sent = message("d:5", false);
    sent.can_forward = true;
    app.state.messages = Arc::new(timeline(vec![sent]));
    let effects = app.dispatch(Action::StartForward("d:5".into()));
    assert!(matches!(
        effects[..],
        [Effect::Backend(Command::Discord(
            DiscordCommand::PlacesUsed
        ))]
    ));
    let now = crate::clock::now_ms();
    let used = |total, days: &[u64]| crate::chat::PlaceUses {
        total,
        recent: days.iter().map(|days| now - days * 86_400_000).collect(),
    };
    app.apply(Event::PlacesUsed(Ok(vec![
        ("11".into(), used(50, &[0, 0, 0, 0, 0])),
        ("20".into(), used(1, &[0])),
        ("13".into(), used(2, &[30])),
    ])));
    let listed = |app: &Application| -> Vec<String> {
        app.state
            .forward_places()
            .iter()
            .map(|room| room.name.clone())
            .collect()
    };
    // Those opened lately, then those used most; not the message's own.
    assert_eq!(listed(&app), ["games", "general", "Bob", "game-night"]);
    app.dispatch(Action::SearchForward("night".into()));
    assert_eq!(listed(&app), ["game-night"]);
    // Choosing clears the search, and lists those chosen first.
    app.dispatch(Action::ChooseForward(id("13")));
    assert_eq!(app.state.forwarding.as_ref().unwrap().query, "");
    assert_eq!(listed(&app), ["game-night", "games", "general", "Bob"]);
    // A search ranks by the name, then by how much a place is used, which
    // opening it here counts too.
    app.dispatch(Action::SearchForward("g".into()));
    assert_eq!(listed(&app), ["general", "games", "game-night"]);
    app.dispatch(Action::SearchForward("@b".into()));
    assert_eq!(listed(&app), ["Bob"]);
    app.dispatch(Action::SearchForward("#e".into()));
    assert_eq!(listed(&app), ["general", "games", "game-night"]);
    app.dispatch(Action::SearchForward("e".into()));
    assert_eq!(
        listed(&app),
        ["general", "games", "game-night", "weekend plans"]
    );
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
fn discord_forwards_list_eight_places_opened_lately_that_can_take_them() {
    let mut app = connected();
    let channel = |id: u32| ChatRoom {
        id: crate::discord::room_id("10", &id.to_string()),
        name: format!("c{id}"),
        network: Network::Discord,
        capabilities: Network::Discord.capabilities(),
        ..preview_room("x", "x", false, 3)
    };
    app.state.rooms = (11..=20).map(channel).map(Arc::new).collect();
    app.state.discord_online = true;
    // Matrix rooms opened in between take room among those kept, but not
    // among those listed.
    for id in 11..=20 {
        let room = crate::discord::room_id("10", &id.to_string());
        app.state.places_used.visit(&room, crate::clock::now_ms());
        app.state
            .places_used
            .visit(&format!("!m{id}:test"), crate::clock::now_ms());
    }
    app.state.selected = Some(crate::discord::room_id("10", "20"));
    let mut sent = message("d:5", false);
    sent.can_forward = true;
    app.state.messages = Arc::new(timeline(vec![sent]));
    app.dispatch(Action::StartForward("d:5".into()));
    let listed: Vec<String> = app
        .state
        .forward_places()
        .iter()
        .map(|room| room.name.clone())
        .collect();
    // Those opened lately but the message's own, then the others used.
    assert_eq!(
        listed,
        [
            "c19", "c18", "c17", "c16", "c15", "c14", "c13", "c11", "c12"
        ]
    );
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

#[test]
fn failed_sends_keep_the_draft_and_newer_drafts_survive_confirmation() {
    let mut app = connected();
    assert!(matches!(
        app.dispatch(Action::SetDraft("First".into())).as_slice(),
        [
            Effect::Backend(Command::Typing { room: typing, typing: true }),
            Effect::Backend(Command::SaveDraft { room, body }),
        ] if typing == "dm" && room == "dm" && body == "First"
    ));
    assert!(app.dispatch(Action::SetDraft("First".into())).is_empty());
    assert!(matches!(
        send_command(app.dispatch(Action::Send)),
        Command::Send { reply: None, .. }
    ));
    assert!(app.dispatch(Action::Send).is_empty(), "one send at a time");
    app.apply(Event::SendResult {
        room: "dm".into(),
        body: "First".into(),
        result: Err("offline"),
    });
    assert_eq!(app.state.draft(), "First");
    assert_eq!(app.state.notice.as_deref(), Some("offline"));
    send_command(app.dispatch(Action::Send));
    app.dispatch(Action::SetDraft("Next message".into()));
    app.apply(Event::SendResult {
        room: "dm".into(),
        body: "First".into(),
        result: Ok(()),
    });
    assert_eq!(app.state.draft(), "Next message");
    assert!(app.state.sending.is_none());
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    app.dispatch(Action::SetDraft("Other room draft".into()));
    app.apply(Event::SendResult {
        room: "dm".into(),
        body: "Next message".into(),
        result: Ok(()),
    });
    assert_eq!(app.state.draft(), "Other room draft");
}

#[test]
fn up_in_an_empty_composer_edits_the_newest_own_message() {
    let mut app = connected();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![
            message("e:$old", true),
            message("e:$mine", true),
            message("e:$bob", false),
        ]),
    });
    app.dispatch(Action::EditLast);
    assert!(matches!(app.state.compose(), Some(Compose::Edit { item, .. }) if item == "e:$mine"));
    assert_eq!(app.state.draft(), "Body of e:$mine");
}

#[test]
fn blocked_messages_can_leave_the_timeline_and_come_back() {
    let mut app = connected();
    let blocked = ChatMessage {
        blocked: true,
        ..message("e:$troll", false)
    };
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![message("e:$bob", false), blocked]),
    });
    let items = |app: &Application| -> Vec<String> {
        app.state
            .messages
            .iter()
            .map(|message| message.item.clone())
            .collect()
    };
    assert_eq!(items(&app), ["e:$bob", "e:$troll"], "collapsed, not hidden");
    app.dispatch(Action::HideBlocked(true));
    assert_eq!(items(&app), ["e:$bob"]);
    app.dispatch(Action::HideBlocked(false));
    assert_eq!(items(&app), ["e:$bob", "e:$troll"]);
}

#[test]
fn a_reply_mentions_its_author_unless_switched_off() {
    let mut app = connected();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![message("e:$bob", false)]),
    });
    app.dispatch(Action::ReplyPing(true));
    app.dispatch(Action::Reply("e:$bob".into()));
    assert!(matches!(
        app.state.compose(),
        Some(Compose::Reply { ping: true, .. })
    ));
    app.dispatch(Action::ToggleReplyPing { aside: false });
    app.dispatch(Action::SetDraft("@silent hi".into()));
    let Command::Send { body, options, .. } = send_command(app.dispatch(Action::Send)) else {
        unreachable!()
    };
    assert_eq!(body, "@silent hi", "Matrix has no silent messages");
    assert_eq!(
        options,
        crate::chat::SendOptions {
            quiet_reply: true,
            silent: false
        }
    );
}

#[test]
fn replies_edits_and_saved_drafts_share_one_composer() {
    let mut app = connected();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![message("e:$bob", false), message("e:$mine", true)]),
    });
    app.apply(Event::Draft {
        room: "dm".into(),
        body: "Saved earlier".into(),
    });
    assert_eq!(app.state.draft(), "Saved earlier");
    app.dispatch(Action::SetDraft("Typed now".into()));
    app.apply(Event::Draft {
        room: "dm".into(),
        body: "Stale".into(),
    });
    assert_eq!(app.state.draft(), "Typed now");

    assert!(app.dispatch(Action::Edit("e:$bob".into())).is_empty());
    assert!(
        app.state.compose().is_none(),
        "only own messages are editable"
    );
    app.dispatch(Action::Reply("e:$bob".into()));
    assert!(matches!(app.state.compose(), Some(Compose::Reply { author, .. }) if author == "Bob"));
    assert!(matches!(
        send_command(app.dispatch(Action::Send)),
        Command::Send { reply: Some(item), body, .. } if item == "e:$bob" && body == "Typed now"
    ));
    app.apply(Event::SendResult {
        room: "dm".into(),
        body: "Typed now".into(),
        result: Ok(()),
    });
    assert!(app.state.compose().is_none() && app.state.draft().is_empty());

    app.dispatch(Action::SetDraft("Unsent".into()));
    app.dispatch(Action::Edit("e:$mine".into()));
    assert_eq!(app.state.draft(), "Body of e:$mine");
    assert!(
        app.dispatch(Action::SetDraft("Corrected".into()))
            .is_empty(),
        "edits are not saved as drafts"
    );
    let effects = app.dispatch(Action::Send);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Edit { item, body, .. })] if item == "e:$mine" && body == "Corrected"
    ));
    app.apply(Event::EditResult {
        room: "dm".into(),
        item: "e:$mine".into(),
        result: Err("refused".into()),
    });
    assert_eq!(app.state.draft(), "Corrected");
    app.dispatch(Action::Send);
    app.apply(Event::EditResult {
        room: "dm".into(),
        item: "e:$mine".into(),
        result: Ok(()),
    });
    assert_eq!(app.state.draft(), "Unsent");
    app.dispatch(Action::Edit("e:$mine".into()));
    app.dispatch(Action::CancelCompose);
    assert_eq!(app.state.draft(), "Unsent");
}

#[test]
fn pinning_a_message_takes_leave_and_unpins_a_pinned_one() {
    let mut app = connected();
    let mut pinned = message("e:$pinned", false);
    pinned.can_pin = true;
    pinned.pinned = true;
    let mut pinnable = message("e:$pinnable", false);
    pinnable.can_pin = true;
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![message("e:$bob", false), pinnable, pinned]),
    });
    assert!(app.dispatch(Action::PinMessage("e:$bob".into())).is_empty());
    assert!(matches!(
        app.dispatch(Action::PinMessage("e:$pinnable".into())).as_slice(),
        [Effect::Backend(Command::Pin { room, item, pinned: true })]
            if room == "dm" && item == "e:$pinnable"
    ));
    assert!(matches!(
        app.dispatch(Action::PinMessage("e:$pinned".into()))
            .as_slice(),
        [Effect::Backend(Command::Pin { pinned: false, .. })]
    ));
}

#[test]
fn deletion_needs_confirmation_and_queue_actions_need_a_pending_message() {
    let mut app = connected();
    let mut pending = message("t:txn", true);
    pending.delivery = chat::Delivery::Failed;
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![
            message("e:$bob", false),
            message("e:$mine", true),
            pending,
        ]),
    });
    app.dispatch(Action::RequestDelete("e:$bob".into()));
    assert!(app.state.confirm_delete.is_none());
    app.dispatch(Action::RequestDelete("e:$mine".into()));
    app.dispatch(Action::CancelDelete);
    assert!(app.dispatch(Action::ConfirmDelete).is_empty());
    app.dispatch(Action::RequestDelete("e:$mine".into()));
    assert!(matches!(
        app.dispatch(Action::ConfirmDelete).as_slice(),
        [Effect::Backend(Command::Delete { item, .. })] if item == "e:$mine"
    ));
    assert!(app.dispatch(Action::Retry("e:$mine".into())).is_empty());
    assert!(matches!(
        app.dispatch(Action::Retry("t:txn".into())).as_slice(),
        [Effect::Backend(Command::Retry { .. })]
    ));
    assert!(matches!(
        app.dispatch(Action::CancelSend("t:txn".into())).as_slice(),
        [Effect::Backend(Command::Cancel { .. })]
    ));
    assert!(matches!(
        app.dispatch(Action::React { item: "e:$bob".into(), key: "👍".into() }).as_slice(),
        [Effect::Backend(Command::React { key, .. })] if key == "👍"
    ));
    app.state.trusted = false;
    assert!(
        app.dispatch(Action::React {
            item: "e:$bob".into(),
            key: "👍".into()
        })
        .is_empty()
    );
    assert!(!app.state.can_attach());
}

#[test]
fn files_wait_in_the_composer_until_sent() {
    let folder = tempfile::tempdir().unwrap();
    let file = |name: &str, size: u64| {
        let path = folder.path().join(name);
        std::fs::File::create(&path).unwrap().set_len(size).unwrap();
        path
    };
    let (picture, notes) = (file("cat.png", 3), file("notes.txt", 2048));
    let big = file("big.mov", 51 * 1024 * 1024);
    let mut app = connected();
    assert!(!app.state.can_send(), "nothing to send yet");
    app.dispatch(Action::Stage(vec![picture.clone(), big, notes.clone()]));
    let staged: Vec<_> = app
        .state
        .staged()
        .iter()
        .map(|file| (file.name.as_str(), file.image))
        .collect();
    assert_eq!(staged, [("cat.png", true), ("notes.txt", false)]);
    assert_eq!(
        app.state.notice.as_deref(),
        Some("big.mov is 51 MB; files here can be up to 50 MB.")
    );
    app.dispatch(Action::Unstage(1));
    app.dispatch(Action::Stage(vec![notes.clone()]));
    assert!(app.state.can_send(), "files alone make a message");
    let Command::Send { body, files, .. } = send_command(app.dispatch(Action::Send)) else {
        unreachable!()
    };
    assert_eq!(
        (body.as_str(), files.clone()),
        ("", vec![picture.clone().into(), notes.clone().into()])
    );
    app.apply(Event::SendResult {
        room: "dm".into(),
        body,
        result: Ok(()),
    });
    assert!(
        app.state.staged().is_empty(),
        "sent files leave the composer"
    );
    // Only a network with spoilers marks files as them.
    app.dispatch(Action::Stage(vec![picture.clone(), notes.clone()]));
    app.dispatch(Action::ToggleSpoiler(0));
    assert!(
        !app.state.staged()[0].spoiler,
        "Matrix has no spoiler files"
    );
    let room = app
        .state
        .rooms
        .iter_mut()
        .map(Arc::make_mut)
        .find(|room| room.id == "dm")
        .unwrap();
    room.capabilities = Network::Discord.capabilities();
    app.dispatch(Action::ToggleSpoiler(1));
    app.dispatch(Action::ToggleSpoiler(1));
    app.dispatch(Action::ToggleSpoiler(0));
    let Command::Send { files, .. } = send_command(app.dispatch(Action::Send)) else {
        unreachable!()
    };
    let spoilers: Vec<_> = files.iter().map(|file| file.spoiler).collect();
    assert_eq!(spoilers, [true, false]);
    app.apply(Event::SendResult {
        room: "dm".into(),
        body: String::new(),
        result: Ok(()),
    });
    app.dispatch(Action::Stage(vec![picture.clone(); chat::MAX_FILES + 1]));
    assert_eq!(app.state.staged().len(), chat::MAX_FILES);
    assert_eq!(
        app.state.notice.as_deref(),
        Some("A message carries up to 10 files.")
    );
    assert!(!app.state.can_attach());
    app.state.notice = None;
    app.dispatch(Action::Stage(vec![picture]));
    assert_eq!(
        app.state.notice.as_deref(),
        Some("A message carries up to 10 files."),
        "a paste into a full message says why it's refused"
    );
}

#[test]
fn a_drop_is_refused_whole_as_discords_client_refuses_it() {
    let folder = tempfile::tempdir().unwrap();
    let file = |name: &str, size: u64| {
        let path = folder.path().join(name);
        std::fs::File::create(&path).unwrap().set_len(size).unwrap();
        path
    };
    let (picture, big) = (file("cat.png", 3), file("big.mov", 51 * 1024 * 1024));
    let mut app = connected();
    assert_eq!(app.state.drop_refusal(std::slice::from_ref(&picture)), None);
    let refusal = app.state.drop_refusal(&[picture.clone(), big]).unwrap();
    assert_eq!(
        (refusal.title(), refusal.help()),
        (
            "Your files are too powerful",
            "Max file size is 50.00 MB please.".into()
        )
    );
    let refusal = app
        .state
        .drop_refusal(&[picture.clone(), folder.path().into()])
        .unwrap();
    assert_eq!(refusal, chat::Refusal::Invalid, "a folder isn't a file");
    assert_eq!(app.state.drop_refusal(&[]), Some(chat::Refusal::Invalid));
    // The files already waiting count.
    app.dispatch(Action::Stage(vec![picture.clone(); 4]));
    assert_eq!(app.state.drop_refusal(&vec![picture.clone(); 6]), None);
    let refusal = app.state.drop_refusal(&vec![picture; 7]).unwrap();
    assert_eq!(
        (refusal.title(), refusal.help()),
        (
            "Too many uploads!",
            "You can only upload 10 files at a time!".into()
        )
    );
}

#[test]
fn uploads_name_their_destination_as_discord_does() {
    let mut app = connected();
    fn room(app: &mut Application) -> &mut ChatRoom {
        let id = app.state.selected.clone().unwrap();
        app.state
            .rooms
            .iter_mut()
            .map(Arc::make_mut)
            .find(|room| room.id == id)
            .unwrap()
    }
    room(&mut app).name = "Sam".into();
    room(&mut app).direct = true;
    room(&mut app).members = 2;
    room(&mut app).person = Some("sam".into());
    assert_eq!(app.state.upload_destination(), "@Sam");
    room(&mut app).members = 3;
    assert_eq!(app.state.upload_destination(), "Sam", "a group DM");
    room(&mut app).direct = false;
    assert_eq!(app.state.upload_destination(), "#Sam");
    room(&mut app).parent = Some("general".into());
    assert_eq!(app.state.upload_destination(), "\"Sam\"", "a thread");
}

#[test]
fn stopped_transport_unblocks_retry_and_failed_logout_keeps_session() {
    let mut app = connected();
    app.dispatch(Action::SetDraft("Retry me".into()));
    let first = send_command(app.dispatch(Action::Send));
    app.backend_failed(&first);
    assert!(app.state.sending.is_none());
    assert_eq!(app.state.draft(), "Retry me");
    let retry = send_command(app.dispatch(Action::Send));
    app.backend_failed(&retry);
    assert!(matches!(
        app.dispatch(Action::Logout).as_slice(),
        [Effect::Backend(Command::Logout)]
    ));
    app.backend_failed(&Command::Logout);
    assert!(!app.state.busy);
    assert_eq!(app.state.account.as_deref(), Some("@alice:test"));
    app.dispatch(Action::Logout);
    app.apply(Event::Error(
        "Sign-out failed. Your session is kept.".into(),
    ));
    assert_eq!(app.state.account.as_deref(), Some("@alice:test"));
    app.dispatch(Action::Logout);
    app.apply(Event::LoggedOut);
    assert!(app.state.account.is_none());
    assert!(app.state.rooms.is_empty());
    assert!(app.state.draft().is_empty());
    assert_eq!(app.state.reset_version, 1);
}

#[test]
fn jumping_shows_a_loaded_message_or_a_window_of_the_messages_around_it() {
    let mut app = connected();
    let message = |item: &str| ChatMessage {
        id: item.into(),
        item: item.into(),
        body: item.into(),
        ..Default::default()
    };
    let show = |app: &mut Application, items: &[&str]| {
        app.apply(Event::Timeline {
            room: "dm".into(),
            messages: timeline(items.iter().map(|item| message(item)).collect()),
        })
    };
    show(&mut app, &["e:20", "e:30"]);
    assert!(app.dispatch(Action::Jump("e:30".into())).is_empty());
    assert_eq!(app.state.jump, Some(("e:30".into(), 0)));

    // One the timeline doesn't have loads the messages around it, which
    // show in its place, rather than every page between.
    let effects = app.dispatch(Action::Jump("e:10".into()));
    assert!(matches!(
        &effects[..],
        [Effect::Backend(Command::Around { room, item })] if room == "dm" && item == "e:10"
    ));
    assert!(
        app.dispatch(Action::Earlier).is_empty(),
        "the window has its own start"
    );
    let before = app.state.messages.clone();
    show(&mut app, &["e:20", "e:30", "e:40"]);
    assert!(
        Arc::ptr_eq(&before, &app.state.messages),
        "the window replaces what showed before"
    );
    app.apply(window("dm", chat::Direction::Around, false));
    assert!(app.state.detached);
    assert_eq!(app.state.jump, Some(("e:30".into(), 0)), "not yet");
    show(&mut app, &["e:9", "e:10", "e:11"]);
    assert_eq!(app.state.jump, Some(("e:10".into(), 1)));
    assert!(app.state.notice.is_none());

    // Scrolling down its end loads newer pages, one at a time, until they
    // reach the newest message.
    let later = |app: &mut Application| {
        matches!(
            app.dispatch(Action::Later).as_slice(),
            [Effect::Backend(Command::Later(room))] if room == "dm"
        )
    };
    assert!(later(&mut app));
    assert!(!later(&mut app), "one page at a time");
    app.apply(Event::HistoryLoaded {
        room: "dm".into(),
        direction: chat::Direction::Later,
        history: chat::History::Failed,
    });
    assert!(app.state.later_failed);
    assert!(later(&mut app), "asking again retries");
    app.apply(window("dm", chat::Direction::Later, false));
    assert!(app.state.detached && !app.state.loading_later);
    assert!(later(&mut app));
    app.apply(window("dm", chat::Direction::Later, true));
    assert!(!app.state.detached, "the window reached the present");
    assert!(!later(&mut app), "nothing is after the newest message");

    // Jump to present shows the newest messages again.
    app.dispatch(Action::Jump("e:1".into()));
    app.apply(window("dm", chat::Direction::Around, false));
    show(&mut app, &["e:1", "e:2"]);
    assert!(matches!(
        app.dispatch(Action::JumpToPresent).as_slice(),
        [Effect::Backend(Command::Present(room))] if room == "dm"
    ));
    assert!(!app.state.detached);
    assert!(app.dispatch(Action::JumpToPresent).is_empty());

    // So does sending a message.
    app.dispatch(Action::Jump("e:3".into()));
    app.apply(window("dm", chat::Direction::Around, false));
    show(&mut app, &["e:3"]);
    app.dispatch(Action::SetDraft("Back to now".into()));
    let effects = app.dispatch(Action::Send);
    assert!(matches!(
        &effects[..],
        [
            Effect::Backend(Command::Present(room)),
            Effect::Backend(Command::Send { .. })
        ] if room == "dm"
    ));
    assert!(!app.state.detached);

    // A message that isn't there once its window came was deleted.
    app.dispatch(Action::Jump("e:5".into()));
    app.apply(window("dm", chat::Direction::Around, false));
    show(&mut app, &["e:4", "e:6"]);
    assert!(
        app.state
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("couldn't find"))
    );
    // A window that failed to load leaves what showed.
    let jumped = app.state.jump.clone();
    app.dispatch(Action::Jump("e:7".into()));
    app.apply(Event::HistoryLoaded {
        room: "dm".into(),
        direction: chat::Direction::Around,
        history: chat::History::Failed,
    });
    show(&mut app, &["e:4", "e:6"]);
    assert_eq!(app.state.jump, jumped);
    assert_eq!(app.state.messages.len(), 2);

    // Leaving the room lets the window go.
    assert!(app.state.detached);
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert!(!app.state.detached);
}

#[test]
fn jumping_into_another_room_opens_it_on_the_message() {
    let mut app = connected();
    let effects = app.dispatch(Action::JumpInRoom {
        room: "games".into(),
        item: "e:1".into(),
    });
    assert_eq!(app.state.selected.as_deref(), Some("games"));
    assert_eq!(
        app.state.navigation,
        Navigation::Server("friends".into()),
        "its server shows"
    );
    let around = effects.iter().position(|effect| {
        matches!(effect, Effect::Backend(Command::Around { room, item }) if room == "games" && item == "e:1")
    });
    let open = effects.iter().rposition(
        |effect| matches!(effect, Effect::Backend(Command::Open(room)) if room == "games"),
    );
    assert!(open < around && around.is_some(), "opened, then focused");
    // What the room shows at first isn't the window, which it waits for.
    app.apply(Event::Timeline {
        room: "games".into(),
        messages: timeline(vec![message("e:50", false)]),
    });
    assert!(!app.state.room_ready);
    app.apply(window("games", chat::Direction::Around, false));
    app.apply(Event::Timeline {
        room: "games".into(),
        messages: timeline(vec![message("e:1", false), message("e:2", false)]),
    });
    assert!(app.state.room_ready && app.state.detached);
    assert_eq!(app.state.jump, Some(("e:1".into(), 0)));
    // In the open room it's an ordinary jump.
    let effects = app.dispatch(Action::JumpInRoom {
        room: "games".into(),
        item: "e:2".into(),
    });
    assert!(effects.is_empty());
    assert_eq!(app.state.jump, Some(("e:2".into(), 1)));
}

#[test]
fn stale_timeline_and_history_events_do_not_replace_the_selected_room() {
    let mut app = connected();
    app.dispatch(Action::Earlier);
    assert!(app.state.loading_history);
    app.apply(more("other"));
    assert!(app.state.loading_history);
    let messages = app.state.messages.clone();
    app.apply(Event::Timeline {
        room: "other".into(),
        messages: timeline(vec![ChatMessage {
            id: "other-0".into(),
            author: "Other".into(),
            body: "Stale".into(),
            time: "now".into(),
            ..Default::default()
        }]),
    });
    assert!(Arc::ptr_eq(&messages, &app.state.messages));
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert!(!app.state.room_ready);
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![]),
    });
    assert!(!app.state.room_ready);
    app.apply(more("dm"));
    assert!(!app.state.loading_history);
}

/// A page of earlier history arrived, with more before it.
fn more(room: &str) -> Event {
    Event::HistoryLoaded {
        room: room.into(),
        direction: chat::Direction::Earlier,
        history: chat::History::Loaded {
            start: false,
            present: true,
        },
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

#[test]
fn history_loads_a_page_at_a_time_until_the_first_message() {
    let mut app = connected();
    let earlier = |app: &mut Application| {
        matches!(
            app.dispatch(Action::Earlier).as_slice(),
            [Effect::Backend(Command::Earlier(room))] if room == "dm"
        )
    };
    assert!(earlier(&mut app));
    assert!(!earlier(&mut app), "one page at a time");
    app.apply(more("dm"));
    assert!(earlier(&mut app));
    // A page that fails stops loading until asked again.
    app.apply(Event::HistoryLoaded {
        room: "dm".into(),
        direction: chat::Direction::Earlier,
        history: chat::History::Failed,
    });
    assert!(app.state.history_failed);
    assert!(earlier(&mut app), "asking again retries");
    assert!(!app.state.history_failed);
    app.apply(start("dm"));
    assert!(app.state.history_start);
    assert!(!earlier(&mut app), "nothing is before the first message");
    // Another room starts over.
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert!(!app.state.history_start);
    app.dispatch(Action::SelectRoom("dm".into()));
    app.dispatch(Action::Navigate(Navigation::DirectMessages));
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![]),
    });
    assert!(!app.state.history_start);
    assert!(earlier(&mut app));
}

#[test]
fn a_timeline_that_drops_its_oldest_messages_has_history_to_load_again() {
    let mut app = connected();
    let message = |item: &str| ChatMessage {
        id: item.into(),
        item: item.into(),
        ..Default::default()
    };
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![message("d:1"), message("d:2")]),
    });
    app.dispatch(Action::Earlier);
    app.apply(start("dm"));
    // A new message keeps the first one loaded.
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![message("d:1"), message("d:2"), message("d:3")]),
    });
    assert!(app.state.history_start);
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![message("d:9")]),
    });
    assert!(!app.state.history_start);
}

#[test]
fn security_gates_are_shared_by_every_ui_adapter() {
    let mut app = connected();
    app.dispatch(Action::SetDraft("Hello".into()));
    app.state.trusted = false;
    assert!(app.dispatch(Action::Send).is_empty());
    assert!(
        app.dispatch(Action::RequestJoinVoice("lounge".into()))
            .is_empty()
    );
    app.state.trusted = true;
    app.state
        .rooms
        .iter_mut()
        .map(Arc::make_mut)
        .find(|room| room.id == "dm")
        .unwrap()
        .encrypted = false;
    assert!(app.dispatch(Action::Send).is_empty());
    app.state
        .rooms
        .iter_mut()
        .map(Arc::make_mut)
        .find(|room| room.id == "lounge")
        .unwrap()
        .encrypted = false;
    assert!(
        app.dispatch(Action::RequestJoinVoice("lounge".into()))
            .is_empty()
    );
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    app.dispatch(Action::SelectRoom("party-voice".into()));
    app.dispatch(Action::SetDraft("Never send text to voice".into()));
    assert!(app.dispatch(Action::Send).is_empty());
    assert!(matches!(
        app.dispatch(Action::RequestJoinVoice("party-voice".into()))
            .as_slice(),
        [Effect::Backend(Command::Voice(voice::Command::Join(_)))]
    ));
}

/// The streams effects watch, by user id.
fn watches(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Backend(Command::Voice(voice::Command::Watch(user))) => Some(user.as_str()),
            _ => None,
        })
        .collect()
}

fn in_call(id: &str, local: bool, streaming: bool) -> voice::Participant {
    voice::Participant {
        id: id.into(),
        user: id.into(),
        person: id.into(),
        local,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected: true,
        problem: None,
        streaming,
        video: None,
        avatar: None,
    }
}

fn call(phase: Phase, room: &str, participants: Vec<voice::Participant>) -> Event {
    Event::Voice(voice::State {
        phase,
        room: Some(room.into()),
        participants,
        ..Default::default()
    })
}

/// `connected()`, with Bob sharing his screen in the Lounge.
fn bob_live_in_lounge() -> Application {
    let mut app = connected();
    let lounge = app
        .state
        .rooms
        .iter_mut()
        .map(Arc::make_mut)
        .find(|room| room.id == "lounge")
        .unwrap();
    lounge.people = vec![chat::VoicePerson {
        id: "@bob:test".into(),
        name: "Bob".into(),
        streaming: true,
        ..Default::default()
    }];
    app
}

fn show_bob(app: &mut Application) -> Vec<Effect> {
    app.dispatch(Action::ShowVoicePerson {
        room: "lounge".into(),
        user: "@bob:test".into(),
    })
}

#[test]
fn someone_in_a_voice_channel_has_a_card_without_being_listed() {
    let mut app = bob_live_in_lounge();
    app.dispatch(Action::SelectRoom("lounge".into()));
    app.dispatch(Action::ShowProfile {
        user: "@bob:test".into(),
        large: false,
    });
    let view = app.state.profile.as_ref().expect("a card");
    assert_eq!(view.profile.name, "Bob");
}

#[test]
fn clicking_someone_sharing_from_outside_a_call_asks_then_watches_once_connected() {
    let mut app = bob_live_in_lounge();
    assert!(watches(&show_bob(&mut app)).is_empty());
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));
    assert_eq!(app.state.switch_voice.as_deref(), Some("lounge"));
    assert!(app.state.pending_watch.is_some());
    // Cancelling keeps the channel shown and forgets the stream.
    app.dispatch(Action::CancelVoiceSwitch);
    assert!(app.state.pending_watch.is_none() && app.state.switch_voice.is_none());
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));

    show_bob(&mut app);
    // Not in a call, joining leaves none.
    assert!(matches!(
        app.dispatch(Action::ConfirmVoiceSwitch).as_slice(),
        [Effect::Backend(Command::Voice(voice::Command::Join(room)))] if room == "lounge"
    ));
    let me = || in_call("@alice:test", true, false);
    assert!(watches(&app.apply(call(Phase::Joining, "lounge", vec![me()]))).is_empty());
    // Discord drops a watch before the call connects, and Bob may show
    // sharing only after.
    assert!(watches(&app.apply(call(Phase::Connected, "lounge", vec![me()]))).is_empty());
    let bob = in_call("@bob:test", false, true);
    assert_eq!(
        watches(&app.apply(call(Phase::Connected, "lounge", vec![me(), bob.clone()]))),
        ["@bob:test"]
    );
    assert!(app.state.pending_watch.is_none());
    // Watched once.
    assert!(watches(&app.apply(call(Phase::Connected, "lounge", vec![me(), bob]))).is_empty());
}

#[test]
fn clicking_someone_sharing_from_another_call_switches_and_forgets_a_failed_join() {
    let mut app = bob_live_in_lounge();
    let me = || in_call("@alice:test", true, false);
    app.apply(call(Phase::Connected, "party-voice", vec![me()]));
    show_bob(&mut app);
    assert_eq!(app.state.switch_voice.as_deref(), Some("lounge"));
    assert!(matches!(
        app.dispatch(Action::ConfirmVoiceSwitch).as_slice(),
        [
            Effect::Backend(Command::Voice(voice::Command::Leave)),
            Effect::Backend(Command::Voice(voice::Command::Join(room))),
        ] if room == "lounge"
    ));
    // Leaving the other call keeps the stream to watch.
    app.apply(Event::Voice(voice::State::default()));
    app.apply(call(Phase::Joining, "lounge", vec![]));
    assert!(app.state.pending_watch.is_some());
    // Joining failed.
    app.apply(Event::Voice(voice::State {
        error: Some("Discord voice took too long to connect. Try again.".into()),
        ..Default::default()
    }));
    assert!(app.state.pending_watch.is_none());
    let bob = in_call("@bob:test", false, true);
    assert!(watches(&app.apply(call(Phase::Connected, "lounge", vec![me(), bob]))).is_empty());
}

#[test]
fn clicking_someone_in_the_call_shows_it_and_watches_at_once() {
    let mut app = bob_live_in_lounge();
    let me = |streaming| in_call("@alice:test", true, streaming);
    let bob = in_call("@bob:test", false, true);
    app.apply(call(Phase::Connected, "lounge", vec![me(true), bob]));
    assert_eq!(watches(&show_bob(&mut app)), ["@bob:test"]);
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));
    assert!(app.state.switch_voice.is_none() && app.state.pending_watch.is_none());
    // Not one's own stream.
    app.dispatch(Action::Open("general".into()));
    let effects = app.dispatch(Action::ShowVoicePerson {
        room: "lounge".into(),
        user: "@alice:test".into(),
    });
    assert!(watches(&effects).is_empty() && app.state.pending_watch.is_none());
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));

    // Until Bob shows sharing in the call, it waits; another room or
    // leaving forgets it.
    app.apply(call(Phase::Connected, "lounge", vec![me(false)]));
    assert!(watches(&show_bob(&mut app)).is_empty());
    assert!(app.state.pending_watch.is_some());
    app.dispatch(Action::Open("general".into()));
    assert!(app.state.pending_watch.is_none());
    show_bob(&mut app);
    app.dispatch(Action::Voice(voice::Command::Leave));
    assert!(app.state.pending_watch.is_none());
}

#[test]
fn clicking_someone_with_only_a_camera_shows_the_channel_without_joining() {
    let mut app = connected();
    let lounge = app
        .state
        .rooms
        .iter_mut()
        .map(Arc::make_mut)
        .find(|room| room.id == "lounge")
        .unwrap();
    lounge.people = vec![chat::VoicePerson {
        id: "@bob:test".into(),
        name: "Bob".into(),
        video: true,
        ..Default::default()
    }];
    let effects = show_bob(&mut app);
    assert_eq!(app.state.selected.as_deref(), Some("lounge"));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::Voice(_))))
    );
    assert!(app.state.switch_voice.is_none() && app.state.pending_watch.is_none());
    assert!(!app.state.voice.active());
}

#[test]
fn clicking_a_voice_channel_joins_it_and_asks_before_leaving_another() {
    let mut app = Application::default();
    app.dispatch(Action::Preview);
    app.dispatch(Action::SelectRoom("lounge".into()));
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    // Clicking the channel you are in only shows it.
    app.dispatch(Action::SelectRoom("general".into()));
    app.dispatch(Action::SelectRoom("lounge".into()));
    assert!(app.state.switch_voice.is_none());
    app.dispatch(Action::Navigate(Navigation::Server("coop".into())));
    app.dispatch(Action::SelectRoom("party-voice".into()));
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    assert_eq!(app.state.switch_voice.as_deref(), Some("party-voice"));
    app.dispatch(Action::CancelVoiceSwitch);
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    // Coming back to a server restores its voice channel without joining it.
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    app.dispatch(Action::Voice(voice::Command::Leave));
    app.dispatch(Action::Navigate(Navigation::Server("coop".into())));
    assert_eq!(app.state.selected.as_deref(), Some("party-voice"));
    assert!(!app.state.voice.active() && app.state.switch_voice.is_none());
    app.dispatch(Action::SelectRoom("party-voice".into()));
    assert_eq!(app.state.voice.room.as_deref(), Some("party-voice"));
}

#[test]
fn a_discord_voice_channels_chat_opens_without_joining_it() {
    let mut app = connected();
    let id = "discord:10:11";
    let mut room = preview_room(id, "Lounge", false, 3);
    room.voice = true;
    room.join_locked = true;
    app.state.discord_online = true;
    app.state.rooms.push(Arc::new(room));
    Arc::make_mut(&mut app.state.servers[0])
        .channels
        .push(id.into());
    let opens = |effects: &[Effect]| {
        effects.iter().any(|effect| {
            matches!(
                effect, Effect::Backend(Command::Open(room)) if room == id
            )
        })
    };

    // Going to it shows its call, and loads no messages.
    let effects = app.dispatch(Action::Open(id.into()));
    assert!(!opens(&effects));
    assert!(!app.state.shows_text() && !app.state.can_write());
    assert!(app.dispatch(Action::Earlier).is_empty());
    assert!(app.state.menu_choices(id).contains(&MenuChoice::OpenChat));

    // Its chat opens without joining, which its roles don't allow.
    let effects = app.dispatch(Action::OpenChat(id.into()));
    assert!(opens(&effects));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::Voice(_))))
    );
    assert!(app.state.voice_chat && !app.state.can_join_voice(id));
    app.apply(Event::Timeline {
        room: id.into(),
        messages: timeline(vec![]),
    });
    assert!(app.state.can_write());
    app.dispatch(Action::SetDraft("Hello from voice chat".into()));
    assert!(matches!(
        send_command(app.dispatch(Action::Send)),
        Command::Send { room, body, .. } if room == id && body == "Hello from voice chat"
    ));
    assert!(app.dispatch(Action::Earlier).iter().any(|effect| matches!(
        effect, Effect::Backend(Command::Earlier(room)) if room == id
    )));

    // The header switches back to its call and to its chat again.
    let effects = app.dispatch(Action::ShowVoiceChat(false));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::CloseRoom)))
    );
    assert!(!app.state.voice_chat && !app.state.can_write());
    assert!(opens(&app.dispatch(Action::ShowVoiceChat(true))));
    assert!(
        app.state.voice_chat && app.state.room_ready,
        "its messages stay"
    );

    app.state
        .rooms
        .iter_mut()
        .map(Arc::make_mut)
        .find(|room| room.id == id)
        .unwrap()
        .read_only = true;
    assert!(!app.state.can_write());
}

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
                user_id: "2".into(),
                name: "Annie".into(),
                avatar: String::new(),
                nickname: Some("Annie".into()),
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
        app.dispatch(Action::AskPerson { user: "2".into(), ask: PersonAsk::Note }).as_slice(),
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
        })
        .is_empty()
    );
    assert!(matches!(
        app.dispatch(Action::AnswerPerson(String::new())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Relationship {
            user,
            change: RelationshipChange::Block,
        }))] if user == "2"
    ));
    assert_eq!(app.state.person_dialog, None);
    assert!(matches!(
        app.dispatch(Action::ChangeRelationship {
            user: "2".into(),
            change: RelationshipChange::Ignore
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
fn the_preview_lists_and_chooses_real_devices_across_calls() {
    let mut app = Application::default();
    app.dispatch(Action::Preview);
    let effects = app.dispatch(Action::Voice(voice::Command::Devices));
    assert!(matches!(effects.as_slice(), [Effect::ListDevices]));
    let device = |id: &str| voice::Device {
        id: id.into(),
        name: id.into(),
    };
    app.preview_devices(Ok(voice::Devices {
        microphones: vec![device("mic-a"), device("mic-b")],
        speakers: vec![device("out")],
        cameras: vec![device("camera-a"), device("camera-b")],
    }));
    assert!(
        app.dispatch(Action::Voice(voice::Command::Microphone("mic-b".into())))
            .is_empty()
    );
    app.dispatch(Action::Voice(voice::Command::Camera("camera-b".into())));
    app.dispatch(Action::RequestJoinVoice("lounge".into()));
    app.dispatch(Action::Voice(voice::Command::Leave));
    assert_eq!(app.state.voice.microphones.len(), 2);
    assert_eq!(app.state.voice.microphone.as_deref(), Some("mic-b"));
    assert_eq!(app.state.voice.cameras.len(), 2);
    assert_eq!(app.state.voice.camera.as_deref(), Some("camera-b"));
    app.preview_devices(Err("No sound server.".into()));
    assert_eq!(app.state.voice.error.as_deref(), Some("No sound server."));
    // A signed-in session lists devices in its voice worker instead.
    app.dispatch(Action::Logout);
    app.preview_devices(Ok(voice::Devices::default()));
    assert!(app.state.voice.error.is_none());
}

#[test]
fn preview_and_voice_switching_need_no_backend_and_voice_survives_navigation() {
    let mut app = Application::default();
    assert!(app.dispatch(Action::Preview).is_empty());
    assert_eq!(app.state.messages.len(), 5);
    app.dispatch(Action::SetDraft("Local message".into()));
    assert!(app.dispatch(Action::Send).is_empty());
    assert_eq!(app.state.messages.len(), 6);
    assert_eq!(app.state.messages.last().unwrap().body, "Local message");
    assert!(app.state.draft().is_empty());
    // The preview's calls sound as real ones do.
    assert_eq!(
        sounds(&app.dispatch(Action::RequestJoinVoice("lounge".into()))),
        [Sound::Join]
    );
    assert_eq!(app.state.voice.phase, Phase::Connected);
    assert!(app.state.voice.muted);
    assert!(
        app.dispatch(Action::Navigate(Navigation::DirectMessages))
            .is_empty()
    );
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    assert!(
        app.dispatch(Action::RequestJoinVoice("party-voice".into()))
            .is_empty()
    );
    assert_eq!(app.state.switch_voice.as_deref(), Some("party-voice"));
    app.dispatch(Action::CancelVoiceSwitch);
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    app.dispatch(Action::RequestJoinVoice("party-voice".into()));
    assert_eq!(
        sounds(&app.dispatch(Action::ConfirmVoiceSwitch)),
        [Sound::Leave, Sound::Join]
    );
    assert_eq!(app.state.voice.room.as_deref(), Some("party-voice"));
    assert_eq!(
        sounds(&app.dispatch(Action::Voice(voice::Command::Deafen(true)))),
        [Sound::Deafen]
    );
    assert_eq!(
        sounds(&app.dispatch(Action::Voice(voice::Command::Deafen(false)))),
        [Sound::Undeafen]
    );
    assert!(app.state.voice.muted);
    // Unmuting while deafened undeafens too.
    app.dispatch(Action::Voice(voice::Command::Deafen(true)));
    assert_eq!(
        sounds(&app.dispatch(Action::Voice(voice::Command::Mute(false)))),
        [Sound::Undeafen]
    );
    assert!(!app.state.voice.muted && !app.state.voice.deafened);
    assert_eq!(
        sounds(&app.dispatch(Action::Voice(voice::Command::Mute(true)))),
        [Sound::Mute]
    );
    // The preference that unmutes on joining does so without a sound.
    assert!(app.dispatch(Action::UnmuteOnJoin).is_empty());
    assert!(!app.state.voice.muted);
    // And the one that deafens, which mutes as well.
    assert!(app.dispatch(Action::DeafenOnJoin).is_empty());
    assert!(app.state.voice.muted && app.state.voice.deafened);
    assert!(app.dispatch(Action::Logout).is_empty());
    assert!(!app.state.preview);
    assert!(!app.state.voice.active());
}

#[test]
fn streams_sound_as_others_start_and_stop_watching() {
    let mut app = connected();
    let person = |id: &str, local: bool| voice::Participant {
        id: id.into(),
        user: id.into(),
        person: id.into(),
        local,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected: true,
        problem: None,
        streaming: false,
        video: None,
        avatar: None,
    };
    let frames = std::sync::Arc::new(crate::frames::Frames::create(2, 2).unwrap());
    let call = |sharing: Option<&[&str]>, watching: Option<&[&str]>| voice::State {
        phase: Phase::Connected,
        room: Some("lounge".into()),
        name: "Lounge".into(),
        participants: vec![
            person("me", true),
            person("sam", false),
            person("kim", false),
        ],
        sharing: sharing.map(|viewers| voice::Sharing {
            phase: voice::SharePhase::Live,
            frames: frames.clone(),
            viewers: viewers.iter().map(|&viewer| viewer.into()).collect(),
        }),
        watching: watching.map(|viewers| {
            Box::new(voice::Watching {
                user: "sam".into(),
                name: "sam".into(),
                opening: false,
                frames: None,
                viewers: viewers.iter().map(|&viewer| viewer.into()).collect(),
            })
        }),
        ..Default::default()
    };
    let mut heard = |state: voice::State| sounds(&app.apply(Event::Voice(state)));
    assert_eq!(heard(call(None, None)), [Sound::Join]);
    // Going live, with no one watching yet, and someone starting.
    assert_eq!(heard(call(Some(&[]), None)), []);
    assert_eq!(heard(call(Some(&["sam"]), None)), [Sound::ViewerJoin]);
    assert_eq!(heard(call(Some(&["sam"]), None)), []);
    assert_eq!(heard(call(Some(&[]), None)), [Sound::ViewerLeave]);
    // Watching Sam's stream: this account among its viewers makes no sound,
    // and nor do those already watching; others coming and going do.
    assert_eq!(heard(call(Some(&[]), Some(&["me", "kim"]))), []);
    assert_eq!(
        heard(call(Some(&["kim"]), Some(&["me", "kim"]))),
        [Sound::ViewerJoin]
    );
    assert_eq!(
        heard(call(Some(&["kim"]), Some(&["me"]))),
        [Sound::ViewerLeave]
    );
    // A stream ending makes no viewer sound.
    assert_eq!(heard(call(None, Some(&["me"]))), []);
}

#[test]
fn others_going_live_and_ending_sounds_but_not_as_anyone_comes_or_goes_streaming() {
    let mut app = connected();
    let person = |id: &str, local: bool, streaming: bool| voice::Participant {
        id: id.into(),
        user: id.into(),
        person: id.into(),
        local,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected: true,
        problem: None,
        streaming,
        video: None,
        avatar: None,
    };
    let call = |me: bool, people: &[(&str, bool)]| voice::State {
        phase: Phase::Connected,
        room: Some("lounge".into()),
        name: "Lounge".into(),
        participants: std::iter::once(person("me", true, me))
            .chain(
                people
                    .iter()
                    .map(|&(id, streaming)| person(id, false, streaming)),
            )
            .collect(),
        ..Default::default()
    };
    let mut heard = |state: voice::State| sounds(&app.apply(Event::Voice(state)));
    // Sam streams already as this account joins, and Kim as they arrive.
    assert_eq!(heard(call(false, &[("sam", true)])), [Sound::Join]);
    assert_eq!(
        heard(call(false, &[("sam", true), ("kim", true)])),
        [Sound::Join]
    );
    assert_eq!(
        heard(call(false, &[("sam", true), ("kim", false)])),
        [Sound::StreamStop]
    );
    assert_eq!(
        heard(call(false, &[("sam", true), ("kim", true)])),
        [Sound::StreamStart]
    );
    assert_eq!(heard(call(false, &[("sam", true), ("kim", true)])), []);
    assert_eq!(
        heard(call(false, &[("sam", false), ("kim", true)])),
        [Sound::StreamStop]
    );
    // Kim leaving mid-stream makes only the sound of leaving.
    assert_eq!(heard(call(false, &[("sam", false)])), [Sound::Leave]);
    // This account going live and ending makes none.
    assert_eq!(heard(call(true, &[("sam", false)])), []);
    assert_eq!(heard(call(false, &[("sam", false)])), []);
}

#[test]
fn calls_sound_as_people_come_and_go_but_not_while_reconnecting_or_ringing() {
    let mut app = connected();
    let person = |id: &str, local: bool, connected: bool| voice::Participant {
        id: id.into(),
        user: id.into(),
        person: id.into(),
        local,
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        speaking: false,
        connected,
        problem: None,
        streaming: false,
        video: None,
        avatar: None,
    };
    let call = |phase: Phase, people: &[(&str, bool)]| voice::State {
        phase,
        room: Some("lounge".into()),
        name: "Lounge".into(),
        muted: false,
        participants: std::iter::once(person("me", true, true))
            .chain(
                people
                    .iter()
                    .map(|&(id, connected)| person(id, false, connected)),
            )
            .collect(),
        ..Default::default()
    };
    let mut heard = |state: voice::State| sounds(&app.apply(Event::Voice(state)));
    assert_eq!(heard(call(Phase::Joining, &[])), []);
    // Those already there when this account arrives make no sound of their own.
    assert_eq!(
        heard(call(Phase::Connected, &[("sam", true)])),
        [Sound::Join]
    );
    // Someone a DM call is ringing isn't there until they answer.
    assert_eq!(
        heard(call(Phase::Connected, &[("sam", true), ("kim", false)])),
        []
    );
    assert_eq!(
        heard(call(Phase::Connected, &[("sam", true), ("kim", true)])),
        [Sound::Join]
    );
    // Two going at once make one sound.
    assert_eq!(heard(call(Phase::Connected, &[])), [Sound::Leave]);
    assert_eq!(heard(call(Phase::Reconnecting, &[])), []);
    assert_eq!(heard(call(Phase::Connected, &[("sam", true)])), []);
    assert_eq!(heard(call(Phase::Leaving, &[])), [Sound::Leave]);
    assert_eq!(heard(voice::State::default()), []);

    // Muting sounds as it's asked for, rather than once the call confirms it.
    app.apply(Event::Voice(call(Phase::Connected, &[])));
    let effects = app.dispatch(Action::Voice(voice::Command::Mute(true)));
    assert!(matches!(
        effects.as_slice(),
        [
            Effect::Backend(Command::Voice(voice::Command::Mute(true))),
            Effect::Sound(Sound::Mute)
        ]
    ));
    assert!(matches!(
        app.dispatch(Action::UnmuteOnJoin).as_slice(),
        [Effect::Backend(Command::Voice(voice::Command::Mute(false)))]
    ));
    assert!(matches!(
        app.dispatch(Action::DeafenOnJoin).as_slice(),
        [Effect::Backend(Command::Voice(voice::Command::Deafen(
            true
        )))]
    ));
}

#[test]
fn notifications_say_whether_they_mention_this_account() {
    let mut app = connected();
    let effects = app.apply(Event::Notify(chat::Notification {
        room: "games".into(),
        author: "Sam".into(),
        body: "@alice co-op?".into(),
        avatar: None,
        mention: true,
    }));
    assert!(matches!(effects.as_slice(), [Effect::Notify(alert)] if alert.mention && !alert.call));
}

#[test]
fn a_call_moved_to_another_channel_takes_its_view_along() {
    let mut app = connected();
    let call = |room: &str| {
        Event::Voice(voice::State {
            phase: Phase::Connected,
            room: Some(room.into()),
            ..Default::default()
        })
    };
    app.dispatch(Action::Open("lounge".into()));
    app.apply(call("lounge"));
    app.apply(call("party-voice"));
    assert_eq!(app.state.selected.as_deref(), Some("party-voice"));
    // Viewing another room, the view stays.
    app.dispatch(Action::Open("games".into()));
    app.apply(call("lounge"));
    assert_eq!(app.state.selected.as_deref(), Some("games"));
}

#[test]
fn native_voice_switch_is_explicit_and_rechecks_trust_before_leaving() {
    let mut app = connected();
    app.apply(Event::Voice(voice::State {
        phase: Phase::Connected,
        room: Some("lounge".into()),
        name: "Lounge".into(),
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    assert!(
        app.dispatch(Action::RequestJoinVoice("party-voice".into()))
            .is_empty()
    );
    assert_eq!(app.state.voice.room.as_deref(), Some("lounge"));
    app.apply(Event::Trust {
        trusted: false,
        error: None,
    });
    assert!(app.dispatch(Action::ConfirmVoiceSwitch).is_empty());
    assert!(app.state.switch_voice.is_none());
    app.apply(Event::Trust {
        trusted: true,
        error: None,
    });
    app.dispatch(Action::RequestJoinVoice("party-voice".into()));
    assert!(matches!(
        app.dispatch(Action::ConfirmVoiceSwitch).as_slice(),
        [
            Effect::Backend(Command::Voice(voice::Command::Leave)),
            Effect::Backend(Command::Voice(voice::Command::Join(id)))
        ] if id == "party-voice"
    ));
}

#[test]
fn login_recovery_and_error_state_do_not_depend_on_widgets() {
    let mut app = Application::default();
    assert!(
        app.dispatch(Action::Login {
            homeserver: "".into(),
            username: "alice".into(),
            password: "secret".into()
        })
        .is_empty()
    );
    assert!(!app.state.busy);
    let effects = app.dispatch(Action::Login {
        homeserver: " https://example.org ".into(),
        username: " @alice:example.org ".into(),
        password: " secret ".into(),
    });
    assert!(
        matches!(effects.as_slice(), [Effect::Start(chat::Start::Login { homeserver, username, password })] if homeserver == "https://example.org" && username == "@alice:example.org" && password == " secret ")
    );
    assert!(app.state.busy);
    assert!(app.dispatch(Action::Restore).is_empty());
    app.apply(Event::Error("Wrong credentials".into()));
    app.stopped();
    assert_eq!(app.state.notice.as_deref(), Some("Wrong credentials"));
    assert!(!app.state.busy);
    app = connected();
    assert!(
        matches!(app.dispatch(Action::Recover(" key ".into())).as_slice(), [Effect::Backend(Command::Recover(key))] if key == "key")
    );
    assert!(app.state.busy);
    app.backend_failed(&Command::Recover("key".into()));
    assert!(!app.state.busy);
    app.apply(Event::Voice(voice::State {
        phase: Phase::Connected,
        room: Some("lounge".into()),
        ..Default::default()
    }));
    assert!(app.dispatch(Action::Recover("key".into())).is_empty());
    assert!(!app.state.busy);
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
fn videos_play_in_the_open_room_until_it_closes() {
    let mut app = connected();
    let file = |name: &str, video: bool| chat::Attachment {
        name: name.into(),
        size: None,
        image: false,
        dimensions: None,
        preview: None,
        preview_failed: false,
        placement: chat::Placement::Message,
        gif: None,
        spoiler: false,
        video,
        url: None,
    };
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![ChatMessage {
            id: "m".into(),
            item: "e:m".into(),
            attachments: vec![file("notes.txt", false), file("clip.mp4", true)],
            ..Default::default()
        }]),
    });
    let play = |index| Action::PlayVideo {
        item: "e:m".into(),
        index,
        width: 720,
        height: 404,
    };
    assert!(app.dispatch(play(0)).is_empty(), "a file isn't a video");
    assert!(matches!(
        app.dispatch(play(1)).as_slice(),
        [Effect::Backend(Command::PlayVideo { room, index: 1, width: 720, .. })] if room == "dm"
    ));
    let video = |room: &str| chat::Video {
        room: room.into(),
        item: "e:m".into(),
        index: 1,
        frames: None,
        duration_ms: Some(6000),
        size: Some((480, 270)),
        loading: false,
        playing: true,
        ended: false,
        sound: true,
        muted: false,
        volume: 1.0,
        error: None,
    };
    app.apply(Event::Video(Some(video("elsewhere"))));
    assert!(
        app.state.video.is_none(),
        "another room's video doesn't show"
    );
    app.apply(Event::Video(Some(video("dm"))));
    assert!(app.state.video.as_ref().is_some_and(|video| video.playing));
    assert!(matches!(
        app.dispatch(Action::Video(chat::VideoCommand::Pause))
            .as_slice(),
        [Effect::Backend(Command::Video(chat::VideoCommand::Pause))]
    ));
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert!(app.state.video.is_none(), "leaving the room stops it");
}

#[test]
fn the_viewer_steps_through_a_messages_images() {
    let mut app = connected();
    let file = |name: &str, image: bool| chat::Attachment {
        name: name.into(),
        size: None,
        image,
        dimensions: None,
        preview: None,
        preview_failed: false,
        placement: chat::Placement::Message,
        gif: None,
        spoiler: false,
        video: false,
        url: None,
    };
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![ChatMessage {
            id: "m".into(),
            item: "e:m".into(),
            attachments: vec![
                file("a.png", true),
                file("notes.txt", false),
                file("b.png", true),
            ],
            ..Default::default()
        }]),
    });
    let viewed = |effects: Vec<Effect>| -> Vec<usize> {
        effects
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Backend(Command::ViewImage { index, .. }) => Some(index),
                _ => None,
            })
            .collect()
    };
    let view = |index| Action::ViewImage {
        item: "e:m".into(),
        index,
    };
    assert!(
        viewed(app.dispatch(view(1))).is_empty(),
        "a file isn't an image"
    );
    assert!(app.state.viewer.is_none());
    assert_eq!(viewed(app.dispatch(view(0))), [0]);
    assert_eq!(
        viewed(app.dispatch(Action::StepImage(1))),
        [2],
        "the file is skipped"
    );
    assert!(
        viewed(app.dispatch(Action::StepImage(1))).is_empty(),
        "back to the first, which is already loading"
    );
    assert_eq!(
        app.state.viewer.as_ref().map(|viewer| viewer.index),
        Some(0)
    );
    app.dispatch(Action::StepImage(-1));
    assert_eq!(
        app.state.viewer.as_ref().map(|viewer| viewer.index),
        Some(2),
        "and around backwards"
    );
    app.apply(Event::Image {
        room: "dm".into(),
        item: "e:m".into(),
        index: 2,
        picture: Err("gone"),
    });
    assert_eq!(
        app.state.viewer.as_ref().unwrap().pictures[&2],
        Some(Err("gone"))
    );
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert!(app.state.viewer.is_none(), "another room closes it");
}

#[test]
fn a_picture_with_an_address_is_copied_to_the_clipboard() {
    let mut app = connected();
    let file = |url: Option<&str>| chat::Attachment {
        name: "cat.png".into(),
        size: None,
        image: true,
        dimensions: None,
        preview: None,
        preview_failed: false,
        placement: chat::Placement::Message,
        gif: None,
        spoiler: false,
        video: false,
        url: url.map(Into::into),
    };
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![ChatMessage {
            id: "m".into(),
            item: "e:m".into(),
            attachments: vec![file(None), file(Some("https://cdn.example/cat.png"))],
            ..Default::default()
        }]),
    });
    let copy = |index| Action::CopyAttachment {
        item: "e:m".into(),
        index,
    };
    assert!(
        app.dispatch(copy(0)).is_empty(),
        "a picture without an address can't be downloaded to copy"
    );
    assert!(matches!(
        app.dispatch(copy(1)).as_slice(),
        [Effect::Backend(Command::CopyAttachment { room, index: 1, .. })] if room == "dm"
    ));
    assert!(matches!(
        app.apply(Event::Copied(Ok(b"png".to_vec()))).as_slice(),
        [Effect::CopyPicture(picture)] if picture == b"png"
    ));
    app.apply(Event::Copied(Err(
        "Yapfast couldn't copy this picture. Save it instead.",
    )));
    assert_eq!(
        app.state.notice.as_deref(),
        Some("Yapfast couldn't copy this picture. Save it instead.")
    );
}

#[test]
#[cfg(feature = "discord")]
fn asks_once_for_the_pictures_on_screen() {
    use yapfast_discord::{
        ready::{Channel, Server},
        service::{Snapshot, Status, VoiceMember},
    };
    let snapshot = |member: &str| Snapshot {
        status: Status::Online,
        servers: vec![MarkedServer::from(Server {
            id: "10".into(),
            name: "Test".into(),
            icon: Some("icon.png".into()),
            voice_channels: vec![Channel {
                id: "11".into(),
                name: "Lounge".into(),
                ..Default::default()
            }],
            ..Default::default()
        })],
        voice: [(
            "11".to_owned(),
            vec![VoiceMember {
                user_id: member.into(),
                name: member.into(),
                avatar: format!("{member}.png"),
                muted: false,
                deafened: false,
                streaming: false,
                video: false,
                elsewhere: false,
                server_muted: false,
                server_deafened: false,
            }],
        )]
        .into(),
        ..Default::default()
    };
    let asked = |effects: Vec<Effect>| -> Vec<String> {
        effects
            .into_iter()
            .flat_map(|effect| match effect {
                Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls))) => urls,
                _ => vec![],
            })
            .collect()
    };
    let mut app = connected();
    assert_eq!(
        asked(app.apply(discord_event(snapshot("ann")))),
        Vec::<String>::new(),
        "virtualized server icons are requested by the UI"
    );
    assert_eq!(
        asked(app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())))),
        ["ann.png"]
    );
    assert!(asked(app.apply(discord_event(snapshot("ann")))).is_empty());
    assert_eq!(
        asked(app.apply(discord_event(snapshot("bob")))),
        ["bob.png"]
    );
    app.apply(Event::Picture {
        url: "icon.png".into(),
        picture: Arc::new(Picture {
            width: 1,
            height: 1,
            rgba: crate::chat::Pixels::new(1, 1, vec![0; 4]),
            ..Default::default()
        }),
    });
    assert!(app.state.pictures.contains_key("icon.png"));
    assert_eq!(
        asked(app.dispatch(Action::LoadPictures(vec!["icon.png".into()]))),
        ["icon.png"]
    );
    assert!(asked(app.dispatch(Action::LoadPictures(vec!["icon.png".into()]))).is_empty());
}

fn member(id: &str, name: &str) -> ChatMember {
    ChatMember {
        id: id.into(),
        name: name.into(),
        username: None,
        role: Role::Member,
        avatar: None,
        group: None,
        presence: None,
        color: None,
        activity: None,
        activities: Default::default(),
        roles: Default::default(),
        timed_out_until: None,
    }
}

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
fn clicked_mentions_show_people_and_roles() {
    use crate::mention::{Mentioned, Target};
    let mut app = connected();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![ChatMessage {
            body: "@Riley ask @Mods".into(),
            mentions: vec![
                Mentioned {
                    text: "@Riley".into(),
                    target: Target::User("@riley:test".into()),
                    color: None,
                    at: vec![0],
                },
                Mentioned {
                    text: "@Mods".into(),
                    target: Target::Role("30".into()),
                    color: Some(0x3498db),
                    at: vec![11],
                },
            ],
            ..message("e:$bob", false)
        }]),
    });
    // Riley isn't listed, but the mention names them.
    app.dispatch(Action::ShowProfile {
        user: "@riley:test".into(),
        large: false,
    });
    let view = app.state.profile.as_ref().expect("a card");
    assert_eq!(view.profile.name, "Riley");
    assert_eq!(view.profile.handle, "@riley:test");
    app.dispatch(Action::CloseProfile);

    // A role shows by the name the message gives it; Matrix asks no backend.
    assert!(app.dispatch(Action::ShowRole("30".into())).is_empty());
    let role = app.state.role.clone().expect("a role card");
    assert_eq!((role.name.as_str(), role.members), ("Mods", None));
    app.apply(Event::RoleMembers {
        room: "dm".into(),
        role: "30".into(),
        name: "Mods".into(),
        color: Some(0x3498db),
        members: vec![member("@sam:test", "Sam")],
    });
    assert_eq!(
        app.state
            .role
            .as_ref()
            .unwrap()
            .members
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    // Someone on the role's list opens over it.
    app.dispatch(Action::ShowProfile {
        user: "@sam:test".into(),
        large: false,
    });
    assert_eq!(app.state.profile.as_ref().unwrap().profile.name, "Sam");
    assert!(app.state.role.is_some());
    app.dispatch(Action::CloseRole);
    assert!(app.state.role.is_none());
}

#[test]
fn matrix_mentions_link_to_the_person() {
    let mut app = connected();
    app.apply(Event::Members {
        room: "dm".into(),
        members: Ok(vec![member("@sam:test", "Sam")]),
    });
    // Matrix lists every member, so nothing is searched for.
    assert!(
        app.dispatch(Action::MentionQuery(Some("s".into())))
            .is_empty()
    );
    let names: Vec<_> = app
        .state
        .mention_suggestions()
        .into_iter()
        .map(|candidate| candidate.name)
        .collect();
    assert_eq!(names, ["Sam"]);
    app.dispatch(Action::SetDraft("@Sam are you in?".into()));
    let Command::Send { body, .. } = send_command(app.dispatch(Action::Send)) else {
        unreachable!()
    };
    assert_eq!(body, "[Sam](https://matrix.to/#/@sam:test) are you in?");
    assert_eq!(app.state.mention_query, None);
    app.apply(Event::SendResult {
        room: "dm".into(),
        body,
        result: Ok(()),
    });
    assert_eq!(app.state.draft(), "", "the sent draft is cleared");
}

#[test]
#[cfg(feature = "discord")]
fn discord_mentions_search_the_server_and_name_roles() {
    use yapfast_discord::{
        ready::{Channel, Emoji, Server},
        service::{Account, Snapshot, Status},
    };
    let mut app = connected();
    let emoji = |id: &str, name: &str, animated| Emoji {
        id: id.into(),
        name: name.into(),
        animated,
        available: true,
    };
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        // Nitro, for the animated emoji.
        account: Some(Account {
            name: "Alice".into(),
            avatar: String::new(),
            premium_type: 2,
            mfa_enabled: false,
            has_email: true,
        }),
        servers: vec![MarkedServer::from(Server {
            id: "10".into(),
            name: "Test".into(),
            text_channels: vec![Channel {
                id: "21".into(),
                name: "general".into(),
                ..Default::default()
            }],
            emojis: vec![emoji("5", "talk", false), emoji("6", "party", true)].into(),
            ..Default::default()
        })],
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    let room = "discord:10:21".to_owned();
    app.apply(Event::Timeline {
        room: room.clone(),
        messages: timeline(vec![]),
    });
    app.apply(Event::Members {
        room: room.clone(),
        members: Ok(vec![
            ChatMember {
                username: Some("bob".into()),
                ..member("2", "Bob")
            },
            member("1", "Alice"),
        ]),
    });
    app.apply(Event::MentionGroups {
        room: room.clone(),
        groups: vec![MentionGroup {
            name: "Mods".into(),
            mention: "<@&30>".into(),
            color: Some(0x3498db),
        }],
    });
    // People with their usernames, then roles; never this account.
    app.dispatch(Action::MentionQuery(Some(String::new())));
    let suggested: Vec<_> = app
        .state
        .mention_suggestions()
        .into_iter()
        .map(|candidate| (candidate.name, candidate.username, candidate.group))
        .collect();
    assert_eq!(
        suggested,
        [
            ("Bob".into(), Some("bob".into()), false),
            ("Mods".into(), None, true)
        ]
    );
    // Few listed members match, so the server is searched, once per name.
    assert!(matches!(
        app.dispatch(Action::MentionQuery(Some("Bo".into()))).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::SearchMembers { query, .. }))] if query == "Bo"
    ));
    assert!(
        app.dispatch(Action::MentionQuery(Some("bo".into())))
            .is_empty()
    );
    app.apply(Event::MemberMatches {
        room: room.clone(),
        query: "bo".into(),
        members: vec![member("2", "Bob"), member("9", "Bobby")],
    });
    let names: Vec<_> = app
        .state
        .mention_suggestions()
        .into_iter()
        .map(|candidate| candidate.name)
        .collect();
    assert_eq!(names, ["Bob", "Bobby"]);
    // Custom emoji from the picker go out in Discord's form.
    app.dispatch(Action::SetDraft(
        "@Bobby and @mods, look :talk: :party:".into(),
    ));
    let Command::Send { body, .. } = send_command(app.dispatch(Action::Send)) else {
        unreachable!()
    };
    assert_eq!(body, "<@9> and <@&30>, look <:talk:5> <a:party:6>");
    app.apply(Event::SendResult {
        room: room.clone(),
        body,
        result: Ok(()),
    });
    // `@silent` sends a message that notifies nobody, and links lose their
    // tracking.
    app.dispatch(Action::StripTracking(true));
    app.dispatch(Action::SetDraft(
        "@silent see https://a.example/p?utm_source=x&id=2".into(),
    ));
    let Command::Send { body, options, .. } = send_command(app.dispatch(Action::Send)) else {
        unreachable!()
    };
    assert_eq!(body, "see https://a.example/p?id=2");
    assert!(options.silent && !options.quiet_reply);
    // Another conversation starts afresh.
    app.dispatch(Action::Navigate(Navigation::DirectMessages));
    app.dispatch(Action::SelectRoom("dm".into()));
    assert!(app.state.mention_groups.is_empty() && app.state.found_members.is_empty());
}

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
        messages: timeline(vec![]),
    });
    assert!(!app.state.can_write(), "read-only channel");
    // Back to general: shown as it was left at once, while the backend
    // sends it afresh.
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    assert!(app.state.room_ready);
    assert_eq!(app.state.messages[0].body, "kept");
    assert_eq!(app.state.members, Some(Ok(vec![])));
    app.apply(Event::Timeline {
        room: "discord:10:21".into(),
        messages: timeline(vec![]),
    });
    app.apply(discord_event(Snapshot {
        status: Status::Offline("Reconnecting".into()),
        ..snapshot(false)
    }));
    assert!(!app.state.can_write(), "sending waits for Discord");
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
fn a_linked_discord_profile_loads_for_someone_not_listed() {
    let mut app = discord_server();
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::Open("discord:10:11".into()));

    let effects = app.dispatch(Action::ShowProfile {
        user: "4242".into(),
        large: true,
    });
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::Profile { room, user }))
                if room == "discord:10:11" && user == "4242"
        )),
        "Discord's profile is fetched"
    );
    let view = app.state.profile.as_ref().unwrap();
    assert!(view.large && view.loading);
    assert_eq!(view.profile.id, "4242");
    assert_eq!(view.profile.name, "4242", "the id stands in while it loads");

    app.apply(Event::Profile {
        user: "4242".into(),
        result: Ok(Profile {
            id: "4242".into(),
            name: "Nia".into(),
            handle: "nia".into(),
            ..Default::default()
        }),
    });
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.loading);
    assert_eq!(view.profile.name, "Nia");
    assert_eq!(view.profile.handle, "nia");
}

#[test]
#[cfg(feature = "discord")]
fn discord_links_yapfast_cant_show_are_left_to_the_browser() {
    use discord::DiscordRoute;
    let channel = |guild: &str, channel: &str| DiscordRoute::Channel {
        guild: guild.into(),
        channel: channel.into(),
    };
    let user = DiscordRoute::User {
        user: "4242".into(),
    };
    let event = |guild: &str| DiscordRoute::Event {
        guild: guild.into(),
        event: "12".into(),
    };
    let mut app = discord_server();
    let state = &app.state;
    assert!(state.opens_discord_route(&DiscordRoute::Server { guild: "10".into() }));
    assert!(state.opens_discord_route(&channel("10", "11")));
    assert!(
        state.opens_discord_route(&channel("10", "77")),
        "a thread, which opening asks for"
    );
    assert!(state.opens_discord_route(&user));
    assert!(state.opens_discord_route(&event("10")));
    assert!(
        !state.opens_discord_route(&DiscordRoute::Server { guild: "50".into() }),
        "a server the account isn't in"
    );
    assert!(!state.opens_discord_route(&event("50")));
    assert!(!state.opens_discord_route(&channel("50", "51")));
    assert!(
        !state.opens_discord_route(&channel("@me", "77")),
        "an unknown DM"
    );

    app.state.discord_online = false;
    let state = &app.state;
    assert!(state.opens_discord_route(&channel("10", "11")), "listed");
    assert!(!state.opens_discord_route(&channel("10", "77")));
    assert!(!state.opens_discord_route(&user));
    assert!(!state.opens_discord_route(&DiscordRoute::Invite { code: "abc".into() }));
    assert!(
        !state.opens_discord_route(&event("10")),
        "the server is listed, but the event can't be looked up"
    );
}

#[test]
fn matrix_profiles_show_what_the_member_list_knows() {
    let mut app = connected();
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    app.dispatch(Action::SelectRoom("general".into()));
    app.apply(Event::Members {
        room: "general".into(),
        members: Ok(vec![ChatMember {
            id: "@bob:test".into(),
            name: "Bob".into(),
            username: None,
            role: Role::Moderator,
            avatar: Some("bob.png".into()),
            group: None,
            presence: None,
            color: None,
            activity: None,
            activities: Default::default(),
            roles: Default::default(),
            timed_out_until: None,
        }]),
    });
    let effects = app.dispatch(Action::ShowProfile {
        user: "@bob:test".into(),
        large: false,
    });
    assert!(
        !effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::Profile { .. }))
        )),
        "Matrix has nothing more to fetch"
    );
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.large && !view.loading);
    assert_eq!(view.profile.handle, "@bob:test");
    assert_eq!(view.profile.roles[0].name, "Moderator");
    app.dispatch(Action::EnlargeProfile);
    assert!(app.state.profile.as_ref().unwrap().large);
    app.dispatch(Action::SelectRoom("games".into()));
    assert!(app.state.profile.is_none(), "another room closes it");

    app.dispatch(Action::Navigate(Navigation::DirectMessages));
    app.dispatch(Action::ShowDmProfile("dm".into()));
    let view = app.state.profile.as_ref().unwrap();
    assert!(view.large, "a DM's profile opens full size");
    assert_eq!(view.profile.name, "DM");
    app.dispatch(Action::CloseProfile);
    assert!(app.state.profile.is_none());
    app.dispatch(Action::ShowDmProfile("general".into()));
    assert!(app.state.profile.is_none(), "a room has no one person");
}

#[test]
#[cfg(feature = "discord")]
fn discord_profiles_load_the_rest_from_discord() {
    use yapfast_discord::service::{Conversation, Snapshot, Status};
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        conversations: vec![Arc::new(Conversation {
            id: "30".into(),
            name: "Ann".into(),
            avatar: Some("ann.png".into()),
            person: Some("7".into()),
            people: 2,
            ..Default::default()
        })],
        ..Default::default()
    }));
    let effects = app.dispatch(Action::ShowDmProfile("discord:@me:30".into()));
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::Profile { room, user })) if room == "discord:@me:30" && user == "7"
        )),
        "Discord's profile is fetched"
    );
    let view = app.state.profile.as_ref().unwrap();
    assert!(view.loading && view.profile.handle.is_empty());
    assert_eq!(view.profile.avatar.as_deref(), Some("ann.png"));

    app.apply(Event::Profile {
        user: "8".into(),
        result: Err("gone".into()),
    });
    assert!(
        app.state.profile.as_ref().unwrap().loading,
        "another person's answer is stale"
    );
    let effects = app.apply(Event::Profile {
        user: "7".into(),
        result: Ok(Profile {
            id: "7".into(),
            name: "Ann".into(),
            handle: "ann".into(),
            banner: Some("ann-banner.png".into()),
            bio: "Hi".into(),
            ..Default::default()
        }),
    });
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls))) if urls.iter().any(|url| url == "ann-banner.png")
        )),
        "the banner downloads"
    );
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.loading && view.note.is_none());
    assert_eq!(
        (view.profile.handle.as_str(), view.profile.bio.as_str()),
        ("ann", "Hi")
    );

    app.dispatch(Action::ShowDmProfile("discord:@me:30".into()));
    app.apply(Event::Profile {
        user: "7".into(),
        result: Err("Missing Access".into()),
    });
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.loading && view.note.is_some());
}

#[test]
#[cfg(feature = "discord")]
fn discord_profiles_show_the_kept_one_until_discord_answers() {
    use yapfast_discord::service::{Conversation, Snapshot, Status};
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        conversations: vec![Arc::new(Conversation {
            id: "30".into(),
            name: "Ann".into(),
            person: Some("7".into()),
            people: 2,
            ..Default::default()
        })],
        ..Default::default()
    }));
    let ann = |bio: &str| Profile {
        id: "7".into(),
        name: "Ann".into(),
        handle: "ann".into(),
        bio: bio.into(),
        ..Default::default()
    };
    app.dispatch(Action::ShowDmProfile("discord:@me:30".into()));
    app.apply(Event::KeptProfile {
        user: "7".into(),
        profile: ann("Old"),
    });
    let view = app.state.profile.as_ref().unwrap();
    assert!(view.loading && view.kept);
    assert_eq!(view.profile.bio, "Old");
    app.apply(Event::Profile {
        user: "7".into(),
        result: Ok(ann("New")),
    });
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.loading && !view.kept);
    assert_eq!(view.profile.bio, "New");

    // Offline, the kept one stays, without a note.
    app.dispatch(Action::ShowDmProfile("discord:@me:30".into()));
    app.apply(Event::KeptProfile {
        user: "7".into(),
        profile: ann("Old"),
    });
    app.apply(Event::Profile {
        user: "7".into(),
        result: Err("offline".into()),
    });
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.loading && view.note.is_none());
    assert_eq!(view.profile.bio, "Old");
}

#[test]
#[cfg(feature = "discord")]
fn discord_friends_are_listed_and_messaged() {
    use yapfast_discord::{
        member_list::Presence as Discord,
        service::{Conversation, Friend, Snapshot, Status},
    };
    let friend = |id: &str, name: &str, presence| Friend {
        user_id: id.into(),
        name: name.into(),
        avatar: format!("{id}.png"),
        nickname: None,
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
                activity: Some("making tea".into()),
                activities: Default::default(),
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
            id: "2".into(),
            name: "Ann".into(),
            nickname: None,
            avatar: Some("2.png".into()),
            presence: Some(chat::Presence::Idle),
            activity: Some("making tea".into()),
            activities: Default::default(),
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
        [MenuChoice::Profile, MenuChoice::Message, MenuChoice::CopyId]
    );
    assert_eq!(
        app.state.person_choices("3"),
        [MenuChoice::Message, MenuChoice::CopyId]
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
fn a_profile_card_messages_its_person_in_their_dm() {
    use yapfast_discord::service::{Conversation, Snapshot, Status};
    let snapshot = |dms: &[(&str, &str)]| Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        friends: vec!["2".into(), "3".into()].into(),
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
    let sends = |effects: &[Effect], to: &str, text: &str| {
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Backend(Command::Send { room, body, reply: None, .. }) if room == to && body == text
        ))
    };
    let message = |user: &str, text: &str| Action::MessageFromProfile {
        user: user.into(),
        text: text.into(),
    };
    let mut app = Application::default();
    app.apply(discord_event(snapshot(&[("30", "2")])));
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));

    let ready = |app: &mut Application, room: &str| {
        app.apply(Event::Timeline {
            room: room.into(),
            messages: timeline(vec![]),
        })
    };

    // A listed DM opens and the message goes once it can, leaving its
    // composer as it was.
    app.state
        .drafts
        .insert("discord:@me:30".into(), "half written".into());
    let effects = app.dispatch(message("2", "hi Ann"));
    assert_eq!(app.state.selected.as_deref(), Some("discord:@me:30"));
    assert!(
        !sends(&effects, "discord:@me:30", "hi Ann"),
        "not before it's ready"
    );
    assert!(sends(
        &ready(&mut app, "discord:@me:30"),
        "discord:@me:30",
        "hi Ann"
    ));
    assert_eq!(app.state.drafts["discord:@me:30"], "half written");
    app.apply(Event::SendResult {
        room: "discord:@me:30".into(),
        body: "hi Ann".into(),
        result: Ok(()),
    });
    assert_eq!(app.state.drafts["discord:@me:30"], "half written");

    // Nothing goes empty, to oneself, or to someone Yapfast can't message.
    for (user, text) in [("2", "  "), ("1", "hi me"), ("9", "hi")] {
        assert!(app.dispatch(message(user, text)).is_empty(), "{user}");
    }

    // Without a DM, one starts, and the message goes once it opens; another
    // DM opening first gets nothing.
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    assert!(matches!(
        app.dispatch(message("3", "hi Bob")).as_slice(),
        [Effect::Backend(Command::StartDm { id, discord: true })] if id == "3"
    ));
    app.apply(Event::DmStarted(Ok("discord:@me:31".into())));
    app.apply(discord_event(snapshot(&[("30", "2"), ("31", "3")])));
    assert_eq!(app.state.selected.as_deref(), Some("discord:@me:31"));
    assert!(sends(
        &ready(&mut app, "discord:@me:31"),
        "discord:@me:31",
        "hi Bob"
    ));
    assert!(app.pending_message.is_none());

    // Leaving the DM before it's ready drops the message.
    app.dispatch(Action::OpenDmScreen(DmScreen::Friends));
    app.dispatch(message("3", "hi"));
    app.dispatch(Action::Open("discord:@me:30".into()));
    assert!(!sends(
        &ready(&mut app, "discord:@me:30"),
        "discord:@me:30",
        "hi"
    ));
    assert!(app.pending_message.is_none());

    // One that couldn't start drops its message.
    app.apply(discord_event(snapshot(&[("30", "2")])));
    app.dispatch(message("3", "hi again"));
    app.apply(Event::DmStarted(Err("Could not start the DM".into())));
    assert!(app.pending_message.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn a_one_to_one_dm_shows_its_persons_profile_beside_it() {
    use yapfast_discord::service::{Conversation, Snapshot, Status};
    let dm = |id: &str, person: &str, people| Conversation {
        id: id.into(),
        name: format!("DM {id}"),
        person: Some(person.into()),
        people,
        group: people > 2,
        ..Default::default()
    };
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        conversations: vec![
            Arc::new(dm("30", "7", 2)),
            Arc::new(dm("31", "8", 2)),
            Arc::new(dm("32", "9", 3)),
        ],
        ..Default::default()
    }));
    assert_eq!(app.state.selected.as_deref(), Some("discord:@me:30"));
    assert!(app.state.panel_profile.is_none(), "hidden until shown");

    let fetches = |effects: &[Effect]| -> Vec<String> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Backend(Command::Discord(DiscordCommand::Profile { user, .. })) => {
                    Some(user.clone())
                }
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        fetches(&app.dispatch(Action::ShowProfilePanel(true))),
        ["7"]
    );
    let view = app.state.panel_profile.as_ref().unwrap();
    assert!(view.loading && view.profile.name == "DM 30");
    app.apply(Event::Profile {
        user: "7".into(),
        result: Ok(Profile {
            id: "7".into(),
            name: "Ann".into(),
            bio: "Hi".into(),
            ..Default::default()
        }),
    });
    assert_eq!(app.state.panel_profile.as_ref().unwrap().profile.bio, "Hi");
    assert!(
        fetches(&app.dispatch(Action::SelectRoom("discord:@me:30".into()))).is_empty(),
        "the same person isn't fetched again"
    );

    // It follows the open DM, and a group DM has none.
    assert_eq!(
        fetches(&app.dispatch(Action::SelectRoom("discord:@me:31".into()))),
        ["8"]
    );
    // A mutual friend opens full size, fetched as their DM shows them.
    app.apply(Event::Profile {
        user: "8".into(),
        result: Ok(Profile {
            id: "8".into(),
            name: "Bea".into(),
            friends: vec![chat::ProfileFriend {
                id: "7".into(),
                name: "Ann".into(),
                avatar: Some("ann.png".into()),
            }],
            ..Default::default()
        }),
    });
    let effects = app.dispatch(Action::ShowFriendProfile {
        user: "7".into(),
        large: false,
    });
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Backend(Command::Discord(DiscordCommand::Profile { room, user })) if room == "discord:@me:30" && user == "7"
    )));
    let view = app.state.profile.as_ref().unwrap();
    assert!(!view.large && view.profile.name == "Ann");
    assert_eq!(view.profile.avatar.as_deref(), Some("ann.png"));
    assert!(
        app.dispatch(Action::ShowFriendProfile {
            user: "99".into(),
            large: true
        })
        .is_empty(),
        "only those listed"
    );

    app.dispatch(Action::SelectRoom("discord:@me:32".into()));
    assert!(app.state.panel_profile.is_none());
    app.dispatch(Action::SelectRoom("discord:@me:31".into()));
    app.dispatch(Action::ShowProfilePanel(false));
    assert!(app.state.panel_profile.is_none());
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
fn discord_dm_calls_start_ring_and_are_answered() {
    use yapfast_discord::service::{Conversation, Snapshot, Status, VoiceMember};
    let person = |id: &str, name: &str| VoiceMember {
        user_id: id.into(),
        name: name.into(),
        avatar: format!("{id}.png"),
        muted: false,
        deafened: false,
        streaming: false,
        video: false,
        elsewhere: false,
        server_muted: false,
        server_deafened: false,
    };
    let snapshot = |ringing: Vec<VoiceMember>, in_call: Vec<VoiceMember>| Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        conversations: vec![
            Arc::new(Conversation {
                id: "30".into(),
                name: "Ann".into(),
                people: 2,
                person: Some("2".into()),
                ringing,
                ..Default::default()
            }),
            Arc::new(Conversation {
                id: "40".into(),
                name: "Erin".into(),
                people: 2,
                request: true,
                ..Default::default()
            }),
        ],
        voice: [("30".to_owned(), in_call)].into(),
        ..Default::default()
    };
    let mut app = Application::default();
    app.apply(discord_event(snapshot(vec![], vec![])));
    let dm = "discord:@me:30".to_owned();
    assert!(app.state.can_join_voice(&dm), "a DM has a call");
    assert!(
        !app.state.can_join_voice("discord:@me:40"),
        "a request waits for an answer"
    );

    // Ann calls: the DM rings once, with a notification, until answered.
    let effects = app.apply(discord_event(snapshot(
        vec![person("1", "Me")],
        vec![person("2", "Ann")],
    )));
    let alerts: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Notify(alert) => Some((alert.call, alert.room.as_str(), alert.title.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(alerts, [(true, dm.as_str(), "Ann")]);
    let room = app.state.rooms.iter().find(|room| room.id == dm).unwrap();
    assert!(room.incoming && room.ringing.is_empty());
    assert_eq!(room.people[0].name, "Ann");
    assert!(
        !app.apply(discord_event(snapshot(
            vec![person("1", "Me")],
            vec![person("2", "Ann")],
        )))
        .iter()
        .any(|effect| matches!(effect, Effect::Notify(_))),
        "a call notifies once"
    );
    assert!(matches!(
        app.dispatch(Action::DeclineCall(dm.clone())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::DeclineCall { room }))] if *room == dm
    ));
    let effects = app.dispatch(Action::AcceptCall(dm.clone()));
    assert_eq!(
        app.state.selected.as_ref(),
        Some(&dm),
        "answering opens the DM"
    );
    assert!(matches!(
        effects.last(),
        Some(Effect::Backend(Command::Voice(voice::Command::Join(id)))) if *id == dm
    ));
}

#[test]
#[cfg(feature = "discord")]
fn do_not_disturb_shows_a_call_without_notifying() {
    use yapfast_discord::service::{Conversation, DO_NOT_DISTURB, Snapshot, Status, VoiceMember};
    let ann = VoiceMember {
        user_id: "2".into(),
        name: "Ann".into(),
        avatar: "2.png".into(),
        muted: false,
        deafened: false,
        streaming: false,
        video: false,
        elsewhere: false,
        server_muted: false,
        server_deafened: false,
    };
    let mut app = Application::default();
    let effects = app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        presence: DO_NOT_DISTURB.into(),
        conversations: vec![Arc::new(Conversation {
            id: "30".into(),
            name: "Ann".into(),
            people: 2,
            person: Some("2".into()),
            ringing: vec![VoiceMember {
                user_id: "1".into(),
                name: "Me".into(),
                ..ann.clone()
            }],
            ..Default::default()
        })],
        voice: [("30".to_owned(), vec![ann])].into(),
        ..Default::default()
    }));
    assert!(app.state.do_not_disturb());
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Notify(_))),
        "Do Not Disturb doesn't notify"
    );
    let room = app
        .state
        .rooms
        .iter()
        .find(|room| room.id == "discord:@me:30");
    assert!(
        room.is_some_and(|room| room.incoming),
        "the call still shows"
    );
}

#[test]
#[cfg(feature = "discord")]
fn links_discord_by_qr_code_and_unlinks_it() {
    use crate::discord_link::Progress;
    use yapfast_discord::service::{Snapshot, Status};
    // Signed in to Matrix, Yapfast links Discord from settings and connects it.
    let mut app = connected();
    assert!(matches!(
        app.dispatch(Action::LinkDiscord).as_slice(),
        [Effect::LinkDiscord]
    ));
    assert_eq!(app.state.discord_link, Some(DiscordLink::Starting));
    app.linking(Progress::Code("https://discord.com/ra/one".into()));
    assert_eq!(
        app.state.discord_link,
        Some(DiscordLink::Code("https://discord.com/ra/one".into()))
    );
    // An expired code makes way for a new one.
    assert!(matches!(
        app.linking(Progress::Expired).as_slice(),
        [Effect::LinkDiscord]
    ));
    assert_eq!(app.state.discord_link, Some(DiscordLink::Starting));
    // Without a code first, it's a failure, not a loop.
    assert!(app.linking(Progress::Expired).is_empty());
    assert!(matches!(
        app.state.discord_link,
        Some(DiscordLink::Failed(_))
    ));
    app.dispatch(Action::LinkDiscord);
    app.linking(Progress::Scanned("alice".into()));
    assert_eq!(
        app.state.discord_link,
        Some(DiscordLink::Scanned("alice".into()))
    );
    // Discord wants a captcha: the browser opens its page, and the dialog
    // can open it again.
    let page = "http://127.0.0.1:9/captcha/1".to_owned();
    assert!(matches!(
        app.linking(Progress::Captcha(page.clone())).as_slice(),
        [Effect::OpenPage(opened)] if *opened == page
    ));
    assert_eq!(
        app.state.discord_link,
        Some(DiscordLink::Captcha(page.clone()))
    );
    assert!(matches!(
        app.dispatch(Action::OpenDiscordCaptcha).as_slice(),
        [Effect::OpenPage(opened)] if *opened == page
    ));
    assert!(matches!(
        app.linking(Progress::Linked).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Start))]
    ));
    assert_eq!(app.state.discord_link, None);
    assert!(app.state.discord_linked);
    assert!(
        app.dispatch(Action::LinkDiscord).is_empty(),
        "already linked"
    );

    // Unlinking forgets everything Discord showed.
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        friends: vec!["2".into()].into(),
        ..Default::default()
    }));
    assert!(matches!(
        app.dispatch(Action::UnlinkDiscord).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Unlink))]
    ));
    app.apply(Event::DiscordUnlinked);
    assert!(!app.state.discord_linked);
    assert_eq!(app.state.discord_user, None);
    assert!(app.state.discord_friends.is_empty());

    // Stopping linking ignores what the stopped linking says next.
    app.dispatch(Action::LinkDiscord);
    assert!(matches!(
        app.dispatch(Action::CancelDiscordLink).as_slice(),
        [Effect::StopDiscordLink]
    ));
    assert!(app.linking(Progress::Linked).is_empty());
    assert!(!app.state.discord_linked);

    // Without a session, as in a build without Matrix, linking signs in.
    let mut app = Application::default();
    app.dispatch(Action::LinkDiscord);
    assert!(matches!(
        app.linking(Progress::Linked).as_slice(),
        [Effect::Start(chat::Start::Restore)]
    ));
    assert!(app.state.busy);
}

#[test]
#[cfg(feature = "discord")]
fn links_discord_by_token() {
    use crate::discord_link::Progress;
    let mut app = Application::default();
    assert!(
        app.dispatch(Action::LinkDiscordToken("  ".into()))
            .is_empty(),
        "nothing pasted"
    );
    assert!(matches!(
        app.dispatch(Action::LinkDiscordToken("abc.def".into())).as_slice(),
        [Effect::LinkDiscordToken(token)] if token == "abc.def"
    ));
    assert_eq!(app.state.discord_link, Some(DiscordLink::Checking));
    app.linking(Progress::Failed("Discord didn't accept that token.".into()));
    assert!(matches!(
        app.state.discord_link,
        Some(DiscordLink::Failed(_))
    ));
    // Pasted again, it signs in as linking by QR code does.
    app.dispatch(Action::LinkDiscordToken("abc.def".into()));
    assert!(matches!(
        app.linking(Progress::Linked).as_slice(),
        [Effect::Start(chat::Start::Restore)]
    ));
    assert!(app.state.discord_linked);
}

#[test]
#[cfg(feature = "discord")]
fn adds_friends_by_username_and_sets_the_custom_status() {
    use yapfast_discord::service::{Snapshot, Status};
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        ..Default::default()
    }));

    app.dispatch(Action::OpenAddFriend);
    assert!(matches!(
        app.dispatch(Action::AddFriend(" @Erin ".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::AddFriend { username }))] if username == "erin"
    ));
    assert!(
        app.dispatch(Action::AddFriend("erin".into())).is_empty(),
        "one request at a time"
    );
    app.apply(Event::FriendAdded(Err(
        "Discord has nobody by that username.".into(),
    )));
    let dialog = app.state.add_friend.as_ref().expect("still open");
    assert!(!dialog.sending);
    assert!(matches!(&dialog.outcome, Some(Err(_))));

    app.dispatch(Action::OpenStatusEditor);
    assert_eq!(
        app.dispatch(Action::SetCustomStatus {
            emoji: Some(":pog:".into()),
            text: "Hi".into(),
            clear_after: ClearAfter::Never,
        })
        .len(),
        0
    );
    assert_eq!(
        app.state
            .status_editor
            .as_ref()
            .and_then(|editor| editor.error.as_deref()),
        Some("A custom emoji in a status needs Discord Nitro.")
    );
    let effects = app.dispatch(Action::SetCustomStatus {
        emoji: Some("🔧".into()),
        text: " Fixing the bridge ".into(),
        clear_after: ClearAfter::Minutes(30),
    });
    let [Effect::Backend(Command::Discord(DiscordCommand::SetCustomStatus(Some(status))))] =
        effects.as_slice()
    else {
        panic!("the status goes to Discord");
    };
    assert_eq!(status.text, "Fixing the bridge");
    assert_eq!(status.emoji.as_deref(), Some("🔧"));
    let left = status.expires_at_ms.unwrap() as i64 - chrono::Local::now().timestamp_millis();
    assert!((29 * 60_000..=30 * 60_000).contains(&left));
    app.apply(Event::CustomStatusSet(Ok(())));
    assert!(app.state.status_editor.is_none());

    // Nothing to say clears it.
    app.dispatch(Action::OpenStatusEditor);
    assert!(matches!(
        app.dispatch(Action::SetCustomStatus {
            emoji: None,
            text: " ".into(),
            clear_after: ClearAfter::Today,
        })
        .as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::SetCustomStatus(None)
        ))]
    ));
}

#[test]
#[cfg(feature = "discord")]
fn discord_dialogs_stop_waiting_when_the_backend_cant_answer() {
    use yapfast_discord::service::{Snapshot, Status};
    let mut app = Application::default();
    app.apply(discord_event(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        ..Default::default()
    }));
    app.dispatch(Action::OpenAddFriend);
    let effects = app.dispatch(Action::AddFriend("erin".into()));
    let [Effect::Backend(command)] = effects.as_slice() else {
        panic!("the request goes to Discord");
    };
    app.backend_failed(command);
    let dialog = app.state.add_friend.as_ref().expect("still open");
    assert!(!dialog.sending);
    assert!(matches!(&dialog.outcome, Some(Err(_))));

    // The backend stopping ends the status editor's and a profile's waits.
    app.dispatch(Action::OpenStatusEditor);
    app.dispatch(Action::SetCustomStatus {
        emoji: None,
        text: "Hi".into(),
        clear_after: ClearAfter::Never,
    });
    assert!(
        app.state
            .status_editor
            .as_ref()
            .is_some_and(|editor| editor.saving)
    );
    app.state.profile = Some(ProfileView {
        profile: Profile {
            id: "2".into(),
            ..Default::default()
        },
        large: false,
        loading: true,
        kept: false,
        note: None,
    });
    app.stopped();
    let editor = app.state.status_editor.as_ref().expect("still open");
    assert!(!editor.saving && editor.error.is_some());
    let profile = app.state.profile.as_ref().expect("still open");
    assert!(!profile.loading && profile.note.is_some());
}

#[test]
fn clears_a_status_today_at_midnight() {
    use chrono::{Datelike, Local, TimeZone, Timelike};
    let evening = Local.with_ymd_and_hms(2026, 10, 7, 21, 30, 0).unwrap();
    let at = ClearAfter::Today.at(evening).unwrap();
    let midnight = Local.timestamp_millis_opt(at as i64).unwrap();
    assert_eq!(
        (midnight.day(), midnight.hour(), midnight.minute()),
        (8, 0, 0)
    );
    assert_eq!(ClearAfter::Never.at(evening), None);
}

#[test]
#[cfg(feature = "discord")]
fn shares_the_game_as_the_account_settings_say() {
    use yapfast_discord::{
        own_activity::Playing,
        service::{Snapshot, Status},
    };
    let mut app = Application::default();
    let snapshot = |show_game: bool, playing: Option<&str>| Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        show_game,
        playing: playing.map(|name| {
            Arc::new(Playing {
                name: name.into(),
                started_ms: Some(5),
            })
        }),
        ..Default::default()
    };
    app.apply(discord_event(snapshot(true, Some("Game"))));
    assert!(app.state.discord_show_game);
    assert_eq!(
        app.state.discord_playing,
        Some(chat::Playing {
            name: "Game".into(),
            started_ms: Some(5)
        })
    );
    assert!(
        app.dispatch(Action::ShowGame(true)).is_empty(),
        "it's shared already"
    );
    assert!(matches!(
        app.dispatch(Action::ShowGame(false)).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ShowGame(
            false
        )))]
    ));
    assert!(!app.state.discord_show_game, "the switch moves at once");
    app.apply(Event::ShowGameSet(Err("Discord said no.".into())));
    assert!(app.state.discord_show_game, "and back when Discord refuses");
    assert!(app.state.notice.is_some());
    app.apply(discord_event(snapshot(false, None)));
    assert!(!app.state.discord_show_game);
    assert_eq!(app.state.discord_playing, None);
}

#[test]
#[cfg(feature = "discord")]
fn sets_the_status_and_holds_notifications_while_busy() {
    use yapfast_discord::service::{Snapshot, Status};
    let mut app = Application::default();
    let snapshot = |presence: &str| Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        presence: presence.into(),
        ..Default::default()
    };
    app.apply(discord_event(snapshot("online")));
    assert!(
        app.dispatch(Action::SetStatus("online".into())).is_empty(),
        "it's already online"
    );
    assert!(matches!(
        app.dispatch(Action::SetStatus("dnd".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::SetStatus(status)))] if status == "dnd"
    ));
    app.apply(Event::StatusSet(Ok(())));
    app.apply(discord_event(snapshot("dnd")));
    assert_eq!(app.state.discord_presence.as_deref(), Some("dnd"));
    let notify = |app: &mut Application| {
        app.apply(Event::Notify(chat::Notification {
            room: "dm".into(),
            author: "Sam".into(),
            body: "Around?".into(),
            avatar: None,
            mention: false,
        }))
    };
    assert!(
        notify(&mut app).is_empty(),
        "Do Not Disturb holds them back"
    );
    app.apply(discord_event(snapshot("idle")));
    assert_eq!(notify(&mut app).len(), 1);
    app.apply(Event::StatusSet(Err("Discord said no.".into())));
    assert_eq!(
        app.state.notice.as_deref(),
        Some("Couldn't change your status: Discord said no.")
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
            vec![Message, CopyId],
            vec![AcceptFriend, CopyId],
            vec![CancelFriendRequest, CopyId],
            vec![AddFriend, CopyId],
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

#[test]
fn custom_emoji_are_requested_only_for_nearby_messages_and_stale_views_are_ignored() {
    let asked = |effects: Vec<Effect>| -> Vec<String> {
        effects
            .into_iter()
            .flat_map(|effect| match effect {
                Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls))) => urls,
                _ => vec![],
            })
            .collect()
    };
    let mut app = connected();
    let messages: Vec<_> = (0..100)
        .map(|index| ChatMessage {
            emoji: vec![chat::CustomEmoji {
                picture: format!("emoji/{index}"),
                text: ":test:".into(),
                id: index.to_string(),
                animated: false,
            }],
            ..message(&format!("e:${index}"), false)
        })
        .collect();
    assert!(
        asked(app.apply(Event::Timeline {
            room: "dm".into(),
            messages: timeline(messages)
        }))
        .is_empty()
    );
    let view = |room: &str, from, until| Action::VisibleMessages {
        room: room.into(),
        items: (from..until).map(|index| format!("e:${index}")).collect(),
        at_end: false,
    };
    assert_eq!(asked(app.dispatch(view("dm", 90, 100))).len(), 10);
    assert!(asked(app.dispatch(view("dm", 90, 100))).is_empty());
    assert!(app.dispatch(view("old-room", 0, 10)).is_empty());
    assert_eq!(asked(app.dispatch(view("dm", 0, 10))).len(), 10);
    assert!(asked(app.dispatch(Action::SetDraft("Hi".into()))).is_empty());
}

#[test]
fn closed_channels_keep_metadata_without_pinning_previews() {
    let picture = Arc::new(Picture::default());
    let with_picture = |index: usize| ChatMessage {
        attachments: vec![crate::chat::Attachment {
            name: "a.png".into(),
            size: None,
            image: true,
            dimensions: None,
            preview: Some(picture.clone()),
            preview_failed: false,
            placement: crate::chat::Placement::Message,
            gif: None,
            spoiler: false,
            video: false,
            url: None,
        }],
        ..message(&format!("e:${index}"), false)
    };
    let rows: Vec<_> = (0..discord::KEPT_MESSAGES + 50)
        .map(|id| Arc::new(with_picture(id)))
        .collect();
    let kept = shelved(&rows);
    assert_eq!(kept.len(), discord::KEPT_MESSAGES);
    assert_eq!(kept[0].id, "e:$50", "the oldest went");
    let previews: Vec<bool> = kept
        .iter()
        .map(|message| message.attachments[0].preview.is_some())
        .collect();
    assert!(
        previews.iter().all(|shown| !shown),
        "remembered UI metadata does not pin decoded previews"
    );
    let plain = [Arc::new(message("e:$plain", false))];
    assert!(
        Arc::ptr_eq(&shelved(&plain)[0], &plain[0]),
        "rows without previews aren't copied"
    );
}

#[test]
fn pictures_off_screen_go_once_many_have_arrived() {
    let mut app = connected();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![ChatMessage {
            avatar: Some("ann.png".into()),
            ..message("e:$ann", false)
        }]),
    });
    let arrive = |app: &mut Application, url: String| {
        app.apply(Event::Picture {
            url,
            picture: Arc::new(Picture::default()),
        });
    };
    arrive(&mut app, "ann.png".into());
    // The emoji picker asks for a server's custom emoji all at once.
    let emoji = "https://cdn.discordapp.com/emojis/5.png?size=64";
    app.requested_pictures.insert(emoji.into());
    arrive(&mut app, emoji.into());
    // The last of these is one more than are kept, the emoji aside.
    for index in 0..KEPT_PICTURES {
        app.requested_pictures.insert(format!("{index}.png"));
        arrive(&mut app, format!("{index}.png"));
    }
    assert_eq!(
        app.state.pictures.len(),
        2,
        "all but the one on screen and the custom emoji went"
    );
    assert!(app.state.pictures.contains_key("ann.png"));
    assert!(app.state.pictures.contains_key(emoji));
    assert!(app.requested_pictures.contains(emoji));
    assert!(
        !app.requested_pictures.contains("1.png"),
        "asked for again when shown"
    );
    assert_eq!(app.state.picture_changes, KEPT_PICTURES as u64 + 2);
}

#[test]
fn avatars_stay_only_near_the_rows_on_screen() {
    let mut app = connected();
    // A busy channel followed for a while: each message someone else's.
    let messages: Vec<_> = (0..300)
        .map(|index| ChatMessage {
            avatar: Some(format!("{index}.png")),
            ..message(&format!("e:${index}"), false)
        })
        .collect();
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(messages),
    });
    app.dispatch(Action::VisibleMessages {
        room: "dm".into(),
        items: (280..290).map(|index| format!("e:${index}")).collect(),
        at_end: false,
    });
    for index in 0..=KEPT_PICTURES {
        app.requested_pictures.insert(format!("{index}.png"));
        app.apply(Event::Picture {
            url: format!("{index}.png"),
            picture: Arc::new(Picture::default()),
        });
    }
    let kept = |index: usize| app.state.pictures.contains_key(&format!("{index}.png"));
    assert!(kept(280) && kept(289), "those on screen stay");
    assert!(kept(250) && kept(299), "as do those a short scroll away");
    assert!(!kept(249) && !kept(0), "the rest of the timeline's go");
    assert_eq!(app.state.pictures.len(), 50);
}

#[test]
fn custom_emoji_kept_do_not_let_other_pictures_pile_up() {
    let mut app = connected();
    let mut arrive = |url: String| {
        app.requested_pictures.insert(url.clone());
        app.apply(Event::Picture {
            url,
            picture: Arc::new(Picture::default()),
        });
    };
    // A few servers' emoji, as the picker asks for them.
    for index in 0..2 * KEPT_PICTURES {
        arrive(format!("https://cdn.discordapp.com/emojis/{index}.png"));
    }
    for index in 0..=KEPT_PICTURES {
        arrive(format!("{index}.png"));
    }
    assert_eq!(
        app.state.pictures.len(),
        2 * KEPT_PICTURES,
        "only the emoji stay"
    );
}

#[cfg(feature = "discord")]
fn unread_channel(unread: bool, last_read: u64) -> Event {
    use yapfast_discord::{
        ready::{Channel, Server},
        service::{Snapshot, Status},
    };
    discord_event(Snapshot {
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
            &[(
                "21",
                ChannelMarks {
                    unread,
                    last_read,
                    ..Default::default()
                },
            )],
        )],
        ..Default::default()
    })
}

/// Bob's Discord messages with these snowflakes, and Alice's where `own`.
#[cfg(feature = "discord")]
fn discord_messages(ids: impl IntoIterator<Item = u64>, own: &[u64]) -> Vec<ChatMessage> {
    ids.into_iter()
        .map(|id| message(&format!("d:{id}"), own.contains(&id)))
        .collect()
}

#[test]
#[cfg(feature = "discord")]
fn the_new_divider_marks_the_first_unread_message_until_the_channel_is_left() {
    let general = "discord:10:21";
    let mut app = connected();
    app.apply(unread_channel(true, 103));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom(general.into()));
    // The backend answers the messages around the last read one from
    // what it kept, which reaches it.
    app.apply(window(general, chat::Direction::Around, true));
    assert_eq!(app.state.first_unread, None);
    // The view still at the end of what showed before reads nothing: the
    // divider isn't placed yet.
    assert!(app.dispatch(Action::Reading(true)).is_empty());
    let effects = app.apply(Event::Timeline {
        room: general.into(),
        messages: timeline(discord_messages(95..=110, &[104])),
    });
    assert!(
        effects.is_empty(),
        "the last read message is loaded, so no earlier history is, and \
         the view goes to the divider, not reading the channel yet"
    );
    // Alice's own message after the last read one isn't new to her.
    assert_eq!(app.state.first_unread.as_deref(), Some("d:105"));
    assert_eq!(app.state.unread_jump, 1, "shown once");
    assert!(!app.state.reading && !app.state.caught_up);
    assert!(
        !app.apply(unread_channel(true, 103))
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::MarkRead { .. }))),
        "news of the channel doesn't read it either"
    );

    // Read down to the newest message, the channel is read, and the bar
    // back to the divider goes.
    app.dispatch(Action::Reading(false));
    assert!(matches!(
        app.dispatch(Action::Reading(true)).as_slice(),
        [Effect::Backend(Command::MarkRead { room, .. })] if room == general
    ));
    assert!(app.state.caught_up);

    // Reading the channel and newer messages leave the divider where it is.
    app.apply(unread_channel(false, 110));
    app.apply(Event::Timeline {
        room: general.into(),
        messages: timeline(discord_messages(95..=111, &[104])),
    });
    assert_eq!(app.state.first_unread.as_deref(), Some("d:105"));
    assert_eq!(app.state.unread_jump, 1);
    assert!(app.dispatch(Action::JumpToUnread).is_empty());
    assert_eq!(app.state.unread_jump, 2);
    assert_eq!(app.state.jump, None, "the jump doesn't highlight");

    // Left and read, the channel has no divider when it opens again.
    app.dispatch(Action::SelectRoom("discord:10:22".into()));
    assert_eq!(app.state.first_unread, None);
    assert!(!app.state.caught_up);
    app.dispatch(Action::SelectRoom(general.into()));
    // The backend answers the messages around the last read one from
    // what it kept, which reaches it.
    app.apply(window(general, chat::Direction::Around, true));
    app.apply(Event::Timeline {
        room: general.into(),
        messages: timeline(discord_messages(95..=111, &[104])),
    });
    assert_eq!(app.state.first_unread, None);
    assert!(app.dispatch(Action::JumpToUnread).is_empty());
    assert_eq!(app.state.unread_jump, 2);
}

#[test]
#[cfg(feature = "discord")]
fn a_channel_whose_new_messages_dont_show_reads_once_its_newest_came() {
    let general = "discord:10:21";
    let latest = |history| Event::HistoryLoaded {
        room: general.into(),
        direction: chat::Direction::Latest,
        history,
    };
    let reads = |effects: &[Effect]| {
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::MarkRead { room, .. }) if room == general))
    };
    let mut app = connected();
    // Discord's newest message is 104, deleted, of a kind Yapfast doesn't
    // show or hidden as blocked.
    app.apply(unread_channel(true, 103));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom(general.into()));
    // The backend answers the messages around the last read one from
    // what it kept, which reaches it.
    app.apply(window(general, chat::Direction::Around, true));
    app.dispatch(Action::Reading(true));
    // What was kept on disk shows first; newer messages may come with the
    // newest page.
    let effects = app.apply(Event::Timeline {
        room: general.into(),
        messages: timeline(discord_messages(95..=103, &[])),
    });
    assert!(!reads(&effects));
    assert!(!reads(&app.apply(latest(chat::History::Failed))));
    assert!(!reads(&app.dispatch(Action::Reading(true))));

    // The newest page came with nothing new: no divider, and the channel reads.
    app.apply(Event::Timeline {
        room: general.into(),
        messages: timeline(discord_messages(95..=103, &[])),
    });
    let effects = app.apply(latest(chat::History::Loaded {
        start: false,
        present: true,
    }));
    assert!(reads(&effects));
    assert_eq!(app.state.first_unread, None);
    assert!(app.state.caught_up);

    // When it brings new messages, the divider goes above them as before,
    // and the view goes there first.
    app.dispatch(Action::SelectRoom("discord:10:22".into()));
    app.apply(unread_channel(true, 103));
    app.dispatch(Action::SelectRoom(general.into()));
    // The backend answers the messages around the last read one from
    // what it kept, which reaches it.
    app.apply(window(general, chat::Direction::Around, true));
    app.dispatch(Action::Reading(true));
    app.apply(Event::Timeline {
        room: general.into(),
        messages: timeline(discord_messages(95..=106, &[])),
    });
    let effects = app.apply(latest(chat::History::Loaded {
        start: false,
        present: true,
    }));
    assert!(!reads(&effects));
    assert_eq!(app.state.first_unread.as_deref(), Some("d:104"));
}

#[test]
#[cfg(feature = "discord")]
fn opening_a_channel_shows_the_messages_around_its_last_read_one() {
    let general = "discord:10:21";
    let around = |effects: &[Effect]| {
        effects.iter().find_map(|effect| match effect {
            Effect::Backend(Command::Around { room, item }) if room == general => {
                Some(item.clone())
            }
            _ => None,
        })
    };
    let show = |app: &mut Application, ids: Vec<u64>| {
        app.apply(Event::Timeline {
            room: general.into(),
            messages: timeline(discord_messages(ids, &[])),
        })
    };
    let mut app = connected();
    app.apply(unread_channel(true, 50));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    let effects = app.dispatch(Action::SelectRoom(general.into()));
    let open = effects.iter().position(
        |effect| matches!(effect, Effect::Backend(Command::Open(room)) if room == general),
    );
    assert!(open.is_some(), "it opens");
    assert_eq!(
        around(&effects).as_deref(),
        Some("d:50"),
        "nothing shows yet, so the messages around the last read one load with the newest"
    );
    let effects = show(&mut app, (100..110).collect());
    assert!(around(&effects).is_none(), "asked once");
    assert_eq!(app.state.first_unread, None);
    // Until the window comes, timelines are of what showed before.
    assert!(around(&show(&mut app, (100..111).collect())).is_none());
    assert_eq!(app.state.first_unread, None);
    app.apply(window(general, chat::Direction::Around, false));
    // Messages 49 and 50 were deleted: the divider goes above the next one.
    let effects = show(&mut app, (40..48).chain(100..110).collect());
    assert!(around(&effects).is_none());
    assert_eq!(app.state.first_unread.as_deref(), Some("d:100"));
    assert_eq!(app.state.unread_jump, 1);
    assert!(app.state.detached);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::MarkRead { .. }))),
        "a window of older messages doesn't read the channel"
    );

    // Scrolled on to the present, the first unread message was let go;
    // the jump loads the messages around it again.
    app.apply(window(general, chat::Direction::Later, true));
    show(&mut app, (105..120).collect());
    assert_eq!(app.state.first_unread.as_deref(), Some("d:100"), "it stays");
    assert_eq!(
        around(&app.dispatch(Action::JumpToUnread)).as_deref(),
        Some("d:100")
    );
    app.apply(window(general, chat::Direction::Around, false));
    show(&mut app, (95..110).collect());
    assert_eq!(app.state.unread_jump, 2);
    assert_eq!(app.state.jump, None, "the jump doesn't highlight");

    // When even the messages around the last read one are all newer, the
    // divider goes above the oldest loaded.
    app.dispatch(Action::SelectRoom("discord:10:22".into()));
    app.apply(unread_channel(true, 1));
    assert!(around(&app.dispatch(Action::SelectRoom(general.into()))).is_some());
    show(&mut app, (1000..1010).collect());
    app.apply(window(general, chat::Direction::Around, false));
    assert!(around(&show(&mut app, (1000..1010).collect())).is_none());
    assert_eq!(app.state.first_unread.as_deref(), Some("d:1000"));
}

#[test]
fn a_matrix_room_divider_follows_its_read_receipt_once_loaded() {
    let mut app = connected();
    app.apply(Event::Rooms(RoomIndex {
        rooms: vec![
            preview_room("dm", "DM", true, 2),
            ChatRoom {
                unread: 2,
                last_read: Some("e:2".into()),
                ..preview_room("general", "general", false, 3)
            },
        ],
        servers: vec![ChatServer {
            id: "friends".into(),
            name: "Friends".into(),
            channels: vec!["general".into()],
            discord: false,
            muted: false,
            unavailable: false,
            icon: None,
            folder: None,
            hide_muted: false,
            community: false,
            pinned: Vec::new(),
        }],
    }));
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    app.dispatch(Action::SelectRoom("general".into()));
    let messages = |items: &[&str]| items.iter().map(|item| message(item, false)).collect();
    // The receipt's message isn't loaded: Matrix can't tell where it is,
    // so no history loads for it.
    let effects = app.apply(Event::Timeline {
        room: "general".into(),
        messages: timeline(messages(&["e:3", "e:4"])),
    });
    assert!(effects.is_empty());
    assert_eq!(app.state.first_unread, None);
    app.apply(Event::Timeline {
        room: "general".into(),
        messages: timeline(messages(&["e:1", "e:2", "e:3", "e:4"])),
    });
    assert_eq!(app.state.first_unread.as_deref(), Some("e:3"));
    assert_eq!(app.state.unread_jump, 1);
}

/// Which of the selected room's or `aside`'s messages the timeline
/// highlights, by item.
fn highlighted(app: &Application, aside: bool) -> Vec<String> {
    let (messages, seen) = match (&app.state.aside, aside) {
        (Some(aside), true) => (&aside.messages, aside.seen.as_deref()),
        _ => (&app.state.messages, app.state.seen.as_deref()),
    };
    let first_new = chat::first_new(messages, seen);
    messages
        .iter()
        .enumerate()
        .filter(|(index, message)| message.highlighted(*index >= first_new))
        .map(|(_, message)| message.item.clone())
        .collect()
}

/// Bob's messages with these items, replying to Alice's where `replies`
/// says and mentioning her where `mentions` does.
fn calling(items: &[&str], replies: &[&str], mentions: &[&str]) -> Vec<Arc<ChatMessage>> {
    timeline(
        items
            .iter()
            .map(|item| ChatMessage {
                replies_to_me: replies.contains(item),
                mentions_me: mentions.contains(item),
                ..message(item, false)
            })
            .collect(),
    )
}

#[test]
#[cfg(feature = "discord")]
fn replies_to_this_account_are_highlighted_while_new_and_mentions_always() {
    let general = "discord:10:21";
    let replies = ["d:100", "d:106", "d:111"];
    let mentions = ["d:99"];
    let show = |app: &mut Application, last: &str| {
        let items: Vec<String> = (95..=last.parse().unwrap())
            .map(|id| format!("d:{id}"))
            .collect();
        let items: Vec<&str> = items.iter().map(String::as_str).collect();
        app.apply(Event::Timeline {
            room: general.into(),
            messages: calling(&items, &replies, &mentions),
        });
    };
    let mut app = connected();
    app.apply(unread_channel(true, 103));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom(general.into()));
    // The backend answers the messages around the last read one from
    // what it kept, which reaches it.
    app.apply(window(general, chat::Direction::Around, true));
    show(&mut app, "110");
    assert_eq!(app.state.first_unread.as_deref(), Some("d:104"));
    // A reply from the "New" divider down is highlighted, an older one
    // isn't, and a mention is however old.
    assert_eq!(highlighted(&app, false), ["d:99", "d:106"]);
    // One arriving while the channel is open is new too, even once read.
    app.apply(unread_channel(false, 110));
    show(&mut app, "111");
    assert_eq!(highlighted(&app, false), ["d:99", "d:106", "d:111"]);

    // Opened again with nothing unread, only what comes after is new.
    app.dispatch(Action::SelectRoom("discord:10:22".into()));
    app.apply(unread_channel(false, 111));
    app.dispatch(Action::SelectRoom(general.into()));
    // The backend answers the messages around the last read one from
    // what it kept, which reaches it.
    app.apply(window(general, chat::Direction::Around, true));
    show(&mut app, "111");
    assert_eq!(app.state.first_unread, None);
    assert_eq!(highlighted(&app, false), ["d:99"]);
    let mut replies = replies.to_vec();
    replies.push("d:112");
    app.apply(Event::Timeline {
        room: general.into(),
        messages: calling(&["d:110", "d:111", "d:112"], &replies, &mentions),
    });
    assert_eq!(highlighted(&app, false), ["d:112"]);
}

#[test]
fn a_matrix_rooms_new_replies_follow_its_receipt_or_its_newest_message() {
    let mut app = connected();
    let rooms = |last_read: &str| {
        Event::Rooms(RoomIndex {
            rooms: vec![
                ChatRoom {
                    last_read: Some(last_read.into()),
                    ..preview_room("dm", "DM", true, 2)
                },
                preview_room("other", "Other", true, 2),
            ],
            servers: vec![],
        })
    };
    let show = |app: &mut Application, items: &[&str]| {
        app.apply(Event::Timeline {
            room: "dm".into(),
            messages: calling(items, &["e:1", "e:3", "e:5"], &[]),
        });
    };
    // Its receipt is on the second message: the third is new.
    app.apply(rooms("e:2"));
    app.dispatch(Action::SelectRoom("dm".into()));
    show(&mut app, &["e:1", "e:2", "e:3"]);
    assert_eq!(highlighted(&app, false), ["e:3"]);

    // A receipt the timeline doesn't have, as on an event it leaves out,
    // makes what's newest when it opens the last message seen.
    app.dispatch(Action::SelectRoom("other".into()));
    app.apply(rooms("e:0"));
    app.dispatch(Action::SelectRoom("dm".into()));
    show(&mut app, &["e:1", "e:2", "e:3"]);
    assert!(highlighted(&app, false).is_empty());
    show(&mut app, &["e:1", "e:2", "e:3", "e:4", "e:5"]);
    assert_eq!(highlighted(&app, false), ["e:5"]);
}

#[test]
#[cfg(feature = "discord")]
fn the_thread_beside_highlights_new_replies_from_its_own_read_point() {
    use yapfast_discord::{
        ready::{Channel, Server, Thread},
        service::{Snapshot, Status},
    };
    let snapshot = |last_read: u64| {
        discord_event(Snapshot {
            status: Status::Online,
            servers: vec![marked(
                Server {
                    id: "10".into(),
                    name: "Test".into(),
                    text_channels: vec![Channel {
                        id: "21".into(),
                        name: "general".into(),
                        ..Default::default()
                    }],
                    threads: vec![Channel {
                        id: "30".into(),
                        name: "Plans".into(),
                        parent: Some("21".into()),
                        thread: Some(Box::new(Thread {
                            joined: true,
                            ..Default::default()
                        })),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                &[(
                    "30",
                    ChannelMarks {
                        last_read,
                        ..Default::default()
                    },
                )],
            )],
            ..Default::default()
        })
    };
    let show = |app: &mut Application, items: &[&str]| {
        app.apply(Event::Timeline {
            room: "discord:10:30".into(),
            messages: calling(items, &["d:5", "d:7", "d:9"], &["d:4"]),
        });
    };
    let mut app = connected();
    app.apply(snapshot(6));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    app.dispatch(Action::OpenAside("discord:10:30".into()));
    show(&mut app, &["d:4", "d:5", "d:6", "d:7"]);
    assert_eq!(highlighted(&app, true), ["d:4", "d:7"]);
    show(&mut app, &["d:4", "d:5", "d:6", "d:7", "d:8", "d:9"]);
    assert_eq!(highlighted(&app, true), ["d:4", "d:7", "d:9"]);

    // Never read, it has its newest message as its read point.
    app.dispatch(Action::CloseAside);
    app.apply(snapshot(0));
    app.dispatch(Action::OpenAside("discord:10:30".into()));
    show(&mut app, &["d:4", "d:5", "d:6", "d:7"]);
    assert_eq!(highlighted(&app, true), ["d:4"]);
    show(&mut app, &["d:4", "d:5", "d:6", "d:7", "d:8", "d:9"]);
    assert_eq!(highlighted(&app, true), ["d:4", "d:9"]);
}

/// Alice's Discord DMs: one with Annie, and a group with Annie and Ben. Her
/// friends are Annie, Ben and Cara.
#[cfg(feature = "discord")]
fn discord_dms(extra: Vec<yapfast_discord::service::Conversation>) -> Application {
    use yapfast_discord::service::{Conversation, Friend, Snapshot, Status};
    let friend = |id: &str, name: &str| Friend {
        user_id: id.into(),
        name: name.into(),
        avatar: String::new(),
        nickname: None,
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
        room: dm.clone(),
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
        [Effect::Backend(Command::Discord(DiscordCommand::Search { place, query, offset: 0 }))]
            if *place == dm && query == "trip"
    ));
    let page = |from: u32, count: u32| -> Vec<chat::SearchHit> {
        (from..from + count)
            .map(|n| chat::SearchHit {
                room: dm.clone(),
                message: message(&format!("d:{n}"), false),
            })
            .collect()
    };
    let found = |query: &str, offset: u32, result| Event::Found {
        place: dm.clone(),
        query: query.into(),
        offset,
        result,
    };
    app.apply(found("old", 0, Ok((1, page(0, 1)))));
    assert!(
        app.state.search.as_ref().unwrap().results.is_empty(),
        "an earlier search's results don't show"
    );
    app.apply(found("trip", 0, Ok((30, page(0, 25)))));
    let search = app.state.search.as_ref().unwrap();
    assert!(!search.loading && search.more());
    assert_eq!((search.total, search.results.len()), (30, 25));

    assert!(matches!(
        app.dispatch(Action::MoreResults).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Search {
            offset: 25,
            ..
        }))]
    ));
    assert!(
        app.dispatch(Action::MoreResults).is_empty(),
        "one page at a time"
    );
    // A message sent meanwhile moves one already shown onto the next page.
    app.apply(found("trip", 25, Ok((51, page(24, 25)))));
    let search = app.state.search.as_ref().unwrap();
    assert_eq!(search.results.len(), 49, "each result shows once");
    assert!(search.more());
    app.dispatch(Action::MoreResults);
    app.apply(found("trip", 50, Err("Discord is busy.".into())));
    let search = app.state.search.as_ref().unwrap();
    assert_eq!(search.error.as_deref(), Some("Discord is busy."));
    assert_eq!(search.results.len(), 49, "a failed page keeps those found");
    app.dispatch(Action::MoreResults);
    app.apply(found("trip", 50, Ok((51, page(49, 2)))));
    assert!(
        !app.state.search.as_ref().unwrap().more(),
        "all 51 are there"
    );

    app.dispatch(Action::Search(" ".into()));
    assert!(app.state.search.is_none(), "an empty search ends it");
}

#[test]
#[cfg(feature = "discord")]
fn friends_added_to_a_dm_make_a_group_that_opens() {
    use yapfast_discord::service::Conversation;
    let mut app = discord_dms(vec![]);
    let dm = "discord:@me:30".to_owned();
    app.dispatch(Action::SelectRoom(dm.clone()));
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
    let snapshot = discord_dms(vec![group]).discord.clone().unwrap();
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
    app.dispatch(Action::SelectRoom("discord:10:14".into()));
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
    };
    app.apply(Event::Found {
        place: "discord:10".into(),
        query: "plan".into(),
        offset: 0,
        result: Ok((
            2,
            vec![hit("discord:10:13", "d:7"), hit("discord:10:11", "d:5")],
        )),
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
    app.dispatch(Action::SelectRoom("discord:10:14".into()));
    assert!(app.state.search.is_none());
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
            LeaveServer
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
    assert!(app.header_choices("discord:10").is_empty());
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
    change_server(&mut app, |server| {
        let bits =
            permission::VIEW_CHANNEL | permission::SEND_MESSAGES | permission::MANAGE_CHANNELS;
        server.put_role(
            &serde_json::json!({"id": "10", "name": "@everyone", "position": 0,
                                            "permissions": bits.to_string()}),
        );
    });
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
            LeaveServer
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

#[test]
#[cfg(feature = "discord")]
fn a_server_profile_is_edited_as_discords_per_server_profile_editor_does() {
    use MenuChoice::*;
    use chat::{OwnProfile, PickedPicture, ProfilePicture};
    let mut app = discord_server();
    let choices = app.menu_choices("discord:10");
    let at = |choice| choices.iter().position(|known| *known == choice);
    assert!(
        at(HideMuted) < at(EditServerProfile) && at(EditServerProfile) < at(CopyLink),
        "anyone may, though she may not change her nickname"
    );
    assert_eq!(
        app.header_choices("discord:10"),
        [
            ServerSettings,
            ServerNotifications,
            EditServerProfile,
            HideMuted,
            LeaveServer
        ],
        "as Discord's dropdown orders them"
    );

    // Opening it asks Discord for her profile there.
    assert!(matches!(
        app.dispatch(Action::EditServerProfile("discord:10:11".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::OwnProfile { server }))]
            if server.as_deref() == Some("discord:10")
    ));
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(editor.loading() && !editor.may_rename && !editor.nitro());
    assert_eq!(editor.server_name, "Test");
    let profile = OwnProfile {
        name: String::new(),
        pronouns: "she/her".into(),
        bio: String::new(),
        avatar: Some("server-avatar.png".into()),
        banner: None,
        accent: None,
        fallback: chat::Profile {
            name: "Alice".into(),
            avatar: Some("alice.png".into()),
            ..Default::default()
        },
    };
    let pictures = |app: &mut Application| {
        app.apply(Event::OwnProfile {
            server: Some("discord:10".into()),
            result: Ok(profile.clone()),
        })
        .into_iter()
        .flat_map(|effect| match effect {
            Effect::Backend(Command::Discord(DiscordCommand::Pictures(urls))) => urls,
            _ => vec![],
        })
        .collect::<Vec<_>>()
    };
    assert_eq!(
        pictures(&mut app),
        ["server-avatar.png", "alice.png"],
        "the server's avatar and the main profile's, through the picture cache"
    );
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(editor.pronouns, "she/her");
    assert!(editor.change().is_none());

    // Without Nitro, she changes her pronouns, not her bio or pictures.
    for (field, text) in [
        (ProfileText::Name, "Ally"),
        (ProfileText::Pronouns, "they/them "),
        (ProfileText::Bio, "Hi"),
    ] {
        app.dispatch(Action::WriteProfile {
            field,
            text: text.into(),
        });
    }
    assert!(
        app.dispatch(Action::PickProfilePicture {
            kind: ProfilePicture::Avatar,
            path: "cat.png".into(),
        })
        .is_empty()
    );
    let saved = |effects: Vec<Effect>| match effects.as_slice() {
        [Effect::Backend(Command::Discord(DiscordCommand::SaveProfile { server, change }))] => {
            assert_eq!(server.as_deref(), Some("discord:10"));
            change.clone()
        }
        other => panic!("{} effects", other.len()),
    };
    let change = saved(app.dispatch(Action::SaveProfile));
    assert_eq!(change.pronouns.as_deref(), Some("they/them"));
    assert!(change.name.is_none() && change.bio.is_none() && change.avatar.is_none());
    assert!(app.dispatch(Action::SaveProfile).is_empty(), "once");
    app.apply(Event::ProfileSaved {
        server: Some("discord:10".into()),
        result: Err("Discord said no".into()),
    });
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(!editor.saving && editor.error.as_deref() == Some("Discord said no"));
    app.dispatch(Action::CloseProfileEditor);
    assert!(app.state.profile_editor.is_none());

    // With Nitro and Change Nickname, everything; a new avatar, then the
    // banner left to the main profile's.
    change_server(&mut app, |server| {
        use yapfast_discord::ready::permission;
        let bits =
            permission::VIEW_CHANNEL | permission::SEND_MESSAGES | permission::CHANGE_NICKNAME;
        server.put_role(
            &serde_json::json!({"id": "10", "name": "@everyone", "position": 0,
                                            "permissions": bits.to_string()}),
        );
    });
    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    snapshot.account = Some(yapfast_discord::service::Account {
        name: "Alice".into(),
        avatar: "alice.png".into(),
        premium_type: 2,
        mfa_enabled: false,
        has_email: true,
    });
    app.apply(discord_event(snapshot));
    app.dispatch(Action::EditServerProfile("discord:10".into()));
    pictures(&mut app);
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(editor.may_rename && editor.nitro() && !editor.upgrade());
    app.dispatch(Action::WriteProfile {
        field: ProfileText::Name,
        text: "Ally".into(),
    });
    app.dispatch(Action::WriteProfile {
        field: ProfileText::Bio,
        text: "x".repeat(BIO_LIMIT + 1),
    });
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(editor.too_long(), Some(ProfileText::Bio));
    assert!(app.dispatch(Action::SaveProfile).is_empty(), "too long");
    app.dispatch(Action::WriteProfile {
        field: ProfileText::Bio,
        text: "Here for #general.".into(),
    });
    assert!(matches!(
        app.dispatch(Action::PickProfilePicture {
            kind: ProfilePicture::Avatar,
            path: "cat.png".into(),
        })
        .as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::ProfilePicture {
                kind: ProfilePicture::Avatar,
                ..
            }
        ))]
    ));
    assert!(
        app.dispatch(Action::SaveProfile).is_empty(),
        "while it reads"
    );
    let picked = PickedPicture {
        mime: "image/png",
        data: Arc::from(&b"png"[..]),
        preview: Arc::default(),
    };
    app.apply(Event::ProfilePicture {
        kind: ProfilePicture::Avatar,
        path: "cat.png".into(),
        result: Ok(picked),
    });
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(editor.picture(ProfilePicture::Avatar).1.is_some());
    assert!(editor.resettable(ProfilePicture::Avatar));
    assert!(
        !editor.resettable(ProfilePicture::Banner),
        "it has none of its own"
    );
    app.dispatch(Action::ResetProfilePicture(ProfilePicture::Banner));
    let change = saved(app.dispatch(Action::SaveProfile));
    assert_eq!(change.name.as_deref(), Some("Ally"));
    assert_eq!(change.bio.as_deref(), Some("Here for #general."));
    assert!(matches!(&change.avatar, Some(Some(picked)) if &*picked.data == b"png"));
    assert!(change.banner.is_none(), "nothing to reset");
    app.apply(Event::ProfileSaved {
        server: Some("discord:10".into()),
        result: Ok(()),
    });
    assert!(
        app.state.profile_editor.is_none(),
        "its pictures go with it"
    );

    // Resetting the server avatar sends none, so the main one shows.
    app.dispatch(Action::EditServerProfile("discord:10".into()));
    pictures(&mut app);
    app.dispatch(Action::ResetProfilePicture(ProfilePicture::Avatar));
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(editor.picture(ProfilePicture::Avatar), (None, None));
    let change = saved(app.dispatch(Action::SaveProfile));
    assert!(matches!(change.avatar, Some(None)));
    assert!(matches!(
        app.dispatch(Action::UnlockNitro).as_slice(),
        [Effect::OpenPage(page)] if page == "https://discord.com/nitro"
    ));
}

#[test]
#[cfg(feature = "discord")]
fn the_main_profile_is_edited_as_discords_profile_editor_does() {
    use chat::{OwnProfile, PickedPicture, ProfilePicture};
    let mut app = Application::default();
    assert!(
        app.dispatch(Action::EditProfile).is_empty(),
        "not without Discord"
    );
    let mut app = discord_server();
    assert!(matches!(
        app.dispatch(Action::EditProfile).as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::OwnProfile { server: None }
        ))]
    ));
    let profile = OwnProfile {
        name: "Alice".into(),
        bio: "Hi".into(),
        avatar: Some("alice.png".into()),
        accent: Some(0xff0000),
        fallback: chat::Profile {
            name: "alice".into(),
            avatar: Some("default.png".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let load = |app: &mut Application| {
        app.apply(Event::OwnProfile {
            server: None,
            result: Ok(profile.clone()),
        });
    };
    // A server's answer doesn't fill it.
    app.apply(Event::OwnProfile {
        server: Some("discord:10".into()),
        result: Ok(OwnProfile::default()),
    });
    assert!(app.state.profile_editor.as_ref().unwrap().loading());
    load(&mut app);
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(
        (editor.name.as_str(), editor.colour.as_str()),
        ("Alice", "#ff0000")
    );
    // Without Nitro: everything but the banner, which the colour stands in for.
    assert!(editor.may_write(ProfileText::Name) && editor.may_write(ProfileText::Bio));
    assert!(editor.may_write(ProfileText::Colour));
    assert!(
        editor.may_picture(ProfilePicture::Avatar) && !editor.may_picture(ProfilePicture::Banner)
    );
    assert!(editor.change().is_none());

    let write = |app: &mut Application, field, text: &str| {
        app.dispatch(Action::WriteProfile {
            field,
            text: text.into(),
        });
    };
    write(&mut app, ProfileText::Name, "Ally");
    write(&mut app, ProfileText::Bio, "Hello");
    write(&mut app, ProfileText::Colour, "teal");
    assert!(app.dispatch(Action::SaveProfile).is_empty(), "not a colour");
    write(&mut app, ProfileText::Colour, "#00f");
    // The bar's Reset puts back what Discord has.
    app.dispatch(Action::UndoProfileEdits);
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!((editor.name.as_str(), editor.bio.as_str()), ("Alice", "Hi"));
    assert_eq!(editor.colour, "#ff0000");
    assert!(editor.change().is_none());
    write(&mut app, ProfileText::Name, "Ally");
    write(&mut app, ProfileText::Bio, "Hello");
    write(&mut app, ProfileText::Colour, "#00f");
    assert!(
        app.dispatch(Action::PickProfilePicture {
            kind: ProfilePicture::Banner,
            path: "sky.png".into(),
        })
        .is_empty(),
        "a banner takes Nitro"
    );
    let gif = PickedPicture {
        mime: "image/gif",
        data: Arc::from(&b"gif"[..]),
        preview: Arc::default(),
    };
    let read = |app: &mut Application, picked: &PickedPicture| {
        assert_eq!(
            app.dispatch(Action::PickProfilePicture {
                kind: ProfilePicture::Avatar,
                path: "cat.gif".into(),
            })
            .len(),
            1
        );
        app.apply(Event::ProfilePicture {
            kind: ProfilePicture::Avatar,
            path: "cat.gif".into(),
            result: Ok(picked.clone()),
        });
    };
    read(&mut app, &gif);
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert_eq!(
        editor.error.as_deref(),
        Some("An animated avatar takes Nitro.")
    );
    assert_eq!(editor.picture(ProfilePicture::Avatar).0, Some("alice.png"));
    // Remove Avatar goes back to Discord's default.
    app.dispatch(Action::ResetProfilePicture(ProfilePicture::Avatar));
    let saved = |effects: Vec<Effect>| match effects.as_slice() {
        [
            Effect::Backend(Command::Discord(DiscordCommand::SaveProfile {
                server: None,
                change,
            })),
        ] => change.clone(),
        other => panic!("{} effects", other.len()),
    };
    let change = saved(app.dispatch(Action::SaveProfile));
    assert_eq!(change.name.as_deref(), Some("Ally"));
    assert_eq!(change.bio.as_deref(), Some("Hello"));
    assert_eq!(change.accent, Some(Some(0x0000ff)));
    assert!(matches!(change.avatar, Some(None)));
    assert!(change.pronouns.is_none() && change.banner.is_none());
    app.apply(Event::ProfileSaved {
        server: None,
        result: Ok(()),
    });
    assert!(app.state.profile_editor.is_none());

    // With Nitro: a banner and an animated avatar, and no banner colour,
    // which Discord shows only without.
    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    snapshot.account = Some(yapfast_discord::service::Account {
        name: "Alice".into(),
        avatar: "alice.png".into(),
        premium_type: 2,
        mfa_enabled: false,
        has_email: true,
    });
    app.apply(discord_event(snapshot));
    app.dispatch(Action::EditProfile);
    load(&mut app);
    write(&mut app, ProfileText::Colour, "");
    let editor = app.state.profile_editor.as_ref().unwrap();
    assert!(editor.may_picture(ProfilePicture::Banner) && !editor.may_write(ProfileText::Colour));
    assert!(editor.change().is_none(), "the colour isn't sent");
    read(&mut app, &gif);
    let change = saved(app.dispatch(Action::SaveProfile));
    assert!(matches!(&change.avatar, Some(Some(picked)) if picked.mime == "image/gif"));
    assert!(change.accent.is_none());
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
    app.dispatch(Action::SelectRoom("discord:10:13".into()));
    assert_eq!(app.state.threads_channel(), None);
}

#[test]
#[cfg(feature = "discord")]
fn a_poll_is_made_in_discords_dialog_where_polls_may_go() {
    use crate::chat::{NewPoll, NewPollAnswer};
    use yapfast_discord::{
        ready::{self, permission},
        service::{Snapshot, Status},
    };
    let bits = |bits: u64| bits.to_string();
    let server = ready::server(&serde_json::json!({
        "id": "10", "owner_id": "99", "properties": {"name": "Test"},
        "roles": [{"id": "10", "name": "@everyone", "position": 0, "permissions": bits(
            permission::VIEW_CHANNEL | permission::SEND_MESSAGES | permission::SEND_POLLS)}],
        "channels": [
            {"id": "11", "type": 0, "name": "general", "position": 0},
            {"id": "14", "type": 0, "name": "quiet", "position": 1,
             "permission_overwrites": [{"id": "10", "type": 0, "allow": "0",
                                        "deny": bits(permission::SEND_POLLS)}]},
        ],
    }))
    .unwrap();
    let mut app = Application::default();
    app.apply(Event::Discord(Arc::new(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        servers: vec![MarkedServer::from(server)],
        ..Default::default()
    })));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    let open = |app: &mut Application, room: &str| {
        app.dispatch(Action::SelectRoom(room.into()));
        app.apply(Event::Timeline {
            room: room.into(),
            messages: timeline(vec![]),
        });
    };
    open(&mut app, "discord:10:14");
    assert!(!app.can_create_polls(), "Send Polls is denied there");
    app.dispatch(Action::OpenPollDraft);
    assert!(app.state.poll_draft.is_none());
    let general = "discord:10:11".to_owned();
    open(&mut app, &general);
    assert!(app.can_create_polls());

    // It starts with two answers to fill, for a day.
    app.dispatch(Action::OpenPollDraft);
    let draft = |app: &Application| app.state.poll_draft.clone().unwrap();
    assert_eq!(draft(&app).answers.len(), 2);
    assert_eq!(POLL_HOURS[draft(&app).duration], 24);
    assert!(!draft(&app).can_post(), "it has no question yet");
    let edit = |app: &mut Application, edit: PollEdit| app.dispatch(Action::EditPoll(edit));
    edit(&mut app, PollEdit::Question(" Lunch? ".into()));
    edit(
        &mut app,
        PollEdit::Answer {
            index: 1,
            text: "Soup".into(),
        },
    );
    assert!(draft(&app).can_post(), "one answer is enough");
    edit(
        &mut app,
        PollEdit::Answer {
            index: 0,
            text: "x".repeat(56),
        },
    );
    assert_eq!(
        draft(&app).problem(),
        Some("Answers can be at most 55 characters.")
    );
    assert!(!draft(&app).can_post());
    edit(
        &mut app,
        PollEdit::Answer {
            index: 0,
            text: "Pizza".into(),
        },
    );
    edit(
        &mut app,
        PollEdit::Emoji {
            index: 0,
            emoji: Some("🍕".into()),
        },
    );

    // Ten answers at most, and one stays.
    for _ in 0..10 {
        edit(&mut app, PollEdit::AddAnswer);
    }
    assert_eq!(draft(&app).answers.len(), POLL_ANSWERS);
    assert!(!draft(&app).can_add());
    let ids: Vec<u32> = draft(&app).answers.iter().map(|answer| answer.id).collect();
    edit(&mut app, PollEdit::RemoveAnswer(1));
    let kept: Vec<u32> = draft(&app).answers.iter().map(|answer| answer.id).collect();
    assert_eq!(
        kept,
        [&ids[..1], &ids[2..]].concat(),
        "the others keep their ids"
    );
    edit(
        &mut app,
        PollEdit::Answer {
            index: 1,
            text: "Soup".into(),
        },
    );
    edit(&mut app, PollEdit::Duration(6));
    edit(&mut app, PollEdit::Multiselect(true));

    // Posting leaves out the empty answers, and keeps the dialog till
    // Discord answers.
    let posted = app.dispatch(Action::PostPoll);
    let expected = NewPoll {
        question: "Lunch?".into(),
        answers: vec![
            NewPollAnswer {
                text: "Pizza".into(),
                emoji: Some("🍕".into()),
            },
            NewPollAnswer {
                text: "Soup".into(),
                emoji: None,
            },
        ],
        hours: 336,
        multiselect: true,
    };
    assert!(matches!(
        posted.as_slice(),
        [Effect::Backend(Command::SendPoll { room, poll })] if *room == general && *poll == expected
    ));
    assert!(draft(&app).posting && !draft(&app).can_post());
    assert!(app.dispatch(Action::PostPoll).is_empty(), "once");
    app.apply(Event::PollPosted {
        room: general.clone(),
        result: Err("no".into()),
    });
    assert_eq!(draft(&app).error.as_deref(), Some("no"));
    assert!(!draft(&app).posting);
    app.dispatch(Action::PostPoll);
    app.apply(Event::PollPosted {
        room: general.clone(),
        result: Ok(()),
    });
    assert!(app.state.poll_draft.is_none());
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
        room: general.clone(),
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
fn a_reactions_tooltip_and_window_ask_for_who_reacted() {
    use crate::chat::Reactor;
    let mut app = connected();
    let mut reacted = message("e:$bob", false);
    let reaction = |key: &str, count| Reaction {
        key: key.into(),
        count,
        mine: false,
    };
    reacted.reactions = vec![reaction("👍", 2), reaction("🎉", 5)];
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![reacted]),
    });
    let person = |id: &str, me| Reactor {
        id: id.into(),
        name: id.to_uppercase(),
        me,
        ..Default::default()
    };
    let hover = |app: &mut Application, key: &str| {
        app.dispatch(Action::HoverReaction(Some(("e:$bob".into(), key.into()))))
    };

    // Hovering asks for the first three, once.
    assert!(matches!(
        hover(&mut app, "🎉").as_slice(),
        [Effect::Backend(Command::Reactors { key, after: None, limit: 3, .. })] if key == "🎉"
    ));
    assert!(
        app.state.reaction_tip().is_none(),
        "nothing shows until they come"
    );
    app.apply(Event::Reactors {
        room: "dm".into(),
        item: "e:$bob".into(),
        key: "🎉".into(),
        after: None,
        limit: TIP_PEOPLE,
        result: Ok(vec![
            person("a", true),
            person("b", false),
            person("c", false),
        ]),
    });
    let (tip, shown) = app.state.reaction_tip().unwrap();
    assert_eq!(
        tip.names.as_deref(),
        Some(&["A".into(), "B".into(), "C".into()][..])
    );
    assert_eq!(shown.count, 5);
    app.dispatch(Action::HoverReaction(None));
    assert!(app.state.reaction_tip().is_none());
    assert!(
        hover(&mut app, "🎉").is_empty(),
        "kept while its count stays"
    );

    // The window opens on the reaction most reacted with, a page at a time.
    assert!(matches!(
        app.dispatch(Action::ShowReactions { item: "e:$bob".into(), key: None }).as_slice(),
        [Effect::Backend(Command::Reactors { key, after: None, limit: 100, .. })] if key == "🎉"
    ));
    let keys: Vec<&str> = app
        .state
        .window_reactions()
        .iter()
        .map(|reaction| reaction.key.as_str())
        .collect();
    assert_eq!(keys, ["🎉", "👍"]);
    // The tooltip's page doesn't fill it.
    app.apply(Event::Reactors {
        room: "dm".into(),
        item: "e:$bob".into(),
        key: "🎉".into(),
        after: None,
        limit: TIP_PEOPLE,
        result: Ok(vec![person("a", true)]),
    });
    assert!(app.state.reactions.as_ref().unwrap().people.is_empty());
    app.apply(Event::Reactors {
        room: "dm".into(),
        item: "e:$bob".into(),
        key: "🎉".into(),
        after: None,
        limit: 100,
        result: Ok(vec![person("a", true), person("b", false)]),
    });
    // Fewer than a page: no one's after them, whatever the count.
    assert!(!app.state.more_reactors());
    assert!(app.dispatch(Action::MoreReactors).is_empty());

    // Only this account's own reaction goes from a Matrix room's window.
    assert!(app.dispatch(Action::RemoveReactor("b".into())).is_empty());
    assert!(matches!(
        app.dispatch(Action::RemoveReactor("a".into())).as_slice(),
        [Effect::Backend(Command::React { key, .. })] if key == "🎉"
    ));
    let people = &app.state.reactions.as_ref().unwrap().people;
    assert_eq!(people.len(), 1);

    // Another reaction asks again, from the start.
    assert!(matches!(
        app.dispatch(Action::ShowReactors("👍".into())).as_slice(),
        [Effect::Backend(Command::Reactors { key, after: None, .. })] if key == "👍"
    ));
    assert!(app.state.reactions.as_ref().unwrap().people.is_empty());
    app.dispatch(Action::CloseReactions);
    assert!(app.state.reactions.is_none());
}

#[test]
fn votes_go_to_discord_and_the_window_lists_who_voted() {
    use crate::chat::{Poll, PollAnswer};
    let mut app = connected();
    let answer = |id, votes, mine| PollAnswer {
        id,
        text: format!("Answer {id}"),
        votes,
        mine,
        ..Default::default()
    };
    let mut asked = message("e:$poll", true);
    asked.poll = Some(Poll {
        question: "Lunch?".into(),
        answers: vec![answer(1, 2, false), answer(2, 0, false)],
        can_end: true,
        ..Default::default()
    });
    let mut closed = message("e:$closed", false);
    closed.poll = Some(Poll {
        answers: vec![answer(1, 1, false)],
        finalized: true,
        ..Default::default()
    });
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![asked, closed]),
    });
    let vote = |app: &mut Application, item: &str, answers: Vec<u64>| {
        app.dispatch(Action::Vote {
            item: item.into(),
            answers,
        })
    };

    assert!(matches!(
        vote(&mut app, "e:$poll", vec![2]).as_slice(),
        [Effect::Backend(Command::Vote { item, answers, .. })] if item == "e:$poll" && answers == &[2]
    ));
    // Taking it back votes for none.
    assert!(matches!(
        vote(&mut app, "e:$poll", vec![]).as_slice(),
        [Effect::Backend(Command::Vote { answers, .. })] if answers.is_empty()
    ));
    // One answer only, of those it has, while it's open.
    assert!(vote(&mut app, "e:$poll", vec![1, 2]).is_empty());
    assert!(vote(&mut app, "e:$poll", vec![9]).is_empty());
    assert!(vote(&mut app, "e:$closed", vec![1]).is_empty());

    // Closing it asks first, and only its author may.
    app.dispatch(Action::AskEndPoll("e:$closed".into()));
    assert!(app.state.confirm_end_poll.is_none());
    app.dispatch(Action::AskEndPoll("e:$poll".into()));
    assert_eq!(app.state.confirm_end_poll.as_deref(), Some("e:$poll"));
    app.dispatch(Action::CancelEndPoll);
    assert!(app.dispatch(Action::EndPoll).is_empty());
    app.dispatch(Action::AskEndPoll("e:$poll".into()));
    assert!(matches!(
        app.dispatch(Action::EndPoll).as_slice(),
        [Effect::Backend(Command::EndPoll { item, .. })] if item == "e:$poll"
    ));
    assert!(app.state.confirm_end_poll.is_none());

    // Who voted shows by answer, the first unless one is asked for.
    assert!(matches!(
        app.dispatch(Action::ShowVoters {
            item: "e:$poll".into(),
            answer: None
        })
        .as_slice(),
        [Effect::Backend(Command::Voters {
            answer: 1,
            after: None,
            limit: 100,
            ..
        })]
    ));
    let ids: Vec<u64> = app
        .state
        .window_answers()
        .iter()
        .map(|answer| answer.id)
        .collect();
    assert_eq!(ids, [1, 2]);
    assert!(app.state.window_reactions().is_empty());
    app.apply(Event::Reactors {
        room: "dm".into(),
        item: "e:$poll".into(),
        key: "1".into(),
        after: None,
        limit: 100,
        result: Ok(vec![crate::chat::Reactor {
            id: "a".into(),
            me: true,
            ..Default::default()
        }]),
    });
    assert_eq!(app.state.reactions.as_ref().unwrap().people.len(), 1);
    // Both counted votes haven't come, but the page came short.
    assert!(!app.state.more_reactors());
    // A vote isn't taken back from the window.
    assert!(app.dispatch(Action::RemoveReactor("a".into())).is_empty());
    assert!(matches!(
        app.dispatch(Action::ShowReactors("2".into())).as_slice(),
        [Effect::Backend(Command::Voters { answer: 2, .. })]
    ));
}

#[cfg(feature = "discord")]
#[test]
fn favourite_gifs_show_from_disk_and_stars_wait_for_discords() {
    use crate::chat::{FavoriteGif, Gif};
    let mut app = connected();
    let gif = Gif {
        url: "https://klipy.com/gifs/cat-9100".into(),
        src: "https://static.klipy.com/cat.webp".into(),
        width: 200,
        height: 100,
        ..Default::default()
    };
    let with_gif = ChatMessage {
        attachments: vec![crate::chat::Attachment {
            name: "cat.webp".into(),
            size: None,
            image: true,
            dimensions: Some((200, 100)),
            preview: None,
            preview_failed: false,
            placement: crate::chat::Placement::Message,
            gif: Some(gif.clone()),
            spoiler: false,
            video: false,
            url: None,
        }],
        ..message("e:$1", false)
    };
    app.apply(discord_event(yapfast_discord::service::Snapshot {
        status: yapfast_discord::service::Status::Online,
        servers: vec![MarkedServer::from(yapfast_discord::ready::Server {
            id: "10".into(),
            name: "Test".into(),
            text_channels: vec![yapfast_discord::ready::Channel {
                id: "21".into(),
                name: "general".into(),
                ..Default::default()
            }],
            ..Default::default()
        })],
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    app.apply(Event::Timeline {
        room: "discord:10:21".into(),
        messages: timeline(vec![with_gif]),
    });
    assert!(app.state.message("e:$1").is_some());
    let backend = |effects: Vec<Effect>| -> Vec<Command> {
        effects
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Backend(command) => Some(command),
                _ => None,
            })
            .collect()
    };

    // Pointing at the star asks for the favourites, once, and those kept
    // on disk show until Discord's come.
    let asked = backend(app.dispatch(Action::LoadFavoriteGifs));
    assert!(matches!(
        asked.as_slice(),
        [Command::Discord(DiscordCommand::FavoriteGifs)]
    ));
    let other = |name: &str| Gif {
        url: format!("https://klipy.com/gifs/{name}"),
        src: format!("https://static.klipy.com/{name}.webp"),
        ..gif.clone()
    };
    let old = FavoriteGif::of(&other("old"), &[]);
    app.apply(Event::KeptFavoriteGifs(vec![old.clone()]));
    assert!(app.state.is_favorite_gif(&old.url));
    // A click meanwhile shows at once, and waits for Discord's favourites,
    // so what another client changed meanwhile stays.
    let star = || Action::FavoriteMessageGif {
        item: "e:$1".into(),
        index: 0,
    };
    let changed = app.state.favorite_gifs_changed;
    assert!(backend(app.dispatch(star())).is_empty());
    assert!(app.state.is_favorite_gif(&gif.url));
    assert!(app.state.favorite_gifs_changed > changed);
    let elsewhere = FavoriteGif::of(&other("new"), &[]);
    let saved = backend(app.apply(Event::FavoriteGifs(Ok(vec![elsewhere.clone()]))));
    assert!(matches!(
        saved.as_slice(),
        [Command::Discord(DiscordCommand::SetFavoriteGifs(favorites))]
            if favorites == &[elsewhere.clone(), FavoriteGif::of(&gif, std::slice::from_ref(&elsewhere))]
    ));
    assert!(app.state.is_favorite_gif(&gif.url) && !app.state.is_favorite_gif(&old.url));
    // Those kept on disk don't replace Discord's.
    app.apply(Event::KeptFavoriteGifs(vec![old]));
    assert!(app.state.is_favorite_gif(&elsewhere.url));

    // Its star again takes it out.
    let saved = backend(app.dispatch(star()));
    assert!(matches!(
        saved.as_slice(),
        [Command::Discord(DiscordCommand::SetFavoriteGifs(favorites))] if favorites == &[elsewhere]
    ));
    assert!(backend(app.dispatch(Action::LoadFavoriteGifs)).is_empty());
}

#[cfg(feature = "discord")]
#[test]
fn the_gif_picker_searches_sends_and_keeps_favorites() {
    use crate::chat::{FavoriteGif, Gif, GifCategory};
    use yapfast_discord::ready::{Channel, Server};
    let mut app = connected();
    app.apply(discord_event(yapfast_discord::service::Snapshot {
        status: yapfast_discord::service::Status::Online,
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
        ..Default::default()
    }));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom("discord:10:21".into()));
    app.apply(Event::Timeline {
        room: "discord:10:21".into(),
        messages: timeline(vec![]),
    });
    assert!(app.state.can_pick_gifs());
    let gif = |name: &str| Gif {
        id: Some(format!("id-{name}")),
        url: format!("https://klipy.com/gifs/{name}"),
        src: format!("https://static.klipy.com/{name}.webp"),
        width: 200,
        height: 100,
        ..Default::default()
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

    // Opening asks for the home and the favourites, once.
    let asked = backend(app.dispatch(Action::OpenGifs));
    assert!(matches!(
        asked.as_slice(),
        [
            Command::Discord(DiscordCommand::GifCategories),
            Command::Discord(DiscordCommand::FavoriteGifs)
        ]
    ));
    app.apply(Event::GifCategories(Ok((
        vec![GifCategory {
            name: "happy".into(),
            src: "https://static.klipy.com/happy.webp".into(),
        }],
        Some(gif("trend")),
    ))));
    app.apply(Event::FavoriteGifs(Ok(vec![])));
    app.dispatch(Action::CloseGifs);
    assert!(backend(app.dispatch(Action::OpenGifs)).is_empty());

    // A term's tile searches for it; the results show, their pictures load
    // only while in sight, and a short list suggests other terms.
    let asked = backend(app.dispatch(Action::SearchGifCategory("happy".into())));
    assert!(matches!(
        asked.as_slice(),
        [Command::Discord(DiscordCommand::Gifs(Some(query)))] if query == "happy"
    ));
    assert_eq!(app.state.gifs.view, GifView::Search);
    app.apply(Event::Gifs {
        query: Some("happy".into()),
        result: Ok(vec![gif("cat"), gif("dog")]),
    });
    let shown = |names: &[&str]| -> Vec<(String, u32, u32)> {
        names
            .iter()
            .map(|name| (format!("https://static.klipy.com/{name}.webp"), 231, 115))
            .collect()
    };
    let asked = backend(app.dispatch(Action::GifsShown {
        shown: shown(&["cat", "dog"]),
        near_end: true,
    }));
    assert!(matches!(
        asked.as_slice(),
        [
            Command::Discord(DiscordCommand::GifPicture { .. }),
            Command::Discord(DiscordCommand::GifPicture { .. }),
            Command::Discord(DiscordCommand::GifSuggestions(query)),
        ] if query == "happy"
    ));
    assert!(
        backend(app.dispatch(Action::GifsShown {
            shown: shown(&["dog"]),
            near_end: true,
        }))
        .is_empty()
    );
    assert_eq!(app.state.gifs.pictures.len(), 1, "cat's picture goes");

    // Starring keeps the GIF among the favourites on Discord, the newest
    // first.
    let saved = backend(app.dispatch(Action::FavoriteGif(gif("dog").url)));
    assert!(matches!(
        saved.as_slice(),
        [Command::Discord(DiscordCommand::SetFavoriteGifs(favorites))]
            if favorites == &[FavoriteGif::of(&gif("dog"), &[])]
    ));
    assert!(app.state.is_favorite_gif(&gif("dog").url));

    // A click sends the page as its own message and reports the pick; the
    // picker closes, letting go of its pictures.
    let sent = backend(app.dispatch(Action::PickGif {
        url: gif("cat").url,
        insert: false,
    }));
    assert!(matches!(
        sent.as_slice(),
        [
            ..,
            Command::Send { room, body, reply: None, files, .. },
            Command::Discord(DiscordCommand::GifPicked { id, query }),
        ] if room == "discord:10:21" && body == &gif("cat").url && files.is_empty()
            && id == "id-cat" && query == "happy"
    ));
    assert!(!app.state.gifs.open && app.state.gifs.pictures.is_empty());
    // The search stays for next time.
    assert_eq!(app.state.gifs.query, "happy");

    // Shift and a click puts the page in the message instead.
    app.dispatch(Action::SetDraft("Look".into()));
    app.dispatch(Action::OpenGifs);
    app.dispatch(Action::PickGif {
        url: gif("dog").url,
        insert: true,
    });
    assert_eq!(app.state.draft(), "Look https://klipy.com/gifs/dog");

    // Favourites changed elsewhere replace these; starring one again takes
    // it out.
    app.apply(Event::FavoriteGifs(Ok(vec![
        FavoriteGif::of(&gif("dog"), &[]),
        FavoriteGif {
            order: 7,
            ..FavoriteGif::of(&gif("wave"), &[])
        },
    ])));
    app.dispatch(Action::ShowGifs(GifView::Favorites));
    let shown: Vec<&str> = app
        .state
        .shown_favorite_gifs()
        .iter()
        .map(|favorite| favorite.url.as_str())
        .collect();
    assert_eq!(shown, [gif("wave").url, gif("dog").url]);
    let saved = backend(app.dispatch(Action::FavoriteGif(gif("wave").url)));
    assert!(matches!(
        saved.as_slice(),
        [Command::Discord(DiscordCommand::SetFavoriteGifs(favorites))] if favorites.len() == 1
    ));
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
        room: ideas.clone(),
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
    let mut more = posts.clone();
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
    app.dispatch(Action::SelectRoom(ideas.clone()));
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

#[cfg(feature = "discord")]
fn invite_to(code: &str, guild: &str, channel: &str) -> chat::InvitePreview {
    chat::InvitePreview {
        code: code.into(),
        server: format!("discord:{guild}"),
        name: "Builders".into(),
        icon: None,
        room: format!("discord:{guild}:{channel}"),
        channel: "general".into(),
        channel_type: 0,
        voice: false,
        members: Some(42),
        online: Some(7),
    }
}

#[test]
#[cfg(feature = "discord")]
fn join_a_server_shows_the_invite_and_joins_on_the_click() {
    let mut app = discord_server();
    app.dispatch(Action::OpenJoinServer);
    assert!(
        app.dispatch(Action::FindInvite("two words".into()))
            .is_empty()
    );
    assert!(app.state.join.as_ref().unwrap().error.is_some());

    let effects = app.dispatch(Action::FindInvite(" https://discord.gg/abc ".into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ResolveInvite { code, typed: Some(typed) }))]
            if code == "abc" && typed == " https://discord.gg/abc "
    ));
    let join = app.state.join.clone().unwrap();
    assert!(join.resolving && join.error.is_none());
    // An answer about another invite, typed before, changes nothing.
    app.apply(Event::InviteResolved {
        code: "old".into(),
        result: Ok(invite_to("old", "30", "31")),
    });
    assert!(app.state.join.as_ref().unwrap().preview.is_none());
    app.apply(Event::InviteResolved {
        code: "abc".into(),
        result: Ok(invite_to("abc", "20", "21")),
    });
    let join = app.state.join.clone().unwrap();
    assert!(!join.resolving && !join.member);
    assert_eq!(join.preview.unwrap().name, "Builders");

    let effects = app.dispatch(Action::JoinServer);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::JoinServer { preview, from_link: false }))]
            if preview.code == "abc"
    ));
    assert!(
        app.dispatch(Action::JoinServer).is_empty(),
        "one join at a time"
    );
    let effects = app.apply(Event::JoinCaptcha {
        code: "abc".into(),
        page: "http://127.0.0.1:1/captcha/x".into(),
    });
    assert!(matches!(effects.as_slice(), [Effect::OpenPage(page)] if page.ends_with("/x")));
    assert!(matches!(
        app.dispatch(Action::OpenJoinCaptcha).as_slice(),
        [Effect::OpenPage(_)]
    ));

    // Joined: the dialog closes, and the invite's channel opens once
    // Discord lists the server.
    app.apply(Event::ServerJoined {
        code: "abc".into(),
        result: Ok(chat::JoinedServer {
            server: "discord:20".into(),
            room: "discord:20:21".into(),
            form: false,
        }),
    });
    assert!(app.state.join.is_none());
    let mut snapshot = yapfast_discord::service::Snapshot::clone(app.discord.as_ref().unwrap());
    let joined = yapfast_discord::ready::server(&serde_json::json!({
        "id": "20", "owner_id": "99",
        "properties": {"name": "Builders"},
        "roles": [{"id": "20", "name": "@everyone", "position": 0,
                   "permissions": yapfast_discord::ready::permission::VIEW_CHANNEL.to_string()}],
        "channels": [{"id": "21", "type": 0, "name": "general", "position": 0}],
    }))
    .unwrap();
    snapshot.servers.push(MarkedServer::from(joined));
    app.apply(discord_event(snapshot));
    assert_eq!(app.state.selected.as_deref(), Some("discord:20:21"));
    assert_eq!(
        app.state.navigation,
        Navigation::Server("discord:20".into())
    );
}

#[test]
#[cfg(feature = "discord")]
fn an_invite_link_opens_the_server_or_the_dialog() {
    let mut app = discord_server();
    // A link to a server she's in goes straight there.
    let effects = app.dispatch(Action::OpenInviteLink("in".into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::ResolveInvite { typed: None, .. }
        ))]
    ));
    app.apply(Event::InviteResolved {
        code: "in".into(),
        result: Ok(invite_to("in", "10", "13")),
    });
    assert!(app.state.join.is_none());
    assert_eq!(app.state.selected.as_deref(), Some("discord:10:13"));

    // One she isn't in shows it, and Discord's refusal stays in the dialog.
    app.dispatch(Action::OpenInviteLink("new".into()));
    app.apply(Event::InviteResolved {
        code: "new".into(),
        result: Ok(invite_to("new", "30", "31")),
    });
    let effects = app.dispatch(Action::JoinServer);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::JoinServer {
                from_link: true,
                ..
            }
        ))]
    ));
    app.apply(Event::ServerJoined {
        code: "new".into(),
        result: Err("Couldn't join the server: This server is full.".into()),
    });
    let join = app.state.join.clone().unwrap();
    assert!(!join.joining);
    assert!(join.error.unwrap().ends_with("full."));

    // A server with a form to fill in sends it to Discord's web app.
    app.dispatch(Action::JoinServer);
    app.apply(Event::ServerJoined {
        code: "new".into(),
        result: Ok(chat::JoinedServer {
            server: "discord:30".into(),
            room: "discord:30:31".into(),
            form: true,
        }),
    });
    assert!(app.dispatch(Action::JoinServer).is_empty());
    let effects = app.dispatch(Action::OpenJoinForm);
    assert!(matches!(
        effects.as_slice(),
        [Effect::OpenPage(page)] if page == "https://discord.com/channels/30/31"
    ));
    assert!(app.state.join.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn an_unknown_invite_says_so() {
    let mut app = discord_server();
    app.dispatch(Action::OpenJoinServer);
    app.dispatch(Action::FindInvite("gone".into()));
    app.apply(Event::InviteResolved {
        code: "gone".into(),
        result: Err("This invite is invalid or has expired.".into()),
    });
    let join = app.state.join.clone().unwrap();
    assert!(join.preview.is_none() && !join.resolving);
    assert_eq!(
        join.error.as_deref(),
        Some("This invite is invalid or has expired.")
    );
}

#[test]
#[cfg(feature = "discord")]
fn an_event_link_shows_the_event_and_marks_interest_on_the_click() {
    let event = |interested: bool, over: bool| chat::EventPreview {
        event: "11".into(),
        guild: "10".into(),
        name: "Alien meetup".into(),
        description: Some("Aliens only!".into()),
        starts_at: 1_800_000_000,
        ends_at: None,
        room: Some("discord:10:13".into()),
        location: None,
        creator: Some("Dziurwa".into()),
        interested_count: Some(7),
        active: false,
        over,
        interested,
    };
    let mut app = discord_server();
    let effects = app.dispatch(Action::OpenEventLink {
        guild: "10".into(),
        event: "11".into(),
    });
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ResolveEvent { guild, event }))]
            if guild == "10" && event == "11"
    ));
    assert!(app.state.scheduled_event.as_ref().unwrap().resolving);
    // Interest changes only from the click, once the event shows.
    assert!(app.dispatch(Action::ToggleEventInterest).is_empty());
    app.apply(Event::EventResolved {
        event: "other".into(),
        result: Ok(event(false, false)),
    });
    assert!(
        app.state
            .scheduled_event
            .as_ref()
            .unwrap()
            .preview
            .is_none()
    );
    app.apply(Event::EventResolved {
        event: "11".into(),
        result: Ok(event(false, false)),
    });
    let shown = app.state.scheduled_event.clone().unwrap();
    assert!(!shown.resolving);
    assert_eq!(shown.preview.unwrap().name, "Alien meetup");

    let effects = app.dispatch(Action::ToggleEventInterest);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::SetEventInterest { preview, interested: true }))]
            if preview.event == "11"
    ));
    assert!(
        app.dispatch(Action::ToggleEventInterest).is_empty(),
        "one change at a time"
    );
    app.apply(Event::EventInterestSet {
        event: "11".into(),
        interested: true,
        result: Ok(()),
    });
    let shown = app.state.scheduled_event.clone().unwrap();
    let preview = shown.preview.unwrap();
    assert!(preview.interested && !shown.marking);
    assert_eq!(preview.interested_count, Some(8));

    // And back again, on the next click.
    let effects = app.dispatch(Action::ToggleEventInterest);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::SetEventInterest {
                interested: false,
                ..
            }
        ))]
    ));
    app.apply(Event::EventInterestSet {
        event: "11".into(),
        interested: false,
        result: Ok(()),
    });
    let preview = app.state.scheduled_event.clone().unwrap().preview.unwrap();
    assert!(!preview.interested);
    assert_eq!(preview.interested_count, Some(7));

    // An event that's over offers no change, and Discord's refusal to
    // show one stays in the dialog.
    app.dispatch(Action::OpenEventLink {
        guild: "10".into(),
        event: "11".into(),
    });
    app.apply(Event::EventResolved {
        event: "11".into(),
        result: Ok(event(false, true)),
    });
    assert!(app.dispatch(Action::ToggleEventInterest).is_empty());
    app.dispatch(Action::OpenEventLink {
        guild: "10".into(),
        event: "gone".into(),
    });
    app.apply(Event::EventResolved {
        event: "gone".into(),
        result: Err("Unknown Guild Scheduled Event".into()),
    });
    let shown = app.state.scheduled_event.clone().unwrap();
    assert!(shown.preview.is_none() && !shown.resolving);
    assert_eq!(
        shown.error.as_deref(),
        Some("Unknown Guild Scheduled Event")
    );
}

#[test]
#[cfg(feature = "discord")]
fn a_gift_link_shows_the_gift_and_redeems_on_the_click() {
    let gift = |redeemed: bool, spent: bool| chat::GiftPreview {
        name: "Nitro".into(),
        plan: Some("Nitro Monthly".into()),
        gifter: Some("Gift Giver".into()),
        redeemed,
        spent,
        in_discord: false,
    };
    let code = "2CG6SV9QtRxerJTgCYNDnU7M";
    let mut app = discord_server();
    let effects = app.dispatch(Action::OpenGiftLink(code.into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ResolveGift { code: asked }))]
            if asked == code
    ));
    assert!(app.state.gift.as_ref().unwrap().resolving);
    // Redeeming before the gift shows, or from looking it up, never
    // happens; only the click does it.
    assert!(app.dispatch(Action::RedeemGift).is_empty());
    app.apply(Event::GiftResolved {
        code: "other".into(),
        result: Ok(gift(false, false)),
    });
    assert!(app.state.gift.as_ref().unwrap().preview.is_none());
    app.apply(Event::GiftResolved {
        code: code.into(),
        result: Ok(gift(false, false)),
    });
    let shown = app.state.gift.clone().unwrap();
    assert!(!shown.resolving && !shown.redeemed);
    assert_eq!(shown.preview.unwrap().name, "Nitro");

    let effects = app.dispatch(Action::RedeemGift);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::RedeemGift { code: redeemed, .. }))]
            if redeemed == code
    ));
    assert!(
        app.dispatch(Action::RedeemGift).is_empty(),
        "one redemption at a time"
    );
    app.apply(Event::GiftRedeemed {
        code: code.into(),
        result: Ok(()),
    });
    let shown = app.state.gift.clone().unwrap();
    assert!(shown.redeemed && app.state.redeeming_gifts.is_empty());
    assert!(shown.preview.unwrap().redeemed);
    assert!(app.dispatch(Action::RedeemGift).is_empty(), "and not twice");

    // A gift this account redeemed, or anyone used up, shows but offers
    // nothing to redeem.
    for preview in [gift(true, false), gift(false, true)] {
        app.dispatch(Action::OpenGiftLink(code.into()));
        app.apply(Event::GiftResolved {
            code: code.into(),
            result: Ok(preview),
        });
        assert!(app.dispatch(Action::RedeemGift).is_empty());
    }

    // Discord's refusal stays in the dialog.
    app.dispatch(Action::OpenGiftLink("gone".into()));
    app.apply(Event::GiftResolved {
        code: "gone".into(),
        result: Err("Unknown Gift Code".into()),
    });
    let shown = app.state.gift.clone().unwrap();
    assert!(shown.preview.is_none() && !shown.resolving);
    assert_eq!(shown.error.as_deref(), Some("Unknown Gift Code"));
}

#[test]
#[cfg(feature = "discord")]
fn a_gift_only_discord_can_redeem_opens_in_the_browser() {
    let mut app = discord_server();
    app.dispatch(Action::OpenGiftLink("AbCdEfGhIjKlMnOp".into()));
    app.apply(Event::GiftResolved {
        code: "AbCdEfGhIjKlMnOp".into(),
        result: Ok(chat::GiftPreview {
            name: "Nitro".into(),
            plan: None,
            gifter: None,
            redeemed: false,
            spent: false,
            in_discord: true,
        }),
    });
    assert!(app.dispatch(Action::RedeemGift).is_empty());
    assert!(matches!(
        app.dispatch(Action::OpenGiftInBrowser).as_slice(),
        [Effect::OpenPage(page)] if page == "https://discord.com/gifts/AbCdEfGhIjKlMnOp"
    ));
    assert!(app.state.gift.is_none());
}

#[test]
#[cfg(feature = "discord")]
fn a_gift_closed_while_redeeming_shows_the_answer_when_opened_again() {
    let gift = || chat::GiftPreview {
        name: "Nitro".into(),
        plan: None,
        gifter: None,
        redeemed: false,
        spent: false,
        in_discord: false,
    };
    let code = "2CG6SV9QtRxerJTgCYNDnU7M";
    let mut app = discord_server();
    let open = |app: &mut Application| {
        app.dispatch(Action::OpenGiftLink(code.into()));
        app.apply(Event::GiftResolved {
            code: code.into(),
            result: Ok(gift()),
        });
    };
    open(&mut app);
    assert_eq!(app.dispatch(Action::RedeemGift).len(), 1);
    app.dispatch(Action::CloseGift);

    // Opened again before Discord answers, and looked up before it does:
    // the gift is still redeeming, so it isn't redeemed a second time.
    open(&mut app);
    assert_eq!(app.state.redeeming_gifts, [code]);
    assert!(app.dispatch(Action::RedeemGift).is_empty());
    app.apply(Event::GiftRedeemed {
        code: code.into(),
        result: Ok(()),
    });
    let shown = app.state.gift.clone().unwrap();
    assert!(shown.redeemed && shown.preview.unwrap().redeemed);
    assert!(app.state.redeeming_gifts.is_empty());

    // A refusal reaches the reopened dialog too, and the gift can be
    // tried again.
    open(&mut app);
    app.dispatch(Action::RedeemGift);
    app.dispatch(Action::CloseGift);
    app.dispatch(Action::OpenGiftLink(code.into()));
    app.apply(Event::GiftRedeemed {
        code: code.into(),
        result: Err("Couldn't redeem the gift".into()),
    });
    let shown = app.state.gift.clone().unwrap();
    assert_eq!(shown.error.as_deref(), Some("Couldn't redeem the gift"));
    app.apply(Event::GiftResolved {
        code: code.into(),
        result: Ok(gift()),
    });
    assert_eq!(app.dispatch(Action::RedeemGift).len(), 1);
}

#[test]
#[cfg(feature = "discord")]
fn an_authorization_link_shows_the_app_and_authorizes_on_the_click() {
    let bot_preview = |locked: bool| chat::OAuthPreview {
        query: "client_id=157730590492196864&scope=bot&permissions=2048&guild_id=10".into(),
        name: "Alien App".into(),
        about: Some("Beams messages up.".into()),
        icon: None,
        bot: Some("Alien App".into()),
        scopes: vec!["bot".into()],
        permissions: Some(2048),
        authorized: false,
        redirect: None,
        guild_install: true,
        guild_locked: locked,
        wanted_guild: Some("10".into()),
        guilds: vec![
            chat::OAuthGuild {
                id: "10".into(),
                name: "Server".into(),
                icon: None,
                can_add: true,
            },
            chat::OAuthGuild {
                id: "20".into(),
                name: "Builders".into(),
                icon: None,
                can_add: true,
            },
            chat::OAuthGuild {
                id: "30".into(),
                name: "Elsewhere".into(),
                icon: None,
                can_add: false,
            },
        ],
    };
    let query = "client_id=157730590492196864&scope=bot&permissions=2048&guild_id=10";
    let mut app = discord_server();
    let effects = app.dispatch(Action::OpenOAuthLink(query.into()));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::ResolveOAuth { query: sent }))]
            if sent == query
    ));
    // Authorizing before the app shows, or from looking it up, never
    // happens; only the click does it.
    assert!(app.dispatch(Action::AuthorizeOAuth).is_empty());
    app.apply(Event::OAuthResolved {
        query: query.into(),
        result: Ok(bot_preview(false)),
    });
    let shown = app.state.oauth.clone().unwrap();
    assert!(!shown.resolving);
    assert_eq!(
        shown.guild.as_deref(),
        Some("10"),
        "the link's server first"
    );
    // Another managed server can be picked; one she doesn't manage can't.
    app.dispatch(Action::ChooseOAuthGuild("30".into()));
    assert_eq!(
        app.state.oauth.as_ref().unwrap().guild.as_deref(),
        Some("10")
    );
    app.dispatch(Action::ChooseOAuthGuild("20".into()));
    assert_eq!(
        app.state.oauth.as_ref().unwrap().guild.as_deref(),
        Some("20")
    );

    let effects = app.dispatch(Action::AuthorizeOAuth);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::AuthorizeOAuth { guild: Some(guild), .. }))]
            if guild == "20"
    ));
    assert!(
        app.dispatch(Action::AuthorizeOAuth).is_empty(),
        "once at a time"
    );
    app.apply(Event::OAuthAuthorized {
        query: query.into(),
        result: Ok("https://discord.com/channels/@me".into()),
    });
    let shown = app.state.oauth.clone().unwrap();
    assert!(shown.done && !shown.authorizing);

    // A link that locks its server keeps it, whatever is picked.
    app.dispatch(Action::OpenOAuthLink(query.into()));
    app.apply(Event::OAuthResolved {
        query: query.into(),
        result: Ok(bot_preview(true)),
    });
    app.dispatch(Action::ChooseOAuthGuild("20".into()));
    assert_eq!(
        app.state.oauth.as_ref().unwrap().guild.as_deref(),
        Some("10")
    );

    // A sign-in flow sends her on to the app's own address, with its code.
    let sign_in = "response_type=code&client_id=157730590492196864&scope=identify";
    app.dispatch(Action::OpenOAuthLink(sign_in.into()));
    app.apply(Event::OAuthResolved {
        query: sign_in.into(),
        result: Ok(chat::OAuthPreview {
            query: sign_in.into(),
            guild_install: false,
            guilds: vec![],
            wanted_guild: None,
            ..bot_preview(false)
        }),
    });
    app.dispatch(Action::AuthorizeOAuth);
    let effects = app.apply(Event::OAuthAuthorized {
        query: sign_in.into(),
        result: Ok("https://nicememe.website/?code=fake".into()),
    });
    assert!(matches!(
        effects.as_slice(),
        [Effect::OpenPage(page)] if page == "https://nicememe.website/?code=fake"
    ));
    assert!(app.state.oauth.is_none());

    // Discord's refusal stays in the dialog.
    app.dispatch(Action::OpenOAuthLink(query.into()));
    app.apply(Event::OAuthResolved {
        query: query.into(),
        result: Err("Unknown Application".into()),
    });
    let shown = app.state.oauth.clone().unwrap();
    assert!(shown.preview.is_none() && !shown.resolving);
    assert_eq!(shown.error.as_deref(), Some("Unknown Application"));
}

#[test]
fn going_idle_while_signed_out_tells_nobody() {
    let inactive = || Action::Inactive {
        since: Some(1),
        show_idle: true,
    };
    let mut app = Application::default();
    assert!(app.dispatch(inactive()).is_empty());
    let mut app = connected();
    assert!(matches!(
        app.dispatch(inactive()).as_slice(),
        [Effect::Backend(Command::Discord(
            DiscordCommand::Inactive { .. }
        ))]
    ));
}

#[cfg(feature = "discord")]
mod arrange;
#[cfg(feature = "discord")]
mod audit_log;
#[cfg(feature = "discord")]
mod channel_settings;
#[cfg(feature = "discord")]
mod expressions;
#[cfg(feature = "discord")]
mod moderation;
#[cfg(feature = "discord")]
mod ownership;
#[cfg(feature = "discord")]
mod people;
#[cfg(feature = "discord")]
mod server_settings;
