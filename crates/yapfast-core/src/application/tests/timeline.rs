//! The timeline: its members and typing line, history, jumps, and what is done to a message.

use super::*;

#[test]
fn typing_shows_for_the_open_conversation() {
    let mut app = connected();
    // The line as it reads, with each name in brackets: names are bold.
    let typing = |app: &mut Application, room: &str, names: &[&str]| {
        app.apply(Event::Typing {
            room: room.into(),
            names: names.iter().map(|name| (*name).to_owned()).collect(),
        });
        app.state
            .typing_line()
            .into_iter()
            .map(|(piece, name)| {
                if name {
                    format!("[{piece}]")
                } else {
                    piece.to_owned()
                }
            })
            .collect::<String>()
    };
    assert_eq!(typing(&mut app, "dm", &["Sam"]), "[Sam] is typing\u{2026}");
    assert_eq!(
        typing(&mut app, "dm", &["Sam", "Riley"]),
        "[Sam] and [Riley] are typing\u{2026}"
    );
    assert_eq!(
        typing(&mut app, "dm", &["A", "B", "C"]),
        "[A], [B] and [C] are typing\u{2026}"
    );
    assert_eq!(
        typing(&mut app, "dm", &["A", "B", "C", "D"]),
        "[Several people] are typing\u{2026}"
    );
    // Typing elsewhere doesn't show here.
    assert_eq!(
        typing(&mut app, "games", &["Sam"]),
        "[Several people] are typing\u{2026}"
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
fn members_belong_to_the_selected_text_room() {
    let mut app = connected();
    assert!(!app.state.lists_members(), "one-to-one DMs list no members");
    app.dispatch(Action::Navigate(Navigation::Server("friends".into())));
    assert!(app.state.lists_members() && app.state.members.is_none());
    let member = ChatMember {
        bot: false,
        id: "@bob:test".into(),
        name: "Bob".into(),
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
    app.dispatch(Action::Open("lounge".into()));
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
fn only_an_answer_only_this_account_sees_is_dismissed() {
    let mut app = connected();
    let mut ephemeral = message("d:5", false);
    ephemeral.flags = 1 << 6;
    app.apply(Event::Timeline {
        room: "dm".into(),
        messages: timeline(vec![message("d:4", false), ephemeral]),
    });
    assert!(
        app.dispatch(Action::DismissEphemeral("d:4".into()))
            .is_empty()
    );
    assert!(matches!(
        app.dispatch(Action::DismissEphemeral("d:5".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::DismissEphemeral { room, item }))]
            if room == "dm" && item == "d:5"
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
            message("e:$other", true),
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
    // Shift's quick Delete doesn't ask, and leaves another message's
    // question alone.
    app.dispatch(Action::RequestDelete("e:$mine".into()));
    assert!(app.dispatch(Action::DeleteNow("e:$bob".into())).is_empty());
    assert_eq!(app.state.confirm_delete.as_deref(), Some("e:$mine"));
    assert!(matches!(
        app.dispatch(Action::DeleteNow("e:$other".into())).as_slice(),
        [Effect::Backend(Command::Delete { item, .. })] if item == "e:$other"
    ));
    assert_eq!(app.state.confirm_delete.as_deref(), Some("e:$mine"));
    // A quick Delete of the message being asked about ends the question.
    assert!(matches!(
        app.dispatch(Action::DeleteNow("e:$mine".into())).as_slice(),
        [Effect::Backend(Command::Delete { item, .. })] if item == "e:$mine"
    ));
    assert!(app.state.confirm_delete.is_none());
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
fn the_preview_tells_no_backend_which_messages_show() {
    let mut app = Application::default();
    app.dispatch(Action::Preview);
    let room = app.state.selected.clone().expect("an open room");
    let items = app
        .state
        .messages
        .iter()
        .map(|message| message.item.clone())
        .collect();
    let effects = app.dispatch(Action::VisibleMessages {
        room,
        items,
        at_end: true,
    });
    // With no backend the command would fail, and the window would say
    // the connection stopped.
    assert!(effects.is_empty());
}
