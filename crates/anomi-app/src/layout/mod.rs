//! `layout.json`, the window's settings, shared with the Slint app: the same
//! file in the same data folder, so a person's settings carry over both
//! ways. This reads the keys the iced interface uses so far and keeps every
//! other key as it found it. The full `Layout` (`src/ui.rs`) moves here with
//! I01.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

const FILE: &str = "layout.json";

pub struct Layout {
    path: PathBuf,
    values: Map<String, Value>,
}

impl Layout {
    /// The file in `root`, or an empty layout when there's none or it doesn't
    /// read.
    pub fn read(root: &Path) -> Self {
        let path = root.join(FILE);
        let values = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|value| match value {
                Value::Object(values) => Some(values),
                _ => None,
            })
            .unwrap_or_default();
        Self { path, values }
    }

    fn text(&self, key: &str) -> Option<&str> {
        self.values.get(key).and_then(Value::as_str)
    }

    /// The theme chosen, by id: a built-in one's or `custom:N`.
    pub fn theme(&self) -> Option<&str> {
        self.text("theme")
    }

    /// The language chosen, by code.
    pub fn language(&self) -> Option<&str> {
        self.text("language")
    }

    /// The sidebar's width, as its edge was last dragged.
    pub fn sidebar_width(&self) -> Option<f32> {
        self.values
            .get("sidebar_width")
            .and_then(Value::as_f64)
            .map(|width| width as f32)
    }

    /// Whether calls are joined with the microphone on, as Voice's switch or
    /// the account card's microphone outside a call sets it: on unless set.
    pub fn unmute_on_join(&self) -> bool {
        self.values
            .get("unmute_on_join")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    /// Whether calls are joined deafened: off unless set.
    pub fn deafen_on_join(&self) -> bool {
        self.values
            .get("deafen_on_join")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// Sets a key and writes the file.
    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.values.insert(key.to_owned(), value.into());
        self.write();
    }

    /// Writes the whole file anew beside it, then moves it over the old one,
    /// so a crash never leaves half a file.
    fn write(&self) {
        let written = serde_json::to_vec(&self.values)
            .map_err(std::io::Error::other)
            .and_then(|bytes| {
                if let Some(folder) = self.path.parent() {
                    std::fs::create_dir_all(folder)?;
                }
                let fresh = self.path.with_extension("json.new");
                std::fs::write(&fresh, bytes)?;
                std::fs::rename(&fresh, &self.path)
            });
        if let Err(error) = written {
            tracing::warn!(%error, "could not store the window layout");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_it_doesnt_know_survive_a_write() {
        let folder = std::env::temp_dir().join(format!("anomi-layout-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(FILE), r#"{"zoom":1.2,"theme":"nord"}"#).unwrap();
        let mut layout = Layout::read(&folder);
        assert_eq!(layout.theme(), Some("nord"));
        layout.set("language", "en");
        let again = Layout::read(&folder);
        assert_eq!(again.language(), Some("en"));
        assert_eq!(again.values.get("zoom").and_then(Value::as_f64), Some(1.2));
        std::fs::remove_dir_all(folder).unwrap();
    }
}
