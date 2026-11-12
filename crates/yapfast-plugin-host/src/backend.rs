//! What the scheduler asks of a WebAssembly runtime, and nothing more.
//!
//! The scheduler decides *when* and *in what order* plugins run: its own lanes,
//! mailboxes and deadlines. What runs them is a [`Backend`], which the Extism
//! implementation in `runtime` provides and which needs no knowledge of
//! priorities. Keeping the two apart is also what lets the scheduler's tests
//! drive it with a fake backend that traps or sleeps on cue.

use std::path::PathBuf;
use std::sync::Arc;

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{CommandInvocation, CommandOutcome, HostReturn};
use yapfast_plugin_api::event::{EventEnvelope, TopicEvent};
use yapfast_plugin_api::hook::{HookOutcome, HookRequest};
use yapfast_plugin_api::message::Fault;

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

    /// Calls `yf_deactivate`, and drops the instance. The answer is its
    /// hand-off for the next activation, cut to the limit.
    fn deactivate(&self, plugin: &PluginId) -> Result<Option<String>, Fault>;

    /// One batch of events, as `yf_event`; `topics` is what other plugins
    /// published to this one's topics, which arrives in the same call.
    fn event(
        &self,
        plugin: &PluginId,
        events: &[EventEnvelope],
        topics: &[TopicEvent],
    ) -> Result<(), Fault>;

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

    /// Answers a guest's host call that was left pending.
    fn answer(&self, plugin: &PluginId, call: u64, answer: &HostReturn) -> Result<(), Fault>;

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
