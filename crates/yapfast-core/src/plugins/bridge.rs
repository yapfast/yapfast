//! The bridge to the helper: the run loop's task, the frames, and the answers.
//!
//! One Tokio task owns the conversation with the helper process. The
//! application's calls become [`Command`]s into that task; the helper's frames
//! become [`Notice`]s back through the closure the bridge was opened with, which
//! the application turns into an event. A guest's host call arrives as a frame
//! and is answered here, where the token, the storage, the settings, the timer
//! table and the network gate are: none of those ever enter the helper.
//!
//! The process is disposable. It starts when a plugin first has something to do,
//! ends when nothing is left, and is replaced rather than trusted after a crash;
//! [`crate::plugins::spawn`] decides how long to wait before trying again and
//! when to stop trying at all.
//!
//! Three properties shape the code here. An event that no enabled plugin
//! subscribed to must cost one load of an atomic counter and nothing else, so the
//! subscriptions are a bitmask in [`crate::plugins::registry`] and are checked
//! before anything is locked or serialized. A hook is on the path a person waits
//! on, so a chain has the budget the contract gives it and fails open. And the
//! task never blocks: the pipes are read and written by threads of their own,
//! because a helper that stops reading would otherwise stall the timers that
//! exist to notice it.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use yapfast_plugin_api::call::{
    self, CommandInvocation, CommandOutcome, HostCall, HostReturn, HttpRequest, StorageGet,
    StorageList, StorageSet, TimerCancel, TimerRequest,
};
use yapfast_plugin_api::capability::Capability;
use yapfast_plugin_api::event::{EventEnvelope, EventKind};
use yapfast_plugin_api::frame::{self, Frame, FrameKind};
use yapfast_plugin_api::hook::{Draft, HookKind, HookOutcome, HookRequest, HookSubject};
use yapfast_plugin_api::limits;
use yapfast_plugin_api::manifest::CommandDecl;
use yapfast_plugin_api::message::{Fault, FromHelper, LogLevel, LogRecord, PluginStatus, ToHelper};
use yapfast_plugin_api::{API_VERSION, PluginId};

use crate::plugins::registry::{Registry, Subscriptions};
use crate::plugins::spawn::{self, Helper, Restarts};
use crate::plugins::state::{Notice, PreSend};
use crate::plugins::store::{Module, Store};

/// How long coalesced events wait for company.
///
/// A burst of typing is one call rather than forty; a quarter of a second is
/// short enough that a plugin reacting to someone typing still feels immediate.
const COALESCE_MS: u64 = 250;

/// The most events one batch carries, and the most bytes: a busy server can
/// produce more typing than a guest wants to read at once, and a frame is at
/// most 1 MiB.
const BATCH_MAX_EVENTS: usize = 64;
const BATCH_MAX_BYTES: usize = 256 << 10;

/// How long the helper has to answer the hello, and each kind of request after
/// that. The hook budgets come from the contract; the rest are wide enough for a
/// module to compile on a cold cache and narrow enough that a wedged helper is
/// noticed in seconds.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a load gets: it compiles the module into wasmtime's cache the first
/// time, which is work on a cold cache and a busy machine. The guest has its own
/// deadline once it is running, so this is only about the report that it loaded.
const LOAD_TIMEOUT: Duration = Duration::from_secs(10);
/// How long an activation gets, which is `yf_activate` on the other side of the
/// pipe. The helper cancels a guest that runs long and answers with a fault, so
/// this longer budget only catches a helper that stopped answering at all.
const ACTIVATE_TIMEOUT: Duration = Duration::from_secs(5);
const DEACTIVATE_TIMEOUT: Duration = Duration::from_millis(100);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
const EVICT_TIMEOUT: Duration = Duration::from_millis(500);
const KICK_TIMEOUT: Duration = Duration::from_millis(250);
/// A timer the guest asked for: an answer says it was set, nothing more.
const TIMER_TIMEOUT: Duration = Duration::from_secs(2);
const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);

/// How long the application waits for the helper to exit before giving up on a
/// graceful stop.
const CLOSE_WAIT: Duration = Duration::from_secs(3);

/// The parent's half of the plugin system.
pub struct Bridge {
    data_dir: PathBuf,
    /// A runtime of its own for the work that takes seconds, such as a fetch.
    /// Separate from the supervisor's so a fetch doesn't hold the helper's
    /// thread, and created on first use so nothing runs while nothing is asked.
    background: Option<crate::runtime::BackgroundRuntime>,
    /// The fetch tasks that runtime holds, capped: a task cancelled by being
    /// dropped would report nothing, so they are kept until they end.
    fetches: Vec<crate::runtime::BackgroundTask<()>>,
    shared: Arc<Shared>,
    /// The running supervisor, or nothing when the helper isn't needed.
    supervisor: Option<Supervisor>,
    /// Bumped for each send held for a hook chain: the composer's spinner and
    /// the answer that replaces it pair on this number.
    next_send: u64,
}

impl Bridge {
    /// Reads the store and the enabled manifests; starts nothing.
    pub fn open(data_dir: &Path, wake: Arc<dyn Fn(Notice) + Send + Sync>) -> anyhow::Result<Self> {
        let store = Store::open(data_dir)?;
        let registry = Registry::load(&store);
        let shared = Arc::new(Shared {
            registry: RwLock::new(registry),
            store: Mutex::new(store),
            enabled: AtomicBool::new(false),
            wanted: AtomicU64::new(0),
            intercepts: AtomicBool::new(false),
            off: Mutex::new(None),
            gate: Mutex::new(None),
            // Asked once: a file beside the app doesn't appear while it runs.
            helper_installed: spawn::program().is_ok(),
            helper: AtomicU32::new(0),
            wake,
        });
        shared.publish();
        Ok(Bridge {
            data_dir: data_dir.to_owned(),
            background: None,
            fetches: Vec::new(),
            shared,
            supervisor: None,
            next_send: 0,
        })
    }

    /// Whether any plugin is enabled. The tap points test this first, so it
    /// reads a flag rather than the registry.
    pub fn enabled(&self) -> bool {
        self.shared.enabled.load(Ordering::Relaxed)
    }

    /// Whether an enabled plugin intercepts outgoing messages, which is what
    /// makes the send path build a draft at all.
    pub fn intercepts_sends(&self) -> bool {
        self.shared.intercepts.load(Ordering::Relaxed)
    }

    /// Whether anything subscribed to this kind of event.
    ///
    /// The summary's mask is kept as one number for exactly this: a tap asks
    /// before it writes a payload, so an event nobody wants costs one load.
    pub fn subscribes(&self, kind: EventKind) -> bool {
        self.shared.wanted.load(Ordering::Relaxed) & Subscriptions::bit(kind) != 0
    }

    /// The commands the enabled plugins declare, for the composer's parser.
    pub fn commands(&self) -> Vec<(PluginId, CommandDecl)> {
        self.shared.registry().commands()
    }

    /// Hands the bridge the network gate, which is what makes `http.fetch` work.
    ///
    /// The gate is the application's, because it holds the consent dialogs and
    /// the transport; the bridge only passes requests to it and answers the guest
    /// with what it decided. Every enabled plugin's declaration is given to the
    /// gate here, so a request is checked against the manifest the person
    /// reviewed rather than against what the guest claims.
    pub fn install_network(&mut self, gate: Arc<crate::plugins::network::NetworkGate>) {
        *self
            .shared
            .gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(gate);
        let plugins: Vec<PluginId> = {
            let registry = self.shared.registry();
            registry
                .statuses()
                .into_iter()
                .filter(|(plugin, _)| registry.entry(plugin).is_some_and(|entry| entry.runs()))
                .map(|(plugin, _)| plugin)
                .collect()
        };
        for plugin in plugins {
            self.shared.declare(&plugin);
        }
    }

    /// Activates what must run at startup: plugins whose manifest asks for it,
    /// and the ones that intercept sends, since the first message of the session
    /// must not go out before they are ready.
    pub fn start(&mut self) {
        if !self.enabled() {
            return;
        }
        let plugins: Vec<PluginId> = {
            let registry = self.shared.registry();
            registry
                .statuses()
                .into_iter()
                .filter(|(_, status)| !matches!(status, PluginStatus::Installed))
                .filter(|(plugin, _)| {
                    registry
                        .entry(plugin)
                        .is_some_and(|entry| entry.manifest.activates_at_startup())
                })
                .map(|(plugin, _)| plugin)
                .collect()
        };
        if plugins.is_empty() {
            return;
        }
        self.send(Request::Activate { plugins });
    }

    /// Hands one event to the plugins subscribed to it.
    ///
    /// The event is serialized once, and only if someone wants it; the filters
    /// are applied here, so a plugin that asked for one room doesn't wake the
    /// helper for the others.
    pub fn observe(&mut self, event: EventEnvelope) {
        if self.shared.wanted.load(Ordering::Relaxed) & Subscriptions::bit(event.kind) == 0 {
            return;
        }
        let targets = self.shared.registry().subscribers(&event);
        if targets.is_empty() {
            return;
        }
        let Some(shared) = SharedEvent::new(event) else {
            return;
        };
        for plugin in targets {
            self.send(Request::Event {
                plugin,
                event: shared.clone(),
            });
        }
    }

    /// Runs the `message.pre_send` chain, if anything intercepts sends.
    pub fn pre_send(&mut self, draft: Draft) -> PreSend {
        if !self.shared.intercepts.load(Ordering::Relaxed) {
            return PreSend::Send(draft);
        }
        self.intercept(HookKind::MessagePreSend, draft)
    }

    /// Runs the chain in front of an edit, which is a different set of plugins
    /// from the one in front of a send: a plugin that rewrites what is typed
    /// needn't intercept what leaves, and one that filters sends usually
    /// doesn't want to see the same text twice.
    pub fn pre_edit(&mut self, draft: Draft) -> PreSend {
        self.intercept(HookKind::MessagePreEdit, draft)
    }

    /// Holds an outgoing draft for one chain, whichever hook it is.
    fn intercept(&mut self, hook: HookKind, draft: Draft) -> PreSend {
        let chain = self.shared.registry().chain(hook);
        if chain.is_empty() {
            return PreSend::Send(draft);
        }
        self.next_send += 1;
        let id = self.next_send;
        let held = HeldSend {
            id,
            draft: draft.clone(),
            hook,
            chain: chain.clone(),
        };
        let request = HookRequest {
            hook,
            subject: HookSubject::Draft(draft.clone()),
        };
        if !self.send(Request::Hook { held, request }) {
            // Nothing can run the chain, so the send goes as it was: the same
            // answer a plugin that traps gets, and the person isn't left with a
            // message that will never leave.
            return PreSend::Send(draft);
        }
        PreSend::Held { id }
    }

    /// Turns a plugin on or off: the registry, the lock file, the helper's
    /// instances and the page all follow, without a restart.
    pub fn set_enabled(&mut self, plugin: &PluginId, enabled: bool) -> anyhow::Result<()> {
        {
            let mut registry = self.shared.registry_mut();
            let store = self.shared.store();
            registry.set_enabled(plugin, enabled, &store)?;
        }
        self.shared.publish();
        self.shared.declare(plugin);
        if enabled {
            // The person is asking for it to work: a helper that gave up
            // deserves another try.
            self.shared.clear_off();
            self.send(Request::Activate {
                plugins: vec![plugin.clone()],
            });
        } else if self.enabled() {
            self.send(Request::Deactivate {
                plugin: plugin.clone(),
            });
        } else {
            // Nothing is left to run, so the process goes: that is what an idle
            // Yapfast is. Asking rather than dropping the task lets the helper
            // write what the plugin saved on its way out, which `close` waits
            // for.
            self.close();
        }
        self.announce(plugin);
        Ok(())
    }

    /// Runs a command a person typed, through the plugin that declares it.
    ///
    /// The answer comes back as a `CommandDone` frame, which the application
    /// shows; a command that doesn't answer within two seconds is reported as
    /// failed rather than left hanging.
    pub fn run_command(&mut self, plugin: &PluginId, invocation: CommandInvocation) {
        self.send(Request::Command {
            plugin: plugin.clone(),
            invocation,
        });
    }

    /// Drops the plugins' instances now, keeping the process.
    ///
    /// The idle clock and the memory budget belong to the helper, which knows
    /// when an instance has been quiet; this is for a reason it can't know, such
    /// as the window going to the tray, where the person isn't looking and the
    /// memory should go back anyway.
    pub fn evict(&mut self) {
        let plugins: Vec<PluginId> = self
            .shared
            .statuses()
            .into_iter()
            .filter(|(_, status)| matches!(status, PluginStatus::Active))
            .map(|(plugin, _)| plugin)
            .collect();
        if !plugins.is_empty() {
            self.send(Request::Evict { plugins });
        }
    }

    /// Every plugin's state, for the page that lists them.
    pub fn statuses(&self) -> Vec<(PluginId, PluginStatus)> {
        self.shared.statuses()
    }

    /// What both threads of the plugin system read, which the pages' views are
    /// built from.
    pub(crate) fn shared(&self) -> &Shared {
        &self.shared
    }

    /// Reads a plugin out of its repository for the page's Add from URL, and
    /// announces it as [`Notice::Fetched`].
    ///
    /// It takes seconds, so it runs on a runtime of its own: the window's thread
    /// must not wait, and the supervisor's runtime belongs to the helper.
    pub fn fetch(&mut self, url: String) {
        let shared = Arc::clone(&self.shared);
        let asked = url.clone();
        let task = match self.background() {
            Ok(runtime) => runtime.spawn(async move {
                let found =
                    tokio::task::spawn_blocking(move || crate::plugins::installing::fetch(&asked))
                        .await;
                let result = match found {
                    Ok(Ok(crate::plugins::installing::Outcome::Plugin(fetched))) => Ok(fetched),
                    // A repository that holds several plugins: each is installed
                    // by its own address, so say how rather than picking one.
                    Ok(Ok(crate::plugins::installing::Outcome::Index { paths, .. })) => {
                        Err(format!(
                            "That repository holds several plugins; install one by its \
                             address, as …#path={}.",
                            paths.first().cloned().unwrap_or_default()
                        ))
                    }
                    Ok(Err(error)) => Err(error),
                    Err(error) => Err(format!("the fetch didn't finish: {error}")),
                };
                shared.announce(Notice::Fetched { url, result });
            }),
            Err(error) => {
                shared.announce(Notice::Fetched {
                    url,
                    result: Err(format!("{error:#}")),
                });
                return;
            }
        };
        self.fetches.push(task);
        // Four is more than one person asks for in a row, and each one ends by
        // itself; the oldest's handle is only kept alive so the task isn't
        // cancelled before it reports.
        if self.fetches.len() > 4 {
            self.fetches.remove(0);
        }
    }

    /// Writes a fetched plugin into the store, and reads the store again.
    ///
    /// A helper that is running is stopped first: its instances were loaded from
    /// the versions the store held a moment ago, and the plugin's next trigger
    /// starts a fresh one from what is there now.
    pub fn install(&mut self, package: &crate::plugins::store::Package) -> anyhow::Result<()> {
        {
            let mut store = self.shared.store();
            store.install(package)?;
        }
        self.reload();
        Ok(())
    }

    /// Removes a plugin, and its data when `delete_data`, then reads the store
    /// again.
    pub fn uninstall(&mut self, plugin: &PluginId, delete_data: bool) -> anyhow::Result<()> {
        {
            let mut store = self.shared.store();
            store.uninstall(plugin, delete_data)?;
        }
        self.reload();
        Ok(())
    }

    /// Goes back to the version an install replaced.
    pub fn roll_back(&mut self, plugin: &PluginId) -> anyhow::Result<()> {
        {
            let mut store = self.shared.store();
            store.roll_back(plugin)?;
        }
        self.reload();
        Ok(())
    }

    /// Reads the lock file and every installed manifest again, which is what an
    /// install, an uninstall or a roll back changes, and tells the page.
    fn reload(&mut self) {
        // A helper loaded its instances from the versions that were there, so it
        // goes: what runs next is what the store holds now.
        self.close();
        let registry = Registry::load(&self.shared.store());
        *self.shared.registry_mut() = registry;
        self.shared.publish();
        for (plugin, status) in self.shared.statuses() {
            self.shared.announce(Notice::Status { plugin, status });
        }
    }

    /// The runtime the work that takes seconds runs on, started on first use.
    fn background(&mut self) -> anyhow::Result<&crate::runtime::BackgroundRuntime> {
        if self.background.is_none() {
            self.background = Some(crate::runtime::BackgroundRuntime::new()?);
        }
        match &self.background {
            Some(runtime) => Ok(runtime),
            None => anyhow::bail!("the plugin system's runtime isn't there"),
        }
    }

    /// Every installed plugin's manifest, state and recent log, for the page
    /// that lists them.
    pub fn listed(&self) -> Vec<crate::plugins::registry::Listed> {
        self.shared.registry().listed()
    }

    /// What the helper process is doing, for the page that says so.
    pub fn helper(&self) -> HelperState {
        if self.shared.off().is_some() {
            return if self.shared.helper_installed() {
                HelperState::Crashed
            } else {
                HelperState::Missing
            };
        }
        match self.shared.helper_pid() {
            Some(pid) => HelperState::Running {
                memory: resident_memory(pid),
            },
            None => HelperState::Stopped,
        }
    }

    /// Why the plugin system is off, when it is: the page shows this, and it is
    /// `None` while the helper is running or simply not needed yet.
    pub fn off(&self) -> Option<String> {
        self.shared.off()
    }

    /// Stops the helper, if it is running. Called when the application exits.
    pub fn shutdown(&mut self) {
        self.close();
        self.shared.store().flush();
    }

    /// Sends one command to the supervisor, starting it if nothing is running.
    ///
    /// `false` means there is nobody to run it: no plugin is enabled, or the
    /// helper was given up on, and the caller has to answer for itself.
    fn send(&mut self, command: Request) -> bool {
        let Some(supervisor) = self.supervisor() else {
            return false;
        };
        if supervisor.commands.send(command).is_err() {
            // The task has ended, which it does only when it gave up or was
            // asked to stop; either way it has already said why.
            tracing::debug!("the plugin supervisor is no longer listening");
            return false;
        }
        true
    }

    /// The supervisor, started on first use: nothing about plugins runs until a
    /// plugin has something to do.
    fn supervisor(&mut self) -> Option<&Supervisor> {
        if !self.enabled() || self.shared.off().is_some() {
            self.supervisor = None;
            return None;
        }
        if self.supervisor.is_none() {
            let runtime = match crate::runtime::BackgroundRuntime::new() {
                Ok(runtime) => runtime,
                Err(error) => {
                    self.shared
                        .give_up(format!("plugins couldn't start their runtime: {error}"));
                    return None;
                }
            };
            let (commands, receiver) = tokio::sync::mpsc::unbounded_channel();
            let task = runtime.spawn(supervise(
                self.shared.clone(),
                receiver,
                self.data_dir.clone(),
            ));
            self.supervisor = Some(Supervisor {
                commands,
                shared: Arc::clone(&self.shared),
                task,
                runtime,
            });
        }
        self.supervisor.as_ref()
    }

    /// Stops the helper and waits for it, so the next start is a fresh process
    /// rather than a second one beside the first.
    fn close(&mut self) {
        // Kept whole rather than taken apart: dropping it is what forgets the
        // helper's process id, and every way out of this function goes through
        // that drop.
        let Some(mut supervisor) = self.supervisor.take() else {
            return;
        };
        if self.shared.off().is_some() {
            // Plugins are off, so there is no process to stop: dropping the task
            // is enough, and it holds only a channel.
            return;
        }
        let _ = supervisor.commands.send(Request::Shutdown);
        supervisor.runtime.block_on(async {
            match tokio::time::timeout(CLOSE_WAIT, &mut supervisor.task).await {
                Ok(_) => {}
                Err(_) => tracing::warn!("the plugin helper is still stopping; leaving it to exit"),
            }
        });
        // Whatever a plugin saved is written before the process that asked for
        // it is gone, so the last change is not lost to a stop.
        self.shared.store().flush();
    }

    /// Tells the page about one plugin's status, if there is anything to tell.
    fn announce(&self, plugin: &PluginId) {
        let status = self
            .shared
            .registry()
            .entry(plugin)
            .map(|entry| entry.status.clone());
        if let Some(status) = status {
            self.shared.announce(Notice::Status {
                plugin: plugin.clone(),
                status,
            });
        }
    }
}

impl Drop for Supervisor {
    /// Forgets the helper's process id: the task is what holds the process, so
    /// once this is dropped the process is ending, whether it was asked to or
    /// the task was aborted because nothing is enabled any more. The page would
    /// otherwise show a helper that isn't there.
    fn drop(&mut self) {
        self.shared.set_helper_pid(None);
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// What the helper process is doing, for the page that says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelperState {
    /// No process: nothing has asked for one, or what did has gone quiet.
    Stopped,
    /// Its resident memory in bytes, where this system can tell it.
    Running { memory: Option<u64> },
    /// The binary isn't beside Yapfast's, so nothing can start.
    Missing,
    /// It crashed three times in a minute, so plugins stay off until Yapfast
    /// starts again.
    Crashed,
}

/// How much memory a process has resident, in bytes.
///
/// Linux reports it in `/proc/<pid>/status`; elsewhere this doesn't read it,
/// and the page says the helper is running without a number.
fn resident_memory(pid: u32) -> Option<u64> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kilobytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kilobytes * 1024)
}

/// A fault's sentence, as a line in the plugin's own log: the row that says
/// "Suspended: kept crashing" has the why under it, and the detail's log keeps
/// it after the status has moved on.
fn failure(reason: &str) -> LogRecord {
    LogRecord {
        level: LogLevel::Error,
        target: "yapfast".to_owned(),
        message: reason.to_owned(),
        at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as u64),
    }
}

/// Notes a failure in the plugin's log, through the registry's ring.
fn note_failure(shared: &Shared, plugin: &PluginId, reason: &str) {
    shared.registry_mut().note_log(plugin, failure(reason));
}

/// A running supervisor task and the channel into it.
struct Supervisor {
    commands: tokio::sync::mpsc::UnboundedSender<Request>,
    /// The half both threads read, to say the process is gone when this is.
    shared: Arc<Shared>,
    /// Aborted when this value goes, which also drops the session and with it
    /// the helper's process.
    task: crate::runtime::BackgroundTask<()>,
    /// Kept alive with the task: dropping a runtime stops its threads, and the
    /// task needs them until the last frame is written.
    runtime: crate::runtime::BackgroundRuntime,
}

/// What both threads of the plugin system read: the application's thread and
/// the supervisor's.
pub(crate) struct Shared {
    registry: RwLock<Registry>,
    store: Mutex<Store>,
    /// The summary of [`Registry::summary`], kept as plain flags because the
    /// send and event paths read them on every message.
    enabled: AtomicBool,
    wanted: AtomicU64,
    intercepts: AtomicBool,
    /// Why plugins are off, when they are.
    off: Mutex<Option<String>>,
    /// The one place a plugin's HTTP request can be made, once the application
    /// has installed one with a transport.
    gate: Mutex<Option<Arc<crate::plugins::network::NetworkGate>>>,
    /// Whether the helper binary is where [`spawn::program`] looks for it.
    helper_installed: bool,
    /// The helper's process id while it runs, and 0 when none does.
    helper: AtomicU32,
    wake: Arc<dyn Fn(Notice) + Send + Sync>,
}

impl Shared {
    /// Copies the registry's summary into the flags the hot paths read.
    fn publish(&self) {
        let summary = self.registry().summary();
        self.enabled.store(summary.enabled, Ordering::Relaxed);
        self.wanted.store(summary.wanted, Ordering::Relaxed);
        self.intercepts.store(summary.intercepts, Ordering::Relaxed);
    }

    /// The registry for reading.
    ///
    /// A poisoned lock means some thread panicked while holding it; the registry
    /// is still the last consistent state, and refusing to read it would turn one
    /// panic into a plugin system that never works again.
    pub(crate) fn registry(&self) -> RwLockReadGuard<'_, Registry> {
        self.registry.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn registry_mut(&self) -> RwLockWriteGuard<'_, Registry> {
        self.registry
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn statuses(&self) -> Vec<(PluginId, PluginStatus)> {
        self.registry().statuses()
    }

    fn announce(&self, notice: Notice) {
        (self.wake)(notice);
    }

    /// Why plugins are off, when they are.
    fn off(&self) -> Option<String> {
        self.off
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Whether the helper's program is installed beside the app.
    fn helper_installed(&self) -> bool {
        self.helper_installed
    }

    /// The helper's process id, while it runs.
    fn helper_pid(&self) -> Option<u32> {
        match self.helper.load(Ordering::Relaxed) {
            0 => None,
            pid => Some(pid),
        }
    }

    /// Keeps the helper's process id, or forgets it when the process is gone.
    fn set_helper_pid(&self, pid: Option<u32>) {
        self.helper.store(pid.unwrap_or(0), Ordering::Relaxed);
    }

    /// Forgets why plugins were off, so the next trigger starts the helper again.
    fn clear_off(&self) {
        *self.off.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// The network gate, if the application installed one.
    fn gate(&self) -> Option<Arc<crate::plugins::network::NetworkGate>> {
        self.gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Tells the gate what a plugin's manifest declares, which is what every
    /// request from it is checked against.
    fn declare(&self, plugin: &PluginId) {
        let Some(gate) = self.gate() else {
            return;
        };
        let permissions = self
            .registry()
            .entry(plugin)
            .map(|entry| entry.manifest.permissions.clone());
        match permissions {
            Some(permissions) => gate.declare(plugin, permissions),
            // A plugin that is off has nothing to declare, and any grant it had
            // goes with it.
            None => gate.forget(plugin),
        }
    }

    /// Stops the plugin system with a reason the page can show.
    ///
    /// Every enabled plugin is marked suspended rather than installed: the
    /// person turned it on, and the page has to say why it isn't running instead
    /// of pretending nothing was asked for.
    fn give_up(&self, reason: String) {
        tracing::warn!(%reason, "plugins are off");
        *self.off.lock().unwrap_or_else(PoisonError::into_inner) = Some(reason.clone());
        let changed: Vec<PluginId> = {
            let mut registry = self.registry_mut();
            let changed: Vec<PluginId> = registry
                .statuses()
                .into_iter()
                .filter(|(_, status)| !matches!(status, PluginStatus::Installed))
                .map(|(plugin, _)| plugin)
                .collect();
            for plugin in &changed {
                registry.note_log(plugin, failure(&reason));
                registry.set_status(
                    plugin,
                    PluginStatus::Suspended {
                        reason: reason.clone(),
                    },
                );
            }
            changed
        };
        for plugin in changed {
            self.announce(Notice::Status {
                plugin,
                status: PluginStatus::Suspended {
                    reason: reason.clone(),
                },
            });
        }
        // The flags the send and event paths read say there is nothing to wait
        // for any more: without this, every message is still serialized and
        // every hook chain still built, for a helper that is gone.
        self.enabled.store(false, Ordering::Relaxed);
        self.wanted.store(0, Ordering::Relaxed);
        self.intercepts.store(false, Ordering::Relaxed);
    }
}

/// One event, serialized once and shared by every plugin that wants it.
///
/// The wire payload of an event batch embeds these bytes rather than the event's
/// own shape, so an event four plugins subscribed to is written once and copied
/// nowhere: what differs between the frames is the plugin they name.
#[derive(Clone)]
struct SharedEvent {
    json: Arc<[u8]>,
    kind: EventKind,
}

impl SharedEvent {
    /// Serializes an event once, for every plugin that wants it.
    fn new(event: EventEnvelope) -> Option<Arc<SharedEvent>> {
        match serde_json::to_vec(&event) {
            Ok(json) => Some(Arc::new(SharedEvent {
                json: Arc::from(json),
                kind: event.kind,
            })),
            Err(error) => {
                // Nothing inside an `EventEnvelope` can fail to serialize today;
                // if that changes, dropping the event beats waking a plugin with
                // a broken frame.
                tracing::warn!(%error, "an event couldn't be serialized");
                None
            }
        }
    }
}

/// The payload of an `Event` frame: a batch's events, around the plugin they are
/// for.
///
/// This is written by hand because the events are already bytes: serializing a
/// `ToHelper::Event` per plugin would copy every event once per subscriber, which
/// on a busy account is the difference between one serialization and four. The
/// test below reads it back with `serde_json` to keep it honest.
fn event_payload(plugin: &PluginId, events: &[Arc<SharedEvent>]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(
        64 + events
            .iter()
            .map(|event| event.json.len() + 1)
            .sum::<usize>(),
    );
    payload.extend_from_slice(b"{\"t\":\"event\",\"plugin\":");
    match serde_json::to_vec(plugin) {
        Ok(id) => payload.extend_from_slice(&id),
        Err(_) => payload.extend_from_slice(b"null"),
    }
    payload.extend_from_slice(b",\"events\":[");
    for (index, event) in events.iter().enumerate() {
        if index > 0 {
            payload.push(b',');
        }
        payload.extend_from_slice(&event.json);
    }
    payload.extend_from_slice(b"]}");
    payload
}

/// What the application's calls hand the supervisor.
enum Request {
    /// Load and activate these plugins.
    Activate { plugins: Vec<PluginId> },
    /// Drop one plugin's instance, keeping what it handed over.
    Deactivate { plugin: PluginId },
    /// One event for one subscribed plugin.
    Event {
        plugin: PluginId,
        event: Arc<SharedEvent>,
    },
    /// A send waiting on a hook chain.
    Hook {
        held: HeldSend,
        request: HookRequest,
    },
    /// A command a person typed.
    Command {
        plugin: PluginId,
        invocation: CommandInvocation,
    },
    /// Drop these instances now, for a reason the helper can't know.
    Evict { plugins: Vec<PluginId> },
    /// The application is going away.
    Shutdown,
}

/// A send that is waiting on a hook chain.
struct HeldSend {
    /// The composer's id for this send, echoed back to the application.
    id: u64,
    /// The draft as the person wrote it: what goes out if the chain fails.
    draft: Draft,
    hook: HookKind,
    /// The plugins the chain runs through, dependencies first.
    chain: Vec<PluginId>,
}

/// How a session ended.
enum Ending {
    /// The parent asked for it.
    Asked,
    /// Nothing was left to run, so the helper went, as it is meant to.
    Idle,
    /// It went when it shouldn't have.
    Gone(String),
}

/// Runs the helper, restarts it after a crash, and gives up when it keeps
/// crashing.
///
/// A process is started only when there is a command for it: with nothing to do,
/// this task holds a channel and no process at all.
async fn supervise(
    shared: Arc<Shared>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<Request>,
    data_dir: PathBuf,
) {
    let mut restarts = Restarts::new();
    loop {
        let first = match commands.recv().await {
            Some(command) => command,
            // The bridge is gone, which only happens as the application exits.
            None => return,
        };
        if matches!(first, Request::Shutdown) {
            return;
        }
        let mut session = match Session::start(&data_dir) {
            Ok(session) => session,
            Err(error) => {
                shared.give_up(format!("{error:#}"));
                stand_down(&shared, &mut commands).await;
                return;
            }
        };
        shared.set_helper_pid(Some(session.child.id()));
        session.queue(first);
        let ending = session.run(&shared, &mut commands).await;
        // A send that was waiting on this process has to go out as the person
        // wrote it: the chain it was waiting for no longer exists.
        session.fail_open(&shared);
        drop(session);
        shared.set_helper_pid(None);
        match ending {
            Ending::Asked => return,
            Ending::Idle => continue,
            Ending::Gone(reason) => match restarts.crashed(Instant::now()) {
                spawn::Next::After(wait) => {
                    tracing::warn!(
                        %reason,
                        wait_ms = wait.as_millis() as u64,
                        "the plugin helper stopped; starting it again"
                    );
                    tokio::time::sleep(wait).await;
                }
                spawn::Next::Off(reason) => {
                    shared.give_up(reason);
                    stand_down(&shared, &mut commands).await;
                    return;
                }
            },
        }
    }
}

/// Waits with the plugin system off, answering the sends that were already on
/// their way.
///
/// The bridge stops sending the moment it is told plugins are off, so nothing new
/// arrives here. What can arrive is a send that was between the application and
/// this task when the helper gave up: without an answer the composer would wait
/// for a chain that no longer exists, so it goes out as the person wrote it. The
/// task then holds nothing but a channel, and ends when the bridge does.
async fn stand_down(shared: &Shared, commands: &mut tokio::sync::mpsc::UnboundedReceiver<Request>) {
    while let Some(command) = commands.recv().await {
        if let Request::Hook { held, .. } = command {
            shared.announce(Notice::SendReady {
                id: held.id,
                draft: held.draft,
            });
        }
    }
}

/// One helper process, and the state of the conversation with it.
struct Session {
    child: Child,
    /// Frames to write. A thread of its own, so a helper that stops reading
    /// can't stop this task's timers from noticing.
    writer: std::sync::mpsc::Sender<Frame>,
    inbound: tokio::sync::mpsc::UnboundedReceiver<Inbound>,
    /// The thread reading the helper's stdout; ending it means the helper is
    /// gone.
    reader: Option<std::thread::JoinHandle<()>>,
    /// Pairs a request with its answer, counting up so a late answer can never be
    /// mistaken for a new request's.
    next_id: u64,
    pending: BTreeMap<u64, Pending>,
    /// The plugins whose module this process has been given.
    loaded: BTreeSet<PluginId>,
    /// The plugins it has activated.
    active: BTreeSet<PluginId>,
    /// Events waiting for their coalescing window, per plugin.
    batches: BTreeMap<PluginId, Batch>,
    /// The timers guests asked for, which live here so a guest can't hold
    /// memory in the helper with them.
    timers: BTreeMap<TimerKey, Timer>,
    /// Commands that arrived before the hello was answered.
    queued: VecDeque<Request>,
    /// Whether the parent asked the helper to stop, which makes its exit planned.
    asked: bool,
    /// What the helper said as it left, if it said anything.
    bye: Option<String>,
}

/// One frame from the helper, or the end of the pipe.
enum Inbound {
    Frame(Frame),
    /// The pipe closed, with what went wrong if it was a broken frame rather
    /// than a clean end.
    Closed(Option<String>),
}

/// What made the session's loop look up from waiting.
enum Waking {
    /// The helper sent a frame, or its pipe closed.
    Helper(Option<Inbound>),
    /// The application sent a command, or went away.
    Application(Option<Request>),
    /// A deadline came due: a timer, a batch, a request's budget or a save.
    Clock,
}

/// A request waiting for its answer.
struct Pending {
    deadline: Instant,
    purpose: Purpose,
}

/// What an answer is for.
enum Purpose {
    /// A request whose answer is only a report: a timeout is a fault to log.
    Agent {
        what: &'static str,
        plugin: PluginId,
        /// How long it was given, for the sentence a timeout writes.
        budget: Duration,
    },
    /// A hook chain: if it doesn't answer in time the message goes as it was.
    Hook(Box<HeldSend>),
}

/// A batch of events waiting for its coalescing window.
struct Batch {
    events: Vec<Arc<SharedEvent>>,
    bytes: usize,
    due: Instant,
}

/// Which plugin a timer belongs to, and the name the guest gave it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TimerKey {
    plugin: PluginId,
    tag: String,
}

/// A timer a guest asked for, which fires a call into it when it comes due.
struct Timer {
    interval: Duration,
    repeating: bool,
    due: Instant,
}

impl Session {
    /// Starts a helper and the threads that talk to it.
    fn start(data_dir: &Path) -> Result<Session> {
        let Helper {
            child,
            stdin,
            stdout,
        } = spawn::start(data_dir)?;
        let (frames, inbound) = tokio::sync::mpsc::unbounded_channel();
        let reader = std::thread::Builder::new()
            .name("yapfast-plugins-read".to_owned())
            .spawn(move || {
                let mut stdout = stdout;
                loop {
                    match frame::read_frame(&mut stdout) {
                        Ok(Some(frame)) => {
                            if frames.send(Inbound::Frame(frame)).is_err() {
                                return;
                            }
                        }
                        Ok(None) => {
                            let _ = frames.send(Inbound::Closed(None));
                            return;
                        }
                        Err(error) => {
                            let _ = frames.send(Inbound::Closed(Some(error.to_string())));
                            return;
                        }
                    }
                }
            })
            .context("could not start the thread reading the plugin helper")?;
        let (writer, writes) = std::sync::mpsc::channel::<Frame>();
        std::thread::Builder::new()
            .name("yapfast-plugins-write".to_owned())
            .spawn(move || {
                let mut stdin = stdin;
                for frame in writes {
                    if frame::write_frame(&mut stdin, &frame).is_err() {
                        return;
                    }
                }
            })
            .context("could not start the thread writing to the plugin helper")?;
        Ok(Session {
            child,
            writer,
            inbound,
            reader: Some(reader),
            next_id: 0,
            pending: BTreeMap::new(),
            loaded: BTreeSet::new(),
            active: BTreeSet::new(),
            batches: BTreeMap::new(),
            timers: BTreeMap::new(),
            queued: VecDeque::new(),
            asked: false,
            bye: None,
        })
    }

    /// Adds a command to the front of the queue, for one that arrived before the
    /// process did.
    fn queue(&mut self, command: Request) {
        self.queued.push_back(command);
    }

    /// Lets every send this session was holding go out unchanged.
    ///
    /// Called when the process ends, whatever the reason: a chain that can no
    /// longer answer must not keep the composer waiting for it.
    fn fail_open(&mut self, shared: &Shared) {
        let pending = std::mem::take(&mut self.pending);
        for pending in pending.into_values() {
            match pending.purpose {
                Purpose::Hook(held) => shared.announce(Notice::SendReady {
                    id: held.id,
                    draft: held.draft,
                }),
                Purpose::Agent { .. } => {}
            }
        }
    }

    /// Runs the conversation until the helper goes or the parent is done.
    async fn run(
        &mut self,
        shared: &Arc<Shared>,
        commands: &mut tokio::sync::mpsc::UnboundedReceiver<Request>,
    ) -> Ending {
        // Nothing else is sent before the helper has said which protocol it
        // speaks: a mismatch would otherwise show up as a timeout per request.
        self.send_message(&ToHelper::Hello { api: API_VERSION }, FrameKind::Hello);
        match tokio::time::timeout(HELLO_TIMEOUT, self.await_ready()).await {
            Ok(Ok(())) => {}
            Ok(Err(reason)) => return Ending::Gone(reason),
            Err(_) => return Ending::Gone("it didn't answer the hello".to_owned()),
        }
        loop {
            while let Some(command) = self.queued.pop_front() {
                if let Some(ending) = self.on_command(shared, command).await {
                    return ending;
                }
            }
            // What woke this loop is worked out before anything is done about it:
            // the `select!` futures hold a borrow of this session, and handling a
            // frame or a command needs it mutably.
            let deadline = self.next_deadline(shared);
            let waking = tokio::select! {
                biased;
                incoming = self.inbound.recv() => Waking::Helper(incoming),
                command = commands.recv() => Waking::Application(command),
                () = sleep_until(deadline) => Waking::Clock,
            };
            match waking {
                Waking::Helper(Some(Inbound::Frame(frame))) => {
                    if let Some(ending) = self.on_frame(shared, frame) {
                        return ending;
                    }
                }
                Waking::Helper(Some(Inbound::Closed(reason))) => return self.ending(reason),
                Waking::Helper(None) => return self.ending(None),
                Waking::Application(Some(command)) => {
                    if let Some(ending) = self.on_command(shared, command).await {
                        return ending;
                    }
                }
                // The bridge is gone: stop talking and let the process go.
                Waking::Application(None) => return Ending::Asked,
                Waking::Clock => self.on_tick(shared),
            }
        }
    }

    /// Waits for the helper's `Ready`, which is the answer to the hello.
    async fn await_ready(&mut self) -> Result<(), String> {
        if let Some(incoming) = self.inbound.recv().await {
            let frame = match incoming {
                Inbound::Frame(frame) => frame,
                Inbound::Closed(reason) => {
                    return Err(reason.unwrap_or_else(|| "it closed the pipe".to_owned()));
                }
            };
            // The contract doesn't pin which frame kind carries the greeting, so
            // any of the three is accepted as long as it is a `Ready`.
            match serde_json::from_slice::<FromHelper>(&frame.payload) {
                Ok(FromHelper::Ready { api, version }) => {
                    if api != API_VERSION {
                        return Err(format!(
                            "the helper speaks plugin API {api}, and this Yapfast speaks {API_VERSION}"
                        ));
                    }
                    tracing::debug!(%version, "the plugin helper is up");
                    return Ok(());
                }
                Ok(_) => return Err("it said something before the hello".to_owned()),
                Err(_) => return Err("its hello wasn't a message this build knows".to_owned()),
            }
        }
        Err("it closed the pipe before the hello".to_owned())
    }

    /// What ended this session, once the pipe has closed.
    ///
    /// A helper that goes with nothing outstanding has run out of work, which is
    /// how it is meant to end; one that goes with a request or an instance still
    /// live crashed, and is worth replacing.
    fn ending(&self, reason: Option<String>) -> Ending {
        if self.asked {
            return Ending::Asked;
        }
        if self.active.is_empty() && self.pending.is_empty() {
            return Ending::Idle;
        }
        match &self.bye {
            Some(bye) => Ending::Gone(bye.clone()),
            None => Ending::Gone(reason.unwrap_or_else(|| "it closed the pipe".to_owned())),
        }
    }

    /// The next moment this session has work to do, if it has any.
    fn next_deadline(&self, shared: &Shared) -> Option<Instant> {
        let pending = self.pending.values().map(|pending| pending.deadline).min();
        let batch = self.batches.values().map(|batch| batch.due).min();
        let timer = self.timers.values().map(|timer| timer.due).min();
        let save = shared.store().pending_save();
        [pending, batch, timer, save].into_iter().flatten().min()
    }

    /// Handles one command from the application.
    async fn on_command(&mut self, shared: &Arc<Shared>, command: Request) -> Option<Ending> {
        match command {
            Request::Activate { plugins } => {
                for plugin in plugins {
                    self.activate(shared, &plugin);
                }
                None
            }
            Request::Deactivate { plugin } => {
                self.deactivate(&plugin);
                None
            }
            Request::Event { plugin, event } => {
                self.deliver(shared, plugin, event);
                None
            }
            Request::Hook { held, request } => {
                self.hold(shared, held, request);
                None
            }
            Request::Command { plugin, invocation } => {
                self.command(shared, &plugin, invocation);
                None
            }
            Request::Evict { plugins } => {
                for plugin in plugins {
                    self.evict(&plugin);
                }
                None
            }
            Request::Shutdown => Some(self.stop().await),
        }
    }

    /// Loads and activates one plugin, if it isn't up already.
    ///
    /// Loading is what hands the module's path to the helper, so it happens here
    /// and only for a plugin this process hasn't been given: a restart therefore
    /// re-loads on the next trigger rather than eagerly.
    fn activate(&mut self, shared: &Shared, plugin: &PluginId) -> bool {
        if self.active.contains(plugin) {
            return true;
        }
        // A trigger can outlive the plugin it was for — a queued event, a
        // timer that fired a moment before the person turned it off. Starting
        // it here would run a plugin nothing says should run.
        if !shared
            .registry()
            .entry(plugin)
            .is_some_and(|entry| entry.enabled)
        {
            return false;
        }
        if !self.loaded.contains(plugin) {
            let Some(module) = self.ready_module(shared, plugin) else {
                return false;
            };
            let message = ToHelper::Load {
                plugin: plugin.clone(),
                wasm: module.path,
                memory_pages: module.memory_pages,
                wasi: module.wasi,
            };
            self.request(
                &message,
                LOAD_TIMEOUT,
                Purpose::Agent {
                    what: "load",
                    plugin: plugin.clone(),
                    budget: LOAD_TIMEOUT,
                },
            );
            // Marked as given even before the answer: the pipe is ordered, so
            // sending the activation behind the load is one round trip, and a
            // load that fails answers with a fault that clears this again.
            self.loaded.insert(plugin.clone());
        }
        let message = ToHelper::Activate {
            plugin: plugin.clone(),
        };
        self.request(
            &message,
            ACTIVATE_TIMEOUT,
            Purpose::Agent {
                what: "activation",
                plugin: plugin.clone(),
                budget: ACTIVATE_TIMEOUT,
            },
        );
        true
    }

    /// The module to hand the helper, if it still checks out.
    fn ready_module(&self, shared: &Shared, plugin: &PluginId) -> Option<Module> {
        let module = shared.registry().entry(plugin)?.module.clone()?;
        if let Err(reason) = shared.store().recheck(&module) {
            tracing::warn!(%plugin, %reason, "a plugin's module changed; not running it");
            note_failure(shared, plugin, &reason);
            let status = PluginStatus::Suspended {
                reason: reason.clone(),
            };
            shared.registry_mut().set_status(plugin, status.clone());
            shared.announce(Notice::Status {
                plugin: plugin.clone(),
                status,
            });
            return None;
        }
        Some(module)
    }

    /// Drops one plugin's instance, keeping what it hands over.
    fn deactivate(&mut self, plugin: &PluginId) {
        // Before the early return: a plugin that faulted or was evicted is off
        // already, and its timers still have to go.
        self.forget_timers(plugin);
        if !self.active.contains(plugin) {
            return;
        }
        let message = ToHelper::Deactivate {
            plugin: plugin.clone(),
        };
        self.request(
            &message,
            DEACTIVATE_TIMEOUT,
            Purpose::Agent {
                what: "deactivation",
                plugin: plugin.clone(),
                budget: DEACTIVATE_TIMEOUT,
            },
        );
    }

    /// Drops one plugin's instance now, without its say.
    fn evict(&mut self, plugin: &PluginId) {
        if !self.loaded.contains(plugin) {
            return;
        }
        let message = ToHelper::Evict {
            plugin: plugin.clone(),
        };
        self.request(
            &message,
            EVICT_TIMEOUT,
            Purpose::Agent {
                what: "eviction",
                plugin: plugin.clone(),
                budget: EVICT_TIMEOUT,
            },
        );
    }

    /// Delivers one event, or holds it for the coalescing window.
    fn deliver(&mut self, shared: &Arc<Shared>, plugin: PluginId, event: Arc<SharedEvent>) {
        if !event.kind.is_coalesced() {
            // An event is only worth sending to an instance that exists: a
            // plugin whose module doesn't check out hears about nothing.
            if self.activate(shared, &plugin) {
                let payload = event_payload(&plugin, std::slice::from_ref(&event));
                self.write(FrameKind::Event, 0, payload);
            }
            return;
        }
        let full = {
            let batch = self.batches.entry(plugin.clone()).or_insert_with(|| Batch {
                events: Vec::new(),
                bytes: 0,
                due: Instant::now() + Duration::from_millis(COALESCE_MS),
            });
            batch.bytes += event.json.len();
            batch.events.push(event);
            batch.events.len() >= BATCH_MAX_EVENTS || batch.bytes >= BATCH_MAX_BYTES
        };
        if full {
            self.flush_batch(shared, &plugin);
        }
    }

    /// Sends one plugin's held events, as one call into it.
    fn flush_batch(&mut self, shared: &Arc<Shared>, plugin: &PluginId) {
        let Some(batch) = self.batches.remove(plugin) else {
            return;
        };
        if batch.events.is_empty() || !self.activate(shared, plugin) {
            return;
        }
        let payload = event_payload(plugin, &batch.events);
        self.write(FrameKind::Event, 0, payload);
    }

    /// Runs a hook chain, or holds the send until it answers.
    fn hold(&mut self, shared: &Arc<Shared>, mut held: HeldSend, request: HookRequest) {
        // Frames are handled in order, so loading each plugin in the chain first
        // means the chain arrives at instances that are already up. A plugin
        // that can't be loaded leaves the chain rather than holding the send.
        held.chain.retain(|plugin| self.activate(shared, plugin));
        if held.chain.is_empty() {
            shared.announce(Notice::SendReady {
                id: held.id,
                draft: held.draft,
            });
            return;
        }
        let budget = request.hook.budget();
        let budget = Duration::from_millis(budget.chain_ms.unwrap_or(budget.per_plugin_ms));
        let message = ToHelper::Hook {
            chain: held.chain.clone(),
            request,
        };
        self.request(&message, budget, Purpose::Hook(Box::new(held)));
    }

    /// Runs a command a person typed.
    fn command(&mut self, shared: &Arc<Shared>, plugin: &PluginId, invocation: CommandInvocation) {
        self.activate(shared, plugin);
        let message = ToHelper::Command {
            plugin: plugin.clone(),
            command: invocation,
        };
        self.request(
            &message,
            COMMAND_TIMEOUT,
            Purpose::Agent {
                what: "command",
                plugin: plugin.clone(),
                budget: COMMAND_TIMEOUT,
            },
        );
    }

    /// Handles one frame from the helper.
    fn on_frame(&mut self, shared: &Arc<Shared>, frame: Frame) -> Option<Ending> {
        let message: FromHelper = match serde_json::from_slice(&frame.payload) {
            Ok(message) => message,
            Err(error) => {
                // A frame this build can't read is a version skew the hello
                // should have caught; it is dropped rather than fatal, so the
                // rest of the plugins keep working.
                tracing::warn!(%error, kind = frame.kind as u8, "the helper sent a message this build doesn't know");
                return None;
            }
        };
        match message {
            FromHelper::Ready { api, version } => {
                tracing::debug!(api, %version, "the plugin helper greeted twice");
                None
            }
            FromHelper::Call {
                plugin,
                call,
                request,
            } => {
                // The number the guest was told to expect its answer under, not
                // the frame's: only the helper's runtime knows which call it is
                // answering, so it travels in the message.
                self.on_call(shared, call, plugin, request);
                None
            }
            FromHelper::Activated { plugin, .. } => {
                self.active.insert(plugin.clone());
                let status = PluginStatus::Active;
                shared.registry_mut().set_status(&plugin, status.clone());
                shared.announce(Notice::Status { plugin, status });
                None
            }
            FromHelper::Deactivated { plugin, .. } => {
                self.active.remove(&plugin);
                let status = PluginStatus::Enabled;
                shared.registry_mut().set_status(&plugin, status.clone());
                shared.announce(Notice::Status { plugin, status });
                None
            }
            FromHelper::HookDone { outcome, failures } => {
                self.on_hook_done(shared, frame.id, outcome, failures);
                None
            }
            FromHelper::CommandDone { plugin, outcome } => {
                self.on_command_done(shared, &plugin, outcome);
                None
            }
            FromHelper::EventDone { .. } => None,
            FromHelper::Status { plugin, status } => {
                if !matches!(status, PluginStatus::Active) {
                    self.active.remove(&plugin);
                }
                shared.registry_mut().set_status(&plugin, status.clone());
                shared.announce(Notice::Status { plugin, status });
                None
            }
            FromHelper::Fault { plugin, fault } => {
                self.on_fault(shared, plugin, fault);
                None
            }
            FromHelper::Log { plugin, record } => {
                shared.registry_mut().note_log(&plugin, record);
                None
            }
            FromHelper::Bye { reason } => {
                tracing::info!(%reason, "the plugin helper is exiting");
                self.bye = Some(reason.clone());
                Some(self.ending(Some(reason)))
            }
        }
    }

    /// Answers a guest's host call.
    ///
    /// `call` is the guest's own number for it, which the runtime handed out
    /// when `yf_call` answered `pending`: the answer goes back under it, and the
    /// runtime delivers it to the guest in its next event batch.
    ///
    /// `http.fetch` goes beside the conversation instead of inside it: a plugin
    /// waiting on the network must not stop events reaching the others.
    fn on_call(&mut self, shared: &Arc<Shared>, call: u64, plugin: PluginId, request: HostCall) {
        if request.method == call::method::HTTP_FETCH {
            self.fetch(shared, call, plugin, request);
            return;
        }
        let answer = self.answer(shared, &plugin, request);
        self.reply(&plugin, call, answer);
    }

    /// Answers a host call that needs no waiting.
    fn answer(&mut self, shared: &Shared, plugin: &PluginId, call: HostCall) -> HostReturn {
        let registry = shared.registry();
        let Some(entry) = registry.entry(plugin) else {
            return HostReturn::Error {
                message: format!("{plugin} isn't installed"),
            };
        };
        let permissions = &entry.manifest.permissions;
        let storage_cap = entry.manifest.limits.storage_bytes;
        let method = call.method.as_str();
        let capability = match method {
            call::method::STORAGE_GET
            | call::method::STORAGE_SET
            | call::method::STORAGE_DELETE
            | call::method::STORAGE_LIST => Some(Capability::Storage),
            _ => None,
        };
        if let Some(capability) = capability
            && let Err(denial) = crate::plugins::permissions::check(permissions, capability)
        {
            return HostReturn::Denied {
                reason: denial.reason().to_owned(),
            };
        }
        match method {
            call::method::STORAGE_GET => {
                let request: StorageGet = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                let value = shared.store().storage_get(plugin, &request.key);
                let value = value.unwrap_or(serde_json::Value::Null);
                // A value can be as large as the plugin's whole quota, and one
                // answer is one frame: the guest is told rather than handed
                // nothing, which would read as an absent key.
                let bytes = value.to_string().len();
                if bytes > yapfast_plugin_api::frame::MAX_FRAME - 4096 {
                    return HostReturn::Error {
                        message: format!(
                            "`{}` is {} bytes, and one answer carries at most {}: \
                             keep a value under that, or split it across keys",
                            request.key,
                            bytes,
                            yapfast_plugin_api::frame::MAX_FRAME - 4096
                        ),
                    };
                }
                HostReturn::Ok(value)
            }
            call::method::STORAGE_SET => {
                let request: StorageSet = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                match shared
                    .store()
                    .storage_set(plugin, &request.key, request.value, storage_cap)
                {
                    Ok(()) => HostReturn::Ok(serde_json::Value::Bool(true)),
                    Err(reason) => HostReturn::Denied { reason },
                }
            }
            call::method::STORAGE_DELETE => {
                let request: StorageGet = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                match shared.store().storage_delete(plugin, &request.key) {
                    Ok(()) => HostReturn::Ok(serde_json::Value::Bool(true)),
                    Err(reason) => HostReturn::Denied { reason },
                }
            }
            call::method::STORAGE_LIST => {
                let request: StorageList = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                let keys = shared.store().storage_list(plugin, &request.prefix);
                match serde_json::to_value(keys) {
                    Ok(value) => HostReturn::Ok(value),
                    Err(error) => HostReturn::Error {
                        message: format!("could not answer `{method}`: {error}"),
                    },
                }
            }
            call::method::SETTINGS_GET => {
                let request: StorageGet = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                let settings = shared.store().settings(plugin, &entry.manifest);
                match settings.get(&request.key) {
                    // A setting the manifest doesn't declare has no answer to
                    // give: an empty one would look like a value the person set.
                    Some(value) => HostReturn::Ok(value.clone()),
                    None => HostReturn::Error {
                        message: format!("{plugin} declares no setting named `{}`", request.key),
                    },
                }
            }
            call::method::TIMERS_AFTER | call::method::TIMERS_EVERY => {
                let request: TimerRequest = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                self.set_timer(plugin, request, method == call::method::TIMERS_EVERY)
            }
            call::method::TIMERS_CANCEL => {
                let request: TimerCancel = match call.parse() {
                    Ok(request) => request,
                    Err(error) => return wrong_payload(method, error),
                };
                let key = TimerKey {
                    plugin: plugin.clone(),
                    tag: request.tag,
                };
                HostReturn::Ok(serde_json::Value::Bool(self.timers.remove(&key).is_some()))
            }
            // Everything else is named in the contract so a plugin can
            // feature-detect it, and answers so rather than failing to link.
            _ => HostReturn::Unsupported,
        }
    }

    /// Registers a timer, which fires a call into the plugin when it comes due.
    ///
    /// The answer is the registration rather than a `Pending` one, because the
    /// parent has no frame that carries a late answer back to a guest: the timer
    /// fires as a `Kick`, which the guest cannot tell apart from any other
    /// arrival. A plugin that needs to know which timer went off has to keep its
    /// own count.
    fn set_timer(
        &mut self,
        plugin: &PluginId,
        request: TimerRequest,
        repeating: bool,
    ) -> HostReturn {
        let held = self
            .timers
            .keys()
            .filter(|key| key.plugin == *plugin)
            .count();
        if held >= limits::MAX_TIMERS {
            return HostReturn::Denied {
                reason: format!("{plugin} already holds {} timers", limits::MAX_TIMERS),
            };
        }
        let interval =
            Duration::from_millis(request.interval_ms.max(limits::MIN_TIMER_INTERVAL_MS));
        let key = TimerKey {
            plugin: plugin.clone(),
            tag: request.tag.clone(),
        };
        self.timers.insert(
            key,
            Timer {
                interval,
                repeating,
                due: Instant::now() + interval,
            },
        );
        HostReturn::Ok(serde_json::json!({
            "tag": request.tag,
            "interval_ms": interval.as_millis() as u64,
            "repeating": repeating,
        }))
    }

    /// Runs an `http.fetch` beside the conversation.
    ///
    /// The guest's request goes through the network gate, which holds the host
    /// list the manifest declares and the consent that widens it. The guest never
    /// sees a socket, and the check is made again on every hop of a redirect.
    fn fetch(&mut self, shared: &Arc<Shared>, id: u64, plugin: PluginId, call: HostCall) {
        let (permissions, response_cap) = match shared.registry().entry(&plugin) {
            Some(entry) => (
                entry.manifest.permissions.clone(),
                entry.manifest.limits.http_response_bytes,
            ),
            None => {
                self.reply(
                    &plugin,
                    id,
                    HostReturn::Error {
                        message: format!("{plugin} isn't installed"),
                    },
                );
                return;
            }
        };
        // `network` reaches anywhere the gate allows and `network.local` only
        // loopback: a plugin that asks for the second alone is asking for less,
        // and the gate below is what decides which hosts are reachable.
        let may_fetch =
            permissions.allows(Capability::Network) || permissions.allows(Capability::NetworkLocal);
        if !may_fetch
            && let Err(denial) =
                crate::plugins::permissions::check(&permissions, Capability::Network)
        {
            self.reply(
                &plugin,
                id,
                HostReturn::Denied {
                    reason: denial.reason().to_owned(),
                },
            );
            return;
        }
        let request: HttpRequest = match call.parse() {
            Ok(request) => request,
            Err(error) => {
                self.reply(&plugin, id, wrong_payload(call::method::HTTP_FETCH, error));
                return;
            }
        };
        // A build with no transport can't make the request. The contract's
        // `unsupported` is the answer that lets a plugin feature-detect it rather
        // than wait for a result that will never come.
        let Some(gate) = shared.gate() else {
            self.reply(&plugin, id, HostReturn::Unsupported);
            return;
        };
        let writer = self.writer.clone();
        // The task outlives this call, so it owns what it reports through
        // rather than borrowing a reference the caller is about to drop.
        let shared = Arc::clone(shared);
        tokio::spawn(async move {
            let answer = match gate.fetch(&plugin, request, response_cap).await {
                Ok(crate::plugins::network::Outcome::Done(answer)) => {
                    match serde_json::to_value(&answer.response) {
                        Ok(value) => HostReturn::Ok(value),
                        Err(error) => HostReturn::Error {
                            message: format!("could not answer `http.fetch`: {error}"),
                        },
                    }
                }
                // The gate asks rather than holds the request open: the person
                // answers about the host, and the plugin asks again.
                Ok(crate::plugins::network::Outcome::AwaitingConsent { host }) => {
                    // The person is asked by a page rather than by a dialog
                    // this thread holds open, so the ask travels as a notice.
                    shared.announce(Notice::Consent {
                        plugin: plugin.clone(),
                        host: host.to_string(),
                    });
                    HostReturn::Denied {
                        reason: format!(
                            "{plugin} wants to reach {host}, which you haven't allowed yet"
                        ),
                    }
                }
                Err(error) => HostReturn::Denied {
                    reason: format!("{error:#}"),
                },
            };
            // The answer travels under the id the guest asked with, so the
            // helper can hand it to the call that is waiting for it.
            match serde_json::to_vec(&ToHelper::Return {
                plugin,
                call: id,
                answer,
            }) {
                Ok(payload) => {
                    let _ = writer.send(Frame::new(FrameKind::Reply, id, payload));
                }
                Err(error) => tracing::warn!(%error, "could not answer a fetch"),
            }
        });
    }

    /// Answers a guest's call under the id it asked with.
    fn reply(&self, plugin: &PluginId, id: u64, answer: HostReturn) {
        let message = ToHelper::Return {
            plugin: plugin.clone(),
            call: id,
            answer,
        };
        match serde_json::to_vec(&message) {
            Ok(payload) => self.write(FrameKind::Reply, id, payload),
            Err(error) => tracing::warn!(%error, "could not answer a plugin's call"),
        }
    }

    /// Reports a chain's answer to the application, or fails open.
    fn on_hook_done(
        &mut self,
        shared: &Arc<Shared>,
        id: u64,
        outcome: HookOutcome,
        failures: Vec<(PluginId, Fault)>,
    ) {
        for (plugin, fault) in failures {
            tracing::warn!(%plugin, reason = %fault.reason, kind = ?fault.kind, "a plugin failed in a hook chain");
        }
        let Some(pending) = self.pending.remove(&id) else {
            // The chain answered after its deadline, and the send has already
            // gone: nothing to do but say so.
            tracing::debug!("a hook chain answered after its budget");
            return;
        };
        let Purpose::Hook(held) = pending.purpose else {
            return;
        };
        match outcome {
            HookOutcome::Block { reason } => shared.announce(Notice::SendBlocked {
                id: held.id,
                reason,
            }),
            HookOutcome::Changed(HookSubject::Draft(draft)) => {
                shared.announce(Notice::SendReady { id: held.id, draft });
            }
            HookOutcome::Changed(_) => {
                // A pre-send chain answers about the draft it was given; another
                // subject means a plugin answered the wrong question, and the
                // message goes as the person wrote it.
                tracing::warn!("a pre-send chain answered about something else");
                shared.announce(Notice::SendReady {
                    id: held.id,
                    draft: held.draft,
                });
            }
            HookOutcome::Unchanged => shared.announce(Notice::SendReady {
                id: held.id,
                draft: held.draft,
            }),
        }
    }

    /// Reports a command's answer: a line for the person, or the reason it
    /// failed, on the application's own event path.
    fn on_command_done(&self, shared: &Arc<Shared>, plugin: &PluginId, outcome: CommandOutcome) {
        match outcome {
            CommandOutcome::None => {}
            CommandOutcome::Notice { text } => {
                shared.announce(Notice::Command {
                    plugin: plugin.clone(),
                    text,
                });
            }
            CommandOutcome::Value(value) => {
                tracing::info!(%plugin, %value, "a plugin command answered with a value");
            }
            // A command that failed says why where the person typed it: a
            // helper's log line is not where they look.
            CommandOutcome::Failed { message } => {
                shared.announce(Notice::Command {
                    plugin: plugin.clone(),
                    text: message,
                });
            }
        }
    }

    /// Records a fault and stops routing work to a plugin that failed.
    fn on_fault(&mut self, shared: &Arc<Shared>, plugin: PluginId, fault: Fault) {
        tracing::warn!(%plugin, reason = %fault.reason, kind = ?fault.kind, "a plugin faulted");
        self.active.remove(&plugin);
        if matches!(
            fault.kind,
            yapfast_plugin_api::message::FaultKind::BadModule
        ) {
            // A module that didn't instantiate has to be handed over again
            // before the next trigger can try it.
            self.loaded.remove(&plugin);
        }
        let reason = fault.reason;
        note_failure(shared, &plugin, &reason);
        let status = PluginStatus::Faulted {
            reason,
            memory_pages: 0,
        };
        shared.registry_mut().set_status(&plugin, status.clone());
        shared.announce(Notice::Status { plugin, status });
    }

    /// Does the work whose moment has come: timers, batches, timeouts, saves.
    fn on_tick(&mut self, shared: &Arc<Shared>) {
        let now = Instant::now();
        let expired: Vec<u64> = self
            .pending
            .iter()
            .filter(|(_, pending)| pending.deadline <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            self.expire(shared, id);
        }
        let due: Vec<PluginId> = self
            .batches
            .iter()
            .filter(|(_, batch)| batch.due <= now)
            .map(|(plugin, _)| plugin.clone())
            .collect();
        for plugin in due {
            self.flush_batch(shared, &plugin);
        }
        let fired: Vec<TimerKey> = self
            .timers
            .iter()
            .filter(|(_, timer)| timer.due <= now)
            .map(|(key, _)| key.clone())
            .collect();
        for key in fired {
            self.fire(shared, key);
        }
        shared.store().flush_due(now);
    }

    /// Gives up on a request that ran out of time.
    fn expire(&mut self, shared: &Arc<Shared>, id: u64) {
        let Some(pending) = self.pending.remove(&id) else {
            return;
        };
        match pending.purpose {
            Purpose::Hook(held) => {
                // Fail open, as the contract says: a plugin that runs late
                // leaves the message as the person wrote it.
                tracing::warn!(
                    chain = held.chain.len(),
                    hook = %held.hook,
                    "a hook chain ran out of time; the message goes unchanged"
                );
                shared.announce(Notice::SendReady {
                    id: held.id,
                    draft: held.draft,
                });
            }
            Purpose::Agent {
                what,
                plugin,
                budget,
            } => {
                tracing::warn!(%plugin, what, "the helper didn't answer in time");
                let reason = format!(
                    "the helper didn't answer its {what} request within {} s",
                    budget.as_secs_f32()
                );
                note_failure(shared, &plugin, &reason);
                let status = PluginStatus::Faulted {
                    reason,
                    memory_pages: 0,
                };
                shared.registry_mut().set_status(&plugin, status.clone());
                shared.announce(Notice::Status { plugin, status });
            }
        }
    }

    /// Drops every timer a plugin holds.
    ///
    /// A repeating timer is the plugin asking to be called again on its own
    /// initiative, so it lives exactly as long as the plugin is on: leaving one
    /// behind would wake a plugin the person turned off.
    fn forget_timers(&mut self, plugin: &PluginId) {
        self.timers.retain(|key, _| key.plugin != *plugin);
    }

    /// Fires one timer, as a call into its plugin with nothing new.
    fn fire(&mut self, shared: &Arc<Shared>, key: TimerKey) {
        let Some(timer) = self.timers.get_mut(&key) else {
            return;
        };
        let interval = timer.interval;
        if timer.repeating {
            timer.due = Instant::now() + interval;
        } else {
            self.timers.remove(&key);
        }
        if !self.activate(shared, &key.plugin) {
            return;
        }
        let message = ToHelper::Kick {
            plugin: key.plugin.clone(),
        };
        self.request(
            &message,
            KICK_TIMEOUT,
            Purpose::Agent {
                what: "timer",
                plugin: key.plugin,
                budget: TIMER_TIMEOUT,
            },
        );
    }

    /// Sends a request and remembers what its answer is for.
    fn request(&mut self, message: &ToHelper, budget: Duration, purpose: Purpose) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        match serde_json::to_vec(message) {
            Ok(payload) => {
                self.pending.insert(
                    id,
                    Pending {
                        deadline: Instant::now() + budget,
                        purpose,
                    },
                );
                self.write(FrameKind::Request, id, payload);
            }
            Err(error) => tracing::warn!(%error, "a plugin message couldn't be serialized"),
        }
        id
    }

    /// Sends a message that isn't paired with an answer.
    fn send_message(&mut self, message: &ToHelper, kind: FrameKind) {
        match serde_json::to_vec(message) {
            Ok(payload) => self.write(kind, 0, payload),
            Err(error) => tracing::warn!(%error, "a plugin message couldn't be serialized"),
        }
    }

    /// Hands one frame to the thread that writes it.
    ///
    /// Left unqueued rather than awaited: the helper may be slow to read, and a
    /// task that waited on it couldn't run the timers that are supposed to notice
    /// that.
    fn write(&self, kind: FrameKind, id: u64, payload: Vec<u8>) {
        if self.writer.send(Frame::new(kind, id, payload)).is_err() {
            tracing::debug!("the plugin helper is no longer being written to");
        }
    }

    /// Sends `Shutdown` and waits for the process to go, killing it if it
    /// lingers.
    async fn stop(&mut self) -> Ending {
        self.asked = true;
        self.send_message(&ToHelper::Shutdown, FrameKind::Request);
        let deadline = Instant::now() + SHUTDOWN_WAIT;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Ok(None) => {
                    tracing::warn!("the plugin helper didn't stop; ending it");
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
                Err(error) => {
                    tracing::warn!(%error, "could not wait for the plugin helper");
                    break;
                }
            }
        }
        Ending::Asked
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // A session that ends without a clean stop, such as one aborted while the
        // application exits, must not leave the process behind.
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

/// Waits until `deadline`, or for ever when there is nothing due.
async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
        None => std::future::pending().await,
    }
}

/// The answer to a payload that wasn't the shape its method defines.
fn wrong_payload(method: &str, error: serde_json::Error) -> HostReturn {
    HostReturn::Error {
        message: format!("`{method}` was given a payload it can't read: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_plugin_api::PluginManifest;
    use yapfast_plugin_api::event::Network;

    fn id(text: &str) -> PluginId {
        PluginId::parse(text).expect("a plugin id")
    }

    fn event(kind: EventKind, body: &str) -> EventEnvelope {
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

    #[test]
    fn an_event_batch_frame_reads_back_as_the_contract_s_message() {
        let plugin = id("com.example.batch");
        let first = event(EventKind::Typing, "one");
        let second = event(EventKind::Typing, "two");
        let events: Vec<Arc<SharedEvent>> = [first.clone(), second.clone()]
            .into_iter()
            .map(|event| SharedEvent::new(event).expect("it serializes"))
            .collect();
        let payload = event_payload(&plugin, &events);
        let read: ToHelper = serde_json::from_slice(&payload).expect("the payload is a message");
        assert_eq!(
            read,
            ToHelper::Event {
                plugin,
                events: vec![first, second],
            }
        );
    }

    #[test]
    fn a_bridge_with_nothing_installed_costs_nothing() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = woken.clone();
        let mut bridge = Bridge::open(
            directory.path(),
            Arc::new(move |_| {
                counted.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .expect("a bridge");
        assert!(!bridge.enabled());
        assert_eq!(bridge.off(), None);
        assert!(bridge.statuses().is_empty());
        assert_eq!(
            woken.load(Ordering::Relaxed),
            0,
            "opening the bridge tells the page nothing"
        );
        // An event nobody subscribed to is dropped without touching a plugin.
        bridge.observe(event(EventKind::MessageCreate, "hi"));
        let draft = Draft {
            room: "room-1".to_owned(),
            body: "hello".to_owned(),
            reply_to: None,
            attachments: Vec::new(),
        };
        match bridge.pre_send(draft.clone()) {
            PreSend::Send(sent) => assert_eq!(sent, draft),
            PreSend::Held { .. } => panic!("nothing intercepts sends"),
        }
        bridge.shutdown();
    }

    #[test]
    fn a_missing_helper_is_reported_rather_than_retried() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let mut bridge = Bridge::open(directory.path(), Arc::new(|_| {})).expect("a bridge");
        // Nothing is enabled, so nothing is started and nothing is claimed.
        bridge.start();
        assert!(bridge.off().is_none(), "no plugin asked for the helper");
    }

    #[test]
    fn timers_are_capped_and_short_ones_are_lengthened() {
        let (_directory, mut session, _written, _inbound) = session_stub();
        let plugin = id("com.example.timer");
        let answer = session.set_timer(
            &plugin,
            TimerRequest {
                interval_ms: 1,
                tag: "tick".to_owned(),
            },
            true,
        );
        let HostReturn::Ok(value) = answer else {
            panic!("a timer is registered");
        };
        assert_eq!(value["interval_ms"], limits::MIN_TIMER_INTERVAL_MS);
        for index in 0..(limits::MAX_TIMERS - 1) {
            session.set_timer(
                &plugin,
                TimerRequest {
                    interval_ms: 1_000,
                    tag: format!("t{index}"),
                },
                false,
            );
        }
        let refused = session.set_timer(
            &plugin,
            TimerRequest {
                interval_ms: 1_000,
                tag: "one-too-many".to_owned(),
            },
            false,
        );
        assert!(matches!(refused, HostReturn::Denied { .. }));
    }

    /// A session around a process that does nothing, with the frames it writes
    /// and the frames it would read held by the test.
    fn session_stub() -> (
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
                timers: BTreeMap::new(),
                queued: VecDeque::new(),
                asked: false,
                bye: None,
            },
            written,
            frames,
        )
    }

    /// Reads one frame the session wrote, which must be there.
    fn next_frame(written: &std::sync::mpsc::Receiver<Frame>) -> Frame {
        written
            .recv_timeout(Duration::from_secs(2))
            .expect("the session should have written a frame")
    }

    /// The message inside a frame the session wrote.
    fn message(frame: &Frame) -> ToHelper {
        serde_json::from_slice(&frame.payload).expect("the frame carries a message")
    }

    /// A data directory with one installed plugin whose module is `wasm`.
    ///
    /// The lock file's digest is the module's own, so a test starts from a store
    /// a real install would have produced.
    fn installed(directory: &Path, manifest: &str, wasm: &[u8]) -> PluginId {
        let sha = {
            use sha2::Digest;
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

    /// The plugin system's shared half around a data directory, as the bridge
    /// builds it.
    fn shared(directory: &Path, wake: Arc<dyn Fn(Notice) + Send + Sync>) -> Arc<Shared> {
        let store = Store::open(directory).expect("a store");
        let registry = Registry::load(&store);
        Arc::new(Shared {
            registry: RwLock::new(registry),
            store: Mutex::new(store),
            enabled: AtomicBool::new(false),
            wanted: AtomicU64::new(0),
            intercepts: AtomicBool::new(false),
            off: Mutex::new(None),
            gate: Mutex::new(None),
            helper_installed: false,
            helper: AtomicU32::new(0),
            wake,
        })
    }

    /// Where the bridge's notices arrive, in place of the application's event.
    fn notice_channel() -> (
        Arc<dyn Fn(Notice) + Send + Sync>,
        std::sync::mpsc::Receiver<Notice>,
    ) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let wake = Arc::new(move |notice: Notice| {
            let _ = sender.send(notice);
        });
        (wake, receiver)
    }

    fn runtime() -> crate::runtime::BackgroundRuntime {
        crate::runtime::BackgroundRuntime::new().expect("a runtime for the test")
    }

    /// A manifest for a plugin that intercepts sends, keeps storage and listens
    /// to typing.
    const HOOK_MANIFEST: &str = r#"{"api":1,"id":"com.example.hook","name":"Hook","version":"1.0.0",
        "wasm":"plugin.wasm",
        "permissions":{"capabilities":["messages.intercept","storage","messages.read"]},
        "activation":{"on":["event:typing"]}}"#;

    fn draft(body: &str) -> Draft {
        Draft {
            room: "room-1".to_owned(),
            body: body.to_owned(),
            reply_to: None,
            attachments: Vec::new(),
        }
    }

    #[test]
    fn a_held_send_loads_the_chain_and_goes_out_with_its_answer() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
        let (wake, notices) = notice_channel();
        let shared = shared(directory.path(), wake);
        let (_session_directory, mut session, written, _inbound) = session_stub();

        let held = HeldSend {
            id: 4,
            draft: draft("hi"),
            hook: HookKind::MessagePreSend,
            chain: vec![plugin.clone()],
        };
        let request = HookRequest {
            hook: HookKind::MessagePreSend,
            subject: HookSubject::Draft(draft("hi")),
        };
        runtime().block_on(session.on_command(&shared, Request::Hook { held, request }));

        // The module is handed over first, then the instance is started, and
        // only then does the chain see the draft: frames are handled in order.
        let load = next_frame(&written);
        assert!(
            matches!(message(&load), ToHelper::Load { plugin: loaded, .. } if loaded == plugin)
        );
        let activate = next_frame(&written);
        assert!(
            matches!(message(&activate), ToHelper::Activate { plugin: started } if started == plugin)
        );
        let hook = next_frame(&written);
        let ToHelper::Hook { chain, .. } = message(&hook) else {
            panic!("the third frame should be the chain");
        };
        assert_eq!(chain, vec![plugin]);

        let changed = HookSubject::Draft(draft("HELLO"));
        let answer = Frame::new(
            FrameKind::Report,
            hook.id,
            serde_json::to_vec(&FromHelper::HookDone {
                outcome: HookOutcome::Changed(changed),
                failures: Vec::new(),
            })
            .expect("it serializes"),
        );
        assert!(session.on_frame(&shared, answer).is_none());
        assert_eq!(
            notices
                .recv_timeout(Duration::from_secs(2))
                .expect("a notice"),
            Notice::SendReady {
                id: 4,
                draft: draft("HELLO"),
            }
        );
    }

    #[test]
    fn a_blocked_send_says_why_and_a_failed_chain_lets_it_through() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
        let (wake, notices) = notice_channel();
        let shared = shared(directory.path(), wake);
        let (_session_directory, mut session, written, _inbound) = session_stub();

        let held = HeldSend {
            id: 9,
            draft: draft("hi"),
            hook: HookKind::MessagePreSend,
            chain: vec![plugin.clone()],
        };
        let request = HookRequest {
            hook: HookKind::MessagePreSend,
            subject: HookSubject::Draft(draft("hi")),
        };
        runtime().block_on(session.on_command(&shared, Request::Hook { held, request }));
        let _load = next_frame(&written);
        let _activate = next_frame(&written);
        let hook = next_frame(&written);
        let block = Frame::new(
            FrameKind::Report,
            hook.id,
            serde_json::to_vec(&FromHelper::HookDone {
                outcome: HookOutcome::Block {
                    reason: "that word isn't allowed here".to_owned(),
                },
                failures: Vec::new(),
            })
            .expect("it serializes"),
        );
        assert!(session.on_frame(&shared, block).is_none());
        assert_eq!(
            notices
                .recv_timeout(Duration::from_secs(2))
                .expect("a notice"),
            Notice::SendBlocked {
                id: 9,
                reason: "that word isn't allowed here".to_owned(),
            }
        );
    }

    #[test]
    fn a_send_goes_out_unchanged_when_the_process_ends() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
        let (wake, notices) = notice_channel();
        let shared = shared(directory.path(), wake);
        let (_session_directory, mut session, written, _inbound) = session_stub();

        let held = HeldSend {
            id: 12,
            draft: draft("hi"),
            hook: HookKind::MessagePreSend,
            chain: vec![plugin.clone()],
        };
        let request = HookRequest {
            hook: HookKind::MessagePreSend,
            subject: HookSubject::Draft(draft("hi")),
        };
        runtime().block_on(session.on_command(&shared, Request::Hook { held, request }));
        let _load = next_frame(&written);
        let _activate = next_frame(&written);
        let _hook = next_frame(&written);

        // The helper went away with the send still waiting: the message goes as
        // the person wrote it rather than never arriving.
        session.fail_open(&shared);
        assert_eq!(
            notices
                .recv_timeout(Duration::from_secs(2))
                .expect("a notice"),
            Notice::SendReady {
                id: 12,
                draft: draft("hi"),
            }
        );
    }

    #[test]
    fn a_storage_call_is_answered_under_the_id_it_asked_with() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
        let (wake, _notices) = notice_channel();
        let shared = shared(directory.path(), wake);
        let (_session_directory, mut session, written, _inbound) = session_stub();

        let request = HostCall::new(
            call::method::STORAGE_SET,
            StorageSet {
                key: "seen".to_owned(),
                value: serde_json::json!(3),
            },
        )
        .expect("a call");
        let frame = Frame::new(
            FrameKind::Request,
            0,
            serde_json::to_vec(&FromHelper::Call {
                plugin: plugin.clone(),
                call: 11,
                request,
            })
            .expect("it serializes"),
        );
        assert!(session.on_frame(&shared, frame).is_none());

        let reply = next_frame(&written);
        assert_eq!(reply.kind, FrameKind::Reply);
        assert_eq!(reply.id, 11);
        let ToHelper::Return {
            call: answered,
            answer,
            ..
        } = message(&reply)
        else {
            panic!("a host call is answered with a return");
        };
        assert_eq!(answered, 11);
        assert_eq!(answer, HostReturn::Ok(serde_json::json!(true)));
        assert_eq!(
            shared.store().storage_get(&plugin, "seen"),
            Some(serde_json::json!(3))
        );
    }

    #[test]
    fn a_capability_the_manifest_does_not_declare_is_refused() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        // A plugin with no `storage` line in its manifest.
        let manifest = r#"{"api":1,"id":"com.example.plain","name":"Plain","version":"1.0.0",
            "wasm":"plugin.wasm"}"#;
        let plugin = installed(directory.path(), manifest, b"not a real module");
        let (wake, _notices) = notice_channel();
        let shared = shared(directory.path(), wake);
        let (_session_directory, mut session, written, _inbound) = session_stub();

        let request = HostCall::new(
            call::method::STORAGE_GET,
            StorageGet {
                key: "seen".to_owned(),
            },
        )
        .expect("a call");
        let frame = Frame::new(
            FrameKind::Request,
            0,
            serde_json::to_vec(&FromHelper::Call {
                plugin: plugin.clone(),
                call: 2,
                request,
            })
            .expect("it serializes"),
        );
        assert!(session.on_frame(&shared, frame).is_none());
        let reply = next_frame(&written);
        let ToHelper::Return { answer, .. } = message(&reply) else {
            panic!("a host call is answered with a return");
        };
        assert!(matches!(answer, HostReturn::Denied { .. }), "{answer:?}");
    }

    #[test]
    fn typed_events_arrive_as_one_batch_after_their_window() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
        let (wake, _notices) = notice_channel();
        let shared = shared(directory.path(), wake);
        let (_session_directory, mut session, written, _inbound) = session_stub();

        let runtime = runtime();
        for body in ["one", "two"] {
            let event = SharedEvent::new(event(EventKind::Typing, body)).expect("it serializes");
            runtime.block_on(session.on_command(
                &shared,
                Request::Event {
                    plugin: plugin.clone(),
                    event,
                },
            ));
        }
        // Nothing is sent while the window is open: a burst is one call.
        assert!(
            written.recv_timeout(Duration::from_millis(50)).is_err(),
            "typing is held for its window"
        );

        std::thread::sleep(Duration::from_millis(COALESCE_MS + 50));
        session.on_tick(&shared);
        let _load = next_frame(&written);
        let _activate = next_frame(&written);
        let batch = next_frame(&written);
        assert_eq!(batch.kind, FrameKind::Event);
        let ToHelper::Event { plugin: to, events } = message(&batch) else {
            panic!("a batch is one event frame");
        };
        assert_eq!(to, plugin);
        assert_eq!(events.len(), 2, "one burst arrives as one call");
    }

    #[test]
    fn a_message_event_goes_out_at_once() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let plugin = installed(directory.path(), HOOK_MANIFEST, b"not a real module");
        let (wake, _notices) = notice_channel();
        let shared = shared(directory.path(), wake);
        let (_session_directory, mut session, written, _inbound) = session_stub();

        let event = SharedEvent::new(event(EventKind::MessageCreate, "hi")).expect("it serializes");
        runtime().block_on(session.on_command(
            &shared,
            Request::Event {
                plugin: plugin.clone(),
                event,
            },
        ));
        let _load = next_frame(&written);
        let _activate = next_frame(&written);
        let batch = next_frame(&written);
        let ToHelper::Event { events, .. } = message(&batch) else {
            panic!("a message event is one event frame");
        };
        assert_eq!(events.len(), 1, "a message is not coalesced");
    }
}
