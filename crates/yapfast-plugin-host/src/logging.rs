//! Logs, without touching stdout.
//!
//! The helper's own lines go to stderr, which the parent reads into its log.
//! A plugin's lines are its own: [`GuestLogs`] takes the lines Extism's guest
//! logging turns into tracing events, files each under the plugin the runtime
//! stamps on it and sends it to the parent as a frame, which keeps the ring, so the Plugins page can show a plugin's recent lines without a log
//! file of its own. Nothing a guest says is written to stderr, where it would
//! be indistinguishable from the helper's own diagnostics.
//!
//! `YAPFAST_LOG` is the variable Yapfast itself reads, and the only one: the
//! helper started from the app inherits it, so one setting turns up the detail
//! on both sides of the pipe. `RUST_LOG` is deliberately not read, because a
//! guest's environment is cleared and a stray variable in a developer's shell
//! shouldn't change what a plugin sees.

use std::fmt;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::message::{FromHelper, LogLevel, LogRecord};

/// The most of a line's text that is kept.
///
/// A frame may carry a mebibyte and a guest may write one per line, so a plugin
/// that logged in large print would pin the frame on its way to the parent and
/// the parent's ring: hundreds of megabytes for a page that shows a few dozen
/// short lines. The text is cut where each record is made, from a tracing event
/// here and from a `yf_post` in the runtime. The rest of the record travels as
/// it was.
pub const MAX_LINE_BYTES: usize = 4 << 10;

/// A line's text, cut to [`MAX_LINE_BYTES`] on a character boundary.
pub(crate) fn capped(mut message: String) -> String {
    const CUT: &str = "…";
    if message.len() <= MAX_LINE_BYTES {
        return message;
    }
    let mut end = MAX_LINE_BYTES - CUT.len();
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message.truncate(end);
    message.push_str(CUT);
    message
}

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
    //
    // Set as the default rather than with `try_init`, which would also install
    // a bridge from the `log` crate. Nothing here reads a `log` record: the
    // stderr layer shows this crate's lines and the guest layer Extism's, and
    // both are `tracing`. The bridge only raised `log`'s level to trace, so
    // every trace line wasmtime writes on a call was built and offered to both
    // filters to be refused; without it the macro stops at one load of a level.
    let subscriber = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(filter()),
        )
        .with(GuestLogs.with_filter(guest_targets()));
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// The targets a plugin's line can come from: Extism's guest logging, and the
/// helper's own lines that name a plugin.
///
/// Without a filter of its own this layer was interested in everything, which
/// turned every trace line wasmtime, Cranelift and Extism write on every call
/// into a formatted record that [`GuestLogs`] then threw away for having no
/// plugin: a third of the helper's time on a hook. Filtered, a callsite outside
/// these targets is never enabled, so its arguments are never formatted.
fn guest_targets() -> Targets {
    Targets::new()
        .with_target("yapfast_plugin_host", LevelFilter::TRACE)
        .with_target("extism::pdk", LevelFilter::TRACE)
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
            message: capped(fields.message.unwrap_or_default()),
            at: now_ms(),
        };
        // The parent keeps the ring; this only sends the line.
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

    /// A guest writes what it wants, so a line is cut before it is sent.
    #[test]
    fn a_long_line_is_cut_to_a_few_kilobytes() {
        // Two bytes a character, so the cut lands inside one and has to walk
        // back to a boundary: the text stays valid UTF-8.
        let kept = capped("é".repeat(MAX_LINE_BYTES));
        assert!(kept.len() <= MAX_LINE_BYTES, "{} bytes", kept.len());
        assert!(kept.ends_with('…'), "{kept}");
    }

    /// A line that already fits is the line the guest wrote.
    #[test]
    fn a_line_at_the_cap_is_kept_as_it_is() {
        let exact = "a".repeat(MAX_LINE_BYTES);
        assert_eq!(capped(exact.clone()), exact);
    }
}
