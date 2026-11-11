//! The parent and the helper together: a plugin installed in a store receives
//! an event, and turning it off ends the helper.
//!
//! This is M1's exit criterion from `PLAN.md`, and the only test that runs the
//! real `yapfast-plugins` binary: the bridge's own tests drive a fake session,
//! and the helper's end-to-end tests drive the helper alone. It needs the
//! binary, which `YAPFAST_PLUGINS` names; without one it says so and passes,
//! because a test that silently tested nothing would be worse than none.
//!
//! ```sh
//! cargo build --profile fast -p yapfast-plugin-host
//! YAPFAST_PLUGINS=$PWD/target/fast/yapfast-plugins \
//!   cargo test --profile fast -p yapfast-core --features plugins --test plugins_runtime
//! ```

#![cfg(feature = "plugins")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use yapfast_core::plugins::bridge::HelperState;
use yapfast_core::plugins::installing;
use yapfast_core::plugins::page;
use yapfast_core::plugins::store::{Package, Store};
use yapfast_core::plugins::{self, PluginState};
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::event::{EventEnvelope, EventKind, Network};

/// How long the helper is given to start, load a plugin and answer, and how
/// long it is given to end: an unoptimized build compiles the module first.
const START: Duration = Duration::from_secs(30);
const END: Duration = Duration::from_secs(30);

#[test]
fn a_message_reaches_an_installed_plugin_and_disabling_it_ends_the_helper() {
    let Some(helper) = helper() else {
        eprintln!("no yapfast-plugins beside the build and no YAPFAST_PLUGINS; skipped");
        return;
    };
    // The helper reads this itself; the parent only has to pass the variable on.
    // SAFETY: this is the only test in this binary, and nothing else reads the
    // environment while it runs.
    unsafe { std::env::set_var("YAPFAST_PLUGINS", &helper) };

    let directory = tempfile::tempdir().expect("a temporary directory");
    let manifest = manifest();
    let wasm = wasm();
    let mut store = Store::open(directory.path()).expect("a store");
    let id = PluginId::parse("dev.local.echo").expect("a plugin id");
    install(&mut store, &manifest, &wasm);
    store.set_enabled(&id, true).expect("the plugin is on");

    let statuses = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&statuses);
    let wake: Arc<dyn Fn(plugins::state::Notice) + Send + Sync> =
        Arc::new(move |notice| recorded.lock().unwrap().push(notice));
    let mut state = PluginState::open(directory.path(), wake).expect("the plugin system");

    assert!(
        !matches!(state.helper(), HelperState::Running { .. }),
        "no plugin has asked for anything yet"
    );

    assert!(
        state.subscribes(EventKind::MessageCreate),
        "the manifest subscribes, and `messages.read` is what lets it"
    );
    state.observe(envelope("hello"));

    let started = Instant::now();
    let running = || {
        matches!(state.helper(), HelperState::Running { .. })
            && state
                .statuses()
                .into_iter()
                .any(|(plugin, status)| plugin == id && is_active(&status))
    };
    while started.elapsed() < START && !running() {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        running(),
        "helper={:?} statuses={:?} off={:?}",
        state.helper(),
        state.statuses(),
        state.off()
    );

    state.set_enabled(&id, false).expect("the plugin is off");
    let disabled = Instant::now();
    while disabled.elapsed() < END && !matches!(state.helper(), HelperState::Stopped) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let processes = std::process::Command::new("pgrep")
        .args(["-fa", "yapfast-plugins"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    assert!(
        matches!(state.helper(), HelperState::Stopped),
        "helper={:?} statuses={:?} off={:?} processes={processes}",
        state.helper(),
        state.statuses(),
        state.off()
    );
    state.shutdown();
}

/// A plugin that traps in `yf_activate` is **faulted**, not suspended: it stays
/// on and the runtime tries it again on its next trigger, and both halves of the
/// page say why — the row carries the runtime's own sentence and the line it
/// logged is the same sentence. Calling this "suspended" was wrong and told a
/// person their plugin was off when it was still on.
#[test]
fn a_plugin_that_traps_says_why_in_its_status_and_its_log() {
    let Some(helper) = helper() else {
        eprintln!("no yapfast-plugins beside the build and no YAPFAST_PLUGINS; skipped");
        return;
    };
    unsafe { std::env::set_var("YAPFAST_PLUGINS", &helper) };

    let directory = tempfile::tempdir().expect("a temporary directory");
    let store = Store::open(directory.path()).expect("a store");
    let mut store = store;
    install_named(&mut store, "dev.local.trap", "trap.wasm");
    let id = PluginId::parse("dev.local.trap").expect("a plugin id");
    store.set_enabled(&id, true).expect("the plugin is on");

    let wake: Arc<dyn Fn(plugins::state::Notice) + Send + Sync> = Arc::new(|_| {});
    let mut state = PluginState::open(directory.path(), wake).expect("the plugin system");
    state.set_enabled(&id, true).expect("it is woken");
    state.observe(envelope("hello"));

    let started = Instant::now();
    let suspended = || {
        state
            .page()
            .installed
            .first()
            .is_some_and(|plugin| matches!(plugin.status, page::Status::Suspended(_)))
    };
    while started.elapsed() < END && !suspended() {
        std::thread::sleep(Duration::from_millis(50));
    }
    let view = state.page();
    let plugin = view.installed.first().expect("it is listed");
    let status = plugin.status.text();
    assert!(
        matches!(plugin.status, page::Status::Faulted(_)),
        "it faults, and stays on to try again: {status}"
    );
    assert!(
        status.starts_with("Faulted: ") && status.len() > "Faulted: ".len(),
        "the status says what happened: {status}"
    );
    let logged = plugin.log.last().expect("the reason is in its log");
    assert!(logged.error, "as an error: {logged:?}");
    assert!(
        status.contains(logged.text.trim_start_matches("yapfast: ")),
        "the row and the log agree: {status} / {:?}",
        logged.text
    );
    state.shutdown();
}

/// Installs one fixture module as a plugin of its own.
fn install_named(store: &mut Store, id: &str, fixture: &str) {
    let path = fixture_path(fixture);
    let wasm = std::fs::read(&path).expect("the fixture");
    let manifest = format!(
        r#"{{
  "api": 1,
  "id": "{id}",
  "name": "Trap",
  "version": "0.1.0",
  "wasm": "plugin.wasm",
  "limits": {{ "memory_pages": 64 }},
  "activation": {{ "on": ["event:message_create"] }},
  "permissions": {{ "capabilities": ["messages.read"], "network": [] }}
}}
"#
    );
    store
        .install(&Package {
            id: PluginId::parse(id).expect("a plugin id"),
            source: "https://example.org/ada/trap".to_owned(),
            path: String::new(),
            reference: "main".to_owned(),
            rev: "0".repeat(40),
            manifest,
            module: wasm,
            icon: None,
        })
        .expect("it installs");
}

/// The repository's own sample plugin, installed the way the file picker
/// installs one and run through the real helper: it declares `/ping`, the
/// helper loads it, and the composer would offer the command.
///
/// It is the smallest plugin that does something, so a break here is a break in
/// the path every plugin follows: manifest beside the module, store, helper,
/// guest, back to a command the person can type.
#[test]
fn the_sample_ping_plugin_loads_and_declares_its_command() {
    let Some(helper) = helper() else {
        eprintln!("no yapfast-plugins beside the build and no YAPFAST_PLUGINS; skipped");
        return;
    };
    unsafe { std::env::set_var("YAPFAST_PLUGINS", &helper) };

    let directory = tempfile::tempdir().expect("a temporary directory");
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/ping/plugin.wasm");
    let fetched = installing::from_wasm(&sample).expect("the sample reads");
    assert_eq!(fetched.id.to_string(), "com.example.ping");
    assert_eq!(
        fetched.review().capabilities,
        Vec::<String>::new(),
        "it asks for nothing: a command's answer is a line, not a message"
    );

    let mut store = Store::open(directory.path()).expect("a store");
    store.install(&fetched.package()).expect("it installs");
    let id = fetched.id.clone();
    store.set_enabled(&id, true).expect("it is on");

    let wake: Arc<dyn Fn(plugins::state::Notice) + Send + Sync> = Arc::new(|_| {});
    let mut state = PluginState::open(directory.path(), wake).expect("the plugin system");
    let started = Instant::now();
    state.set_enabled(&id, true).expect("it wakes");
    let running = || {
        state
            .statuses()
            .into_iter()
            .any(|(plugin, status)| plugin == id && is_active(&status))
    };
    while started.elapsed() < START && !running() {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        running(),
        "loaded and active in {:?}: {:?} {:?}",
        started.elapsed(),
        state.statuses(),
        state.off()
    );
    eprintln!(
        "ping loaded in {:?} (helper {:?})",
        started.elapsed(),
        state.helper()
    );

    // What the composer's `/` parser reads: the command its manifest declares.
    let commands = state.commands();
    assert!(
        commands.iter().any(|(_, command)| command.name == "ping"),
        "{commands:?}"
    );

    state.set_enabled(&id, false).expect("it is off");
    let off = Instant::now();
    while off.elapsed() < END && !matches!(state.helper(), HelperState::Stopped) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(matches!(state.helper(), HelperState::Stopped));
    state.shutdown();
}

fn is_active(status: &yapfast_plugin_api::message::PluginStatus) -> bool {
    matches!(status, yapfast_plugin_api::message::PluginStatus::Active)
}

/// A plugin that only subscribes to messages, and hand it one.
///
/// `bad_answer.wasm` is the fixture whose `yf_event` returns 0 and answers
/// nothing, which is what the ABI says an event call answers. `echo.wasm` copies
/// its batch back instead, and the runtime refuses that as an answer an event
/// doesn't take; `bad_answer` misbehaves in its hook, which this test never runs.
fn manifest() -> String {
    r#"{
  "api": 1,
  "id": "dev.local.echo",
  "name": "Echo",
  "version": "0.1.0",
  "description": "Answers every message with what it read.",
  "license": "MIT",
  "wasm": "plugin.wasm",
  "limits": { "memory_pages": 64 },
  "activation": { "on": ["event:message_create"] },
  "permissions": { "capabilities": ["messages.read"], "network": [] }
}
"#
    .to_owned()
}

fn wasm() -> Vec<u8> {
    let path = fixture("bad_answer.wasm");
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn install(store: &mut Store, manifest: &str, wasm: &[u8]) {
    store
        .install(&Package {
            id: PluginId::parse("dev.local.echo").expect("a plugin id"),
            source: "https://example.org/ada/echo".to_owned(),
            path: String::new(),
            reference: "main".to_owned(),
            rev: "0".repeat(40),
            manifest: manifest.to_owned(),
            module: wasm.to_vec(),
            icon: None,
        })
        .expect("the plugin is written into the store");
}

fn envelope(body: &str) -> EventEnvelope {
    EventEnvelope {
        kind: EventKind::MessageCreate,
        network: Network::Discord,
        at: 0,
        room: Some("1".to_owned()),
        author: Some("2".to_owned()),
        own: false,
        payload: serde_json::json!({
            "message": { "id": "3", "content": body, "room": "1", "author": "2" },
        }),
    }
}

/// The helper's path: what `YAPFAST_PLUGINS` names, or the one this build made
/// beside the tests.
fn helper() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("YAPFAST_PLUGINS") {
        let named = PathBuf::from(named);
        if named.is_file() {
            return Some(named);
        }
    }
    let beside = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(profile())
        .join("yapfast-plugins");
    beside.is_file().then_some(beside)
}

fn profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "fast"
    }
}

fn fixture_path(name: &str) -> PathBuf {
    fixture(name)
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testing/plugins")
        .join(name)
}
