//! Yapfast's Discord service against testing/fake_discord.ts, which each
//! test starts afresh with bun. `scripts/test-fake-discord [FILTER]` runs
//! them, side by side under cargo nextest.
//!
//! Each area's tests are in its file beside this one, which holds what they
//! share: the fake a test starts ([`Fixture`]) and the waits on it.

use std::{sync::Arc, time::Duration};

use tokio::time::timeout;
use yapfast_discord::{
    api::Span,
    client,
    member_list::{ListedMember, Presence},
    message::{Reply, Shown},
    notifications::{Change, Level},
    ready::{Access, Deletes, PRIVATE},
    remote_auth::{self, Step},
    rtp::Codec,
    search::Sort,
    service::{Discord, Live, Searched, Snapshot, Status},
    settings::{self, CustomStatus},
    stream,
    stream_connection::{Frame, Link, SEND_CODEC, Sound, StreamReceiver, StreamSender},
    token,
    voice::{Call, Listener},
};

mod account;
mod calls;
mod direct_messages;
mod expressions;
mod links;
mod messages;
mod profiles;
mod servers;
mod threads;
mod voice_channels;

const WAIT: Duration = Duration::from_secs(10);

/// A call's audio, unheard.
struct Quiet;

impl Listener for Quiet {
    fn tick(&self, _: &[(u64, &[i16])]) {}
    fn disconnected(&self, reason: &str) {
        panic!("voice disconnected: {reason}");
    }
}

/// Waits until `check` holds, polling the fake.
async fn until(what: &str, mut check: impl FnMut() -> bool) {
    let started = std::time::Instant::now();
    while !check() {
        assert!(started.elapsed() < WAIT, "timed out waiting until {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Posts `body` to one of the fake's controls.
fn post_control(server: &str, path: &str, body: &str) {
    control(server, "POST", path, body);
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
    // Read the body by its length: Bun can leave the connection open after
    // an async handler's answer until its idle timeout, about 10 seconds.
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

/// The presence Alice's session last sent the fake. Against it, Yapfast
/// holds presence updates back for 300 ms rather than Discord's 20 seconds,
/// and looks for games every half second rather than 15.
async fn presence_where(server: &str, check: impl Fn(&serde_json::Value) -> bool) {
    timeout(WAIT + Duration::from_secs(40), async {
        loop {
            let presence = serde_json::from_str(&control(server, "GET", "/_control/presence", ""))
                .unwrap_or_default();
            if check(&presence) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("timed out waiting for the presence");
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

#[test]
fn waits_as_detected_game() {
    if std::env::var_os("YAPFAST_TEST_DETECTED_GAME").is_some() {
        use std::io::Read;
        let _ = std::io::stdin().read_exact(&mut [0_u8]);
    }
}

/// The fake Discord, started afresh for each test, so no test sees what
/// another left. The first in a process lets the fake pick a free port and
/// points Yapfast at it; the rest take turns on that port, as `cargo test`
/// runs them. `cargo nextest` runs each test in a process of its own, so
/// there each test has a port of its own and they run side by side.
struct Fake {
    process: std::process::Child,
    _games: Option<Games>,
    _turn: tokio::sync::MutexGuard<'static, ()>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// Directories of a test's own for Rich Presence's socket, the game list
/// and Steam, which turn both on against the fake while it runs.
struct Games {
    _directory: tempfile::TempDir,
}

impl Games {
    const VARIABLES: [&str; 3] = [
        "YAPFAST_DISCORD_RPC_DIR",
        "YAPFAST_DISCORD_GAMES_CACHE",
        "YAPFAST_STEAM_DIR",
    ];

    fn new() -> Games {
        // As short a path as a socket's may be, which must stay under 108
        // bytes: /tmp, and on Windows the system's temporary directory, the
        // shortest there is.
        #[cfg(not(windows))]
        let base = std::path::PathBuf::from("/tmp");
        #[cfg(windows)]
        let base = std::env::temp_dir();
        let directory = tempfile::Builder::new()
            .prefix("yapfast-rpc")
            .tempdir_in(base)
            .unwrap();
        let paths = [
            directory.path().to_owned(),
            directory.path().join("games.json"),
            directory.path().join("steam"),
        ];
        for (variable, path) in Self::VARIABLES.into_iter().zip(paths) {
            // SAFETY: the test holds the turn, and its session, which
            // reads these, hasn't started.
            unsafe { std::env::set_var(variable, path) };
        }
        Games {
            _directory: directory,
        }
    }
}

impl Drop for Games {
    fn drop(&mut self) {
        for variable in Self::VARIABLES {
            // SAFETY: the test still holds the turn.
            unsafe { std::env::remove_var(variable) };
        }
    }
}

/// A test's fake Discord and Alice's session on it, once READY and its
/// voice states have arrived.
struct Fixture {
    _fake: Fake,
    discord: Discord,
    server: &'static str,
    snapshot: Arc<Snapshot>,
    guild: String,
    general: String,
    lounge: String,
    bob: String,
}

impl Fixture {
    /// Starts the fake for the test `name`, logging to
    /// target/fake-discord-`name`.log, and Alice's session on it.
    async fn start(name: &str) -> Fixture {
        Self::open(name, false).await
    }

    /// The same, with Rich Presence and game detection on.
    async fn start_with_games(name: &str) -> Fixture {
        Self::open(name, true).await
    }

    async fn open(name: &str, games: bool) -> Fixture {
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
        // The fake says where it listens once it does; the rest of what it
        // says goes to the log.
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
            _games: games.then(Games::new),
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
        assert_eq!(
            server, address,
            "the test server was read before the fake started"
        );
        // A test that refreshed the login, or failed, may have left another.
        token::store(client::TEST_TOKEN).unwrap();
        let token = token::load().unwrap();
        let discord = Discord::start(token).unwrap();
        let snapshot = snapshot_where(&discord, |snapshot| {
            snapshot.status == Status::Online && !snapshot.voice.is_empty()
        })
        .await;
        let shown = &snapshot.servers[0];
        let guild = shown.id.clone();
        let general = shown.text_channels().next().unwrap().id.clone();
        let lounge = shown.voice_channels[0].id.clone();
        let bob = snapshot.voice[&lounge][0].user_id.clone();
        Fixture {
            _fake: fake,
            guild,
            general,
            lounge,
            bob,
            discord,
            server,
            snapshot,
        }
    }
}

/// A plain send, as the composer makes it, with `nonce` and `reply`.
fn sending<'a>(nonce: &'a str, reply: Option<&'a str>) -> yapfast_discord::api::Sending<'a> {
    yapfast_discord::api::Sending {
        nonce,
        reply,
        ping_reply: true,
        silent: false,
    }
}
