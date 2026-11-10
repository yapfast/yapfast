//! Logs, without touching stdout.
//!
//! The helper's own lines go to stderr, which the parent reads into its log.
//! A plugin's lines are its own: [`GuestLogs`] takes the lines Extism's guest
//! logging turns into tracing events, files each under the plugin the runtime
//! stamps on it, keeps it in that plugin's ring and sends it to the parent as a
//! frame, so the Plugins page can show a plugin's recent lines without a log
//! file of its own. Nothing a guest says is written to stderr, where it would
//! be indistinguishable from the helper's own diagnostics.
//!
//! `YAPFAST_LOG` is the variable Yapfast itself reads, and the only one: the
//! helper started from the app inherits it, so one setting turns up the detail
//! on both sides of the pipe. `RUST_LOG` is deliberately not read, because a
//! guest's environment is cleared and a stray variable in a developer's shell
//! shouldn't change what a plugin sees.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::message::{FromHelper, LogLevel, LogRecord};

/// Lines kept for each plugin. A page shows the last few dozen; the parent's
/// own ring is the record that outlives this process.
pub const RING_LINES: usize = 100;

/// Installs the helper's subscriber: its own lines to standard error at
/// `YAPFAST_LOG`'s level, every plugin's lines to `reports` instead.
///
/// The helper's standard output is the pipe, so a line written there would
/// corrupt a frame. Dependencies stay at their configured level even when
/// `YAPFAST_LOG` asks for debug: wasmtime's and Extism's own traces are large
/// and say nothing about Yapfast.
pub fn init(reports: Sender<FromHelper>) {
    *REPORTS.lock().unwrap_or_else(PoisonError::into_inner) = Some(reports);
    // Several tests in one process may call this; the first subscriber stays.
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(filter()),
        )
        .with(GuestLogs)
        .try_init();
}

/// Lets go of the report channel.
///
/// The subscriber is global and outlives any pipe, so this is what a helper
/// calls on its way out: the pipe's relay stops when the last sender is gone,
/// and the exit would otherwise wait on a channel the subscriber still holds.
pub fn close() {
    REPORTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
}

/// Where guest log lines go while the helper runs.
static REPORTS: Mutex<Option<Sender<FromHelper>>> = Mutex::new(None);

/// The level this helper logs at.
fn filter() -> Targets {
    let level = match std::env::var("YAPFAST_LOG").as_deref() {
        Ok("trace") => LevelFilter::TRACE,
        Ok("debug") => LevelFilter::DEBUG,
        _ => LevelFilter::INFO,
    };
    Targets::new().with_target("yapfast_plugin_host", level)
}

/// Every plugin's recent log lines.
///
/// Cloned freely: the rings are shared, so the pipe that fills them and
/// whatever shows them read the same lines. Nothing here is on a plugin's call
/// path except [`LogBuffer::record`], which appends to a ring that was sized
/// when the plugin first logged.
#[derive(Clone, Default)]
pub struct LogBuffer {
    rings: Arc<Mutex<HashMap<PluginId, VecDeque<LogRecord>>>>,
}

impl LogBuffer {
    pub fn new() -> Self {
        LogBuffer::default()
    }

    /// Keeps one line, dropping the oldest once the ring is full.
    pub fn record(&self, plugin: &PluginId, record: LogRecord) {
        let mut rings = lock(&self.rings);
        let ring = rings
            .entry(plugin.clone())
            .or_insert_with(|| VecDeque::with_capacity(RING_LINES));
        if ring.len() == RING_LINES {
            ring.pop_front();
        }
        ring.push_back(record);
    }

    /// A plugin's lines, oldest first.
    pub fn lines(&self, plugin: &PluginId) -> Vec<LogRecord> {
        lock(&self.rings)
            .get(plugin)
            .map(|ring| ring.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Drops a plugin's lines, for when its module is unloaded and nothing can
    /// show them any more.
    pub fn forget(&self, plugin: &PluginId) {
        lock(&self.rings).remove(plugin);
    }

    /// Whether nothing at all has been logged yet.
    pub fn is_empty(&self) -> bool {
        lock(&self.rings).is_empty()
    }
}

/// Takes the rings' lock, ignoring poisoning: a panicking thread must not stop
/// the helper from recording why.
fn lock<T>(rings: &Mutex<T>) -> MutexGuard<'_, T> {
    rings
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Files a guest's log lines under the plugin that wrote them.
///
/// Extism's guest logging reaches only tracing, so this layer reads the
/// `plugin` field the runtime stamps on those events and sends the line to the
/// parent, which keeps the ring that outlives this process. An event without
/// that field is the helper's own and is left to the stderr layer: the two are
/// never mixed, because one of them belongs to a plugin's page and the other to
/// a developer's terminal.
pub struct GuestLogs;

impl<S> Layer<S> for GuestLogs
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let Some(plugin) = fields.plugin.and_then(|text| PluginId::parse(&text).ok()) else {
            return;
        };
        let record = LogRecord {
            level: level_of(*event.metadata().level()),
            target: event.metadata().target().to_owned(),
            message: fields.message.unwrap_or_default(),
            at: now_ms(),
        };
        // The pipe's relay keeps the ring as the line passes it, so there is
        // one owner of it rather than one here and one there.
        let reports = REPORTS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(reports) = reports.as_ref() {
            let _ = reports.send(FromHelper::Log { plugin, record });
        }
    }
}

/// The fields of one event that matter to a plugin's log: what it said, and
/// which plugin said it.
#[derive(Default)]
struct Fields {
    message: Option<String>,
    plugin: Option<String>,
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.message = Some(value.to_owned()),
            "plugin" => self.plugin = Some(value.to_owned()),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        // A message is written as `fmt::Arguments`, which records as debug and
        // prints as the text the guest meant.
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        }
    }
}

/// The level a plugin's line is kept at.
fn level_of(level: Level) -> LogLevel {
    match level {
        Level::ERROR => LogLevel::Error,
        Level::WARN => LogLevel::Warn,
        Level::INFO => LogLevel::Info,
        Level::DEBUG => LogLevel::Debug,
        Level::TRACE => LogLevel::Trace,
    }
}

/// Milliseconds since the Unix epoch, which is what a [`LogRecord`] carries.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_plugin_api::message::LogLevel;

    fn plugin() -> PluginId {
        PluginId::parse("com.example.word-filter").expect("a valid id")
    }

    fn line(message: &str) -> LogRecord {
        LogRecord {
            level: LogLevel::Info,
            target: "guest".into(),
            message: message.into(),
            at: 0,
        }
    }

    #[test]
    fn a_ring_keeps_the_newest_lines_and_drops_the_rest() {
        let logs = LogBuffer::new();
        for index in 0..RING_LINES + 5 {
            logs.record(&plugin(), line(&index.to_string()));
        }
        let lines = logs.lines(&plugin());
        assert_eq!(lines.len(), RING_LINES);
        assert_eq!(lines[0].message, "5");
        assert_eq!(lines[RING_LINES - 1].message, (RING_LINES + 4).to_string());
    }

    #[test]
    fn a_ring_belongs_to_one_plugin() {
        let logs = LogBuffer::new();
        logs.record(&plugin(), line("one"));
        assert_eq!(logs.lines(&plugin()).len(), 1);
        let other = PluginId::parse("com.example.other").expect("a valid id");
        assert!(logs.lines(&other).is_empty());
        logs.forget(&plugin());
        assert!(logs.lines(&plugin()).is_empty());
        assert!(logs.is_empty());
    }
}
