//! Apps' command indexes fetched as Alice against testing/fake_discord.ts,
//! whose "Server" has Status Bot's commands and whose Alice installed Notes
//! for herself; see "Commands" at the top of the fake.
//! `scripts/test-fake-discord commands` runs them.

use std::{sync::Arc, time::Duration};

use tokio::time::timeout;
use yapfast_discord::{
    client,
    commands::{self, CommandIndex, Place},
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

/// A test's fake Discord and Alice's session on it, online.
struct Fixture {
    _fake: Fake,
    discord: Discord,
    server: &'static str,
    snapshot: Arc<Snapshot>,
}

/// "Server"'s #general and #ideas, and Alice's DM with Bob, as the fake has them.
const GENERAL: &str = "1400000000000000102";
const IDEAS: &str = "1400000000000000106";
const DM_BOB: &str = "1400000000000000120";

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
        Fixture {
            _fake: fake,
            discord,
            server,
            snapshot,
        }
    }

    fn guild(&self) -> &str {
        &self.snapshot.servers[0].id
    }

    /// The indexes Alice fetched, by resource, oldest first.
    fn fetches(&self) -> Vec<String> {
        serde_json::from_str(&control(
            self.server,
            "GET",
            "/_control/command-index-fetches",
            "",
        ))
        .unwrap()
    }

    /// How many times Alice fetched `resource`'s index.
    fn fetched(&self, resource: &str) -> usize {
        self.fetches()
            .iter()
            .filter(|fetched| *fetched == resource)
            .count()
    }

    /// The slash commands Alice may use in "Server"'s `channel`, by name.
    fn slash_commands_in(&self, index: &CommandIndex, channel: &str) -> Vec<String> {
        let server = &self.snapshot.servers[0];
        let me = self.snapshot.user_id.as_deref().unwrap();
        let roles = &server.marks.roles;
        let found = server.server.channel(channel).expect("the fake's channel");
        let place = Place {
            guild: Some(&server.id),
            channel,
            parent: None,
            recipient: None,
            permissions: server.channel_permissions(found, me, roles),
            roles,
            nsfw: found.nsfw,
        };
        slash_names(index, &place)
    }
}

/// The names of the slash commands of `index` usable at `place`, subcommands
/// apart.
fn slash_names(index: &CommandIndex, place: &Place) -> Vec<String> {
    index
        .usable(place)
        .filter(|command| command.kind == commands::CHAT_INPUT)
        .flat_map(|command| {
            command
                .variants()
                .iter()
                .map(|variant| variant.name())
                .collect::<Vec<_>>()
        })
        .collect()
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
async fn the_index_is_fetched_once_per_guild_until_discord_says_it_changed() {
    let fixture =
        Fixture::start("the_index_is_fetched_once_per_guild_until_discord_says_it_changed").await;
    let guild = fixture.guild().to_owned();
    let resource = format!("guilds/{guild}");
    // Nothing is asked ahead: only typing "/" asks.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(fixture.fetches().is_empty(), "{:?}", fixture.fetches());
    let index = fixture
        .discord
        .command_index(Some(&guild), GENERAL)
        .await
        .unwrap();
    assert!(index.commands.iter().any(|command| command.name == "ping"));
    assert_eq!(
        fixture.fetches(),
        [resource.clone(), "users/@me".to_owned()]
    );
    // Another channel of the same server, and the first again, ask nothing.
    let again = fixture
        .discord
        .command_index(Some(&guild), IDEAS)
        .await
        .unwrap();
    assert_eq!(again.commands.len(), index.commands.len());
    fixture
        .discord
        .command_index(Some(&guild), GENERAL)
        .await
        .unwrap();
    assert_eq!(fixture.fetches().len(), 2);
    // Discord says the server's commands changed: the next "/" asks again,
    // for the server's alone.
    let mut live = fixture.discord.live();
    control(fixture.server, "POST", "/_control/commands-changed", "");
    let changed = live_where(&mut live, |live| match live {
        Live::CommandIndexStale(commands::Stale::Guild { guild, version }) => {
            Some((guild, version))
        }
        _ => None,
    })
    .await;
    assert_eq!(changed.0, guild);
    assert_ne!(changed.1.as_deref(), Some(index.version.as_str()));
    timeout(WAIT, async {
        while fixture.fetched(&resource) < 2 {
            fixture
                .discord
                .command_index(Some(&guild), GENERAL)
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the changed index was never fetched again");
    assert_eq!(fixture.fetched(&resource), 2);
    assert_eq!(fixture.fetched("users/@me"), 1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn commands_alice_cannot_use_are_not_offered() {
    let fixture = Fixture::start("commands_alice_cannot_use_are_not_offered").await;
    let guild = fixture.guild().to_owned();
    let index = fixture
        .discord
        .command_index(Some(&guild), GENERAL)
        .await
        .unwrap();
    // /admin is for administrators, which Alice isn't; Notes is hers.
    assert!(index.commands.iter().any(|command| command.name == "admin"));
    assert_eq!(
        fixture.slash_commands_in(&index, GENERAL),
        [
            "ping",
            "echo",
            "pick",
            "find",
            "upload",
            "config set",
            "config get",
            "note"
        ]
    );
    // Status Bot's commands are off in #ideas; Alice's own app isn't the
    // server's to turn off.
    assert_eq!(fixture.slash_commands_in(&index, IDEAS), ["note"]);
    // Its message and user commands are for menus, not the "/" picker.
    let menus: Vec<_> = index
        .commands
        .iter()
        .filter(|command| command.kind != commands::CHAT_INPUT)
        .map(|command| (command.kind, command.name.as_str()))
        .collect();
    assert_eq!(
        menus,
        [(commands::MESSAGE, "Quote"), (commands::USER, "Wave")]
    );
    // What goes back to Discord with a command is what its index said.
    let echo = index
        .commands
        .iter()
        .find(|command| command.name == "echo")
        .unwrap();
    let sent = echo.to_json();
    assert_eq!(sent["name_default"], "echo");
    assert_eq!(sent["options"][1]["max_value"], 5);
    assert_eq!(sent["version"], echo.version.as_str());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts testing/fake_discord.ts with bun; run scripts/test-fake-discord"]
async fn user_installed_commands_show_in_dms() {
    let fixture = Fixture::start("user_installed_commands_show_in_dms").await;
    let index = fixture.discord.command_index(None, DM_BOB).await.unwrap();
    // Bob is no bot, so his DM has no index of its own to ask for, as
    // Discord's client asks for none.
    assert_eq!(fixture.fetches(), ["users/@me".to_owned()]);
    let bob = fixture
        .snapshot
        .conversations
        .iter()
        .find(|dm| dm.id == DM_BOB)
        .and_then(|dm| dm.person.clone())
        .unwrap();
    let place = Place {
        guild: None,
        channel: DM_BOB,
        parent: None,
        recipient: Some(&bob),
        permissions: 0,
        roles: &[],
        nsfw: false,
    };
    assert_eq!(slash_names(&index, &place), ["note"]);
    let notes = index.app(&index.commands[0].application_id).unwrap();
    assert_eq!(notes.name, "Notes");
    // The account's own is kept.
    fixture.discord.command_index(None, DM_BOB).await.unwrap();
    assert_eq!(fixture.fetches().len(), 1);
}
