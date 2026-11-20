//! What the plugin devtools show: each plugin's host calls and what it was
//! handed, with their timing and size, and the processor time its code took.
//!
//! Nothing is kept while the devtools are off, which they are until a person
//! turns them on in Settings → Plugins → Advanced: every hook here starts by
//! reading one flag ([`on`]), so a session that never opens them pays a
//! relaxed atomic load per call. Once on, each plugin keeps its last
//! [`CALLS`] records, and turning them off forgets everything.
//!
//! The recorder is one per process, as the plugin helper is: the bridge's
//! session writes it from the plugin runtime's thread and the Plugins page
//! reads a copy of one plugin's records ([`snapshot`]).

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// How many records one plugin keeps, the oldest dropped first.
pub const CALLS: usize = 200;

/// A host call waits this long for its answer before its record is dropped
/// unanswered: a call the application never answers mustn't hold memory.
const STALE: Duration = Duration::from_secs(60);

/// Which way a record went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    /// The plugin asked Yapfast for something: a host call.
    Call,
    /// Yapfast handed the plugin something: events, a click, a question.
    Handed,
}

/// One host call or one thing handed to a plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub way: Way,
    /// When it started, in milliseconds since the Unix epoch.
    pub at_ms: u64,
    /// The method called, or what was handed (`event`, `input`, `decorate`).
    pub what: String,
    /// The request's size in bytes, and the answer's once it came.
    pub sent: usize,
    pub answered: Option<usize>,
    /// How long the answer took, once it came.
    pub took: Option<Duration>,
    /// `ok`, `denied`, `error` or `pending`, for a call; empty otherwise.
    pub outcome: &'static str,
}

/// What one plugin's devtools show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plugin {
    /// The oldest first.
    pub records: VecDeque<Record>,
    /// The processor time its code took in the helper, as the helper last
    /// reported it, and how many host calls it made since the devtools
    /// opened.
    pub cpu: Option<Duration>,
    pub calls: u64,
}

#[derive(Default)]
struct Inner {
    /// Host calls waiting for their answer, by plugin and the guest's call
    /// number, with the place their record holds.
    waiting: HashMap<(String, u64), (Instant, u64)>,
    plugins: BTreeMap<String, Plugin>,
    /// Numbers each record so an answer finds its call after older records
    /// were dropped.
    serial: u64,
    serials: BTreeMap<String, VecDeque<u64>>,
}

static ON: AtomicBool = AtomicBool::new(false);
static INNER: Mutex<Option<Inner>> = Mutex::new(None);

fn inner() -> std::sync::MutexGuard<'static, Option<Inner>> {
    INNER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether the devtools record anything.
pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Turns recording on or off; off forgets what was kept.
pub fn set_on(on: bool) {
    ON.store(on, Ordering::Relaxed);
    let mut inner = inner();
    *inner = on.then(Inner::default);
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

impl Inner {
    fn push(&mut self, plugin: &str, record: Record) -> u64 {
        self.serial += 1;
        let serial = self.serial;
        let kept = self.plugins.entry(plugin.to_owned()).or_default();
        let serials = self.serials.entry(plugin.to_owned()).or_default();
        if kept.records.len() == CALLS {
            kept.records.pop_front();
            serials.pop_front();
        }
        if record.way == Way::Call {
            kept.calls += 1;
        }
        kept.records.push_back(record);
        serials.push_back(serial);
        serial
    }
}

/// A plugin made a host call, `call` by its own number, of `sent` bytes.
pub fn called(plugin: &str, call: u64, method: &str, sent: usize) {
    if !on() {
        return;
    }
    let mut inner = inner();
    let Some(inner) = inner.as_mut() else {
        return;
    };
    let serial = inner.push(
        plugin,
        Record {
            way: Way::Call,
            at_ms: now_ms(),
            what: method.to_owned(),
            sent,
            answered: None,
            took: None,
            outcome: "pending",
        },
    );
    let now = Instant::now();
    inner
        .waiting
        .retain(|_, (started, _)| now.duration_since(*started) < STALE);
    inner
        .waiting
        .insert((plugin.to_owned(), call), (now, serial));
}

/// The word a call's answer is recorded under.
pub fn outcome(answer: &yapfast_plugin_api::call::HostReturn) -> &'static str {
    use yapfast_plugin_api::call::HostReturn;
    match answer {
        HostReturn::Ok(_) => "ok",
        HostReturn::Unsupported => "unsupported",
        HostReturn::Denied { .. } => "denied",
        HostReturn::Pending { .. } => "pending",
        HostReturn::Error { .. } => "error",
    }
}

/// What Yapfast hands a plugin, by the message's kind, and the plugins it
/// goes to: a hook chain's are several.
pub fn handed_to(
    message: &yapfast_plugin_api::message::ToHelper,
) -> Option<(&'static str, Vec<&str>)> {
    use yapfast_plugin_api::message::ToHelper;
    fn one<'a>(
        what: &'static str,
        plugin: &'a yapfast_plugin_api::PluginId,
    ) -> Option<(&'static str, Vec<&'a str>)> {
        Some((what, vec![plugin.as_str()]))
    }
    match message {
        ToHelper::Event { plugin, .. } => one("event", plugin),
        ToHelper::Input { plugin, .. } => one("input", plugin),
        ToHelper::Command { plugin, .. } => one("command", plugin),
        ToHelper::Invoke { plugin, .. } => one("plugin.call", plugin),
        ToHelper::Decorate { plugin, .. } => one("decorate", plugin),
        ToHelper::Identity { plugin, .. } => one("identity", plugin),
        ToHelper::CodeBlock { plugin, .. } => one("code block", plugin),
        ToHelper::DecorateMember { plugin, .. } => one("member badge", plugin),
        ToHelper::Activate { plugin, .. } => one("activate", plugin),
        ToHelper::Hook { chain, .. } => {
            Some(("hook", chain.iter().map(|id| id.as_str()).collect()))
        }
        _ => None,
    }
}

/// A plugin's host call was answered with `bytes`, as `outcome`.
pub fn answered(plugin: &str, call: u64, bytes: usize, outcome: &'static str) {
    if !on() {
        return;
    }
    let mut inner = inner();
    let Some(inner) = inner.as_mut() else {
        return;
    };
    let Some((started, serial)) = inner.waiting.remove(&(plugin.to_owned(), call)) else {
        return;
    };
    let place = inner
        .serials
        .get(plugin)
        .and_then(|serials| serials.iter().position(|&kept| kept == serial));
    let Some(record) = place.and_then(|place| {
        inner
            .plugins
            .get_mut(plugin)
            .and_then(|kept| kept.records.get_mut(place))
    }) else {
        return;
    };
    record.took = Some(started.elapsed());
    record.answered = Some(bytes);
    record.outcome = outcome;
}

/// Yapfast handed a plugin `what`, `sent` bytes of it.
pub fn handed(plugin: &str, what: &str, sent: usize) {
    if !on() {
        return;
    }
    let mut inner = inner();
    let Some(inner) = inner.as_mut() else {
        return;
    };
    inner.push(
        plugin,
        Record {
            way: Way::Handed,
            at_ms: now_ms(),
            what: what.to_owned(),
            sent,
            answered: None,
            took: None,
            outcome: "",
        },
    );
}

/// The processor time a plugin's code has taken in the helper, as it says.
pub fn used(plugin: &str, cpu: Duration) {
    if !on() {
        return;
    }
    let mut inner = inner();
    if let Some(inner) = inner.as_mut() {
        inner.plugins.entry(plugin.to_owned()).or_default().cpu = Some(cpu);
    }
}

/// A copy of what is kept for one plugin, while the devtools are on.
pub fn snapshot(plugin: &str) -> Option<Plugin> {
    if !on() {
        return None;
    }
    let inner = inner();
    Some(
        inner
            .as_ref()?
            .plugins
            .get(plugin)
            .cloned()
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Off, nothing is kept; on, a call and its answer are one record with
    /// its timing and sizes, and the ring keeps the last [`CALLS`].
    #[test]
    fn calls_are_recorded_only_while_on() {
        // The recorder is the process's: this test is the only one that
        // turns it on, and it leaves it off.
        set_on(false);
        called("a.dev", 1, "storage.get", 10);
        assert!(snapshot("a.dev").is_none(), "nothing while off");
        set_on(true);
        called("a.dev", 1, "storage.get", 10);
        handed("a.dev", "event", 40);
        answered("a.dev", 1, 7, "ok");
        let kept = snapshot("a.dev").expect("on");
        assert_eq!(kept.records.len(), 2);
        assert_eq!(kept.calls, 1);
        let call = &kept.records[0];
        assert_eq!(call.what, "storage.get");
        assert_eq!((call.sent, call.answered), (10, Some(7)));
        assert_eq!(call.outcome, "ok");
        assert!(call.took.is_some());
        assert_eq!(kept.records[1].way, Way::Handed);
        for number in 0..(CALLS as u64 + 10) {
            called("a.dev", 100 + number, "ui.toast", 1);
        }
        let kept = snapshot("a.dev").expect("on");
        assert_eq!(kept.records.len(), CALLS, "the ring is capped");
        used("a.dev", Duration::from_millis(5));
        assert_eq!(
            snapshot("a.dev").unwrap().cpu,
            Some(Duration::from_millis(5))
        );
        set_on(false);
        assert!(snapshot("a.dev").is_none(), "off forgets");
    }
}
