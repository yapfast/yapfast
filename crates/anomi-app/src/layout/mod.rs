//! `layout.json`, the window's settings, shared with the Slint app: the same
//! file in the same data folder, in the same format ([`Saved`]), so a
//! person's settings carry over both ways. Opening it writes nothing; every
//! key either app wrote survives a write, those this interface doesn't know
//! included.
//!
//! One that isn't a layout (cut short, or not JSON) is set aside as
//! [`SET_ASIDE`] rather than lost under the next write.
//!
//! A change goes through [`Layout::edit`] and reaches the file a second
//! after the last of a run of changes ([`WRITE_AFTER`], `write_layout_soon`
//! in `src/ui.rs:3323`): `App::update` asks [`Layout::wait`] after each
//! message and hands [`Layout::due`] the wait that ended; quitting flushes.
//! The window's thread only makes the file's bytes: a thread of its own
//! writes them, one write after another in the order made, and
//! [`Layout::flush`] waits for it.
//! Reading goes through `Deref`: `app.layout.members_hidden`.
//!
//! [`carry`] hands the core what it keeps of the layout (pinned DMs, the
//! last channel of each server, the switches), and keeps what the core
//! changed of it.

pub mod carry;
mod saved;
#[cfg(test)]
mod tests;
mod writer;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize as _;
use serde_json::{Map, Value};
use yapfast_core::layout_file::LAYOUT as FILE;

pub use saved::{Animate, InputMode, KeptSticker, KeptStickerUse, Saved};

/// How long a change waits for the next before the file is written.
pub const WRITE_AFTER: Duration = Duration::from_secs(1);

/// Where a `layout.json` that isn't one is kept, rather than lost under the
/// first change written.
pub const SET_ASIDE: &str = "layout.json.unread";

pub struct Layout {
    path: PathBuf,
    saved: Saved,
    /// Keys whose value doesn't read as [`Saved`] types it, kept as they came
    /// and written back while their setting stays unset here: a value one
    /// app writes in a shape the other doesn't know loses nothing else.
    odd: Map<String, Value>,
    /// Changes made, the number of the last whose wait started, and of the
    /// last written.
    edits: u64,
    waiting: u64,
    written: u64,
    /// The core's `selection_version` when [`carry::follow`] last looked.
    pub(crate) selection: u64,
    /// The core's `people_changes` when [`carry::follow`] last looked.
    pub(crate) people: u64,
}

impl std::ops::Deref for Layout {
    type Target = Saved;

    fn deref(&self) -> &Saved {
        &self.saved
    }
}

impl Layout {
    /// The file in `root`, or an empty layout when there's none or it isn't
    /// a layout, which is then set aside as [`SET_ASIDE`].
    pub fn read(root: &Path) -> Self {
        let path = root.join(FILE);
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::from_values(path, Map::new());
        };
        let values = match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Object(values)) => values,
            _ => {
                tracing::warn!("layout.json isn't a layout; keeping it as {SET_ASIDE}");
                if let Err(error) = std::fs::rename(&path, root.join(SET_ASIDE)) {
                    tracing::warn!(%error, "could not set layout.json aside");
                }
                Map::new()
            }
        };
        Self::from_values(path, values)
    }

    fn from_values(path: PathBuf, values: Map<String, Value>) -> Self {
        let (mut saved, odd) = parse(values);
        saved.settle();
        Self {
            path,
            saved,
            odd,
            edits: 0,
            waiting: 0,
            written: 0,
            selection: 0,
            people: 0,
        }
    }

    /// Changes what's saved; the file follows a moment later.
    pub fn edit(&mut self, change: impl FnOnce(&mut Saved)) {
        change(&mut self.saved);
        self.edits += 1;
    }

    /// Whether a change came since the last wait started: then a new wait,
    /// by its number, which [`Self::due`] takes once it ends. Each change
    /// starts the wait over.
    pub fn wait(&mut self) -> Option<u64> {
        (self.waiting != self.edits).then(|| {
            self.waiting = self.edits;
            self.edits
        })
    }

    /// A wait ended: the file is written if no change came after it.
    pub fn due(&mut self, wait: u64) {
        if wait == self.edits {
            self.store();
        }
    }

    /// Hands a change still waiting to the writer, at once: as the window
    /// closes to the tray.
    pub fn store(&mut self) {
        if self.written != self.edits {
            self.written = self.edits;
            self.write();
        }
    }

    /// Writes a change still waiting, and waits until every write handed
    /// over is in the file: before quitting.
    pub fn flush(&mut self) {
        self.store();
        writer::settle();
    }

    /// The theme chosen, by id: a built-in one's or `custom:N`.
    pub fn theme(&self) -> Option<&str> {
        self.saved.theme.as_deref()
    }

    /// The language chosen, by code.
    pub fn language(&self) -> Option<&str> {
        self.saved.language.as_deref()
    }

    /// Whether calls are joined with the microphone on, as Voice's switch or
    /// the account card's microphone outside a call sets it: on unless set.
    pub fn unmute_on_join(&self) -> bool {
        self.saved.unmute_on_join.unwrap_or(true)
    }

    /// Whether calls are joined deafened: off unless set.
    pub fn deafen_on_join(&self) -> bool {
        self.saved.deafen_on_join
    }

    /// The file's bytes: the saved settings, with each odd key as it came
    /// while its setting is still unset here.
    fn bytes(&self) -> serde_json::Result<Vec<u8>> {
        if self.odd.is_empty() {
            return serde_json::to_vec(&self.saved);
        }
        let Value::Object(mut values) = serde_json::to_value(&self.saved)? else {
            return serde_json::to_vec(&self.saved);
        };
        let unset = serde_json::to_value(Saved::default())?;
        for (key, value) in &self.odd {
            if values.get(key) == unset.get(key) {
                values.insert(key.clone(), value.clone());
            }
        }
        serde_json::to_vec(&values)
    }

    /// Hands the writer the whole file, which it writes anew beside the old
    /// one, then moves over it, so a crash never leaves half a file: as the
    /// Slint app does, through the core's `layout_file`.
    fn write(&self) {
        let folder = self.path.parent().unwrap_or(Path::new("."));
        match self.bytes() {
            Ok(bytes) => writer::write(folder.to_owned(), bytes),
            Err(error) => tracing::warn!(%error, "could not store the window layout"),
        }
    }
}

/// The settings a file holds, and the keys among them whose values don't
/// read as their setting's type, each left out of the settings.
fn parse(values: Map<String, Value>) -> (Saved, Map<String, Value>) {
    let whole = Value::Object(values);
    if let Ok(saved) = Saved::deserialize(&whole) {
        return (saved, Map::new());
    }
    let Value::Object(values) = whole else {
        return Default::default();
    };
    let (mut fine, mut odd) = (Map::new(), Map::new());
    for (key, value) in values {
        let alone = Value::Object(Map::from_iter([(key, value)]));
        let fits = Saved::deserialize(&alone).is_ok();
        let Value::Object(alone) = alone else {
            continue;
        };
        for (key, value) in alone {
            if !fits {
                tracing::warn!(key, "a layout.json setting doesn't read; kept as it is");
            }
            if fits { &mut fine } else { &mut odd }.insert(key, value);
        }
    }
    let saved = Saved::deserialize(&Value::Object(fine)).unwrap_or_default();
    (saved, odd)
}
