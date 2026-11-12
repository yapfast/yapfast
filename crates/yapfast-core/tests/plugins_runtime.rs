//! The parent and the helper together: a plugin installed in a store receives
//! an event, and turning it off ends the helper.
//!
//! This is the runtime's exit criterion, and the only test that runs the
//! real `yapfast-plugins` binary: the bridge's own tests drive a fake session,
//! and the helper's end-to-end tests drive the helper alone. It needs the
//! binary, and a missing one fails the test with the command that builds it:
//! a test that skipped would pass having looked at nothing, which is the one
//! thing this file exists to rule out.
//!
//! ```sh
//! nix develop path:./nix --command cargo build --profile fast -p yapfast-plugin-host
//! nix develop path:./nix --command cargo test --profile fast -p yapfast-core \
//!   --features plugins --test plugins_runtime
//! ```
//!
//! `YAPFAST_PLUGINS` names a helper built another way; without it, the helper
//! the build made in this test binary's own profile directory is used.

#![cfg(feature = "plugins")]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use yapfast_core::plugins::bridge::HelperState;
use yapfast_core::plugins::installing;
use yapfast_core::plugins::page;
use yapfast_core::plugins::store::{Package, Store};
use yapfast_core::plugins::{self, PluginState};
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::event::{EventEnvelope, EventKind, Network};
use yapfast_plugin_api::hook::HookKind;

/// How long the helper is given to start, load a plugin and answer, and how
/// long it is given to end: an unoptimized build compiles the module first.
const START: Duration = Duration::from_secs(30);
const END: Duration = Duration::from_secs(30);
/// What a cold machine may add to a hook's chain budget before the answer lands:
/// two threads, a pipe and the answer being assembled, on an unoptimized profile
/// with every core busy.
const ANSWER_SLACK: Duration = Duration::from_millis(150);
/// The same, for a chain that ran out of time: the cancel being noticed and the
/// message going out as it was written are two more steps than an answer is.
const CANCEL_SLACK: Duration = Duration::from_millis(400);

#[test]
fn a_message_reaches_an_installed_plugin_and_disabling_it_ends_the_helper() {
    let _environment = helper();

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
    let mut state = PluginState::open(directory.path(), "0.3.0", wake).expect("the plugin system");

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
    // And it says that what the plugin drew goes with it, which is the notice the
    // application takes every tree, decoration and badge away on.
    assert!(
        statuses
            .lock()
            .expect("the notices lock")
            .iter()
            .any(|notice| matches!(notice, plugins::state::Notice::SlotsGone(off) if *off == id)),
        "the runtime reports that its trees go with it"
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
    let _environment = helper();

    let directory = tempfile::tempdir().expect("a temporary directory");
    let store = Store::open(directory.path()).expect("a store");
    let mut store = store;
    install_named(&mut store, "dev.local.trap", "trap.wasm");
    let id = PluginId::parse("dev.local.trap").expect("a plugin id");
    store.set_enabled(&id, true).expect("the plugin is on");

    let wake: Arc<dyn Fn(plugins::state::Notice) + Send + Sync> = Arc::new(|_| {});
    let mut state = PluginState::open(directory.path(), "0.3.0", wake).expect("the plugin system");
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

/// A plugin that draws under messages is asked about the rows on screen, and
/// one that draws nothing leaves the row plain: `echo.wasm` exports
/// `yf_decorate` and answers nothing, which is the "no decoration" answer.
#[test]
fn a_decorator_is_asked_and_drawing_nothing_leaves_the_row_plain() {
    let _environment = helper();

    let directory = tempfile::tempdir().expect("a temporary directory");
    let mut store = Store::open(directory.path()).expect("a store");
    install_named(&mut store, "dev.local.decorator", "echo.wasm");
    let id = PluginId::parse("dev.local.decorator").expect("a plugin id");
    store.set_enabled(&id, true).expect("the plugin is on");

    let notices = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&notices);
    let wake: Arc<dyn Fn(plugins::state::Notice) + Send + Sync> =
        Arc::new(move |notice| recorded.lock().unwrap().push(notice));
    let mut state = PluginState::open(directory.path(), "0.3.0", wake).expect("the plugin system");
    assert!(
        state.decorates(),
        "the manifest asks to draw under messages"
    );

    state.decorate(&[yapfast_plugin_api::message::DecorateRequest {
        room: "discord:1:2".to_owned(),
        message: "message-1".to_owned(),
        author: "person-1".to_owned(),
        body: "hi".to_owned(),
        own: false,
    }]);

    // The ask starts the instance, which is the proof the request reached the
    // helper and the export ran: a decoration that draws nothing is silence.
    let started = Instant::now();
    let active = || {
        state
            .statuses()
            .into_iter()
            .any(|(plugin, status)| plugin == id && is_active(&status))
    };
    while started.elapsed() < START && !active() {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        active(),
        "statuses={:?} off={:?}",
        state.statuses(),
        state.off()
    );
    std::thread::sleep(Duration::from_millis(200));
    let drawn = notices
        .lock()
        .unwrap()
        .iter()
        .any(|notice| matches!(notice, plugins::state::Notice::Slot(_)));
    assert!(!drawn, "a plugin that draws nothing shows nothing");
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
  "activation": {{ "on": ["event:message_create", "decorate"] }},
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
    let _environment = helper();

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
    let mut state = PluginState::open(directory.path(), "0.3.0", wake).expect("the plugin system");
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

    // And it stays running: the answer to its activation used to leave its
    // deadline behind, so thirty seconds after loading, a plugin that had been
    // working the whole time was faulted for "not answering" its activation.
    std::thread::sleep(std::time::Duration::from_secs(31));
    assert!(
        running(),
        "still active after its deadlines would have passed: {:?} {:?}",
        state.statuses(),
        state
            .page()
            .installed
            .first()
            .map(|plugin| plugin.log.clone())
    );

    state.set_enabled(&id, false).expect("it is off");
    let off = Instant::now();
    while off.elapsed() < END && !matches!(state.helper(), HelperState::Stopped) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(matches!(state.helper(), HelperState::Stopped));
    state.shutdown();
}

/// A plugin enabled before the app started is idle until something uses it, and
/// using it — `/ping` typed in the composer — is what loads and starts it. That
/// is the lazy activation the manifest declares; a plugin that only wakes when
/// it is switched off and on again in the page is not lazy, it is stuck.
#[test]
fn an_enabled_plugin_answers_its_command_after_a_start() {
    let _environment = helper();

    let directory = tempfile::tempdir().expect("a temporary directory");
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/ping/plugin.wasm");
    let fetched = installing::from_wasm(&sample).expect("the sample reads");
    let id = fetched.id.clone();
    let mut store = Store::open(directory.path()).expect("a store");
    store.install(&fetched.package()).expect("it installs");
    store
        .set_enabled(&id, true)
        .expect("it is on before the app starts");
    drop(store);

    // What starting the app does: nothing is asked for yet, so nothing runs.
    let answers = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&answers);
    let wake: Arc<dyn Fn(plugins::state::Notice) + Send + Sync> = Arc::new(move |notice| {
        recorded.lock().unwrap().push(notice);
    });
    let mut state = PluginState::open(directory.path(), "0.3.0", wake).expect("the plugin system");
    state.start();
    assert!(
        matches!(state.helper(), HelperState::Stopped),
        "a lazy plugin starts no helper"
    );
    assert_eq!(
        state.statuses(),
        vec![(
            id.clone(),
            yapfast_plugin_api::message::PluginStatus::Enabled
        )],
        "enabled and idle, waiting for its trigger"
    );

    // The command the composer reads, and the command being run. It is offered
    // with its plugin's name, because a Discord app may declare `ping` too — and
    // the row is how the person says which one they meant.
    assert!(
        state
            .commands()
            .iter()
            .any(|(_, command)| command.name == "ping"),
        "{:?}",
        state.commands()
    );
    let named = state.commands_named();
    assert!(
        named
            .iter()
            .any(|(_, name, command)| name == "Ping" && command.name == "ping"),
        "the list names the plugin: {named:?}"
    );
    state.run_command(
        &id,
        yapfast_plugin_api::call::CommandInvocation {
            name: "ping".to_owned(),
            options: serde_json::Map::new(),
        },
    );

    let started = Instant::now();
    let answered = || {
        answers
            .lock()
            .unwrap()
            .iter()
            .any(|notice| matches!(notice, plugins::state::Notice::Command { text, .. } if text == "pong"))
    };
    while started.elapsed() < START && !answered() {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        answered(),
        "the command answered from idle in {:?}: {:?} {:?}",
        started.elapsed(),
        state.statuses(),
        state.off()
    );
    state.shutdown();
}

/// A plugin whose manifest asks to run at startup is running once the app has
/// started, without the person touching it.
#[test]
fn a_startup_plugin_runs_when_the_app_starts() {
    let _environment = helper();

    let directory = tempfile::tempdir().expect("a temporary directory");
    let mut store = Store::open(directory.path()).expect("a store");
    let id = PluginId::parse("dev.local.startup").expect("a plugin id");
    install_named(&mut store, "dev.local.startup", "bad_answer.wasm");
    // Its manifest asks to run at startup, which is the one trigger nothing has
    // to use by hand.
    let entry = {
        let lock = store.lock().expect("a lock");
        lock.plugins[&id].clone()
    };
    let mut manifest: serde_json::Value =
        serde_json::from_str(&store.manifest_text(&entry).expect("its manifest"))
            .expect("it is JSON");
    manifest["activation"] = serde_json::json!({ "on": ["startup"] });
    let text = serde_json::to_string(&manifest).expect("it serializes");
    store.uninstall(&id, false).expect("it goes");
    store
        .install(&Package {
            id: id.clone(),
            source: "https://example.org/ada/startup".to_owned(),
            path: String::new(),
            reference: "main".to_owned(),
            rev: "0".repeat(40),
            manifest: text,
            module: std::fs::read(fixture("bad_answer.wasm")).expect("the fixture"),
            icon: None,
        })
        .expect("it installs");
    store.set_enabled(&id, true).expect("it is on");
    drop(store);

    let wake: Arc<dyn Fn(plugins::state::Notice) + Send + Sync> = Arc::new(|_| {});
    let mut state = PluginState::open(directory.path(), "0.3.0", wake).expect("the plugin system");
    // What the application does when a session starts.
    state.start();

    let started = Instant::now();
    while started.elapsed() < START
        && !state
            .statuses()
            .into_iter()
            .any(|(plugin, status)| plugin == id && is_active(&status))
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        state
            .statuses()
            .into_iter()
            .any(|(plugin, status)| plugin == id && is_active(&status)),
        "a startup plugin is running: {:?} {:?}",
        state.statuses(),
        state.off()
    );
    state.shutdown();
}

fn is_active(status: &yapfast_plugin_api::message::PluginStatus) -> bool {
    matches!(status, yapfast_plugin_api::message::PluginStatus::Active)
}

/// Installs one fixture as a plugin that intercepts outgoing messages, which is
/// what puts it in a `message.pre_send` chain, and turns it on.
fn install_interceptor(store: &mut Store, id: &str, fixture: &str) -> PluginId {
    let path = fixture_path(fixture);
    let module = std::fs::read(&path).unwrap_or_else(|error| panic!("{path:?}: {error}"));
    let id = PluginId::parse(id).expect("a plugin id");
    let manifest = format!(
        r#"{{
  "api": 1,
  "id": "{id}",
  "name": "Interceptor",
  "version": "0.1.0",
  "wasm": "plugin.wasm",
  "limits": {{ "memory_pages": 64 }},
  "permissions": {{ "capabilities": ["messages.intercept"], "network": [] }}
}}
"#
    );
    store
        .install(&Package {
            id: id.clone(),
            source: "https://example.org/ada/interceptor".to_owned(),
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

/// The notices a plugin system reports, kept for the test to read: a hook's
/// answer arrives as one rather than as a return value.
type Notices = Arc<std::sync::Mutex<Vec<plugins::state::Notice>>>;

/// Opens the plugin system on a directory and starts it the way a session
/// does, with every notice it reports kept.
fn open_started(directory: &Path) -> (PluginState, Notices) {
    let notices: Notices = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&notices);
    let wake: Arc<dyn Fn(plugins::state::Notice) + Send + Sync> = Arc::new(move |notice| {
        recorded.lock().expect("the notices lock").push(notice);
    });
    let mut state = PluginState::open(directory, "0.3.0", wake).expect("the plugin system");
    state.start();
    (state, notices)
}

/// Waits until every one of `plugins` is running, which is what a send may
/// expect before its chain is asked: a plugin that intercepts starts with the
/// app for exactly that reason.
fn wait_active(state: &PluginState, plugins: &[PluginId]) {
    let started = Instant::now();
    while started.elapsed() < START {
        let running = plugins.iter().all(|wanted| {
            state
                .statuses()
                .into_iter()
                .any(|(plugin, status)| plugin == *wanted && is_active(&status))
        });
        if running {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "the chain never ran: {:?} {:?}",
        state.statuses(),
        state.off()
    );
}

/// Waits for the draft a held send is answered with, and answers how long it
/// took from the moment the send was asked about.
fn wait_send_ready(
    notices: &Notices,
    within: Duration,
) -> (Duration, yapfast_plugin_api::hook::Draft) {
    let started = Instant::now();
    while started.elapsed() < within + START {
        if let Some(notice) = notices
            .lock()
            .expect("the notices lock")
            .iter()
            .find_map(|notice| match notice {
                plugins::state::Notice::SendReady { draft, .. } => Some(draft.clone()),
                _ => None,
            })
        {
            return (started.elapsed(), notice);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!(
        "the send was never answered: {:?}",
        notices.lock().map(|notices| notices.len())
    );
}

/// What a send's chain may take before the runtime gives up on it, plus `slack`.
///
/// Written as the budget [`HookKind::MessagePreSend`] promises rather than as a
/// number of its own, so that the promise moving moves the assertions with it.
fn send_budget(slack: Duration) -> Duration {
    let chain = HookKind::MessagePreSend
        .budget()
        .chain_ms
        .expect("a person waits on a send, so its chain has a budget");
    Duration::from_millis(chain) + slack
}

fn a_draft(body: &str) -> yapfast_plugin_api::hook::Draft {
    yapfast_plugin_api::hook::Draft {
        room: "room-1".to_owned(),
        body: body.to_owned(),
        reply_to: None,
        attachments: Vec::new(),
    }
}

/// M4's first exit criterion: a filter plugin rewrites the outgoing text within
/// its budget, and the rewritten text is what the send goes out with.
#[test]
fn a_filter_rewrites_the_text_a_send_goes_out_with() {
    let _environment = helper();

    let directory = tempfile::tempdir().expect("a temporary directory");
    let mut store = Store::open(directory.path()).expect("a store");
    let plugin = install_interceptor(&mut store, "dev.local.filter", "filter.wasm");
    drop(store);

    let (mut state, notices) = open_started(directory.path());
    wait_active(&state, std::slice::from_ref(&plugin));

    // The send is answered by the chain rather than going out as it was typed,
    // and what the chain answered is what the send carries: the room and the
    // body are both the fixture's, not the composer's.
    let decided = state.pre_send(a_draft("hi"));
    let plugins::state::PreSend::Held { id } = decided else {
        panic!("a chain with an interceptor in it holds the send");
    };
    let (took, answered) = wait_send_ready(&notices, Duration::from_secs(5));
    assert_eq!(answered.body, "filtered", "the chain's text is the send's");
    assert_eq!(answered.room, "filtered-room");
    assert!(
        took < send_budget(ANSWER_SLACK),
        "a filter answers within the chain's budget, and this took {took:?} ({id})"
    );
    state.shutdown();
}

/// M4's other exit criterion: a plugin that never returns from `yf_hook` can't
/// delay a send by more than the chain's 100 ms, and the message goes out as it
/// was written.
///
/// The slack past the chain's 100 ms is [`CANCEL_SLACK`], and it is the machine's
/// — two threads, a pipe and a cancel being noticed — not the promise's: a helper
/// that waited for the plugin's own twenty-five milliseconds per plugin, or for
/// its activation budget of thirty seconds, would be nowhere near this number.
#[test]
fn a_plugin_that_hangs_in_a_hook_costs_the_chain_s_budget_and_not_the_send() {
    let _environment = helper();

    let directory = tempfile::tempdir().expect("a temporary directory");
    let mut store = Store::open(directory.path()).expect("a store");
    let plugin = install_interceptor(&mut store, "dev.local.spin", "spin_hook.wasm");
    drop(store);

    let (mut state, notices) = open_started(directory.path());
    wait_active(&state, std::slice::from_ref(&plugin));

    let plugins::state::PreSend::Held { id } = state.pre_send(a_draft("hi")) else {
        panic!("a chain with an interceptor in it holds the send");
    };
    let (took, answered) = wait_send_ready(&notices, Duration::from_secs(5));
    assert_eq!(
        answered,
        a_draft("hi"),
        "a chain that ran out of time leaves the message as the person wrote it"
    );
    assert!(
        took < send_budget(CANCEL_SLACK),
        "a hung plugin must not hold the send past its budget: {took:?} ({id})"
    );
    state.shutdown();
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

/// One test at a time, for the process's environment: every test here sets
/// `YAPFAST_PLUGINS`, and `setenv` while another thread reads the environment is
/// the data race the standard library marks it as. The path is the same for
/// every test, so the wait is the start of a helper and nothing more.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// Points the process at the helper these tests run, and answers the guard that
/// keeps the environment to this test until it ends: keep it, or another test
/// may set the variable while this one's helper is being started.
fn helper() -> MutexGuard<'static, ()> {
    let environment = ENVIRONMENT.lock().unwrap_or_else(PoisonError::into_inner);
    let path = locate();
    // The helper reads this itself; the parent only has to pass the variable on.
    // SAFETY: the guard above is what every test in this binary takes before it
    // touches the environment, and every test writes the same path, so no test
    // reads an environment another is changing.
    unsafe { std::env::set_var("YAPFAST_PLUGINS", &path) };
    environment
}

/// Where the helper is: what `YAPFAST_PLUGINS` names, or the one this build made
/// in the test binary's own profile directory.
///
/// The profile is the test's own: `cargo test --profile fast` runs it from
/// `target/fast/deps/`, so the helper a build of the workspace makes is in
/// `target/fast/`, whatever `CARGO_TARGET_DIR` or `--target` puts in front of
/// it. A missing helper fails the test rather than skipping it.
fn locate() -> PathBuf {
    if let Some(named) = std::env::var_os("YAPFAST_PLUGINS") {
        let named = PathBuf::from(named);
        assert!(
            named.is_file(),
            "YAPFAST_PLUGINS names {}, which isn't a file",
            named.display()
        );
        return named;
    }
    let name = if cfg!(windows) {
        "yapfast-plugins.exe"
    } else {
        "yapfast-plugins"
    };
    let directory = profile();
    let beside = directory.join(name);
    assert!(beside.is_file(), "{}", missing(&directory));
    beside
}

/// The directory this test binary was built into, which is its profile's.
fn profile() -> PathBuf {
    let exe = std::env::current_exe().expect("a test binary has a path");
    let directory = exe
        .parent()
        .and_then(Path::parent)
        .expect("a target directory above the test binary's deps");
    directory.to_path_buf()
}

/// What to say when the helper isn't where it should be: the command that builds
/// it, with the profile this test needs.
fn missing(directory: &Path) -> String {
    let named = directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Cargo calls the debug profile `debug` on disk and `dev` on the command
    // line; every other profile has the one name.
    let profile = match named.as_str() {
        "debug" => "dev",
        other => other,
    };
    format!(
        "there's no yapfast-plugins helper in {}, and YAPFAST_PLUGINS names none. It is the real \
         helper this test runs, so build it in the dev shell and run the test again:\n\
         \x20 nix develop path:./nix --command cargo build --profile {profile} -p yapfast-plugin-host\n\
         or set YAPFAST_PLUGINS to one you built another way. A test that ran nothing doesn't pass.",
        directory.display()
    )
}

fn fixture_path(name: &str) -> PathBuf {
    fixture(name)
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testing/plugins")
        .join(name)
}
