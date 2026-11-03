use super::{open_channel::*, render::*, *};
use crate::chat::ForwardFailure;
use yapfast_discord::ready::Deletes;

fn message(id: &str, nonce: Option<&str>) -> Message {
    Message {
        id: id.into(),
        channel_id: "11".into(),
        nonce: nonce.map(Into::into),
        author_id: "1".into(),
        content: id.into(),
        ..Default::default()
    }
}

#[test]
fn answers_discord_commands_while_discord_is_away() {
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    let why = "Discord couldn't start: the credential store is locked";
    // What dialogs wait for fails, so they stop waiting.
    for command in [
        DiscordCommand::AddFriend {
            username: "ann".into(),
        },
        DiscordCommand::SetCustomStatus(None),
        DiscordCommand::Profile {
            room: "discord:10:11".into(),
            user: "2".into(),
        },
    ] {
        without(Command::Discord(command), why, &events);
    }
    assert!(matches!(
        received.try_recv(),
        Ok(Event::FriendAdded(Err(error))) if error == why
    ));
    assert!(matches!(
        received.try_recv(),
        Ok(Event::CustomStatusSet(Err(error))) if error == why
    ));
    assert!(matches!(
        received.try_recv(),
        Ok(Event::Profile { result: Err(error), .. }) if error == why
    ));
    // So does a window of older messages the timeline waits for.
    without(
        Command::Around {
            room: "discord:10:11".into(),
            item: "d:5".into(),
        },
        why,
        &events,
    );
    assert!(matches!(received.try_recv(), Ok(Event::RoomError { .. })));
    assert!(matches!(
        received.try_recv(),
        Ok(Event::HistoryLoaded {
            direction: Direction::Around,
            history: History::Failed,
            ..
        })
    ));
}

#[test]
fn live_events_echoes_and_failed_sends() {
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    open.loaded(
        Load::Latest,
        shown(vec![message("3", None), message("5", None)]),
    );
    assert!(
        open.shown.complete,
        "a short first page is the whole channel"
    );
    assert!(open.live(&Live::Created(message("7", None))));
    assert!(!open.live(&Live::Created(Message {
        channel_id: "12".into(),
        ..message("8", None)
    })));
    assert!(open.live(&Live::Reaction {
        channel: "11".into(),
        message: "7".into(),
        key: "👍".into(),
        emoji: "👍".into(),
        me: true,
        added: true,
    }));
    let edited = Message {
        content: "changed".into(),
        edited: true,
        ..message("7", None)
    };
    open.edited(edited);
    let seven = &open.shown.messages[&7];
    assert_eq!(
        (seven.content.as_str(), seven.reactions.len()),
        ("changed", 1)
    );
    assert!(open.live(&Live::Deleted {
        channel: "11".into(),
        id: "3".into()
    }));
    let order: Vec<_> = open.shown.messages.keys().copied().collect();
    assert_eq!(order, [5, 7]);

    // A send that failed can be cancelled; one that arrives replaces its echo.
    for nonce in ["100", "101"] {
        open.sent.insert(nonce.into());
        open.pending.push(Pending {
            nonce: nonce.into(),
            body: "hi".into(),
            reply: None,
            options: Default::default(),
            files: vec![],
            failed: false,
        });
    }
    assert!(!open.cancel("n:100"), "still sending");
    assert_eq!(
        open.sent("100", Err("refused".into())),
        Some("refused".into())
    );
    assert!(open.cancel("n:100"));
    assert!(open.live(&Live::Created(message("9", Some("101")))));
    assert!(open.pending.is_empty());
    assert_eq!(sent_id("d:9"), Some("9"));
    assert_eq!(sent_id("n:101"), None);
}

fn ids(open: &OpenChannel) -> Vec<u64> {
    open.shown.messages.keys().copied().collect()
}

/// A page as Discord sends it, when Yapfast shows every message on it.
fn shown(messages: Vec<Message>) -> Page {
    Page {
        oldest: messages.first().map(Message::order),
        full: messages.len() >= yapfast_discord::api::PAGE,
        newest: false,
        messages,
    }
}

/// A page of messages with these snowflakes, oldest first.
fn page(ids: impl IntoIterator<Item = u64>) -> Vec<Message> {
    ids.into_iter()
        .map(|id| message(&id.to_string(), None))
        .collect()
}

#[test]
fn the_latest_page_fits_what_was_kept() {
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    // Two full pages kept: 1000 to 1099.
    open.loaded(Load::Latest, shown(page(1050..1100)));
    open.loaded(Load::Earlier, shown(page(1000..1050)));
    assert!(!open.shown.complete);

    // Since then 1060 was deleted, 1099 edited, and 1100 to 1109 came.
    let mut latest = page((1060..1110).filter(|&id| id != 1060));
    latest.insert(0, message("1059", None));
    latest
        .iter_mut()
        .find(|message| message.id == "1099")
        .unwrap()
        .content = "edited".into();
    open.loaded(Load::Latest, shown(latest));
    assert_eq!(
        ids(&open),
        (1000..1110).filter(|&id| id != 1060).collect::<Vec<_>>()
    );
    assert_eq!(open.shown.messages[&1099].content, "edited");

    // A full page entirely newer than what's kept leaves a gap: only it stays.
    open.loaded(Load::Latest, shown(page(2000..2050)));
    assert_eq!(ids(&open), (2000..2050).collect::<Vec<_>>());
    assert!(!open.shown.complete);

    // A short page is the whole channel: what it lacks was deleted.
    open.loaded(Load::Latest, shown(page([2010, 2049])));
    assert_eq!(ids(&open), [2010, 2049]);
    assert!(open.shown.complete);
    open.loaded(Load::Latest, shown(vec![]));
    assert!(ids(&open).is_empty(), "every message deleted");
}

#[test]
fn messages_yapfast_does_not_show_still_fill_a_page() {
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    // Discord sent fifty, the oldest a boost that Yapfast doesn't show.
    let boosted = |messages| Page {
        messages,
        oldest: Some(1050),
        full: true,
        newest: false,
    };
    open.loaded(Load::Latest, boosted(page(1051..1100)));
    assert!(
        !open.shown.complete,
        "a full page with a boost isn't the start"
    );
    assert_eq!(
        open.earliest(),
        Some(1050),
        "earlier ones load from before the boost"
    );

    // A whole page of boosts moves on past them.
    open.loaded(
        Load::Earlier,
        Page {
            messages: vec![],
            oldest: Some(1000),
            full: true,
            newest: false,
        },
    );
    assert!(!open.shown.complete);
    assert_eq!(open.earliest(), Some(1000));
    open.loaded(Load::Earlier, shown(page(990..1000)));
    assert!(open.shown.complete);
    assert_eq!(open.earliest(), Some(990));

    // The latest page starting at a boost after what's kept leaves no gap.
    open.loaded(Load::Latest, boosted(page(1051..1100)));
    assert_eq!(open.earliest(), Some(990));
}

#[test]
fn closed_channels_keep_metadata_without_pinning_previews() {
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    let with_image = |id: u64| Message {
        attachments: vec![yapfast_discord::message::Attachment {
            name: "a.png".into(),
            size: None,
            url: "".into(),
            image: true,
            dimensions: None,
            preview_url: Some(format!("preview/{id}")),
            view_url: None,
            still_url: None,
            video_url: None,
            shown: Shown::Message,
            link_url: None,
            animated: false,
            spoiler: false,
        }],
        ..message(&id.to_string(), None)
    };
    // Fifty more than it keeps, the newest page with pictures.
    let total = KEPT_MESSAGES as u64 + 50;
    open.loaded(
        Load::Latest,
        shown((total - 50..total).map(with_image).collect()),
    );
    for start in (0..(total - 50) / 50).rev().map(|page| page * 50) {
        open.loaded(Load::Earlier, shown(page(start..start + 50)));
    }
    for id in total - 50..total {
        let url = format!("preview/{id}");
        open.requested.insert(url.clone());
        open.picture(url, None);
    }
    open.shelve();
    assert_eq!(ids(&open), (50..total).collect::<Vec<_>>());
    assert!(!open.shown.complete, "the oldest went, so they load again");
    assert!(
        open.pictures.is_empty(),
        "closed channels do not own decoded pixels"
    );
    open.picture(format!("preview/{}", total - 30), None);
    assert!(
        open.pictures.is_empty(),
        "a preview it gave up arrives late"
    );
}

#[test]
fn changes_render_again_only_the_messages_they_change() {
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    let with_image = Message {
        attachments: vec![yapfast_discord::message::Attachment {
            name: "a.png".into(),
            size: None,
            url: "".into(),
            image: true,
            dimensions: None,
            preview_url: Some("preview/9".into()),
            view_url: None,
            still_url: None,
            video_url: None,
            shown: Shown::Message,
            link_url: None,
            animated: false,
            spoiler: false,
        }],
        ..message("9", None)
    };
    open.loaded(
        Load::Latest,
        shown([page([3, 5, 7]), vec![with_image]].concat()),
    );
    let show = |open: &mut OpenChannel| {
        for order in [3, 5, 7, 9] {
            open.rendered.insert(order, Arc::default());
        }
    };
    let kept = |open: &OpenChannel| {
        let mut kept: Vec<u64> = open.rendered.keys().copied().collect();
        kept.sort();
        kept
    };
    show(&mut open);
    open.live(&Live::Reaction {
        channel: "11".into(),
        message: "5".into(),
        key: "👍".into(),
        emoji: "👍".into(),
        me: false,
        added: true,
    });
    assert_eq!(kept(&open), [3, 7, 9]);
    show(&mut open);
    open.edited(Message {
        content: "changed".into(),
        ..message("7", None)
    });
    open.live(&Live::Created(message("11", None)));
    assert_eq!(kept(&open), [3, 5, 9], "an edit, and a new message");
    show(&mut open);
    open.requested.insert("preview/9".into());
    open.picture("preview/9".into(), None);
    assert_eq!(kept(&open), [3, 5, 7], "its preview arrived");
    open.live(&Live::Names);
    assert_eq!(kept(&open), [3, 5, 7], "publishing checks for new names");
    open.shelve();
    assert!(open.rendered.is_empty(), "closed, it keeps fewer previews");
}

#[test]
fn votes_count_live_and_last_through_an_update_without_them() {
    use yapfast_discord::message::{Poll, PollAnswer};
    let poll = |counted, votes: [usize; 2]| Poll {
        question: "Lunch?".into(),
        answers: votes
            .iter()
            .zip(1..)
            .map(|(votes, id)| PollAnswer {
                id,
                votes: *votes,
                ..Default::default()
            })
            .collect(),
        counted,
        ..Default::default()
    };
    let asked = |poll| Message {
        poll: Some(poll),
        ..message("5", None)
    };
    let votes = |open: &OpenChannel| -> Vec<(usize, bool)> {
        open.shown.messages[&5]
            .poll
            .as_ref()
            .unwrap()
            .answers
            .iter()
            .map(|answer| (answer.votes, answer.me))
            .collect()
    };
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    open.loaded(Load::Latest, shown(vec![asked(poll(true, [1, 0]))]));
    let vote = |answer, me, added| Live::Vote {
        channel: "11".into(),
        message: "5".into(),
        answer,
        me,
        added,
    };
    assert!(open.live(&vote(2, true, true)));
    assert!(open.live(&vote(1, false, false)));
    assert_eq!(votes(&open), [(0, false), (1, true)]);
    // An update without the votes keeps those counted; one with them
    // takes them, as when the poll closes.
    assert!(open.live(&Live::Updated(asked(poll(false, [0, 0])))));
    assert_eq!(votes(&open), [(0, false), (1, true)]);
    assert!(open.live(&Live::Updated(asked(poll(true, [4, 2])))));
    assert_eq!(votes(&open), [(4, false), (2, false)]);
}

#[test]
fn messages_render_again_once_what_they_show_changes() {
    use yapfast_discord::marked::{ChannelMarks, MarkedServer, ServerMarks};
    let server = |name: &str, unread: bool| {
        let server = yapfast_discord::ready::Server {
            id: "10".into(),
            text_channels: vec![yapfast_discord::ready::Channel {
                id: "11".into(),
                name: name.into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        Some(MarkedServer::marked(server, ServerMarks::default(), |_| {
            ChannelMarks {
                unread,
                ..Default::default()
            }
        }))
    };
    let rendering = |names: u64, server| Rendering {
        names,
        me: "1".into(),
        blocked: vec![].into(),
        server,
        may_pin: false,
        may_forward: false,
        deletes: Deletes::Own,
    };
    let shown = rendering(1, server("general", false));
    assert!(
        shown.alike(&rendering(1, server("general", true))),
        "unread"
    );
    assert!(
        !shown.alike(&rendering(1, server("chat", false))),
        "renamed"
    );
    // This account's roles decide which mentions call on it.
    let mut roles = shown.server.clone().unwrap();
    Arc::make_mut(&mut roles.marks).roles = vec!["20".into()];
    assert!(!shown.alike(&rendering(1, Some(roles))), "roles");
    assert!(
        !shown.alike(&rendering(2, server("general", false))),
        "names"
    );
    assert!(!shown.alike(&Rendering {
        blocked: vec!["2".into()].into(),
        ..rendering(1, server("general", false))
    }));
    assert!(
        !shown.alike(&Rendering {
            may_pin: true,
            ..rendering(1, server("general", false))
        }),
        "may pin"
    );
    assert!(
        !shown.alike(&Rendering {
            may_forward: true,
            ..rendering(1, server("general", false))
        }),
        "may forward"
    );
    assert!(
        !shown.alike(&Rendering {
            deletes: Deletes::Any,
            ..rendering(1, server("general", false))
        }),
        "may delete others' messages"
    );
}

#[test]
fn channels_go_to_disk_once_they_change() {
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    assert!(open.take_changes().is_none(), "nothing to save yet");
    open.loaded(Load::Latest, shown(page(1..=3)));
    let (channel, saved) = open.take_changes().unwrap();
    assert_eq!(channel, "11");
    assert_eq!(saved.len(), 3);
    assert!(open.take_changes().is_none(), "saved, and unchanged since");
    assert!(!open.live(&Live::Deleted {
        channel: "11".into(),
        id: "9".into()
    }));
    assert!(open.take_changes().is_none(), "a message it didn't have");
    open.deleted("2");
    let (_, saved) = open.take_changes().unwrap();
    assert_eq!(
        saved
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        ["1", "3"]
    );
}

#[test]
fn a_page_asked_for_before_a_delete_doesnt_bring_the_message_back() {
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    open.loaded(Load::Latest, shown(page(1..=3)));
    // Someone deletes one, and this account another, while a page asked
    // for before is on its way.
    open.live(&Live::Deleted {
        channel: "11".into(),
        id: "2".into(),
    });
    open.deleted("3");
    open.loaded(Load::Latest, shown(page(1..=4)));
    assert!(open.shown.messages.keys().eq(&[1, 4]));
}

#[test]
fn a_pin_marks_its_message_and_goes_to_disk() {
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    open.loaded(Load::Latest, shown(page(1..=3)));
    open.take_changes();
    open.pinned("2", true);
    assert!(open.message(2).unwrap().pinned);
    let (_, saved) = open.take_changes().unwrap();
    assert!(
        saved
            .iter()
            .any(|message| message.id == "2" && message.pinned)
    );
    open.pinned("2", true);
    assert!(open.take_changes().is_none(), "pinned already");
    open.pinned("2", false);
    assert!(!open.message(2).unwrap().pinned);
}

#[test]
fn closed_channels_follow_live_events_and_open_again() {
    let mut channels = Channels::default();
    let mut first = OpenChannel::new("discord:10:11").unwrap();
    first.loaded(Load::Latest, shown(page([3, 5])));
    channels.open = Some(first);
    channels.close();
    assert!(channels.open.is_none());
    channels.follow(&Live::Created(message("7", None)));
    assert!(channels.get_mut("discord:10:11").is_some());
    let again = channels.reopen("discord:10:11").unwrap();
    assert_eq!(ids(&again), [3, 5, 7]);
    assert!(again.loaded);
    assert!(channels.reopen("discord:10:11").is_none(), "taken out");
    for channel in 0..=KEPT_CHANNELS as u64 {
        channels.open = OpenChannel::new(&format!("discord:10:{channel}"));
        channels.close();
    }
    assert_eq!(channels.recent.len(), KEPT_CHANNELS);
    assert!(
        channels.get_mut("discord:10:0").is_none(),
        "the oldest went"
    );
}

#[test]
fn a_busy_closed_channel_keeps_only_its_newest_messages() {
    let mut channels = Channels::default();
    let mut busy = OpenChannel::new("discord:10:11").unwrap();
    busy.loaded(Load::Latest, shown(page([1, 2])));
    channels.open = Some(busy);
    channels.close();
    let total = KEPT_MESSAGES as u64 + 50;
    for id in 3..=total {
        channels.follow(&Live::Created(message(&id.to_string(), None)));
    }
    let busy = channels.reopen("discord:10:11").unwrap();
    assert_eq!(ids(&busy), (51..=total).collect::<Vec<_>>());
    assert!(!busy.shown.complete, "the oldest went, so they load again");
}

#[test]
fn a_busy_open_channel_lets_go_of_messages_well_above_the_view() {
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    open.loaded(Load::Latest, shown(page([1, 2])));
    let view = |open: &mut OpenChannel, from: u64, to: u64| {
        open.visible((from..=to).map(|id| format!("d:{id}")).collect());
    };
    // The timeline follows its newest message, and says so as each comes.
    let come = |open: &mut OpenChannel, ids: std::ops::RangeInclusive<u64>| {
        for id in ids {
            open.live(&Live::Created(message(&id.to_string(), None)));
            open.let_go_above_view();
        }
    };
    // Without a view, nothing is known to be off screen.
    come(&mut open, 3..=400);
    assert_eq!(open.shown.messages.len(), 400);
    // Following the newest, it keeps the newest it keeps when closed.
    view(&mut open, 390, 400);
    come(&mut open, 401..=401);
    assert_eq!(ids(&open), (202..=401).collect::<Vec<_>>());
    assert!(!open.shown.complete, "the oldest went, so they load again");
    come(&mut open, 402..=500);
    assert_eq!(open.shown.messages.len(), 299, "not each time one comes");
    // Those in view and some above them stay, wherever the view is.
    view(&mut open, 220, 230);
    come(&mut open, 501..=600);
    assert_eq!(ids(&open).first(), Some(&202));
    view(&mut open, 400, 410);
    come(&mut open, 601..=601);
    assert_eq!(ids(&open).first(), Some(&350));
}

#[test]
fn a_window_of_older_messages_shows_in_place_of_the_newest_until_it_reaches_them() {
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    let newest = |open: &OpenChannel| -> Vec<u64> {
        open.latest
            .as_ref()
            .map(|latest| latest.messages.keys().copied().collect())
            .unwrap_or_default()
    };
    assert!(!open.synced);
    open.loaded(Load::Latest, shown(page(1000..1050)));
    assert!(open.synced, "the latest page came");
    // Around a message among the newest shown, they're what shows around
    // it, without asking.
    assert!(open.reaches(1020) && open.reaches(1000));
    assert!(!open.reaches(999));
    open.loaded(Load::Around(900), shown(page(875..925)));
    assert!(!open.reaches(900), "not while a window shows");
    assert!(open.detached());
    assert!(!open.shown.complete, "a whole half before the snowflake");
    assert_eq!(ids(&open), (875..925).collect::<Vec<_>>());
    assert_eq!(newest(&open), (1000..1050).collect::<Vec<_>>(), "kept");

    // New messages join the newest kept, not the window; edits show in both.
    assert!(!open.live(&Live::Created(message("1050", None))));
    assert_eq!(newest(&open).last(), Some(&1050));
    assert_eq!(ids(&open).last(), Some(&924));
    assert!(open.live(&Live::Updated(Message {
        content: "edited".into(),
        ..message("900", None)
    })));
    assert_eq!(open.shown.messages[&900].content, "edited");
    let (_, saved) = open.take_changes().unwrap();
    assert_eq!(
        saved.last().map(|message| message.id.as_str()),
        Some("1050"),
        "the newest go to disk, not the window"
    );

    // Earlier pages grow it up to what a closed channel keeps, letting go
    // of its newest messages, which load again.
    for start in [825, 775, 725, 675] {
        open.loaded(Load::Earlier, shown(page(start..start + 50)));
    }
    assert_eq!(ids(&open), (675..875).collect::<Vec<_>>());
    // Newer pages keep the oldest, above what's on screen.
    open.loaded(Load::Later, shown(page(875..925)));
    open.loaded(Load::Later, shown(page(925..975)));
    assert_eq!(ids(&open), (675..975).collect::<Vec<_>>());
    assert!(open.detached());
    // One that reaches the newest kept shows them with it, as they are.
    open.loaded(Load::Later, shown(page(975..1025)));
    assert!(!open.detached());
    assert_eq!(ids(&open).last(), Some(&1050));
    assert_eq!(open.shown.messages[&1050].content, "1050");
    assert!(open.live(&Live::Created(message("1051", None))));

    // Back to the present, the newest show again as they were kept.
    open.loaded(Load::Around(500), shown(page(475..525)));
    let generation = open.generation;
    open.present();
    assert!(!open.detached());
    assert_eq!(ids(&open).first(), Some(&675), "the window went");
    assert_ne!(
        open.generation, generation,
        "pages for the window are dropped"
    );

    // A short page after a window is the newest, even with nothing kept.
    let mut fresh = OpenChannel::new("discord:10:11").unwrap();
    fresh.loaded(Load::Around(10), shown(page(5..15)));
    assert!(fresh.detached());
    assert!(
        fresh.shown.complete,
        "too few before the snowflake: the start"
    );
    fresh.loaded(Load::Later, shown(page(15..20)));
    assert!(!fresh.detached());
    assert_eq!(ids(&fresh), (5..20).collect::<Vec<_>>());

    // Less than half a page from the snowflake on reaches the newest
    // message, so it isn't a window, and the newest kept join it.
    let mut opened = OpenChannel::new("discord:10:11").unwrap();
    opened.loaded(Load::Latest, shown(page(100..150)));
    opened.loaded(
        Load::Around(130),
        Page {
            newest: true,
            ..shown(page(105..140))
        },
    );
    assert!(!opened.detached());
    assert_eq!(ids(&opened), (100..150).collect::<Vec<_>>());

    // Closing lets the window go.
    fresh.loaded(Load::Latest, shown(page(100..150)));
    fresh.loaded(Load::Around(7), shown(page(5..55)));
    assert!(fresh.detached());
    fresh.shelve();
    assert!(!fresh.detached());
    assert_eq!(ids(&fresh), (100..150).collect::<Vec<_>>());
}

#[test]
fn pages_for_what_showed_before_are_dropped() {
    let mut channels = Channels::default();
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    open.loaded(Load::Latest, shown(page(100..150)));
    open.loading_earlier = true;
    open.present();
    open.generation += 1;
    channels.open = Some(open);
    let (done, _) = tokio::sync::mpsc::unbounded_channel();
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    finished(
        Done::History {
            room: "discord:10:11".into(),
            load: Load::Earlier,
            generation: 0,
            result: Ok(shown(page(50..100))),
        },
        None,
        &mut channels,
        &done,
        &events,
    );
    assert!(received.try_recv().is_err(), "nothing is said of it");
    assert_eq!(
        ids(channels.open.as_ref().unwrap()),
        (100..150).collect::<Vec<_>>()
    );
}

/// The fake Discord's server and its #general.
const FAKE_GUILD: &str = "1400000000000000100";
const FAKE_GENERAL: &str = "1400000000000000102";

/// Discord connected to the fake server, once it's online. Run the tests
/// that use it with scripts/test-fake-discord.
async fn fake_discord() -> Discord {
    assert!(
        yapfast_discord::client::test_server().is_some(),
        "set YAPFAST_DISCORD_TEST_SERVER to the fake Discord"
    );
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let discord = Discord::start(yapfast_discord::token::load().unwrap()).unwrap();
    let mut snapshots = discord.snapshots();
    tokio::time::timeout(
        Duration::from_secs(10),
        snapshots.wait_for(|snapshot| snapshot.status == Status::Online),
    )
    .await
    .expect("the fake Discord answers")
    .unwrap();
    discord
}

/// Applies finished work until `found` picks an event it led to.
async fn until<T>(
    discord: &Discord,
    channels: &mut Channels,
    done: &UnboundedSender<Done>,
    finished: &mut UnboundedReceiver<Done>,
    events: &mut UnboundedReceiver<Event>,
    found: impl Fn(&Event) -> Option<T>,
) -> T {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            while let Ok(event) = events.try_recv() {
                if let Some(found) = found(&event) {
                    return found;
                }
                if let Event::RoomError { message, .. } = event {
                    panic!("{message}");
                }
            }
            let work = finished.recv().await.expect("work goes on");
            let (to, mut from) = tokio::sync::mpsc::unbounded_channel();
            super::finished(work, Some(discord), channels, done, &to);
            drop(to);
            while let Some(event) = from.recv().await {
                if let Some(found) = found(&event) {
                    return found;
                }
                if let Event::RoomError { message, .. } = event {
                    panic!("{message}");
                }
            }
        }
    })
    .await
    .expect("timed out waiting for Discord")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
async fn earlier_history_loads_a_page_at_a_time_until_the_first_message() {
    let discord = fake_discord().await;
    let (done, mut finished) = tokio::sync::mpsc::unbounded_channel();
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    let mut channels = Channels::default();
    let room = discord::room_id(FAKE_GUILD, FAKE_GENERAL);
    command(
        Command::Open(room.clone()),
        Ok(&discord),
        &mut channels,
        &done,
        &events,
    );
    let first = until(
        &discord,
        &mut channels,
        &done,
        &mut finished,
        &mut received,
        |event| match event {
            Event::Timeline { messages, .. } => Some(messages.len()),
            _ => None,
        },
    )
    .await;
    // The fake's boost is on it, unless the service tests have posted since.
    let page = yapfast_discord::api::PAGE;
    assert!(
        (page - 1..=page).contains(&first),
        "the latest page, less a boost on it"
    );
    let mut pages = 0;
    let history = loop {
        // Asking twice while a page loads loads it once.
        for _ in 0..2 {
            command(
                Command::Earlier(room.clone()),
                Ok(&discord),
                &mut channels,
                &done,
                &events,
            );
        }
        let history = until(
            &discord,
            &mut channels,
            &done,
            &mut finished,
            &mut received,
            |event| match event {
                Event::HistoryLoaded {
                    direction: Direction::Earlier,
                    history,
                    ..
                } => Some(*history),
                _ => None,
            },
        )
        .await;
        pages += 1;
        tokio::time::sleep(Duration::from_millis(200)).await;
        while let Ok(work) = finished.try_recv() {
            assert!(
                !matches!(
                    work,
                    Done::History {
                        load: Load::Earlier,
                        ..
                    }
                ),
                "no second page was asked for"
            );
            super::finished(work, Some(&discord), &mut channels, &done, &events);
        }
        let more = History::Loaded {
            start: false,
            present: true,
        };
        if history != more || pages > 20 {
            break history;
        }
    };
    let start = History::Loaded {
        start: true,
        present: true,
    };
    assert_eq!(history, start);
    let open = channels.open.as_ref().unwrap();
    assert!(open.shown.complete);
    assert!(
        open.shown
            .messages
            .values()
            .next()
            .unwrap()
            .content
            .starts_with("Welcome to **Yapfast**"),
        "the channel's first message is loaded"
    );
    // Once it is, asking again says so at once.
    while received.try_recv().is_ok() {}
    command(
        Command::Earlier(room.clone()),
        Ok(&discord),
        &mut channels,
        &done,
        &events,
    );
    assert!(matches!(
        received.try_recv(),
        Ok(Event::HistoryLoaded { history, .. }) if history == start
    ));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
async fn a_channel_reopened_in_step_with_live_events_asks_for_nothing() {
    let discord = fake_discord().await;
    let (done, mut finished) = tokio::sync::mpsc::unbounded_channel();
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    let mut channels = Channels::default();
    let room = discord::room_id(FAKE_GUILD, FAKE_GENERAL);
    let run = |command: Command, channels: &mut Channels| {
        super::command(command, Ok(&discord), channels, &done, &events);
    };
    run(Command::Open(room.clone()), &mut channels);
    until(
        &discord,
        &mut channels,
        &done,
        &mut finished,
        &mut received,
        |event| match event {
            Event::HistoryLoaded {
                direction: Direction::Latest,
                ..
            } => Some(()),
            _ => None,
        },
    )
    .await;
    // Closed and opened again, it shows what it kept and says its newest
    // messages are in, without asking Discord.
    let reopen = |channels: &mut Channels, received: &mut UnboundedReceiver<Event>| {
        run(Command::CloseRoom, channels);
        while received.try_recv().is_ok() {}
        run(Command::Open(room.clone()), channels);
        std::iter::from_fn(|| received.try_recv().ok()).any(|event| {
            matches!(
                event,
                Event::HistoryLoaded {
                    direction: Direction::Latest,
                    history: History::Loaded { present: true, .. },
                    ..
                }
            )
        })
    };
    assert!(reopen(&mut channels, &mut received), "said at once");
    tokio::time::sleep(Duration::from_millis(300)).await;
    while let Ok(work) = finished.try_recv() {
        assert!(
            !matches!(
                work,
                Done::History {
                    load: Load::Latest,
                    ..
                }
            ),
            "the latest page wasn't asked for again"
        );
    }
    // Once live events may have gone by without it, it asks again.
    run(Command::CloseRoom, &mut channels);
    channels.missed();
    assert!(!reopen(&mut channels, &mut received), "not until it came");
    let work = tokio::time::timeout(Duration::from_secs(5), finished.recv())
        .await
        .expect("the latest page loads")
        .unwrap();
    assert!(matches!(
        work,
        Done::History {
            load: Load::Latest,
            ..
        }
    ));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
async fn a_thread_beside_the_open_channel_shows_its_own_pages() {
    let discord = fake_discord().await;
    let (done, mut finished) = tokio::sync::mpsc::unbounded_channel();
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    let mut channels = Channels::default();
    let general = discord::room_id(FAKE_GUILD, FAKE_GENERAL);
    let plans = discord
        .snapshots()
        .borrow()
        .servers
        .iter()
        .flat_map(|server| &server.threads)
        .find(|thread| thread.name == "Plans")
        .map(|thread| discord::room_id(FAKE_GUILD, &thread.id))
        .expect("the fake's Plans thread");
    command(
        Command::Open(general.clone()),
        Ok(&discord),
        &mut channels,
        &done,
        &events,
    );
    command(
        Command::Aside(Some(plans.clone())),
        Ok(&discord),
        &mut channels,
        &done,
        &events,
    );
    // What was kept on disk shows at once; the page Discord sends follows.
    while received.try_recv().is_ok() {}
    let authors = until(
        &discord,
        &mut channels,
        &done,
        &mut finished,
        &mut received,
        |event| match event {
            Event::Timeline { room, messages } if *room == plans => Some(
                messages
                    .iter()
                    .map(|message| message.author.clone())
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        },
    )
    .await;
    assert!(
        authors.iter().any(|author| author == "Carol"),
        "{authors:?}"
    );
    assert_eq!(channels.open.as_ref().unwrap().room, general);
    command(
        Command::Aside(None),
        Ok(&discord),
        &mut channels,
        &done,
        &events,
    );
    assert!(channels.aside.is_none());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
async fn a_jump_shows_the_messages_around_one_then_newer_pages_to_the_present() {
    let discord = fake_discord().await;
    let (done, mut finished) = tokio::sync::mpsc::unbounded_channel();
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    let mut channels = Channels::default();
    let room = discord::room_id(FAKE_GUILD, FAKE_GENERAL);
    // Opened by hand, without its latest page, so the window of the fake's
    // short history doesn't reach the newest messages kept at once.
    channels.open = OpenChannel::new(&room);
    let welcome = discord
        .pins(FAKE_GUILD, FAKE_GENERAL)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.content.starts_with("Welcome to **Yapfast**"))
        .expect("the fake pins Bob's welcome")
        .id;
    let run = |command: Command, channels: &mut Channels| {
        super::command(command, Ok(&discord), channels, &done, &events);
    };
    run(
        Command::Around {
            room: room.clone(),
            item: format!("d:{welcome}"),
        },
        &mut channels,
    );
    // Said before the window is published, which then shows the message.
    let said = std::cell::Cell::new(false);
    let window = until(
        &discord,
        &mut channels,
        &done,
        &mut finished,
        &mut received,
        |event| match event {
            Event::HistoryLoaded {
                direction: Direction::Around,
                history,
                ..
            } => {
                // The window starts at the first message, which Discord
                // tells by sending less than half a page from before it.
                assert_eq!(
                    *history,
                    History::Loaded {
                        start: true,
                        present: false
                    }
                );
                said.set(true);
                None
            }
            Event::Timeline { messages, .. } => {
                assert!(said.get(), "the window comes after it's said");
                Some(messages.clone())
            }
            _ => None,
        },
    )
    .await;
    assert_eq!(window[0].item, format!("d:{welcome}"), "the first message");
    assert!(
        window.len() <= yapfast_discord::api::PAGE / 2 + 1,
        "half a page of newer messages, and nothing before the first"
    );
    assert!(channels.open.as_ref().unwrap().detached());
    // Newer pages load as the window scrolls down, until the newest.
    let mut pages = 0;
    let history = loop {
        run(Command::Later(room.clone()), &mut channels);
        let history = until(
            &discord,
            &mut channels,
            &done,
            &mut finished,
            &mut received,
            |event| match event {
                Event::HistoryLoaded {
                    direction: Direction::Later,
                    history,
                    ..
                } => Some(*history),
                _ => None,
            },
        )
        .await;
        pages += 1;
        if history
            != (History::Loaded {
                start: true,
                present: false,
            })
            || pages > 5
        {
            break history;
        }
    };
    assert!(
        matches!(history, History::Loaded { present: true, .. }),
        "{history:?}"
    );
    let open = channels.open.as_ref().unwrap();
    assert!(!open.detached());
    let ids: Vec<u64> = open.shown.messages.keys().copied().collect();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(ids.len() > window.len(), "the newer pages joined it");
    // Asked again, it says at once that it's at the present.
    run(Command::Later(room.clone()), &mut channels);
    assert!(
        std::iter::from_fn(|| received.try_recv().ok()).any(|event| matches!(
            event,
            Event::HistoryLoaded {
                direction: Direction::Later,
                history: History::Loaded { present: true, .. },
                ..
            }
        ))
    );
}

#[test]
fn spoilers_go_to_discord_named_as_spoilers() {
    assert_eq!(upload_name("cat.png", true), "SPOILER_cat.png");
    assert_eq!(upload_name("SPOILER_cat.png", true), "SPOILER_cat.png");
    assert_eq!(upload_name("cat.png", false), "cat.png");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
async fn a_file_marked_as_a_spoiler_arrives_as_one() {
    let discord = fake_discord().await;
    let (done, mut finished) = tokio::sync::mpsc::unbounded_channel();
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    let mut channels = Channels::default();
    let room = discord::room_id(FAKE_GUILD, FAKE_GENERAL);
    command(
        Command::Open(room.clone()),
        Ok(&discord),
        &mut channels,
        &done,
        &events,
    );
    until(
        &discord,
        &mut channels,
        &done,
        &mut finished,
        &mut received,
        |event| matches!(event, Event::Timeline { .. }).then_some(()),
    )
    .await;
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("plot.txt");
    std::fs::write(&path, "the butler did it").unwrap();
    command(
        Command::Send {
            room: room.clone(),
            body: String::new(),
            reply: None,
            options: Default::default(),
            files: vec![OutgoingFile {
                path,
                spoiler: true,
            }],
        },
        Ok(&discord),
        &mut channels,
        &done,
        &events,
    );
    // Covered while it sends, and once Discord has it.
    let spoiler = |delivery: Delivery| {
        move |event: &Event| match event {
            Event::Timeline { messages, .. } => messages
                .iter()
                .find(|message| {
                    message.delivery == delivery
                        && message
                            .attachments
                            .iter()
                            .any(|file| file.name.ends_with("plot.txt"))
                })
                .map(|message| message.attachments[0].clone()),
            _ => None,
        }
    };
    let sending = until(
        &discord,
        &mut channels,
        &done,
        &mut finished,
        &mut received,
        spoiler(Delivery::Sending),
    )
    .await;
    assert!(sending.spoiler);
    let sent = until(
        &discord,
        &mut channels,
        &done,
        &mut finished,
        &mut received,
        spoiler(Delivery::Sent),
    )
    .await;
    assert!(sent.spoiler && sent.name == "SPOILER_plot.txt");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
async fn a_sent_picture_has_a_link_and_downloads_to_copy() {
    let discord = fake_discord().await;
    let (done, mut finished) = tokio::sync::mpsc::unbounded_channel();
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    let mut channels = Channels::default();
    let room = discord::room_id(FAKE_GUILD, FAKE_GENERAL);
    command(
        Command::Open(room.clone()),
        Ok(&discord),
        &mut channels,
        &done,
        &events,
    );
    until(
        &discord,
        &mut channels,
        &done,
        &mut finished,
        &mut received,
        |event| matches!(event, Event::Timeline { .. }).then_some(()),
    )
    .await;
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("copied.png");
    let picture = b"\x89PNG not really".to_vec();
    std::fs::write(&path, &picture).unwrap();
    command(
        Command::Send {
            room: room.clone(),
            body: String::new(),
            reply: None,
            options: Default::default(),
            files: vec![OutgoingFile {
                path,
                spoiler: false,
            }],
        },
        Ok(&discord),
        &mut channels,
        &done,
        &events,
    );
    let (item, url) = until(
        &discord,
        &mut channels,
        &done,
        &mut finished,
        &mut received,
        |event| match event {
            Event::Timeline { messages, .. } => messages
                .iter()
                .find(|message| {
                    message.delivery == Delivery::Sent
                        && message
                            .attachments
                            .iter()
                            .any(|file| file.name == "copied.png")
                })
                .map(|message| (message.item.clone(), message.attachments[0].url.clone())),
            _ => None,
        },
    )
    .await;
    assert!(url.is_some_and(|url| url.ends_with("copied.png")));
    command(
        Command::CopyAttachment {
            room,
            item,
            index: 0,
        },
        Ok(&discord),
        &mut channels,
        &done,
        &events,
    );
    let copied = until(
        &discord,
        &mut channels,
        &done,
        &mut finished,
        &mut received,
        |event| match event {
            Event::Copied(copied) => Some(copied.clone()),
            _ => None,
        },
    )
    .await;
    assert_eq!(copied, Ok(picture));
}

#[test]
fn changing_the_view_drops_old_previews_and_ignores_late_downloads() {
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    open.loaded(
        Load::Latest,
        shown(vec![
            Message {
                attachments: vec![yapfast_discord::message::Attachment {
                    name: "a.png".into(),
                    size: None,
                    url: "image".into(),
                    image: true,
                    dimensions: Some((1, 1)),
                    preview_url: Some("preview".into()),
                    view_url: None,
                    still_url: None,
                    video_url: None,
                    shown: Shown::Message,
                    link_url: None,
                    animated: false,
                    spoiler: false,
                }],
                ..message("1", None)
            },
            message("2", None),
        ]),
    );
    assert!(open.visible(vec!["d:1".into()]));
    assert!(!open.visible(vec!["d:1".into()]));
    open.requested.insert("preview".into());
    let picture = Arc::new(Picture {
        width: 1,
        height: 1,
        rgba: crate::chat::Pixels::new(1, 1, vec![0; 4]),
        ..Default::default()
    });
    let weak = Arc::downgrade(&picture);
    open.picture("preview".into(), Some(picture));
    assert!(open.pictures["preview"].is_some());
    open.rendered.insert(1, Arc::default());
    open.rendered.insert(2, Arc::default());
    assert!(open.visible(vec!["d:2".into()]));
    assert!(open.pictures.is_empty() && open.requested.is_empty());
    assert_eq!(
        open.rendered.keys().collect::<Vec<_>>(),
        [&2],
        "a row without a preview shows the same anywhere"
    );
    open.picture("preview".into(), Some(Arc::new(Picture::default())));
    assert!(
        open.pictures.is_empty(),
        "a late download cannot repopulate old rows"
    );
    let cached = open.previews.lock().unwrap().get("preview").unwrap();
    assert!(Arc::ptr_eq(&cached, &weak.upgrade().unwrap()));
    drop(cached);
    assert_eq!(
        weak.strong_count(),
        1,
        "only the shared cache owns the decoded pixels"
    );
    open.shelve();
    assert_eq!(weak.strong_count(), 1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
async fn previews_follow_the_view_and_share_cache_across_channels() {
    let discord = fake_discord().await;
    let (done, mut finished) = tokio::sync::mpsc::unbounded_channel();
    let previews = channels::SharedPreviews::default();
    let with_image = |id: &str| Message {
        attachments: vec![yapfast_discord::message::Attachment {
            name: "a.png".into(),
            size: None,
            url: "".into(),
            image: true,
            dimensions: Some((720, 540)),
            preview_url: Some(format!(
                "{}/external/viewport-{id}/720x540.png",
                yapfast_discord::client::test_server().unwrap()
            )),
            view_url: None,
            still_url: None,
            video_url: None,
            shown: Shown::Message,
            link_url: None,
            animated: false,
            spoiler: false,
        }],
        ..message(id, None)
    };
    let mut open = OpenChannel::new("discord:10:11").unwrap();
    open.previews = previews.clone();
    open.loaded(Load::Latest, shown(vec![with_image("1"), with_image("2")]));
    assert!(
        open.publish(&discord, &done)
            .iter()
            .all(|row| row.attachments[0].preview.is_none())
    );
    assert!(
        open.requested.is_empty(),
        "loading history alone does not download images"
    );
    open.visible(vec!["d:1".into()]);
    open.publish(&discord, &done);
    assert_eq!(open.requested.len(), 1);
    let work = tokio::time::timeout(Duration::from_secs(10), finished.recv())
        .await
        .unwrap()
        .unwrap();
    let Done::Picture { url, picture, .. } = work else {
        panic!("expected the visible preview")
    };
    let source = picture.as_ref().unwrap().clone();
    open.picture(url, picture);
    let rows = open.publish(&discord, &done);
    assert!(Arc::ptr_eq(
        rows[0].attachments[0].preview.as_ref().unwrap(),
        &source
    ));
    assert!(rows[1].attachments[0].preview.is_none());
    drop(rows);
    open.shelve();
    assert!(open.pictures.is_empty() && open.rendered.is_empty());
    // A different chat using the same URL also reuses the cached allocation.
    let mut other = OpenChannel::new("discord:10:12").unwrap();
    other.previews = previews;
    other.loaded(Load::Latest, shown(vec![with_image("1")]));
    other.visible(vec!["d:1".into()]);
    let rows = other.publish(&discord, &done);
    assert!(Arc::ptr_eq(
        rows[0].attachments[0].preview.as_ref().unwrap(),
        &source
    ));
    assert!(
        other.requested.is_empty(),
        "cache hits do not start another download"
    );
    other.visible(vec![]);
    assert!(
        other.publish(&discord, &done)[0].attachments[0]
            .preview
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
async fn a_server_channels_pins_and_search_show_its_members() {
    let discord = fake_discord().await;
    let (done, _finished) = tokio::sync::mpsc::unbounded_channel();
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    let mut channels = Channels::default();
    let room = discord::room_id(FAKE_GUILD, FAKE_GENERAL);
    // These answer straight from the account, with no channel work; with
    // no command, the next answer comes.
    let mut run = async |command: Option<Command>| {
        if let Some(command) = command {
            self::command(command, Ok(&discord), &mut channels, &done, &events);
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let event = received.recv().await.expect("events go on");
                if matches!(event, Event::Pins { .. } | Event::Found { .. }) {
                    return event;
                }
            }
        })
        .await
        .expect("timed out waiting for Discord")
    };
    let Event::Pins {
        result: Ok(pins), ..
    } = run(Some(Command::Discord(DiscordCommand::Pins {
        room: room.clone(),
    })))
    .await
    else {
        panic!("the pins came")
    };
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].author, "Bob");
    assert_eq!(pins[0].color, Some(0x3498db), "Bob is a Mod");
    // Then again with their pictures, of which the welcome has none.
    let Event::Pins {
        result: Ok(again), ..
    } = run(None).await
    else {
        panic!("the pins came again")
    };
    assert_eq!(again, pins);

    // A search of the whole server, each result with its channel.
    let search = |query: &str| {
        Command::Discord(DiscordCommand::Search {
            place: discord::server_id(FAKE_GUILD),
            query: query.into(),
            offset: 0,
        })
    };
    let found = |event| match event {
        Event::Found { result, .. } => result,
        _ => panic!("expected search results"),
    };
    let (total, hits) = found(run(Some(search("welcome"))).await).unwrap();
    assert_eq!(total, 2);
    assert_eq!(hits[0].room, room);
    let first = &hits[0].message;
    assert_eq!(first.body, "The welcome has the plan");
    assert_eq!(first.author, "Bob");
    assert_eq!(first.color, Some(0x3498db));
    assert!(
        first
            .avatar
            .as_deref()
            .is_some_and(|avatar| avatar.contains("/avatars/1400000000000000002/"))
    );

    // Filters name people and channels, which Yapfast finds by id.
    let (total, hits) =
        found(run(Some(search("welcome from:bob in:#general has:link"))).await).unwrap();
    assert_eq!(total, 1, "Bob's welcome carries a link");
    assert!(hits[0].message.body.starts_with("Welcome to **Yapfast**"));
    let refused = found(run(Some(search("from:nobody"))).await).unwrap_err();
    assert!(refused.contains("nobody"), "{refused}");
    let refused = found(run(Some(search("in:nowhere"))).await).unwrap_err();
    assert!(refused.contains("#nowhere"), "{refused}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testing/fake_discord.ts and YAPFAST_DISCORD_TEST_SERVER"]
async fn forwards_with_the_message_added_and_refuses_what_cant_go() {
    let discord = fake_discord().await;
    let (done, _finished) = tokio::sync::mpsc::unbounded_channel();
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    let mut channels = Channels::default();
    let mut live = discord.live();
    let room = discord::room_id(FAKE_GUILD, FAKE_GENERAL);
    let latest = discord
        .history(FAKE_GUILD, FAKE_GENERAL, yapfast_discord::api::Span::Latest)
        .await
        .unwrap();
    let original = latest
        .messages
        .iter()
        .rfind(|message| message.forwardable && !message.content.is_empty())
        .unwrap();
    let mut forward = |item: String, target: &str, note: Option<String>| {
        let command = Command::Forward {
            room: room.clone(),
            item,
            target: target.into(),
            note,
            silent: false,
        };
        self::command(command, Ok(&discord), &mut channels, &done, &events);
    };
    let mut answer = async || {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Event::Forwarded { result, .. } = received.recv().await.unwrap() {
                    return result;
                }
            }
        })
        .await
        .expect("timed out waiting for Discord")
    };
    forward(
        format!("d:{}", original.id),
        &room,
        Some("look at this".into()),
    );
    assert_eq!(answer().await, Ok(()));
    // The forward first, then the message added; other tests post too.
    let mut shown = vec![];
    while shown.len() < 2 {
        match live.recv().await {
            Ok(Live::Created(message)) if message.forwarded => shown.push(message),
            Ok(Live::Created(message)) if message.content == "look at this" => shown.push(message),
            _ => {}
        }
    }
    assert!(shown[0].forwarded && shown[0].content == original.content);
    assert_eq!(shown[1].content, "look at this");
    // Nothing goes with a message added too long, nor into Matrix.
    forward(
        format!("d:{}", original.id),
        &room,
        Some("a".repeat(MAX_CHARS + 1)),
    );
    assert!(matches!(answer().await, Err(ForwardFailure::Forward(_))));
    forward(format!("d:{}", original.id), "!room:example.org", None);
    assert!(matches!(answer().await, Err(ForwardFailure::Forward(_))));
    // Nor a message Discord doesn't have.
    forward("d:1".into(), &room, None);
    assert!(matches!(answer().await, Err(ForwardFailure::Forward(_))));
}
