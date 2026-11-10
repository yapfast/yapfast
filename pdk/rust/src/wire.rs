//! The JSON that crosses the guest boundary, as the shapes it has in api 1.
//!
//! Every type here is a copy of one Yapfast and its plugin helper agree on, so
//! a plugin built against this crate never pulls in Yapfast's own crates: the
//! PDK is published and the app is not. The copies are kept byte-compatible
//! with the app's: the same field names, the same `snake_case` tags, and the
//! same optional fields left out when they are `None`.
//!
//! Reading this module is how to find out what a plugin is handed. Writing to
//! it is not: nothing here is a stabilised API of its own, and a shape that
//! changes in a breaking way changes `api` in the plugin's manifest.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The guest ABI version a plugin is built for, which its manifest repeats.
pub const API_VERSION: u32 = 1;

/// A plugin's identifier: reverse-DNS, lowercase, stable forever.
///
/// The helper validates ids before a plugin runs, so this copy only carries
/// one: it is here because activation is handed its own id.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PluginId(Box<str>);

impl PluginId {
    /// The id as written in a manifest.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which network an event came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Network {
    Discord,
    Matrix,
}

/// The kinds of event a plugin may be told about.
///
/// A kind this copy doesn't know is kept rather than refused: the wire names
/// kinds as strings, and a plugin built for api 1 should see a new kind as
/// [`EventKind::Other`] instead of trapping because it can't decode its batch.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventKind {
    MessageCreate,
    MessageUpdate,
    MessageDelete,
    ReactionAdd,
    ReactionRemove,
    RoomSelected,
    VoiceState,
    AppFocus,
    AccountReady,
    /// Opt-in and coalesced: a burst arrives as one call.
    Typing,
    /// Opt-in and coalesced.
    Presence,
    /// Opt-in and coalesced.
    Speaking,
    /// A kind this version of the PDK doesn't know by name.
    Other(String),
}

impl EventKind {
    /// The kinds this copy has a name for, in the manifest's order.
    pub const NAMED: [EventKind; 12] = [
        EventKind::MessageCreate,
        EventKind::MessageUpdate,
        EventKind::MessageDelete,
        EventKind::ReactionAdd,
        EventKind::ReactionRemove,
        EventKind::RoomSelected,
        EventKind::VoiceState,
        EventKind::AppFocus,
        EventKind::AccountReady,
        EventKind::Typing,
        EventKind::Presence,
        EventKind::Speaking,
    ];

    /// The kind with this wire name, as `Other` when it isn't one we know.
    pub fn from_name(name: &str) -> Self {
        match name {
            "message_create" => EventKind::MessageCreate,
            "message_update" => EventKind::MessageUpdate,
            "message_delete" => EventKind::MessageDelete,
            "reaction_add" => EventKind::ReactionAdd,
            "reaction_remove" => EventKind::ReactionRemove,
            "room_selected" => EventKind::RoomSelected,
            "voice_state" => EventKind::VoiceState,
            "app_focus" => EventKind::AppFocus,
            "account_ready" => EventKind::AccountReady,
            "typing" => EventKind::Typing,
            "presence" => EventKind::Presence,
            "speaking" => EventKind::Speaking,
            other => EventKind::Other(other.to_owned()),
        }
    }

    /// The name this kind has on the wire.
    pub fn as_str(&self) -> &str {
        match self {
            EventKind::MessageCreate => "message_create",
            EventKind::MessageUpdate => "message_update",
            EventKind::MessageDelete => "message_delete",
            EventKind::ReactionAdd => "reaction_add",
            EventKind::ReactionRemove => "reaction_remove",
            EventKind::RoomSelected => "room_selected",
            EventKind::VoiceState => "voice_state",
            EventKind::AppFocus => "app_focus",
            EventKind::AccountReady => "account_ready",
            EventKind::Typing => "typing",
            EventKind::Presence => "presence",
            EventKind::Speaking => "speaking",
            EventKind::Other(name) => name,
        }
    }

    /// Whether a burst of these is batched rather than sent one by one.
    pub fn is_coalesced(&self) -> bool {
        matches!(
            self,
            EventKind::Typing | EventKind::Presence | EventKind::Speaking
        )
    }
}

impl std::fmt::Display for EventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for EventKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EventKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(EventKind::from_name(&String::deserialize(deserializer)?))
    }
}

/// One event, with everything a filter reads kept at the top.
///
/// The kind's own fields are in `payload`, as that kind's JSON: the same bytes
/// go to every plugin subscribed, so nothing is reshaped per plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub kind: EventKind,
    pub network: Network,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    /// The room it happened in, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<String>,
    /// The person it is about, when it is about someone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Whether the account did it itself.
    #[serde(default)]
    pub own: bool,
    /// The kind's own fields, or `{"truncated": true, ...}` when they were too
    /// large to send whole.
    pub payload: Value,
}

/// What `yf_activate` is given.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivatePayload {
    /// The API version the helper speaks.
    pub api: u32,
    pub plugin: PluginId,
    /// What the last deactivation handed on, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// What `yf_deactivate` returns: its hand-off to the next activation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeactivatePayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// What `yf_event` is given, in one call.
///
/// Events are batched, so a burst of typing is one call. Answers to host calls
/// that were left pending travel in the same call, keyed by their call id.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EventBatch {
    #[serde(default)]
    pub events: Vec<EventEnvelope>,
    #[serde(default)]
    pub results: Vec<CallResult>,
}

/// The answer to one host call that was left pending.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CallResult {
    /// The id the host gave the call when it answered `pending`.
    pub call: u64,
    pub answer: HostReturn,
}

/// One host call: a method name and that method's payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostCall {
    pub method: String,
    #[serde(default)]
    pub payload: Value,
}

/// What a host call is answered with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostReturn {
    /// The method ran; the payload is that method's own answer.
    Ok(Value),
    /// This build doesn't have this method.
    Unsupported,
    /// Yapfast refused it: a missing capability, a host not granted, a cap
    /// reached. `reason` is written for a person to read.
    Denied { reason: String },
    /// The answer is coming in a later call, tagged with this value.
    Pending { tag: String },
    /// The call was understood and failed.
    Error { message: String },
}

/// The method names a plugin may call.
///
/// The first group has a typed wrapper in this crate. The rest are named by the
/// ABI so a plugin can feature-detect them: a build that lacks one answers
/// `unsupported`, which arrives as an error naming the method.
pub mod method {
    pub const APP_VERSION: &str = "app.version";
    pub const APP_LOCALE: &str = "app.locale";
    pub const APP_THEME_MODE: &str = "app.theme_mode";
    pub const ACCOUNT_ME: &str = "account.me";
    pub const ROOMS_SELECTED: &str = "rooms.selected";
    pub const ROOMS_GET: &str = "rooms.get";
    pub const ROOMS_LIST: &str = "rooms.list";
    pub const MESSAGES_GET: &str = "messages.get";
    pub const MESSAGES_HISTORY: &str = "messages.history";
    pub const PEOPLE_GET: &str = "people.get";
    pub const STORAGE_GET: &str = "storage.get";
    pub const STORAGE_SET: &str = "storage.set";
    pub const STORAGE_DELETE: &str = "storage.delete";
    pub const STORAGE_LIST: &str = "storage.list";
    pub const SETTINGS_GET: &str = "settings.get";
    pub const HTTP_FETCH: &str = "http.fetch";
    pub const TIMERS_AFTER: &str = "timers.after";
    pub const TIMERS_EVERY: &str = "timers.every";
    pub const TIMERS_CANCEL: &str = "timers.cancel";

    // Implemented with their capability, and named here so a plugin can call
    // them: `messages.send` needs `messages.send`, `open.url` needs
    // `open.app`, and so on.
    pub const MESSAGES_SEND: &str = "messages.send";
    pub const MESSAGES_EDIT: &str = "messages.edit";
    pub const MESSAGES_DELETE: &str = "messages.delete";
    pub const MESSAGES_REACT: &str = "messages.react";
    pub const MESSAGES_JUMP_TO: &str = "messages.jump_to";
    pub const COMPOSER_GET_DRAFT: &str = "composer.get_draft";
    pub const COMPOSER_SET_DRAFT: &str = "composer.set_draft";
    pub const COMPOSER_INSERT: &str = "composer.insert";
    pub const PLUGIN_CALL: &str = "plugin.call";
    pub const PLUGIN_PUBLISH: &str = "plugin.publish";
    pub const PLUGIN_SUBSCRIBE: &str = "plugin.subscribe";
    pub const NOTIFY_SEND: &str = "notify.send";
    pub const CLIPBOARD_WRITE: &str = "clipboard.write";
    pub const OPEN_URL: &str = "open.url";
}

/// `storage.set`: the value is the plugin's own JSON.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StorageSet {
    pub key: String,
    pub value: Value,
}

/// `storage.get`: `null` in the answer when the key isn't there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageGet {
    pub key: String,
}

/// `storage.list`: every key the plugin holds under `prefix`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageList {
    #[serde(default)]
    pub prefix: String,
}

/// `settings.get`: one of the plugin's settings, by the key its manifest gave.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingGet {
    pub key: String,
}

/// `timers.after` and `timers.every`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerRequest {
    /// Milliseconds; a repeating timer is at least
    /// [`MIN_TIMER_INTERVAL_MS`].
    pub interval_ms: u64,
    /// The plugin's own name for this timer, echoed when it fires.
    pub tag: String,
}

/// `timers.cancel`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerCancel {
    pub tag: String,
}

/// The shortest interval a repeating timer may ask for.
pub const MIN_TIMER_INTERVAL_MS: u64 = 1_000;

/// `app.version`'s answer: which Yapfast is running this plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    pub version: String,
    pub locale: String,
    pub theme_mode: String,
}

/// `http.fetch`: one request through the network gate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpRequest {
    #[serde(default = "HttpRequest::default_method")]
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// The body, base64, because a body isn't always text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_base64: Option<String>,
    /// The plugin's own name for this request, echoed in its answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

impl HttpRequest {
    fn default_method() -> String {
        "GET".to_owned()
    }
}

/// `http.fetch`'s answer: the head as it arrived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// The final URL, after redirects.
    pub url: String,
    pub bytes: u64,
    /// The body, base64, when it fit in one answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_base64: Option<String>,
}

/// A command a person typed in the composer, with its options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandInvocation {
    pub name: String,
    #[serde(default)]
    pub options: serde_json::Map<String, Value>,
}

/// What a command answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    /// Nothing to show.
    None,
    /// A line for the person, in the room they typed the command in.
    Notice { text: String },
    /// The command's own JSON.
    Value(Value),
    /// The command failed; `message` is shown to the person.
    Failed { message: String },
}

/// A hook a plugin can intercept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookKind {
    /// Before an outgoing message is sent.
    MessagePreSend,
    /// Before an edit is sent.
    MessagePreEdit,
    /// The typing indicator, before it is sent.
    TypingPreSend,
    /// A file, before it is uploaded.
    AttachmentPreUpload,
    /// An activity, before it is published.
    ActivityPrePublish,
}

/// The thing a hook is about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookSubject {
    /// Text about to be sent.
    Draft(Draft),
    Attachment(AttachmentPending),
    /// An activity about to be published, as its JSON.
    Activity(Value),
}

/// An outgoing message, as a hook may change it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub room: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub attachments: Vec<AttachmentView>,
}

/// A file on its way up, as a hook may rename or strip it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentPending {
    pub room: String,
    pub name: String,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

/// A hook's question to the plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRequest {
    pub hook: HookKind,
    pub subject: HookSubject,
}

/// What a plugin answers a hook with.
///
/// A hook is on the path the person is waiting on, so it fails open: a plugin
/// that traps, answers late or answers nonsense leaves the message as it was.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookOutcome {
    /// It touched nothing, or doesn't handle this hook.
    Unchanged,
    /// The new subject; the next plugin in the chain sees this one.
    Changed(HookSubject),
    /// Stop the send. Only respected where the plugin may intercept.
    Block { reason: String },
}

/// A message as a plugin sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageView {
    pub id: String,
    pub room: String,
    pub author: PersonView,
    pub body: String,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    #[serde(default)]
    pub edited: bool,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub attachments: Vec<AttachmentView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    /// Whether this message mentions the account.
    #[serde(default)]
    pub mentions_me: bool,
}

/// A person, as events and calls name them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonView {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Whether this is the account's own.
    #[serde(default)]
    pub own: bool,
    #[serde(default)]
    pub bot: bool,
}

/// A file attached to a message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentView {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    /// A `discord://` or `matrix://` reference: a plugin never gets a URL that
    /// carries the account's token.
    pub media: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

/// A room a plugin can be told about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomView {
    pub id: String,
    pub name: String,
    pub network: Network,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default)]
    pub dm: bool,
}

/// Base64, standard alphabet with padding.
///
/// Only bodies need it: everything else on the wire is JSON, and a body isn't
/// always text.
pub mod base64 {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    /// Encodes bytes as base64, padded.
    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let packed = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            out.push(ALPHABET[(packed >> 18) as usize & 0x3f] as char);
            out.push(ALPHABET[(packed >> 12) as usize & 0x3f] as char);
            out.push(if chunk.len() > 1 {
                ALPHABET[(packed >> 6) as usize & 0x3f] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                ALPHABET[packed as usize & 0x3f] as char
            } else {
                '='
            });
        }
        out
    }

    /// Decodes padded base64; `None` if it isn't.
    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let bytes = text.as_bytes();
        if bytes.len() % 4 != 0 {
            return None;
        }
        let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
        for chunk in bytes.chunks(4) {
            let mut packed = 0u32;
            let mut padding = 0;
            for (index, byte) in chunk.iter().enumerate() {
                let sextet = if *byte == b'=' {
                    if index < 2 {
                        return None;
                    }
                    padding += 1;
                    0
                } else {
                    if padding > 0 {
                        return None;
                    }
                    ALPHABET.iter().position(|a| a == byte)? as u32
                };
                packed = (packed << 6) | sextet;
            }
            out.push((packed >> 16) as u8);
            if padding < 2 {
                out.push((packed >> 8) as u8);
            }
            if padding < 1 {
                out.push(packed as u8);
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_this_copy_knows_round_trips_by_name() {
        for kind in EventKind::NAMED {
            let text = serde_json::to_string(&kind).unwrap();
            assert_eq!(text, format!("\"{}\"", kind.as_str()));
            assert_eq!(serde_json::from_str::<EventKind>(&text).unwrap(), kind);
        }
    }

    #[test]
    fn a_kind_from_a_later_api_decodes_instead_of_failing() {
        let kind: EventKind = serde_json::from_str("\"something_new\"").unwrap();
        assert_eq!(kind, EventKind::Other("something_new".into()));
        assert_eq!(kind.as_str(), "something_new");
    }

    #[test]
    fn a_payload_is_the_shape_the_app_writes() {
        let text = r#"{
            "kind": "message_create",
            "network": "discord",
            "at": 17,
            "room": "room-1",
            "own": false,
            "payload": {"body": "hi"}
        }"#;
        let envelope: EventEnvelope = serde_json::from_str(text).unwrap();
        assert_eq!(envelope.kind, EventKind::MessageCreate);
        assert_eq!(envelope.room.as_deref(), Some("room-1"));
        assert!(envelope.author.is_none());
    }

    #[test]
    fn a_pending_answer_is_the_round_trip_the_helper_writes() {
        let answer = HostReturn::Pending {
            tag: "12".to_owned(),
        };
        assert_eq!(
            serde_json::to_string(&answer).unwrap(),
            r#"{"pending":{"tag":"12"}}"#
        );
        let denied = HostReturn::Denied {
            reason: "no messages.send".to_owned(),
        };
        assert_eq!(
            serde_json::to_string(&denied).unwrap(),
            r#"{"denied":{"reason":"no messages.send"}}"#
        );
    }

    #[test]
    fn base64_round_trips() {
        for length in 0..=16 {
            let bytes: Vec<u8> = (0..length).map(|i| b"wasm"[i % 4]).collect();
            assert_eq!(
                base64::decode(&base64::encode(&bytes)).as_deref(),
                Some(&bytes[..])
            );
        }
    }
}
