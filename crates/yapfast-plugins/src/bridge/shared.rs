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

/// The pictures each plugin handed its overlay, each under its `held:<n>`
/// name, oldest first.
pub(super) type HeldPictures = BTreeMap<PluginId, Vec<(String, Arc<[u8]>)>>;

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
    /// What each plugin answered about the messages on screen, so a row that
    /// comes back into view isn't asked about again.
    pub(super) decorated: Mutex<decorate_cache::DecorateCache>,
    /// The powerful tier's switch and the person's answers (`powerful.rs`).
    pub(crate) powerful: Arc<crate::powerful::Powerful>,
    /// The powerful questions on screen now, so a plugin that calls again
    /// while the person reads one doesn't stack another.
    pub(super) powerful_asked: Mutex<BTreeSet<(PluginId, Capability)>>,
    /// Each plugin's `net.listen` endpoint, while it serves.
    pub(super) endpoints: Mutex<BTreeMap<PluginId, crate::listen::Endpoint>>,
    /// The requests an endpoint is waiting on the plugin to answer.
    pub(super) answers:
        Mutex<BTreeMap<(PluginId, u64), tokio::sync::oneshot::Sender<crate::listen::Reply>>>,
    /// The calls waiting on a powerful question the person hasn't answered:
    /// each goes ahead when they say yes, and is refused when they say no.
    pub(super) powerful_waiting: Mutex<Vec<(PluginId, Capability, u64, HostCall)>>,
    /// Each plugin's `net.socket` connections, by the socket's number.
    pub(super) sockets: Mutex<BTreeMap<PluginId, BTreeMap<u64, crate::socket::Handle>>>,
    /// The next socket's number, which is never reused in a session.
    pub(super) next_socket: AtomicU64,
    /// When each plugin last sent as an agent, for its rate.
    pub(super) agent_sends: Mutex<BTreeMap<PluginId, Vec<Instant>>>,
    /// The picture store `media.get` reads, once the application has installed
    /// it: the parent never fetches a picture for a plugin, so a build without
    /// it answers `unsupported`.
    pub(super) pictures: Mutex<Option<Arc<dyn crate::media::Pictures>>>,
    /// When the person last asked each plugin for something: its command,
    /// its menu row, a widget it drew. A send, an edit or a navigation stands
    /// behind that plugin's own gesture and no other's.
    pub(super) gestures: crate::permissions::Gestures,
    /// When each plugin was last in a `message_pre_send` chain: the person
    /// sending is a gesture for editing, deleting and reacting to their own
    /// messages.
    pub(super) sent_through: crate::permissions::Gestures,
    /// When each plugin last sent a message, for the 5-per-10-s the plan puts
    /// on `messages.send`.
    pub(super) sends: Mutex<BTreeMap<PluginId, Vec<Instant>>>,
    /// Why plugins are off, when they are.
    pub(super) off: Mutex<Option<String>>,
    /// The one place a plugin's HTTP request can be made, once the application
    /// has installed it; nothing here can reach the network without it.
    /// has installed one with a transport.
    pub(super) gate: Mutex<Option<Arc<crate::network::NetworkGate>>>,
    /// The `http.fetch` calls kept waiting on a consent dialog, per plugin and
    /// host. The person's answer is what lets them go: the task that made the
    /// request is still holding it, and makes it again once this says yes.
    pub(super) held: Mutex<BTreeMap<(PluginId, String), Waiting>>,
    /// Whether the helper binary is where [`spawn::program`] looks for it.
    pub(super) helper_installed: bool,
    /// The helper's process id while it runs, and 0 when none does.
    pub(super) helper: AtomicU32,
    /// What the last helper said of its OS sandbox in its `Ready`: 0 while
    /// none has started this session, 1 when it was complete, 2 when a part
    /// was missing. It outlives the helper, as the page's Advanced dialog
    /// reads it whether or not one runs now.
    pub(super) sandbox: std::sync::atomic::AtomicU8,
    /// What the person allows the helper: its whole memory budget and how long
    /// an instance may sit without a call. The pair a helper is given on its
    /// `Ready` is read from here, so a change made while one was starting
    /// reaches it too.
    pub(super) limits: Mutex<limits::Limits>,
    /// What the application knows, which the reads answer from: a host call
    /// is answered in this task, not on the thread that holds the state.
    pub(super) world: Arc<dyn World>,
    /// The files `attachment_pre_upload` holds while plugins read or replace
    /// their bytes.
    pub(super) uploads: Mutex<crate::uploads::Uploads>,
    /// Answers too large for one frame, kept on disk for `http.read`: by
    /// number, whose they are, where, how large, and since when.
    pub(super) bodies: Mutex<BTreeMap<u64, KeptBody>>,
    /// The overlay frames sent and not yet answered with `overlay.set`, by
    /// plugin, so a slow plugin is paced by its own answers.
    pub(super) overlay_waiting: Mutex<BTreeMap<PluginId, Instant>>,
    /// Pictures a plugin handed over with `overlay.picture`, by plugin, each
    /// under the name its sprites use, oldest first.
    pub(super) overlay_pictures: Mutex<HeldPictures>,
    pub(super) next_body: AtomicU64,
    /// The sign-ins waiting for a service's server to hand a token back.
    pub(super) oauth_returns: crate::oauth::Returns,
    pub(super) wake: Arc<dyn Fn(Notice) + Send + Sync>,
}

/// One large answer's body, kept for its plugin to read.
pub(crate) struct KeptBody {
    pub(super) plugin: PluginId,
    pub(super) path: std::path::PathBuf,
    pub(super) bytes: u64,
    pub(super) at: Instant,
    /// A recording's length in milliseconds and its waveform, which make it
    /// one `messages.voice` can send.
    pub(super) voice: Option<(u64, String)>,
}

/// How long a large answer's body is kept unread.
const KEEP_BODY: std::time::Duration = std::time::Duration::from_secs(10 * 60);

impl Shared {
    /// Keeps a large answer's body for `http.read`, and lets go of any kept
    /// longer than ten minutes.
    pub(super) fn keep_body(&self, plugin: &PluginId, path: std::path::PathBuf, bytes: u64) -> u64 {
        let id = self.next_body.fetch_add(1, Ordering::Relaxed) + 1;
        let mut bodies = self.bodies.lock().unwrap_or_else(PoisonError::into_inner);
        bodies.retain(|_, kept| {
            // A recording is the plugin's to send for as long as the session
            // lasts; only an answer's body goes after ten minutes.
            let fresh = kept.voice.is_some() || kept.at.elapsed() < KEEP_BODY;
            if !fresh {
                let _ = std::fs::remove_file(&kept.path);
            }
            fresh
        });
        bodies.insert(
            id,
            KeptBody {
                plugin: plugin.clone(),
                path,
                bytes,
                at: Instant::now(),
                voice: None,
            },
        );
        id
    }

    /// Keeps a recording for its plugin to read and send.
    pub(super) fn keep_recording(
        &self,
        plugin: &PluginId,
        path: std::path::PathBuf,
        bytes: u64,
        voice: Option<(u64, String)>,
    ) -> u64 {
        let id = self.keep_body(plugin, path, bytes);
        if let Some(kept) = self
            .bodies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(&id)
        {
            kept.voice = voice;
        }
        id
    }

    /// A recording of this plugin's that can go as a voice message: its file,
    /// its length in milliseconds and its waveform.
    pub(super) fn recording(
        &self,
        plugin: &PluginId,
        id: u64,
    ) -> Result<(std::path::PathBuf, u64, String), String> {
        let bodies = self.bodies.lock().unwrap_or_else(PoisonError::into_inner);
        let kept = bodies
            .get(&id)
            .filter(|kept| kept.plugin == *plugin)
            .ok_or_else(|| format!("no recording of this plugin's is kept as {id}"))?;
        let (duration, waveform) = kept.voice.clone().ok_or_else(|| {
            "only an Opus recording goes as a voice message, as Discord takes one".to_owned()
        })?;
        Ok((kept.path.clone(), duration, waveform))
    }

    /// A piece of a kept body, for the plugin it is.
    pub(super) fn read_body(
        &self,
        plugin: &PluginId,
        request: &yapfast_plugin_api::call::BodyRead,
    ) -> Result<(Vec<u8>, u64, bool), String> {
        use std::io::{Read, Seek, SeekFrom};
        let (path, bytes) = {
            let bodies = self.bodies.lock().unwrap_or_else(PoisonError::into_inner);
            let kept = bodies
                .get(&request.body)
                .filter(|kept| kept.plugin == *plugin)
                .ok_or_else(|| format!("no answer is kept as body {}", request.body))?;
            (kept.path.clone(), kept.bytes)
        };
        if request.offset > bytes {
            return Err(format!("the body is {bytes} bytes, not {}", request.offset));
        }
        let length = request
            .length
            .unwrap_or(yapfast_plugin_api::call::UploadRead::MAX_PIECE)
            .min(yapfast_plugin_api::call::UploadRead::MAX_PIECE);
        let mut file = std::fs::File::open(&path).map_err(|error| error.to_string())?;
        file.seek(SeekFrom::Start(request.offset))
            .map_err(|error| error.to_string())?;
        let mut piece = Vec::new();
        file.take(u64::from(length))
            .read_to_end(&mut piece)
            .map_err(|error| error.to_string())?;
        let eof = request.offset + piece.len() as u64 >= bytes;
        Ok((piece, bytes, eof))
    }

    /// The whole of a kept body of this plugin's, for a send that attaches
    /// it: refused over `cap` before anything is read.
    pub(super) fn body_bytes(
        &self,
        plugin: &PluginId,
        body: u64,
        cap: u64,
    ) -> Result<Vec<u8>, String> {
        let (path, bytes) = {
            let bodies = self.bodies.lock().unwrap_or_else(PoisonError::into_inner);
            let kept = bodies
                .get(&body)
                .filter(|kept| kept.plugin == *plugin)
                .ok_or_else(|| format!("no answer is kept as body {body}"))?;
            (kept.path.clone(), kept.bytes)
        };
        if bytes > cap {
            return Err(format!(
                "body {body} is {bytes} bytes, and a plugin's file is at most {cap}"
            ));
        }
        std::fs::read(&path).map_err(|error| format!("body {body} can't be read: {error}"))
    }

    /// Lets a kept body go: its file is removed.
    pub(super) fn release_body(&self, plugin: &PluginId, body: u64) -> bool {
        let mut bodies = self.bodies.lock().unwrap_or_else(PoisonError::into_inner);
        if bodies.get(&body).is_some_and(|kept| kept.plugin == *plugin) {
            if let Some(kept) = bodies.remove(&body) {
                let _ = std::fs::remove_file(kept.path);
            }
            return true;
        }
        false
    }

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

    /// Records that the person just asked `plugin` for something, which is
    /// what its send needs behind it.
    pub(super) fn gesture(&self, plugin: &PluginId) {
        self.gestures.record(plugin, Instant::now());
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

    /// Whether the person sent a message through `plugin`'s pre-send hook
    /// recently enough for an edit, a delete or a reaction of their own
    /// message to stand behind it.
    pub(super) fn sent_through(&self, plugin: &PluginId) -> bool {
        self.sent_through.path(plugin, Instant::now()) == crate::permissions::SendPath::Gesture
    }

    /// Whether a person did something recently enough for a plugin's send to
    /// stand behind it.
    pub(super) fn has_gesture(&self, plugin: &PluginId) -> bool {
        self.gestures.path(plugin, Instant::now()) == crate::permissions::SendPath::Gesture
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

    pub(super) fn decorated(&self) -> MutexGuard<'_, decorate_cache::DecorateCache> {
        self.decorated
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// One of a plugin's own assets, by the name its manifest gives it.
    pub(super) fn asset(
        &self,
        plugin: &PluginId,
        name: &str,
    ) -> Result<(yapfast_plugin_api::assets::AssetKind, Vec<u8>), String> {
        let manifest = self
            .registry()
            .entry(plugin)
            .map(|entry| entry.manifest.clone())
            .ok_or_else(|| format!("{plugin} isn't installed"))?;
        let store = self.store();
        let lock = store.lock().map_err(|error| format!("{error:#}"))?;
        let entry = lock
            .plugins
            .get(plugin)
            .ok_or_else(|| format!("{plugin} isn't installed"))?;
        store.asset(entry, &manifest, name)
    }

    /// Every asset a plugin carries, as `assets.list` answers.
    pub(super) fn asset_infos(
        &self,
        plugin: &PluginId,
    ) -> Result<Vec<yapfast_plugin_api::assets::AssetInfo>, String> {
        let manifest = self
            .registry()
            .entry(plugin)
            .map(|entry| entry.manifest.clone())
            .ok_or_else(|| format!("{plugin} isn't installed"))?;
        let store = self.store();
        let lock = store.lock().map_err(|error| format!("{error:#}"))?;
        let entry = lock
            .plugins
            .get(plugin)
            .ok_or_else(|| format!("{plugin} isn't installed"))?;
        Ok(store.asset_infos(entry, &manifest))
    }

    pub(super) fn statuses(&self) -> Vec<(PluginId, PluginStatus)> {
        self.registry().statuses()
    }

    pub(super) fn announce(&self, notice: Notice) {
        // A plugin that stops can't finish the files it held: they go as the
        // person picked them.
        if let Notice::SlotsGone(plugin) = &notice {
            for id in self.uploads().let_go(plugin) {
                (self.wake)(Notice::Attachment {
                    id,
                    name: None,
                    blocked: None,
                });
            }
        }
        (self.wake)(notice);
    }

    /// The pictures plugins handed their overlays.
    pub(super) fn overlay_pictures(&self) -> std::sync::MutexGuard<'_, HeldPictures> {
        self.overlay_pictures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn overlay_waiting(&self) -> std::sync::MutexGuard<'_, BTreeMap<PluginId, Instant>> {
        self.overlay_waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The held uploads.
    pub(super) fn uploads(&self) -> std::sync::MutexGuard<'_, crate::uploads::Uploads> {
        self.uploads.lock().unwrap_or_else(PoisonError::into_inner)
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

    /// Whether the helper runs without part of its OS sandbox: always on a
    /// platform that has none yet (Windows and macOS), and on Linux when a
    /// helper started this session said a part was missing. Before a Linux
    /// helper starts nothing is known, so nothing is claimed.
    pub(crate) fn sandbox_unavailable(&self) -> bool {
        cfg!(not(target_os = "linux")) || self.sandbox.load(Ordering::Relaxed) == 2
    }

    /// Keeps what the helper said about its sandbox in its `Ready`.
    pub(super) fn set_sandboxed(&self, sandboxed: bool) {
        self.sandbox
            .store(if sandboxed { 1 } else { 2 }, Ordering::Relaxed);
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
    pub(super) fn gate(&self) -> Option<Arc<crate::network::NetworkGate>> {
        self.gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// What the person allows the helper right now.
    ///
    /// A poisoned lock means some thread panicked while holding it; the pair is
    /// two numbers that were consistent when they were written, and refusing to
    /// read them would turn one panic into a plugin system that never works
    /// again.
    pub(crate) fn limits(&self) -> limits::Limits {
        *self.limits.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Keeps a new pair, for the next helper's `Ready` and the page's view.
    pub(crate) fn set_limits(&self, chosen: limits::Limits) {
        *self.limits.lock().unwrap_or_else(PoisonError::into_inner) = chosen;
    }
}
