//! Bots' buttons, menus and forms used as Alice against
//! testing/fake_discord.ts, whose Status Bot answers them; see "Interactions"
//! at the top of the fake. `scripts/test-fake-discord interaction` runs them.

use std::{sync::Arc, time::Duration};

use serde_json::{Value, json};
use tokio::time::timeout;
use yapfast_discord::{
    client,
    interactions::{self, Interaction},
    message::Message,
    service::{Discord, Live, Snapshot, Status},
    token,
};

const WAIT: Duration = Duration::from_secs(10);

/// The fake, stopped when the test ends. Tests in one process take turns,
/// on the port the first one's fake took.
struct Fake {
    process: std::process::Child,
    _turn: tokio::sync::MutexGuard<'static, ()>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// A test's fake Discord and Alice's session on it, with Status Bot's
/// deploy message posted in #general.
struct Fixture {
    _fake: Fake,
    discord: Discord,
    server: &'static str,
    guild: String,
    deploy: Message,
}

impl Fixture {
    async fn start(name: &str) -> Fixture {
        static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        static PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
        let turn = TURN.lock().await;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let log =
            std::fs::File::create(root.join(format!("target/fake-discord-{name}.log"))).unwrap();
        let port = PORT.get().copied().unwrap_or(0).to_string();
        let mut process = std::process::Command::new("bun")
            .args(["testing/fake_discord.ts", "--port", &port])
            .current_dir(&root)
            .stdout(log.try_clone().unwrap())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("could not start bun testing/fake_discord.ts");
        let said = process.stderr.take().unwrap();
        let (listening, address) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            use std::io::{BufRead, Write};
            let mut log = log;
            let mut listening = Some(listening);
            for line in std::io::BufReader::new(said).lines().map_while(Result::ok) {
                let found = line
                    .strip_prefix("fake Discord on ")
                    .and_then(|rest| rest.split_whitespace().next());
                if let (Some(found), Some(listening)) = (found, listening.take()) {
                    let _ = listening.send(found.to_owned());
                }
                let _ = writeln!(log, "{line}");
            }
        });
        let fake = Fake {
            process,
            _turn: turn,
        };
        let address = timeout(WAIT, address)
            .await
            .expect("the fake Discord didn't start")
            .unwrap_or_else(|_| {
                panic!("the fake Discord exited; see target/fake-discord-{name}.log")
            });
        if PORT.get().is_none() {
            PORT.set(address.rsplit(':').next().unwrap().parse().unwrap())
                .unwrap();
            // SAFETY: the first test of the process sets this, holding the
            // turn, before anything reads it.
            unsafe { std::env::set_var("YAPFAST_DISCORD_TEST_SERVER", &address) };
        }
        let server = client::test_server().unwrap();
        assert_eq!(server, address);
        token::store(client::TEST_TOKEN).unwrap();
        let discord = Discord::start(token::load().unwrap()).unwrap();
        let snapshot = snapshot_where(&discord, |snapshot| snapshot.status == Status::Online).await;
        let guild = snapshot.servers[0].id.clone();
        let mut live = discord.live();
        control(server, "POST", "/_control/components", "");
        let deploy = live_where(&mut live, |live| match live {
            Live::Created(message) if message.content.starts_with("Deploy") => Some(message),
            _ => None,
        })
        .await;
        Fixture {
            _fake: fake,
            discord,
            server,
            guild,
            deploy,
        }
    }

    /// A click on the deploy message's control `custom_id`, of `kind`.
    fn click(&self, nonce: &str, kind: u64, custom_id: &str) -> Interaction {
        Interaction::Component {
            nonce: nonce.into(),
            guild: Some(self.guild.clone()),
            channel: self.deploy.channel_id.clone(),
            message: self.deploy.id.clone(),
            message_flags: self.deploy.flags,
            // Discord leaves a bot's own message without one.
            application: None,
            author: self.deploy.author_id.clone(),
            component_type: kind,
            custom_id: custom_id.into(),
            values: None,
        }
    }

    /// The bodies Alice sent to POST /interactions, oldest first.
    fn sent(&self) -> Vec<Value> {
        serde_json::from_str(&control(self.server, "GET", "/_control/interactions", "")).unwrap()
    }
}

/// Asks one of the fake's controls, and returns its answer's body.
fn control(server: &str, method: &str, path: &str, body: &str) -> String {
    use std::io::{Read, Write};
    let address = server.trim_start_matches("http://");
    let mut connection = std::net::TcpStream::connect(address).unwrap();
    write!(
        connection,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut answer = Vec::new();
    let mut byte = [0_u8];
    while !answer.ends_with(b"\r\n\r\n") {
        connection.read_exact(&mut byte).unwrap();
        answer.push(byte[0]);
    }
    let head = String::from_utf8(answer).unwrap();
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let length = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().unwrap())
    });
    let mut body = Vec::new();
    match length {
        Some(length) => {
            body.resize(length, 0);
            connection.read_exact(&mut body).unwrap();
        }
        None => {
            connection.read_to_end(&mut body).unwrap();
        }
    }
    String::from_utf8(body).unwrap()
}

/// The first snapshot that passes `check`.
async fn snapshot_where(discord: &Discord, check: impl Fn(&Snapshot) -> bool) -> Arc<Snapshot> {
    let mut snapshots = discord.snapshots();
    timeout(WAIT, async {
        loop {
            let snapshot = snapshots.borrow_and_update().clone();
            if let Status::Offline(reason) = &snapshot.status {
                panic!("offline: {reason}");
            }
            if check(&snapshot) {
                return snapshot;
            }
            snapshots.changed().await.unwrap();
        }
    })
    .await
    .expect("timed out waiting for a snapshot")
}

/// The next live event that `pick` accepts.
async fn live_where<T>(
    live: &mut tokio::sync::broadcast::Receiver<Live>,
    pick: impl Fn(Live) -> Option<T>,
) -> T {
    timeout(WAIT, async {
        loop {
            if let Some(found) = pick(live.recv().await.unwrap()) {
                return found;
            }
        }
    })
    .await
    .expect("timed out waiting for a live event")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn a_button_click_posts_discords_body_and_succeeds() {
    let fixture = Fixture::start("a_button_click_posts_discords_body_and_succeeds").await;
    let mut live = fixture.discord.live();
    let click = fixture.click("1000", 2, "ok");
    assert!(fixture.discord.interact(&click).await.unwrap());
    let sent = fixture.sent();
    assert_eq!(sent.len(), 1);
    let body = &sent[0];
    assert_eq!(body["type"], 3);
    assert_eq!(body["nonce"], "1000");
    assert_eq!(body["guild_id"], fixture.guild.as_str());
    assert_eq!(body["channel_id"], fixture.deploy.channel_id.as_str());
    assert_eq!(body["message_id"], fixture.deploy.id.as_str());
    assert_eq!(body["message_flags"], 0);
    // A bot message without application_id is its bot's application's.
    assert_eq!(body["application_id"], fixture.deploy.author_id.as_str());
    assert!(body["session_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(
        body["data"],
        json!({"component_type": 2, "custom_id": "ok"})
    );
    // Discord said it took it and the app answered, then the app changed
    // its message.
    let created = live_where(&mut live, |live| match live {
        Live::InteractionCreated { nonce, id } if nonce == "1000" => Some(id),
        _ => None,
    })
    .await;
    assert!(!created.is_empty());
    let updated = live_where(&mut live, |live| match live {
        Live::Updated(message) if message.id == fixture.deploy.id => Some(message),
        _ => None,
    })
    .await;
    assert!(
        updated.content.contains("Approved by Alice"),
        "{}",
        updated.content
    );

    // A pick in its menu sends the values picked.
    let mut pick = fixture.click("1001", 3, "environment");
    if let Interaction::Component { values, .. } = &mut pick {
        *values = Some(vec!["production".into()]);
    }
    assert!(fixture.discord.interact(&pick).await.unwrap());
    assert_eq!(
        fixture.sent()[1]["data"],
        json!({"component_type": 3, "custom_id": "environment", "values": ["production"]})
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn a_modal_opens_and_submits_its_values() {
    let fixture = Fixture::start("a_modal_opens_and_submits_its_values").await;
    let mut live = fixture.discord.live();
    assert!(
        fixture
            .discord
            .interact(&fixture.click("2000", 2, "form"))
            .await
            .unwrap()
    );
    let form = live_where(&mut live, |live| match live {
        Live::InteractionModal(form) if form["nonce"] == "2000" => Some(form),
        _ => None,
    })
    .await;
    assert_eq!(form["title"], "Deploy notes");
    assert_eq!(form["components"][0]["type"], 10);
    let submit = Interaction::Modal {
        nonce: "2001".into(),
        guild: Some(fixture.guild.clone()),
        channel: form["channel_id"].as_str().unwrap().into(),
        application: form["application"]["id"].as_str().unwrap().into(),
        id: form["id"].as_str().unwrap().into(),
        custom_id: form["custom_id"].as_str().unwrap().into(),
        components: json!([
            {"type": 18, "component": {"type": 4, "custom_id": "summary", "value": "Faster start"}},
            {"type": 18, "component": {"type": 3, "custom_id": "environment", "values": ["production"]}},
        ]),
    };
    assert!(fixture.discord.interact(&submit).await.unwrap());
    let body = &fixture.sent()[1];
    assert_eq!(body["type"], 5);
    assert_eq!(body["data"]["id"], form["id"]);
    assert_eq!(body["data"]["custom_id"], "deploy-notes");
    assert!(body.get("message_id").is_none());
    // The app's answer only Alice sees.
    let reply = live_where(&mut live, |live| match live {
        Live::Created(message) if message.content.starts_with("Noted") => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(reply.content, "Noted: Faster start; production");
    assert_eq!(reply.flags & (1 << 6), 1 << 6);

    // A form Discord didn't open is refused, and never sent again.
    let stale = Interaction::Modal {
        nonce: "2002".into(),
        guild: None,
        channel: fixture.deploy.channel_id.clone(),
        application: fixture.deploy.author_id.clone(),
        id: "1".into(),
        custom_id: "deploy-notes".into(),
        components: json!([]),
    };
    assert!(fixture.discord.interact(&stale).await.is_err());
    assert_eq!(fixture.sent().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn a_failed_interaction_says_why() {
    let fixture = Fixture::start("a_failed_interaction_says_why").await;
    let mut live = fixture.discord.live();
    assert!(
        fixture
            .discord
            .interact(&fixture.click("3000", 2, "fail"))
            .await
            .unwrap()
    );
    let reason = live_where(&mut live, |live| match live {
        Live::InteractionFailure { nonce, reason } if nonce == "3000" => Some(reason),
        _ => None,
    })
    .await;
    assert_eq!(
        interactions::failure_reason(reason),
        "This interaction failed."
    );
    // An app Discord doesn't know refuses the request itself, with
    // Discord's words.
    let mut unknown = fixture.click("3001", 2, "ok");
    if let Interaction::Component { application, .. } = &mut unknown {
        *application = Some("1".into());
    }
    let error = fixture.discord.interact(&unknown).await.unwrap_err();
    assert_eq!(error.to_string(), "Discord doesn't know that application.");
    assert_eq!(fixture.sent().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn a_slow_bot_holds_up_only_its_own_interaction() {
    let fixture = Arc::new(Fixture::start("a_slow_bot_holds_up_only_its_own_interaction").await);
    let slow = {
        let fixture = fixture.clone();
        tokio::spawn(async move {
            fixture
                .discord
                .interact(&fixture.click("4000", 2, "slow"))
                .await
        })
    };
    // The fake took the slow one before the next goes.
    timeout(WAIT, async {
        while fixture.sent().is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        timeout(
            WAIT,
            fixture.discord.interact(&fixture.click("4001", 2, "ok"))
        )
        .await
        .expect("a slow bot held up another interaction")
        .unwrap()
    );
    assert!(
        !slow.is_finished(),
        "the slow one still waits for its answer"
    );
    slow.abort();
    // Each was sent once.
    let nonces: Vec<_> = fixture
        .sent()
        .iter()
        .map(|body| body["nonce"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(nonces, ["4000", "4001"]);
}

/// Status Bot's command `name` as its command index gives it, in short.
fn command(name: &str) -> Arc<Value> {
    Arc::new(json!({
        "id": format!("1400000000000000{}", 700 + name.len()),
        "application_id": "1400000000000000007",
        "version": "1400000000000000799",
        "type": 1,
        "name": name,
        "description": "",
    }))
}

impl Fixture {
    /// Status Bot's command `name` with `options`, in #general.
    fn run(&self, nonce: &str, name: &str, options: Value) -> interactions::CommandUse {
        interactions::CommandUse {
            nonce: nonce.into(),
            guild: Some(self.guild.clone()),
            channel: self.deploy.channel_id.clone(),
            application: self.deploy.author_id.clone(),
            command: command(name),
            options,
            target: None,
            files: vec![],
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn a_slash_command_posts_its_options_and_its_answer_arrives() {
    let fixture = Fixture::start("a_slash_command_posts_its_options_and_its_answer_arrives").await;
    let mut live = fixture.discord.live();
    let options = json!([{"type": 1, "name": "set", "options": [
        {"type": 3, "name": "key", "value": "region"},
    ]}]);
    let run = fixture.run("5000", "config", options.clone());
    assert!(fixture.discord.run_command(run, vec![]).await.unwrap());
    let sent = fixture.sent();
    assert_eq!(sent.len(), 1);
    let body = &sent[0];
    assert_eq!(body["type"], 2);
    assert_eq!(body["nonce"], "5000");
    assert_eq!(body["guild_id"], fixture.guild.as_str());
    assert_eq!(body["application_id"], fixture.deploy.author_id.as_str());
    assert_eq!(body["analytics_location"], "slash_ui");
    assert!(body["session_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(body["data"]["name"], "config");
    assert_eq!(body["data"]["version"], "1400000000000000799");
    assert_eq!(body["data"]["application_command"], *command("config"));
    assert_eq!(body["data"]["options"], options);
    // Discord passed it on, then the app's answer came as a message.
    let created = live_where(&mut live, |live| match live {
        Live::InteractionCreated { nonce, id } if nonce == "5000" => Some(id),
        _ => None,
    })
    .await;
    assert!(!created.is_empty());
    let answer = live_where(&mut live, |live| match live {
        Live::Created(message) if message.author_id == fixture.deploy.author_id => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(answer.content, "Ran /config.");
    assert_eq!(
        answer.application_id.as_deref(),
        Some(fixture.deploy.author_id.as_str())
    );

    // One the app answers only Alice: her own, never in the channel.
    let run = fixture.run("5001", "secret", json!([]));
    assert!(fixture.discord.run_command(run, vec![]).await.unwrap());
    let secret = live_where(&mut live, |live| match live {
        Live::Created(message) if message.content.starts_with("Only you") => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(secret.flags & (1 << 6), 1 << 6);
    assert_eq!(fixture.sent().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn a_deferred_answer_shows_thinking_then_its_content() {
    let fixture = Fixture::start("a_deferred_answer_shows_thinking_then_its_content").await;
    let mut live = fixture.discord.live();
    let options = json!([
        {"type": 3, "name": "text", "value": "hi"},
        {"type": 4, "name": "times", "value": 2},
    ]);
    let run = fixture.run("6000", "echo", options);
    assert!(fixture.discord.run_command(run, vec![]).await.unwrap());
    let thinking = live_where(&mut live, |live| match live {
        Live::Created(message) if message.author_id == fixture.deploy.author_id => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(thinking.content, "");
    assert_eq!(thinking.flags & (1 << 7), 1 << 7);
    let answered = live_where(&mut live, |live| match live {
        Live::Updated(message) if message.id == thinking.id => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(answered.content, "hi hi");
    assert_eq!(answered.flags & (1 << 7), 0);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn autocomplete_keeps_only_the_newest_answer() {
    let fixture = Arc::new(Fixture::start("autocomplete_keeps_only_the_newest_answer").await);
    let typed = |nonce: &str, value: &str| {
        fixture.run(
            nonce,
            "find",
            json!([{"type": 3, "name": "query", "value": value, "focused": true}]),
        )
    };
    // Keys pressed within the pause: only the last is sent.
    let first = {
        let (fixture, run) = (fixture.clone(), typed("7000", "s"));
        tokio::spawn(async move { fixture.discord.autocomplete(run).await })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    let choices = fixture
        .discord
        .autocomplete(typed("7001", "st"))
        .await
        .unwrap()
        .expect("the newest autocomplete is answered");
    assert_eq!(
        choices,
        json!([{"name": "staging", "value": "staging"}, {"name": "status", "value": "status"}])
    );
    assert_eq!(first.await.unwrap().unwrap(), None);
    let sent = fixture.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["type"], 4);
    assert_eq!(sent[0]["nonce"], "7001");
    assert!(sent[0].get("analytics_location").is_none());
    assert_eq!(sent[0]["data"]["options"][0]["focused"], true);

    // One sent but answered after a newer one started is dropped.
    let slow = {
        let (fixture, run) = (fixture.clone(), typed("7002", "sl"));
        tokio::spawn(async move { fixture.discord.autocomplete(run).await })
    };
    timeout(WAIT, async {
        while fixture.sent().len() < 2 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let newest = fixture
        .discord
        .autocomplete(typed("7003", "p"))
        .await
        .unwrap();
    assert_eq!(
        newest,
        Some(json!([{"name": "production", "value": "production"}]))
    );
    assert_eq!(slow.await.unwrap().unwrap(), None);
    assert_eq!(fixture.sent().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn an_attachment_option_uploads_in_the_same_request() {
    let fixture = Fixture::start("an_attachment_option_uploads_in_the_same_request").await;
    let mut live = fixture.discord.live();
    let mut run = fixture.run(
        "8000",
        "upload",
        json!([{"type": 11, "name": "file", "value": "0"}]),
    );
    run.files = vec!["a.png".into()];
    assert!(
        fixture
            .discord
            .run_command(run, vec![vec![7; 300]])
            .await
            .unwrap()
    );
    let sent = fixture.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0]["data"]["attachments"],
        json!([{"id": "0", "filename": "a.png"}])
    );
    assert_eq!(
        sent[0]["files"],
        json!([{"index": "0", "name": "a.png", "size": 300}])
    );
    let answer = live_where(&mut live, |live| match live {
        Live::Created(message) if message.author_id == fixture.deploy.author_id => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(answer.content, "Got a.png (300 bytes)");

    // A file named but not sent never goes.
    let mut lost = fixture.run(
        "8001",
        "upload",
        json!([{"type": 11, "name": "file", "value": "0"}]),
    );
    lost.files = vec!["b.png".into()];
    assert!(fixture.discord.run_command(lost, vec![]).await.is_err());
    assert_eq!(fixture.sent().len(), 1);
}
