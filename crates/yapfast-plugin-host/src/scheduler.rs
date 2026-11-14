//! When plugins run, and in what order.
//!
//! Each plugin has a FIFO mailbox; a plugin is on at most one worker, so the
//! number of plugins never changes the number of threads. Two lanes: work the
//! person is waiting for (hooks, commands, clicks) goes before background work
//! (events, timers), and one worker never takes background work, so a slow
//! event handler can't delay a send.
//!
//! Nothing blocks a worker. HTTP, storage, timers and prompts are requests that
//! the parent answers later, and a guest that wants to wait returns instead.
//! Deadlines are the watchdog's: it holds each running call's deadline, fires
//! the cancel, and discards an instance that stopped at an arbitrary point.
//!
//! Calls for different plugins overlap inside the runtime: the backend keeps one
//! lock per plugin, so a background event handler can't hold the path an
//! interactive send needs. Whichever worker frees up still takes the person's
//! work first, which is what the lanes buy on top of that.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tracing::{debug, warn};

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{CommandInvocation, CommandOutcome, HostReturn, base64};
use yapfast_plugin_api::event::{EventEnvelope, TopicEvent};
use yapfast_plugin_api::hook::{HookOutcome, HookRequest, Lane};
use yapfast_plugin_api::limits;
use yapfast_plugin_api::message::{Fault, FaultKind, FromHelper, PluginStatus};

use crate::backend::{Backend, CancelToken, HookRun, LoadSpec, Wait, Waits};
use crate::pipe::{Outbox, Waker};

/// How long one batch of events may run. Events aren't on a person's path, so
/// this is looser than a hook's budget, but a batch that never returns holds
/// its mailbox and its instance's memory.
const EVENT_BUDGET: Duration = Duration::from_millis(250);

/// How long a command may run. A command was typed by a person who is waiting
/// for it, and the parent waits the same two seconds for the answer.
const COMMAND_BUDGET: Duration = Duration::from_secs(2);

/// How long one plugin's call into another may run. The caller is waiting for
/// the answer as certainly as a command's typist is, so it gets the same
/// budget; the parent holds the same one and answers the caller rather than
/// leaving it waiting for an instance that was discarded.
const INVOKE_BUDGET: Duration = Duration::from_secs(2);

/// How long one plugin's `yf_decorate` may run. A row is on screen and its
/// decoration is a nicety: it gets the plan's 16 ms, and a plugin that misses
/// it leaves the row plain until it is asked again.
const DECORATE_BUDGET: Duration = Duration::from_millis(16);

/// How long a plugin's `yf_input` may run.
///
/// The person already clicked, so nothing waits on the answer; what this bounds
/// is how long one stuck instance blocks the lane it is in. A quarter of a
/// second is enough to send, draw or answer through a host call, and short
/// enough that a hung plugin is noticed while the person still remembers
/// clicking.
const INPUT_BUDGET: Duration = Duration::from_millis(250);

/// How long one plugin's `yf_identity` may run. The answer is kept and used by
/// every surface that shows that person, so it is asked once and not per row;
/// it still runs in the background lane, and the person is drawn as Yapfast
/// already knows them until it comes.
const IDENTITY_BUDGET: Duration = Duration::from_millis(16);

/// How long one plugin's `yf_code_block` may run. A block is on screen and its
/// colours are a nicety, like a decoration's: it gets the plan's 16 ms, and a
/// block that misses it is drawn plain, which is what it looked like anyway.
const CODE_BLOCK_BUDGET: Duration = Duration::from_millis(16);

/// How long `yf_deactivate` may take. Its answer is a convenience handed to the
/// next activation, so it gets long enough to write that and no longer: an
/// instance going away must not hold the helper.
const DEACTIVATE_BUDGET: Duration = Duration::from_millis(100);

/// How long instantiating a module and its `yf_activate` may take, and how long
/// compiling one may take: both are slow on a cold cache and neither is a
/// plugin misbehaving.
const START_BUDGET: Duration = Duration::from_secs(2);

/// How often the watchdog fires a deadline that has already passed.
///
/// A cancel reaches a guest only while the runtime is holding the call it is
/// for: one aimed at a worker still on its way into the backend, or at a call
/// extism hasn't started, is dropped there. So a missed deadline is fired again
/// at this pace for as long as its call is out, which is well inside the
/// budgets a person waits on and far inside the 30 s backstop.
const RE_FIRE: Duration = Duration::from_millis(1);

/// Bytes in a page of guest memory, for the helper's own budget.
const PAGE_BYTES: u64 = 64 << 10;

/// How the scheduler is started.
#[derive(Clone, Debug)]
pub struct Config {
    /// Workers to start. Zero takes the machine's shape: two, or one on a
    /// machine with two cores or fewer, where a second would only compete for
    /// the same two.
    pub workers: usize,
    /// How long an instance may go without a call before it is evicted. Five
    /// minutes in a real helper; a test sets a few milliseconds, since it can't
    /// wait for the real one.
    pub idle_after: Duration,
    /// The helper's whole budget for plugin memory. [`limits::MAX_HELPER_BYTES`]
    /// in a real helper, and the person's choice after a
    /// [`Scheduler::set_limits`]; a test sets a smaller one where the real
    /// budget would need more instances than the test has.
    pub budget_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            workers: 0,
            idle_after: Duration::from_millis(limits::IDLE_EVICTION_MS),
            budget_bytes: limits::MAX_HELPER_BYTES,
        }
    }
}

impl Config {
    /// The worker count this machine gets.
    fn worker_count(&self) -> usize {
        if self.workers > 0 {
            return self.workers;
        }
        match thread::available_parallelism() {
            Ok(cores) if cores.get() > 2 => 2,
            _ => 1,
        }
    }
}

/// What the parent asked for, once its frame is decoded.
#[derive(Clone, Debug)]
pub enum Message {
    /// Take a module and its limits, without activating it.
    Load { spec: LoadSpec },
    /// Drop a module and its instance, keeping nothing.
    Unload { plugin: PluginId },
    /// Load the module first when `load` is there, then instantiate and call
    /// `yf_activate`; answered with [`FromHelper::Activated`].
    ///
    /// The two jobs are queued back to back in the plugin's own mailbox, so its
    /// module is loaded and its instance started as one step of the phase, while
    /// other plugins' loads run on other workers meanwhile.
    Activate {
        id: u64,
        plugin: PluginId,
        load: Option<LoadSpec>,
    },
    /// Call `yf_deactivate`; answered with [`FromHelper::Deactivated`].
    Deactivate { id: u64, plugin: PluginId },
    /// Run the instance again with nothing new: a repeat timer's trigger.
    /// Nothing answers it, because the clock that sent it isn't waiting.
    Kick { plugin: PluginId },
    /// One batch of events; answered with [`FromHelper::EventDone`].
    Event {
        id: u64,
        plugin: PluginId,
        events: Vec<EventEnvelope>,
        /// What other plugins published to this one's topics. It travels with
        /// the events because a subscriber handles both in one call.
        topics: Vec<TopicEvent>,
    },
    /// One hook, for the whole chain at once; answered with
    /// [`FromHelper::HookDone`].
    Hook {
        id: u64,
        chain: Vec<PluginId>,
        request: HookRequest,
    },
    /// A command someone typed; answered with [`FromHelper::CommandDone`].
    Command {
        id: u64,
        plugin: PluginId,
        command: CommandInvocation,
    },
    /// One export of one plugin, called for another: `plugin.call`, answered
    /// with [`FromHelper::Invoked`].
    Invoke {
        id: u64,
        plugin: PluginId,
        function: String,
        args_base64: String,
    },
    /// What a plugin draws under a message, answered with
    /// [`FromHelper::Decorated`]: `yf_decorate`'s export, with the row.
    Decorate {
        id: u64,
        plugin: PluginId,
        message: String,
        request: yapfast_plugin_api::message::DecorateRequest,
    },
    /// How a plugin says one person looks, answered with
    /// [`FromHelper::Identity`]: `yf_identity`'s export, with the pair.
    Identity {
        id: u64,
        plugin: PluginId,
        request: yapfast_plugin_api::identity::IdentityRequest,
    },
    /// What a plugin makes of one code block, answered with
    /// [`FromHelper::CodeBlock`]: `yf_code_block`'s export, with the block.
    CodeBlock {
        id: u64,
        plugin: PluginId,
        request: yapfast_plugin_api::code::CodeBlockRequest,
    },
    DecorateMember {
        id: u64,
        plugin: PluginId,
        member: String,
        request: yapfast_plugin_api::message::DecorateMemberRequest,
    },
    Input {
        id: u64,
        plugin: PluginId,
        input: yapfast_plugin_api::message::InputEvent,
    },
    /// The parent's answer to a guest's host call.
    Return {
        plugin: PluginId,
        call: u64,
        answer: HostReturn,
    },
    /// Evict now: the parent's idle clock ran out, or it wants the memory.
    Evict { plugin: PluginId },
}

/// The mailboxes, the lanes, the deadlines and the watchdog.
pub struct Scheduler<B: Backend + 'static> {
    shared: Arc<Shared<B>>,
    workers: Vec<JoinHandle<()>>,
    watchdog: Option<JoinHandle<()>>,
}

impl<B: Backend + 'static> Scheduler<B> {
    /// Starts the workers and the watchdog around a backend.
    pub fn start(backend: B, outbox: Outbox, waker: Waker, config: Config) -> io::Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::new(config.idle_after, config.budget_bytes)),
            work: Condvar::new(),
            watch: Condvar::new(),
            backend,
            outbox,
            waker,
        });

        // The runtime is the only thing that sees a guest's host call or its
        // timers, so it is where both are reported from; the account is the
        // scheduler's, and a weak handle to it is what the runtime holds. A
        // shared one would be a cycle: the account owns the runtime.
        let waits: Waits = {
            let account = Arc::downgrade(&shared);
            Arc::new(move |plugin: &PluginId, wait: Wait| {
                if let Some(account) = account.upgrade() {
                    account.waits(plugin, wait);
                }
            })
        };
        shared.backend.set_wait_sink(waits);

        let count = config.worker_count();
        let mut workers = Vec::with_capacity(count);
        for index in 0..count {
            // The first worker takes both lanes; the rest take interactive work
            // only, so a slow event handler can never delay a send. On a
            // machine with one worker, that worker has to take both or
            // background work would never run.
            let background = index == 0 || count == 1;
            let shared = Arc::clone(&shared);
            workers.push(
                thread::Builder::new()
                    .name(format!("plugin-worker-{index}"))
                    .spawn(move || work(shared, background))?,
            );
        }
        let watchdog = {
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("plugin-watchdog".into())
                .spawn(move || watch(shared))?
        };
        Ok(Scheduler {
            shared,
            workers,
            watchdog: Some(watchdog),
        })
    }

    /// Queues what the parent asked for, in the lane it belongs to.
    pub fn send(&self, message: Message) {
        self.shared.enqueue(message);
    }

    /// The host calls the parent hasn't answered for a plugin yet.
    ///
    /// This is what keeps the instance off the eviction list, and it is the only
    /// way to see it: a hold that was never recorded writes no frame, so it
    /// shows only as an instance evicted while a guest was waiting to be
    /// resumed.
    pub fn waiting(&self, plugin: &PluginId) -> usize {
        let state = lock(&self.shared.state);
        state
            .index
            .get(plugin)
            .map_or(0, |&slot| state.slots[slot].waiting.len())
    }

    /// Takes new limits: how long an instance may sit without a call, and the
    /// helper's whole memory budget, both at once.
    ///
    /// A budget the live caps no longer fit evicts the least recently used
    /// instances that may go, and only those: a busy instance, one a guest is
    /// waiting on, one holding a timer or with work in its mailbox stays, and
    /// the helper remains over budget rather than dropping it. The victims are
    /// queued as conditional work for a worker — this runs on the pipe loop,
    /// which must never touch the runtime — and each one is checked again
    /// where it is dropped: a hold or work that arrived after the victim was
    /// chosen keeps the instance, and so does a drop or a raised budget that
    /// has already made the eviction unnecessary, because the choice was made
    /// under this lock and carried out later. The parent's own `Evict` is a
    /// different thing and stays unconditional: it is the parent saying the
    /// instance goes now.
    /// The watchdog is woken too, so a shorter idle is measured now rather than
    /// when the old deadline would have come round.
    pub fn set_limits(&self, idle_after: Duration, budget_bytes: u64) {
        let victims = {
            let mut state = lock(&self.shared.state);
            state.idle_after = idle_after;
            state.budget_bytes = budget_bytes;
            state.over_budget()
        };
        self.shared.watch.notify_all();
        for plugin in victims {
            self.shared.queue_evict_for_limits(&plugin);
        }
    }

    /// Stops the watchdog and the workers, dropping work that hasn't started.
    pub fn shutdown(&mut self) {
        {
            let mut state = lock(&self.shared.state);
            state.stopping = true;
        }
        self.shared.work.notify_all();
        self.shared.watch.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
        if let Some(watchdog) = self.watchdog.take() {
            let _ = watchdog.join();
        }
    }
}

/// The state the workers, the watchdog and the run loop share.
struct Shared<B> {
    state: Mutex<State>,
    /// Signalled when a task becomes ready or the loop is stopping.
    work: Condvar,
    /// Signalled when a deadline is added, removed, or has passed.
    watch: Condvar,
    /// The runtime, shared without a lock of its own: every call takes the lock
    /// of the plugin it runs on, so a background event and an interactive send
    /// don't wait for each other.
    backend: B,
    outbox: Outbox,
    waker: Waker,
}

/// One worker: claim a task, run it, let go of its plugins.
fn work<B: Backend + 'static>(shared: Arc<Shared<B>>, take_background: bool) {
    while let Some(task) = shared.next(take_background) {
        shared.run(&task);
        shared.finished(&task);
    }
}

/// The watchdog: the only thread that measures time rather than waiting.
///
/// It sleeps until the earliest deadline it holds, which may be a running
/// call's or an idle instance's, and does nothing in between. A helper with no
/// plugin running and no instance to evict is asleep with no timer at all.
fn watch<B: Backend + 'static>(shared: Arc<Shared<B>>) {
    let mut state = lock(&shared.state);
    loop {
        if state.stopping {
            return;
        }
        let now = Instant::now();
        let idle_after = state.idle_after;
        let mut overdue: Vec<u64> = Vec::new();
        let mut idle: Vec<usize> = Vec::new();
        let mut next: Option<Instant> = None;
        for running in &state.running {
            let Some(deadline) = running.deadline else {
                continue;
            };
            // A deadline that fired isn't over: the runtime drops a cancel aimed
            // at a call it isn't holding, so the entry stays overdue until the
            // worker's call comes back and takes it away.
            if deadline <= now {
                overdue.push(running.serial);
            } else {
                next = earliest(next, deadline);
            }
        }
        for (index, slot) in state.slots.iter().enumerate() {
            if !slot.live || slot.busy || slot.has_work() || !slot.unheld() {
                continue;
            }
            let at = slot.last_call + idle_after;
            if at <= now {
                idle.push(index);
            } else {
                next = earliest(next, at);
            }
        }
        if !overdue.is_empty() {
            // The call is still out, so the watchdog comes back for it at this
            // pace: one fire may have been swallowed, and the next one lands.
            next = earliest(next, now + RE_FIRE);
        }
        if !overdue.is_empty() || !idle.is_empty() {
            drop(state);
            for serial in &overdue {
                shared.fire(*serial);
            }
            for index in idle {
                shared.evict(index);
            }
            state = lock(&shared.state);
        }
        state = match next {
            Some(at) => {
                shared
                    .watch
                    .wait_timeout(state, at.saturating_duration_since(now))
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .0
            }
            None => shared
                .watch
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        };
    }
}

impl<B: Backend + 'static> Shared<B> {
    /// The next task this worker may run, waiting until one is ready.
    fn next(&self, take_background: bool) -> Option<Arc<Task>> {
        let mut state = lock(&self.state);
        loop {
            if state.stopping {
                return None;
            }
            if let Some(task) = state.claim(take_background) {
                return Some(task);
            }
            state = self
                .work
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    /// Lets go of a task's plugins and lets the next worker in.
    ///
    /// This is also where the helper notices that it is finished. The task that
    /// unloaded or evicted the last instance is itself holding that plugin as
    /// busy while it runs, so a check made from inside the task always finds
    /// work in hand; only once it lets go is there nothing left, and the helper
    /// is disposable, so it goes.
    fn finished(&self, task: &Task) {
        let mut state = lock(&self.state);
        let now = Instant::now();
        for &slot in task.slots.as_slice() {
            let entry = &mut state.slots[slot];
            entry.busy = false;
            entry.last_call = now;
        }
        self.work.notify_all();
        drop(state);
        self.check_empty();
    }

    /// Puts a message in the lane and mailbox it belongs to.
    fn enqueue(&self, message: Message) {
        let mut state = lock(&self.state);
        match message {
            Message::Load { spec } => {
                let slot = state.slot_for(&spec.plugin);
                state.slots[slot].pending = true;
                // A load is on the path to the plugin's first call, so it goes
                // where that call goes: queued behind it, activation would
                // overtake the module and find nothing to instantiate.
                state.push(slot, Lane::Interactive, 0, Job::Load(spec));
            }
            Message::Unload { plugin } => {
                let slot = state.slot_for(&plugin);
                state.push(slot, Lane::Interactive, 0, Job::Unload);
            }
            Message::Activate { id, plugin, load } => {
                let slot = state.slot_for(&plugin);
                // A module to give it: the load goes in the same mailbox, behind
                // whatever the plugin is doing and ahead of its activation.
                if let Some(spec) = load {
                    state.slots[slot].pending = true;
                    state.push(slot, Lane::Interactive, 0, Job::Load(spec));
                }
                state.push(slot, Lane::Interactive, id, Job::Activate);
            }
            Message::Deactivate { id, plugin } => {
                let slot = state.slot_for(&plugin);
                state.push(slot, Lane::Interactive, id, Job::Deactivate);
            }
            Message::Kick { plugin } => {
                let slot = state.slot_for(&plugin);
                state.push(slot, Lane::Background, 0, Job::Kick);
            }
            Message::Event {
                id,
                plugin,
                events,
                topics,
            } => {
                let slot = state.slot_for(&plugin);
                state.push(slot, Lane::Background, id, Job::Event(events, topics));
            }
            Message::Hook { id, chain, request } => {
                // The parent orders a chain by dependencies and then by id; the
                // helper runs it in that order as one exchange, so the chain
                // holds every plugin in it rather than each waiting its turn.
                let mut slots = Vec::with_capacity(chain.len());
                for plugin in &chain {
                    slots.push(state.slot_for(plugin));
                }
                let Some(&owner) = slots.first() else {
                    return;
                };
                let lane = request.hook.budget().lane;
                let task = Task {
                    id,
                    lane,
                    slots: Occupied::Chain(slots),
                    job: Job::Hook(request),
                };
                state.push_task(owner, task);
            }
            Message::Command {
                id,
                plugin,
                command,
            } => {
                let slot = state.slot_for(&plugin);
                state.push(slot, Lane::Interactive, id, Job::Command(command));
            }
            Message::Invoke {
                id,
                plugin,
                function,
                args_base64,
            } => {
                let slot = state.slot_for(&plugin);
                state.push(
                    slot,
                    Lane::Interactive,
                    id,
                    Job::Invoke {
                        function,
                        args_base64,
                    },
                );
            }
            Message::Decorate {
                id,
                plugin,
                message,
                request,
            } => {
                let slot = state.slot_for(&plugin);
                let request = serde_json::to_value(request).unwrap_or(serde_json::Value::Null);
                // A drawing waits behind whatever the plugin is already doing,
                // in the background lane: the row shows plain until it comes.
                state.push(
                    slot,
                    Lane::Background,
                    id,
                    Job::Decorate { message, request },
                );
            }
            Message::Identity {
                id,
                plugin,
                request,
            } => {
                let slot = state.slot_for(&plugin);
                let server = request.server.clone();
                let person = request.person.clone();
                let request = serde_json::to_value(request).unwrap_or(serde_json::Value::Null);
                // One answer serves every row the person wrote, so it is worth
                // waiting behind other work for: the person is drawn as Yapfast
                // already knows them until it comes.
                state.push(
                    slot,
                    Lane::Background,
                    id,
                    Job::Identity {
                        person,
                        server,
                        request,
                    },
                );
            }
            Message::CodeBlock {
                id,
                plugin,
                request,
            } => {
                let slot = state.slot_for(&plugin);
                let block = request.block.clone();
                let text = request.text.clone();
                let request = serde_json::to_value(request).unwrap_or(serde_json::Value::Null);
                // Only the blocks on screen are asked about, and the answer is
                // kept by the block's own hash, so the same snippet twice is
                // one answer.
                state.push(
                    slot,
                    Lane::Background,
                    id,
                    Job::CodeBlock {
                        block,
                        text,
                        request,
                    },
                );
            }
            Message::DecorateMember {
                id,
                plugin,
                member,
                request,
            } => {
                let slot = state.slot_for(&plugin);
                let request = serde_json::to_value(request).unwrap_or(serde_json::Value::Null);
                // A badge is a nicety over a list that scrolls regardless, so it
                // waits in the background lane like a message's decoration.
                state.push(
                    slot,
                    Lane::Background,
                    id,
                    Job::DecorateMember { member, request },
                );
            }
            Message::Input { id, plugin, input } => {
                let slot = state.slot_for(&plugin);
                // A click is a person waiting on nothing: it is queued ahead of
                // a decoration because it is what the person just did, and it
                // is not in the background lane where a screenful of rows is.
                state.push(slot, Lane::Interactive, id, Job::Input { input });
            }
            Message::Return {
                plugin,
                call,
                answer,
            } => {
                let slot = state.slot_for(&plugin);
                state.push(slot, Lane::Interactive, 0, Job::Return { call, answer });
            }
            Message::Evict { plugin } => {
                let slot = state.slot_for(&plugin);
                state.push(slot, Lane::Interactive, 0, Job::Evict);
            }
        }
        drop(state);
        self.work.notify_all();
    }

    /// Queues the eviction a lowered budget wants, as work for a worker.
    ///
    /// The same mailbox and lane the parent's own `Evict` takes, but a job that
    /// is conditional: the victims were chosen under the state lock and a
    /// worker carries the drop out later, so the worker is the one that sees
    /// whether the instance may still go and whether the budget still needs it
    /// gone. This runs on the pipe loop, which must never touch the runtime.
    fn queue_evict_for_limits(&self, plugin: &PluginId) {
        let mut state = lock(&self.state);
        let slot = state.slot_for(plugin);
        state.push(slot, Lane::Interactive, 0, Job::EvictIfIdle);
        drop(state);
        self.work.notify_all();
    }

    /// Records what a guest left a plugin waiting on.
    ///
    /// The runtime calls this from the thread running the guest, which is why
    /// the slot is looked up rather than assumed: a guest speaks for a plugin
    /// the scheduler has a mailbox for, and one that has none has nothing that
    /// could be evicted either.
    fn waits(&self, plugin: &PluginId, wait: Wait) {
        let mut state = lock(&self.state);
        let Some(&slot) = state.index.get(plugin) else {
            return;
        };
        let slot = &mut state.slots[slot];
        match wait {
            Wait::Call(call) => slot.waiting.push(call),
            Wait::Timer(on) => slot.timer = on,
        }
    }

    /// Runs one task on this worker.
    fn run(&self, task: &Task) {
        match &task.job {
            Job::Load(spec) => self.load(spec),
            Job::Unload => self.unload(task.owner()),
            Job::Evict => {
                let slot = task.owner();
                if self.is_live(slot) {
                    self.evict(slot);
                }
            }
            // A lowered budget's victim, checked against the world as it is
            // now rather than as it was when the victim was chosen, and
            // dropped in the same breath as the check.
            Job::EvictIfIdle => self.evict_for_limits(task.owner()),
            Job::Activate => self.activate(task.owner(), task.id),
            Job::Deactivate => self.deactivate(task.owner(), task.id),
            Job::Kick => self.kick(task.owner()),
            Job::Event(events, topics) => self.event(task, events, topics),
            Job::Hook(request) => self.hook(task, request),
            Job::Command(command) => self.command(task, command),
            Job::Invoke {
                function,
                args_base64,
            } => self.invoke(task, function, args_base64),
            Job::Decorate { message, request } => self.decorate(task, message, request),
            Job::DecorateMember { member, request } => self.decorate_member(task, member, request),
            Job::Input { input } => self.input(task, input),
            Job::Identity {
                person,
                server,
                request,
            } => self.identity(task, person, server.as_deref(), request),
            Job::CodeBlock {
                block,
                text,
                request,
            } => self.code_block(task, block, text, request),
            Job::Return { call, answer } => self.answer(task.owner(), *call, answer),
        }
    }

    // Everything below runs on a worker, with no lock held on the way in.

    /// Takes a module into the runtime. Loading instantiates nothing, so it
    /// isn't a status the page shows; a module that won't load is a fault.
    fn load(&self, spec: &LoadSpec) {
        let slot = self.slot(&spec.plugin);
        let (result, late) = self.call(&[slot], START_BUDGET, |backend| backend.load(spec.clone()));
        {
            let mut state = lock(&self.state);
            let entry = &mut state.slots[slot];
            entry.memory_pages = spec.memory_pages;
            entry.loaded = result.is_ok().then(|| spec.clone());
        }
        if late {
            self.discard(slot, deadline_fault("loading the module"), None);
        } else if let Err(fault) = result {
            self.discard(slot, fault, None);
        }
    }

    /// Runs a plugin's `yf_input`: what the person did with something it drew.
    ///
    /// Nothing is answered, so nothing is waited for. A plugin that doesn't
    /// write the export is skipped rather than called: the call would answer a
    /// fault the runtime can't tell from a trap, and discarding a good instance
    /// because it chose not to listen to clicks would be wrong.
    fn input(&self, task: &Task, input: &yapfast_plugin_api::message::InputEvent) {
        let slot = task.owner();
        let plugin = self.id(slot);
        if self.ensure_active(slot).is_none() {
            return;
        }
        if !self.backend.has_export(&plugin, "yf_input") {
            return;
        }
        let Ok(bytes) = serde_json::to_vec(input) else {
            return;
        };
        let (result, late) = self.call(&[slot], INPUT_BUDGET, |backend| {
            backend.invoke(&plugin, "yf_input", &bytes)
        });
        match result {
            Ok(_) if late => self.discard(
                slot,
                deadline_fault("what a person did with what it drew"),
                None,
            ),
            Ok(_) => {}
            Err(fault) => self.discard(slot, fault, None),
        }
    }

    /// Drops a module and everything its instance held.
    fn unload(&self, slot: usize) {
        let plugin = self.id(slot);
        self.backend.unload(&plugin);
        {
            let mut state = lock(&self.state);
            let entry = &mut state.slots[slot];
            entry.live = false;
            entry.pending = false;
            entry.loaded = None;
            // Timers belong to the parent and an eviction leaves them alone,
            // but a module leaving takes the record of them with it, and the
            // parent forgets them with the plugin.
            entry.timer = false;
        }
        self.status(slot, PluginStatus::Installed);
        self.check_empty();
    }

    /// Instantiates and calls `yf_activate`, answering the parent's request.
    fn activate(&self, slot: usize, id: u64) {
        // Before anything else: a parent that asks for an activation is a person
        // turning the plugin on again, and that is the one thing that lifts a
        // suspension. Without it the status stayed `suspended` for the life of
        // the helper — nothing on this side clears it — and every later attempt
        // was refused with the reason that caused it, which the parent counts
        // as another strike.
        self.resume(slot);
        lock(&self.state).slots[slot].pending = false;
        match self.ensure_active(slot) {
            Some(handoff) => {
                let pages = self
                    .memory_pages(slot)
                    .unwrap_or_else(|| self.cap_pages(slot));
                let _ = self.reply(
                    id,
                    &FromHelper::Activated {
                        plugin: self.plugin(slot),
                        state: handoff,
                        memory_pages: pages,
                    },
                );
            }
            None => {
                // The parent is waiting on this request, so it is always
                // answered: silence here would leave a send or a command
                // waiting on a page nobody is going to update.
                let _ = self.reply(
                    id,
                    &FromHelper::Fault {
                        plugin: self.plugin(slot),
                        fault: self.refusal(slot),
                    },
                );
            }
        }
    }

    /// Calls `yf_deactivate` and drops the instance, keeping its hand-off.
    fn deactivate(&self, slot: usize, id: u64) {
        let plugin = self.id(slot);
        let live = self.is_live(slot);
        let mut handoff = None;
        let mut fault = None;
        if live {
            let (result, late) = self.call(&[slot], DEACTIVATE_BUDGET, |backend| {
                backend.deactivate(&plugin)
            });
            match result {
                Ok(state) => handoff = state,
                Err(error) => fault = Some(error),
            }
            self.set_live(slot, false);
            if late {
                fault = Some(deadline_fault("deactivating"));
            }
        }
        let _ = self.reply(
            id,
            &FromHelper::Deactivated {
                plugin: self.plugin(slot),
                state: handoff,
            },
        );
        match fault {
            Some(fault) => self.discard(slot, fault, None),
            None if live => self.status(slot, PluginStatus::Enabled),
            None => {}
        }
    }

    /// Runs an instance again with nothing new, which resets its idle clock and
    /// lets a repeat timer's tick reach the guest as an empty batch.
    fn kick(&self, slot: usize) {
        if self.ensure_active(slot).is_none() {
            return;
        }
        let plugin = self.id(slot);
        let (result, late) = self.call(&[slot], EVENT_BUDGET, |backend| {
            backend.event(&plugin, &[], &[])
        });
        if late {
            self.discard(slot, deadline_fault("a timer tick"), None);
            return;
        }
        if let Err(fault) = result {
            self.discard(slot, fault, None);
        }
    }

    /// Hands one batch of events to a guest.
    fn event(&self, task: &Task, events: &[EventEnvelope], topics: &[TopicEvent]) {
        let slot = task.owner();
        if self.ensure_active(slot).is_none() {
            let _ = self.reply(
                task.id,
                &FromHelper::EventDone {
                    plugin: self.plugin(slot),
                },
            );
            return;
        }
        let plugin = self.id(slot);
        let (result, late) = self.call(&[slot], EVENT_BUDGET, |backend| {
            backend.event(&plugin, events, topics)
        });
        let _ = self.reply(
            task.id,
            &FromHelper::EventDone {
                plugin: self.plugin(slot),
            },
        );
        if late {
            self.discard(slot, deadline_fault("an event batch"), None);
        } else if let Err(fault) = result {
            self.discard(slot, fault, None);
        }
    }

    /// Runs a whole hook chain, one plugin at a time.
    ///
    /// Each plugin gets its own deadline and the chain has a whole-chain one:
    /// a plugin that hangs costs its own budget and the chain moves on with the
    /// subject as it was, and a chain whose time is spent stops where it is.
    /// That is what makes a plugin unable to hold the person waiting, and what
    /// makes the two budgets in the contract more than a sentence.
    fn hook(&self, task: &Task, request: &HookRequest) {
        let mut chain = Vec::with_capacity(task.slots.len());
        for &slot in task.slots.as_slice() {
            if self.ensure_active(slot).is_some() {
                chain.push((slot, self.id(slot)));
            }
        }
        if chain.is_empty() {
            let _ = self.reply(
                task.id,
                &FromHelper::HookDone {
                    outcome: HookOutcome::Unchanged,
                    failures: Vec::new(),
                    by: None,
                },
            );
            return;
        }
        let budget = request.hook.budget();
        let per_plugin = Duration::from_millis(budget.per_plugin_ms);
        let whole = budget
            .chain_ms
            .map(|ms| Instant::now() + Duration::from_millis(ms));
        let mut asked = request.clone();
        let mut run = HookRun::unchanged();
        for (slot, plugin) in &chain {
            // This plugin's own budget, and no more than what is left of the
            // chain's: the last plugin of a chain that has nearly spent it gets
            // the rest, and one that is out of time doesn't run at all.
            let step = whole.map_or(per_plugin, |end| {
                per_plugin.min(end.saturating_duration_since(Instant::now()))
            });
            if step.is_zero() {
                debug!(%plugin, "a hook chain ran out of time before this plugin's turn");
                break;
            }
            let (answer, late) = self.call(&[*slot], step, |backend| backend.hook(plugin, &asked));
            if late {
                // The instance stopped wherever the cancel found it, so it is
                // thrown away; the chain goes on as if it had said nothing.
                let fault = deadline_fault("a hook");
                self.discard(*slot, fault.clone(), None);
                run.failures.push(((**plugin).clone(), fault));
                continue;
            }
            match answer {
                // The next plugin sees what this one produced, and the chain
                // answers with the last subject anyone produced: a later
                // plugin that leaves it alone doesn't take that back.
                Ok(HookOutcome::Changed(subject)) => {
                    asked.subject = subject.clone();
                    run.outcome = Some(HookOutcome::Changed(subject));
                    run.by = Some((**plugin).clone());
                }
                Ok(block @ HookOutcome::Block { .. }) => {
                    run.outcome = Some(block);
                    run.by = Some((**plugin).clone());
                    break;
                }
                Ok(HookOutcome::Unchanged) => {}
                // A hook fails open: the next plugin sees the subject as it
                // was, because a plugin that trapped has no opinion on it.
                Err(fault) => {
                    self.discard(*slot, fault.clone(), None);
                    run.failures.push(((**plugin).clone(), fault));
                }
            }
        }
        let _ = self.reply(
            task.id,
            &FromHelper::HookDone {
                outcome: run.outcome.unwrap_or(HookOutcome::Unchanged),
                failures: run.failures,
                by: run.by,
            },
        );
    }

    /// Runs one command someone typed.
    fn command(&self, task: &Task, command: &CommandInvocation) {
        let slot = task.owner();
        if self.ensure_active(slot).is_none() {
            let _ = self.reply(
                task.id,
                &FromHelper::CommandDone {
                    plugin: self.plugin(slot),
                    outcome: CommandOutcome::Failed {
                        message: "the plugin isn't running".into(),
                    },
                },
            );
            return;
        }
        let plugin = self.id(slot);
        let (result, late) = self.call(&[slot], COMMAND_BUDGET, |backend| {
            backend.command(&plugin, command)
        });
        let outcome = match &result {
            Ok(outcome) => outcome.clone(),
            Err(fault) => CommandOutcome::Failed {
                message: fault.reason.clone(),
            },
        };
        let _ = self.reply(
            task.id,
            &FromHelper::CommandDone {
                plugin: self.plugin(slot),
                outcome,
            },
        );
        match result {
            Ok(_) if late => self.discard(slot, deadline_fault("a command"), None),
            Ok(_) => {}
            Err(fault) => self.discard(slot, fault, None),
        }
    }

    /// Asks a plugin what it draws under a message on screen, and answers the
    /// parent with the tree or with nothing.
    ///
    /// The row is drawn plain while this runs: a decoration is worth a frame
    /// and not a wait, which is why the budget is the plan's 16 ms.
    fn decorate(&self, task: &Task, message: &str, request: &serde_json::Value) {
        let slot = task.owner();
        let plugin = self.id(slot);
        let nothing = || FromHelper::Decorated {
            plugin: plugin.as_ref().clone(),
            message: message.to_owned(),
            tree: None,
            hide: false,
            hide_media: false,
        };
        if self.ensure_active(slot).is_none() {
            let _ = self.reply(task.id, &nothing());
            return;
        }
        let Ok(bytes) = serde_json::to_vec(request) else {
            let _ = self.reply(task.id, &nothing());
            return;
        };
        let (result, late) = self.call(&[slot], DECORATE_BUDGET, |backend| {
            backend.invoke(&plugin, "yf_decorate", &bytes)
        });
        let decoration = match &result {
            // An export that answered nothing, or answered `null`, draws
            // nothing: a decoration is optional, so an empty one is no fault.
            Ok(Some(answer)) => serde_json::from_slice::<serde_json::Value>(answer)
                .ok()
                .and_then(yapfast_plugin_api::message::Decoration::parse),
            Ok(None) => None,
            Err(_) => None,
        }
        .unwrap_or_default();
        let _ = self.reply(
            task.id,
            &FromHelper::Decorated {
                plugin: plugin.as_ref().clone(),
                message: message.to_owned(),
                tree: decoration.tree,
                hide: decoration.hide,
                hide_media: decoration.hide_media,
            },
        );
        match result {
            Ok(_) if late => self.discard(slot, deadline_fault("a message decoration"), None),
            Ok(_) => {}
            Err(fault) => self.discard(slot, fault, None),
        }
    }

    /// Asks a plugin how one person looks, and answers the parent with the
    /// answer or with nothing.
    ///
    /// The person is drawn as Yapfast already knows them while this runs, which
    /// is why the budget is 16 ms: the answer is kept by the parent and shown
    /// everywhere that person appears.
    fn identity(
        &self,
        task: &Task,
        person: &str,
        server: Option<&str>,
        request: &serde_json::Value,
    ) {
        let slot = task.owner();
        let plugin = self.id(slot);
        let nothing = || FromHelper::Identity {
            plugin: plugin.as_ref().clone(),
            person: person.to_owned(),
            server: server.map(str::to_owned),
            identity: None,
        };
        if self.ensure_active(slot).is_none() {
            let _ = self.reply(task.id, &nothing());
            return;
        }
        let Ok(bytes) = serde_json::to_vec(request) else {
            let _ = self.reply(task.id, &nothing());
            return;
        };
        let (result, late) = self.call(&[slot], IDENTITY_BUDGET, |backend| {
            backend.invoke(&plugin, "yf_identity", &bytes)
        });
        let identity = match &result {
            // An export that said nothing, or answered `null`, changes nothing:
            // an answer is optional, so an empty one is no fault.
            Ok(Some(answer)) => {
                serde_json::from_slice::<yapfast_plugin_api::identity::Identity>(answer).ok()
            }
            Ok(None) => None,
            Err(_) => None,
        };
        let _ = self.reply(
            task.id,
            &FromHelper::Identity {
                plugin: plugin.as_ref().clone(),
                person: person.to_owned(),
                server: server.map(str::to_owned),
                identity,
            },
        );
        match result {
            Ok(_) if late => self.discard(slot, deadline_fault("how someone looks"), None),
            Ok(_) => {}
            Err(fault) => self.discard(slot, fault, None),
        }
    }

    /// Asks a plugin to colour one code block, and answers the parent with the
    /// spans it gave or with nothing.
    ///
    /// The block is drawn plain while this runs, which is what it looked like
    /// anyway: colours are worth a frame and not a wait.
    fn code_block(&self, task: &Task, block: &str, text: &str, request: &serde_json::Value) {
        let slot = task.owner();
        let plugin = self.id(slot);
        let nothing = || FromHelper::CodeBlock {
            plugin: plugin.as_ref().clone(),
            block: block.to_owned(),
            spans: None,
        };
        if self.ensure_active(slot).is_none() {
            let _ = self.reply(task.id, &nothing());
            return;
        }
        let Ok(bytes) = serde_json::to_vec(request) else {
            let _ = self.reply(task.id, &nothing());
            return;
        };
        let (result, late) = self.call(&[slot], CODE_BLOCK_BUDGET, |backend| {
            backend.invoke(&plugin, "yf_code_block", &bytes)
        });
        let spans = match &result {
            // An export that answered nothing, or answered `null`, colours
            // nothing: an answer is optional, so an empty one is no fault.
            Ok(Some(answer)) => {
                serde_json::from_slice::<Vec<yapfast_plugin_api::code::Span>>(answer)
                    .ok()
                    .filter(|spans| yapfast_plugin_api::code::fits(text, spans))
            }
            Ok(None) => None,
            Err(_) => None,
        };
        let _ = self.reply(
            task.id,
            &FromHelper::CodeBlock {
                plugin: plugin.as_ref().clone(),
                block: block.to_owned(),
                spans,
            },
        );
        match result {
            Ok(_) if late => self.discard(slot, deadline_fault("a code block's colours"), None),
            Ok(_) => {}
            Err(fault) => self.discard(slot, fault, None),
        }
    }

    fn decorate_member(&self, task: &Task, member: &str, request: &serde_json::Value) {
        let slot = task.owner();
        let plugin = self.id(slot);
        let nothing = || FromHelper::MemberDecorated {
            plugin: plugin.as_ref().clone(),
            member: member.to_owned(),
            badge: None,
        };
        if self.ensure_active(slot).is_none() {
            let _ = self.reply(task.id, &nothing());
            return;
        }
        let Ok(bytes) = serde_json::to_vec(request) else {
            let _ = self.reply(task.id, &nothing());
            return;
        };
        let (result, late) = self.call(&[slot], DECORATE_BUDGET, |backend| {
            backend.invoke(&plugin, "yf_member_decorate", &bytes)
        });
        let badge = match &result {
            // An export that answered nothing draws no badge, which is what a
            // plugin that doesn't badge this person answers.
            Ok(Some(answer)) => serde_json::from_slice::<serde_json::Value>(answer)
                .ok()
                .filter(|badge| !badge.is_null()),
            Ok(None) => None,
            Err(_) => None,
        };
        let _ = self.reply(
            task.id,
            &FromHelper::MemberDecorated {
                plugin: plugin.as_ref().clone(),
                member: member.to_owned(),
                badge,
            },
        );
        match result {
            Ok(_) if late => self.discard(slot, deadline_fault("a member badge"), None),
            Ok(_) => {}
            Err(fault) => self.discard(slot, fault, None),
        }
    }

    /// Calls one export of one plugin for another: what `plugin.call` asked the
    /// parent for, run where the callee's instance is.
    fn invoke(&self, task: &Task, function: &str, args_base64: &str) {
        let slot = task.owner();
        if self.ensure_active(slot).is_none() {
            let _ = self.reply(
                task.id,
                &FromHelper::Invoked {
                    answer_base64: None,
                    fault: Some(self.refusal(slot)),
                },
            );
            return;
        }
        let Some(args) = base64::decode(args_base64) else {
            let _ = self.reply(
                task.id,
                &FromHelper::Invoked {
                    answer_base64: None,
                    fault: Some(Fault {
                        kind: FaultKind::BadAnswer,
                        reason: format!("the arguments of `{function}` weren't base64"),
                    }),
                },
            );
            return;
        };
        let plugin = self.id(slot);
        let (result, late) = self.call(&[slot], INVOKE_BUDGET, |backend| {
            backend.invoke(&plugin, function, &args)
        });
        let answer = match result {
            Ok(bytes) => (bytes.map(|bytes| base64::encode(&bytes)), None),
            Err(fault) => (None, Some(fault)),
        };
        let _ = self.reply(
            task.id,
            &FromHelper::Invoked {
                answer_base64: answer.0,
                fault: answer.1,
            },
        );
        if late {
            self.discard(slot, deadline_fault("a call from another plugin"), None);
        }
    }

    /// Hands a guest the answer the parent sent for a call it made earlier.
    fn answer(&self, slot: usize, call: u64, answer: &HostReturn) {
        // The call is settled whether or not the instance that asked is still
        // there, and by its own id: the parent also answers posts, which no
        // instance was ever held for, so anything else would release a hold that
        // belongs to a call still out.
        {
            let mut state = lock(&self.state);
            let entry = &mut state.slots[slot];
            if let Some(at) = entry.waiting.iter().position(|&pending| pending == call) {
                entry.waiting.swap_remove(at);
            }
        }
        if !self.is_live(slot) {
            // The instance stopped at an arbitrary point and was discarded since
            // it asked; there is nobody left to tell.
            debug!("an answer arrived for a call no live instance has");
            return;
        }
        let plugin = self.id(slot);
        let (result, late) = self.call(&[slot], EVENT_BUDGET, |backend| {
            backend.answer(&plugin, call, answer)
        });
        if late {
            self.discard(slot, deadline_fault("an answer"), None);
        } else if let Err(fault) = result {
            self.discard(slot, fault, None);
        }
    }

    // Instances, admissions and evictions.

    /// Instantiates the plugin if it isn't up; `Some(state)` is the hand-off the
    /// last `yf_deactivate` left, and `None` means it can't be run.
    fn ensure_active(&self, slot: usize) -> Option<Option<String>> {
        let (plugin, suspended, live, loaded) = {
            let state = lock(&self.state);
            let entry = &state.slots[slot];
            (
                entry.id.clone(),
                matches!(entry.status, PluginStatus::Suspended { .. }),
                entry.live,
                entry.loaded.is_some(),
            )
        };
        if suspended {
            return None;
        }
        if live {
            return Some(None);
        }
        if !loaded {
            // The parent sent work for a plugin it never loaded, which is a bug
            // there rather than a strike here.
            debug!(%plugin, "no module is loaded for this plugin; the work is dropped");
            return None;
        }
        if !self.admit(slot) {
            return None;
        }
        let (result, late) = self.call(&[slot], START_BUDGET, |backend| backend.activate(&plugin));
        if late {
            self.discard(slot, deadline_fault("activating"), None);
            return None;
        }
        match result {
            Ok(handoff) => {
                self.set_live(slot, true);
                self.status(slot, PluginStatus::Active);
                Some(handoff)
            }
            Err(fault) => {
                self.discard(slot, fault, None);
                None
            }
        }
    }

    /// Whether one more instance of this plugin's cap fits in the helper's
    /// budget — the live one, which the person may have lowered or raised with
    /// [`Scheduler::set_limits`]. Over it, the least recently used idle
    /// instance is evicted; if there is none, the activation is refused with a
    /// fault the parent shows, naming the budget in force.
    fn admit(&self, slot: usize) -> bool {
        let wanted = u64::from(self.cap_pages(slot)) * PAGE_BYTES;
        loop {
            let (budget, victim) = {
                let mut state = lock(&self.state);
                let mut used = 0u64;
                let mut victim: Option<(usize, Instant)> = None;
                for (index, entry) in state.slots.iter().enumerate() {
                    if !entry.live {
                        continue;
                    }
                    used += u64::from(entry.memory_pages) * PAGE_BYTES;
                    if index == slot || entry.busy || !entry.unheld() || entry.has_work() {
                        continue;
                    }
                    if victim.is_none_or(|(_, at)| entry.last_call < at) {
                        victim = Some((index, entry.last_call));
                    }
                }
                if used + wanted <= state.budget_bytes {
                    // Reserve the cap before the backend call. Another worker
                    // admitting a different plugin must see it in `used` now,
                    // not only after this activation returns.
                    state.slots[slot].live = true;
                    return true;
                }
                let victim = victim.map(|(index, _)| index);
                if let Some(victim) = victim {
                    // Reserve this drop too, so two workers cannot select the
                    // same idle instance and count its room twice. The backend
                    // drop still happens outside the lock in `evict`.
                    state.slots[victim].live = false;
                }
                (state.budget_bytes, victim)
            };
            match victim {
                Some(victim) => self.evict(victim),
                None => {
                    let fault = Fault {
                        kind: FaultKind::ActivateFailed,
                        reason: format!(
                            "the helper's {} MiB of plugin memory is taken",
                            budget >> 20
                        ),
                    };
                    // Full memory is the helper's limit, not this plugin's
                    // doing, so it is reported without a strike.
                    self.status(
                        slot,
                        PluginStatus::Faulted {
                            reason: fault.reason.clone(),
                            memory_pages: self.cap_pages(slot),
                        },
                    );
                    self.report(&FromHelper::Fault {
                        plugin: self.plugin(slot),
                        fault,
                    });
                    return false;
                }
            }
        }
    }

    /// Drops a lowered budget's victim, if the world still wants it gone.
    ///
    /// Both halves of the question are asked against the live state in one
    /// critical section: the eligibility [`State::over_budget`] chose the
    /// victim by — live, with nothing a guest is waiting on and no work in its
    /// mailbox — and whether the live caps still exceed the live budget at
    /// all. Another drop may already have made room, or the person may have
    /// raised the budget since the victim was chosen; then the eviction is not
    /// needed and nothing happens. A queued victim is never dropped to meet a
    /// number that is already met.
    ///
    /// A drop both checks allow is committed in that same critical section, by
    /// clearing the instance before the lock goes. That is what makes the
    /// check and the drop one step: work that arrives after it finds a slot
    /// that is not live and takes the ordinary path to a fresh instance
    /// through `ensure_active`, and no admission can count a cap that is
    /// already logically dead. The runtime is told outside the state lock, as
    /// every backend call is.
    ///
    /// The task asking holds its plugin busy by its own claim and has taken
    /// itself out of the mailbox already, so neither says anything here; what
    /// this sees is what arrived since the choice — a host call a guest is now
    /// waiting on, a timer, work queued behind this very task — and each of
    /// those keeps the instance. Work that ran and left no hold and no queue
    /// behind it doesn't: the instance is as idle as the choice found it.
    fn evict_for_limits(&self, slot: usize) {
        let plugin = {
            let mut state = lock(&self.state);
            let entry = &state.slots[slot];
            let eligible = entry.live && entry.unheld() && !entry.has_work();
            let used: u64 = state
                .slots
                .iter()
                .filter(|entry| entry.live)
                .map(|entry| u64::from(entry.memory_pages) * PAGE_BYTES)
                .sum();
            if !eligible || used <= state.budget_bytes {
                return;
            }
            let entry = &mut state.slots[slot];
            entry.pending = false;
            entry.live = false;
            Arc::clone(&entry.id)
        };
        self.backend.evict(&plugin);
        self.status(slot, PluginStatus::Enabled);
        self.check_empty();
    }

    /// Drops an instance because it has been idle, which is how its guest
    /// memory comes back: WebAssembly can't shrink.
    fn evict(&self, slot: usize) {
        lock(&self.state).slots[slot].pending = false;
        self.drop_instance(slot);
        self.status(slot, PluginStatus::Enabled);
        self.check_empty();
    }

    /// Discards an instance that stopped at an arbitrary point: a trap, an
    /// out-of-memory, a deadline. It re-instantiates on its next trigger, and
    /// enough faults inside the window suspend it for the session.
    fn discard(&self, slot: usize, fault: Fault, reply: Option<u64>) {
        self.drop_instance(slot);
        let (suspended, status) = {
            let mut state = lock(&self.state);
            let now = Instant::now();
            let window = Duration::from_millis(limits::STRIKE_WINDOW_MS);
            let entry = &mut state.slots[slot];
            entry.strikes.retain(|at| now.duration_since(*at) < window);
            entry.strikes.push_back(now);
            let suspended = entry.strikes.len() as u32 >= limits::MAX_STRIKES;
            let status = if suspended {
                PluginStatus::Suspended {
                    reason: fault.reason.clone(),
                }
            } else {
                PluginStatus::Faulted {
                    reason: fault.reason.clone(),
                    memory_pages: entry.memory_pages,
                }
            };
            entry.status = status.clone();
            (suspended, status)
        };
        self.report(&FromHelper::Status {
            plugin: self.plugin(slot),
            status,
        });
        let reported = FromHelper::Fault {
            plugin: self.plugin(slot),
            fault,
        };
        match reply {
            Some(id) => {
                let _ = self.reply(id, &reported);
            }
            None => self.report(&reported),
        }
        if suspended {
            self.suspend(slot);
        }
        self.check_empty();
    }

    /// Stops a plugin for the session. Work already queued is answered as
    /// failed, so the parent isn't left waiting on a reply that will never come.
    fn suspend(&self, slot: usize) {
        let abandoned: Vec<Arc<Task>> = {
            let mut state = lock(&self.state);
            let entry = &mut state.slots[slot];
            let mut abandoned: Vec<Arc<Task>> = entry.interactive.drain(..).collect();
            abandoned.extend(entry.background.drain(..));
            entry.queued = false;
            abandoned
        };
        for task in abandoned {
            self.abandon(&task);
        }
    }

    /// Lifts a suspension, for an activation the parent asked for.
    ///
    /// The strikes go with it: they were the price of the failures that stopped
    /// the plugin, and a plugin a person turned back on is one that starts with
    /// a clean account. There is no message that carries a strike count, so the
    /// parent's own idea of the plugin is the status it is told here — `enabled`
    /// first, so a row that said `suspended` says why it isn't any more — and the
    /// `active` or `faulted` that follows the activation.
    fn resume(&self, slot: usize) {
        let lifted = {
            let mut state = lock(&self.state);
            let entry = &mut state.slots[slot];
            let suspended = matches!(entry.status, PluginStatus::Suspended { .. });
            if suspended {
                entry.status = PluginStatus::Enabled;
                entry.strikes.clear();
            }
            suspended
        };
        if lifted {
            self.status(slot, PluginStatus::Enabled);
        }
    }

    /// Answers a task the scheduler will never run.
    fn abandon(&self, task: &Task) {
        let slot = task.owner();
        let plugin = self.plugin(slot);
        let suspended = "the plugin is suspended".to_string();
        match &task.job {
            Job::Activate => {
                let _ = self.reply(
                    task.id,
                    &FromHelper::Fault {
                        plugin,
                        fault: Fault {
                            kind: FaultKind::ActivateFailed,
                            reason: suspended,
                        },
                    },
                );
            }
            Job::Deactivate => {
                let _ = self.reply(
                    task.id,
                    &FromHelper::Deactivated {
                        plugin,
                        state: None,
                    },
                );
            }
            Job::Event(..) => {
                let _ = self.reply(task.id, &FromHelper::EventDone { plugin });
            }
            Job::Invoke { function, .. } => {
                let _ = self.reply(
                    task.id,
                    &FromHelper::Invoked {
                        answer_base64: None,
                        fault: Some(Fault {
                            kind: FaultKind::Trap,
                            reason: format!("`{function}` wasn't called: {suspended}"),
                        }),
                    },
                );
            }
            Job::Command(_) => {
                let _ = self.reply(
                    task.id,
                    &FromHelper::CommandDone {
                        plugin,
                        outcome: CommandOutcome::Failed { message: suspended },
                    },
                );
            }
            Job::Hook(_) => {
                let _ = self.reply(
                    task.id,
                    &FromHelper::HookDone {
                        outcome: HookOutcome::Unchanged,
                        failures: Vec::new(),
                        by: None,
                    },
                );
            }
            // A decoration that never runs leaves the row plain, which is what
            // it looked like while the plugin was asked.
            Job::Decorate { message, .. } => {
                let _ = self.reply(
                    task.id,
                    &FromHelper::Decorated {
                        plugin,
                        message: message.clone(),
                        tree: None,
                        hide: false,
                        hide_media: false,
                    },
                );
            }
            // A person nobody answered about is drawn as Yapfast already knows
            // them.
            Job::Identity { person, server, .. } => {
                let _ = self.reply(
                    task.id,
                    &FromHelper::Identity {
                        plugin,
                        person: person.clone(),
                        server: server.clone(),
                        identity: None,
                    },
                );
            }
            // A block nobody coloured is drawn plain, which is what it looked
            // like while the plugin was asked.
            Job::CodeBlock { block, .. } => {
                let _ = self.reply(
                    task.id,
                    &FromHelper::CodeBlock {
                        plugin,
                        block: block.clone(),
                        spans: None,
                    },
                );
            }
            // A badge that never runs leaves the name as it was drawn.
            Job::DecorateMember { member, .. } => {
                let _ = self.reply(
                    task.id,
                    &FromHelper::MemberDecorated {
                        plugin,
                        member: member.clone(),
                        badge: None,
                    },
                );
            }
            // Nothing answers an input, a load, an unload, a tick, an eviction
            // or an answer: there is no one waiting on any of them.
            Job::Load(_)
            | Job::Unload
            | Job::Evict
            | Job::EvictIfIdle
            | Job::Kick
            | Job::Input { .. }
            | Job::Return { .. } => {}
        }
    }

    /// Tells the run loop to finish once no instance and no work is left: the
    /// helper is disposable, and the parent starts a new one on the next
    /// activation.
    fn check_empty(&self) {
        let empty = {
            let state = lock(&self.state);
            !state.slots.is_empty()
                && !state
                    .slots
                    .iter()
                    .any(|slot| slot.live || slot.has_work() || slot.busy || slot.pending)
                && state.running.is_empty()
        };
        if empty {
            self.waker.finish();
        }
    }

    // The one place a backend call is made, with the deadline it must fit in.

    /// Runs one backend call while the watchdog holds its deadline. The flag is
    /// whether the deadline passed, in which case the instance is discarded
    /// later: it stopped wherever the cancel found it.
    ///
    /// The cancel triggers are taken before the call starts, because the
    /// watchdog must reach a guest that is inside a call and holding its own
    /// plugin's lock: that is the whole reason they exist.
    fn call<T>(&self, slots: &[usize], budget: Duration, call: impl FnOnce(&B) -> T) -> (T, bool) {
        // The names first, so the state lock is released before the backend's.
        // The deadline is stamped once the call has the backend, not before:
        // a call queued behind another's backend lock hasn't started, and
        // charging it for the wait would fault a plugin that never ran.
        let plugins: Vec<_> = slots.iter().map(|&slot| self.id(slot)).collect();
        // Registered before the backend is asked for, so a worker waiting its
        // turn is visible to the watchdog and the other worker gets a look in.
        let serial = {
            let mut state = lock(&self.state);
            state.serial += 1;
            let serial = state.serial;
            state.running.push(Running {
                serial,
                deadline: None,
                fired: false,
                triggers: Triggers::none(),
            });
            serial
        };
        let mut triggers = Triggers::none();
        for plugin in &plugins {
            if let Some(token) = self.backend.cancel_handle(plugin) {
                triggers.push(token);
            }
        }
        // The clock starts here, with nothing left to wait for but the plugin's
        // own lock.
        {
            let mut state = lock(&self.state);
            if let Some(running) = state.running.iter_mut().find(|r| r.serial == serial) {
                running.triggers = triggers;
                running.deadline = Some(Instant::now() + budget);
            }
        }
        self.watch.notify_all();
        let value = call(&self.backend);
        let mut state = lock(&self.state);
        match state.running.iter().position(|r| r.serial == serial) {
            Some(index) => {
                let running = state.running.remove(index);
                drop(state);
                if running.fired {
                    // Fired once more now that the worker is back. A cancel the
                    // runtime had nowhere to put — the deadline passed before the
                    // guest was inside the call — was dropped there, and this is
                    // the cheapest way to say "stop" to an instance that is about
                    // to be discarded anyway rather than only throwing it away.
                    for trigger in running.triggers.as_slice() {
                        trigger();
                    }
                }
                (value, running.fired)
            }
            None => (value, false),
        }
    }

    /// Fires a deadline that has passed.
    ///
    /// The triggers were taken when the call started, so the watchdog needs no
    /// lock the call is holding and the cancel reaches the guest mid-call, which
    /// is the only moment it matters. They stay with the entry rather than being
    /// consumed, because a single fire can be swallowed: the runtime drops a
    /// cancel aimed at a call it isn't handing yet, and the guest then runs to
    /// the 30 s backstop while the parent gave up long ago. The watchdog fires
    /// again until the worker's call comes back and removes the entry.
    fn fire(&self, serial: u64) {
        // Fired with the state lock held: the entry is the only thing that says
        // the call is still out, and a fire that landed after it went would
        // cancel whatever that plugin does next.
        let mut state = lock(&self.state);
        let Some(running) = state.running.iter_mut().find(|r| r.serial == serial) else {
            return;
        };
        running.fired = true;
        for trigger in running.triggers.as_slice() {
            trigger();
        }
    }

    // Small helpers that keep the call sites readable.

    /// The slot for a plugin, making one if the parent named a plugin the
    /// scheduler hasn't seen: its mailbox exists before its module does, so work
    /// that arrives first still keeps its order.
    fn slot(&self, plugin: &PluginId) -> usize {
        let mut state = lock(&self.state);
        state.slot_for(plugin)
    }

    fn id(&self, slot: usize) -> Arc<PluginId> {
        lock(&self.state).slots[slot].id.clone()
    }

    fn plugin(&self, slot: usize) -> PluginId {
        (*self.id(slot)).clone()
    }

    fn is_live(&self, slot: usize) -> bool {
        lock(&self.state).slots[slot].live
    }

    fn set_live(&self, slot: usize, live: bool) {
        lock(&self.state).slots[slot].live = live;
    }

    fn cap_pages(&self, slot: usize) -> u32 {
        lock(&self.state).slots[slot].memory_pages
    }

    fn memory_pages(&self, slot: usize) -> Option<u32> {
        self.backend.memory_pages(&self.id(slot))
    }

    fn last_fault(&self, slot: usize) -> Option<Fault> {
        match &lock(&self.state).slots[slot].status {
            PluginStatus::Faulted { reason, .. } => Some(Fault {
                kind: FaultKind::ActivateFailed,
                reason: reason.clone(),
            }),
            PluginStatus::Suspended { reason } => Some(Fault {
                kind: FaultKind::ActivateFailed,
                reason: reason.clone(),
            }),
            _ => None,
        }
    }

    /// Why an activation didn't happen, in words a person can read.
    fn refusal(&self, slot: usize) -> Fault {
        self.last_fault(slot).unwrap_or_else(|| Fault {
            kind: FaultKind::ActivateFailed,
            reason: "no module is loaded for this plugin".into(),
        })
    }

    fn drop_instance(&self, slot: usize) {
        let plugin = self.id(slot);
        self.backend.evict(&plugin);
        lock(&self.state).slots[slot].live = false;
    }

    fn status(&self, slot: usize, status: PluginStatus) {
        {
            let mut state = lock(&self.state);
            state.slots[slot].status = status.clone();
        }
        self.report(&FromHelper::Status {
            plugin: self.plugin(slot),
            status,
        });
    }

    fn report(&self, report: &FromHelper) {
        if let Err(error) = self.outbox.report(report) {
            warn!(%error, "a report never reached Yapfast");
        }
    }

    fn reply(&self, id: u64, report: &FromHelper) -> bool {
        // A task with no id is one nothing pairs with: an event batch the parent
        // sent to be delivered, or a timer's tick.
        if id == 0 {
            return false;
        }
        match self.outbox.reply(id, report) {
            Ok(()) => true,
            Err(error) => {
                warn!(%error, "an answer never reached Yapfast");
                false
            }
        }
    }
}

/// A fault for a call that ran past its deadline. The instance is discarded
/// because the cancel stopped it at an arbitrary point.
fn deadline_fault(what: &str) -> Fault {
    Fault {
        kind: FaultKind::Deadline,
        reason: format!("{what} ran past its deadline"),
    }
}

/// The earlier of two wakeups.
fn earliest(current: Option<Instant>, candidate: Instant) -> Option<Instant> {
    Some(current.map_or(candidate, |current| current.min(candidate)))
}

/// Takes a lock, ignoring poisoning: one panicking worker must not stop the
/// rest from running plugins.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The work in a plugin's queue.
enum Job {
    Load(LoadSpec),
    Unload,
    Evict,
    /// A lowered budget's eviction, dropped only if the instance is still one
    /// the budget may take when a worker reaches it.
    EvictIfIdle,
    Activate,
    Deactivate,
    Kick,
    Event(Vec<EventEnvelope>, Vec<TopicEvent>),
    Hook(HookRequest),
    Command(CommandInvocation),
    Invoke {
        function: String,
        args_base64: String,
    },
    DecorateMember {
        member: String,
        request: serde_json::Value,
    },
    Input {
        input: yapfast_plugin_api::message::InputEvent,
    },
    Decorate {
        message: String,
        request: serde_json::Value,
    },
    Identity {
        person: String,
        server: Option<String>,
        request: serde_json::Value,
    },
    CodeBlock {
        block: String,
        text: String,
        request: serde_json::Value,
    },
    Return {
        call: u64,
        answer: HostReturn,
    },
}

/// One unit of work, with the reply frame it answers.
struct Task {
    /// The parent's frame id, or `0` when nothing waits for an answer.
    id: u64,
    lane: Lane,
    /// The plugins it occupies: one for most work, the whole chain for a hook.
    slots: Occupied,
    job: Job,
}

impl Task {
    /// The plugin whose mailbox holds the task, and which a single-plugin task
    /// is about.
    fn owner(&self) -> usize {
        self.slots.as_slice()[0]
    }
}

/// The plugins a task occupies, without an allocation for the usual case of
/// one: every event and command would otherwise need a vector.
enum Occupied {
    One(usize),
    Chain(Vec<usize>),
}

impl Occupied {
    fn as_slice(&self) -> &[usize] {
        match self {
            Occupied::One(slot) => std::slice::from_ref(slot),
            Occupied::Chain(slots) => slots,
        }
    }

    fn len(&self) -> usize {
        self.as_slice().len()
    }
}

/// One plugin's place in the scheduler.
struct Slot {
    id: Arc<PluginId>,
    /// Work the person is waiting for: hooks, commands, clicks, activation.
    interactive: VecDeque<Arc<Task>>,
    /// Work nobody is waiting for: events, timer ticks, loading.
    background: VecDeque<Arc<Task>>,
    /// Whether it is in the rotation already.
    queued: bool,
    /// Whether a worker holds it, 0 or 1, so it is never on two at once.
    busy: bool,
    /// Whether an instance is up, or an activation in progress has reserved its cap.
    live: bool,
    /// The module in the runtime, once the parent has loaded it.
    loaded: Option<LoadSpec>,
    /// A module the parent loaded and hasn't activated yet.
    ///
    /// Loading and activating are two phases: the parent loads a plugin's whole
    /// dependency closure in parallel and then activates it, so between the
    /// two there is a slot with a module in the runtime and no instance. That
    /// is work in progress, not an idle helper: counting it as idle let the
    /// helper leave in the gap, and the activation that arrived a moment later
    /// reached a closed pipe.
    pending: bool,
    /// The pages its manifest allows, which is what admission control sums: a
    /// ceiling, not what the instance touched.
    memory_pages: u32,
    /// When a fault last counted against it.
    strikes: VecDeque<Instant>,
    status: PluginStatus,
    /// The last call it was given, for idle eviction.
    last_call: Instant,
    /// The host calls the parent hasn't answered, by the id it will answer
    /// under.
    ///
    /// By id rather than counted: the parent answers every request a guest
    /// sends, including the posts nothing waits on, and an answer that arrived
    /// for one of those mustn't release a hold another call is still using.
    waiting: Vec<u64>,
    /// Whether it holds a timer.
    timer: bool,
}

impl Slot {
    fn new(id: PluginId) -> Self {
        Slot {
            id: Arc::new(id),
            interactive: VecDeque::new(),
            background: VecDeque::new(),
            queued: false,
            busy: false,
            live: false,
            loaded: None,
            pending: false,
            memory_pages: limits::DEFAULT_MEMORY_PAGES,
            strikes: VecDeque::new(),
            status: PluginStatus::Installed,
            last_call: Instant::now(),
            waiting: Vec::new(),
            timer: false,
        }
    }

    fn has_work(&self) -> bool {
        !self.interactive.is_empty() || !self.background.is_empty()
    }

    /// The task the mailbox would run next: the interactive lane first, FIFO
    /// within a lane.
    fn head(&self) -> Option<&Arc<Task>> {
        self.interactive.front().or_else(|| self.background.front())
    }

    fn pop(&mut self) -> Option<Arc<Task>> {
        if self.interactive.is_empty() {
            self.background.pop_front()
        } else {
            self.interactive.pop_front()
        }
    }

    /// Nothing is waiting on it, so its memory can go back.
    fn unheld(&self) -> bool {
        self.waiting.is_empty() && !self.timer
    }
}

/// A call the watchdog holds a deadline for.
///
/// No slot is kept here: the worker that made the call is the one that knows
/// which instances it was for, and it is the one that discards them when the
/// call comes back late.
struct Running {
    serial: u64,
    /// When the call has to be done by, or `None` while it is still queued
    /// behind another call's hold on the backend.
    ///
    /// A call waiting for the backend hasn't started, and charging it for the
    /// wait would fault a plugin that never ran; so the entry is registered
    /// before the wait, for the workers to see, and the clock starts when the
    /// call does.
    deadline: Option<Instant>,
    /// Set when the deadline passed, so the worker knows the call ran late.
    fired: bool,
    /// Taken from the runtime when the call started, and fired by the
    /// watchdog when the deadline passes.
    triggers: Triggers,
}

/// What fires a call's deadline: one trigger for most work, one per plugin for
/// a chain, and nothing at all for a runtime that offers none.
#[derive(Default)]
enum Triggers {
    #[default]
    None,
    One(CancelToken),
    Many(Vec<CancelToken>),
}

impl Triggers {
    fn none() -> Self {
        Triggers::None
    }

    fn push(&mut self, trigger: CancelToken) {
        match self {
            Triggers::None => *self = Triggers::One(trigger),
            Triggers::One(first) => {
                let first = Arc::clone(first);
                *self = Triggers::Many(vec![first, trigger]);
            }
            Triggers::Many(triggers) => triggers.push(trigger),
        }
    }

    fn as_slice(&self) -> &[CancelToken] {
        match self {
            Triggers::None => &[],
            Triggers::One(trigger) => std::slice::from_ref(trigger),
            Triggers::Many(triggers) => triggers,
        }
    }
}

/// Everything the scheduler owns, behind one lock.
struct State {
    slots: Vec<Slot>,
    index: HashMap<PluginId, usize>,
    /// Plugins with queued work, in the order they were given it: the rotation
    /// is what gives two busy plugins a turn each.
    ready: VecDeque<usize>,
    running: Vec<Running>,
    serial: u64,
    idle_after: Duration,
    /// The helper's whole budget for plugin memory, in bytes: what admission
    /// and eviction hold the live caps against.
    budget_bytes: u64,
    stopping: bool,
}

impl State {
    fn new(idle_after: Duration, budget_bytes: u64) -> Self {
        State {
            slots: Vec::new(),
            index: HashMap::new(),
            ready: VecDeque::new(),
            running: Vec::with_capacity(2),
            serial: 0,
            idle_after,
            budget_bytes,
            stopping: false,
        }
    }

    /// The instances a lower budget has no room for, least recently used first,
    /// and only those that may go.
    ///
    /// The eligibility is the one admission's victims have: a busy instance,
    /// one a guest is waiting on, one holding a timer or with work in its
    /// mailbox is not dropped to meet a number, and the helper stays over
    /// budget for as long as only such instances are in the way. The choice is
    /// one moment's view, and a worker checks each victim again where it drops
    /// it — against the holds, the work and the budget the world has then: the
    /// world moves between here and there.
    fn over_budget(&self) -> Vec<PluginId> {
        let cap = |slot: &Slot| u64::from(slot.memory_pages) * PAGE_BYTES;
        let mut used: u64 = self.slots.iter().filter(|slot| slot.live).map(cap).sum();
        let mut victims = Vec::new();
        let mut taken = vec![false; self.slots.len()];
        while used > self.budget_bytes {
            let victim = self
                .slots
                .iter()
                .enumerate()
                .filter(|(index, slot)| {
                    slot.live && !taken[*index] && !slot.busy && slot.unheld() && !slot.has_work()
                })
                .min_by_key(|(_, slot)| slot.last_call)
                .map(|(index, slot)| (index, cap(slot)));
            let Some((index, bytes)) = victim else {
                break;
            };
            taken[index] = true;
            used -= bytes;
            victims.push((*self.slots[index].id).clone());
        }
        victims
    }

    /// The slot for a plugin, making one if it is new.
    fn slot_for(&mut self, plugin: &PluginId) -> usize {
        if let Some(&slot) = self.index.get(plugin) {
            return slot;
        }
        let slot = self.slots.len();
        self.slots.push(Slot::new(plugin.clone()));
        self.index.insert(plugin.clone(), slot);
        slot
    }

    /// Queues a single-plugin task.
    fn push(&mut self, slot: usize, lane: Lane, id: u64, job: Job) {
        self.push_task(
            slot,
            Task {
                id,
                lane,
                slots: Occupied::One(slot),
                job,
            },
        );
    }

    /// Queues a task and makes sure its mailbox is in the rotation.
    fn push_task(&mut self, slot: usize, task: Task) {
        let entry = &mut self.slots[slot];
        let task = Arc::new(task);
        match task.lane {
            Lane::Interactive => entry.interactive.push_back(task),
            Lane::Background => entry.background.push_back(task),
        }
        let first = !entry.queued;
        entry.queued = true;
        if first {
            self.ready.push_back(slot);
        }
    }

    /// The next claimable task: interactive work first, whoever queued it, then
    /// background work from a plugin's own place in the rotation.
    fn claim(&mut self, take_background: bool) -> Option<Arc<Task>> {
        loop {
            // A fault can empty a mailbox while its plugin is in the rotation.
            if let Some(position) = self
                .ready
                .iter()
                .position(|&slot| !self.slots[slot].has_work())
            {
                if let Some(slot) = self.ready.remove(position) {
                    self.slots[slot].queued = false;
                }
                continue;
            }
            for lane in [Lane::Interactive, Lane::Background] {
                if lane == Lane::Background && !take_background {
                    return None;
                }
                let mut chosen = None;
                for position in 0..self.ready.len() {
                    let slot = self.ready[position];
                    let Some(head) = self.slots[slot].head() else {
                        continue;
                    };
                    if head.lane != lane {
                        continue;
                    }
                    // A chain holds every plugin in it, so it waits until all
                    // of them are free.
                    if head
                        .slots
                        .as_slice()
                        .iter()
                        .any(|&busy| self.slots[busy].busy)
                    {
                        continue;
                    }
                    chosen = Some(position);
                    break;
                }
                let Some(position) = chosen else {
                    continue;
                };
                let slot = self.ready.remove(position)?;
                let task = self.slots[slot].pop()?;
                for &busy in task.slots.as_slice() {
                    self.slots[busy].busy = true;
                }
                let entry = &mut self.slots[slot];
                entry.queued = entry.has_work();
                if entry.queued {
                    self.ready.push_back(slot);
                }
                return Some(task);
            }
            return None;
        }
    }
}
