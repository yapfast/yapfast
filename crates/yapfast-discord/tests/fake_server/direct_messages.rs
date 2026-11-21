//! Direct messages, groups and friends.

use super::*;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn lists_dms_and_notifies() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        general,
        ..
    } = Fixture::start("lists_dms_and_notifies").await;
    let control = reqwest::Client::new();
    let mut live = discord.live();
    // DMs and message requests, the latest first; each unread message in a
    // DM counts as a mention, and requests count none.
    let listed: Vec<_> = snapshot
        .conversations
        .iter()
        .map(|dm| {
            (
                dm.name.as_str(),
                dm.request,
                dm.spam,
                dm.unread,
                dm.mentions,
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("Erin", true, false, true, 0),
            ("Free Nitro", true, true, false, 0),
            ("Bob", false, false, true, 2),
            ("Weekend plans", false, false, false, 0)
        ]
    );
    let id = |name: &str| {
        snapshot
            .conversations
            .iter()
            .find(|dm| dm.name == name)
            .unwrap()
            .id
            .clone()
    };
    let (dm_bob, weekend) = (id("Bob"), id("Weekend plans"));
    assert!(
        snapshot.conversations[2]
            .avatar
            .as_deref()
            .is_some_and(|avatar| avatar.contains("/avatars/1400000000000000002/bob1.png"))
    );
    let history = discord
        .history(PRIVATE, &dm_bob, Span::Latest)
        .await
        .unwrap()
        .messages;
    let said: Vec<_> = history
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(
        said,
        [
            "Did you see the trip notes?",
            "Yes, bringing snacks",
            "Great",
            "See you at nine"
        ]
    );
    discord.mark_read(PRIVATE, &dm_bob, true);
    snapshot_where(&discord, |snapshot| {
        let bob = snapshot.conversations.iter().find(|dm| dm.id == dm_bob);
        bob.is_some_and(|bob| !bob.unread && bob.mentions == 0)
    })
    .await;
    let nonce = yapfast_discord::message::nonce();
    let sent = discord
        .send(
            PRIVATE,
            &dm_bob,
            "On my way",
            sending(&nonce, Some(&history[3].id)),
            // A message carries several files.
            vec![
                ("map.txt".into(), b"left at the lake".to_vec()),
                ("list.txt".into(), b"snacks".to_vec()),
            ],
        )
        .await
        .unwrap();
    assert!(sent.reply.is_some());
    let names: Vec<_> = sent
        .attachments
        .iter()
        .map(|file| file.name.as_str())
        .collect();
    assert_eq!(names, ["map.txt", "list.txt"]);
    live_where(&mut live, |event| match event {
        Live::Created(message) if message.id == sent.id => Some(()),
        _ => None,
    })
    .await;

    // A group DM lists everyone in it, with friends' presence.
    discord.view(PRIVATE, Some(&weekend));
    let members = live_where(&mut live, |event| match event {
        Live::Members { channel, members } if channel == weekend => Some(members),
        _ => None,
    })
    .await;
    let listed: Vec<_> = members
        .iter()
        .map(|member| (member.name.as_str(), member.group.as_str(), member.presence))
        .collect();
    assert_eq!(
        listed,
        [
            ("Alice", "Members — 3", Presence::Online),
            ("Bob", "Members — 3", Presence::DoNotDisturb),
            ("Carol", "Members — 3", Presence::Idle)
        ]
    );

    // Dave writes for the first time: his new DM comes first, unread.
    control
        .post(format!("{server}/_control/dm"))
        .send()
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot.conversations.len() == 5
            && snapshot.conversations[0].name == "Dave"
            && snapshot.conversations[0].mentions == 1
    })
    .await;
    // His message notifies, as DMs do.
    let notified = live_where(&mut live, |event| match event {
        Live::Notify { guild, message } => Some((guild, message.author)),
        _ => None,
    })
    .await;
    assert_eq!(notified, (None, "Dave".to_owned()));
    // The server notifies for mentions only: Bob's photos don't, his mention does.
    for control_path in ["gallery", "mention"] {
        control
            .post(format!("{server}/_control/{control_path}"))
            .send()
            .await
            .unwrap();
    }
    let notified = live_where(&mut live, |event| match event {
        Live::Notify { guild, message } => Some((guild, message.content)),
        _ => None,
    })
    .await;
    assert_eq!(notified.0.as_deref(), Some(guild.as_str()));
    assert!(
        notified.1.ends_with("are you in for co-op tonight?"),
        "{}",
        notified.1
    );

    // Reading #general and Dave's DM together, as marking a server read
    // does, acknowledges both in one request.
    let dm_dave = discord.snapshots().borrow().conversations[0].id.clone();
    let before = serde_json::from_str::<Vec<Vec<String>>>(&self::control(
        server,
        "GET",
        "/_control/acks",
        "",
    ))
    .unwrap()
    .len();
    discord.mark_read(&guild, &general, true);
    discord.mark_read(PRIVATE, &dm_dave, true);
    // Reads from just before, such as Bob's DM, may go in the same request.
    let both = |request: &Vec<String>| request.contains(&general) && request.contains(&dm_dave);
    timeout(WAIT, async {
        loop {
            let acks: Vec<Vec<String>> =
                serde_json::from_str(&self::control(server, "GET", "/_control/acks", "")).unwrap();
            if acks[before..].iter().any(both) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("both reads are acknowledged in one request");

    // Mark Unread on Dave's message reads up to just before it, as a manual
    // acknowledgement with the one mention after it, and his DM is unread
    // again, as the MESSAGE_ACK that comes back says too.
    let read = snapshot_where(&discord, |snapshot| {
        snapshot
            .conversations
            .iter()
            .any(|dm| dm.id == dm_dave && !dm.unread && dm.last_read > 0)
    })
    .await;
    let newest = read
        .conversations
        .iter()
        .find(|dm| dm.id == dm_dave)
        .unwrap()
        .last_read;
    let before = newest - (1 << 22);
    discord.mark_unread(&dm_dave, before, 1, true);
    let manual = timeout(WAIT, async {
        loop {
            let acks: Vec<serde_json::Value> =
                serde_json::from_str(&self::control(server, "GET", "/_control/manual-acks", ""))
                    .unwrap();
            if let Some(ack) = acks.into_iter().next() {
                break ack;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Mark Unread is acknowledged");
    assert_eq!(
        manual,
        serde_json::json!({"channel_id": dm_dave, "message_id": before.to_string(), "mention_count": 1})
    );
    snapshot_where(&discord, |snapshot| {
        snapshot
            .conversations
            .iter()
            .any(|dm| dm.id == dm_dave && dm.unread && dm.mentions == 1 && dm.last_read == before)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn opens_dms_and_searches_pins() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        general,
        bob,
        ..
    } = Fixture::start("opens_dms_and_searches_pins").await;
    let mut live = discord.live();
    let dm = |name: &str| {
        snapshot
            .conversations
            .iter()
            .find(|dm| dm.name == name)
            .unwrap()
            .id
            .clone()
    };
    let dm_bob = dm("Bob");
    // New DMs: friends come first, others by name, and opening a DM with
    // Carol starts one, which the DM list gains; opening it again finds it.
    let friends: Vec<_> = discord
        .find_people("")
        .into_iter()
        .map(|person| (person.name, person.friend))
        .collect();
    assert_eq!(
        friends,
        [("Bob".to_owned(), true), ("Carol".to_owned(), true)]
    );
    let found = discord.find_people("da");
    assert_eq!(found[0].name, "Dave");
    assert!(!found[0].friend);
    let carol_id = discord.find_people("carol")[0].id.clone();
    let carol_dm = discord.open_dm(&carol_id).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot
            .conversations
            .iter()
            .any(|conversation| conversation.id == carol_dm)
    })
    .await;
    assert_eq!(discord.open_dm(&carol_id).await.unwrap(), carol_dm);

    // A DM's pins, and a search of it, which waits while Discord indexes
    // the DM, newest first, and asks again as Discord's client does.
    let pinned: Vec<_> = discord.pins(PRIVATE, &dm_bob).await.unwrap();
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].content, "Did you see the trip notes?");
    let waits = std::sync::atomic::AtomicUsize::new(0);
    let waited = || {
        waits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    };
    let search = |guild: String, channel: Option<String>, text: &'static str, sort| {
        let discord = discord.clone();
        async move {
            let channel = channel.as_deref();
            match discord.search(&guild, channel, text, sort, 0, &|| {}).await {
                Ok(Searched::Found(found)) => found,
                other => panic!("{other:?}"),
            }
        }
    };
    let found = match discord
        .search(PRIVATE, Some(&dm_bob), "SEE", Sort::Newest, 0, &waited)
        .await
        .unwrap()
    {
        Searched::Found(found) => found,
        Searched::Indexing => panic!("still indexing"),
    };
    let said: Vec<_> = found
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(said, ["See you at nine", "Did you see the trip notes?"]);
    assert_eq!(found.total, 2);
    assert_eq!(waits.into_inner(), 1);
    let oldest = search(
        PRIVATE.to_owned(),
        Some(dm_bob.clone()),
        "see",
        Sort::Oldest,
    )
    .await;
    assert_eq!(oldest.messages[0].content, "Did you see the trip notes?");
    // From Bob by username, with a key and its answer apart; someone
    // nobody knows is words.
    let from_bob = search(
        PRIVATE.to_owned(),
        Some(dm_bob.clone()),
        "from: bob",
        Sort::Newest,
    )
    .await;
    assert!(
        from_bob
            .messages
            .iter()
            .all(|message| message.author_id == bob)
    );
    assert_eq!(from_bob.total, 3);
    let asked: Vec<String> =
        serde_json::from_str(&self::control(server, "GET", "/_control/searches", "")).unwrap();
    assert_eq!(
        asked,
        [
            "content=SEE&sort_by=timestamp&sort_order=desc&offset=0",
            "content=SEE&sort_by=timestamp&sort_order=desc&offset=0&attempts=1",
            "content=see&sort_by=timestamp&sort_order=asc&offset=0",
            format!("author_id={bob}&sort_by=timestamp&sort_order=desc&offset=0").as_str(),
        ]
    );

    // A server channel's pins, and a search of its server, here narrowed to
    // the channel with in:, which Discord indexes the same way.
    let pinned = discord.pins(&guild, &general).await.unwrap();
    assert_eq!(pinned.len(), 1);
    assert!(pinned[0].content.starts_with("Welcome to **Yapfast**"));
    // Bob's picture after it, pinned as Alice's Crew role allows: Discord
    // says it's pinned, and it leads the pins until it's unpinned.
    let after = discord
        .history(&guild, &general, Span::After(&pinned[0].id))
        .await
        .unwrap()
        .messages;
    let sunset = after
        .iter()
        .find(|message| {
            message
                .attachments
                .iter()
                .any(|attachment| attachment.image)
        })
        .expect("Bob's picture follows the welcome");
    assert!(pinned[0].pinned && !sunset.pinned);
    discord.pin(&general, &sunset.id, true).await.unwrap();
    let updated = live_where(&mut live, |event| match event {
        Live::Updated(message) if message.id == sunset.id => Some(message),
        _ => None,
    })
    .await;
    assert!(updated.pinned);
    let pins = discord.pins(&guild, &general).await.unwrap();
    assert_eq!(pins.len(), 2);
    assert_eq!(pins[0].id, sunset.id);
    assert!(pins[0].attachments[0].preview_url.is_some());
    discord.pin(&general, &sunset.id, false).await.unwrap();
    assert_eq!(discord.pins(&guild, &general).await.unwrap().len(), 1);
    let found = search(guild.clone(), None, "welcome in:general", Sort::Newest).await;
    let said: Vec<_> = found
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(said.len(), 2);
    assert_eq!(said[0], "The welcome has the plan");
    assert!(said[1].starts_with("Welcome to **Yapfast**"));
    assert_eq!(discord.name(&guild, &bob).as_deref(), Some("Bob"));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn makes_group_dms_and_answers_requests() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        ..
    } = Fixture::start("makes_group_dms_and_answers_requests").await;
    let dm = |name: &str| {
        snapshot
            .conversations
            .iter()
            .find(|dm| dm.name == name)
            .unwrap()
            .id
            .clone()
    };
    let (erin, frank) = (dm("Erin"), dm("Free Nitro"));
    let carol_id = discord.find_people("carol")[0].id.clone();
    // Friends added to a DM make a group of them; more join it later.
    let dave = discord.find_people("da")[0].id.clone();
    let group = discord
        .create_group(&[carol_id.clone(), dave.clone()])
        .await
        .unwrap();
    let people_in = |snapshot: &Snapshot| {
        snapshot
            .conversations
            .iter()
            .find(|conversation| conversation.id == group && conversation.group)
            .map(|conversation| conversation.people)
    };
    snapshot_where(&discord, |snapshot| people_in(snapshot) == Some(3)).await;
    let bob_id = discord.find_people("bob")[0].id.clone();
    discord.add_to_group(&group, &[bob_id]).await.unwrap();
    snapshot_where(&discord, |snapshot| people_in(snapshot) == Some(4)).await;
    // Leaving it without telling the others, as its dialog's box does.
    discord.leave_group(&group, true).await.unwrap();
    snapshot_where(&discord, |snapshot| people_in(snapshot).is_none()).await;
    assert_eq!(
        self::control(server, "GET", "/_control/group-leaves", ""),
        format!(r#"[{{"channel":"{group}","silent":"true"}}]"#)
    );

    // Each request shows its latest message, as Discord's requests list
    // loads it, and stops once it's answered.
    snapshot_where(&discord, |snapshot| {
        snapshot.conversations.iter().any(|dm| {
            dm.id == erin
                && dm.preview.as_ref().is_some_and(|preview| {
                    preview.text.starts_with("Hi Alice, Bob gave me your name")
                })
        })
    })
    .await;
    // A request can be read before answering it. Accepting Erin's makes it
    // a DM; ignoring the spam removes it.
    let asked = discord
        .history(PRIVATE, &erin, Span::Latest)
        .await
        .unwrap()
        .messages;
    assert!(
        asked[0]
            .content
            .starts_with("Hi Alice, Bob gave me your name")
    );
    discord.answer_request(&erin, true).await.unwrap();
    discord.answer_request(&frank, false).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot
            .conversations
            .iter()
            .any(|dm| dm.id == erin && !dm.request && dm.mentions == 1 && dm.preview.is_none())
            && !snapshot.conversations.iter().any(|dm| dm.id == frank)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn rings_and_befriends() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("rings_and_befriends").await;
    // Dave isn't a friend, and Discord doesn't let Alice ring him.
    post_control(server, "/_control/dm", "");
    let dave = snapshot_where(&discord, |snapshot| {
        snapshot
            .conversations
            .iter()
            .any(|found| found.name == "Dave")
    })
    .await
    .conversations
    .iter()
    .find(|found| found.name == "Dave")
    .cloned()
    .unwrap();
    let (dave, dave_user) = (dave.id.clone(), dave.person.clone().unwrap());
    let assigned = discord.join(PRIVATE, &dave).await.unwrap();
    let mut call = timeout(
        WAIT,
        Call::connect(&assigned, PRIVATE, &dave, std::sync::Arc::new(Quiet)),
    )
    .await
    .expect("timed out connecting to the DM call")
    .unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot
            .conversations
            .iter()
            .any(|found| found.id == dave && found.call)
    })
    .await;
    assert!(!discord.ring(&dave).await.unwrap(), "Dave can't be rung");

    // Alice sends Dave a friend request, which he accepts; then he can be rung.
    discord.befriend(&dave_user).await.unwrap();
    let requested = |snapshot: &Snapshot| -> Vec<(String, bool)> {
        snapshot
            .friend_requests
            .iter()
            .map(|request| (request.name.clone(), request.incoming))
            .collect()
    };
    snapshot_where(&discord, |snapshot| {
        requested(snapshot) == [("Erin".to_owned(), true), ("Dave".to_owned(), false)]
    })
    .await;
    snapshot_where(&discord, |snapshot| snapshot.friends.contains(&dave_user)).await;
    assert!(discord.ring(&dave).await.unwrap(), "Dave is a friend now");
    call.leave();
    discord.leave(PRIVATE);
    snapshot_where(&discord, |snapshot| !snapshot.voice.contains_key(&dave)).await;

    // Erin's friend request is ignored.
    let erin = discord.snapshots().borrow().friend_requests[0]
        .user_id
        .clone();
    discord.remove_friend_request(&erin).await.unwrap();
    snapshot_where(&discord, |snapshot| snapshot.friend_requests.is_empty()).await;

    // Friend requests by username: to someone unknown, to Frank, who takes
    // none, and to Erin.
    let refused = |name: &'static str| {
        let discord = &discord;
        async move {
            discord
                .add_friend_by_username(name)
                .await
                .expect_err("refused")
                .to_string()
        }
    };
    assert_eq!(
        refused("nobody").await,
        "Discord has nobody by that username."
    );
    assert_eq!(
        refused("frank_deals").await,
        "They don't take friend requests from people they don't know."
    );
    assert_eq!(refused("bob").await, "You're already friends.");
    discord.add_friend_by_username("erin").await.unwrap();
    snapshot_where(&discord, |snapshot| {
        requested(snapshot) == [("Erin".to_owned(), false)]
    })
    .await;
}

/// Erin shares no friend and no small server with Alice, so Discord counts her
/// request a stranger's and accepts it only once the client confirms it. The
/// fake refuses one without the confirmation, as Discord answers 80013.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn accepts_a_strangers_friend_request() {
    let Fixture { _fake, discord, .. } = Fixture::start("accepts_a_strangers_friend_request").await;
    let erin = snapshot_where(&discord, |snapshot| !snapshot.friend_requests.is_empty())
        .await
        .friend_requests[0]
        .clone();
    assert!(erin.name == "Erin" && erin.incoming, "{erin:?}");
    discord.befriend(&erin.user_id).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot.friends.contains(&erin.user_id)
    })
    .await;
    snapshot_where(&discord, |snapshot| snapshot.friend_requests.is_empty()).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn acts_from_a_dms_menu() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("acts_from_a_dms_menu").await;
    // Dave writes, and Alice and he become friends.
    post_control(server, "/_control/dm", "");
    let dave = snapshot_where(&discord, |snapshot| {
        snapshot
            .conversations
            .iter()
            .any(|found| found.name == "Dave")
    })
    .await
    .conversations
    .iter()
    .find(|found| found.name == "Dave")
    .cloned()
    .unwrap();
    let (dave, dave_user) = (dave.id.clone(), dave.person.clone().unwrap());
    discord.befriend(&dave_user).await.unwrap();
    snapshot_where(&discord, |snapshot| snapshot.friends.contains(&dave_user)).await;
    // From Dave's DM's menu: a nickname for him and a note on him, ignoring
    // and muting, then unfriending, blocking and unblocking, and closing it.
    let dave_dm = |snapshot: &Snapshot| {
        snapshot
            .conversations
            .iter()
            .find(|found| found.id == dave)
            .cloned()
    };
    discord
        .set_friend_nickname(&dave_user, "Davey")
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        dave_dm(snapshot).is_some_and(|dm| dm.name == "Davey")
            && snapshot
                .friend_list
                .iter()
                .any(|friend| friend.name == "Davey" && friend.nickname.as_deref() == Some("Davey"))
    })
    .await;
    assert_eq!(discord.note(&dave_user).await.unwrap(), "", "no note yet");
    discord
        .set_note(&dave_user, "Met at the jam")
        .await
        .unwrap();
    assert_eq!(discord.note(&dave_user).await.unwrap(), "Met at the jam");
    // Discord is asked once; another client's change comes by the gateway.
    let note_reads = || control(server, "GET", "/_control/note-reads", "");
    assert_eq!(note_reads(), "1");
    post_control(
        server,
        &format!("/_control/note-elsewhere?user={dave_user}&note=Owes+me"),
        "",
    );
    timeout(WAIT, async {
        while discord.note(&dave_user).await.unwrap() != "Owes me" {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(note_reads(), "1");
    discord.ignore(&dave_user, true).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        *snapshot.ignored == [dave_user.clone()]
    })
    .await;
    discord.ignore(&dave_user, false).await.unwrap();
    snapshot_where(&discord, |snapshot| snapshot.ignored.is_empty()).await;
    discord
        .mute_private(&dave, true, Some(15 * 60))
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        dave_dm(snapshot).is_some_and(|dm| dm.muted)
    })
    .await;
    discord.mute_private(&dave, false, None).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        dave_dm(snapshot).is_some_and(|dm| !dm.muted)
    })
    .await;
    discord.mute_private(&dave, true, None).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        dave_dm(snapshot).is_some_and(|dm| dm.muted)
    })
    .await;

    discord.unfriend(&dave_user, "ContextMenu").await.unwrap();
    snapshot_where(&discord, |snapshot| {
        !snapshot.friends.contains(&dave_user)
            && dave_dm(snapshot).is_some_and(|dm| dm.name == "Dave")
    })
    .await;
    discord
        .block(&dave_user, true, "ContextMenu")
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot.blocked.contains(&dave_user)
            && snapshot
                .blocked_list
                .iter()
                .any(|blocked| blocked.user_id == dave_user && blocked.name == "Dave")
    })
    .await;
    discord
        .block(&dave_user, false, "ContextMenu")
        .await
        .unwrap();
    snapshot_where(&discord, |snapshot| {
        !snapshot.blocked.contains(&dave_user) && snapshot.blocked_list.is_empty()
    })
    .await;
    // Each says where in Discord's client it came from: the request from
    // a profile, the rest from the DM's menu.
    assert_eq!(
        self::control(server, "GET", "/_control/relationship-changes", ""),
        format!(
            r#"[{{"method":"PUT","user":"{dave_user}","type":null,"location":"User Profile"}},{{"method":"DELETE","user":"{dave_user}","type":null,"location":"ContextMenu"}},{{"method":"PUT","user":"{dave_user}","type":2,"location":"ContextMenu"}},{{"method":"DELETE","user":"{dave_user}","type":null,"location":"ContextMenu"}}]"#
        )
    );
    discord.close_dm(&dave).await.unwrap();
    snapshot_where(&discord, |snapshot| dave_dm(snapshot).is_none()).await;
}
