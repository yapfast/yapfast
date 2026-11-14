use super::*;

impl Bridge {
    /// Reads the store and the enabled manifests; starts nothing.
    ///
    /// `world` is the application's state as it publishes it, which is where a
    /// guest's reads are answered from: `app.version`, among them, is the
    /// version of the program that is running, which only the application knows.
    pub fn open(
        data_dir: &Path,
        wake: Arc<dyn Fn(Notice) + Send + Sync>,
        host: Host,
        world: Arc<dyn World>,
    ) -> anyhow::Result<Self> {
        let store = Store::open(data_dir)?;
        let registry = Registry::load(&store);
        // What the person allows the helper, from the last run: every helper
        // is told the pair right after its `Ready`.
        let limits = crate::budget::load(store.root());
        let shared = Arc::new(Shared {
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
            // Asked once: a file beside the app doesn't appear while it runs.
            helper_installed: spawn::program().is_ok(),
            helper: AtomicU32::new(0),
            limits: Mutex::new(limits),
            world,
            wake,
        });
        shared.publish();
        Ok(Bridge {
            data_dir: data_dir.to_owned(),
            host,
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

    /// Whether an enabled plugin draws under messages. A row that isn't on
    /// screen for anyone costs one load of this and nothing else.
    pub fn decorates(&self) -> bool {
        self.shared.decorates.load(Ordering::Relaxed)
    }

    /// Whether an enabled plugin badges people in the member list. A list that
    /// nobody badged costs one load of this and nothing else.
    pub fn decorates_members(&self) -> bool {
        self.shared.decorates_members.load(Ordering::Relaxed)
    }

    /// How many plugins badge people in the member list: a person the list has
    /// heard from every one of them about isn't asked about again.
    pub fn member_decorator_count(&self) -> usize {
        self.shared.registry().member_decorator_count()
    }

    /// Whether an enabled plugin says how people look. One answer serves every
    /// row that person wrote, so this is asked once per person and not per row.
    pub fn identifies(&self) -> bool {
        self.shared.identifies.load(Ordering::Relaxed)
    }

    /// Asks every plugin that says how people look about each of these people.
    ///
    /// The callers ask about the people on screen whose answer is missing or
    /// stale, never about every row: one pair is one answer, and the parent
    /// keeps it. A plugin is asked in the chain's order, so which answer wins is
    /// the chain's order too and not the order two answers arrive in.
    pub fn ask_identities(&mut self, people: &[yapfast_plugin_api::identity::IdentityRequest]) {
        let plugins = self.shared.registry().identities();
        if plugins.is_empty() || people.is_empty() {
            return;
        }
        for request in people.iter().take(MAX_IDENTITIES) {
            for plugin in &plugins {
                self.send(Request::Identity {
                    plugin: plugin.clone(),
                    request: request.clone(),
                });
            }
        }
    }

    /// Whether an enabled plugin keeps deleted messages visible. The timeline
    /// asks once per arriving timeline, and merges nothing when nobody does.
    pub fn retains(&self) -> bool {
        self.shared.retains.load(Ordering::Relaxed)
    }

    /// Whether an enabled plugin colours code blocks. A row reads its markdown
    /// for fences only when one does.
    pub fn colours_code(&self) -> bool {
        self.shared.colours_code.load(Ordering::Relaxed)
    }

    /// Asks every plugin that colours code blocks about each of these blocks.
    ///
    /// Only the blocks of the messages on screen are asked about, and the
    /// application asks about none of the blocks it already has an answer for:
    /// a block is named by its own content hash, so the same snippet twice is
    /// one question.
    pub fn ask_code_blocks(&mut self, blocks: &[yapfast_plugin_api::code::CodeBlockRequest]) {
        let plugins = self.shared.registry().code_colourers();
        if plugins.is_empty() || blocks.is_empty() {
            return;
        }
        for request in blocks.iter().take(MAX_CODE_BLOCKS) {
            for plugin in &plugins {
                self.send(Request::CodeBlock {
                    plugin: plugin.clone(),
                    request: request.clone(),
                });
            }
        }
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
}
