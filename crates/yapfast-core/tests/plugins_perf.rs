//! What the plugin system costs the application, measured through the same
//! [`PluginState`] the application calls on its own thread, with the real
//! helper behind it.
//!
//! With nothing installed every call must cost next to nothing and no helper may
//! start. With plugins installed, a call on the application's thread must still
//! return at once, since the answer comes back as a notice; this measures how
//! long each call holds that thread, and how long the answer takes to arrive.
//!
//! Measurements, not checks, so they are ignored by default:
//!
//! ```sh
//! nix develop path:./nix --command cargo build --profile fast -p yapfast-plugin-host
//! nix develop path:./nix --command cargo test --profile fast -p yapfast-core \
//!   --features plugins --test plugins_perf -- --ignored --nocapture --test-threads 1
//! ```

#![cfg(feature = "plugins")]

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use yapfast_core::plugins::PluginState;
use yapfast_core::plugins::bridge::HelperState;
use yapfast_core::plugins::state::{Notice, PreSend};
use yapfast_core::plugins::store::{Package, Store};
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::event::{EventEnvelope, EventKind, Network};
use yapfast_plugin_api::message::{DecorateRequest, PluginStatus};

const WAIT: Duration = Duration::from_secs(30);

#[test]
#[ignore = "a measurement; run with --ignored --nocapture"]
fn perf_parent() {
    // SAFETY: this binary's only test, so nothing else reads the environment.
    unsafe { std::env::set_var("YAPFAST_PLUGINS", locate()) };
    println!(
        "load average: {}",
        std::fs::read_to_string("/proc/loadavg")
            .unwrap_or_default()
            .trim()
    );
    let mut rows = Vec::new();

    // Nothing installed.
    let empty = tempfile::tempdir().expect("a directory");
    let mut opens = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        let state = PluginState::open(empty.path(), "0.3.0", Arc::new(|_| {})).expect("opens");
        opens.push(started.elapsed());
        drop(state);
    }
    rows.push(row("open, nothing installed", &mut opens));
    let mut state = PluginState::open(empty.path(), "0.3.0", Arc::new(|_| {})).expect("opens");
    state.start();
    let draft = a_draft("hi");
    rows.push(per_call("subscribes(), nothing installed", 100_000, || {
        std::hint::black_box(state.subscribes(EventKind::MessageCreate));
    }));
    rows.push(per_call("decorates(), nothing installed", 100_000, || {
        std::hint::black_box(state.decorates());
    }));
    let mut state_cell = state;
    rows.push(per_call("pre_send(), nothing installed", 100_000, || {
        std::hint::black_box(state_cell.pre_send(draft.clone()));
    }));
    rows.push(per_call("typing(), nothing installed", 100_000, || {
        std::hint::black_box(state_cell.typing("room-1"));
    }));
    rows.push(per_call("observe(), nothing installed", 100_000, || {
        state_cell.observe(envelope(1));
    }));
    assert!(
        matches!(state_cell.helper(), HelperState::Stopped),
        "nothing installed starts no helper: {:?}",
        state_cell.helper()
    );
    state_cell.shutdown();

    // Three plugins: a send filter, a typing hook, and one that reads every
    // message and draws under each row.
    let directory = tempfile::tempdir().expect("a directory");
    let mut store = Store::open(directory.path()).expect("a store");
    let filter = install(
        &mut store,
        "dev.local.filter",
        "filter.wasm",
        r#"{ "capabilities": ["messages.intercept"], "network": [] }"#,
        None,
    );
    let _typing = install(
        &mut store,
        "dev.local.typing",
        "echo.wasm",
        r#"{ "capabilities": ["typing.intercept"], "network": [] }"#,
        None,
    );
    let reader = install(
        &mut store,
        "dev.local.reader",
        "bad_answer.wasm",
        r#"{ "capabilities": ["messages.read"], "network": [] }"#,
        Some(r#"{ "on": ["event:message_create", "decorate"] }"#),
    );
    drop(store);
    let mut opens = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        let state = PluginState::open(directory.path(), "0.3.0", Arc::new(|_| {})).expect("opens");
        opens.push(started.elapsed());
        drop(state);
    }
    rows.push(row("open, 3 installed", &mut opens));

    let (sender, notices) = mpsc::channel();
    let sender = Mutex::new(sender);
    let wake: Arc<dyn Fn(Notice) + Send + Sync> = Arc::new(move |notice| {
        let _ = sender
            .lock()
            .expect("the sender")
            .send((Instant::now(), notice));
    });
    let started = Instant::now();
    let mut state = PluginState::open(directory.path(), "0.3.0", wake).expect("opens");
    state.start();
    let start_call = started.elapsed();
    wait_active(&state, std::slice::from_ref(&filter));
    println!(
        "start: open+start held the caller {:.1} ms; the send filter was active after {:.1} ms",
        start_call.as_secs_f64() * 1e3,
        started.elapsed().as_secs_f64() * 1e3
    );
    // The reader starts on its first event.
    state.observe(envelope(0));
    wait_active(&state, std::slice::from_ref(&reader));

    let mut held = Vec::new();
    let mut answered = Vec::new();
    for round in 0..600 {
        drain(&notices);
        let asked = Instant::now();
        let PreSend::Held { .. } = state.pre_send(a_draft("hi")) else {
            panic!("a filter holds the send");
        };
        held.push(asked.elapsed());
        let at = wait_for(&notices, |notice| {
            matches!(notice, Notice::SendReady { .. })
        });
        if round >= 100 {
            answered.push(at - asked);
        } else {
            held.pop();
        }
    }
    rows.push(row("pre_send(), held the caller", &mut held));
    rows.push(row("pre_send() to SendReady", &mut answered));

    let mut held = Vec::new();
    let mut answered = Vec::new();
    for round in 0..600 {
        drain(&notices);
        let asked = Instant::now();
        state.typing("room-1").expect("a typing hook holds it");
        held.push(asked.elapsed());
        let at = wait_for(&notices, |notice| matches!(notice, Notice::Typing { .. }));
        if round >= 100 {
            answered.push(at - asked);
        } else {
            held.pop();
        }
    }
    rows.push(row("typing(), held the caller", &mut held));
    rows.push(row("typing() to Notice::Typing", &mut answered));

    let mut held = Vec::new();
    for at in 0..1000 {
        let asked = Instant::now();
        state.observe(envelope(at));
        held.push(asked.elapsed());
    }
    rows.push(row("observe() x1000, held the caller", &mut held));

    // A screenful moving by one row at a time, as a scroll reports it, every
    // 50 ms: what the helper spends on it is the cost of scrolling.
    std::thread::sleep(Duration::from_secs(1));
    let pid = helper_pid();
    let before = usage(pid).1;
    let mut held = Vec::new();
    for top in 0..200 {
        std::thread::sleep(Duration::from_millis(50));
        let screen: Vec<DecorateRequest> = (top..top + 40).map(a_row).collect();
        let asked = Instant::now();
        state.decorate(&screen);
        held.push(asked.elapsed());
    }
    rows.push(row("decorate(40 rows) x200, held", &mut held));
    // Let the helper finish what was sent, then see what it cost.
    std::thread::sleep(Duration::from_secs(1));
    println!(
        "a 10 s scroll, 40 rows a pass every 50 ms: the helper spent {} ticks of 10 ms",
        usage(pid).1 - before
    );
    let before = usage(pid);
    std::thread::sleep(Duration::from_secs(5));
    let after = usage(pid);
    println!(
        "helper idle 5 s, 3 plugins: rss {:.1} MiB, cpu {:.3}%, page shows {:?}",
        after.0 as f64 / 1024.0,
        (after.1 - before.1) as f64 / 5.0,
        state.helper()
    );
    println!("helper cpu over the whole run: {} ticks", after.1);
    state.shutdown();

    println!(
        "| {:<36} | {:>9} | {:>9} | {:>9} | {:>6} |",
        "what", "p50 µs", "p99 µs", "max µs", "n"
    );
    for line in rows {
        println!("{line}");
    }
}

fn row(name: &str, times: &mut [Duration]) -> String {
    times.sort();
    let at = |q: f64| times[((times.len() - 1) as f64 * q).round() as usize];
    format!(
        "| {name:<36} | {:>9.2} | {:>9.2} | {:>9.2} | {:>6} |",
        micros(at(0.5)),
        micros(at(0.99)),
        micros(*times.last().expect("a sample")),
        times.len()
    )
}

fn per_call(name: &str, count: u32, mut call: impl FnMut()) -> String {
    let started = Instant::now();
    for _ in 0..count {
        call();
    }
    let per = started.elapsed() / count;
    format!(
        "| {name:<36} | {:>9.3} | {:>9} | {:>9} | {:>6} |",
        micros(per),
        "mean",
        "",
        count
    )
}

fn micros(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e6
}

fn drain(notices: &Receiver<(Instant, Notice)>) {
    while notices.try_recv().is_ok() {}
}

fn wait_for(notices: &Receiver<(Instant, Notice)>, wanted: impl Fn(&Notice) -> bool) -> Instant {
    let deadline = Instant::now() + WAIT;
    loop {
        let (at, notice) = notices
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("the answer came");
        if wanted(&notice) {
            return at;
        }
    }
}

fn wait_active(state: &PluginState, plugins: &[PluginId]) {
    let started = Instant::now();
    while started.elapsed() < WAIT {
        let statuses = state.statuses();
        if plugins.iter().all(|wanted| {
            statuses
                .iter()
                .any(|(plugin, status)| plugin == wanted && matches!(status, PluginStatus::Active))
        }) {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("never active: {:?} {:?}", state.statuses(), state.off());
}

/// The helper this process started: the one child of ours by that name.
fn helper_pid() -> u32 {
    let me = std::process::id().to_string();
    for entry in std::fs::read_dir("/proc").expect("proc").flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let stat = std::fs::read_to_string(entry.path().join("stat")).unwrap_or_default();
        let Some((name, rest)) = stat.split_once(") ") else {
            continue;
        };
        let parent = rest.split_whitespace().nth(1).unwrap_or_default();
        if parent == me && name.ends_with("(yapfast-plugins") {
            return pid;
        }
    }
    panic!("no helper running");
}

/// Resident KiB and CPU ticks of one process.
fn usage(pid: u32) -> (u64, u64) {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
    let rss = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|value| value.split_whitespace().next()?.parse().ok())
        .unwrap_or(0);
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    let fields: Vec<&str> = stat
        .rsplit_once(')')
        .map(|(_, rest)| rest.split_whitespace().collect())
        .unwrap_or_default();
    let tick = |at: usize| {
        fields
            .get(at)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    (rss, tick(11) + tick(12))
}

fn install(
    store: &mut Store,
    id: &str,
    fixture: &str,
    permissions: &str,
    activation: Option<&str>,
) -> PluginId {
    let module = std::fs::read(fixtures().join(fixture)).expect("the fixture");
    let id = PluginId::parse(id).expect("a plugin id");
    let activation = activation
        .map(|activation| format!(r#""activation": {activation},"#))
        .unwrap_or_default();
    let manifest = format!(
        r#"{{
  "api": 1,
  "id": "{id}",
  "name": "Perf",
  "version": "0.1.0",
  "wasm": "plugin.wasm",
  "limits": {{ "memory_pages": 64 }},
  {activation}
  "permissions": {permissions}
}}
"#
    );
    store
        .install(&Package {
            id: id.clone(),
            source: "https://example.org/ada/perf".to_owned(),
            path: String::new(),
            reference: "main".to_owned(),
            rev: "0".repeat(40),
            manifest,
            module,
            icon: None,
        })
        .expect("it installs");
    store.set_enabled(&id, true).expect("it is on");
    id
}

fn a_draft(body: &str) -> yapfast_plugin_api::hook::Draft {
    yapfast_plugin_api::hook::Draft {
        room: "room-1".to_owned(),
        body: body.to_owned(),
        reply_to: None,
        attachments: Vec::new(),
    }
}

fn a_row(at: usize) -> DecorateRequest {
    DecorateRequest {
        room: "room-1".to_owned(),
        message: format!("message-{at}"),
        author: "2".to_owned(),
        body: "a message of an ordinary length, with a link https://example.com/a/b".to_owned(),
        own: false,
    }
}

fn envelope(at: u64) -> EventEnvelope {
    EventEnvelope {
        kind: EventKind::MessageCreate,
        network: Network::Discord,
        at,
        room: Some("room-1".to_owned()),
        author: Some("2".to_owned()),
        own: false,
        payload: serde_json::json!({
            "message": {
                "id": at.to_string(),
                "content": "a message of an ordinary length, with a link https://example.com/a/b",
                "room": "room-1",
                "author": "2",
            },
        }),
    }
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/plugins")
}

/// What `YAPFAST_PLUGINS` names, or the helper beside this test's profile.
fn locate() -> PathBuf {
    if let Some(named) = std::env::var_os("YAPFAST_PLUGINS") {
        return PathBuf::from(named);
    }
    let exe = std::env::current_exe().expect("a path");
    exe.parent()
        .and_then(Path::parent)
        .expect("a profile directory")
        .join("yapfast-plugins")
}
