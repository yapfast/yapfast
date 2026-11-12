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
//!
//! A call the parent will answer, and a timer the guest set, outlive the call
//! that made them: the guest returns and only the answer, or the clock, resumes
//! it. Both are reported to the scheduler from here, because this is the only
//! place that sees them — the request goes straight to the parent rather than
//! through the scheduler — and an instance waiting on one must not be evicted as
//! idle.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};

use extism::{CurrentPlugin, Error, Function, PTR, UserData, Val};
use serde::Deserialize;

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{HostCall, HostReturn, TimerCancel, TimerRequest, method};
use yapfast_plugin_api::limits;
use yapfast_plugin_api::message::{FromHelper, LogLevel, LogRecord};

use crate::backend::{Wait, Waits};

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
    /// Where a wait is reported, once the scheduler has asked to hear of them. A
    /// channel with none — a runtime built without a scheduler — costs only the
    /// accounting: nothing is evicted there.
    waits: Option<Waits>,
    /// The timers the guest asked for and hasn't cancelled, each with whether it
    /// repeats.
    ///
    /// It lives here rather than with an instance because a timer belongs to the
    /// parent and survives an eviction; it goes when the module does, or when the
    /// parent drops the plugin's timers.
    timers: Mutex<Vec<(String, bool)>>,
}

impl Channel {
    pub fn new(plugin: PluginId, outbox: mpsc::Sender<FromHelper>, waits: Option<Waits>) -> Self {
        Channel {
            plugin,
            outbox,
            next_call: AtomicU64::new(0),
            waits,
            timers: Mutex::new(Vec::new()),
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

    /// Tells the scheduler what a guest left this plugin waiting on.
    fn waited(&self, wait: Wait) {
        if let Some(sink) = &self.waits {
            sink(&self.plugin, wait);
        }
    }

    /// Sends a call the parent will answer, and answers the guest `pending`.
    ///
    /// The wait is reported before the call goes out: the parent's answer comes
    /// back through a pipe and a worker, but an account that heard of the call
    /// only after its answer had been filed would count a wait with no end, and
    /// the instance would never look idle again. A `send` that fails means the
    /// pipe to Yapfast is gone, and the hold dies with the helper.
    fn request(&self, call: HostCall) -> Result<HostReturn, Error> {
        let id = self.next_call();
        self.waited(Wait::Call(id));
        self.registered(&call);
        self.send(FromHelper::Call {
            plugin: self.plugin.clone(),
            call: id,
            request: call,
        })?;
        Ok(HostReturn::Pending {
            tag: id.to_string(),
        })
    }

    /// Follows what a call says about the timers the plugin holds.
    ///
    /// The parent keeps the timers and answers a registration in the same call,
    /// so this only follows what the guest asked for. A request the parent will
    /// refuse — a seventeenth timer, a payload that isn't the ABI's — is not
    /// recorded: a hold nothing ever ends would keep that instance alive for the
    /// rest of the helper's life.
    fn registered(&self, call: &HostCall) {
        match call.method.as_str() {
            method::TIMERS_AFTER | method::TIMERS_EVERY => {
                let Ok(request) = call.parse::<TimerRequest>() else {
                    return;
                };
                let repeating = call.method == method::TIMERS_EVERY;
                let mut timers = lock(&self.timers);
                if let Some(timer) = timers.iter_mut().find(|timer| timer.0 == request.tag) {
                    timer.1 = repeating;
                } else if timers.len() < limits::MAX_TIMERS {
                    timers.push((request.tag, repeating));
                } else {
                    return;
                }
                drop(timers);
                self.waited(Wait::Timer(true));
            }
            method::TIMERS_CANCEL => {
                let Ok(request) = call.parse::<TimerCancel>() else {
                    return;
                };
                let empty = {
                    let mut timers = lock(&self.timers);
                    let had = !timers.is_empty();
                    timers.retain(|timer| timer.0 != request.tag);
                    had && timers.is_empty()
                };
                if empty {
                    self.waited(Wait::Timer(false));
                }
            }
            _ => {}
        }
    }

    /// One timer went off, as far as the helper can tell.
    ///
    /// The parent re-arms a repeating timer and drops a one-shot, and sends the
    /// same empty wake-up for both. So the helper knows a one-shot is spent only
    /// when it holds no repeating timer to have been the one that fired; a
    /// plugin holding both keeps the hold, which is the safe side — a hold kept
    /// too long costs an eviction that never happens, and one let go too early
    /// drops an instance with a timer still pending.
    pub(super) fn ticked(&self) {
        let spent = {
            let mut timers = lock(&self.timers);
            if timers.iter().any(|timer| timer.1) {
                return;
            }
            let Some(at) = timers.iter().position(|timer| !timer.1) else {
                return;
            };
            timers.remove(at);
            timers.is_empty()
        };
        if spent {
            self.waited(Wait::Timer(false));
        }
    }

    /// Lets go of every timer, because the parent dropped them.
    ///
    /// A deactivation is preceded by the parent forgetting a plugin's timers, so
    /// this is where they go: an instance that isn't running can't be told, and
    /// a hold on timers that are gone would keep it off the eviction list for
    /// ever.
    pub fn forget_timers(&self) {
        let had = {
            let mut timers = lock(&self.timers);
            let had = !timers.is_empty();
            timers.clear();
            had
        };
        if had {
            self.waited(Wait::Timer(false));
        }
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
///
/// Every name `call::method` has belongs here: a method left out is one no
/// guest can reach, and a test compares the two lists to keep that from
/// happening.
const FORWARDED: [&str; 56] = [
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
    method::PEOPLE_MEMBER,
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
    // What the interface draws for a plugin, and the media it plays. The parent
    // owns both, so it is the one that knows whether this build has a place for
    // the slot or a decoder for the bytes, and its answer is the one a plugin
    // can act on.
    method::UI_SET_SLOT,
    method::UI_CLEAR_SLOT,
    method::UI_NOTICE,
    method::UI_TOAST,
    method::UI_OPEN_DIALOG,
    method::MEDIA_GET,
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
    // The curated Discord actions, and audio. Each is the parent's to check
    // against a capability, the person's own permissions and its rate limiter,
    // so a guest asking for one is asking to be told no in words.
    method::DISCORD_SET_STATUS,
    method::DISCORD_SET_CUSTOM_STATUS,
    method::DISCORD_SET_ACTIVITY,
    method::DISCORD_CLEAR_ACTIVITY,
    method::DISCORD_MARK_READ,
    method::DISCORD_SET_MUTE,
    method::DISCORD_OPEN_ROOM,
    method::DISCORD_START_REPLY,
    method::DISCORD_START_EDIT,
    method::DISCORD_REACTORS,
    method::DISCORD_PAUSE_INVITES,
    method::DISCORD_ADD_EMOJI,
    method::DISCORD_ADD_STICKER,
    method::AUDIO_PLAY,
    method::AUDIO_SPEAK,
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
        channel.request(call)?
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
    posted(&channel, post)
}

/// Where a post goes: into a log record, to the parent, or, for a name this
/// build doesn't forward, nowhere.
///
/// A post is never answered, so a name this build doesn't know can't be told
/// `unsupported` the way a call is told it. Sending it would only make the
/// parent answer a call nobody waits for, so the same table that decides what
/// `yf_call` sends decides this too, and a post outside it is dropped without a
/// word — the guest learns the name is missing by calling it.
///
/// It is a function of its own because this is the part worth a test, and
/// nothing reaches it without a guest: no fixture posts.
fn posted(channel: &Channel, post: HostCall) -> Result<(), Error> {
    if post.method == LOG_METHOD
        && let Some(record) = log_record(&post.payload)
    {
        return channel.send(FromHelper::Log {
            plugin: channel.plugin.clone(),
            record,
        });
    }
    if !forwards(&post.method) {
        return Ok(());
    }
    // Fire and forget: the guest is given no answer, but the post still takes a
    // number so a parent's log can name it.
    let id = channel.next_call();
    // A timer asked for with a post is still a timer the parent keeps, and the
    // hold it takes is the same one a call takes.
    channel.registered(&post);
    channel.send(FromHelper::Call {
        plugin: channel.plugin.clone(),
        call: id,
        request: post,
    })
}

/// One post that reads as a log line, when its payload has that shape.
///
/// A plugin that uses `log` as a name for something else is left with its own
/// meaning rather than turned into a log line. Its post then falls to the same
/// filter as every other name, which drops it if this build doesn't forward it,
/// because a post has no answer to say why.
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

/// Takes a lock, ignoring poisoning: a guest's bookkeeping is worth less than
/// the helper carrying on.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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

    /// A method the api names and this table doesn't has no path to the parent:
    /// the guest is answered `unsupported` however ready the app is, which is
    /// the bug this test exists to keep from coming back.
    #[test]
    fn every_method_this_build_names_is_forwarded() {
        for (index, name) in method::ALL.iter().enumerate() {
            assert!(
                !method::ALL[..index].contains(name),
                "`{name}` is named twice in `call::method`"
            );
        }
        for name in method::ALL {
            assert!(
                forwards(name),
                "`{name}` is a method a guest may call, so it has to travel to the parent"
            );
        }
        for name in FORWARDED {
            assert!(
                method::ALL.contains(&name),
                "`{name}` is forwarded but is not a name `call::method` has"
            );
        }
        assert_eq!(
            FORWARDED.len(),
            method::ALL.len(),
            "the table forwards one of them twice"
        );
    }

    #[test]
    fn a_post_of_a_name_this_build_does_not_forward_goes_nowhere() {
        let (outbox, inbox) = mpsc::channel();
        let channel = Channel::new(
            PluginId::parse("com.example.word-filter").unwrap(),
            outbox,
            None,
        );
        // A typo, or a method from a newer build: a post has no answer to carry
        // `unsupported`, so the parent is spared one nobody is waiting for.
        posted(&channel, HostCall::new("messages.sent", json!({})).unwrap()).unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "a name nothing can answer is not sent"
        );
        posted(
            &channel,
            HostCall::new(method::UI_NOTICE, json!({ "text": "hi" })).unwrap(),
        )
        .unwrap();
        assert!(
            matches!(inbox.try_recv(), Ok(FromHelper::Call { .. })),
            "a method the table has still travels"
        );
        posted(
            &channel,
            HostCall::new(LOG_METHOD, json!({ "message": "hi" })).unwrap(),
        )
        .unwrap();
        assert!(
            matches!(inbox.try_recv(), Ok(FromHelper::Log { .. })),
            "a log line is a record rather than a call"
        );
    }

    #[test]
    fn ids_are_never_zero_and_never_repeat() {
        let (outbox, _inbox) = mpsc::channel();
        let channel = Channel::new(
            PluginId::parse("com.example.word-filter").unwrap(),
            outbox,
            None,
        );
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
        let channel = Channel::new(
            PluginId::parse("com.example.word-filter").unwrap(),
            outbox,
            None,
        );
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
