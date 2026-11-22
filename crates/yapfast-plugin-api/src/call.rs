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
    pub const STORAGE_CLEAR: &str = "storage.clear";
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
    pub const MEDIA_GET: &str = "media.get";

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

    // The curated actions of PLAN §17.2, and audio. Each wraps something
    // Yapfast already does, so the token, the rate limiter and the person's
    // own permissions stay here.
    pub const DISCORD_SET_STATUS: &str = "discord.set_status";
    pub const DISCORD_SET_CUSTOM_STATUS: &str = "discord.set_custom_status";
    pub const DISCORD_SET_ACTIVITY: &str = "discord.set_activity";
    pub const DISCORD_CLEAR_ACTIVITY: &str = "discord.clear_activity";
    pub const DISCORD_MARK_READ: &str = "discord.mark_read";
    pub const DISCORD_SET_MUTE: &str = "discord.set_mute";
    pub const DISCORD_OPEN_ROOM: &str = "discord.open_room";
    pub const DISCORD_START_REPLY: &str = "discord.start_reply";
    pub const DISCORD_START_EDIT: &str = "discord.start_edit";
    pub const DISCORD_REACTORS: &str = "discord.reactors";
    pub const DISCORD_PAUSE_INVITES: &str = "discord.pause_invites";
    pub const DISCORD_ADD_EMOJI: &str = "discord.add_emoji";
    pub const DISCORD_ADD_STICKER: &str = "discord.add_sticker";
    pub const AUDIO_PLAY: &str = "audio.play";
    pub const AUDIO_SPEAK: &str = "audio.speak";

    // Reads of the call, the save dialog, and the package's own assets.
    pub const VOICE_STATE: &str = "voice.state";
    pub const FILES_SAVE: &str = "files.save";
    pub const ASSETS_LIST: &str = "assets.list";
    pub const ASSETS_GET: &str = "assets.get";

    // The powerful tier: a folder, the network past the manifest's hosts, and
    // a local endpoint. Each is behind its capability, the Advanced switch and
    // a first-use dialog.
    pub const FS_LIST: &str = "fs.list";
    pub const FS_STAT: &str = "fs.stat";
    pub const FS_READ: &str = "fs.read";
    pub const FS_WRITE: &str = "fs.write";
    pub const FS_THUMBNAIL: &str = "fs.thumbnail";
    pub const NET_LISTEN: &str = "net.listen";
    pub const NET_RESPOND: &str = "net.respond";
    // `net.socket`, powerful too: a raw TCP or WebSocket connection out, whose
    // data arrives as further answers to `net.connect`.
    pub const NET_CONNECT: &str = "net.connect";
    pub const NET_SEND: &str = "net.send";
    pub const NET_CLOSE: &str = "net.close";

    // A few words after the window's title: `window.title`.
    pub const WINDOW_SET_TITLE: &str = "window.set_title";

    // Sending a plugin's own files, the account's own activity and profile,
    // the reads of search, drafts and sessions, and the linked Spotify. Each
    // has a capability of its own; see `crate::account` for the payloads.
    pub const MESSAGES_ATTACH: &str = "messages.attach";
    pub const ACTIVITY_SET: &str = "activity.set";
    pub const ACTIVITY_CLEAR: &str = "activity.clear";
    pub const PROFILE_SET: &str = "profile.set";
    pub const MESSAGES_SEARCH: &str = "messages.search";
    pub const DRAFTS_LIST: &str = "drafts.list";
    pub const ACCOUNT_SESSIONS: &str = "account.sessions";
    pub const SPOTIFY_NOW: &str = "spotify.now";
    pub const SPOTIFY_PLAY: &str = "spotify.play";
    pub const SPOTIFY_PAUSE: &str = "spotify.pause";
    pub const SPOTIFY_NEXT: &str = "spotify.next";
    pub const SPOTIFY_PREVIOUS: &str = "spotify.previous";
    pub const SPOTIFY_SEEK: &str = "spotify.seek";
    pub const SPOTIFY_VOLUME: &str = "spotify.volume";
    pub const SPOTIFY_SHUFFLE: &str = "spotify.shuffle";
    pub const SPOTIFY_REPEAT: &str = "spotify.repeat";
    // Who Discord ranks closest to the account, read-only: `relationships.read`.
    pub const RELATIONSHIPS_AFFINITIES: &str = "relationships.affinities";

    // The bytes of a file `attachment_pre_upload` holds: read it in pieces,
    // write the file that goes in its place, and say what goes.
    // `uploads.intercept`.
    pub const UPLOADS_READ: &str = "uploads.read";
    pub const UPLOADS_WRITE: &str = "uploads.write";
    pub const UPLOADS_FINISH: &str = "uploads.finish";

    // A plugin's own login to a service, such as Spotify with its own app:
    // `oauth.login`.
    pub const OAUTH_LOGIN: &str = "oauth.login";
    pub const OAUTH_TOKEN: &str = "oauth.token";
    pub const OAUTH_LOGOUT: &str = "oauth.logout";

    /// Every name above, in the order they are declared.
    ///
    /// A method is reachable only if the code in front of it knows its name, so
    /// the helper's table of the calls it forwards is a list of these, and this
    /// is what a test compares that table against: a method added above without
    /// a place in the table would answer `unsupported` to every guest forever,
    /// and nothing else would say so.
    pub const ALL: [&str; 95] = [
        APP_VERSION,
        APP_LOCALE,
        APP_THEME_MODE,
        ACCOUNT_ME,
        ROOMS_SELECTED,
        ROOMS_GET,
        ROOMS_LIST,
        MESSAGES_GET,
        MESSAGES_HISTORY,
        PEOPLE_GET,
        PEOPLE_MEMBER,
        STORAGE_GET,
        STORAGE_SET,
        STORAGE_DELETE,
        STORAGE_LIST,
        STORAGE_CLEAR,
        SETTINGS_GET,
        SETTINGS_ON_CHANGE,
        HTTP_FETCH,
        TIMERS_AFTER,
        TIMERS_EVERY,
        TIMERS_CANCEL,
        UI_SET_SLOT,
        UI_CLEAR_SLOT,
        UI_NOTICE,
        UI_TOAST,
        UI_OPEN_DIALOG,
        MEDIA_GET,
        MESSAGES_SEND,
        MESSAGES_EDIT,
        MESSAGES_DELETE,
        MESSAGES_REACT,
        MESSAGES_JUMP_TO,
        COMPOSER_GET_DRAFT,
        COMPOSER_SET_DRAFT,
        COMPOSER_INSERT,
        PLUGIN_CALL,
        PLUGIN_PUBLISH,
        PLUGIN_SUBSCRIBE,
        NOTIFY_SEND,
        CLIPBOARD_WRITE,
        OPEN_URL,
        DISCORD_SET_STATUS,
        DISCORD_SET_CUSTOM_STATUS,
        DISCORD_SET_ACTIVITY,
        DISCORD_CLEAR_ACTIVITY,
        DISCORD_MARK_READ,
        DISCORD_SET_MUTE,
        DISCORD_OPEN_ROOM,
        DISCORD_START_REPLY,
        DISCORD_START_EDIT,
        DISCORD_REACTORS,
        DISCORD_PAUSE_INVITES,
        DISCORD_ADD_EMOJI,
        DISCORD_ADD_STICKER,
        AUDIO_PLAY,
        AUDIO_SPEAK,
        VOICE_STATE,
        FILES_SAVE,
        ASSETS_LIST,
        ASSETS_GET,
        FS_LIST,
        FS_STAT,
        FS_READ,
        FS_WRITE,
        FS_THUMBNAIL,
        NET_LISTEN,
        NET_RESPOND,
        NET_CONNECT,
        NET_SEND,
        NET_CLOSE,
        WINDOW_SET_TITLE,
        MESSAGES_ATTACH,
        ACTIVITY_SET,
        ACTIVITY_CLEAR,
        PROFILE_SET,
        MESSAGES_SEARCH,
        DRAFTS_LIST,
        ACCOUNT_SESSIONS,
        SPOTIFY_NOW,
        SPOTIFY_PLAY,
        SPOTIFY_PAUSE,
        SPOTIFY_NEXT,
        SPOTIFY_PREVIOUS,
        SPOTIFY_SEEK,
        SPOTIFY_VOLUME,
        SPOTIFY_SHUFFLE,
        SPOTIFY_REPEAT,
        RELATIONSHIPS_AFFINITIES,
        UPLOADS_READ,
        UPLOADS_WRITE,
        UPLOADS_FINISH,
        OAUTH_LOGIN,
        OAUTH_TOKEN,
        OAUTH_LOGOUT,
    ];

    /// The Spotify calls, which share one capability.
    pub const SPOTIFY: [&str; 9] = [
        SPOTIFY_NOW,
        SPOTIFY_PLAY,
        SPOTIFY_PAUSE,
        SPOTIFY_NEXT,
        SPOTIFY_PREVIOUS,
        SPOTIFY_SEEK,
        SPOTIFY_VOLUME,
        SPOTIFY_SHUFFLE,
        SPOTIFY_REPEAT,
    ];
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
    /// The body is a file `attachment_pre_upload` holds for this plugin,
    /// streamed from disk as it is sent, so a file of any size never crosses
    /// the pipe or sits in memory. Instead of `body_base64`, never with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_upload: Option<UploadBody>,
}

/// `http.fetch`'s `body_upload`: which held file, and whether it goes as the
/// whole body or as one part of a `multipart/form-data` form, as most file
/// hosts take one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadBody {
    /// The `upload` the hook's subject named.
    pub upload: u64,
    /// The file the plugin wrote with `uploads.write`, rather than the one
    /// the person picked.
    #[serde(default)]
    pub replacement: bool,
    /// Send it inside a form, with the boundary and the `Content-Type` made
    /// here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<UploadForm>,
}

/// A `multipart/form-data` form around a held file: its own field, and the
/// text fields before it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadForm {
    /// The file's field name, `file` unless given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// The file name the form gives, the upload's own unless given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// The file part's type, `application/octet-stream` unless given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Text fields, in order, before the file.
    #[serde(default)]
    pub fields: Vec<(String, String)>,
}

/// `uploads.read`: a piece of the held file, from `offset`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadRead {
    pub upload: u64,
    #[serde(default)]
    pub offset: u64,
    /// At most [`UploadRead::MAX_PIECE`], which is also the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    /// The plugin's written file, rather than the person's.
    #[serde(default)]
    pub replacement: bool,
}

impl UploadRead {
    /// The most one read answers: a frame's worth once it is base64.
    pub const MAX_PIECE: u32 = 512 * 1024;
}

/// `uploads.read`'s answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadPiece {
    pub data_base64: String,
    pub offset: u64,
    /// The whole file's size.
    pub bytes: u64,
    /// Whether this piece reaches its end.
    pub eof: bool,
}

/// `uploads.write`: the next piece of the file that goes in place of the
/// person's, at `offset`, which is where the last piece ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadWrite {
    pub upload: u64,
    #[serde(default)]
    pub offset: u64,
    pub data_base64: String,
}

/// `uploads.finish`: what goes, once the plugin is done with a file it held.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadFinish {
    pub upload: u64,
    pub send: UploadSend,
    /// A new name for what goes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Text put into the composer where the person is writing: a link to the
    /// file on the plugin's own host, say, when nothing goes to Discord.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert: Option<String>,
}

/// Which file goes with the message, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadSend {
    /// The person's file, as they picked it.
    Original,
    /// The file the plugin wrote with `uploads.write`.
    Replacement,
    /// Nothing: the file stays off the message.
    Nothing,
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
            body_upload: None,
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
    /// The plugin's name for this timer. It is kept for the plugin's own
    /// bookkeeping and not sent back: a timer that fires arrives as an empty
    /// event batch, which is the only thing the guest is told.
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
/// `messages.send`: a message the plugin wants sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendMessage {
    pub room: String,
    pub body: String,
    /// Files from the plugin's folder to send with it, by their paths inside
    /// that folder: `messages.attach` and `fs.folder` or `fs.full`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// Files the plugin made, or its package carries: `messages.attach`.
    /// With `files`, at most [`SendMessage::MAX_FILES`] in all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
}

impl SendMessage {
    /// The most files one message carries, as Discord's own limit.
    pub const MAX_FILES: usize = 10;
}

/// One file a plugin sends with a message: bytes it made, base64, or one of
/// its own assets by name, and never both. The name is what Discord shows,
/// a file name and no path.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// Covered until clicked, as a person's spoiler is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub spoiler: bool,
}

/// `fs.list`, `fs.stat` and `fs.read`'s subject: a path inside the folder the
/// plugin was given, with `/` between its parts. Empty is the folder itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsPath {
    #[serde(default)]
    pub path: String,
}

/// One entry of `fs.list`, or `fs.stat`'s answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsEntry {
    /// Its path inside the folder.
    pub path: String,
    pub dir: bool,
    pub bytes: u64,
    /// When it last changed, in milliseconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
}

/// `fs.read`: a piece of one file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsRead {
    pub path: String,
    #[serde(default)]
    pub offset: u64,
    /// At most [`FsRead::MAX_BYTES`]; that many when unsaid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u64>,
}

impl FsRead {
    /// The most one read answers, so the answer fits one frame as base64.
    pub const MAX_BYTES: u64 = 512 << 10;
}

/// `fs.read`'s answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsBytes {
    /// The file's whole length, so a plugin knows when it has read it all.
    pub size: u64,
    pub offset: u64,
    pub bytes_base64: String,
}

/// `fs.write`: one file's contents, or more of them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsWrite {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_base64: Option<String>,
    /// Add to the end rather than replace.
    #[serde(default)]
    pub append: bool,
}

/// `fs.thumbnail`: a small picture of one image in the folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsThumbnail {
    pub path: String,
    /// The longer side, in pixels: 16 to [`FsThumbnail::MAX_SIDE`].
    #[serde(default = "FsThumbnail::default_side")]
    pub size: u32,
}

impl FsThumbnail {
    pub const MAX_SIDE: u32 = 256;
    /// The largest picture file a thumbnail is made from.
    pub const MAX_SOURCE_BYTES: u64 = 32 << 20;

    fn default_side() -> u32 {
        128
    }
}

/// `fs.thumbnail`'s answer: a PNG.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub png_base64: String,
}

/// `net.listen`'s first answer, `pending`, is followed by one answer per
/// request under the same call: an [`Incoming`] each.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incoming {
    /// The number `net.respond` answers it with.
    pub request: u64,
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body_base64: String,
}

/// `net.respond`: the answer to one [`Incoming`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Respond {
    pub request: u64,
    #[serde(default = "Respond::ok")]
    pub status: u16,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_base64: Option<String>,
}

impl Respond {
    fn ok() -> u16 {
        200
    }
}

/// `net.connect`: a raw connection out, under `net.socket`.
///
/// `tls` wraps it in TLS, checked against the system's roots for `host`;
/// `websocket` makes it a WebSocket at that path (`/` and what follows), over
/// `wss` with `tls` and `ws` without. The first answer says it connected,
/// and every answer after it is a [`SocketNews`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetConnect {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub tls: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub websocket: Option<String>,
}

/// `net.send`: bytes, or a WebSocket's text, for one open connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetSend {
    pub socket: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_base64: Option<String>,
}

/// `net.close`: one connection, closed from the plugin's side.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetClose {
    pub socket: u64,
}

/// What a connection tells its plugin, as one more answer to `net.connect`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "news")]
pub enum SocketNews {
    /// It is open, to this address.
    Connected { socket: u64, address: String },
    /// Bytes arrived, or for a WebSocket, a binary message.
    Data { socket: u64, data_base64: String },
    /// A WebSocket's text message.
    Text { socket: u64, text: String },
    /// It is closed, and why: the far end, the plugin, a limit or Yapfast.
    Closed { socket: u64, reason: String },
}

/// `window.set_title`: the few words after the window's title, or none to take
/// them away.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowTitle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
}

impl WindowTitle {
    /// The most characters a plugin adds to the title.
    pub const MAX_SUFFIX: usize = 32;
}

/// `messages.edit`: a message of the open room, with what it should say.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditMessage {
    pub room: String,
    pub message: String,
    pub body: String,
}

/// `messages.get`'s and the other single-message calls' subject.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRef {
    pub room: String,
    pub message: String,
}

/// `messages.react`: the emoji to react with, as Discord writes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactMessage {
    pub room: String,
    pub message: String,
    pub emoji: String,
}

/// `composer.get_draft` and `composer.set_draft`'s room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftRequest {
    pub room: String,
    /// What the draft should say, for `composer.set_draft`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// `composer.insert`: text to put after what the person has written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InsertText {
    pub room: String,
    pub text: String,
}

/// `notify.send`: a line for the person, as the OS notification shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotifySend {
    pub text: String,
}

/// `clipboard.write`: text for the clipboard.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyText {
    pub text: String,
}

/// `open.url`: a page for the browser, which Yapfast confirms as it does a
/// link in a message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenUrl {
    pub url: String,
}

/// `discord.set_status`: the status the account shows, as Discord names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetStatus {
    /// `online`, `idle`, `dnd` or `invisible`.
    pub status: String,
}

/// `discord.set_custom_status`: the line under the account's name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetCustomStatus {
    /// Empty clears the text, and with no emoji the status too.
    #[serde(default)]
    pub text: String,
    /// A Unicode emoji, or a custom one's `:name:`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    /// Seconds after which it goes; absent keeps it until it's changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_after: Option<u64>,
}

/// `discord.set_activity`: whether the game Discord sees this account play
/// shows on it. A plugin's own activity is `activity.set`
/// ([`crate::account::ActivitySet`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetActivity {
    pub enabled: bool,
}

/// How long a mute lasts, as `discord.set_mute` asks for one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mute {
    /// Unmute it.
    Off,
    /// Until it's unmuted.
    Always,
    /// For this many minutes.
    Minutes(u32),
}

/// `discord.set_mute`: a conversation, and how long it stays quiet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetMute {
    pub room: String,
    pub muting: Mute,
}

/// `discord.pause_invites`: a server's invites and direct messages, paused
/// or resumed, for `hours`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PauseInvites {
    pub server: String,
    #[serde(default)]
    pub invites: bool,
    #[serde(default)]
    pub dms: bool,
    /// One of the lengths Discord's own dialog offers.
    #[serde(default)]
    pub hours: u8,
}

/// `discord.add_emoji`: a new emoji for a server, as a picture the plugin
/// brings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddEmoji {
    pub server: String,
    pub name: String,
    /// A PNG, JPEG or GIF, base64: the whole picture is one frame, so it is
    /// capped at 256 KB as Discord caps an emoji. Empty when `asset` names one.
    #[serde(default)]
    pub image_base64: String,
    /// One of the manifest's picture `assets`, by name, instead of bytes: how a
    /// plugin's own picture becomes an emoji a person can react with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}

/// `discord.add_sticker`: a new sticker for a server, with its name, its
/// related emoji and its description.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddSticker {
    pub server: String,
    pub name: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub description: String,
    /// A PNG, JPEG or GIF, base64: one frame, capped at 512 KB as Discord
    /// caps a sticker. Empty when `asset` names one.
    #[serde(default)]
    pub image_base64: String,
    /// One of the manifest's picture `assets`, by name, instead of bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}

/// `discord.reactors`' answer: one reaction of a message, as the timeline
/// Yapfast holds has it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionCount {
    /// The emoji as Discord writes it in the message's `reactions`.
    pub emoji: String,
    pub count: u64,
    /// Whether this account gave it.
    #[serde(default)]
    pub me: bool,
}

/// `audio.play`: a sound the plugin brings, or one of the sounds its package
/// carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaySound {
    /// A sound file base64: WAV, or, in a build with Discord's media stack,
    /// the MP3 and Ogg the soundboard plays too. Empty when `asset` names one.
    #[serde(default)]
    pub sound_base64: String,
    /// One of the manifest's `assets`, by name, instead of bytes in the call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// A percentage of the sound's own loudness; absent plays it as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<u8>,
}

/// `files.save`: a file the plugin hands the person, who chooses where it goes
/// in the system's save dialog.
///
/// The parent writes the file, never the plugin: the guest has no filesystem,
/// and the only path is the one the person picked. Exactly one of `text` and
/// `bytes_base64` is given, and the bytes are at most
/// [`crate::limits::MAX_SAVED_FILE_BYTES`], since they arrive in one frame.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveFile {
    /// The name the dialog suggests: a file name, never a path.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_base64: Option<String>,
}

/// `files.save`'s answer, once the person closed the dialog.
///
/// The name of the file they saved, and never the folder: the path would tell
/// a plugin the person's account name and how their disk is laid out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSaved {
    pub saved: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// `assets.get`: one of the package's own assets, by its manifest name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetGet {
    pub name: String,
}

/// `assets.get`'s answer: the asset's kind and its bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetBytes {
    pub name: String,
    pub kind: crate::assets::AssetKind,
    pub bytes: u64,
    pub bytes_base64: String,
}

/// `voice.state`'s answer, and the `voice_state` event's payload: the call this
/// account is in, as the call panel shows it. Read-only: nothing here mutes,
/// moves or hears anyone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceView {
    /// The voice channel or the DM the call is in.
    pub room: String,
    /// The name the call panel shows for it.
    pub name: String,
    /// The server, for a voice channel of one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// `joining`, `connected` or `reconnecting`.
    pub phase: String,
    /// This account's microphone is off.
    pub muted: bool,
    /// This account hears nothing.
    pub deafened: bool,
    /// This account shares its screen.
    pub streaming: bool,
    /// This account's camera is on.
    pub video: bool,
    /// Whose stream this account watches, by person id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watching: Option<String>,
    /// Who is in the call, this account included, in the call panel's order.
    /// At most [`VoiceView::MAX_PEOPLE`].
    pub people: Vec<VoicePerson>,
}

impl VoiceView {
    /// The most people one answer lists: a stage of thousands is a list no
    /// plugin needs whole, and the answer stays far under a frame.
    pub const MAX_PEOPLE: usize = 99;
}

/// One person in the call.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoicePerson {
    /// Who they are, by Discord or Matrix user id.
    pub id: String,
    /// Whether it is this account.
    #[serde(default)]
    pub own: bool,
    pub muted: bool,
    pub deafened: bool,
    /// Muted or deafened by the server, rather than by themselves.
    #[serde(default)]
    pub server_muted: bool,
    pub speaking: bool,
    /// Sharing their screen.
    pub streaming: bool,
    /// Their camera is on.
    pub video: bool,
}

/// `audio.speak`: what the OS's speech says, and nothing else.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speak {
    pub text: String,
}

/// `media.get`: the bytes of an attachment, an avatar or an emoji Yapfast already
/// keeps, by its address.
///
/// The answer is `{ "bytes": 1234, "bytes_base64": "…" }`: the length, so a
/// plugin can decide before decoding, and the bytes, base64 because a frame's
/// payload is JSON. A picture larger than one frame is refused and never cut.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaGet {
    pub url: String,
}

/// `ui.set_slot`: a widget tree to draw in one of the interface's slots.
///
/// `slot` is one of `banner`, `toast`, `composer`, `message.actions` or
/// `dialog`. `tree` is the widget tree the interface draws: rows of cells,
/// which `docs/plugins/interface.md` describes. It is untyped here because
/// the crate that checks it is the parent's. `part` is which of the names the
/// manifest's `parts` gives this slot the tree is drawn as; left out, it is the
/// first, or none when the manifest names none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SetSlot {
    pub slot: String,
    pub tree: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
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
