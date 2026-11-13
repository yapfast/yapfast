use super::*;

/// A send that is waiting on a hook chain.
pub(super) struct HeldSend {
    /// The composer's id for this send, echoed back to the application.
    pub(super) id: u64,
    /// The draft as the person wrote it: what goes out if the chain fails.
    /// For a hook that isn't a send, only its room is filled in.
    pub(super) draft: Draft,
    pub(super) hook: HookKind,
    /// The plugins the chain runs through, dependencies first.
    pub(super) chain: Vec<PluginId>,
    /// When the person stops waiting: the moment they sent it plus the hook's
    /// whole-chain budget.
    pub(super) deadline: Instant,
}

impl HeldSend {
    /// What the application does with a chain that let its subject go as it
    /// was, or that answered nothing at all: a held send goes out, a typing
    /// indicator goes, a file stages as it was read. It is the fail-open answer
    /// every hook gets when a plugin traps, runs late or answers nonsense.
    pub(super) fn let_go(self, shared: &Shared) {
        match self.hook {
            HookKind::MessagePreSend | HookKind::MessagePreEdit => {
                shared.announce(Notice::SendReady {
                    id: self.id,
                    draft: self.draft,
                });
            }
            HookKind::TypingPreSend => shared.announce(Notice::Typing {
                id: self.id,
                room: self.draft.room,
            }),
            HookKind::AttachmentPreUpload => shared.announce(Notice::Attachment {
                id: self.id,
                name: None,
                blocked: None,
            }),
            // Nothing holds an activity: Yapfast publishes none, so a chain in
            // front of one can't have anything waiting on it.
            HookKind::ActivityPrePublish => {}
        }
    }
}

/// One helper process, and the state of the conversation with it.
pub(super) struct Session {
    pub(super) child: Child,
    /// Frames to write. A thread of its own, so a helper that stops reading
    /// can't stop this task's timers from noticing.
    pub(super) writer: std::sync::mpsc::Sender<Frame>,
    pub(super) inbound: tokio::sync::mpsc::UnboundedReceiver<Inbound>,
    /// The thread reading the helper's stdout; ending it means the helper is
    /// gone.
    pub(super) reader: Option<std::thread::JoinHandle<()>>,
    /// Pairs a request with its answer, counting up so a late answer can never be
    /// mistaken for a new request's.
    pub(super) next_id: u64,
    pub(super) pending: BTreeMap<u64, Pending>,
    /// The plugins whose module this process has been given.
    pub(super) loaded: BTreeSet<PluginId>,
    /// The plugins it has activated.
    pub(super) active: BTreeSet<PluginId>,
    /// Events waiting for their coalescing window, per plugin.
    pub(super) batches: BTreeMap<PluginId, Batch>,
    /// What plugins drew since the interface was last given a frame, and when
    /// it may be given one.
    pub(super) painting: Painting,
    /// The timers guests asked for, which live here so a guest can't hold
    /// memory in the helper with them.
    pub(super) timers: BTreeMap<TimerKey, Timer>,
    /// The `settings.on_change` calls of each plugin that asked to be told of a
    /// change, by the guest's own call id. They belong to the instance that
    /// asked, so a discarded one takes them with it.
    pub(super) watchers: BTreeMap<PluginId, Vec<u64>>,
    /// Commands that arrived before the hello was answered.
    pub(super) queued: VecDeque<Request>,
    /// What a plugin's last `yf_deactivate` handed on, waiting for the swap that
    /// will load its replacement: the state crosses the version change, so an
    /// update doesn't cost the plugin its memory of where it was.
    pub(super) handoff: BTreeMap<PluginId, Option<String>>,
    /// The update in progress, if any: its steps run one after another, each
    /// waiting for the helper to say it is done, so no dependent is started
    /// before what it needs is back.
    pub(super) swap: Option<Swap>,
    /// The application's work that arrived while a swap was running. It is done
    /// after, rather than woken into a half-swapped world: a dependent's event
    /// handler asking its dependency something must find it there.
    pub(super) held: Vec<Request>,
    /// The guest calls one plugin is making into another, innermost last. It is
    /// what a loop of calls is refused with: the chain names them in the order
    /// that made the loop.
    pub(super) calls: Vec<InFlight>,
    /// `plugin.call`s waiting for a swap to finish, so a caller is never told its
    /// callee isn't installed when it is only being replaced.
    pub(super) held_calls: Vec<(PluginId, u64, HostCall)>,
    /// The plugins loaded from a folder, and what their files looked like when
    /// they were read: the development loop, which polls them on this session's
    /// own clock rather than starting a thread for one person's editor.
    pub(super) dev: BTreeMap<PluginId, DevWatch>,
    /// Whether the parent asked the helper to stop, which makes its exit planned.
    pub(super) asked: bool,
    /// What the helper said as it left, if it said anything.
    pub(super) bye: Option<String>,
}

/// One frame from the helper, or the end of the pipe.
pub(super) enum Inbound {
    Frame(Frame),
    /// The pipe closed, with what went wrong if it was a broken frame rather
    /// than a clean end.
    Closed(Option<String>),
}

/// What made the session's loop look up from waiting.
pub(super) enum Waking {
    /// The helper sent a frame, or its pipe closed.
    Helper(Option<Box<Inbound>>),
    /// The application sent a command, or went away.
    Application(Option<Box<Request>>),
    /// A deadline came due: a timer, a batch, a request's budget or a save.
    Clock,
}

/// A request waiting for its answer.
pub(super) struct Pending {
    pub(super) deadline: Instant,
    pub(super) purpose: Purpose,
}

/// What an answer is for.
pub(super) enum Purpose {
    /// A request whose answer is only a report: a timeout is a fault to log.
    Agent {
        what: &'static str,
        plugin: PluginId,
        /// How long it was given, for the sentence a timeout writes.
        budget: Duration,
    },
    /// A hook chain: if it doesn't answer in time the message goes as it was.
    Hook(Box<HeldSend>),
    /// One plugin's call into another, which the caller is waiting on: the
    /// answer goes back under the guest's own number for it.
    Invocation {
        /// The plugin that made the call.
        caller: PluginId,
        /// The number its `yf_call` answered `pending` with.
        call: u64,
        /// The export, for the sentence a timeout writes.
        function: String,
    },
    /// One step of a swap. Its answer moves the swap on rather than reporting a
    /// fault: an instance that doesn't come back is a fault of its own, and a
    /// deadline here must not fail the plugin that is merely being replaced.
    Swap { plugin: PluginId },
}

/// One call between two plugins that is waiting for its answer.
pub(super) struct InFlight {
    /// The frame id that will answer it.
    pub(super) frame: u64,
    pub(super) from: PluginId,
    pub(super) to: PluginId,
}

/// A plugin being replaced, and the steps left to do it in.
pub(super) struct Swap {
    /// The plugin being replaced.
    pub(super) plugin: PluginId,
    /// Everything the swap touches: the plugin and the dependents being stopped
    /// and started again. Kept for the whole swap, so a dependent stopped early
    /// is still known to be part of it.
    pub(super) participants: BTreeSet<PluginId>,
    /// What is left, in the order it runs.
    pub(super) steps: VecDeque<Step>,
    /// The frame whose answer moves this on, if one is outstanding.
    pub(super) waiting: Option<u64>,
}

/// One step of an update.
pub(super) enum Step {
    /// Stop a dependent, outermost first.
    Stop(PluginId),
    /// Stop the plugin itself and forget its module, so the next activation
    /// carries the one the store holds now.
    Replace,
    /// Start the replacement.
    StartSelf,
    /// Start a dependent again, innermost first.
    Start(PluginId),
}

/// A batch of events waiting for its coalescing window.
pub(super) struct Batch {
    pub(super) events: Vec<Arc<SharedEvent>>,
    /// What was published to this plugin's topics in the same window.
    pub(super) topics: Vec<Arc<SharedTopic>>,
    pub(super) bytes: usize,
    pub(super) due: Instant,
}

/// Which plugin a timer belongs to, and the name the guest gave it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct TimerKey {
    pub(super) plugin: PluginId,
    pub(super) tag: String,
}

/// A timer a guest asked for, which fires a call into it when it comes due.
pub(super) struct Timer {
    pub(super) interval: Duration,
    pub(super) repeating: bool,
    pub(super) due: Instant,
}

/// The work a chain for a hook that isn't a message holds, or `None` when
/// nothing intercepts that hook.
///
/// Only the room travels in the held draft: the subject a chain that changes
/// nothing keeps is the one it was asked about, and none of these hooks has a
/// draft to hand back.
pub(super) fn held_hook(shared: &Shared, id: u64, hook: HookKind, room: &str) -> Option<HeldSend> {
    let chain = shared.registry().chain(hook);
    if chain.is_empty() {
        return None;
    }
    let budget = hook.budget();
    let deadline =
        Instant::now() + Duration::from_millis(budget.chain_ms.unwrap_or(budget.per_plugin_ms));
    Some(HeldSend {
        id,
        draft: Draft {
            room: room.to_owned(),
            body: String::new(),
            reply_to: None,
            attachments: Vec::new(),
        },
        hook,
        chain,
        deadline,
    })
}

/// What came of asking for a plugin to be up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Started {
    /// It was up already.
    Active,
    /// The helper was asked to start it, under this frame id.
    Asked(u64),
    /// It can't run: not enabled, or its module doesn't check out.
    Refused,
}

impl Started {
    /// Whether the plugin is up, or will be once the helper has answered.
    pub(super) fn runs(self) -> bool {
        !matches!(self, Started::Refused)
    }
}
