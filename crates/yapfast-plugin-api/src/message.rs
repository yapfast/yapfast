//! What crosses the pipe between Yapfast and the plugin helper.
//!
//! These are the payloads of the [`crate::frame`] kinds: the parent asks the
//! helper to run something, the helper reports back, and a guest's host calls
//! travel the other way to be answered by the parent, which holds the token, the
//! network gate and the consent dialogs.
//!
//! A frame's `id` pairs a request with its answer for either direction, so the
//! enums here carry no ids of their own.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::call::{CommandInvocation, CommandOutcome, HostCall, HostReturn};
use crate::event::EventEnvelope;
use crate::hook::{HookOutcome, HookRequest};
use crate::ids::PluginId;

/// Where a plugin stands, as the page shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    /// Files on disk; nothing in memory.
    Installed,
    /// A registry entry in the parent; the helper needn't be running.
    Enabled,
    /// An instance in the helper.
    Active,
    /// It trapped, ran out of memory or missed a deadline, and will be
    /// re-instantiated on its next trigger.
    Faulted { reason: String, memory_pages: u32 },
    /// Three strikes in a minute: it stays off until the person enables it.
    Suspended { reason: String },
}

/// Why an instance was discarded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultKind {
    /// WebAssembly trapped.
    Trap,
    /// It went past its page limit, which traps as `oom`; the instance can't
    /// shrink, so it's discarded.
    OutOfMemory,
    /// It ran past its deadline and was cancelled.
    Deadline,
    /// The module didn't instantiate.
    BadModule,
    /// `yf_activate` failed or ran late.
    ActivateFailed,
    /// Its answers weren't the shape the ABI says.
    BadAnswer,
}

/// What went wrong, in words a person can read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fault {
    pub kind: FaultKind,
    pub reason: String,
}

/// One line of a plugin's log, ring-buffered in its registry entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRecord {
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
}

/// The levels a guest's log lines are kept at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// What to load before activating a plugin: the fields [`ToHelper::Load`]
/// carries, for the one request that both loads and activates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Activation {
    pub wasm: PathBuf,
    pub memory_pages: u32,
    pub wasi: bool,
}

/// The parent's request to the helper.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ToHelper {
    /// The first frame: which API this parent speaks.
    Hello { api: u32 },
    /// Load a module and its limits, without activating it.
    Load {
        plugin: PluginId,
        wasm: PathBuf,
        memory_pages: u32,
        wasi: bool,
    },
    /// Drop an instance, keeping nothing.
    Unload { plugin: PluginId },
    /// Load this plugin's module if the helper hasn't been given it, then call
    /// `yf_activate`: one request for the whole phase.
    ///
    /// It is one request and not a `Load` followed by an `Activate` because the
    /// two are one phase. Between them the helper holds a module with no
    /// instance, which looks exactly like an idle plugin, and on a busy machine
    /// it left in that gap — the activation that arrived a moment later found a
    /// closed pipe, and the parent waited out its deadline. `load` is `None` for
    /// a module the helper already has.
    Activate {
        plugin: PluginId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        load: Option<Activation>,
    },
    /// Call `yf_deactivate`, which may hand state to the next activation.
    Deactivate { plugin: PluginId },
    /// Run an instance again with nothing new: an eviction clock's trigger.
    Kick { plugin: PluginId },
    /// A batch of events, already filtered and serialized.
    Event {
        plugin: PluginId,
        events: Vec<EventEnvelope>,
    },
    /// One hook, for the whole chain at once: the helper runs it as a single
    /// exchange, in this order, rather than one round trip per plugin.
    Hook {
        chain: Vec<PluginId>,
        request: HookRequest,
    },
    /// A command the person typed, with its options.
    Command {
        plugin: PluginId,
        command: CommandInvocation,
    },
    /// The answer to a guest's host call.
    Return {
        plugin: PluginId,
        call: u64,
        answer: HostReturn,
    },
    /// Evict now: the idle clock ran out or the memory budget is full.
    Evict { plugin: PluginId },
    /// The parent is going away; exit.
    Shutdown,
}

/// The helper's report to the parent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum FromHelper {
    /// The answer to [`ToHelper::Hello`]: the helper's own version.
    Ready { api: u32, version: String },
    /// A guest wants something from the parent, which answers with
    /// [`ToHelper::Return`] for the same `call`. The id lives here rather than
    /// in the frame header, because the helper hands its outbox to the runtime
    /// and only the runtime knows which call it is answering.
    Call {
        plugin: PluginId,
        /// The guest's own number for this call, which it was told to expect
        /// the answer under when `yf_call` answered `pending`.
        call: u64,
        request: HostCall,
    },
    /// The instance is up; `state` is what the last `yf_deactivate` handed on.
    Activated {
        plugin: PluginId,
        state: Option<String>,
        memory_pages: u32,
    },
    /// The instance is gone; `state` is its hand-off for the next activation.
    Deactivated {
        plugin: PluginId,
        state: Option<String>,
    },
    /// A chain's final answer, and the plugins that failed on the way.
    HookDone {
        outcome: HookOutcome,
        failures: Vec<(PluginId, Fault)>,
    },
    /// A command's answer.
    CommandDone {
        plugin: PluginId,
        outcome: CommandOutcome,
    },
    /// An event batch finished; nothing to report.
    EventDone { plugin: PluginId },
    /// A plugin's state changed, for the page and the parent's registry.
    Status {
        plugin: PluginId,
        status: PluginStatus,
    },
    /// An instance was discarded, and why.
    Fault { plugin: PluginId, fault: Fault },
    /// A line of a plugin's log.
    Log { plugin: PluginId, record: LogRecord },
    /// The helper is exiting, and why.
    Bye { reason: String },
}

/// The guest exports the helper requires, in the order a PDK generates them.
///
/// A guest that doesn't export one gets [`FaultKind::BadModule`] at activation,
/// so a PDK that forgets `yf_event` fails early and clearly rather than on the
/// first message. `yf_decorate` is not here: drawing beside a row is the
/// interface half of the API, which doesn't exist yet.
pub const EXPORTS: [&str; 5] = [
    "yf_activate",
    "yf_deactivate",
    "yf_event",
    "yf_hook",
    "yf_command",
];

/// The host functions a guest imports.
///
/// `yf_call` is a request with an answer; `yf_post` is fire and forget, so a
/// plugin can log or publish without waiting.
pub const IMPORTS: [&str; 2] = ["yf_call", "yf_post"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook::HookKind;

    #[test]
    fn a_hello_round_trips() {
        let hello = ToHelper::Hello {
            api: crate::API_VERSION,
        };
        let text = serde_json::to_string(&hello).unwrap();
        assert_eq!(text, "{\"t\":\"hello\",\"api\":1}");
        assert_eq!(serde_json::from_str::<ToHelper>(&text).unwrap(), hello);
    }

    #[test]
    fn a_fault_carries_a_kind_and_a_sentence() {
        let fault = Fault {
            kind: FaultKind::OutOfMemory,
            reason: "ran out of its 16 MiB".into(),
        };
        let report = FromHelper::Fault {
            plugin: PluginId::parse("com.example.word-filter").unwrap(),
            fault,
        };
        let text = serde_json::to_string(&report).unwrap();
        assert!(text.contains("out_of_memory"));
        assert_eq!(serde_json::from_str::<FromHelper>(&text).unwrap(), report);
    }

    #[test]
    fn a_hook_request_names_its_hook_and_plugin() {
        let request = ToHelper::Hook {
            chain: vec![PluginId::parse("com.example.word-filter").unwrap()],
            request: HookRequest {
                hook: HookKind::MessagePreSend,
                subject: crate::hook::HookSubject::Draft(crate::hook::Draft {
                    room: "room-1".into(),
                    body: "hi".into(),
                    reply_to: None,
                    attachments: Vec::new(),
                }),
            },
        };
        let text = serde_json::to_string(&request).unwrap();
        assert!(text.contains("\"message_pre_send\""));
        assert_eq!(serde_json::from_str::<ToHelper>(&text).unwrap(), request);
    }
}
