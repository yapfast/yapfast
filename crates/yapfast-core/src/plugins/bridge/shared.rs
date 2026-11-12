use super::*;

/// How a request kept for a consent dialog is let go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Answered {
    /// The person allowed the host: the request is made again, and the gate
    /// finds the answer it was waiting on.
    Allowed,
    /// The person refused it, or nothing will ever answer: the guest is told
    /// why.
    Denied(String),
}

/// The requests waiting on one consent dialog, by the call that asked.
///
/// A person is asked about one host at a time, so the answers all share one
/// dialog: the tasks that asked are woken by the one answer it gives.
pub(super) type Waiting = Vec<tokio::sync::oneshot::Sender<Answered>>;

/// What both threads of the plugin system read: the application's thread and
/// the supervisor's.
pub(crate) struct Shared {
    pub(super) registry: RwLock<Registry>,
    pub(super) store: Mutex<Store>,
    /// The summary of [`Registry::summary`], kept as plain flags because the
    /// send and event paths read them on every message.
    pub(super) enabled: AtomicBool,
    pub(super) wanted: AtomicU64,
    pub(super) intercepts: AtomicBool,
    /// Whether an enabled plugin draws under messages, which a row asks before
    /// it costs one call per message on screen.
    pub(super) decorates: AtomicBool,
    /// Whether an enabled plugin badges people in the member list, which the
    /// list asks before it costs one call per row.
    pub(super) decorates_members: AtomicBool,
    /// Whether an enabled plugin says how people look, which a profile asks
    /// before it costs one answer per person.
    pub(super) identifies: AtomicBool,
    /// Whether an enabled plugin keeps deleted messages visible, which is the
    /// only reason the timeline merges what a backend dropped.
    pub(super) retains: AtomicBool,
    /// Whether an enabled plugin colours code blocks, which a row asks before
    /// it reads its own markdown for fences.
    pub(super) colours_code: AtomicBool,
    /// The picture store `media.get` reads, once the application has installed
    /// it: the parent never fetches a picture for a plugin, so a build without
    /// it answers `unsupported`.
    pub(super) pictures: Mutex<Option<Arc<dyn crate::plugins::media::Pictures>>>,
    /// When the person last did something a plugin's send can stand behind.
    pub(super) gesture: Mutex<Option<Instant>>,
    /// When each plugin last sent a message, for the 5-per-10-s the plan puts
    /// on `messages.send`.
    pub(super) sends: Mutex<BTreeMap<PluginId, Vec<Instant>>>,
    /// Why plugins are off, when they are.
    pub(super) off: Mutex<Option<String>>,
    /// The one place a plugin's HTTP request can be made, once the application
    /// has installed it; nothing here can reach the network without it.
    /// has installed one with a transport.
    pub(super) gate: Mutex<Option<Arc<crate::plugins::network::NetworkGate>>>,
    /// The `http.fetch` calls kept waiting on a consent dialog, per plugin and
    /// host. The person's answer is what lets them go: the task that made the
    /// request is still holding it, and makes it again once this says yes.
    pub(super) held: Mutex<BTreeMap<(PluginId, String), Waiting>>,
    /// Whether the helper binary is where [`spawn::program`] looks for it.
    pub(super) helper_installed: bool,
    /// The helper's process id while it runs, and 0 when none does.
    pub(super) helper: AtomicU32,
    /// What the application knows, which the reads answer from: a host call
    /// is answered in this task, not on the thread that holds the state.
    pub(super) world: Arc<crate::plugins::world::World>,
    pub(super) wake: Arc<dyn Fn(Notice) + Send + Sync>,
}

impl Shared {
    /// Copies the registry's summary into the flags the hot paths read.
    pub(super) fn publish(&self) {
        let summary = self.registry().summary();
        self.enabled.store(summary.enabled, Ordering::Relaxed);
        self.wanted.store(summary.wanted, Ordering::Relaxed);
        self.intercepts.store(summary.intercepts, Ordering::Relaxed);
        self.decorates.store(summary.decorates, Ordering::Relaxed);
        self.decorates_members
            .store(summary.decorates_members, Ordering::Relaxed);
        self.identifies.store(summary.identifies, Ordering::Relaxed);
        self.retains.store(summary.retains, Ordering::Relaxed);
        self.colours_code
            .store(summary.colours_code, Ordering::Relaxed);
    }

    /// Records that the person just did something, which is what a plugin's
    /// send needs behind it.
    pub(super) fn gesture(&self) {
        *self.gesture.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
    }

    /// Whether `plugin` may send now: five in any ten seconds, as §9.1 caps it.
    pub(super) fn may_send(&self, plugin: &PluginId) -> bool {
        let mut sends = self.sends.lock().unwrap_or_else(PoisonError::into_inner);
        let held = sends.entry(plugin.clone()).or_default();
        let since = Instant::now() - SEND_WINDOW;
        held.retain(|at| *at > since);
        if held.len() >= SEND_LIMIT {
            return false;
        }
        held.push(Instant::now());
        true
    }

    /// Whether a person did something recently enough for a plugin's send to
    /// stand behind it.
    pub(super) fn has_gesture(&self) -> bool {
        self.gesture
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some_and(|at| at.elapsed() < GESTURE_WINDOW)
    }

    /// The registry for reading.
    ///
    /// A poisoned lock means some thread panicked while holding it; the registry
    /// is still the last consistent state, and refusing to read it would turn one
    /// panic into a plugin system that never works again.
    pub(crate) fn registry(&self) -> RwLockReadGuard<'_, Registry> {
        self.registry.read().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn registry_mut(&self) -> RwLockWriteGuard<'_, Registry> {
        self.registry
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn statuses(&self) -> Vec<(PluginId, PluginStatus)> {
        self.registry().statuses()
    }

    pub(super) fn announce(&self, notice: Notice) {
        (self.wake)(notice);
    }

    /// Why plugins are off, when they are.
    pub(super) fn off(&self) -> Option<String> {
        self.off
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Whether the helper's program is installed beside the app.
    pub(super) fn helper_installed(&self) -> bool {
        self.helper_installed
    }

    /// The helper's process id, while it runs.
    pub(super) fn helper_pid(&self) -> Option<u32> {
        match self.helper.load(Ordering::Relaxed) {
            0 => None,
            pid => Some(pid),
        }
    }

    /// Keeps the helper's process id, or forgets it when the process is gone.
    pub(super) fn set_helper_pid(&self, pid: Option<u32>) {
        self.helper.store(pid.unwrap_or(0), Ordering::Relaxed);
    }

    /// Forgets why plugins were off, so the next trigger starts the helper again.
    pub(super) fn clear_off(&self) {
        *self.off.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// The network gate, if the application installed one.
    pub(super) fn gate(&self) -> Option<Arc<crate::plugins::network::NetworkGate>> {
        self.gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
