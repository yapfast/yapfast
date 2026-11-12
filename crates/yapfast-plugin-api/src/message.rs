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
use crate::code::CodeBlockRequest;
use crate::event::{EventEnvelope, TopicEvent};
use crate::hook::{HookOutcome, HookRequest};
use crate::identity::{Identity, IdentityRequest};
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
    /// What the plugin's last `yf_deactivate` handed on, when it is being
    /// replaced rather than loaded for the first time: the state crosses the
    /// swap, so an update doesn't cost the plugin's memory of where it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
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
        /// What other plugins published to this one's topics, in the same call:
        /// a subscriber handles both in one `yf_event`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        topics: Vec<TopicEvent>,
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
    /// One export of one plugin, called on behalf of another: `plugin.call`.
    ///
    /// It is a request of its own rather than a guest's call answered by the
    /// parent, because running it is the helper's to do: the callee's instance
    /// lives here, and the answer has to be its export's.
    Invoke {
        plugin: PluginId,
        /// The export on the callee's module, which the helper calls with the
        /// bytes and reads the answer from.
        function: String,
        /// What it is given, base64. A call carries bytes, not JSON, so a
        /// plugin can pass whatever its own pair of plugins agreed on.
        args_base64: String,
    },
    /// Ask a plugin to draw under a message it is on screen for, with
    /// `yf_decorate`: the answer is a widget tree, or nothing.
    ///
    /// It is a request rather than a guest call because the parent doesn't ask
    /// the guest anything: the plugin's instance lives here, and the drawing is
    /// its export's answer.
    Decorate {
        plugin: PluginId,
        request: DecorateRequest,
    },
    /// Ask a plugin how one person should look, with `yf_identity`: the answer
    /// is one [`Identity`], or nothing.
    ///
    /// It is one request per (person, server) and not per message: the parent
    /// keeps the answer, and every surface that shows that person reads it from
    /// there.
    Identity {
        plugin: PluginId,
        request: IdentityRequest,
    },
    /// Ask a plugin to colour one code block on screen, with `yf_code_block`:
    /// the answer is spans that spell the block again.
    CodeBlock {
        plugin: PluginId,
        request: CodeBlockRequest,
    },
    DecorateMember {
        plugin: PluginId,
        request: DecorateMemberRequest,
    },
    /// Something the person did with what this plugin drew, for its
    /// `yf_input`: a menu row it added, a widget it drew in a slot, a notice of
    /// its that was closed.
    ///
    /// There is no answer. A plugin that wants the person to see something says
    /// it the way it always does, with `ui.notice` or by drawing again, and a
    /// click that answered into thin air would be an answer nobody reads. It is
    /// still a request and not a call because the instance is the helper's: the
    /// parent turns the person's click into this, and the helper runs the
    /// export.
    Input { plugin: PluginId, input: InputEvent },
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

/// What the person did with something a plugin drew, as `yf_input` is given it.
///
/// The names and values are the interface's own: `slot` is a slot's id, `id` is
/// a widget's, `target` is what a menu row was clicked on, and `subject` is what
/// a widget was drawn on. A plugin compares them with what it declared.
///
/// ```json
/// { "kind": "menu", "action": "wave", "target": "com.example.someone" }
/// { "kind": "widget", "slot": "composer", "id": "shout", "value": "", "subject": "1234" }
/// { "kind": "dismissed", "slot": "toast" }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputEvent {
    /// A menu row the plugin added was chosen.
    Menu {
        /// The row's `action`, as the manifest declares it.
        action: String,
        /// What the menu was about: a person's, conversation's or server's id.
        target: String,
    },
    /// A widget the plugin drew was clicked or changed.
    Widget {
        /// The slot it was drawn in.
        slot: String,
        /// The widget's own id.
        id: String,
        /// Its value after the change: a toggle's `true` or `false`, a select's
        /// option, an input's text, a slider's number, a link's own value, and
        /// nothing for a button.
        value: String,
        /// What it was drawn on: the message, or the open conversation.
        subject: String,
    },
    /// A notice or toast the plugin drew was closed, so it is leaving the
    /// screen.
    Dismissed {
        /// The slot it was in.
        slot: String,
    },
}

/// What `yf_decorate` may answer: what to draw under a message, and whether the
/// message or its media should be hidden from view.
///
/// A bare widget tree is still an answer, so a plugin that draws and nothing else
/// says what it always said. The object form is for a plugin that also filters:
///
/// ```json
/// { "hide_media": true }
/// { "hide": true, "tree": { "rows": [] } }
/// ```
///
/// Hiding is display only: nothing is deleted, nothing is sent, and turning the
/// plugin off brings the row back. Both flags need `messages.filter`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decoration {
    /// The widget tree to draw under the row, when the plugin drew one.
    pub tree: Option<serde_json::Value>,
    /// Hide the whole row from view.
    pub hide: bool,
    /// Hide the row's pictures and files.
    pub hide_media: bool,
}

impl Decoration {
    /// What a plugin's answer means, or `None` when it is not an answer at all.
    ///
    /// A bare object with `rows` is a tree, as it always was; `null`, or nothing,
    /// draws nothing and hides nothing.
    pub fn parse(answer: serde_json::Value) -> Option<Self> {
        if answer.is_null() {
            return None;
        }
        let object = answer.as_object()?;
        if object.contains_key("rows") {
            return Some(Decoration {
                tree: Some(answer),
                ..Decoration::default()
            });
        }
        let flag = |field: &str| object.get(field).and_then(serde_json::Value::as_bool);
        let decoration = Decoration {
            tree: object.get("tree").filter(|tree| !tree.is_null()).cloned(),
            hide: flag("hide").unwrap_or(false),
            hide_media: flag("hide_media").unwrap_or(false),
        };
        if decoration.tree.is_none() && !decoration.hide && !decoration.hide_media {
            return None;
        }
        Some(decoration)
    }
}

#[cfg(test)]
mod decoration_tests {
    use super::*;

    #[test]
    fn a_bare_tree_is_still_a_decoration() {
        let tree = serde_json::json!({ "rows": [{ "cells": [{ "kind": "text", "text": "hi" }] }] });
        let parsed = Decoration::parse(tree.clone()).unwrap();
        assert_eq!(parsed.tree, Some(tree));
        assert!(!parsed.hide && !parsed.hide_media);
    }

    #[test]
    fn hiding_media_or_a_row_is_an_answer_on_its_own() {
        let parsed = Decoration::parse(serde_json::json!({ "hide_media": true })).unwrap();
        assert!(parsed.hide_media);
        assert_eq!(parsed.tree, None);
        assert!(
            !Decoration::parse(serde_json::json!({ "hide": true }))
                .unwrap()
                .hide_media
        );
        let both =
            Decoration::parse(serde_json::json!({ "hide": true, "hide_media": false })).unwrap();
        assert!(both.hide && !both.hide_media);
    }

    #[test]
    fn nothing_and_nonsense_are_no_answer() {
        assert!(Decoration::parse(serde_json::Value::Null).is_none());
        assert!(Decoration::parse(serde_json::json!({})).is_none());
        assert!(Decoration::parse(serde_json::json!("hello")).is_none());
        assert!(Decoration::parse(serde_json::json!({ "hide": "yes" })).is_none());
    }
}

/// it needs to decide whether it does.
///
/// It is the row's own summary and no more: a plugin that wants the whole
/// message reads it with `messages.get`, which is what `messages.read` is for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecorateRequest {
    pub room: String,
    pub message: String,
    /// Who wrote it, by id.
    pub author: String,
    /// What it says, before any plugin changed the display.
    pub body: String,
    /// Whether this account wrote it.
    pub own: bool,
}

/// What `yf_member_decorate` is given: someone in the open room's member list,
/// with what decides whether a plugin badges them.
///
/// It is one person's place in the list and no more: a plugin that wants the
/// whole member reads them with `people.get`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecorateMemberRequest {
    /// The server the list is of; empty for a direct message.
    #[serde(default)]
    pub server: String,
    pub room: String,
    /// The person, by id.
    pub member: String,
    /// Their display name in this room.
    pub name: String,
    /// Whether it is this account.
    pub own: bool,
    /// A Discord application's account.
    pub bot: bool,
    /// The ids of the roles they have here; empty where unknown.
    #[serde(default)]
    pub roles: Vec<String>,
}

/// What a plugin badges a person in the member list with: a few characters and
/// a tone, not a tree. The parent caps and checks it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberBadge {
    pub text: String,
    /// `normal`, `muted`, `accent`, `success`, `warning` or `danger`, as a
    /// widget tree's cells name theirs.
    #[serde(default)]
    pub tone: String,
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
    /// A chain's final answer, the plugins that failed on the way, and the
    /// plugin that answered, when one changed or stopped the subject.
    HookDone {
        outcome: HookOutcome,
        failures: Vec<(PluginId, Fault)>,
        /// Which plugin produced `outcome`, for a parent that has to ask what
        /// its manifest allows. Absent for a chain nobody answered into.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<PluginId>,
    },
    /// A command's answer.
    CommandDone {
        plugin: PluginId,
        outcome: CommandOutcome,
    },
    /// An export's answer to [`ToHelper::Invoke`]: its bytes, base64, or the
    /// fault that stopped it, which the parent reports to the caller.
    Invoked {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answer_base64: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fault: Option<Fault>,
    },
    /// What a plugin drew under a message, as `yf_decorate` answered: a widget
    /// tree, or nothing when it drew none. The parent checks the tree before
    /// anything shows it.
    Decorated {
        plugin: PluginId,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tree: Option<serde_json::Value>,
        /// Hide the row, and hide its pictures, from view. Display only, and
        /// both need `messages.filter`; the parent drops them otherwise.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        hide: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        hide_media: bool,
    },
    /// How a plugin answered about one person, as `yf_identity` returned it, or
    /// nothing when it said nothing. The parent checks the answer, and keeps it
    /// under the pair it was asked about.
    Identity {
        plugin: PluginId,
        /// The person and the server the parent asked about, echoed back so an
        /// answer that arrives late still files where it belongs.
        person: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identity: Option<Identity>,
    },
    /// What a plugin coloured one code block, as `yf_code_block` answered: the
    /// spans, or nothing when it drew none. The parent checks that they spell
    /// the block before anything shows them.
    CodeBlock {
        plugin: PluginId,
        /// The block, by the content hash it was asked about with.
        block: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spans: Option<Vec<crate::code::Span>>,
    },
    /// What a plugin badges a person in the member list with, as
    /// `yf_member_decorate` answered: unchecked, so the parent checks it.
    MemberDecorated {
        plugin: PluginId,
        member: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        badge: Option<serde_json::Value>,
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
