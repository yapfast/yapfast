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
use yapfast_plugin_api::event::EventEnvelope;
use yapfast_plugin_api::hook::{HookOutcome, HookRequest};
use yapfast_plugin_api::message::Fault;

/// Fires the deadline of the call a plugin is running.
pub type CancelToken = Arc<dyn Fn() + Send + Sync>;

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
}

/// Runs one plugin's calls.
///
/// Every method is called from one worker at a time for a given plugin, so the
/// implementation needs no locking for its own instances. A call that fails
/// returns a [`Fault`], which the scheduler counts as a strike.
pub trait Backend: Send {
    /// Takes a module and its limits; an instance is made on activation, so
    /// loading many modules costs memory only for their compiled code.
    fn load(&mut self, spec: LoadSpec) -> Result<(), Fault>;

    /// Drops a module, and any instance of it.
    fn unload(&mut self, plugin: &PluginId);

    /// Instantiates if needed and calls `yf_activate`. `Ok` is the state the
    /// last `yf_deactivate` handed on, if any.
    fn activate(&mut self, plugin: &PluginId) -> Result<Option<String>, Fault>;

    /// Calls `yf_deactivate`, and drops the instance. The answer is its
    /// hand-off for the next activation, cut to the limit.
    fn deactivate(&mut self, plugin: &PluginId) -> Result<Option<String>, Fault>;

    /// One batch of events, as `yf_event`.
    fn event(&mut self, plugin: &PluginId, events: &[EventEnvelope]) -> Result<(), Fault>;

    /// The whole hook chain, in the order given, as one exchange: each plugin
    /// sees the previous one's result.
    fn hook(&mut self, chain: &[PluginId], request: &HookRequest) -> HookRun;

    /// One command, as `yf_command`.
    fn command(
        &mut self,
        plugin: &PluginId,
        command: &CommandInvocation,
    ) -> Result<CommandOutcome, Fault>;

    /// Answers a guest's host call that was left pending.
    fn answer(&mut self, plugin: &PluginId, call: u64, answer: &HostReturn) -> Result<(), Fault>;

    /// Drops an instance, keeping nothing. Used for idle eviction, for the
    /// memory budget, and after a fault.
    fn evict(&mut self, plugin: &PluginId);

    /// The pages an instance holds now, for the page and the budget.
    fn memory_pages(&self, plugin: &PluginId) -> Option<u32>;

    /// A trigger for the call this plugin is running, if it is running one.
    ///
    /// A deadline is fired from the watchdog, while the call holds the backend,
    /// so the scheduler takes this once and calls it from another thread. The
    /// instance stops at an arbitrary point and is therefore discarded.
    fn cancel_handle(&self, plugin: &PluginId) -> Option<CancelToken>;
}

/// A chain's answer, and whoever failed on the way.
///
/// A failure doesn't stop the chain: the next plugin sees the subject as it
/// was, because hooks fail open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HookRun {
    /// The last subject any plugin produced, or the one asked about.
    pub outcome: Option<HookOutcome>,
    pub failures: Vec<(PluginId, Fault)>,
}

impl HookRun {
    /// The answer of a chain nobody changed.
    pub fn unchanged() -> Self {
        HookRun {
            outcome: Some(HookOutcome::Unchanged),
            failures: Vec::new(),
        }
    }
}
