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
use yapfast_plugin_api::call::{CommandInvocation, CommandOutcome, HostReturn};
use yapfast_plugin_api::event::{EventEnvelope, EventKind, Network, TopicEvent};
use yapfast_plugin_api::frame::{Frame, FrameKind, read_frame};
use yapfast_plugin_api::hook::{Draft, HookKind, HookOutcome, HookRequest, HookSubject};
use yapfast_plugin_api::message::{Fault, FaultKind, FromHelper, PluginStatus};

use yapfast_plugin_host::backend::{Backend, CancelToken, LoadSpec, Wait, Waits};
use yapfast_plugin_host::pipe::{Pipe, Signal};
use yapfast_plugin_host::scheduler::{Config, Message, Scheduler};

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
        topics: Vec::new(),
    });
    harness.send(Message::Event {
        id: 3,
        plugin: b.clone(),
        events: vec![event("e2")],
        topics: Vec::new(),
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
            topics: Vec::new(),
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

    // One blocker per plugin first, so each queue is already held by a worker
    // before the commands arrive. Without them the first worker can drain both
    // queues before the second one wakes, and whether it does is the machine's
    // load rather than the scheduler's behaviour.
    harness.hold(true);
    for (id, plugin) in [(9, &a), (19, &b)] {
        harness.send(Message::Command {
            id,
            plugin: plugin.clone(),
            command: command("blocker"),
        });
    }
    harness.wait_log("command:com.example.a:blocker");
    harness.wait_log("command:com.example.b:blocker");

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
    harness.hold(false);
    harness.wait_replies(14);

    let log = harness.log();
    let calls: Vec<&String> = log
        .iter()
        .filter(|call| call.starts_with("command:") && !call.ends_with(":blocker"))
        .collect();
    assert_eq!(calls.len(), 10, "every command should run: {calls:?}");

    // Each plugin's own calls keep their order.
    let for_a: Vec<&str> = calls
        .iter()
        .filter(|call| call.starts_with("command:com.example.a"))
        .map(|call| call.rsplit(':').next().unwrap_or_default())
        .collect();
    assert_eq!(for_a, ["a0", "a1", "a2", "a3", "a4"]);

    // Two queues were in flight at once, which is what the second worker is
    // for: both blockers had entered and were still held when the commands
    // arrived, and they ran on different workers. Their commands need not
    // interleave on a busy machine, since one worker can drain a queue of
    // calls that each take microseconds before the other picks its queue up.
    let blocker_of_a = harness.threads_for("command:com.example.a:blocker");
    let blocker_of_b = harness.threads_for("command:com.example.b:blocker");
    assert_eq!(blocker_of_a.len(), 1, "one worker ran a's blocker");
    assert_eq!(blocker_of_b.len(), 1, "one worker ran b's blocker");
    assert_ne!(
        blocker_of_a[0], blocker_of_b[0],
        "both blockers were in flight at once: {blocker_of_a:?}"
    );
    let threads = harness.threads_for("command:");
    assert_eq!(
        threads.len(),
        2,
        "both workers should have run: {threads:?}"
    );
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

/// One fire isn't enough to stop a guest.
///
/// The deadline is stamped before the worker has the backend, and the runtime
/// drops a cancel aimed at a call it isn't holding — one that arrives while the
/// guest is still being set up never reaches it. So the watchdog comes back for a
/// deadline that has passed until the call returns, which is what keeps a
/// spinning guest from holding its worker and its whole hook chain for the 30 s
/// backstop.
#[test]
fn a_deadline_is_fired_again_while_the_call_is_still_out() {
    let harness = Harness::new(config(1));
    let a = plugin("a");
    harness.load(&a, 256);
    harness.live(&a, 1);
    // Four times the 25 ms a typing hook has.
    harness.sleep(Duration::from_millis(100));

    harness.send(Message::Hook {
        id: 2,
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
    harness.wait_until(|reports| {
        reports
            .iter()
            .any(|report| matches!(report, FromHelper::HookDone { .. }))
    });

    let cancels = harness
        .log()
        .iter()
        .filter(|call| call.as_str() == "cancel:com.example.a")
        .count();
    assert!(
        cancels > 1,
        "the deadline is fired until the call is over, not once: {cancels}"
    );
    harness.finish();
}

#[test]
fn a_plugin_is_asked_how_a_person_looks_through_its_own_export() {
    let harness = Harness::new(config(1));
    let a = plugin("a");
    harness.load(&a, 256);
    harness.live(&a, 101);

    harness.send(Message::Identity {
        id: 3,
        plugin: a.clone(),
        request: yapfast_plugin_api::identity::IdentityRequest {
            person: "person-1".into(),
            server: Some("server-1".into()),
            name: "Ada".into(),
            username: None,
            own: false,
        },
    });

    // The question reaches the plugin's own export, not a host call: only the
    // helper holds the instance, so `yf_identity` is the answer.
    harness.wait_log("invoke:com.example.a:yf_identity");
    let reports = harness.wait_until(|reports| {
        reports
            .iter()
            .any(|report| matches!(report, FromHelper::Identity { .. }))
    });
    assert!(
        reports.iter().any(|report| matches!(
            report,
            FromHelper::Identity { person, server, .. }
                if person == "person-1" && server.as_deref() == Some("server-1")
        )),
        "the pair the parent asked about comes back with the answer: {reports:?}"
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

/// A suspension lasts the session, and a person turning the plugin back on is
/// the one thing that ends it: an activation after a disable starts with a
/// clean account rather than with the strikes that stopped the plugin.
#[test]
fn activating_a_suspended_plugin_lifts_the_suspension() {
    let harness = Harness::new(config(1));
    let a = plugin("a");
    let b = plugin("b");
    harness.load(&a, 256);
    harness.load(&b, 256);
    harness.live(&a, 1);
    // b is what keeps the helper alive across the disable, so the activation
    // reaches a helper that still has the suspended plugin in its slots.
    harness.live(&b, 2);
    harness.trap("command:com.example.a");

    for id in 1..=3 {
        harness.send(Message::Command {
            id,
            plugin: a.clone(),
            command: command("boom"),
        });
    }
    harness.wait_until(|reports| {
        reports.iter().any(|report| {
            matches!(
                report,
                FromHelper::Status {
                    plugin,
                    status: PluginStatus::Suspended { .. },
                } if *plugin == a
            )
        })
    });

    // Off and on again, as the page does it.
    harness.send(Message::Deactivate {
        id: 10,
        plugin: a.clone(),
    });
    harness.wait_for_reply(10);
    harness.trap("nothing:");
    harness.activate(&a, 11);
    harness.wait_until(|reports| {
        reports.iter().any(|report| {
            matches!(
                report,
                FromHelper::Activated { plugin, .. } if *plugin == a
            )
        })
    });

    // The strikes went with the suspension: one more fault faults the plugin
    // rather than suspending it again after a single failure.
    harness.trap("command:com.example.a");
    harness.send(Message::Command {
        id: 12,
        plugin: a.clone(),
        command: command("boom again"),
    });
    let reports = harness.wait_until(|reports| {
        let Some(resumed) = resumed_at(reports, &a) else {
            return false;
        };
        reports[resumed..]
            .iter()
            .any(|report| matches!(report, FromHelper::Fault { plugin, .. } if *plugin == a))
    });
    let resumed = resumed_at(&reports, &a).expect("the suspension was lifted");
    assert!(
        !reports[resumed..].iter().any(|report| matches!(
            report,
            FromHelper::Status {
                status: PluginStatus::Suspended { .. },
                ..
            }
        )),
        "one fault after the person turned it on is a fault, not a suspension: {reports:?}"
    );
    harness.finish();
}

/// Where a plugin's `enabled` status was last reported, which is the one that
/// says a suspension was lifted: eviction reports the same status, and every one
/// of those comes before it.
fn resumed_at(reports: &[FromHelper], plugin: &PluginId) -> Option<usize> {
    reports.iter().rposition(|report| {
        matches!(
            report,
            FromHelper::Status {
                plugin: reported,
                status: PluginStatus::Enabled,
            } if reported == plugin
        )
    })
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
    harness.report(&b, Wait::Timer(true));

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

/// A guest that made a host call and returned is not idle: the answer is the
/// only thing that resumes it, and evicting the instance would leave the answer
/// arriving for a call nobody has.
#[test]
fn a_plugin_waiting_for_an_answer_is_not_idle() {
    let harness = Harness::new(Config {
        workers: 1,
        idle_after: Duration::from_millis(40),
    });
    let a = plugin("a");
    harness.load(&a, 256);
    harness.live(&a, 1);

    // The guest asked the parent something — an HTTP fetch, a storage write —
    // and returned pending. The runtime reports the wait, and this is the
    // account that keeps the instance off the eviction list.
    harness.report(&a, Wait::Call(7));
    assert_eq!(
        harness.scheduler.waiting(&a),
        1,
        "a pending call is a hold, and it is the only record of it"
    );

    // Well past the idle timeout with nothing else to do: the instance stays,
    // because it has work in the world rather than work in its mailbox.
    thread::sleep(Duration::from_millis(150));
    let log = harness.log();
    assert!(!log.contains(&"evict:com.example.a".to_string()), "{log:?}");

    // The answer settles the call, and the instance is idle again from there.
    harness.send(Message::Return {
        plugin: a.clone(),
        call: 7,
        answer: HostReturn::Unsupported,
    });
    harness.wait_log("answer:com.example.a:7");
    assert_eq!(harness.scheduler.waiting(&a), 0, "the answer ends the wait");
    harness.wait_until(|reports| {
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
    harness.finish();
}

/// The parent answers every request, including the posts nothing waits on. An
/// answer to one of those must not take the hold off a call still in flight, so
/// the account keeps ids rather than a count.
#[test]
fn an_answer_nothing_was_held_for_does_not_settle_a_pending_call() {
    let harness = Harness::new(config(1));
    let a = plugin("a");
    harness.load(&a, 256);
    harness.live(&a, 1);
    harness.report(&a, Wait::Call(7));

    harness.send(Message::Return {
        plugin: a.clone(),
        call: 99,
        answer: HostReturn::Unsupported,
    });
    harness.wait_log("answer:com.example.a:99");
    assert_eq!(
        harness.scheduler.waiting(&a),
        1,
        "a post's answer is not the answer a fetch is waiting for"
    );
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
                state: None,
            },
        });
        self.wait_log(&format!("load:{plugin}"));
    }

    fn activate(&self, plugin: &PluginId, id: u64) {
        self.send(Message::Activate {
            id,
            plugin: plugin.clone(),
            load: None,
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

    /// Reports what a guest left the plugin waiting on, through the sink the
    /// scheduler gave the backend: the same path a host call takes, from the
    /// thread running a guest to the account the watchdog and the eviction list
    /// read.
    fn report(&self, plugin: &PluginId, wait: Wait) {
        let sink = lock(&self.fake.waits)
            .clone()
            .expect("the scheduler installs a wait sink when it starts");
        sink(plugin, wait);
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
    /// Where the runtime would report a guest's waits; the scheduler installs
    /// it, and a test writes through it the way a guest's host call does.
    waits: Mutex<Option<Waits>>,
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

    fn event(
        &self,
        plugin: &PluginId,
        events: &[EventEnvelope],
        topics: &[TopicEvent],
    ) -> Result<(), Fault> {
        let _ = topics;
        let label = events
            .first()
            .map(|event| event.payload["label"].as_str().unwrap_or("?").to_owned())
            .unwrap_or_else(|| "none".to_owned());
        self.calls.enter(format!("event:{plugin}:{label}"))
    }

    fn invoke(
        &self,
        plugin: &PluginId,
        function: &str,
        args: &[u8],
    ) -> Result<Option<Vec<u8>>, Fault> {
        self.calls.enter(format!("invoke:{plugin}:{function}"))?;
        Ok(Some(args.to_vec()))
    }

    fn hook(&self, plugin: &PluginId, request: &HookRequest) -> Result<HookOutcome, Fault> {
        self.calls.enter(format!("hook:{plugin}"))?;
        let _ = request;
        Ok(HookOutcome::Unchanged)
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

    fn set_wait_sink(&self, waits: Waits) {
        *lock(&self.calls.waits) = Some(waits);
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
            state: None,
        },
    });
    scheduler.send(Message::Activate {
        id: 1,
        plugin: plugin.clone(),
        load: None,
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
        topics: Vec::new(),
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
