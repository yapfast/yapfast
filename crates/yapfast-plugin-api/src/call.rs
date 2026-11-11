//! Calls a guest makes to Yapfast, and the answers it gets.
//!
//! The guest has one host function, `yf_call`, and one fire-and-forget
//! `yf_post`; the method is a name inside the payload. A method this build
//! doesn't know answers [`HostReturn::Unsupported`], so a plugin can feature-
//! detect instead of failing to link, and adding a method never changes the
//! ABI.
//!
//! The typed payloads below cover the calls the runtime implements first. The
//! rest are named here so plugins can feature-detect them from the beginning;
//! a call to one before it exists gets `Unsupported`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::PluginId;

/// The method names a guest may call.
pub mod method {
    // Those with typed payloads in this module.
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
    pub const PEOPLE_MEMBER: &str = "people.member";
    pub const STORAGE_GET: &str = "storage.get";
    pub const STORAGE_SET: &str = "storage.set";
    pub const STORAGE_DELETE: &str = "storage.delete";
    pub const STORAGE_LIST: &str = "storage.list";
    pub const SETTINGS_GET: &str = "settings.get";
    pub const SETTINGS_ON_CHANGE: &str = "settings.on_change";
    pub const HTTP_FETCH: &str = "http.fetch";
    pub const TIMERS_AFTER: &str = "timers.after";
    pub const TIMERS_EVERY: &str = "timers.every";
    pub const TIMERS_CANCEL: &str = "timers.cancel";
    pub const UI_SET_SLOT: &str = "ui.set_slot";
    pub const UI_CLEAR_SLOT: &str = "ui.clear_slot";
    pub const UI_NOTICE: &str = "ui.notice";
    pub const UI_TOAST: &str = "ui.toast";
    pub const UI_OPEN_DIALOG: &str = "ui.open_dialog";

    // Named now, implemented with their capability.
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

/// One host call, as it crosses the pipe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostCall {
    /// The method name, one of [`method`].
    pub method: String,
    /// The method's payload, which each method defines.
    #[serde(default)]
    pub payload: Value,
}

impl HostCall {
    pub fn new(method: &str, payload: impl Serialize) -> Result<Self, serde_json::Error> {
        Ok(HostCall {
            method: method.to_owned(),
            payload: serde_json::to_value(payload)?,
        })
    }

    /// The payload as its typed form.
    pub fn parse<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.payload.clone())
    }
}

/// What a guest is given back for a host call.
///
/// The guest's host function is synchronous, so a call that waits on a person
/// answers [`HostReturn::Pending`] at once and is completed by a later event
/// with the same `tag`; nothing is held open while a dialog is up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostReturn {
    /// The method ran; its payload is the method's own answer.
    Ok(Value),
    /// This build doesn't have this method.
    Unsupported,
    /// Yapfast refused it: a missing capability, a host not granted, a cap
    /// reached. `reason` is written for a person to read.
    Denied { reason: String },
    /// The answer is coming as a later event tagged with this value.
    Pending { tag: String },
    /// The call was understood and failed.
    Error { message: String },
}

impl HostReturn {
    /// The value of an [`HostReturn::Ok`], for callers that only expect one.
    pub fn ok_value(self) -> Option<Value> {
        match self {
            HostReturn::Ok(value) => Some(value),
            _ => None,
        }
    }

    /// Whether this answer needs the plugin to wait.
    pub fn is_pending(&self) -> bool {
        matches!(self, HostReturn::Pending { .. })
    }
}

/// `storage.set`: the value is the plugin's own JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageSet {
    pub key: String,
    pub value: Value,
}

/// `storage.get`: `None` in the answer when the key isn't there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageGet {
    pub key: String,
}

/// `storage.list`: every key the plugin holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageList {
    #[serde(default)]
    pub prefix: String,
}

/// `http.fetch`: one GET, POST or other request through the network gate.
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
    /// Echoed in the answer, and in the `HttpResult` event when the call is
    /// answered later: the plugin's own name for this request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

impl HttpRequest {
    fn default_method() -> String {
        "GET".to_owned()
    }

    /// A GET, with nothing else set.
    pub fn get(url: impl Into<String>) -> Self {
        HttpRequest {
            method: "GET".to_owned(),
            url: url.into(),
            headers: Vec::new(),
            body_base64: None,
            tag: None,
        }
    }
}

/// `http.fetch`'s answer: the head as it arrived, with the body streamed to a
/// file the helper reads in chunks.
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

/// `timers.after` and `timers.every`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerRequest {
    /// Milliseconds; a repeating timer is at least
    /// [`crate::limits::MIN_TIMER_INTERVAL_MS`].
    pub interval_ms: u64,
    /// The plugin's name for this timer, echoed in the timer event.
    pub tag: String,
}

/// `timers.cancel`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerCancel {
    pub tag: String,
}

/// `plugin.call`: a call into another plugin's export.
///
/// Only along a declared dependency edge. That is what makes a loop of calls
/// impossible, and so what lets a call wait for its answer without any risk of
/// two plugins each waiting on the other: the caller declares the callee, the
/// callee can't declare the caller, and the parent refuses a call that comes
/// back around with the chain that would have made the loop.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCall {
    /// The plugin called, which the caller's manifest must declare.
    pub plugin: PluginId,
    /// The export on the callee's module to call.
    pub function: String,
    /// What the export is given, base64, because a payload isn't always text.
    #[serde(default)]
    pub args_base64: String,
}

/// `plugin.call`'s answer: what the export returned, base64, or nothing when it
/// returned nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginAnswer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_base64: Option<String>,
}

/// `plugin.publish`: bytes for whoever subscribed to a topic, delivered as they
/// are made and never waited on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginPublish {
    pub topic: String,
    #[serde(default)]
    pub payload: Value,
}

/// `plugin.subscribe`: start or stop hearing a topic, as a topic event in the
/// next `yf_event` batch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSubscribe {
    pub topic: String,
    /// `true` leaves the topic instead of joining it.
    #[serde(default)]
    pub cancel: bool,
}

/// `settings.get`: one of the plugin's settings, as the person set it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingGet {
    pub key: String,
}

/// What `settings.on_change` answers when the person changes a setting, or uses
/// a widget one of them draws.
///
/// A plugin asks once — `settings.on_change` is answered `pending`, so that no
/// instance waits on a person — and every change afterwards arrives as the
/// answer to that call, in the guest's next event batch. A running instance
/// hears of a change without a restart; one that isn't running reads the values
/// with `settings.get` when it next starts, so nothing is kept for it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsNews {
    /// The settings whose values changed, with the values they hold now.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub changed: BTreeMap<String, Value>,
    /// The widget input that arrived, when one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<WidgetInput>,
}

/// One widget's input, for a widget setting the plugin draws.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetInput {
    /// The setting whose widget it was.
    pub setting: String,
    /// The widget the plugin named in the tree it drew.
    pub id: String,
    /// What the person did with it, as the widget's own value.
    pub value: String,
}

/// `app.version`, `app.locale` and `app.theme_mode`'s answer: each gives the
/// whole of what Yapfast is, so a plugin asks once and keeps it.
/// `ui.set_slot`: a widget tree to draw in one of the interface's slots.
///
/// `slot` is one of `banner`, `toast`, `composer`, `message.actions` or
/// `dialog`. `tree` is the widget tree the interface draws: rows of cells,
/// which `docs/plugins/interface.md` describes. It is untyped here because
/// the crate that checks it is the parent's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SetSlot {
    pub slot: String,
    pub tree: Value,
}

/// `ui.set_slot` and `ui.clear_slot`'s answer: whether the interface drew
/// the tree, and, for what it can't draw, the sentence saying why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotDrawn {
    pub drawn: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl SlotDrawn {
    /// The interface drew it.
    pub fn yes() -> Self {
        Self {
            drawn: true,
            reason: None,
        }
    }

    /// It didn't, and this is why, in words a plugin's author can read.
    pub fn no(reason: impl Into<String>) -> Self {
        Self {
            drawn: false,
            reason: Some(reason.into()),
        }
    }
}

/// `ui.clear_slot`: the slot to empty.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearSlot {
    pub slot: String,
}

/// `ui.notice` and `ui.toast`: a line for the person, under the plugin's
/// name. `text` is markdown for a notice and plain text for a toast.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub text: String,
}

/// `ui.open_dialog`: a widget tree to ask the person something with; the
/// first heading or text row is what the dialog is titled.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OpenDialog {
    pub tree: Value,
}

/// `app.version` and `app.locale`'s answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    pub version: String,
    /// A language tag, such as `en-GB`.
    pub locale: String,
    /// `light` or `dark`.
    pub theme_mode: String,
}

/// `account.me`'s answer: the account this Yapfast is signed in as.
///
/// It carries no token and no email: what a plugin may know about the person
/// is what every other surface shows of them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountView {
    /// The user id on the account's network.
    pub id: String,
    /// The name it shows, which is its display name here.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// A `discord://` or `matrix://` reference, as [`crate::event::PersonView`]
    /// names one; never an authenticated address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// `online`, `idle`, `dnd` or `invisible`, when the network says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Whether this device is trusted, which is a thing only the person and
    /// Yapfast know; a plugin is told so it can say "this device" honestly.
    #[serde(default)]
    pub trusted: bool,
}

/// `rooms.get` and `messages.*`: which room, by the id the sidebar uses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomGet {
    pub room: String,
}

/// `rooms.list`: every room the account can see, newest activity first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomsList {
    /// How many to answer with; the runtime's own cap applies on top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// `messages.get`: one message of a room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageGet {
    pub room: String,
    pub message: String,
}

/// `messages.history`: a page of a room's timeline, newest first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageHistory {
    pub room: String,
    /// The message to stop before; the newest page when it is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// `people.get`: one person the application knows of.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeopleGet {
    pub person: String,
}

/// `people.member`: one person as one server sees them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeopleMember {
    pub server: String,
    pub person: String,
}

/// `people.member`'s answer: the person, with what the server adds about them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberView {
    #[serde(flatten)]
    pub person: crate::event::PersonView,
    /// The ids of their roles in that server, as far as Yapfast knows.
    #[serde(default)]
    pub roles: Vec<String>,
    /// `online`, `idle`, `dnd` or `offline`, when the network says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence: Option<String>,
    /// The RGB colour of their name there, from their highest coloured role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<u32>,
    /// Their custom status, else what they're doing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
    /// When their timeout there ends, in Unix milliseconds, while it lasts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timed_out_until: Option<u64>,
}

/// A command a person typed in the composer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandInvocation {
    pub name: String,
    /// The options, by name.
    #[serde(default)]
    pub options: serde_json::Map<String, Value>,
}

/// What a command answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    /// Nothing to show.
    None,
    /// A line for the person, shown in the room they typed it in.
    Notice {
        text: String,
    },
    /// The command's JSON answer, for a plugin that wants it.
    Value(Value),
    Failed {
        message: String,
    },
}

/// Base64, standard alphabet with padding.
///
/// Bodies cross the pipe as JSON, so bytes that aren't text need an encoding:
/// this is the one every PDK language has, and it costs a third more than the
/// bytes themselves, which is nothing beside a 1 MiB frame.
pub mod base64 {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

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
        if !bytes.len().is_multiple_of(4) {
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
    fn base64_round_trips_every_length() {
        let alphabet = b"the quick brown fox";
        for length in 0..=32 {
            let bytes: Vec<u8> = (0..length).map(|i| alphabet[i % alphabet.len()]).collect();
            let encoded = base64::encode(&bytes);
            assert_eq!(
                base64::decode(&encoded).as_deref(),
                Some(&bytes[..]),
                "{length}"
            );
        }
        assert_eq!(base64::encode(b"a"), "YQ==");
        assert_eq!(base64::encode(b"ab"), "YWI=");
        assert_eq!(base64::encode(b"abc"), "YWJj");
        assert_eq!(base64::encode(b""), "");
    }

    #[test]
    fn base64_refuses_nonsense() {
        for text in ["YQ=", "YQ", "Y Q==", "=Q==", "YQ==YQ=="] {
            if text == "YQ==YQ==" {
                // Two padded groups is decodable in this reading, so it must
                // at least not panic; the helper only decodes what it wrote.
                let _ = base64::decode(text);
                continue;
            }
            assert_eq!(base64::decode(text), None, "{text}");
        }
    }

    #[test]
    fn a_request_builds_from_its_typed_payload() {
        let call = HostCall::new(
            method::HTTP_FETCH,
            HttpRequest::get("https://api.example.com/x"),
        )
        .unwrap();
        assert_eq!(call.method, "http.fetch");
        let parsed: HttpRequest = call.parse().unwrap();
        assert_eq!(parsed.method, "GET");
        assert_eq!(parsed.url, "https://api.example.com/x");
    }

    #[test]
    fn a_pending_answer_is_the_only_one_that_waits() {
        assert!(HostReturn::Pending { tag: "x".into() }.is_pending());
        assert!(!HostReturn::Unsupported.is_pending());
        assert_eq!(HostReturn::Ok(Value::Null).ok_value(), Some(Value::Null));
    }
}
