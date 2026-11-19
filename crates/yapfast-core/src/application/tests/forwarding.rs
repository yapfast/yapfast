//! Forwarding a message, and the places its dialog offers.

use super::*;

#[test]
fn forwarding_sends_a_message_to_the_conversations_chosen() {
    let mut app = Application::default();
    app.dispatch(Action::Preview);
    let item = app.state.messages[1].item.clone();
    let body = app.state.messages[1].body.clone();
    app.dispatch(Action::StartForward(item));
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
        ..general
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
