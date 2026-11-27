//! Reading, sending and changing messages, and what they hold.

use super::*;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn reads_history_and_member_lists() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        general,
        lounge,
        bob,
    } = Fixture::start("reads_history_and_member_lists").await;
    // READY, then the voice states from READY_SUPPLEMENTAL, which the
    // fixture waited for.
    assert_eq!(snapshot.servers.len(), 1);
    let server_shown = &snapshot.servers[0];
    assert_eq!(server_shown.name, "Server");
    let general_shown = server_shown.text_channels().next().unwrap();
    assert_eq!(general_shown.name, "general");
    assert!(
        general_shown.marks.unread,
        "the fixture leaves the last messages unread"
    );
    assert_eq!(server_shown.voice_channels[0].name, "Lounge");
    let in_voice: Vec<_> = snapshot.voice[&lounge]
        .iter()
        .map(|member| member.name.as_str())
        .collect();
    assert_eq!(in_voice, ["Bob", "Carol", "Dave"]);
    assert!(snapshot.voice[&lounge][0].streaming, "Bob is live");
    assert_eq!(snapshot.voice[&lounge][0].user_id, bob);
    let last_read = general_shown.marks.last_read;

    // Two pages of history: 50, then the rest, oldest first. The latest
    // starts with a boost, which counts but doesn't show.
    let latest = discord
        .history(&guild, &general, Span::Latest)
        .await
        .unwrap();
    assert!(latest.full);
    let (oldest, latest) = (latest.oldest.unwrap(), latest.messages);
    assert_eq!(latest.len(), 49);
    assert!(oldest < latest[0].order());
    assert_eq!(
        last_read.to_string(),
        latest[latest.len() - 6].id,
        "read up to the fixture's last five messages"
    );
    let older = discord
        .history(&guild, &general, Span::Before(&oldest.to_string()))
        .await
        .unwrap();
    assert!(!older.full);
    let older = older.messages;
    assert_eq!(older.len(), 10);
    assert!(older[0].content.starts_with("Welcome to **Yapfast**"));
    assert_eq!(
        (older[0].reactions[0].count, older[0].reactions[0].me),
        (2, true)
    );
    assert!(older[3].reply.is_some());
    // A jump asks for the messages around one, and scrolling down from
    // there for those after, as Discord's client does.
    let welcome = &older[0].id;
    let around = discord
        .history(&guild, &general, Span::Around(welcome))
        .await
        .unwrap();
    assert_eq!(
        &around.messages[0].id, welcome,
        "nothing is before the first"
    );
    assert!(around.messages.len() >= 20 && around.messages.len() <= 26);
    assert!(!around.full, "so it reaches the channel's start");
    assert!(!around.newest, "half a page from it on, so more may follow");
    let last = &latest.last().unwrap().id;
    let around_last = discord
        .history(&guild, &general, Span::Around(last))
        .await
        .unwrap();
    assert!(around_last.newest, "the newest message is the last");
    assert!(around_last.full, "and half a page is before it");
    let newest = around.messages.last().unwrap().id.clone();
    let after = discord
        .history(&guild, &general, Span::After(&newest))
        .await
        .unwrap();
    assert!(!after.full, "the rest of the channel");
    assert!(after.messages[0].order() > around.messages.last().unwrap().order());
    assert_eq!(
        after.messages.last().map(|message| &message.id),
        latest.last().map(|message| &message.id),
        "up to the newest, oldest first"
    );
    let image = &older[1].attachments[0];
    let preview = discord
        .download(image.preview_url.as_deref().unwrap(), 1 << 20)
        .await
        .unwrap();
    assert!(preview.starts_with(b"\x89PNG"));
    assert_eq!(&preview[16..24], &[0, 0, 2, 38, 0, 0, 1, 88], "550x344");

    // The channel's member list arrives once it's on screen, grouped as in
    // Discord: the hoisted Mods role, then online, then offline.
    let mut live = discord.live();
    discord.view(&guild, Some(&general));
    let members = live_where(&mut live, |event| match event {
        Live::Members { channel, members } if channel == general => Some(members),
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
            ("Bob", "Mods — 1", Presence::DoNotDisturb),
            ("Alice", "Online — 2", Presence::Online),
            ("Carol", "Online — 2", Presence::Idle),
            ("Dave", "Offline — 1", Presence::Offline)
        ]
    );
    assert_eq!(members[0].color, Some(0x3498db), "the Mods role's blue");
    assert_eq!(members[0].activity.as_deref(), Some("Playing Factorio"));
    assert_eq!(
        members[2].activity.as_deref(),
        Some("🌙 back later\nafter dinner, probably")
    );
    assert!(
        members[2].avatar.contains("/guilds/"),
        "Carol's server avatar"
    );

    // Lounge has the same readers as #general, so it shares the list
    // without asking Discord again, and so does going back.
    discord.view(&guild, Some(&lounge));
    let shared = live_where(&mut live, |event| match event {
        Live::Members { channel, members } if channel == lounge => Some(members),
        _ => None,
    })
    .await;
    assert_eq!(shared, members);
    discord.view(&guild, Some(&general));
    live_where(&mut live, |event| match event {
        Live::Members { channel, .. } if channel == general => Some(()),
        _ => None,
    })
    .await;
    assert_eq!(
        self::control(server, "GET", "/_control/list-requests", ""),
        "1"
    );

    // Hidden from Bots, Lounge has a list of its own. Asking for it stops
    // #general's, whose rows show again at once, without asking again.
    post_control(server, "/_control/hide-from-bots", &lounge);
    snapshot_where(&discord, |snapshot| {
        snapshot.servers.iter().any(|server| {
            server
                .voice_channels
                .iter()
                .any(|channel| channel.id == lounge && !channel.overwrites.is_empty())
        })
    })
    .await;
    discord.view(&guild, Some(&lounge));
    let own = live_where(&mut live, |event| match event {
        Live::Members { channel, members } if channel == lounge => Some(members),
        _ => None,
    })
    .await;
    assert_eq!(own, members);
    discord.view(&guild, Some(&general));
    let again = live_where(&mut live, |event| match event {
        Live::Members { channel, members } if channel == general => Some(members),
        _ => None,
    })
    .await;
    assert_eq!(again, members, "#general's list as it was");
    assert_eq!(
        self::control(server, "GET", "/_control/list-requests", ""),
        "2"
    );

    // Invisible, Alice's row shows her offline, as Discord's apps show
    // their own account, though Discord still lists her online.
    let alice = |members: Vec<ListedMember>| {
        members
            .into_iter()
            .find(|member| member.name == "Alice")
            .map(|member| member.presence)
    };
    for (status, presence) in [
        ("invisible", Presence::Offline),
        ("online", Presence::Online),
    ] {
        discord.set_status(status).await.unwrap();
        let shown = live_where(&mut live, |event| match event {
            Live::Members { channel, members } if channel == general => Some(members),
            _ => None,
        })
        .await;
        assert_eq!(alice(shown), Some(presence), "{status}");
    }
    post_control(server, "/_control/hide-from-bots", "");

    let account = snapshot.account.as_ref().expect("the account");
    assert_eq!(account.name, "Alice");
    assert!(
        account
            .avatar
            .contains("/avatars/1400000000000000001/alice1.png")
    );

    // Reading the channel clears its unread mark.
    discord.mark_read(&guild, &general, true);
    let newest = latest.last().unwrap().id.clone();
    snapshot_where(&discord, |snapshot| {
        let general = snapshot.servers[0].text_channels().next().unwrap().marks;
        !general.unread && general.last_read.to_string() == newest
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn a_reply_looks_up_the_original_discord_left_out() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("a_reply_looks_up_the_original_discord_left_out").await;
    let history = async || {
        discord
            .history(&guild, &general, Span::Latest)
            .await
            .unwrap()
            .messages
    };
    let first = history().await.swap_remove(0);
    let reply = control(
        server,
        "POST",
        &format!("/_control/unlooked-reply?channel={general}&to={}", first.id),
        "",
    );
    let replied = history()
        .await
        .into_iter()
        .find(|message| message.id == reply.trim())
        .unwrap();
    assert_eq!(
        replied.reply,
        Some(Reply::Unknown {
            id: first.id.clone()
        })
    );
    let original = discord.original(&guild, &general, &first.id).await.unwrap();
    assert!(
        matches!(&original, Some(Reply::Message { id, content, .. })
            if *id == first.id && *content == first.content),
        "{original:?}"
    );
    // Asking around one that's gone answers with its neighbor.
    let gone = (first.id.parse::<u64>().unwrap() - 1).to_string();
    assert_eq!(
        discord.original(&guild, &general, &gone).await.unwrap(),
        None
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn sends_edits_and_deletes_messages() {
    let Fixture {
        _fake,
        discord,
        server,
        snapshot,
        guild,
        general,
        ..
    } = Fixture::start("sends_edits_and_deletes_messages").await;
    let mut live = discord.live();
    // Bob chats live until stopped.
    let control = reqwest::Client::new();
    control
        .post(format!("{server}/_control/chat-start"))
        .send()
        .await
        .unwrap();
    let message = live_where(&mut live, |event| match event {
        Live::Created(message) => Some(message),
        _ => None,
    })
    .await;
    control
        .post(format!("{server}/_control/chat-stop"))
        .send()
        .await
        .unwrap();
    assert_eq!(message.author, "Bob");
    assert!(message.content.starts_with("Live Message 0"));
    assert_eq!(
        discord.color(&snapshot.servers[0], &message.author_id),
        Some(0x3498db),
        "Bob's name takes the Mods role's blue"
    );
    snapshot_where(&discord, |snapshot| {
        snapshot.servers[0]
            .text_channels()
            .next()
            .unwrap()
            .marks
            .unread
    })
    .await;

    // Sending, reacting, editing and deleting.
    let nonce = yapfast_discord::message::nonce();
    let sent = discord
        .send(
            &guild,
            &general,
            "hello from Yapfast",
            sending(&nonce, Some(&message.id)),
            vec![],
        )
        .await
        .unwrap();
    assert!(sent.reply.is_some());
    let echoed = live_where(&mut live, |event| match event {
        Live::Created(message) if message.id == sent.id => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(echoed.nonce.as_deref(), Some(nonce.as_str()));
    discord.react(&general, &sent.id, "🎉", true).await.unwrap();
    live_where(&mut live, |event| match event {
        Live::Reaction {
            me: true,
            added: true,
            ..
        } => Some(()),
        _ => None,
    })
    .await;
    let edited = discord.edit(&general, &sent.id, "edited").await.unwrap();
    assert!(edited.edited);
    discord.delete(&general, &sent.id).await.unwrap();
    live_where(&mut live, |event| match event {
        Live::Deleted { id, .. } if id == sent.id => Some(()),
        _ => None,
    })
    .await;
    // Deleting one Discord no longer has, as from a timeline that still
    // showed it, does what was asked.
    discord.delete(&general, &sent.id).await.unwrap();

    // Someone else's message goes only with Manage Messages, which Mods
    // gives.
    post_control(server, "/_control/mention", "");
    let bobs = live_where(&mut live, |event| match event {
        Live::Created(message) if message.content.contains("co-op tonight") => Some(message),
        _ => None,
    })
    .await;
    assert!(discord.delete(&general, &bobs.id).await.is_err());
    let alice = discord.snapshots().borrow().user_id.clone().unwrap();
    let mods = "1400000000000000110";
    for add in [true, false] {
        discord
            .member_role(&guild, &alice, mods, add)
            .await
            .unwrap();
        let snapshot = snapshot_where(&discord, |snapshot| {
            snapshot.servers[0]
                .marks
                .roles
                .iter()
                .any(|role| role == mods)
                == add
        })
        .await;
        let joined = &snapshot.servers[0];
        assert_eq!(
            joined.deletes(&joined.text_channels[0], &alice, &joined.marks.roles),
            if add { Deletes::Any } else { Deletes::Own }
        );
        if add {
            discord.delete(&general, &bobs.id).await.unwrap();
            live_where(&mut live, |event| match event {
                Live::Deleted { id, .. } if id == bobs.id => Some(()),
                _ => None,
            })
            .await;
        }
    }

    // Timed out, Alice only reads "Server" until the timeout ends by
    // itself.
    post_control(server, "/_control/timeout-alice", "2");
    let timed_out = snapshot_where(&discord, |snapshot| {
        snapshot.servers[0].my_timeout.is_some()
    })
    .await;
    let access = |snapshot: &yapfast_discord::service::Snapshot| {
        snapshot.servers[0]
            .text_channels()
            .next()
            .unwrap()
            .marks
            .access
    };
    assert_eq!(access(&timed_out), Access::Locked);
    let nonce = yapfast_discord::message::nonce();
    assert!(
        discord
            .send(&guild, &general, "let me in", sending(&nonce, None), vec![])
            .await
            .is_err()
    );
    let lifted = snapshot_where(&discord, |snapshot| {
        snapshot.servers[0].my_timeout.is_none()
    })
    .await;
    assert_eq!(access(&lifted), Access::Join);
    let nonce = yapfast_discord::message::nonce();
    discord
        .send(&guild, &general, "back", sending(&nonce, None), vec![])
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn votes_in_and_posts_polls() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("votes_in_and_posts_polls").await;
    let control = reqwest::Client::new();
    let mut live = discord.live();
    // Polls: a vote comes back live, who voted lists by answer, and Alice's
    // own poll ends early with a notice of its winner.
    let posted = control
        .post(format!("{server}/_control/poll?alice"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let poll = live_where(&mut live, |event| match event {
        Live::Created(message) if message.poll.is_some() => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(poll.id, posted.trim());
    assert!(!poll.editable && !poll.forwardable);
    discord.vote(&general, &poll.id, &[2]).await.unwrap();
    live_where(&mut live, |event| match event {
        Live::Vote {
            answer: 2,
            me: true,
            added: true,
            ..
        } => Some(()),
        _ => None,
    })
    .await;
    let voters = discord
        .voters(&guild, &general, &poll.id, 2, None, 100)
        .await
        .unwrap();
    assert_eq!(voters.len(), 3);
    assert_eq!(voters.iter().filter(|voter| voter.me).count(), 1);
    discord.end_poll(&general, &poll.id).await.unwrap();
    let closed = live_where(&mut live, |event| match event {
        Live::Updated(message) if message.id == poll.id => Some(message),
        _ => None,
    })
    .await;
    assert!(closed.poll.is_some_and(|poll| poll.finalized));
    let notice = live_where(&mut live, |event| match event {
        Live::Created(message) if message.system.is_some() => Some(message),
        _ => None,
    })
    .await;
    assert!(
        notice
            .system
            .as_deref()
            .is_some_and(|said| said.ends_with("Winning answer: 🍜 Ramen, 3 votes (75%).")),
        "{notice:?}"
    );

    // Posting a poll, as Discord's "Create a Poll" dialog does.
    let new_poll = yapfast_discord::api::NewPoll {
        question: "Movie tonight?".into(),
        answers: vec![
            yapfast_discord::api::NewAnswer {
                text: "Yes".into(),
                emoji: Some("🍿".into()),
            },
            yapfast_discord::api::NewAnswer {
                text: "No".into(),
                emoji: None,
            },
        ],
        hours: 4,
        multiselect: false,
    };
    discord
        .send_poll(&guild, &general, &new_poll)
        .await
        .unwrap();
    let posted = live_where(&mut live, |event| match event {
        Live::Created(message) => message
            .poll
            .filter(|poll| poll.question == "Movie tonight?"),
        _ => None,
    })
    .await;
    let answers: Vec<_> = posted
        .answers
        .iter()
        .map(|answer| (answer.id, answer.text.as_str(), answer.emoji.clone()))
        .collect();
    assert_eq!(
        answers,
        [(1, "Yes", Some(("🍿".to_owned(), None))), (2, "No", None)]
    );
    let expiry = posted.expiry.unwrap();
    let now = yapfast_discord::message::snowflake_ms(&yapfast_discord::message::nonce());
    assert!(
        expiry.abs_diff(now + 4 * 3_600_000) < 60_000,
        "it takes votes for four hours"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn forwards_messages() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("forwards_messages").await;
    let control = reqwest::Client::new();
    let mut live = discord.live();
    // A message of Bob's to forward.
    post_control(server, "/_control/chat-start", "");
    let message = live_where(&mut live, |event| match event {
        Live::Created(message) => Some(message),
        _ => None,
    })
    .await;
    post_control(server, "/_control/chat-stop", "");
    // Forwarding, as Discord's client does: a message of its own that
    // carries the original, then the message added as another.
    discord
        .forward((&guild, &general, &message.id), (&guild, &general), false)
        .await
        .unwrap();
    let forward = live_where(&mut live, |event| match event {
        Live::Created(forward) if forward.forwarded => Some(forward),
        _ => None,
    })
    .await;
    assert_eq!(forward.content, message.content);
    assert!(!forward.editable && forward.forwardable);
    discord
        .send_forward_note((&guild, &general), "look at this", false)
        .await
        .unwrap();
    live_where(&mut live, |event| match event {
        Live::Created(note) if note.content == "look at this" => Some(()),
        _ => None,
    })
    .await;
    let forwards: serde_json::Value = serde_json::from_str(
        &control
            .get(format!("{server}/_control/forwards"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(forwards[0]["target"], general.as_str());
    // {"location":"forwarding"}
    assert_eq!(
        forwards[0]["context"],
        "eyJsb2NhdGlvbiI6ImZvcndhcmRpbmcifQ=="
    );
    // A message Discord doesn't have isn't forwarded.
    assert!(
        discord
            .forward((&guild, &general, "1"), (&guild, &general), false)
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn types_and_mentions() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("types_and_mentions").await;
    let control = reqwest::Client::new();
    let mut live = discord.live();
    // Bob starts typing, and Yapfast says Alice is typing.
    control
        .post(format!("{server}/_control/typing"))
        .send()
        .await
        .unwrap();
    let typing = live_where(&mut live, |event| match event {
        Live::Typing {
            guild,
            channel,
            name,
            ..
        } => Some((guild, channel, name)),
        _ => None,
    })
    .await;
    assert_eq!(
        typing,
        (Some(guild.clone()), general.clone(), "Bob".to_owned())
    );
    discord.typing(&general).await.unwrap();
    let typed = control
        .get(format!("{server}/_control/typed"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let typed: Vec<String> = serde_json::from_str(&typed).unwrap();
    assert_eq!(typed, vec![general.clone()]);

    // Mentions: a search finds members by name, and only Mods may be mentioned,
    // since Alice can't mention everyone.
    let found = discord.search_members(&guild, "car").await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(discord.name(&guild, &found[0]).as_deref(), Some("Carol"));
    let listed = discord.snapshots().borrow().servers[0].clone();
    assert!(
        !listed
            .text_channels()
            .next()
            .unwrap()
            .marks
            .mention_everyone
    );
    let mentionable: Vec<_> = listed
        .mentionable(false)
        .into_iter()
        .map(|(_, name, _)| name)
        .collect();
    assert_eq!(mentionable, ["Mods"]);
    assert_eq!(
        listed.mentionable(true).len(),
        5,
        "Bots, Crew, Staff and Leads too, for those who may"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn shows_embeds_and_media() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("shows_embeds_and_media").await;
    let control = reqwest::Client::new();
    let mut live = discord.live();
    // Link previews: the article's arrives in an update, as Discord crawls
    // the page after the message is sent. A GIF link shows as a picture,
    // without its address.
    control
        .post(format!("{server}/_control/embeds"))
        .send()
        .await
        .unwrap();
    let article = live_where(&mut live, |event| match event {
        Live::Updated(message) if !message.embeds.is_empty() => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(article.embeds[0].provider.as_deref(), Some("Example News"));
    assert_eq!(article.attachments[0].shown, Shown::Embed(0));
    let latest = discord
        .history(&guild, &general, Span::Latest)
        .await
        .unwrap()
        .messages;
    let [.., video, gif, card] = latest.as_slice() else {
        panic!("the embeds were posted");
    };
    assert!(video.embeds[0].video);
    assert_eq!(video.attachments[0].shown, Shown::Poster(0));
    assert!(gif.content.is_empty() && gif.embeds.is_empty());
    assert_eq!(gif.attachments[0].shown, Shown::Message);
    assert_eq!(card.author, "Status Bot");
    assert_eq!(card.embeds[0].fields.len(), 4);
    // Pictures come from Discord's proxy; the linked site is never asked.
    // The GIF is an MP4, with its first frame as a picture.
    let preview = gif.attachments[0].preview_url.as_deref().unwrap();
    assert_eq!(
        &discord.download(preview, 1 << 20).await.unwrap()[4..8],
        b"ftyp"
    );
    let still = gif.attachments[0].still_url.as_deref().unwrap();
    assert!(!discord.download(still, 1 << 20).await.unwrap().is_empty());
    assert!(
        discord
            .download("https://example.com/bike.png", 1 << 20)
            .await
            .is_err()
    );

    // An attached video plays from its file, which downloads as it plays,
    // under a poster the media proxy makes.
    control
        .post(format!("{server}/_control/clip"))
        .send()
        .await
        .unwrap();
    let posted = live_where(&mut live, |event| match event {
        Live::Created(message) if message.content == "Watch this" => Some(message),
        _ => None,
    })
    .await;
    let clip = &posted.attachments[0];
    assert!(!clip.image && clip.dimensions == Some((480, 270)));
    let poster = clip.preview_url.as_deref().unwrap();
    assert!(!discord.download(poster, 1 << 20).await.unwrap().is_empty());
    // It downloads in parts, as a browser asks for a video's: the whole
    // file from its start, or its last bytes alone, where an MP4's index
    // may be.
    let video_url = clip.video_url.as_deref().unwrap();
    let (start, size, downloaded) = download_part(&discord, video_url, 0, None).await;
    assert_eq!((start, size), (0, clip.size));
    assert_eq!(Some(downloaded.len() as u64), clip.size);
    assert_eq!(&downloaded[4..8], b"ftyp");
    let end = downloaded.len() as u64;
    let (start, size, tail) = download_part(&discord, video_url, end - 100, None).await;
    assert_eq!((start, size), (end - 100, clip.size));
    assert_eq!(tail, downloaded[downloaded.len() - 100..]);
    let (start, _, middle) = download_part(&discord, video_url, 10, Some(20)).await;
    assert_eq!((start, middle.as_slice()), (10, &downloaded[10..20]));
    assert!(
        discord
            .download_part(video_url, 0, None, end - 1)
            .await
            .is_err(),
        "too large"
    );
    // A forward of it shows the clip it carries.
    let forward = live_where(&mut live, |event| match event {
        Live::Created(message) if message.forwarded => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(forward.content, "Watch this");
    assert_eq!(forward.attachments, posted.attachments);
    // So does a linked post's, its card's thumbnail the poster.
    let linked = live_where(&mut live, |event| match event {
        Live::Created(message) if message.content.starts_with("https://vxtwitter.com/") => {
            Some(message)
        }
        _ => None,
    })
    .await;
    assert!(linked.embeds[0].video);
    let poster = &linked.attachments[0];
    assert_eq!(poster.shown, Shown::Poster(0));
    let (_, _, linked) =
        download_part(&discord, poster.video_url.as_deref().unwrap(), 0, None).await;
    assert_eq!(&linked[4..8], b"ftyp");
}

/// A part of a file, as [`Discord::download_part`] gives it: where it
/// starts, the file's size, and its bytes.
async fn download_part(
    discord: &Discord,
    url: &str,
    from: u64,
    to: Option<u64>,
) -> (u64, Option<u64>, Vec<u8>) {
    use futures_util::StreamExt;
    let mut part = discord.download_part(url, from, to, 1 << 20).await.unwrap();
    let mut data = Vec::new();
    while let Some(chunk) = part.chunks.next().await {
        data.extend_from_slice(&chunk.unwrap());
    }
    (part.start, part.size, data)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn highlights_mentions() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        ..
    } = Fixture::start("highlights_mentions").await;
    // Bob calls on Alice every way Discord highlights for her, through her
    // Crew role among them, and two ways it doesn't.
    post_control(server, "/_control/mentions", "");
    let snapshot = discord.snapshots().borrow().clone();
    let me = snapshot.user_id.clone().unwrap();
    assert!(
        snapshot.servers[0]
            .marks
            .roles
            .contains(&"1400000000000000112".to_owned())
    );
    let latest = discord
        .history(&guild, &general, Span::Latest)
        .await
        .unwrap()
        .messages;
    let highlighted: Vec<bool> = latest[latest.len() - 7..]
        .iter()
        .map(|message| message.mentions_me(&me, Some(&snapshot.servers[0].marks)))
        .collect();
    assert_eq!(
        highlighted,
        [true, true, true, true, true, false, false],
        "by name, Crew, @everyone, @here and a reply; not Mods, nor nothing"
    );
}

/// Who reacted to a message, a page at a time, as a reaction's tooltip and
/// the Reactions window ask; and a moderator removing someone's reaction.
/// Part of [`talks_to_the_fake_server`], whose session it shares.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn lists_who_reacted_as_discords_client_does() {
    let fixture = Fixture::start("lists_who_reacted_as_discords_client_does").await;
    let discord = &fixture.discord;
    let server = fixture.server;
    let guild = fixture.guild.as_str();
    let general = fixture.general.as_str();
    let latest = discord.history(guild, general, Span::Latest).await.unwrap();
    let newest = latest
        .messages
        .iter()
        .find(|message| {
            message
                .reactions
                .iter()
                .any(|reaction| reaction.key == "🎉")
        })
        .expect("the fixture's newest message, with reactions");
    let id = newest.id.clone();
    let names = |people: &[yapfast_discord::service::Reactor]| -> Vec<(String, String, bool)> {
        people
            .iter()
            .map(|person| (person.name.clone(), person.username.clone(), person.me))
            .collect()
    };
    let first = discord
        .reactors(guild, general, &id, "🎉", None, 2)
        .await
        .unwrap();
    assert_eq!(
        names(&first),
        [
            ("Alice".into(), "alice".into(), true),
            ("Bob".into(), "bob".into(), false)
        ]
    );
    let rest = discord
        .reactors(guild, general, &id, "🎉", Some(&first[1].id), 100)
        .await
        .unwrap();
    assert_eq!(
        rest.iter()
            .map(|person| person.name.as_str())
            .collect::<Vec<_>>(),
        ["Carol", "Dave"]
    );
    assert!(!rest[0].avatar.is_empty());
    let pages = self::control(server, "GET", "/_control/reactor-pages", "");
    assert!(pages.contains("limit=2&type=0"), "{pages}");
    assert!(pages.contains(&format!("after={}", first[1].id)), "{pages}");

    // A moderator removes Dave's.
    discord
        .remove_reaction(general, &id, "🎉", &rest[1].id)
        .await
        .unwrap();
    let removed = self::control(server, "GET", "/_control/reactor-removals", "");
    assert!(removed.contains(&format!("🎉 {}", rest[1].id)), "{removed}");
}
