//! Log lines a person can read in Yapfast's plugin page.
//!
//! A plugin's WebAssembly has no stdout worth reading, so a line is a host call
//! like any other: it lands in the plugin's own ring buffer, the person can see
//! it next to the plugin, and nothing needs a terminal.
//!
//! Logging never fails a handler. A post with no answer can't be told apart
//! from one that was dropped, so these return nothing at all: treat a log line
//! as a hint, and storage as the place a value that matters goes.

use serde::Serialize;

use crate::call;

/// How important a line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Error,
    Warn,
    Info,
}

/// The method name a log line travels under, with the level and the text in
/// its payload.
const LOG: &str = "log";

#[derive(Serialize)]
struct Line<'a> {
    level: Level,
    message: &'a str,
}

/// Something went wrong.
pub fn error(message: impl AsRef<str>) {
    line(Level::Error, message.as_ref());
}

/// Something looks wrong but the plugin carried on.
pub fn warn(message: impl AsRef<str>) {
    line(Level::Warn, message.as_ref());
}

/// Something worth seeing, for a plugin whose person is debugging it.
pub fn info(message: impl AsRef<str>) {
    line(Level::Info, message.as_ref());
}

fn line(level: Level, message: &str) {
    let _ = call::post(LOG, Line { level, message });
}
