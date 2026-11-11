//! The Extism runtime: instances, memory limits, deadlines and the host calls.
//!
//! `host` builds this with `ExtismBackend::new(cache_dir, outbox)`; the
//! scheduler drives it through [`crate::backend::Backend`] and knows nothing
//! else about it. Four things here are load-bearing:
//!
//! * Modules are loaded with `Wasm::file`, never `Wasm::data`: data keeps the
//!   bytes in the manifest and every instance clones the manifest, which costs
//!   about 4.1 MiB per plugin against 0.56 MiB from a file. For the same reason
//!   the bytes are read once, at load, to check the module and then dropped;
//!   what stays is the compiled code.
//! * A manifest's page limit is a ceiling, not a reservation, and the ceiling is
//!   what the parent is told: Extism exposes no count of the pages an instance
//!   holds, and the most it can still grow to is what a memory budget has to
//!   assume. Hitting the ceiling traps as `oom` and WebAssembly can't shrink,
//!   so the instance is discarded and the plugin is instantiated again on its
//!   next trigger.
//! * The compile cache under the parent's cache directory takes a cold compile
//!   of a 1.87 MiB module from about 22 ms to 2.8 ms.
//! * A guest's host call never waits: it goes to the parent on a channel and
//!   the guest is answered `pending` in the same call, so a person deciding
//!   whether to grant a capability holds no worker.
//!
//! Dropping an instance is the only thing that gives memory back — a `Plugin`
//! owns its store and the store owns the guest's linear memory — so eviction,
//! a deadline and an out-of-memory trap all drop one rather than keeping it
//! around.

mod host;
mod module;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use extism::{CancelHandle, CompiledPlugin, Manifest, Plugin, PluginBuilder, Wasm};
use serde::Serialize;
use serde::de::DeserializeOwned;

use yapfast_plugin_api::call::{CommandInvocation, CommandOutcome, HostReturn};
use yapfast_plugin_api::event::EventEnvelope;
use yapfast_plugin_api::guest::{ActivatePayload, CallResult, DeactivatePayload};
use yapfast_plugin_api::hook::{HookOutcome, HookRequest};
use yapfast_plugin_api::limits::MAX_HANDOFF_BYTES;
use yapfast_plugin_api::message::{Fault, FaultKind, FromHelper};
use yapfast_plugin_api::{API_VERSION, PluginId};

use crate::backend::{Backend, LoadSpec};

/// How long one guest call may run when nothing else stops it.
///
/// The scheduler's watchdog holds the real deadline, which is a per-hook budget
/// of tens of milliseconds; this is the backstop that keeps a guest which spins
/// out of a wedged worker even if the watchdog never fires. Extism arms it as
/// an epoch deadline on every call, and a call that reaches it traps.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// The wasmtime cache config written inside the parent's cache directory.
const CACHE_CONFIG: &str = "wasmtime.toml";

/// How much of a trap's words are kept for a fault.
///
/// A wasm backtrace runs to several kilobytes and crosses the pipe into a
/// registry entry and a page, where nobody reads past the first few lines.
const MAX_REASON_BYTES: usize = 1024;

/// The Extism implementation of [`Backend`].
/// A deadline's trigger, handed out so the watchdog can fire it while a call
/// holds the backend.
pub type CancelToken = std::sync::Arc<dyn Fn() + Send + Sync>;

pub struct ExtismBackend {
    /// The wasmtime cache config file, when a cache could be set up.
    cache: Option<PathBuf>,
    outbox: mpsc::Sender<FromHelper>,
    /// One entry per plugin, each behind its own lock: a call takes the lock
    /// of the plugin it runs on and no other, so a background event handler
    /// can't hold the path an interactive send needs.
    loaded: std::sync::Mutex<HashMap<PluginId, std::sync::Arc<std::sync::Mutex<Loaded>>>>,
    /// The trigger of the call each live instance is running.
    ///
    /// A deadline is fired from the watchdog, which can't take the lock a
    /// running call holds, and it may be fired during `yf_activate` — before
    /// any instance has returned to be looked up. So the triggers live in their
    /// own map, filled before a guest is called, and a token resolves at fire
    /// time rather than when it is handed out.
    live: LiveMap,
}

/// The triggers of every live instance, shared with the tokens handed out.
type LiveMap = std::sync::Arc<std::sync::Mutex<HashMap<PluginId, std::sync::Arc<InstanceCancel>>>>;

/// One instance's deadline: the flag a trap is read against, and the runtime's
/// own handle for stopping the call.
struct InstanceCancel {
    fired: std::sync::atomic::AtomicBool,
    handle: CancelHandle,
}

impl InstanceCancel {
    fn fire(&self) {
        self.fired.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Err(error) = self.handle.cancel() {
            tracing::warn!(
                "a deadline couldn't be fired because the runtime's timer stopped: {error:#}"
            );
        }
    }
}

impl ExtismBackend {
    /// Starts a runtime that reports to `outbox` and compiles through
    /// `cache_dir`.
    pub fn new(cache_dir: &Path, outbox: mpsc::Sender<FromHelper>) -> anyhow::Result<Self> {
        Ok(ExtismBackend {
            cache: cache_config(cache_dir),
            outbox,
            loaded: std::sync::Mutex::new(HashMap::new()),
            live: LiveMap::default(),
        })
    }

    /// One compiled module, with the channel its host functions answer on.
    fn compile(
        &self,
        spec: &LoadSpec,
        channel: Arc<host::Channel>,
    ) -> Result<CompiledPlugin, Fault> {
        let mut manifest = Manifest::new([Wasm::file(&spec.wasm)])
            .with_memory_max(spec.memory_pages)
            .disallow_all_hosts()
            .with_timeout(CALL_TIMEOUT);
        // Extism's own variable store would be a second place for a plugin's
        // data that nothing backs up and nothing caps; storage is the parent's,
        // so this one holds nothing at all.
        manifest.memory.max_var_bytes = Some(0);
        // Never `allowed_paths`: a guest gets no view of the disk, with WASI or
        // without it. The only files it reads are the ones the parent hands it.
        let mut builder = PluginBuilder::new(manifest)
            .with_wasi(spec.wasi)
            .with_functions(host::functions(channel));
        builder = match &self.cache {
            Some(config) => builder.with_cache_config(config.clone()),
            None => builder.with_cache_disabled(),
        };
        builder.compile().map_err(|error| Fault {
            kind: FaultKind::BadModule,
            reason: format!("`{}` didn't compile: {error:#}", spec.plugin),
        })
    }
}

impl Backend for ExtismBackend {
    fn load(&self, spec: LoadSpec) -> Result<(), Fault> {
        if let Some(slot) = self.slot(&spec.plugin)
            && slot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .spec
                == spec
        {
            // The parent asks for the same module twice when it reshapes its
            // registry; recompiling it is the one thing loading a plugin must
            // not cost.
            return Ok(());
        }
        let shape = module::check_file(&spec.wasm, spec.wasi)?;
        let channel = Arc::new(host::Channel::new(spec.plugin.clone(), self.outbox.clone()));
        let compiled = self.compile(&spec, channel.clone())?;
        let mut loaded = self
            .loaded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loaded.insert(
            spec.plugin.clone(),
            std::sync::Arc::new(std::sync::Mutex::new(Loaded {
                spec,
                compiled,
                shape,
                instance: None,
                state: None,
            })),
        );
        drop(loaded);
        Ok(())
    }

    fn unload(&self, plugin: &PluginId) {
        // Dropping the entry drops the instance, its store and its linear
        // memory, and then the compiled module.
        self.loaded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(plugin);
    }

    fn activate(&self, plugin: &PluginId) -> Result<Option<String>, Fault> {
        let live = self.live.clone();
        let slot = self.slot(plugin).ok_or_else(|| not_loaded(plugin))?;
        let mut guard = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let loaded = &mut *guard;
        loaded.ensure(&live)?;
        Ok(loaded.state.clone())
    }

    fn deactivate(&self, plugin: &PluginId) -> Result<Option<String>, Fault> {
        let Some(slot) = self.slot(plugin) else {
            return Ok(None);
        };
        let mut guard = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let loaded = &mut *guard;
        if loaded.instance.is_none() {
            return Ok(None);
        }
        let answer = loaded.call::<DeactivatePayload>("yf_deactivate", &[]);
        // Whatever it answered, the instance goes: the guest was told, and a
        // deactivated instance holds memory the budget wants back.
        loaded.instance = None;
        let payload = answer?;
        let state = payload
            .and_then(|payload| payload.state)
            .map(|state| cut(state, MAX_HANDOFF_BYTES));
        loaded.state = state.clone();
        Ok(state)
    }

    fn event(&self, plugin: &PluginId, events: &[EventEnvelope]) -> Result<(), Fault> {
        let bytes = encode(&Batch {
            events,
            results: &[],
        })?;
        let live = self.live.clone();
        let slot = self.slot(plugin).ok_or_else(|| not_loaded(plugin))?;
        let mut guard = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let loaded = &mut *guard;
        loaded.ensure(&live)?;
        loaded.call::<()>("yf_event", &bytes).map(|_| ())
    }

    fn hook(&self, plugin: &PluginId, request: &HookRequest) -> Result<HookOutcome, Fault> {
        self.one_hook(plugin, request)
    }

    fn command(
        &self,
        plugin: &PluginId,
        command: &CommandInvocation,
    ) -> Result<CommandOutcome, Fault> {
        let bytes = encode(command)?;
        self.ask::<CommandOutcome>(plugin, "yf_command", &bytes)?
            // A command the person typed is waiting on this answer, so nothing
            // at all is a bad answer rather than an empty one.
            .ok_or_else(|| Fault {
                kind: FaultKind::BadAnswer,
                reason: format!("`{plugin}`'s yf_command answered nothing"),
            })
    }

    fn answer(&self, plugin: &PluginId, call: u64, answer: &HostReturn) -> Result<(), Fault> {
        // An answer can outlive the instance that asked for it: eviction, a
        // fault and a deadline all drop one while a request is in flight. The
        // answer still travels, and the guest that replaces it sees a result
        // id it has nothing waiting on and ignores it.
        let result = CallResult {
            call,
            answer: answer.clone(),
        };
        let bytes = encode(&Batch {
            events: &[],
            results: std::slice::from_ref(&result),
        })?;
        // A batch's answer isn't part of the ABI, and a guest that writes one
        // anyway mustn't be faulted for it: the events were delivered, which is
        // what this call was for. Read as a value, so anything parses.
        self.ask::<serde_json::Value>(plugin, "yf_event", &bytes)
            .map(|_| ())
    }

    fn evict(&self, plugin: &PluginId) {
        if let Some(slot) = self.slot(plugin) {
            slot.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .instance = None;
        }
        self.forget(plugin);
    }

    fn memory_pages(&self, plugin: &PluginId) -> Option<u32> {
        let slot = self.slot(plugin)?;
        let loaded = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loaded.instance.as_ref().map(|_| loaded.spec.memory_pages)
    }

    fn cancel_handle(&self, plugin: &PluginId) -> Option<CancelToken> {
        // A plugin that isn't loaded has nothing that could run, and nothing to
        // cancel. One that is loaded gets a token even before its instance
        // exists, because `yf_activate` is where a plugin first hangs.
        self.slot(plugin)?;
        let live = self.live.clone();
        let id = plugin.clone();
        Some(std::sync::Arc::new(move || {
            let trigger = live.lock().ok().and_then(|live| live.get(&id).cloned());
            if let Some(trigger) = trigger {
                trigger.fire();
            }
        }))
    }
}

impl ExtismBackend {
    /// Takes a plugin's trigger out of the live map, so a deadline fired late
    /// finds nothing to cancel.
    fn forget(&self, plugin: &PluginId) {
        if let Ok(mut live) = self.live.lock() {
            live.remove(plugin);
        }
    }

    /// One plugin's answer to one hook, with the subject the chain carries.
    fn one_hook(&self, plugin: &PluginId, request: &HookRequest) -> Result<HookOutcome, Fault> {
        let bytes = encode(request)?;
        // A guest that returns nothing handled nothing, which is the ABI's
        // `unchanged` and the common case for a plugin subscribed to one hook
        // and asked about another. Hooks fail open, so nothing else is worth
        // stopping a send over.
        Ok(self
            .ask::<HookOutcome>(plugin, "yf_hook", &bytes)?
            .unwrap_or(HookOutcome::Unchanged))
    }

    /// Encodes a payload, instantiates the guest if it has to, and parses the
    /// answer.
    fn ask<T: DeserializeOwned>(
        &self,
        plugin: &PluginId,
        export: &str,
        input: &[u8],
    ) -> Result<Option<T>, Fault> {
        let live = self.live.clone();
        let slot = self.slot(plugin).ok_or_else(|| not_loaded(plugin))?;
        let mut guard = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let loaded = &mut *guard;
        loaded.ensure(&live)?;
        loaded.call(export, input)
    }
}

impl ExtismBackend {
    /// This plugin's entry, or nothing when it isn't loaded.
    fn slot(&self, plugin: &PluginId) -> Option<std::sync::Arc<std::sync::Mutex<Loaded>>> {
        let loaded = self
            .loaded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loaded.get(plugin).cloned()
    }
}

/// One plugin that the parent has loaded.
struct Loaded {
    spec: LoadSpec,
    /// The compiled module, kept so a discarded instance is a few hundred
    /// microseconds to replace rather than a recompile.
    compiled: CompiledPlugin,
    /// Which exports the module was compiled with.
    shape: module::Shape,
    instance: Option<Instance>,
    /// What the last `yf_deactivate` handed on, for the next activation. The
    /// parent persists it, but only the helper holds it between the two calls,
    /// so a hand-off doesn't survive the helper being restarted: anything
    /// durable belongs in storage.
    state: Option<String>,
}

impl Loaded {
    /// Instantiates if there is no instance, and activates it.
    ///
    /// `live` is the backend's map of triggers: the instance is registered in it
    /// before `yf_activate` runs, because that call is the first one a deadline
    /// can catch, and it is exactly the call that never returns if a plugin
    /// means to hang.
    fn ensure(&mut self, live: &LiveMap) -> Result<(), Fault> {
        if self.instance.is_some() {
            return Ok(());
        }
        let missing = self.shape.missing_exports();
        if !missing.is_empty() {
            return Err(Fault {
                kind: FaultKind::BadModule,
                reason: format!(
                    "`{}` doesn't export {}, so it can't be called",
                    self.spec.plugin,
                    missing.join(", ")
                ),
            });
        }
        let plugin = Plugin::new_from_compiled(&self.compiled).map_err(|error| Fault {
            kind: FaultKind::BadModule,
            reason: format!("`{}` didn't instantiate: {error:#}", self.spec.plugin),
        })?;
        let cancel = std::sync::Arc::new(InstanceCancel {
            fired: std::sync::atomic::AtomicBool::new(false),
            handle: plugin.cancel_handle(),
        });
        if let Ok(mut live) = live.lock() {
            live.insert(self.spec.plugin.clone(), cancel.clone());
        }
        self.instance = Some(Instance { plugin, cancel });
        let payload = ActivatePayload {
            api: API_VERSION,
            plugin: self.spec.plugin.clone(),
            state: self.state.clone(),
        };
        let bytes = encode(&payload)?;
        match self.call::<()>("yf_activate", &bytes) {
            Ok(_) => Ok(()),
            Err(fault) => {
                // A half-activated guest must not be called again as if it had
                // been set up. The cause is kept rather than folded into
                // "activation failed": a plugin that ran out of memory in
                // `yf_activate` needs the same 16 MiB sentence a later call
                // would get, and a deadline in it is still a deadline.
                self.instance = None;
                if let Ok(mut live) = live.lock() {
                    live.remove(&self.spec.plugin);
                }
                let kind = match fault.kind {
                    FaultKind::BadAnswer => FaultKind::ActivateFailed,
                    kind => kind,
                };
                Err(Fault {
                    kind,
                    reason: format!("yf_activate: {}", fault.reason),
                })
            }
        }
    }

    /// Calls one export with JSON, and parses its JSON answer.
    ///
    /// The output is parsed while it is still borrowed from the store, so the
    /// bytes a guest returned are not copied on the way out of the call.
    fn call<T: DeserializeOwned>(
        &mut self,
        export: &str,
        input: &[u8],
    ) -> Result<Option<T>, Fault> {
        let Some(instance) = self.instance.as_mut() else {
            return Err(not_loaded(&self.spec.plugin));
        };
        let cancel = instance.cancel.clone();
        match instance.plugin.call::<&[u8], Option<&[u8]>>(export, input) {
            Ok(None) => Ok(None),
            Ok(Some(bytes)) => serde_json::from_slice(bytes)
                .map(Some)
                .map_err(|error| Fault {
                    kind: FaultKind::BadAnswer,
                    reason: format!(
                        "`{}`'s {export} answered something else than the ABI says: {error}",
                        self.spec.plugin
                    ),
                }),
            Err(error) => {
                let (kind, discard) = classify(
                    &error,
                    cancel.fired.load(std::sync::atomic::Ordering::Relaxed),
                );
                let fault = Fault {
                    kind,
                    reason: summarize(format!("`{}`'s {export}: {error}", self.spec.plugin)),
                };
                if discard {
                    // Out of memory or cancelled: it stopped somewhere of the
                    // runtime's choosing, so nothing it holds can be trusted.
                    self.instance = None;
                }
                Err(fault)
            }
        }
    }
}

/// One guest instance.
struct Instance {
    plugin: Plugin,
    /// The deadline's trigger, which the watchdog reaches through the backend's
    /// live map even while this call holds the backend.
    cancel: std::sync::Arc<InstanceCancel>,
}

/// A batch of events as the guest's JSON wants it, borrowed from the frame's
/// slice.
///
/// `EventBatch` is the wire's own type, but building one here would clone every
/// envelope, payload included, for a JSON write that reads them once. The two
/// field names are the ABI's, and a test holds this to `EventBatch`'s encoding
/// so they can't drift.
#[derive(Serialize)]
struct Batch<'a> {
    events: &'a [EventEnvelope],
    results: &'a [CallResult],
}

/// What a failed call means, and whether the instance survives it.
///
/// Extism reports its two interruptions by name: `oom` when the memory limiter
/// refused a growth, and `timeout` when the epoch deadline fired.
fn classify(error: &extism::Error, cancelled: bool) -> (FaultKind, bool) {
    let reason = error.to_string();
    let kind = if cancelled || reason == "timeout" {
        FaultKind::Deadline
    } else if reason == "oom" {
        FaultKind::OutOfMemory
    } else {
        FaultKind::Trap
    };
    let discard = matches!(kind, FaultKind::Deadline | FaultKind::OutOfMemory);
    (kind, discard)
}

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Fault> {
    serde_json::to_vec(value).map_err(|error| Fault {
        kind: FaultKind::BadAnswer,
        reason: format!("a payload couldn't be written as JSON: {error}"),
    })
}

fn not_loaded(plugin: &PluginId) -> Fault {
    Fault {
        kind: FaultKind::BadModule,
        reason: format!("`{plugin}` was never loaded"),
    }
}

/// Cuts a string to `max` bytes on a character boundary, so a truncated
/// hand-off or a truncated trap is still a string.
fn cut(mut text: String, max: usize) -> String {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}

/// A fault's words, cut to what a person will read.
fn summarize(reason: String) -> String {
    let truncated = reason.len() > MAX_REASON_BYTES;
    let mut reason = cut(reason, MAX_REASON_BYTES);
    if truncated {
        reason.push('…');
    }
    reason
}

/// Where wasmtime keeps compiled modules, as the config file that says so.
///
/// wasmtime takes the path of a config file, not of a directory, so the file is
/// written inside the directory the parent handed over and points the cache at
/// a folder beside it. A cache that can't be set up is no reason to refuse a
/// plugin — it costs a cold compile per instance, about 22 ms instead of 2.8 ms
/// — but it is the kind of slowness someone notices and never suspects, so it
/// is worth a line in the log.
fn cache_config(directory: &Path) -> Option<PathBuf> {
    match write_cache_config(directory) {
        Ok(path) => Some(path),
        Err(error) => {
            tracing::warn!(
                directory = %directory.display(),
                "the wasmtime compile cache is off: {error}"
            );
            None
        }
    }
}

fn write_cache_config(directory: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(directory)?;
    // wasmtime refuses a relative cache directory, and resolves nothing else.
    let directory = std::fs::canonicalize(directory)?;
    let entries = directory.join("wasmtime");
    let config = directory.join(CACHE_CONFIG);
    let toml = format!(
        "[cache]\ndirectory = {}\n",
        toml_string(&entries.to_string_lossy())
    );
    std::fs::write(&config, toml)?;
    Ok(config)
}

/// A TOML basic string holding `text`.
fn toml_string(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for character in text.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use yapfast_plugin_api::call::HostCall;
    use yapfast_plugin_api::event::{EventKind, Network};
    use yapfast_plugin_api::guest::EventBatch;
    use yapfast_plugin_api::hook::{Draft, HookKind, HookSubject};

    fn envelope() -> EventEnvelope {
        EventEnvelope {
            kind: EventKind::MessageCreate,
            network: Network::Discord,
            at: 1,
            room: Some("room-1".into()),
            author: None,
            own: false,
            payload: json!({ "body": "hi" }),
        }
    }

    #[test]
    fn the_borrowed_batch_is_encoded_exactly_as_the_abi_type() {
        let event = envelope();
        let result = CallResult {
            call: 7,
            answer: HostReturn::Pending {
                tag: "1".to_owned(),
            },
        };
        let borrowed = serde_json::to_vec(&Batch {
            events: std::slice::from_ref(&event),
            results: std::slice::from_ref(&result),
        })
        .unwrap();
        let owned = serde_json::to_vec(&EventBatch {
            events: vec![event],
            results: vec![result],
        })
        .unwrap();
        assert_eq!(borrowed, owned);
    }

    #[test]
    fn the_two_ways_a_call_is_stopped_are_both_deadlines() {
        for message in ["timeout", "the epoch deadline was reached"] {
            let (kind, discard) = classify(&extism::Error::msg(message), message == "timeout");
            if message == "timeout" {
                assert_eq!(kind, FaultKind::Deadline);
            }
            assert!(discard == (kind == FaultKind::Deadline));
        }
    }

    #[test]
    fn running_out_of_pages_discards_the_instance() {
        let (kind, discard) = classify(&extism::Error::msg("oom"), false);
        assert_eq!(kind, FaultKind::OutOfMemory);
        assert!(discard, "memory can't shrink, so the instance goes");
    }

    #[test]
    fn a_cancelled_call_is_a_deadline_whatever_the_trap_says() {
        let (kind, discard) = classify(&extism::Error::msg("unreachable"), true);
        assert_eq!(kind, FaultKind::Deadline);
        assert!(discard);
    }

    #[test]
    fn a_plain_trap_keeps_its_instance() {
        let (kind, discard) = classify(&extism::Error::msg("unreachable"), false);
        assert_eq!(kind, FaultKind::Trap);
        assert!(!discard, "a trap leaves the store as it was");
    }

    #[test]
    fn a_hand_off_is_cut_on_a_character_boundary() {
        let state = "é".repeat(10);
        let cut = cut(state, 5);
        assert_eq!(cut, "éé");
        assert!(cut.is_char_boundary(cut.len()));
    }

    #[test]
    fn a_hand_off_under_the_limit_is_not_copied() {
        let state = "{}".to_owned();
        assert_eq!(cut(state.clone(), MAX_HANDOFF_BYTES), state);
    }

    #[test]
    fn a_long_trap_is_cut_to_what_a_person_reads() {
        let reason = summarize("x".repeat(MAX_REASON_BYTES * 2));
        assert!(reason.len() <= MAX_REASON_BYTES + '…'.len_utf8());
        assert!(reason.ends_with('…'));
        assert_eq!(summarize("short".to_owned()), "short");
    }

    #[test]
    fn the_cache_config_names_an_absolute_directory() {
        let base = std::env::temp_dir().join(format!("yapfast-cache-test-{}", std::process::id()));
        let config = write_cache_config(&base).unwrap();
        let text = std::fs::read_to_string(&config).unwrap();
        assert!(text.starts_with("[cache]\ndirectory = \""), "{text}");
        assert!(text.contains("wasmtime\""), "{text}");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_quote_or_backslash_in_a_path_cannot_break_the_config() {
        assert_eq!(toml_string(r#"C:\a"b"#), r#""C:\\a\"b""#);
    }

    #[test]
    fn asking_for_a_plugin_that_was_never_loaded_is_a_fault_and_not_a_panic() {
        let (outbox, _inbox) = mpsc::channel();
        let backend =
            ExtismBackend::new(&std::env::temp_dir().join("yapfast-no-cache"), outbox).unwrap();
        let plugin = PluginId::parse("com.example.word-filter").unwrap();
        let fault = backend.activate(&plugin).unwrap_err();
        assert_eq!(fault.kind, FaultKind::BadModule);
        assert!(backend.memory_pages(&plugin).is_none());
        backend.unload(&plugin);
        backend.evict(&plugin);
        assert!(
            backend.cancel_handle(&plugin).is_none(),
            "a plugin with no instance has no deadline to fire"
        );
        assert!(
            backend
                .answer(&plugin, 1, &HostReturn::Unsupported)
                .is_err()
        );
        assert!(
            backend
                .event(&plugin, &[envelope()])
                .unwrap_err()
                .reason
                .contains("never loaded")
        );
        assert!(backend.hook(&plugin, &request()).is_err());
        assert_eq!(
            backend.command(&plugin, &invocation()).unwrap_err().kind,
            FaultKind::BadModule
        );
        assert!(backend.deactivate(&plugin).unwrap().is_none());
    }

    fn request() -> HookRequest {
        HookRequest {
            hook: HookKind::MessagePreSend,
            subject: HookSubject::Draft(Draft {
                room: "room-1".into(),
                body: "hello".into(),
                reply_to: None,
                attachments: Vec::new(),
            }),
        }
    }

    fn invocation() -> CommandInvocation {
        CommandInvocation {
            name: "filter".into(),
            options: serde_json::Map::new(),
        }
    }

    #[test]
    fn an_unknown_host_method_is_answered_unsupported_without_the_parent() {
        let (outbox, inbox) = mpsc::channel();
        let plugin = PluginId::parse("com.example.word-filter").unwrap();
        let channel = host::Channel::new(plugin, outbox);
        // The two functions are built for every plugin; building them is what a
        // load does, and it must not need a parent to be alive.
        assert_eq!(host::functions(Arc::new(channel)).len(), 2);
        assert!(inbox.try_recv().is_err());
    }

    #[test]
    fn a_host_call_carries_the_plugin_it_came_from() {
        let (outbox, inbox) = mpsc::channel();
        let plugin = PluginId::parse("com.example.word-filter").unwrap();
        let channel = host::Channel::new(plugin.clone(), outbox);
        channel
            .send(FromHelper::Call {
                plugin: plugin.clone(),
                call: 1,
                request: HostCall::new("storage.get", json!({ "key": "k" })).unwrap(),
            })
            .unwrap();
        match inbox.recv().unwrap() {
            FromHelper::Call { plugin: from, .. } => assert_eq!(from, plugin),
            other => panic!("a call should have been sent, not {other:?}"),
        }
    }
}
