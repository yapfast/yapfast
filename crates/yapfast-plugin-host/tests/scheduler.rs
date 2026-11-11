//! What the scheduler guarantees, driven by a fake backend.
//!
//! The real backend runs WebAssembly; this one records what it was asked to do
//! and can block, sleep or trap on cue, which is what makes ordering and
//! deadlines observable without a guest. Everything here goes through the same
//! public surface the helper uses: [`Message`] in, [`FromHelper`] frames out.

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{CommandInvocation, CommandOutcome};
use yapfast_plugin_api::event::{EventEnvelope, EventKind, Network};
use yapfast_plugin_api::frame::{Frame, FrameKind, read_frame};
use yapfast_plugin_api::hook::{Draft, HookKind, HookRequest, HookSubject};
use yapfast_plugin_api::message::{Fault, FaultKind, FromHelper, PluginStatus};

use yapfast_plugin_host::backend::{Backend, CancelToken, HookRun, LoadSpec};
use yapfast_plugin_host::pipe::{Pipe, Signal};
use yapfast_plugin_host::scheduler::{Config, Held, Message, Scheduler};

/// How long a test waits for the scheduler before calling it stuck.
const PATIENCE: Duration = Duration::from_secs(5);

#[test]
fn interactive_work_goes_before_background_work() {
    let harness = Harness::new(config(1));
    let a = plugin("a");
    let b = plugin("b");
    harness.load(&a, 256);
    harness.load(&b, 256);
    harness.live(&a, 101);
    harness.live(&b, 102);

    // Hold the only worker inside a call, so what is queued next is decided
    // with a full mailbox in front of it.
    harness.hold(true);
    harness.send(Message::Command {
        id: 1,
        plugin: a.clone(),
        command: command("blocker"),
    });
    harness.wait_log("command:com.example.a:blocker");

    harness.send(Message::Event {
        id: 2,
        plugin: b.clone(),
        events: vec![event("e1")],
    });
    harness.send(Message::Event {
        id: 3,
        plugin: b.clone(),
        events: vec![event("e2")],
    });
    harness.send(Message::Command {
        id: 4,
        plugin: a.clone(),
        command: command("cmd"),
    });
    harness.hold(false);

    harness.wait_replies(6);
    let order = harness.log();
    let command = order.iter().position(|call| call.ends_with(":cmd"));
    let event = order.iter().position(|call| call.ends_with(":e1"));
    assert!(
        command < event,
        "the command should run before the queued events: {order:?}"
    );
    harness.finish();
}

#[test]
fn one_plugins_work_keeps_its_order() {
    let harness = Harness::new(config(1));
    let a = plugin("a");
    harness.load(&a, 256);
    harness.live(&a, 101);

    harness.hold(true);
    harness.send(Message::Command {
        id: 1,
        plugin: a.clone(),
        command: command("blocker"),
    });
    harness.wait_log("command:com.example.a:blocker");
    for (id, label) in [(2, "e1"), (3, "e2"), (4, "e3")] {
        harness.send(Message::Event {
            id,
            plugin: a.clone(),
            events: vec![event(label)],
        });
    }
    harness.hold(false);

    harness.wait_replies(5);
    let order = harness.log();
    let first = order.iter().position(|call| call.ends_with(":e1"));
    let second = order.iter().position(|call| call.ends_with(":e2"));
    let third = order.iter().position(|call| call.ends_with(":e3"));
    assert!(first < second && second < third, "{order:?}");
    harness.finish();
}

#[test]
fn two_plugins_interleave_on_both_workers() {
    let harness = Harness::new(config(2));
    let a = plugin("a");
    let b = plugin("b");
    harness.load(&a, 256);
    harness.load(&b, 256);
    harness.live(&a, 101);
    harness.live(&b, 102);

    for round in 0..5 {
        harness.send(Message::Command {
            id: 10 + round,
            plugin: a.clone(),
            command: command(&format!("a{round}")),
        });
        harness.send(Message::Command {
            id: 20 + round,
            plugin: b.clone(),
            command: command(&format!("b{round}")),
        });
    }
    harness.wait_replies(12);

    let log = harness.log();
    let calls: Vec<&String> = log
        .iter()
        .filter(|call| call.starts_with("command:"))
        .collect();
    assert_eq!(calls.len(), 10, "every command should run: {calls:?}");

    // Each plugin's own calls keep their order.
    let for_a: Vec<&str> = calls
        .iter()
        .filter(|call| call.starts_with("command:com.example.a"))
        .map(|call| call.rsplit(':').next().unwrap_or_default())
        .collect();
    assert_eq!(for_a, ["a0", "a1", "a2", "a3", "a4"]);

    // And the two queues make progress together rather than one draining
    // first, which is what the second worker is for.
    let threads = harness.threads_for("command:");
    assert_eq!(
        threads.len(),
        2,
        "both workers should have run: {threads:?}"
    );
    let first_b = calls
        .iter()
        .position(|call| call.starts_with("command:com.example.b"))
        .expect("b ran");
    let last_a = calls
        .iter()
        .rposition(|call| call.starts_with("command:com.example.a"))
        .expect("a ran");
    assert!(first_b < last_a, "the queues interleave: {calls:?}");
    harness.finish();
}

#[test]
fn a_call_that_runs_late_is_cancelled_and_faulted() {
    let harness = Harness::new(config(1));
    let a = plugin("a");
    harness.load(&a, 256);
    // A typing hook has 25 ms for its chain; this call takes three times that.
    harness.sleep(Duration::from_millis(80));

    harness.send(Message::Hook {
        id: 7,
        chain: vec![a.clone()],
        request: HookRequest {
            hook: HookKind::TypingPreSend,
            subject: HookSubject::Draft(Draft {
                room: "room-1".into(),
                body: "hi".into(),
                reply_to: None,
                attachments: Vec::new(),
            }),
        },
    });

    let reports = harness.wait_until(|reports| {
        reports
            .iter()
            .any(|report| matches!(report, FromHelper::HookDone { .. }))
    });
    assert!(
        reports.iter().any(|report| matches!(
            report,
            FromHelper::Fault {
                fault: Fault {
                    kind: FaultKind::Deadline,
                    ..
                },
                ..
            }
        )),
        "the deadline should be reported as a fault: {reports:?}"
    );
    harness.wait_log("cancel:com.example.a");
    assert!(
        reports.iter().any(|report| matches!(
            report,
            FromHelper::Status {
                status: PluginStatus::Faulted { .. },
                ..
            }
        )),
        "a late call discards the instance: {reports:?}"
    );
    harness.finish();
}

#[test]
fn three_traps_in_a_window_suspend_a_plugin() {
    let harness = Harness::new(config(1));
    let a = plugin("a");
    harness.load(&a, 256);
    harness.trap("command:");

    for id in 1..=3 {
        harness.send(Message::Command {
            id,
            plugin: a.clone(),
            command: command("boom"),
        });
    }

    let reports = harness.wait_until(|reports| {
        reports.iter().any(|report| {
            matches!(
                report,
                FromHelper::Status {
                    status: PluginStatus::Suspended { .. },
                    ..
                }
            )
        })
    });
    let faults = reports
        .iter()
        .filter(|report| matches!(report, FromHelper::Fault { .. }))
        .count();
    assert_eq!(faults, 3, "each trap is a fault: {reports:?}");

    // Suspended for the session: a fourth command is answered, not run.
    harness.send(Message::Command {
        id: 4,
        plugin: a.clone(),
        command: command("again"),
    });
    let reports = harness.wait_until(|reports| {
        reports.iter().any(|report| {
            matches!(
                report,
                FromHelper::CommandDone {
                    outcome: CommandOutcome::Failed { .. },
                    ..
                }
            )
        })
    });
    assert!(reports.iter().any(|report| matches!(
        report,
        FromHelper::CommandDone {
            outcome: CommandOutcome::Failed { .. },
            ..
        }
    )));
    harness.finish();
}

#[test]
fn idle_eviction_leaves_an_instance_holding_a_timer() {
    let harness = Harness::new(Config {
        workers: 1,
        idle_after: Duration::from_millis(40),
    });
    let a = plugin("a");
    let b = plugin("b");
    harness.load(&a, 256);
    harness.load(&b, 256);
    harness.live(&a, 1);
    harness.live(&b, 2);
    harness.scheduler.hold(&b, Held::Timer);

    let reports = harness.wait_until(|reports| {
        reports.iter().any(|report| {
            matches!(
                report,
                FromHelper::Status {
                    plugin,
                    status: PluginStatus::Enabled,
                } if *plugin == a
            )
        })
    });
    assert!(
        !reports.iter().any(|report| matches!(
            report,
            FromHelper::Status {
                plugin,
                status: PluginStatus::Enabled,
            } if *plugin == b
        )),
        "a plugin holding a timer is not idle: {reports:?}"
    );
    let log = harness.log();
    assert!(log.contains(&"evict:com.example.a".to_string()), "{log:?}");
    assert!(!log.contains(&"evict:com.example.b".to_string()), "{log:?}");
    harness.finish();
}

#[test]
fn going_over_the_memory_budget_evicts_the_least_recently_used() {
    let harness = Harness::new(config(1));
    let plugins: Vec<PluginId> = ["a", "b", "c", "d"]
        .iter()
        .map(|name| plugin(name))
        .collect();
    // Four instances of 1024 pages are 256 MiB, which is the helper's whole
    // budget: the fifth has to displace one of them.
    for (index, plugin) in plugins.iter().enumerate() {
        harness.load(plugin, 1024);
        harness.live(plugin, index as u64 + 1);
    }
    let e = plugin("e");
    harness.load(&e, 1024);
    harness.activate(&e, 5);

    harness.wait_until(|reports| {
        reports.iter().any(|report| {
            matches!(
                report,
                FromHelper::Activated { plugin, .. } if *plugin == e
            )
        })
    });
    let log = harness.log();
    assert!(
        log.contains(&"evict:com.example.a".to_string()),
        "the least recently used instance should go first: {log:?}"
    );
    assert!(
        !log.contains(&"evict:com.example.b".to_string()),
        "only the one instance was needed: {log:?}"
    );
    harness.finish();
}

// What a test drives, and what it watches.

/// A worker count, or the machine's own shape when zero.
fn config(workers: usize) -> Config {
    Config {
        workers,
        idle_after: Duration::from_millis(yapfast_plugin_api::limits::IDLE_EVICTION_MS),
    }
}

fn plugin(name: &str) -> PluginId {
    PluginId::parse(&format!("com.example.{name}")).expect("a valid id")
}

fn command(name: &str) -> CommandInvocation {
    CommandInvocation {
        name: name.into(),
        options: serde_json::Map::new(),
    }
}

fn event(label: &str) -> EventEnvelope {
    EventEnvelope {
        kind: EventKind::MessageCreate,
        network: Network::Discord,
        at: 0,
        room: None,
        author: None,
        own: false,
        payload: serde_json::json!({ "label": label }),
    }
}

/// The scheduler under test, with the frames it writes in hand.
struct Harness {
    pipe: Pipe,
    scheduler: Scheduler<Fake>,
    captured: Captured,
    fake: Arc<FakeLog>,
}

impl Harness {
    fn new(config: Config) -> Self {
        let captured = Captured::default();
        // Nothing arrives from a parent in these tests; only what the helper
        // writes matters.
        let pipe = Pipe::new(io::empty(), captured.clone()).expect("the pipe starts");
        let fake = Arc::new(FakeLog::default());
        let scheduler = Scheduler::start(
            Fake {
                calls: Arc::clone(&fake),
            },
            pipe.outbox(),
            pipe.waker(),
            config,
        )
        .expect("the workers start");
        Harness {
            pipe,
            scheduler,
            captured,
            fake,
        }
    }

    fn send(&self, message: Message) {
        self.scheduler.send(message);
    }

    fn load(&self, plugin: &PluginId, memory_pages: u32) {
        self.send(Message::Load {
            spec: LoadSpec {
                plugin: plugin.clone(),
                wasm: PathBuf::from("/dev/null"),
                memory_pages,
                wasi: false,
            },
        });
        self.wait_log(&format!("load:{plugin}"));
    }

    fn activate(&self, plugin: &PluginId, id: u64) {
        self.send(Message::Activate {
            id,
            plugin: plugin.clone(),
        });
    }

    /// Activates a loaded plugin and waits until its instance is up.
    fn live(&self, plugin: &PluginId, id: u64) {
        self.activate(plugin, id);
        self.wait_for_reply(id);
    }

    fn hold(&self, holding: bool) {
        let mut h = lock(&self.fake.holding);
        *h = holding;
        if !holding {
            self.fake.released.notify_all();
        }
    }

    fn sleep(&self, duration: Duration) {
        *lock(&self.fake.sleep) = duration;
    }

    fn trap(&self, call: &str) {
        *lock(&self.fake.traps) = Some(call.to_owned());
    }

    /// Every frame written so far, in order.
    fn frames(&self) -> Vec<Frame> {
        let bytes = lock(&self.captured.0).clone();
        let mut cursor = &bytes[..];
        let mut frames = Vec::new();
        while let Ok(Some(frame)) = read_frame(&mut cursor) {
            frames.push(frame);
        }
        frames
    }

    /// Every report written so far, decoded.
    fn reports(&self) -> Vec<FromHelper> {
        self.frames()
            .into_iter()
            .map(|frame| serde_json::from_slice(&frame.payload).expect("a report decodes"))
            .collect()
    }

    /// Waits until `count` replies have been written: answers to requests, which
    /// is what a test synchronizes on.
    fn wait_replies(&self, count: usize) {
        self.wait_until(|_reports| self.reply_count() >= count);
    }

    fn reply_count(&self) -> usize {
        self.frames()
            .iter()
            .filter(|frame| frame.kind == FrameKind::Reply)
            .count()
    }

    fn wait_for_reply(&self, id: u64) {
        self.wait_until(|_reports| self.frames().iter().any(|frame| frame.id == id));
    }

    fn wait_until(&self, ready: impl Fn(&[FromHelper]) -> bool) -> Vec<FromHelper> {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let reports = self.reports();
            if ready(&reports) {
                return reports;
            }
            assert!(
                Instant::now() < deadline,
                "the scheduler never reported it: {reports:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    /// The calls the fake backend was given, in order.
    fn log(&self) -> Vec<String> {
        lock(&self.fake.calls)
            .iter()
            .map(|entry| entry.call.clone())
            .collect()
    }

    /// The worker threads that made the calls named by `prefix`.
    fn threads_for(&self, prefix: &str) -> Vec<String> {
        let mut threads: Vec<String> = lock(&self.fake.calls)
            .iter()
            .filter(|entry| entry.call.starts_with(prefix))
            .map(|entry| entry.thread.clone())
            .collect();
        threads.sort();
        threads.dedup();
        threads
    }

    fn wait_log(&self, needle: &str) {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if self.log().iter().any(|call| call.contains(needle)) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the backend was never asked for `{needle}`: {:?}",
                self.log()
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn finish(self) {
        let Harness {
            pipe,
            mut scheduler,
            ..
        } = self;
        scheduler.shutdown();
        // The pipe stops writing once nothing holds a sender, so the scheduler
        // goes before it.
        drop(scheduler);
        pipe.finish(None);
    }
}

/// Collects what the pipe's writer produced, so a test can read frames.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        lock(&self.0).extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One call the fake was given, and the worker that made it.
struct Entry {
    thread: String,
    call: String,
}

/// What the fake backend records, and what it is told to do.
#[derive(Default)]
struct FakeLog {
    calls: Mutex<Vec<Entry>>,
    /// While set, every call waits here, holding its worker.
    holding: Mutex<bool>,
    released: Condvar,
    /// The call prefix that traps.
    traps: Mutex<Option<String>>,
    /// How long each call takes, to run past a deadline.
    sleep: Mutex<Duration>,
}

impl FakeLog {
    fn enter(&self, call: String) -> Result<(), Fault> {
        let thread = thread::current().name().unwrap_or("a worker").to_owned();
        lock(&self.calls).push(Entry {
            thread,
            call: call.clone(),
        });
        let sleep = *lock(&self.sleep);
        if !sleep.is_zero() {
            thread::sleep(sleep);
        }
        {
            let mut holding = lock(&self.holding);
            while *holding {
                holding = self
                    .released
                    .wait(holding)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        }
        let traps = lock(&self.traps).clone();
        match traps {
            Some(trap) if call.starts_with(&trap) => Err(Fault {
                kind: FaultKind::Trap,
                reason: format!("`{call}` trapped"),
            }),
            _ => Ok(()),
        }
    }

    fn note(&self, call: String) {
        let thread = thread::current().name().unwrap_or("a worker").to_owned();
        lock(&self.calls).push(Entry { thread, call });
    }
}

/// The runtime, replaced by something that only remembers.
struct Fake {
    calls: Arc<FakeLog>,
}

impl Backend for Fake {
    fn load(&self, spec: LoadSpec) -> Result<(), Fault> {
        self.calls.enter(format!("load:{}", spec.plugin))
    }

    fn unload(&self, plugin: &PluginId) {
        self.calls.note(format!("unload:{plugin}"));
    }

    fn activate(&self, plugin: &PluginId) -> Result<Option<String>, Fault> {
        self.calls.enter(format!("activate:{plugin}"))?;
        Ok(None)
    }

    fn deactivate(&self, plugin: &PluginId) -> Result<Option<String>, Fault> {
        self.calls.enter(format!("deactivate:{plugin}"))?;
        Ok(None)
    }

    fn event(&self, plugin: &PluginId, events: &[EventEnvelope]) -> Result<(), Fault> {
        let label = events
            .first()
            .map(|event| event.payload["label"].as_str().unwrap_or("?").to_owned())
            .unwrap_or_else(|| "none".to_owned());
        self.calls.enter(format!("event:{plugin}:{label}"))
    }

    fn hook(&self, chain: &[PluginId], request: &HookRequest) -> HookRun {
        let names: Vec<&str> = chain.iter().map(|plugin| plugin.as_str()).collect();
        let result = self.calls.enter(format!("hook:{}", names.join(",")));
        if let Err(fault) = result {
            return HookRun {
                outcome: None,
                failures: vec![(chain[0].clone(), fault)],
            };
        }
        let _ = request;
        HookRun::unchanged()
    }

    fn command(
        &self,
        plugin: &PluginId,
        command: &CommandInvocation,
    ) -> Result<CommandOutcome, Fault> {
        self.calls
            .enter(format!("command:{plugin}:{}", command.name))?;
        Ok(CommandOutcome::None)
    }

    fn answer(
        &self,
        plugin: &PluginId,
        call: u64,
        _answer: &yapfast_plugin_api::call::HostReturn,
    ) -> Result<(), Fault> {
        self.calls.enter(format!("answer:{plugin}:{call}"))
    }

    fn evict(&self, plugin: &PluginId) {
        self.calls.note(format!("evict:{plugin}"));
    }

    fn memory_pages(&self, _plugin: &PluginId) -> Option<u32> {
        None
    }

    fn cancel_handle(&self, plugin: &PluginId) -> Option<CancelToken> {
        let plugin = plugin.clone();
        let calls = Arc::clone(&self.calls);
        Some(Arc::new(move || calls.note(format!("cancel:{plugin}"))))
    }
}

/// A reader that never returns, so the pipe's only signal is the scheduler's.
struct Silent;

impl io::Read for Silent {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        loop {
            thread::park();
        }
    }
}

/// The helper is disposable: with nothing left to run it says so, and a fresh
/// one starts on the next activation.
///
/// The last instance's unload is itself a task holding that plugin busily, so
/// the notice has to be given after the worker lets go, not from inside it.
#[test]
fn the_helper_finishes_once_the_last_instance_is_gone() {
    let pipe = Pipe::new(Silent, io::sink()).expect("the pipe starts");
    let fake = Arc::new(FakeLog::default());
    let scheduler = Scheduler::start(
        Fake {
            calls: Arc::clone(&fake),
        },
        pipe.outbox(),
        pipe.waker(),
        config(1),
    )
    .expect("the workers start");

    let plugin = plugin("last");
    scheduler.send(Message::Load {
        spec: LoadSpec {
            plugin: plugin.clone(),
            wasm: PathBuf::from("/dev/null"),
            memory_pages: 256,
            wasi: false,
        },
    });
    scheduler.send(Message::Activate {
        id: 1,
        plugin: plugin.clone(),
    });
    let mut waited = Instant::now();
    while !lock(&fake.calls)
        .iter()
        .any(|entry| entry.call == "activate:com.example.last")
    {
        assert!(waited.elapsed() < PATIENCE, "the plugin never activated");
        thread::sleep(Duration::from_millis(10));
        waited = Instant::now();
    }
    scheduler.send(Message::Unload {
        plugin: plugin.clone(),
    });

    let (sent, arrived) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = sent.send(pipe.recv());
    });
    match arrived.recv_timeout(PATIENCE) {
        Ok(Signal::Finished) => {}
        Ok(_) => panic!("the helper kept a reason to live after its last instance"),
        Err(_) => panic!("the helper never said it was finished"),
    }
}

/// A background call must not hold the path an interactive send needs.
///
/// The two lanes are only real if the runtime lets two plugins run at once: a
/// locked backend, one call at a time, turns them into one queue whatever the
/// mailboxes say. Two calls of 200 ms each are 400 ms serialised and 200 ms
/// overlapped; the margin is wide enough for a busy machine and narrow enough to
/// fail if the lock comes back.
#[test]
fn a_background_call_does_not_hold_an_interactive_send() {
    let harness = Harness::new(config(2));
    let a = plugin("a");
    let b = plugin("b");
    harness.load(&a, 256);
    harness.load(&b, 256);
    harness.live(&a, 101);
    harness.live(&b, 102);
    harness.sleep(Duration::from_millis(200));

    let started = Instant::now();
    harness.send(Message::Event {
        id: 1,
        plugin: a.clone(),
        events: vec![event("e1")],
    });
    harness.send(Message::Command {
        id: 2,
        plugin: b.clone(),
        command: command("cmd"),
    });
    harness.wait_replies(2);
    let took = started.elapsed();

    assert!(
        took < Duration::from_millis(350),
        "the send waited for the event, which serialises the lanes: {took:?}"
    );
}
