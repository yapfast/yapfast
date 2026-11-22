//! What the scheduler asks of a WebAssembly runtime, and nothing more.
//!
//! The scheduler decides *when* and *in what order* plugins run: its own lanes,
//! mailboxes and deadlines. What runs them is a [`Backend`], which the Extism
//! implementation in `runtime` provides and which needs no knowledge of
//! priorities. Keeping the two apart is also what lets the scheduler's tests
//! drive it with a fake backend that traps or sleeps on cue.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::value::RawValue;
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{CommandInvocation, CommandOutcome};
use yapfast_plugin_api::event::{EventEnvelope, TopicEvent};
use yapfast_plugin_api::guest::CallResult;
use yapfast_plugin_api::hook::{HookOutcome, HookRequest};
use yapfast_plugin_api::message::Fault;

/// One batch of events and published topics, as the JSON the parent wrote.
///
/// The parent serializes each event once for every plugin that wants it, and
/// the guest reads the same JSON. Decoding each payload into a tree here only to
/// write it out again was a third of what an event cost the helper, so the two
/// arrays are kept as they arrived and [`Events::batch`] wraps them in the
/// object `yf_event` takes.
#[derive(Clone, Debug)]
pub struct Events {
    events: Box<RawValue>,
    /// Left out when nothing was published, as the batch leaves it out.
    topics: Option<Box<RawValue>>,
    /// The timers that fired, by tag, for a tick: left out of the batch when
    /// none did.
    timers: Vec<String>,
}

impl Events {
    /// A batch from its two arrays as they came off the pipe.
    pub fn raw(events: &RawValue, topics: Option<&RawValue>) -> Self {
        Events {
            events: events.to_owned(),
            topics: topics
                .filter(|topics| !empty(topics))
                .map(RawValue::to_owned),
            timers: Vec::new(),
        }
    }

    /// A batch from decoded events, for a frame that came the slow way and for
    /// tests. An event is JSON already, so encoding one can't fail; if it ever
    /// did, the batch is delivered empty rather than not at all.
    pub fn new(events: &[EventEnvelope], topics: &[TopicEvent]) -> Self {
        let none = || RawValue::from_string("[]".to_owned()).unwrap_or_else(|_| unreachable!());
        Events {
            events: serde_json::value::to_raw_value(events).unwrap_or_else(|_| none()),
            topics: (!topics.is_empty())
                .then(|| serde_json::value::to_raw_value(topics).unwrap_or_else(|_| none())),
            timers: Vec::new(),
        }
    }

    /// The same batch, saying which timers fired.
    pub fn with_timers(mut self, timers: Vec<String>) -> Self {
        self.timers = timers;
        self
    }

    /// Nothing in it at all: a timer's tick, which the parent sends as an empty
    /// batch.
    pub fn is_empty(&self) -> bool {
        empty(&self.events) && self.topics.is_none() && self.timers.is_empty()
    }

    /// The events, decoded: for a test that wants to see what was delivered.
    pub fn envelopes(&self) -> Vec<EventEnvelope> {
        serde_json::from_str(self.events.get()).unwrap_or_default()
    }

    /// The payload `yf_event` takes: `{"events":[…],"results":[],"topics":[…]}`,
    /// the arrays copied in as they are.
    pub fn batch(&self) -> Vec<u8> {
        let events = self.events.get();
        let topics = self.topics.as_deref().map(RawValue::get);
        let mut out = Vec::with_capacity(
            events.len()
                + topics.map_or(0, str::len)
                + r#"{"events":,"results":[],"topics":}"#.len(),
        );
        out.extend_from_slice(br#"{"events":"#);
        out.extend_from_slice(events.as_bytes());
        out.extend_from_slice(br#","results":[]"#);
        if let Some(topics) = topics {
            out.extend_from_slice(br#","topics":"#);
            out.extend_from_slice(topics.as_bytes());
        }
        if !self.timers.is_empty()
            && let Ok(timers) = serde_json::to_vec(&self.timers)
        {
            out.extend_from_slice(br#","timers":"#);
            out.extend_from_slice(&timers);
        }
        out.push(b'}');
        out
    }
}

/// Whether a JSON array is `[]`, however it is spaced.
fn empty(array: &RawValue) -> bool {
    array
        .get()
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .eq(*b"[]")
}

/// Fires the deadline of the call a plugin is running.
pub type CancelToken = Arc<dyn Fn() + Send + Sync>;

/// Something a guest left its plugin waiting on, which the runtime sees and the
/// scheduler keeps the account of.
///
/// A guest that makes a host call returns instead of waiting for the answer, and
/// one that sets a timer returns before it fires. Either way there is more to
/// lose than the memory the instance occupies: evicting it would drop the work
/// it is in the middle of, and the answer would arrive for a call no live
/// instance has. So the runtime reports both, and the scheduler keeps such an
/// instance off the idle and memory-pressure eviction lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// A host call the parent will answer under this id. The guest has already
    /// returned; only that answer resumes it.
    Call(u64),
    /// Whether the guest holds a timer the parent hasn't taken away.
    Timer(bool),
}

/// Where a runtime reports those waits: the scheduler's own account, reached
/// from the thread running a guest.
pub type Waits = Arc<dyn Fn(&PluginId, Wait) + Send + Sync>;

/// A module to load, with the limits its manifest gave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadSpec {
    pub plugin: PluginId,
    /// The `.wasm` in the content store, already hash-checked by the caller.
    pub wasm: PathBuf,
    /// Pages of guest memory, 64 KiB each.
    pub memory_pages: u32,
    /// Whether the guest needs WASI, as Go and JavaScript builds do.
    pub wasi: bool,
    /// The hand-off its last `yf_deactivate` left, when this load replaces a
    /// running version: the state crosses the swap, so an update doesn't cost
    /// the plugin its memory of where it was.
    pub state: Option<String>,
    /// Whether it sits in a hook chain, which keeps its instance from idle
    /// eviction: see [`yapfast_plugin_api::message::Activation::hooks`].
    pub hooks: bool,
}

/// Runs one plugin's calls.
///
/// Every method is called from one worker at a time for a given plugin, so the
/// implementation needs no locking for its own instances. A call that fails
/// returns a [`Fault`], which the scheduler counts as a strike.
pub trait Backend: Send + Sync {
    /// Takes a module and its limits; an instance is made on activation, so
    /// loading many modules costs memory only for their compiled code.
    fn load(&self, spec: LoadSpec) -> Result<(), Fault>;

    /// Drops a module, and any instance of it.
    fn unload(&self, plugin: &PluginId);

    /// Instantiates if needed and calls `yf_activate`. `Ok` is the state the
    /// last `yf_deactivate` handed on, if any.
    fn activate(&self, plugin: &PluginId) -> Result<Option<String>, Fault>;

    /// Whether this plugin's module writes that export.
    ///
    /// The optional ones are asked this before they are called: a call to an
    /// export a module doesn't have answers a fault the runtime can't tell from
    /// a trap, and discarding a good instance over it would be wrong.
    fn has_export(&self, plugin: &PluginId, export: &str) -> bool;

    /// Calls `yf_deactivate`, and drops the instance. The answer is its
    /// hand-off for the next activation, cut to the limit.
    fn deactivate(&self, plugin: &PluginId) -> Result<Option<String>, Fault>;

    /// One batch of events, as `yf_event`, with what other plugins published to
    /// this one's topics, which arrives in the same call.
    fn event(&self, plugin: &PluginId, events: &Events) -> Result<(), Fault>;

    /// The whole hook chain, in the order given, as one exchange: each plugin
    /// sees the previous one's result.
    ///
    /// One plugin's answer to one hook, with the subject the chain carries.
    /// The chain itself is the scheduler's: it decides how long each plugin
    /// gets, so a call here is always one plugin's own budget.
    fn hook(&self, plugin: &PluginId, request: &HookRequest) -> Result<HookOutcome, Fault>;

    /// One command, as `yf_command`.
    fn command(
        &self,
        plugin: &PluginId,
        command: &CommandInvocation,
    ) -> Result<CommandOutcome, Fault>;

    /// One export of one plugin, called on behalf of another: `plugin.call`.
    ///
    /// The bytes are the caller's and go to the callee's export unchanged, and
    /// what it returns is what the caller gets. `None` is an export that
    /// answered nothing, which is a call with no answer rather than a failure.
    fn invoke(
        &self,
        plugin: &PluginId,
        function: &str,
        args: &[u8],
    ) -> Result<Option<Vec<u8>>, Fault>;

    /// Answers host calls a guest left pending, in the order the parent sent
    /// them: every answer queued for the plugin goes in one `yf_event`, so K
    /// answers cost one guest call rather than K.
    fn answer(&self, plugin: &PluginId, results: &[CallResult]) -> Result<(), Fault>;

    /// Drops an instance, keeping nothing. Used for idle eviction, for the
    /// memory budget, and after a fault.
    fn evict(&self, plugin: &PluginId);

    /// The pages an instance holds now, for the page and the budget.
    fn memory_pages(&self, plugin: &PluginId) -> Option<u32>;

    /// A trigger for the call this plugin is running, if it is running one.
    ///
    /// A deadline is fired from the watchdog, while the call holds the backend,
    /// so the scheduler takes this once and calls it from another thread. The
    /// instance stops at an arbitrary point and is therefore discarded.
    fn cancel_handle(&self, plugin: &PluginId) -> Option<CancelToken>;

    /// Takes the place to report what a guest leaves the plugin waiting on.
    ///
    /// A host call and a timer both outlive the call that made them, and only
    /// the runtime sees either: the guest's request goes straight to the parent
    /// rather than through the scheduler. A runtime that doesn't report them
    /// leaves every instance looking idle, which is what evicts a plugin
    /// waiting to be resumed.
    fn set_wait_sink(&self, _waits: Waits) {}
}

/// A chain's answer, and whoever failed on the way.
///
/// A failure doesn't stop the chain: the next plugin sees the subject as it
/// was, because hooks fail open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HookRun {
    /// The last subject any plugin produced, or the one asked about.
    pub outcome: Option<HookOutcome>,
    /// The plugin that produced [`HookRun::outcome`], when one changed or
    /// stopped the subject.
    pub by: Option<PluginId>,
    pub failures: Vec<(PluginId, Fault)>,
}

impl HookRun {
    /// The answer of a chain nobody changed.
    pub fn unchanged() -> Self {
        HookRun {
            outcome: Some(HookOutcome::Unchanged),
            by: None,
            failures: Vec::new(),
        }
    }
}

/// Host calls answered while the guest waits for them.
///
/// A guest's `yf_call` used to answer `pending` for everything, with the real
/// answer only in the next `yf_event` batch, which left a PDK's plain reads —
/// a setting, a stored value, the open room — unable to return anything.
/// Most calls are answered by the parent in well under a millisecond, so the
/// thread running the guest now waits for the answer, up to [`SYNC_WAIT`] and
/// never past the deadline of the guest call it is in (a hook's 25 ms, say),
/// and only answers `pending` when it hasn't come by then. The pipe's reader
/// hands an answer to a waiting call here before the scheduler would queue
/// it, so the wait holds no lock the reader needs.
pub mod answers {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex, MutexGuard, mpsc};
    use std::time::{Duration, Instant};

    use yapfast_plugin_api::PluginId;
    use yapfast_plugin_api::call::HostReturn;

    /// The longest a guest's call waits for the parent before it is answered
    /// `pending` instead. The parent's own answers take well under a
    /// millisecond; one that goes through the window waits for its next
    /// update, a frame or two.
    pub const SYNC_WAIT: Duration = Duration::from_millis(250);

    /// What is left of a guest call's deadline when a host call waits: a
    /// little, so the guest can still use the answer and return in time.
    pub const MARGIN: Duration = Duration::from_millis(3);

    type Waiting = HashMap<(PluginId, u64), mpsc::SyncSender<HostReturn>>;

    static WAITING: LazyLock<Mutex<Waiting>> = LazyLock::new(Mutex::default);

    thread_local! {
        static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
    }

    fn waiting() -> MutexGuard<'static, Waiting> {
        WAITING
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Runs a guest call with its deadline known to the host calls it makes on
    /// this thread.
    pub fn within<T>(deadline: Instant, call: impl FnOnce() -> T) -> T {
        let before = DEADLINE.replace(Some(deadline));
        let value = call();
        DEADLINE.set(before);
        value
    }

    /// How long a host call made now may wait: none when the guest call it is
    /// in has too little left.
    pub fn allowance() -> Option<Duration> {
        let now = Instant::now();
        let cap = match DEADLINE.get() {
            Some(deadline) => deadline.checked_duration_since(now)?.checked_sub(MARGIN)?,
            None => SYNC_WAIT,
        };
        let wait = cap.min(SYNC_WAIT);
        (!wait.is_zero()).then_some(wait)
    }

    /// One call about to wait: register it before the call goes out, so an
    /// answer that overtakes the wait still finds it.
    pub struct Waiter {
        key: (PluginId, u64),
        answer: mpsc::Receiver<HostReturn>,
    }

    impl Waiter {
        pub fn new(plugin: &PluginId, call: u64) -> Waiter {
            let (sender, answer) = mpsc::sync_channel(1);
            let key = (plugin.clone(), call);
            waiting().insert(key.clone(), sender);
            Waiter { key, answer }
        }

        /// The answer, if it came within `wait`. Otherwise `on_late` runs while
        /// no answer can be handed over, and the call is left to the
        /// scheduler's way: the answer, when it comes, goes in a batch.
        pub fn wait(self, wait: Duration, on_late: impl FnOnce()) -> Option<HostReturn> {
            if let Ok(answer) = self.answer.recv_timeout(wait) {
                return Some(answer);
            }
            let mut waiting = waiting();
            if waiting.remove(&self.key).is_none() {
                // Handed over between the timeout and the lock: it is in the
                // channel now.
                drop(waiting);
                return self.answer.try_recv().ok();
            }
            on_late();
            None
        }

        /// The call never went out.
        pub fn cancel(self) {
            waiting().remove(&self.key);
        }
    }

    /// Hands an answer to the guest call waiting for it, or gives it back for
    /// the scheduler when none is.
    pub fn deliver(plugin: &PluginId, call: u64, answer: HostReturn) -> Option<HostReturn> {
        let mut waiting = waiting();
        match waiting.remove(&(plugin.clone(), call)) {
            Some(sender) => {
                // The channel holds one, and this is its only send.
                let _ = sender.try_send(answer);
                None
            }
            None => Some(answer),
        }
    }
}

#[cfg(test)]
mod timer_tests {
    use super::*;

    /// A tick names its timers in the batch, and a batch with none says
    /// nothing of them.
    #[test]
    fn a_tick_names_the_timers_that_fired() {
        let tick = Events::new(&[], &[]).with_timers(vec!["refresh".into(), "save".into()]);
        assert!(!tick.is_empty());
        let batch: yapfast_plugin_api::guest::EventBatch =
            serde_json::from_slice(&tick.batch()).expect("a batch");
        assert_eq!(batch.timers, vec!["refresh", "save"]);
        assert!(
            !String::from_utf8(Events::new(&[], &[]).batch())
                .unwrap()
                .contains("timers")
        );
    }
}
