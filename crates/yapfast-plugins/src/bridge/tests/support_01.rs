//! The harness the tests in this directory share.

use super::*;

use sha2::Digest;

pub(super) fn id(text: &str) -> PluginId {
    PluginId::parse(text).expect("a plugin id")
}

pub(super) fn event(kind: EventKind, body: &str) -> EventEnvelope {
    EventEnvelope {
        kind,
        network: Network::Discord,
        at: 1,
        room: Some("room-1".to_owned()),
        author: Some("person-1".to_owned()),
        own: false,
        payload: serde_json::json!({ "body": body }),
    }
}

/// A session around a process that does nothing, with the frames it writes
/// and the frames it would read held by the test.
pub(super) fn session_stub() -> (
    tempfile::TempDir,
    Session,
    std::sync::mpsc::Receiver<Frame>,
    tokio::sync::mpsc::UnboundedSender<Inbound>,
) {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" })
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("a process that exits");
    let (writer, written) = std::sync::mpsc::channel();
    let (frames, inbound) = tokio::sync::mpsc::unbounded_channel();
    (
        directory,
        Session {
            child,
            writer,
            inbound,
            reader: None,
            next_id: 0,
            pending: BTreeMap::new(),
            loaded: BTreeSet::new(),
            active: BTreeSet::new(),
            batches: BTreeMap::new(),
            painting: Painting::default(),
            timers: BTreeMap::new(),
            watchers: BTreeMap::new(),
            queued: VecDeque::new(),
            handoff: BTreeMap::new(),
            swap: None,
            held: Vec::new(),
            calls: Vec::new(),
            held_calls: Vec::new(),
            dev: BTreeMap::new(),
            asked: false,
            bye: None,
        },
        written,
        frames,
    )
}

/// Reads one frame the session wrote, which must be there.
pub(super) fn next_frame(written: &std::sync::mpsc::Receiver<Frame>) -> Frame {
    written
        .recv_timeout(Duration::from_secs(2))
        .expect("the session should have written a frame")
}

/// The message inside a frame the session wrote.
pub(super) fn message(frame: &Frame) -> ToHelper {
    serde_json::from_slice(&frame.payload).expect("the frame carries a message")
}

/// A data directory with one installed plugin whose module is `wasm`.
///
/// The lock file's digest is the module's own, so a test starts from a store
/// a real install would have produced.
pub(super) fn installed(directory: &Path, manifest: &str, wasm: &[u8]) -> PluginId {
    let sha = {
        let mut hasher = sha2::Sha256::new();
        hasher.update(wasm);
        let mut out = String::new();
        for byte in hasher.finalize() {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    };
    let plugin = PluginManifest::parse(manifest).expect("a manifest").id;
    let stored = directory.join("plugins").join("store").join(&sha);
    std::fs::create_dir_all(&stored).expect("a store directory");
    std::fs::write(stored.join("manifest.json"), manifest).expect("a manifest");
    std::fs::write(stored.join("plugin.wasm"), wasm).expect("a module");
    std::fs::create_dir_all(directory.join("plugins")).expect("a plugin directory");
    std::fs::write(
        directory.join("plugins").join("lock.json"),
        format!(
            r#"{{"version":1,"plugins":{{"{plugin}":{{"wasm_sha256":"{sha}","enabled":true}}}}}}"#
        ),
    )
    .expect("a lock file");
    plugin
}

/// A data directory with several installed plugins, in one lock file.
///
/// Each manifest names its own module as `plugin.wasm`, and the lock entry
/// carries that module's digest, so the store is what a real install would
/// have written.
pub(super) fn install_all(directory: &Path, manifests: &[&str]) -> Vec<PluginId> {
    let mut ids = Vec::new();
    let mut entries = Vec::new();
    for manifest in manifests {
        let parsed = PluginManifest::parse(manifest).expect("a manifest");
        // A module of its own per plugin: the store is content-addressed, so
        // two plugins whose modules were byte-identical would share one
        // directory and the second manifest would replace the first.
        let module = format!("not a real module: {}", parsed.id).into_bytes();
        let mut hasher = sha2::Sha256::new();
        hasher.update(&module);
        let mut sha = String::new();
        for byte in hasher.finalize() {
            sha.push_str(&format!("{byte:02x}"));
        }
        let stored = directory.join("plugins").join("store").join(&sha);
        std::fs::create_dir_all(&stored).expect("a store directory");
        std::fs::write(stored.join("manifest.json"), manifest).expect("a manifest");
        std::fs::write(stored.join("plugin.wasm"), &module).expect("a module");
        entries.push(format!(
            r#""{}":{{"wasm_sha256":"{sha}","enabled":true}}"#,
            parsed.id
        ));
        ids.push(parsed.id);
    }
    std::fs::create_dir_all(directory.join("plugins")).expect("a plugin directory");
    std::fs::write(
        directory.join("plugins").join("lock.json"),
        format!(r#"{{"version":1,"plugins":{{{}}}}}"#, entries.join(",")),
    )
    .expect("a lock file");
    ids
}

/// The plugin system's shared half around a data directory, as the bridge
/// builds it, with a world of the test's own to publish into.
pub(super) fn shared_with_world(
    directory: &Path,
    wake: Arc<dyn Fn(Notice) + Send + Sync>,
    world: Arc<StubWorld>,
) -> Arc<Shared> {
    let store = Store::open(directory).expect("a store");
    let registry = Registry::load(&store);
    // Seeded as the bridge seeds it: the pair the person chose last time.
    let limits = Mutex::new(crate::budget::load(store.root()));
    Arc::new(Shared {
        registry: RwLock::new(registry),
        store: Mutex::new(store),
        enabled: AtomicBool::new(false),
        wanted: AtomicU64::new(0),
        intercepts: AtomicBool::new(false),
        decorates: AtomicBool::new(false),
        decorates_members: AtomicBool::new(false),
        identifies: AtomicBool::new(false),
        retains: AtomicBool::new(false),
        colours_code: AtomicBool::new(false),
        gesture: Mutex::new(None),
        sends: Mutex::new(BTreeMap::new()),
        off: Mutex::new(None),
        gate: Mutex::new(None),
        pictures: Mutex::new(None),
        held: Mutex::new(BTreeMap::new()),
        helper_installed: false,
        helper: AtomicU32::new(0),
        limits,
        world,
        wake,
    })
}

/// The same, with nothing published into it, for the tests that read nothing.
pub(super) fn shared(directory: &Path, wake: Arc<dyn Fn(Notice) + Send + Sync>) -> Arc<Shared> {
    shared_with_world(directory, wake, Arc::new(StubWorld::new()))
}

/// Where the bridge's notices arrive, in place of the application's event.
pub(super) fn notice_channel() -> (
    Arc<dyn Fn(Notice) + Send + Sync>,
    std::sync::mpsc::Receiver<Notice>,
) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let wake = Arc::new(move |notice: Notice| {
        let _ = sender.send(notice);
    });
    (wake, receiver)
}

/// A runtime to drive the sessions' futures on, which is all a test needs of
/// the application's own.
pub(super) fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("a runtime for the test")
}

/// The application's side of the bridge, as a test gives it: a runtime the test
/// drives, and a helper whose own words go nowhere.
pub(super) struct TestTasks(pub(super) tokio::runtime::Runtime);

impl Spawn for TestTasks {
    fn spawn(&self, work: crate::bridge::tasks::Work) -> anyhow::Result<Task> {
        Ok(Task::new(self.0.spawn(work)))
    }

    fn spawn_blocking(&self, work: Box<dyn FnOnce() + Send + 'static>) -> anyhow::Result<Task> {
        Ok(Task::new(self.0.spawn_blocking(work)))
    }

    fn wait(&self, task: &mut Task, limit: std::time::Duration) -> bool {
        self.0
            .block_on(async { tokio::time::timeout(limit, task).await.is_ok() })
    }
}

/// The host a bridge opened by a test is given.
pub(super) fn host() -> Host {
    Host {
        tasks: Arc::new(TestTasks(runtime())),
        stderr: || std::process::Stdio::null(),
    }
}

/// A bridge over a data directory, with a world nothing published into.
pub(super) fn bridge(directory: &Path, wake: Arc<dyn Fn(Notice) + Send + Sync>) -> Bridge {
    Bridge::open(directory, wake, host(), Arc::new(StubWorld::new())).expect("a bridge")
}

/// A manifest for a plugin that intercepts sends, keeps storage and listens
/// to typing.
pub(super) const HOOK_MANIFEST: &str = r#"{"api":1,"id":"com.example.hook","name":"Hook","version":"1.0.0",
        "wasm":"plugin.wasm",
        "permissions":{"capabilities":["messages.intercept","storage","messages.read"]},
        "activation":{"on":["event:typing"]}}"#;

/// The same, for a plugin whose `block` outcome may stop a send: that is
/// what `on_failure` says, and it needs the capability the one above has.
pub(super) const BLOCKING_HOOK_MANIFEST: &str = r#"{"api":1,"id":"com.example.block","name":"Block","version":"1.0.0",
        "wasm":"plugin.wasm",
        "on_failure":"block",
        "permissions":{"capabilities":["messages.intercept"]},
        "activation":{"on":["event:typing"]}}"#;

/// Holds a hook that isn't a message through the session these tests own,
/// as `Bridge::typing` and `Bridge::attachment` do through the bridge.
pub(super) fn hold(
    session: &mut Session,
    shared: &Arc<Shared>,
    id: u64,
    hook: HookKind,
    subject: HookSubject,
    room: &str,
) -> Option<u64> {
    let held = held_hook(shared, id, hook, room)?;
    runtime().block_on(session.on_command(
        shared,
        Request::Hook {
            held,
            request: HookRequest { hook, subject },
        },
    ));
    Some(id)
}

pub(super) fn typing(room: &str) -> HookSubject {
    HookSubject::Typing(TypingPending {
        room: room.to_owned(),
    })
}

pub(super) fn draft(body: &str) -> Draft {
    Draft {
        room: "room-1".to_owned(),
        body: body.to_owned(),
        reply_to: None,
        attachments: Vec::new(),
    }
}

/// A manifest that may read what the application knows.
pub(super) const READ_MANIFEST: &str = r#"{"api":1,"id":"com.example.reader","name":"Reader",
        "version":"1.0.0","wasm":"plugin.wasm",
        "permissions":{"capabilities":["messages.read","people.read","servers.read"]}}"#;

/// One host call, as the helper hands it over, answered the way a guest is.
pub(super) fn ask(
    session: &mut Session,
    written: &std::sync::mpsc::Receiver<Frame>,
    shared: &Arc<Shared>,
    plugin: &PluginId,
    call: HostCall,
) -> HostReturn {
    let frame = Frame::new(
        FrameKind::Request,
        0,
        serde_json::to_vec(&FromHelper::Call {
            plugin: plugin.clone(),
            call: 3,
            request: call,
        })
        .expect("it serializes"),
    );
    assert!(session.on_frame(shared, frame).is_none());
    let reply = next_frame(written);
    let ToHelper::Return { answer, .. } = message(&reply) else {
        panic!("a host call is answered with a return");
    };
    answer
}
