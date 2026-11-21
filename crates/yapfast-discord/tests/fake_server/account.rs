//! The account: its session, status, games and activity.

use super::*;

/// A real SDK-shaped IPC exchange all the way to the fake Gateway, while
/// Alice has a custom status. The normal service runner opts in with a private
/// directory so this test never takes the desktop's Discord IPC socket.
#[cfg(target_os = "linux")]
async fn plays_through_rpc(discord: &Discord, server: &str, directory: &std::path::Path) {
    use serde_json::{Value, json};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::UnixStream,
    };

    struct GameProcess {
        child: std::process::Child,
        _directory: tempfile::TempDir,
    }
    impl Drop for GameProcess {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
    let mut detected = if let Some(cache) = std::env::var_os("YAPFAST_DISCORD_GAMES_CACHE") {
        let executable = std::env::current_exe().unwrap();
        let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
        let game = directory.path().join("yapfast_detected_game");
        std::fs::hard_link(executable, &game).unwrap();
        let child = std::process::Command::new(game)
            .args(["--exact", "waits_as_detected_game"])
            .env("YAPFAST_TEST_DETECTED_GAME", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let process = GameProcess {
            child,
            _directory: directory,
        };
        presence_where(server, |p| {
            p["activities"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["name"] == "Detected Fixture"))
        })
        .await;
        let cache = std::path::PathBuf::from(cache);
        let cached: Value = serde_json::from_slice(&std::fs::read(&cache).unwrap()).unwrap();
        assert!(
            cached[0].get("description").is_none(),
            "discard unused remote metadata"
        );
        assert_eq!(
            std::fs::read_to_string(cache.with_extension("etag")).unwrap(),
            r#"W/"fixture-games""#,
            "the CDN's version, to ask with tomorrow"
        );
        Some(process)
    } else {
        None
    };

    async fn send(stream: &mut UnixStream, opcode: u32, value: Value) {
        let payload = serde_json::to_vec(&value).unwrap();
        stream.write_u32_le(opcode).await.unwrap();
        stream.write_u32_le(payload.len() as u32).await.unwrap();
        stream.write_all(&payload).await.unwrap();
    }
    async fn receive(stream: &mut UnixStream) -> Value {
        timeout(Duration::from_secs(20), async {
            assert_eq!(stream.read_u32_le().await.unwrap(), 1);
            let size = stream.read_u32_le().await.unwrap();
            assert!(size <= 65536);
            let mut payload = vec![0; size as usize];
            stream.read_exact(&mut payload).await.unwrap();
            serde_json::from_slice(&payload).unwrap()
        })
        .await
        .unwrap()
    }
    async fn connect(directory: &std::path::Path) -> UnixStream {
        let mut stream = UnixStream::connect(directory.join("discord-ipc-0"))
            .await
            .unwrap();
        send(
            &mut stream,
            0,
            json!({"v":1,"client_id":"123456789012345678"}),
        )
        .await;
        assert_eq!(receive(&mut stream).await["evt"], "READY");
        stream
    }
    let mut first = connect(directory).await;
    // PING is an opaque IPC payload and must be echoed as PONG.
    send(&mut first, 3, json!({"ping":1})).await;
    assert_eq!(first.read_u32_le().await.unwrap(), 4);
    let size = first.read_u32_le().await.unwrap();
    let mut pong = vec![0; size as usize];
    first.read_exact(&mut pong).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&pong).unwrap(),
        json!({"ping":1})
    );
    let activity = json!({"details":"In a match","timestamps":{"start":1700000000_u64},
        "assets":{"large_image":"game_cover","large_text":"Fixture",
            "small_image":"https://example.com/badge.png"},
        "buttons":[{"label":"Website","url":"https://example.com/game"}]});
    send(
        &mut first,
        1,
        json!({"cmd":"SET_ACTIVITY","nonce":"game","args":{"pid":42,"activity":activity}}),
    )
    .await;
    let response = receive(&mut first).await;
    assert_eq!(response["nonce"], "game");
    assert!(response["evt"].is_null());
    assert_eq!(response["data"]["name"], "Fixture Game");
    presence_where(server, |presence| {
        let activities = presence["activities"].as_array().unwrap();
        activities
            .iter()
            .any(|a| a["type"] == 4 && a["state"] == "Testing Yapfast")
            && activities.iter().any(|a| {
                a["name"] == "Fixture Game"
                    && a["application_id"] == "123456789012345678"
                    && a["timestamps"]["start"] == 1700000000000_u64
                    && a["assets"]["large_image"] == "123456789012345679"
                    && a["assets"]["small_image"]
                        == "mp:external/fixture/https/example.com/badge.png"
                    && a["metadata"]["button_urls"][0] == "https://example.com/game"
            })
    })
    .await;
    send(
        &mut first,
        1,
        json!({"cmd":"SET_ACTIVITY","nonce":"invalid",
        "args":{"pid":42,"activity":{"type":4,"state":"Cannot replace custom status"}}}),
    )
    .await;
    let refused = receive(&mut first).await;
    assert_eq!(refused["evt"], "ERROR");
    assert_eq!(refused["nonce"], "invalid");

    let mut second = connect(directory).await;
    send(
        &mut second,
        1,
        json!({"cmd":"SET_ACTIVITY","nonce":"second",
        "args":{"pid":43,"activity":{"details":"Second game"}}}),
    )
    .await;
    assert!(receive(&mut second).await["evt"].is_null());
    presence_where(server, |p| {
        p["activities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["details"] == "Second game")
    })
    .await;
    // Explicit clear restores the other connected application.
    send(
        &mut second,
        1,
        json!({"cmd":"SET_ACTIVITY","nonce":"clear","args":{"pid":43,"activity":null}}),
    )
    .await;
    assert!(receive(&mut second).await["data"].is_null());
    presence_where(server, |p| {
        p["activities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["details"] == "In a match")
    })
    .await;
    drop(second);
    // Disconnect without a clear must remove the game, preserving custom status.
    drop(first);
    if detected.is_some() {
        use yapfast_discord::game_detection::custom_games::{self, CustomGame};
        custom_games::set(vec![CustomGame {
            executable: "yapfast_detected_game".into(),
            name: "My custom game".into(),
        }]);
        presence_where(server, |p| {
            p["activities"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["name"] == "My custom game"))
        })
        .await;
        // The account's menu shows the same game.
        snapshot_where(discord, |snapshot| {
            snapshot
                .playing
                .as_ref()
                .is_some_and(|playing| playing.name == "My custom game")
        })
        .await;
        let mut disabled_client = connect(directory).await;
        send(&mut disabled_client, 1, json!({"cmd":"SET_ACTIVITY","nonce":"before-disable","args":{"pid":44,"activity":{"details":"Before disabling"}}})).await;
        receive(&mut disabled_client).await;
        presence_where(server, |p| {
            p["activities"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["details"] == "Before disabling"))
        })
        .await;
        discord.set_show_game(false).await.unwrap();
        presence_where(server, |p| {
            p["activities"]
                .as_array()
                .is_some_and(|a| a.len() == 1 && a[0]["type"] == 4)
        })
        .await;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while directory.join("discord-ipc-0").exists() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "disabled RPC must release its socket"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let mut byte = [0];
        assert_eq!(
            timeout(Duration::from_secs(5), disabled_client.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        snapshot_where(discord, |snapshot| {
            !snapshot.show_game && snapshot.playing.is_none()
        })
        .await;
        discord.set_show_game(true).await.unwrap();
        presence_where(server, |p| {
            p["activities"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["name"] == "My custom game"))
        })
        .await;
        custom_games::set(Vec::new());
        drop(connect(directory).await);
        presence_where(server, |p| {
            p["activities"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["name"] == "Detected Fixture"))
        })
        .await;
        // With no RPC clients left, stopping the actual detected process
        // eventually removes the game, preserving Alice's custom status.
        drop(detected.take());
    }
    presence_where(server, |p| {
        p["activities"]
            .as_array()
            .is_some_and(|a| a.len() == 1 && a[0]["type"] == 4)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn shows_status_and_games() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start_with_games("shows_status_and_games").await;
    // Alice's custom status from her settings shows from this session, and
    // changing it changes what the session shows.
    let custom = snapshot_where(&discord, |snapshot| snapshot.custom_status.is_some())
        .await
        .custom_status
        .clone()
        .unwrap();
    assert_eq!(
        (custom.text.as_str(), custom.emoji_name.as_deref()),
        ("Testing Yapfast", Some("🧪"))
    );
    presence_where(server, |presence| {
        presence["activities"][0]["state"] == "Testing Yapfast"
    })
    .await;
    #[cfg(target_os = "linux")]
    if let Some(directory) = std::env::var_os("YAPFAST_DISCORD_RPC_DIR") {
        plays_through_rpc(&discord, server, std::path::Path::new(&directory)).await;
    }
    let away = CustomStatus {
        text: "Fixing the bridge".into(),
        emoji_name: Some("🔧".into()),
        ..Default::default()
    };
    discord.set_custom_status(Some(away.clone())).await.unwrap();
    snapshot_where(&discord, |snapshot| {
        snapshot.custom_status.as_ref() == Some(&away)
    })
    .await;
    presence_where(server, |presence| {
        presence["activities"][0]["emoji"]["name"] == "🔧"
    })
    .await;
    discord.set_custom_status(None).await.unwrap();
    snapshot_where(&discord, |snapshot| snapshot.custom_status.is_none()).await;
    presence_where(server, |presence| {
        presence["activities"] == serde_json::json!([])
    })
    .await;
    // So does the status.
    discord.set_status("dnd").await.unwrap();
    snapshot_where(&discord, |snapshot| snapshot.presence == "dnd").await;
    presence_where(server, |presence| presence["status"] == "dnd").await;
    discord.set_status("online").await.unwrap();
    presence_where(server, |presence| presence["status"] == "online").await;
}

/// What Alice's settings on the fake say of her status.
fn status_settings(server: &str) -> settings::StatusSettings {
    let answer: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/settings", "")).unwrap();
    settings::read(answer["settings"].as_str().unwrap())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn status_custom_status_and_game_sharing_keep_each_other() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("status_custom_status_and_game_sharing_keep_each_other").await;
    snapshot_where(&discord, |snapshot| snapshot.custom_status.is_some()).await;
    // A new custom status keeps the status: Discord resets what a write of
    // the settings' status leaves out.
    discord.set_status("dnd").await.unwrap();
    let away = CustomStatus {
        text: "Fixing the bridge".into(),
        emoji_name: Some("🔧".into()),
        ..Default::default()
    };
    discord.set_custom_status(Some(away.clone())).await.unwrap();
    let settings = status_settings(server);
    assert_eq!(settings.status.as_deref(), Some("dnd"));
    assert_eq!(settings.custom, Some(Some(away.clone())));
    presence_where(server, |presence| {
        presence["status"] == "dnd" && presence["activities"][0]["emoji"]["name"] == "🔧"
    })
    .await;
    // Sharing games or not keeps both.
    discord.set_show_game(false).await.unwrap();
    let settings = status_settings(server);
    assert_eq!(
        (
            settings.status.as_deref(),
            settings.custom,
            settings.show_game
        ),
        (Some("dnd"), Some(Some(away.clone())), Some(false))
    );
    // A new status keeps the custom status and the game sharing.
    discord.set_status("idle").await.unwrap();
    let settings = status_settings(server);
    assert_eq!(
        (
            settings.status.as_deref(),
            settings.custom,
            settings.show_game
        ),
        (Some("idle"), Some(Some(away.clone())), Some(false))
    );
    presence_where(server, |presence| {
        presence["status"] == "idle" && presence["activities"][0]["state"] == "Fixing the bridge"
    })
    .await;
    // Two saves at once each keep what the other changed.
    let lunch = CustomStatus {
        text: "Lunch".into(),
        ..Default::default()
    };
    let (status, custom) = tokio::join!(
        discord.set_status("dnd"),
        discord.set_custom_status(Some(lunch.clone()))
    );
    status.unwrap();
    custom.unwrap();
    let settings = status_settings(server);
    assert_eq!(
        (
            settings.status.as_deref(),
            settings.custom,
            settings.show_game
        ),
        (Some("dnd"), Some(Some(lunch)), Some(false))
    );
    presence_where(server, |presence| {
        presence["status"] == "dnd" && presence["activities"][0]["state"] == "Lunch"
    })
    .await;
    // Two changes to the same field keep the order they were asked in,
    // however their requests end up going.
    let idle_later = discord.set_status("idle");
    let invisible = discord.set_status("invisible");
    invisible.await.unwrap();
    idle_later.await.unwrap();
    assert_eq!(
        status_settings(server).status.as_deref(),
        Some("invisible"),
        "the last asked for wins"
    );
    presence_where(server, |presence| presence["status"] == "invisible").await;
    // As in Discord's client, each change starts from the settings Discord
    // sent, never from asking for them.
    let answer: serde_json::Value =
        serde_json::from_str(&control(server, "GET", "/_control/settings", "")).unwrap();
    assert_eq!(answer["reads"], 0, "no GET /users/@me/settings-proto/1");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn shows_the_accounts_own_activities_from_its_other_sessions() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("shows_the_accounts_own_activities_from_its_other_sessions").await;
    // A custom status has its own line, so this session shows nothing yet.
    let snapshot = snapshot_where(&discord, |snapshot| snapshot.custom_status.is_some()).await;
    assert!(snapshot.own_activities.is_empty());
    // Her phone plays a song, which her own profile shows, as Discord's
    // client shows another session's activities.
    post_control(server, "/_control/other-session", "");
    let snapshot = snapshot_where(&discord, |snapshot| !snapshot.own_activities.is_empty()).await;
    let song = &snapshot.own_activities[0];
    assert_eq!(song.kind, yapfast_discord::activity::Kind::Listening);
    assert_eq!(song.details.as_deref(), Some("Song on the phone"));
    assert!(song.start.is_some() && song.end.is_some());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn fetches_the_activity_feed_and_who_a_channel_may_show() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        general,
        bob,
        ..
    } = Fixture::start("activity_feed").await;
    let feed = discord.activity_feed().await.unwrap();
    assert_eq!(feed.refresh_token.as_deref(), Some("fake-feed-token"));
    assert_eq!(feed.wait_ms, Some(300_000));
    let titles: Vec<&str> = feed.entries.iter().map(|entry| &*entry.title).collect();
    assert_eq!(
        titles,
        [
            "Factorio",
            "Marconi Union",
            "Celeste",
            "Factorio",
            "Marconi Union",
            "Nobody's game",
        ],
    );
    assert!(feed.entries[0].live);
    assert!(!feed.entries[2].live);
    assert_eq!(feed.entries[3].hours, Some(13));
    assert_eq!(feed.entries[4].tracks, Some(31));

    // The second asks from where the first left off.
    discord.activity_feed().await.unwrap();
    let fetches = control(server, "GET", "/_control/activity-fetches", "");
    assert_eq!(fetches, r#"[null,"fake-feed-token"]"#);

    let mut live = discord.live();
    post_control(server, "/_control/activity-stale", "");
    let stale = live_where(&mut live, |event| match event {
        Live::ActivityFeedStale { refresh_after_ms } => Some(refresh_after_ms),
        _ => None,
    })
    .await;
    assert_eq!(stale, Some(0));

    let users: Vec<std::sync::Arc<str>> = feed
        .entries
        .iter()
        .map(|entry| entry.author.clone())
        .collect();
    let people = discord.activity_people(&guild, &general, &users).await;
    assert!(people.iter().any(|person| &*person.id == bob.as_str()));
    assert!(
        people
            .iter()
            .all(|person| &*person.id != "1400000000000000999"),
        "someone in no server shared with the account stays out",
    );
    assert_eq!(
        people
            .iter()
            .find(|person| &*person.id == bob.as_str())
            .map(|person| &*person.name),
        Some("Bob"),
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn resumes_the_gateway() {
    let Fixture {
        _fake,
        discord,
        server,
        ..
    } = Fixture::start("resumes_the_gateway").await;
    let mut live = discord.live();
    // Discord asks the gateway to reconnect: the session resumes, without a
    // new identify, and carries on.
    post_control(server, "/_control/gateway-reconnect", "");
    timeout(WAIT, async {
        while self::control(server, "GET", "/_control/sessions", "")
            != r#"{"identifies":1,"resumes":1}"#
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the session didn't resume");
    post_control(server, "/_control/typing", "");
    live_where(&mut live, |event| {
        matches!(event, Live::Typing { .. }).then_some(())
    })
    .await;
    assert_eq!(discord.snapshots().borrow().status, Status::Online);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn links_by_qr_code() {
    let Fixture { _fake, server, .. } = Fixture::start("links_by_qr_code").await;
    // Linking by QR code: the code shows, Alice scans it and confirms on her
    // phone, and the login arrives.
    let (steps, mut shown) = tokio::sync::mpsc::unbounded_channel();
    let linking = tokio::spawn(remote_auth::link(move |step| {
        let _ = steps.send(step);
    }));
    let Step::Code(code) = next_step(&mut shown).await else {
        panic!("no code")
    };
    assert!(code.starts_with("https://discord.com/ra/"), "{code}");
    post_control(server, "/_control/qr-scan", "");
    assert!(matches!(next_step(&mut shown).await, Step::Scanned { name, .. } if name == "alice"));
    post_control(server, "/_control/qr-confirm", "");
    let login = timeout(WAIT, linking).await.unwrap().unwrap().unwrap();
    assert_eq!(login, client::TEST_TOKEN);

    // Saying no on the phone stops linking.
    let (steps, mut shown) = tokio::sync::mpsc::unbounded_channel();
    let linking = tokio::spawn(remote_auth::link(move |step| {
        let _ = steps.send(step);
    }));
    assert!(matches!(next_step(&mut shown).await, Step::Code(_)));
    post_control(server, "/_control/qr-cancel", "");
    let refused = timeout(WAIT, linking).await.unwrap().unwrap().unwrap_err();
    assert_eq!(
        refused.downcast_ref(),
        Some(&remote_auth::Stopped::Cancelled)
    );

    // Exercise a normal gateway close after phone approval. A valid ticket
    // can still answer two successive challenges, each with its own data.
    post_control(server, "/_control/qr-captcha?twice", "");
    let (steps, mut shown) = tokio::sync::mpsc::unbounded_channel();
    let linking = tokio::spawn(remote_auth::link(move |step| {
        let _ = steps.send(step);
    }));
    assert!(matches!(next_step(&mut shown).await, Step::Code(_)));
    post_control(server, "/_control/qr-scan", "");
    assert!(matches!(next_step(&mut shown).await, Step::Scanned { .. }));
    post_control(server, "/_control/qr-confirm?close", "");
    let browser = reqwest::Client::new();
    for (rqdata, solution) in [
        ("fake-rqdata", "solved"),
        ("second-rqdata", "second-solution"),
    ] {
        let Step::Captcha(page) = next_step(&mut shown).await else {
            panic!("no captcha")
        };
        let html = browser
            .get(&page)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(html.contains(r#""fake-sitekey""#));
        assert!(html.contains(rqdata));
        let answer = browser.post(&page).body(solution).send().await.unwrap();
        assert_eq!(answer.status(), reqwest::StatusCode::NO_CONTENT);
    }
    let login = timeout(WAIT, linking).await.unwrap().unwrap().unwrap();
    assert_eq!(login, client::TEST_TOKEN);

    // Yapfast sent back the device cookie the API set, once it had it: the API
    // set it once, or twice if the first requests went together.
    let cookies: serde_json::Value =
        serde_json::from_str(&self::control(server, "GET", "/_control/cookies", "")).unwrap();
    // The cookie jar outlives each test's fake, so this one may find it
    // already set.
    assert!(cookies["returned"].as_u64().unwrap() > 0, "{cookies}");
    assert!(cookies["set"].as_u64().unwrap() <= 2, "{cookies}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn reports_a_rejected_qr_ticket_after_two_captchas() {
    let Fixture { _fake, server, .. } =
        Fixture::start("reports_a_rejected_qr_ticket_after_two_captchas").await;
    post_control(server, "/_control/qr-captcha?twice&reject", "");
    let (steps, mut shown) = tokio::sync::mpsc::unbounded_channel();
    let linking = tokio::spawn(remote_auth::link(move |step| {
        let _ = steps.send(step);
    }));
    assert!(matches!(next_step(&mut shown).await, Step::Code(_)));
    post_control(server, "/_control/qr-scan", "");
    assert!(matches!(next_step(&mut shown).await, Step::Scanned { .. }));
    post_control(server, "/_control/qr-confirm", "");
    let browser = reqwest::Client::new();
    for solution in ["solved", "second-solution"] {
        let Step::Captcha(page) = next_step(&mut shown).await else {
            panic!("no captcha");
        };
        let status = browser
            .post(&page)
            .body(solution)
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(status, reqwest::StatusCode::NO_CONTENT);
    }
    let error = timeout(WAIT, linking).await.unwrap().unwrap().unwrap_err();
    assert_eq!(yapfast_discord::api::code(&error), Some(20042));
    assert_eq!(
        error.to_string(),
        "Discord rejected the QR sign-in ticket. Try again with a new code."
    );
    assert!(shown.recv().await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn refreshes_the_login_and_signs_out() {
    let Fixture {
        _fake,
        discord,
        server,
        guild,
        ..
    } = Fixture::start("refreshes_the_login_and_signs_out").await;
    // Discord refreshes the login in a READY: Yapfast asks with the new one
    // from then on, and keeps it. The fake takes only the latest, and hands
    // the first back the same way for the tests after these.
    for login in ["yapfast-refreshed-token", client::TEST_TOKEN] {
        post_control(server, "/_control/refresh-token", login);
        timeout(Duration::from_secs(30), async {
            while token::load().unwrap() != login {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("the refreshed login wasn't kept");
        discord.own_profile(Some(&guild)).await.unwrap();
    }

    // Signing out ends the session on Discord.
    discord.logout().await.unwrap();
    assert_eq!(self::control(server, "GET", "/_control/logouts", ""), "1");
}

/// What linking showed next.
async fn next_step(shown: &mut tokio::sync::mpsc::UnboundedReceiver<Step>) -> Step {
    timeout(WAIT, shown.recv())
        .await
        .expect("timed out linking")
        .expect("linking stopped")
}
