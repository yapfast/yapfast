//! End-to-end runs of the helper, as the parent runs it: the real binary, the
//! real pipe and the real Extism runtime, driven by the frames in
//! [`yapfast_plugin_api::frame`] and [`yapfast_plugin_api::message`].
//!
//! Every test starts `yapfast-plugins run` itself, so a scenario that hangs is
//! that test's problem alone: each wait has a budget, and the child is killed
//! when the test ends, whether it passed or not.
//!
//! The modules are the text fixtures in `testing/plugins/`, which wasmtime
//! compiles at load, so nothing here needs a WebAssembly toolchain.

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;
use yapfast_plugin_api::API_VERSION;
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::HostReturn;
use yapfast_plugin_api::event::{EventEnvelope, EventKind, Network};
use yapfast_plugin_api::frame::{Frame, FrameKind, read_frame, write_frame};
use yapfast_plugin_api::hook::{Draft, HookKind, HookOutcome, HookRequest, HookSubject};
use yapfast_plugin_api::message::{Fault, FaultKind, FromHelper, ToHelper};

/// How long one scenario may take to say the next thing it must say.
///
/// The slowest step is a deadline being noticed, and the helper's own start
/// budget for that is seconds; compiling a module from cold takes tens of
/// milliseconds. This is generous beside both and still bounds a hung helper.
const BUDGET: Duration = Duration::from_secs(20);

/// The pages a plugin gets unless its scenario needs another number.
const PAGES: u32 = 256;

#[test]
fn the_handshake_is_answered_with_ready() {
    let mut helper = Helper::start("hello");
    helper.send(FrameKind::Hello, 1, &ToHelper::Hello { api: API_VERSION });
    match helper.wait_for(BUDGET, "Ready", |message| {
        matches!(message, FromHelper::Ready { .. })
    }) {
        FromHelper::Ready { api, version } => {
            assert_eq!(api, API_VERSION, "the helper speaks the parent's API");
            assert!(!version.is_empty(), "the helper names its own version");
        }
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn a_plugin_activates_and_runs_an_event() {
    let mut helper = Helper::start("activate");
    let echo = plugin("echo");
    helper.load(&echo, "echo.wat", PAGES);
    helper.activate(&echo);
    match helper.wait_for(BUDGET, "Activated", |message| {
        matches!(message, FromHelper::Activated { .. })
    }) {
        FromHelper::Activated {
            plugin,
            state,
            memory_pages,
        } => {
            assert_eq!(plugin, echo);
            assert_eq!(state, None, "nothing was handed on by a first activation");
            assert!(
                memory_pages > 0 && memory_pages <= PAGES,
                "an active plugin holds at most the pages it asked for, not {memory_pages}"
            );
        }
        other => panic!("expected Activated, got {other:?}"),
    }
    helper.event(&echo, an_event());
    helper.wait_for(BUDGET, "EventDone", |message| {
        matches!(message, FromHelper::EventDone { .. })
    });
}

#[test]
fn a_spinning_activation_is_cancelled() {
    let mut helper = Helper::start("spin");
    let spin = plugin("spin");
    helper.load(&spin, "spin.wat", PAGES);
    helper.activate(&spin);
    let fault = helper.wait_for_fault(BUDGET, FaultKind::Deadline);
    assert_eq!(fault.kind, FaultKind::Deadline);
}

#[test]
fn an_allocation_past_the_page_limit_is_an_out_of_memory_fault() {
    let mut helper = Helper::start("hog");
    let hog = plugin("hog");
    // 64 pages is 4 MiB, and the fixture grows past whatever it is given.
    helper.load(&hog, "hog.wat", 64);
    helper.activate(&hog);
    let fault = helper.wait_for_fault(BUDGET, FaultKind::OutOfMemory);
    assert_eq!(fault.kind, FaultKind::OutOfMemory);
}

#[test]
fn a_trap_is_a_trap_fault() {
    let mut helper = Helper::start("trap");
    let trapped = plugin("trap");
    helper.load(&trapped, "trap.wat", PAGES);
    helper.activate(&trapped);
    let fault = helper.wait_for_fault(BUDGET, FaultKind::Trap);
    assert_eq!(fault.kind, FaultKind::Trap);
}

#[test]
fn a_plugin_that_refuses_to_activate_is_a_fault() {
    let mut helper = Helper::start("bad-activate");
    let bad = plugin("bad-activate");
    helper.load(&bad, "bad_activate.wat", PAGES);
    helper.activate(&bad);
    let fault = helper.wait_for_fault(BUDGET, FaultKind::ActivateFailed);
    assert_eq!(fault.kind, FaultKind::ActivateFailed);
}

#[test]
fn a_guest_call_reaches_the_parent_and_is_answered() {
    let mut helper = Helper::start("call");
    let slow = plugin("slow-host");
    let echo = plugin("echo");
    helper.load(&slow, "slow_host.wat", PAGES);
    helper.load(&echo, "echo.wat", PAGES);
    for plugin in [&slow, &echo] {
        helper.activate(plugin);
        helper.wait_for(BUDGET, "Activated", |message| {
            matches!(message, FromHelper::Activated { .. })
        });
    }

    // The fixture's handler asks for a URL, which cannot be answered from
    // inside the helper, so the call travels to the parent.
    helper.event(&slow, an_event());
    let (id, message) = helper.wait_for_frame(BUDGET, "Call", |message| {
        matches!(message, FromHelper::Call { .. })
    });
    let call = match message {
        FromHelper::Call {
            plugin,
            call,
            request,
        } => {
            assert_eq!(plugin, slow);
            assert!(call > 0, "a host call has the id its answer names");
            assert_eq!(request.method, "http.fetch");
            assert_eq!(
                request.payload["url"], "https://api.example.com/version",
                "the fixture's own request reaches the parent"
            );
            call
        }
        other => panic!("expected Call, got {other:?}"),
    };

    // While that answer is missing, another plugin still runs, because nothing
    // is waiting on the parent.
    helper.event(&echo, an_event());
    helper.wait_for(BUDGET, "EventDone", |message| {
        matches!(message, FromHelper::EventDone { .. })
    });

    // The answer reaches the guest as a later event, and the instance takes it
    // without faulting.
    helper.send(
        FrameKind::Reply,
        id,
        &ToHelper::Return {
            plugin: slow.clone(),
            // The id the guest was told to expect the answer under.
            call,
            answer: HostReturn::Pending {
                tag: "lookup".into(),
            },
        },
    );
    helper.event(&slow, an_event());
    helper.wait_for(BUDGET, "EventDone", |message| {
        matches!(message, FromHelper::EventDone { .. })
    });
}

#[test]
fn a_hook_that_answers_nonsense_fails_open() {
    let mut helper = Helper::start("bad-answer");
    let bad = plugin("bad-answer");
    helper.load(&bad, "bad_answer.wat", PAGES);
    helper.activate(&bad);
    helper.wait_for(BUDGET, "Activated", |message| {
        matches!(message, FromHelper::Activated { .. })
    });
    let id = helper.next_id();
    helper.send(
        FrameKind::Request,
        id,
        &ToHelper::Hook {
            chain: vec![bad.clone()],
            request: a_hook(),
        },
    );
    // The answer that isn't a HookOutcome is a fault against the plugin, and
    // the chain answers as if it had said nothing.
    let fault = helper.wait_for_fault(BUDGET, FaultKind::BadAnswer);
    assert_eq!(fault.kind, FaultKind::BadAnswer);
    match helper.wait_for(BUDGET, "HookDone", |message| {
        matches!(message, FromHelper::HookDone { .. })
    }) {
        FromHelper::HookDone { outcome, failures } => {
            assert_eq!(
                outcome,
                HookOutcome::Unchanged,
                "a hook that fails leaves the draft as it was"
            );
            assert_eq!(failures.len(), 1, "the failure is reported, not hidden");
            assert_eq!(failures[0].0, bad);
            assert_eq!(failures[0].1.kind, FaultKind::BadAnswer);
        }
        other => panic!("expected HookDone, got {other:?}"),
    }
}

#[test]
fn shutdown_ends_the_process() {
    let mut helper = Helper::start("shutdown");
    helper.send(FrameKind::Hello, 1, &ToHelper::Hello { api: API_VERSION });
    helper.wait_for(BUDGET, "Ready", |message| {
        matches!(message, FromHelper::Ready { .. })
    });
    let id = helper.next_id();
    helper.send(FrameKind::Request, id, &ToHelper::Shutdown);
    match helper.wait_for(BUDGET, "Bye", |message| {
        matches!(message, FromHelper::Bye { .. })
    }) {
        FromHelper::Bye { reason } => assert!(!reason.is_empty(), "a Bye says why"),
        other => panic!("expected Bye, got {other:?}"),
    }
    helper.wait_for_close(BUDGET);
    let status = helper.reap(BUDGET);
    assert!(
        status.success(),
        "a shutdown is a clean exit, not {status:?}"
    );
}

/// A plugin of the scenarios' own naming, under a namespace nothing else uses.
fn plugin(name: &str) -> PluginId {
    PluginId::parse(&format!("com.example.{name}")).expect("a valid plugin id")
}

/// The folder the text fixtures live in.
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testing/plugins")
        .canonicalize()
        .expect("the fixture folder is in the repository")
}

/// The event every scenario sends.
fn an_event() -> EventEnvelope {
    EventEnvelope {
        kind: EventKind::MessageCreate,
        network: Network::Discord,
        at: 1,
        room: Some("room-1".into()),
        author: None,
        own: false,
        payload: json!({ "body": "hi" }),
    }
}

/// A message about to be sent, as a hook sees it.
fn a_hook() -> HookRequest {
    HookRequest {
        hook: HookKind::MessagePreSend,
        subject: HookSubject::Draft(Draft {
            room: "room-1".into(),
            body: "hi".into(),
            reply_to: None,
            attachments: Vec::new(),
        }),
    }
}

/// A helper process, the pipe to it, and what it has said so far.
struct Helper {
    child: Child,
    stdin: ChildStdin,
    /// The frames a reader thread took off the helper's stdout.
    frames: Receiver<Frame>,
    /// Every message received so far, so a failure can say what the helper did
    /// instead of what the test wanted.
    seen: Vec<FromHelper>,
    next: u64,
    cache: PathBuf,
}

impl Helper {
    /// Starts `yapfast-plugins run` with a cache directory of its own.
    ///
    /// A cache shared between tests would let one test's compile hide a module
    /// the next one cannot compile.
    fn start(name: &str) -> Helper {
        let cache = std::env::temp_dir().join(format!("yapfast-plugins-e2e/{name}"));
        let _ = std::fs::remove_dir_all(&cache);
        std::fs::create_dir_all(&cache).expect("a cache directory for the helper");
        let mut child = Command::new(env!("CARGO_BIN_EXE_yapfast-plugins"))
            .arg("run")
            .arg("--cache")
            .arg(&cache)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The helper's own lines go to stderr; inheriting them puts them in
            // the test's output instead of filling a pipe nobody drains.
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the helper binary starts");
        let stdin = child.stdin.take().expect("the helper's stdin is piped");
        let stdout = child.stdout.take().expect("the helper's stdout is piped");
        // One thread reads every frame, and the test waits on the channel with
        // a budget: a helper that stops talking must not stop the test.
        let (sender, frames) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(frame)) = read_frame(&mut reader) {
                if sender.send(frame).is_err() {
                    return;
                }
            }
        });
        Helper {
            child,
            stdin,
            frames,
            seen: Vec::new(),
            next: 1,
            cache,
        }
    }

    /// The id for the next request this test sends.
    fn next_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// Writes one frame, and flushes it.
    fn send(&mut self, kind: FrameKind, id: u64, message: &ToHelper) {
        let payload = serde_json::to_vec(message).expect("a ToHelper is JSON");
        write_frame(&mut self.stdin, &Frame::new(kind, id, payload))
            .expect("the helper's pipe takes a frame");
    }

    /// Loads a fixture as `plugin`, without activating it.
    fn load(&mut self, plugin: &PluginId, fixture: &str, memory_pages: u32) {
        let id = self.next_id();
        self.send(
            FrameKind::Request,
            id,
            &ToHelper::Load {
                plugin: plugin.clone(),
                wasm: fixtures().join(fixture).with_extension("wasm"),
                memory_pages,
                wasi: false,
            },
        );
    }

    /// Asks the helper to instantiate and activate a plugin.
    fn activate(&mut self, plugin: &PluginId) {
        let id = self.next_id();
        self.send(
            FrameKind::Request,
            id,
            &ToHelper::Activate {
                plugin: plugin.clone(),
            },
        );
    }

    /// Sends one event as a request, which is answered with `EventDone`.
    fn event(&mut self, plugin: &PluginId, event: EventEnvelope) {
        let id = self.next_id();
        self.send(
            FrameKind::Request,
            id,
            &ToHelper::Event {
                plugin: plugin.clone(),
                events: vec![event],
            },
        );
    }

    /// Waits for the first message the predicate accepts, with its frame id.
    ///
    /// Anything else the helper says is passed over, except a fault: a scenario
    /// that faults while the test waits for something else has already failed,
    /// and saying so beats waiting out the budget.
    fn wait_for_frame(
        &mut self,
        budget: Duration,
        what: &str,
        mut wanted: impl FnMut(&FromHelper) -> bool,
    ) -> (u64, FromHelper) {
        let deadline = Instant::now() + budget;
        loop {
            let frame = match self
                .frames
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(frame) => frame,
                Err(RecvTimeoutError::Timeout) => {
                    panic!(
                        "waiting for {what}, the helper said nothing; it had said {:?}",
                        self.seen
                    )
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!(
                        "waiting for {what}, the helper closed its pipe; it had said {:?}",
                        self.seen
                    )
                }
            };
            let message: FromHelper = match serde_json::from_slice(&frame.payload) {
                Ok(message) => message,
                Err(error) => panic!(
                    "waiting for {what}, a {:?} frame wasn't a FromHelper: {error}; payload {}",
                    frame.kind,
                    String::from_utf8_lossy(&frame.payload)
                ),
            };
            if wanted(&message) {
                return (frame.id, message);
            }
            if let FromHelper::Fault { plugin, fault } = &message {
                panic!(
                    "waiting for {what}, `{plugin}` faulted: {:?} ({}); the helper said {:?}",
                    fault.kind, fault.reason, self.seen
                );
            }
            self.seen.push(message);
        }
    }

    /// Waits for the first message the predicate accepts.
    fn wait_for(
        &mut self,
        budget: Duration,
        what: &str,
        wanted: impl FnMut(&FromHelper) -> bool,
    ) -> FromHelper {
        self.wait_for_frame(budget, what, wanted).1
    }

    /// Waits for a fault of `kind`.
    ///
    /// A fault caused by a request is reported on its own, so that the plugin's
    /// page shows it even when nobody waits, and answered to the request too;
    /// the reported one is where the kind belongs, so this reads until it finds
    /// it and fails with whatever kind came instead.
    fn wait_for_fault(&mut self, budget: Duration, kind: FaultKind) -> Fault {
        let deadline = Instant::now() + budget;
        let mut seen: Option<Fault> = None;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let frame = match self.frames.recv_timeout(left) {
                Ok(frame) => frame,
                Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => panic!(
                    "waiting for a {:?} fault, the helper said {:?}{}",
                    kind,
                    self.seen,
                    match &seen {
                        Some(fault) => format!("; the only fault was {:?}", fault.kind),
                        None => " and no fault at all".to_owned(),
                    }
                ),
            };
            let Ok(message) = serde_json::from_slice::<FromHelper>(&frame.payload) else {
                continue;
            };
            match message {
                FromHelper::Fault { fault, .. } if fault.kind == kind => {
                    assert!(!fault.reason.is_empty(), "a fault names what happened");
                    return fault;
                }
                FromHelper::Fault { fault, .. } => {
                    seen.get_or_insert(fault);
                }
                other => self.seen.push(other),
            }
        }
    }

    /// Waits for the helper to close its end of the pipe.
    fn wait_for_close(&mut self, budget: Duration) {
        let deadline = Instant::now() + budget;
        loop {
            match self
                .frames
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(frame) => {
                    if let Ok(message) = serde_json::from_slice::<FromHelper>(&frame.payload) {
                        self.seen.push(message);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {
                    panic!("the helper's pipe stayed open; it had said {:?}", self.seen)
                }
            }
        }
    }

    /// Waits for the child to exit, and reaps it.
    fn reap(&mut self, budget: Duration) -> ExitStatus {
        let deadline = Instant::now() + budget;
        loop {
            match self
                .child
                .try_wait()
                .expect("the helper is this test's child")
            {
                Some(status) => return status,
                None if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                None => panic!("the helper is still running after its pipe closed"),
            }
        }
    }
}

impl Drop for Helper {
    /// Kills a helper a failed test left behind, and takes the cache with it.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.cache);
    }
}
