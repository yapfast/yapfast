//! The two host functions a guest may import, and where their requests go.
//!
//! `yf_call` answers at once with `HostReturn::Pending` and lets the parent
//! answer later, because everything a call can reach — storage, the network
//! gate, a timer, a consent dialog — belongs to the parent, and waiting for it
//! inside the host function would hold a worker for as long as a person takes
//! to decide. `yf_post` asks for nothing back.
//!
//! Both take one Extism memory handle, and `yf_call` returns one: the payload
//! is JSON of a host call and the answer is JSON of a host return, written into
//! the guest's own memory for it to read and free. The handle is Extism's usual
//! packing of an offset and a length into one `i64`, which is what a PDK's
//! `Memory` type already holds, so passing one is one store instruction. The
//! handle a guest passes is taken rather than borrowed, as Extism's own kernel
//! functions take one, so neither side has to remember who frees what.
//!
//! Nothing here blocks: the outbox is an unbounded channel the scheduler drains,
//! so a host call costs a `send` and an allocation for the answer.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

use extism::{CurrentPlugin, Error, Function, PTR, UserData, Val};
use serde::Deserialize;

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{HostCall, HostReturn, method};
use yapfast_plugin_api::message::{FromHelper, LogLevel, LogRecord};

/// What a host function needs to reach the parent, and whose requests these
/// are.
///
/// One per plugin, shared by every instance it makes over the helper's life:
/// keeping the counter there rather than in an instance means an id is never
/// used twice for different questions, which is what lets a guest match an
/// answer to the call it made before its instance was discarded.
pub struct Channel {
    plugin: PluginId,
    outbox: mpsc::Sender<FromHelper>,
    next_call: AtomicU64,
}

impl Channel {
    pub fn new(plugin: PluginId, outbox: mpsc::Sender<FromHelper>) -> Self {
        Channel {
            plugin,
            outbox,
            next_call: AtomicU64::new(0),
        }
    }

    /// The id for the next request, counting from one; zero reads like "no
    /// call" in a log and in a plugin's own bookkeeping.
    ///
    /// This number is the tag the guest is answered `pending` with, and the
    /// parent's answer has to come back as `ToHelper::Return::call` for the two
    /// halves of a pending call to meet again. The frame that carries
    /// `FromHelper::Call` to the parent therefore has to go out with this
    /// number as its own id: it is the only id both sides can agree on before
    /// the answer exists.
    fn next_call(&self) -> u64 {
        self.next_call.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn send(&self, message: FromHelper) -> Result<(), Error> {
        self.outbox
            .send(message)
            .map_err(|_| Error::msg("the parent is gone, so nothing can answer a host call"))
    }
}

/// The guest's calls that this build hands to the parent.
///
/// A name outside this list — a typo, or a method from a build newer than this
/// one — is answered `unsupported` here rather than sent: the guest gets an
/// answer it can feature-detect on in the same call, and the parent is spared a
/// round trip for something it can't do. Names that are listed and not yet
/// implemented still travel, because the parent knows which capabilities it has
/// and answers with the reason a person would want to read.
const FORWARDED: [&str; 34] = [
    method::APP_VERSION,
    method::APP_LOCALE,
    method::APP_THEME_MODE,
    method::ACCOUNT_ME,
    method::ROOMS_SELECTED,
    method::ROOMS_GET,
    method::ROOMS_LIST,
    method::MESSAGES_GET,
    method::MESSAGES_HISTORY,
    method::PEOPLE_GET,
    method::STORAGE_GET,
    method::STORAGE_SET,
    method::STORAGE_DELETE,
    method::STORAGE_LIST,
    method::SETTINGS_GET,
    method::SETTINGS_ON_CHANGE,
    method::HTTP_FETCH,
    method::TIMERS_AFTER,
    method::TIMERS_EVERY,
    method::TIMERS_CANCEL,
    method::MESSAGES_SEND,
    method::MESSAGES_EDIT,
    method::MESSAGES_DELETE,
    method::MESSAGES_REACT,
    method::MESSAGES_JUMP_TO,
    method::COMPOSER_GET_DRAFT,
    method::COMPOSER_SET_DRAFT,
    method::COMPOSER_INSERT,
    method::PLUGIN_CALL,
    method::PLUGIN_PUBLISH,
    method::PLUGIN_SUBSCRIBE,
    method::NOTIFY_SEND,
    method::CLIPBOARD_WRITE,
    method::OPEN_URL,
];

/// Whether this build has a method, and so whether it is worth a round trip.
fn forwards(name: &str) -> bool {
    FORWARDED.contains(&name)
}

/// The method a plugin posts a log line with.
///
/// Extism's own guest logging goes through a global `tracing` subscriber that
/// this process already installs for its own lines, so the runtime cannot give
/// one plugin's lines to one plugin's ring buffer. The seam is a post: a PDK's
/// `log` call arrives here and is turned into a log record instead of a host
/// call, which is the only route that keeps a line attributable to its plugin.
const LOG_METHOD: &str = "log";

/// The host functions to compile a plugin with.
pub fn functions(channel: Arc<Channel>) -> [Function; 2] {
    [
        Function::new(
            "yf_call",
            [PTR],
            [PTR],
            UserData::new(channel.clone()),
            yf_call,
        ),
        Function::new("yf_post", [PTR], [], UserData::new(channel), yf_post),
    ]
}

/// A request and its answer: sent to the parent, answered to the guest at once
/// as pending.
fn yf_call(
    plugin: &mut CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<Arc<Channel>>,
) -> Result<(), Error> {
    let channel = channel(&user_data)?;
    let call = read_call(plugin, inputs, "yf_call")?;
    let answer = if forwards(&call.method) {
        let id = channel.next_call();
        channel.send(FromHelper::Call {
            plugin: channel.plugin.clone(),
            call: id,
            request: call,
        })?;
        HostReturn::Pending {
            tag: id.to_string(),
        }
    } else {
        HostReturn::Unsupported
    };
    write_answer(plugin, outputs, &answer)
}

/// Fire and forget: a log line, or something the plugin wants published without
/// waiting for an answer.
fn yf_post(
    plugin: &mut CurrentPlugin,
    inputs: &[Val],
    _outputs: &mut [Val],
    user_data: UserData<Arc<Channel>>,
) -> Result<(), Error> {
    let channel = channel(&user_data)?;
    let post = read_call(plugin, inputs, "yf_post")?;
    if post.method == LOG_METHOD
        && let Some(record) = log_record(&post.payload)
    {
        return channel.send(FromHelper::Log {
            plugin: channel.plugin.clone(),
            record,
        });
    }
    // Fire and forget: the guest is given no answer, but the post still takes a
    // number so a parent's log can name it.
    let id = channel.next_call();
    channel.send(FromHelper::Call {
        plugin: channel.plugin.clone(),
        call: id,
        request: post,
    })
}

/// One post that reads as a log line, when its payload has that shape.
///
/// A plugin that uses `log` as a name for something else still has its post
/// delivered as a call, so the one method name this build reserves can't swallow
/// a plugin's own meaning for it.
fn log_record(payload: &serde_json::Value) -> Option<LogRecord> {
    #[derive(Deserialize)]
    struct Line {
        #[serde(default)]
        level: Option<LogLevel>,
        #[serde(default)]
        target: Option<String>,
        message: String,
    }
    let line: Line = serde_json::from_value(payload.clone()).ok()?;
    Some(LogRecord {
        level: line.level.unwrap_or(LogLevel::Info),
        target: line.target.unwrap_or_else(|| LOG_METHOD.to_owned()),
        message: line.message,
        at: now_ms(),
    })
}

/// Milliseconds since the Unix epoch, which is what a log record carries.
///
/// A clock before the epoch is no reason to lose a line, so it reads as zero.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

fn channel(user_data: &UserData<Arc<Channel>>) -> Result<Arc<Channel>, Error> {
    let shared = user_data.get()?;
    let channel = shared
        .lock()
        .map_err(|_| Error::msg("the host call channel was poisoned"))?
        .clone();
    Ok(channel)
}

/// The host call a guest passed as its one argument.
///
/// The guest hands over its allocation, the way Extism's own kernel functions
/// take one: this frees the handle once it has read it, so a host call costs
/// the guest no bookkeeping and leaks nothing over a long session. A PDK must
/// therefore pass the offset `Memory::new` answered and not free it again.
fn read_call(plugin: &mut CurrentPlugin, inputs: &[Val], name: &str) -> Result<HostCall, Error> {
    let payload = inputs
        .first()
        .ok_or_else(|| Error::msg(format!("{name} was called without a payload")))?;
    let handle = payload
        .i64()
        .and_then(|offset| plugin.memory_handle(offset as u64))
        .ok_or_else(|| {
            Error::msg(format!(
                "{name} was given a payload that isn't in guest memory"
            ))
        })?;
    let call = {
        let bytes = plugin.memory_bytes(handle)?;
        serde_json::from_slice(bytes)
    };
    plugin.memory_free(handle)?;
    call.map_err(|error| {
        Error::msg(format!(
            "{name} was given something that is not a host call: {error}"
        ))
    })
}

/// Writes an answer into the guest's memory and hands back its handle.
fn write_answer(
    plugin: &mut CurrentPlugin,
    outputs: &mut [Val],
    answer: &HostReturn,
) -> Result<(), Error> {
    let Some(slot) = outputs.first_mut() else {
        return Ok(());
    };
    let text = serde_json::to_vec(answer)?;
    plugin.memory_set_val(slot, text.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_named_method_travels_and_a_typo_does_not() {
        assert!(forwards(method::HTTP_FETCH));
        assert!(forwards(method::STORAGE_GET));
        assert!(!forwards("messages.sent"));
        assert!(!forwards(""));
    }

    #[test]
    fn ids_are_never_zero_and_never_repeat() {
        let (outbox, _inbox) = mpsc::channel();
        let channel = Channel::new(PluginId::parse("com.example.word-filter").unwrap(), outbox);
        assert_eq!(channel.next_call(), 1);
        assert_eq!(channel.next_call(), 2);
    }

    #[test]
    fn a_log_post_becomes_a_log_record() {
        let record = log_record(&json!({
            "level": "warn",
            "target": "filter",
            "message": "no room",
        }))
        .expect("this payload is a log line");
        assert_eq!(record.level, LogLevel::Warn);
        assert_eq!(record.target, "filter");
        assert_eq!(record.message, "no room");
    }

    #[test]
    fn a_post_that_is_not_a_log_line_is_left_alone() {
        assert!(log_record(&json!({ "tag": "lookup" })).is_none());
        assert!(log_record(&json!("text")).is_none());
        let record = log_record(&json!({ "message": "hi" })).expect("a message is enough");
        assert_eq!(record.level, LogLevel::Info);
    }

    #[test]
    fn sending_to_a_gone_parent_is_an_error_and_not_a_panic() {
        let (outbox, inbox) = mpsc::channel();
        let channel = Channel::new(PluginId::parse("com.example.word-filter").unwrap(), outbox);
        drop(inbox);
        assert!(
            channel
                .send(FromHelper::EventDone {
                    plugin: PluginId::parse("com.example.word-filter").unwrap(),
                })
                .is_err()
        );
    }
}
