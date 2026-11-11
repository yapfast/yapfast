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
//! The backend is one value behind one lock, because [`Backend`] hands out
//! `&mut self`: calls for different plugins interleave on the workers but never
//! overlap inside the runtime. The lanes still buy what they were for, since
//! whichever worker frees up takes the person's work first.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tracing::{debug, warn};

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{CommandInvocation, CommandOutcome, HostReturn};
use yapfast_plugin_api::event::EventEnvelope;
use yapfast_plugin_api::hook::{HookOutcome, HookRequest, Lane};
use yapfast_plugin_api::limits;
use yapfast_plugin_api::message::{Fault, FaultKind, FromHelper, PluginStatus};

use crate::backend::{Backend, CancelToken, LoadSpec};
use crate::pipe::{Outbox, Waker};

/// How long one batch of events may run. Events aren't on a person's path, so
/// this is looser than a hook's budget, but a batch that never returns holds
/// its mailbox and its instance's memory.
const EVENT_BUDGET: Duration = Duration::from_millis(250);

/// How long a command may run. A command was typed by a person who is waiting
/// for it, and the parent waits the same two seconds for the answer.
const COMMAND_BUDGET: Duration = Duration::from_secs(2);

/// How long `yf_deactivate` may take. Its answer is a convenience handed to the
/// next activation, so it gets long enough to write that and no longer: an
/// instance going away must not hold the helper.
const DEACTIVATE_BUDGET: Duration = Duration::from_millis(100);

/// How long instantiating a module and its `yf_activate` may take, and how long
/// compiling one may take: both are slow on a cold cache and neither is a
/// plugin misbehaving.
const START_BUDGET: Duration = Duration::from_secs(2);

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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            workers: 0,
            idle_after: Duration::from_millis(limits::IDLE_EVICTION_MS),
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

/// Something that keeps an instance from being idle, and so from being evicted.
///
/// A plugin holding a timer, or waiting for the parent to answer a call it made,
/// has more to lose than the memory it occupies: evicting it would drop the work
/// it is in the middle of. The runtime reports both through [`Scheduler::hold`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Held {
    /// A host call the parent has not answered yet.
    Request,
    /// A timer the guest asked for and has not cancelled.
    Timer,
}

/// What the parent asked for, once its frame is decoded.
#[derive(Clone, Debug)]
pub enum Message {
    /// Take a module and its limits, without activating it.
    Load { spec: LoadSpec },
    /// Drop a module and its instance, keeping nothing.
    Unload { plugin: PluginId },
    /// Instantiate if needed, then call `yf_activate`; answered with
    /// [`FromHelper::Activated`].
    Activate { id: u64, plugin: PluginId },
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
            state: Mutex::new(State::new(config.idle_after)),
            work: Condvar::new(),
            watch: Condvar::new(),
            backend: Mutex::new(backend),
            outbox,
            waker,
        });

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

    /// Declares that a plugin is waiting on something, so it isn't evicted as
    /// idle while it is.
    pub fn hold(&self, plugin: &PluginId, held: Held) {
        self.shared.hold(plugin, held, true);
    }

    /// Says the wait is over.
    pub fn release(&self, plugin: &PluginId, held: Held) {
        self.shared.hold(plugin, held, false);
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
    backend: Mutex<B>,
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
            if running.fired {
                continue;
            }
            let Some(deadline) = running.deadline else {
                continue;
            };
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
        if !overdue.is_empty() || !idle.is_empty() {
            drop(state);
            for serial in overdue {
                shared.fire(serial);
            }
            for index in idle {
                shared.evict(index);
            }
            state = lock(&shared.state);
            continue;
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
                // A load is on the path to the plugin's first call, so it goes
                // where that call goes: queued behind it, activation would
                // overtake the module and find nothing to instantiate.
                state.push(slot, Lane::Interactive, 0, Job::Load(spec));
            }
            Message::Unload { plugin } => {
                let slot = state.slot_for(&plugin);
                state.push(slot, Lane::Interactive, 0, Job::Unload);
            }
            Message::Activate { id, plugin } => {
                let slot = state.slot_for(&plugin);
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
            Message::Event { id, plugin, events } => {
                let slot = state.slot_for(&plugin);
                state.push(slot, Lane::Background, id, Job::Event(events));
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

    /// Records or forgets something a plugin is waiting on.
    fn hold(&self, plugin: &PluginId, held: Held, on: bool) {
        let mut state = lock(&self.state);
        let Some(&slot) = state.index.get(plugin) else {
            return;
        };
        let slot = &mut state.slots[slot];
        match held {
            Held::Request if on => slot.waiting += 1,
            Held::Request => slot.waiting = slot.waiting.saturating_sub(1),
            Held::Timer => slot.timer = on,
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
            Job::Activate => self.activate(task.owner(), task.id),
            Job::Deactivate => self.deactivate(task.owner(), task.id),
            Job::Kick => self.kick(task.owner()),
            Job::Event(events) => self.event(task, events),
            Job::Hook(request) => self.hook(task, request),
            Job::Command(command) => self.command(task, command),
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

    /// Drops a module and everything its instance held.
    fn unload(&self, slot: usize) {
        let plugin = self.id(slot);
        {
            let mut backend = lock(&self.backend);
            backend.unload(&plugin);
        }
        {
            let mut state = lock(&self.state);
            let entry = &mut state.slots[slot];
            entry.live = false;
            entry.loaded = None;
        }
        self.status(slot, PluginStatus::Installed);
        self.check_empty();
    }

    /// Instantiates and calls `yf_activate`, answering the parent's request.
    fn activate(&self, slot: usize, id: u64) {
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
        let (result, late) =
            self.call(&[slot], EVENT_BUDGET, |backend| backend.event(&plugin, &[]));
        if late {
            self.discard(slot, deadline_fault("a timer tick"), None);
            return;
        }
        if let Err(fault) = result {
            self.discard(slot, fault, None);
        }
    }

    /// Hands one batch of events to a guest.
    fn event(&self, task: &Task, events: &[EventEnvelope]) {
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
            backend.event(&plugin, events)
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

    /// Runs a whole hook chain as one exchange.
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
                },
            );
            return;
        }
        let plugins: Vec<PluginId> = chain.iter().map(|(_, plugin)| (**plugin).clone()).collect();
        let slots: Vec<usize> = chain.iter().map(|(slot, _)| *slot).collect();
        let budget = chain_budget(request, plugins.len());
        let (run, late) = self.call(&slots, budget, |backend| backend.hook(&plugins, request));
        if late {
            let fault = deadline_fault("a hook chain");
            let mut failures = Vec::with_capacity(slots.len());
            for (slot, plugin) in &chain {
                failures.push(((**plugin).clone(), fault.clone()));
                self.discard(*slot, fault.clone(), Some(task.id));
            }
            let _ = self.reply(
                task.id,
                &FromHelper::HookDone {
                    outcome: HookOutcome::Unchanged,
                    failures,
                },
            );
            return;
        }
        let mut failures = Vec::with_capacity(run.failures.len());
        for (plugin, fault) in &run.failures {
            if let Some(slot) = self.slot_of(plugin) {
                self.discard(slot, fault.clone(), Some(task.id));
            }
            failures.push((plugin.clone(), fault.clone()));
        }
        let _ = self.reply(
            task.id,
            &FromHelper::HookDone {
                outcome: run.outcome.unwrap_or(HookOutcome::Unchanged),
                failures,
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

    /// Hands a guest the answer the parent sent for a call it made earlier.
    fn answer(&self, slot: usize, call: u64, answer: &HostReturn) {
        if !self.is_live(slot) {
            // The instance stopped at an arbitrary point and was discarded since
            // it asked; there is nobody left to tell.
            debug!("an answer arrived for a call no live instance has");
            return;
        }
        let plugin = self.id(slot);
        {
            let mut state = lock(&self.state);
            state.slots[slot].waiting = state.slots[slot].waiting.saturating_sub(1);
        }
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
    /// budget. Over it, the least recently used idle instance is evicted; if
    /// there is none, the activation is refused with a fault the parent shows.
    fn admit(&self, slot: usize) -> bool {
        let wanted = u64::from(self.cap_pages(slot)) * PAGE_BYTES;
        loop {
            let (used, victim) = {
                let state = lock(&self.state);
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
                (used, victim.map(|(index, _)| index))
            };
            if used + wanted <= limits::MAX_HELPER_BYTES {
                return true;
            }
            match victim {
                Some(victim) => self.evict(victim),
                None => {
                    let fault = Fault {
                        kind: FaultKind::ActivateFailed,
                        reason: format!(
                            "the helper's {} MiB of plugin memory is taken",
                            limits::MAX_HELPER_BYTES >> 20
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

    /// Drops an instance because it has been idle, which is how its guest
    /// memory comes back: WebAssembly can't shrink.
    fn evict(&self, slot: usize) {
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
            Job::Event(_) => {
                let _ = self.reply(task.id, &FromHelper::EventDone { plugin });
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
                    },
                );
            }
            Job::Load(_) | Job::Unload | Job::Evict | Job::Kick | Job::Return { .. } => {}
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
                    .any(|slot| slot.live || slot.has_work() || slot.busy)
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
    /// watchdog can't touch the backend while the call holds it: that is the
    /// whole reason they exist.
    fn call<T>(
        &self,
        slots: &[usize],
        budget: Duration,
        call: impl FnOnce(&mut B) -> T,
    ) -> (T, bool) {
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
        let mut backend = lock(&self.backend);
        let mut triggers = Triggers::none();
        for plugin in &plugins {
            if let Some(token) = backend.cancel_handle(plugin) {
                triggers.push(token);
            }
        }
        // The clock starts here, with the backend in hand.
        {
            let mut state = lock(&self.state);
            if let Some(running) = state.running.iter_mut().find(|r| r.serial == serial) {
                running.triggers = triggers;
                running.deadline = Some(Instant::now() + budget);
            }
        }
        self.watch.notify_all();
        let value = call(&mut backend);
        drop(backend);
        let mut state = lock(&self.state);
        match state.running.iter().position(|r| r.serial == serial) {
            Some(index) => {
                let running = state.running.remove(index);
                drop(state);
                (value, running.fired)
            }
            None => (value, false),
        }
    }

    /// Fires a deadline that has passed. The triggers were taken when the call
    /// started, so the watchdog needs no lock the call is holding and the
    /// cancel reaches the guest mid-call, which is the only moment it matters.
    fn fire(&self, serial: u64) {
        let triggers = {
            let mut state = lock(&self.state);
            let Some(running) = state.running.iter_mut().find(|r| r.serial == serial) else {
                return;
            };
            if running.fired {
                return;
            }
            running.fired = true;
            std::mem::take(&mut running.triggers)
        };
        for trigger in triggers.as_slice() {
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

    fn slot_of(&self, plugin: &PluginId) -> Option<usize> {
        lock(&self.state).index.get(plugin).copied()
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
        let backend = lock(&self.backend);
        backend.memory_pages(&self.id(slot))
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
        {
            let mut backend = lock(&self.backend);
            backend.evict(&plugin);
        }
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

/// The whole chain's budget: what the hook allows for the exchange, or, when
/// nobody waits for the answer, one plugin's budget each.
fn chain_budget(request: &HookRequest, chain: usize) -> Duration {
    let budget = request.hook.budget();
    match budget.chain_ms {
        Some(chain_ms) => Duration::from_millis(chain_ms),
        None => Duration::from_millis(budget.per_plugin_ms * chain.max(1) as u64),
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
    Activate,
    Deactivate,
    Kick,
    Event(Vec<EventEnvelope>),
    Hook(HookRequest),
    Command(CommandInvocation),
    Return { call: u64, answer: HostReturn },
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
    /// Whether an instance is up.
    live: bool,
    /// The module in the runtime, once the parent has loaded it.
    loaded: Option<LoadSpec>,
    /// The pages its manifest allows, which is what admission control sums: a
    /// ceiling, not what the instance touched.
    memory_pages: u32,
    /// When a fault last counted against it.
    strikes: VecDeque<Instant>,
    status: PluginStatus,
    /// The last call it was given, for idle eviction.
    last_call: Instant,
    /// Host calls the parent hasn't answered.
    waiting: usize,
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
            memory_pages: limits::DEFAULT_MEMORY_PAGES,
            strikes: VecDeque::new(),
            status: PluginStatus::Installed,
            last_call: Instant::now(),
            waiting: 0,
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
        self.waiting == 0 && !self.timer
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
    stopping: bool,
}

impl State {
    fn new(idle_after: Duration) -> Self {
        State {
            slots: Vec::new(),
            index: HashMap::new(),
            ready: VecDeque::new(),
            running: Vec::with_capacity(2),
            serial: 0,
            idle_after,
            stopping: false,
        }
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
