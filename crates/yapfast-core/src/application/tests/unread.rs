//! Unread messages: the new divider, what marks a channel read, and highlighted replies.

use super::*;

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
fn the_new_divider_waits_for_the_newest_page_when_kept_messages_show_first() {
    let general = "discord:10:21";
    let open = || {
        let mut app = connected();
        app.apply(unread_channel(true, 103));
        app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
        app.dispatch(Action::SelectRoom(general.into()));
        app.apply(window(general, chat::Direction::Kept, true));
        app.apply(Event::Timeline {
            room: general.into(),
            messages: timeline(discord_messages(95..=106, &[])),
        });
        app
    };
    let newest = |app: &mut Application, ids: std::ops::RangeInclusive<u64>| {
        app.apply(Event::Timeline {
            room: general.into(),
            messages: timeline(discord_messages(ids, &[])),
        })
    };
    // What was kept holds unread messages, and shows at once, but newer ones
    // may be missing: no divider yet, and the view at its end reads nothing.
    let mut app = open();
    assert_eq!(app.state.first_unread, None);
    assert_eq!(app.state.unread_jump, 0);
    assert!(app.dispatch(Action::Reading(true)).is_empty());
    assert!(newest(&mut app, 95..=130).is_empty());
    assert_eq!(app.state.first_unread, None, "nor as a timeline comes");

    // The newest page came: the view goes to the divider, reading nothing.
    assert!(
        app.apply(window(general, chat::Direction::Latest, true))
            .is_empty()
    );
    assert_eq!(app.state.first_unread.as_deref(), Some("d:104"));
    assert_eq!(app.state.unread_jump, 1);
    assert!(!app.state.reading && !app.state.caught_up);
    assert!(matches!(
        app.dispatch(Action::Reading(true)).as_slice(),
        [Effect::Backend(Command::MarkRead { room, .. })] if room == general
    ));

    // Without the newest page, it goes in from what's kept.
    let mut app = open();
    app.apply(Event::HistoryLoaded {
        room: general.into(),
        direction: chat::Direction::Latest,
        history: chat::History::Failed,
    });
    assert_eq!(app.state.first_unread.as_deref(), Some("d:104"));

    // A newest page that no longer reaches the last read message asks for the
    // messages around it, once.
    let mut app = open();
    let effects = newest(&mut app, 200..=249);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Backend(Command::Around { room, .. })] if room == general
    ));
    assert!(
        app.apply(window(general, chat::Direction::Latest, true))
            .is_empty()
    );
    assert_eq!(app.state.first_unread, None);
    app.apply(window(general, chat::Direction::Around, false));
    app.apply(Event::Timeline {
        room: general.into(),
        messages: timeline(discord_messages(90..=115, &[])),
    });
    assert_eq!(app.state.first_unread.as_deref(), Some("d:104"));
}

#[test]
#[cfg(feature = "discord")]
fn mark_as_read_on_the_unread_banner_reads_the_channel_and_takes_the_banner_away() {
    let general = "discord:10:21";
    let mut app = connected();
    app.apply(unread_channel(true, 103));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom(general.into()));
    app.apply(window(general, chat::Direction::Around, true));
    app.apply(Event::Timeline {
        room: general.into(),
        messages: timeline(discord_messages(95..=110, &[104])),
    });
    assert!(!app.state.caught_up, "the banner shows");
    assert!(matches!(
        app.dispatch(Action::MarkRead(general.into())).as_slice(),
        [Effect::Backend(Command::MarkRead { room, .. })] if room == general
    ));
    assert!(app.state.caught_up, "the banner went");
    assert_eq!(
        app.state.first_unread.as_deref(),
        Some("d:105"),
        "the divider stays"
    );
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
fn mark_unread_reads_up_to_the_message_before_and_holds_until_the_channel_is_left() {
    let general = "discord:10:21";
    let mut app = connected();
    app.apply(unread_channel(false, 110));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    app.dispatch(Action::SelectRoom(general.into()));
    app.apply(window(general, chat::Direction::Latest, true));
    let mut messages = discord_messages(95..=110, &[104]);
    // One later message mentions Alice.
    messages[14].mentions_me = true;
    app.apply(Event::Timeline {
        room: general.into(),
        messages: timeline(messages),
    });
    app.dispatch(Action::Reading(true));
    assert!(app.marks_unread("d:107"));
    assert!(!app.marks_unread("d:999"), "not loaded");
    assert_eq!(
        app.state.message_link("d:107"),
        Some((
            "https://discord.com/channels/10/21/107".into(),
            "107".into()
        ))
    );
    // Reads stay in Yapfast here, so this does too.
    assert!(matches!(
        app.dispatch(Action::MarkUnread("d:107".into())).as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::MarkUnread {
            room,
            last: 106,
            mentions: 1,
            sync: false,
        }))] if room == general
    ));
    assert_eq!(app.state.first_unread.as_deref(), Some("d:107"));
    // Discord says it's unread, and the newest message in view doesn't
    // read it while it shows.
    let reads = |effects: &[Effect]| {
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Backend(Command::MarkRead { .. })))
    };
    assert!(!reads(&app.apply(unread_channel(true, 106))));
    app.dispatch(Action::Reading(false));
    assert!(!reads(&app.dispatch(Action::Reading(true))));
    // Left, it reads again as it opens.
    app.dispatch(Action::SelectRoom("discord:10:22".into()));
    assert_eq!(app.held_unread, None);
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
            ..Default::default()
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
