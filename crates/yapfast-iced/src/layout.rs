//! What the window keeps in the data folder's `layout.json`, the file the
//! Slint client keeps the same things in, so both clients share them: the
//! theme and the person's own themes (`theme.rs`), the rail's open folders,
//! each server's collapsed channel categories, whether the member list
//! is hidden, whether long messages fold, pinned DMs, and the people
//! settings the core is told of as the window opens ([`Layout::actions`]). Each change writes its one key back, leaving the rest of the
//! file as it was.

use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
};

use yapfast_core::theme::Custom;

/// The file, in the data folder.
pub const LAYOUT: &str = "layout.json";

/// What this client reads of `layout.json`.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
pub struct Layout {
    pub theme: Option<String>,
    pub custom_themes: Vec<Custom>,
    /// The rail's open folders, by folder id.
    pub open_folders: BTreeSet<String>,
    /// Each server's collapsed categories, by server id: a Discord category
    /// by its room id, any other by its heading.
    pub collapsed: HashMap<String, BTreeSet<String>>,
    pub members_hidden: bool,
    /// Copy ID shows in menus, as Discord's Developer Mode.
    pub developer_mode: bool,
    /// How calls are joined: unmuted unless turned off, and deafened.
    pub unmute_on_join: Option<bool>,
    pub deafen_on_join: bool,
    /// Long messages fold to twelve lines, with Show more.
    pub collapse_long: bool,
    /// DMs pinned to the top of the list, by room id, the first pinned
    /// first.
    pub pinned: Vec<String>,
    /// A one-to-one DM's profile beside it is hidden; messages from people
    /// blocked don't show at all; a call from someone's menu starts without
    /// asking; the member list's Activity shows its header alone.
    pub profile_panel_hidden: bool,
    pub hide_blocked: bool,
    pub skip_call_prompt: bool,
    pub activity_cards_hidden: bool,
}

impl Layout {
    /// What the core is told of these as the window opens, as the Slint
    /// client tells it.
    pub fn actions(&self) -> Vec<yapfast_core::application::Action> {
        use yapfast_core::application::Action;
        vec![
            Action::ShowProfilePanel(!self.profile_panel_hidden),
            Action::HideBlocked(self.hide_blocked),
            Action::SkipCallPrompt(self.skip_call_prompt),
            Action::ActivityCardsHidden(self.activity_cards_hidden),
            Action::SetPinned(self.pinned.clone()),
        ]
    }
}

impl Layout {
    /// Reads `layout.json`; a missing or unreadable file keeps nothing.
    pub fn read(root: &Path) -> Self {
        std::fs::read(root.join(LAYOUT))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
}

/// Keeps `value` under `key` in `layout.json`, leaving the rest as it is.
pub fn store(root: &Path, key: &str, value: impl serde::Serialize) {
    let path = root.join(LAYOUT);
    let mut layout = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    layout[key] = serde_json::to_value(value).expect("layout values serialize");
    let result = std::fs::create_dir_all(root).and_then(|()| {
        std::fs::write(
            &path,
            serde_json::to_vec(&layout).expect("layout serializes"),
        )
    });
    if let Err(error) = result {
        tracing::warn!(%error, key, "could not keep the window's layout");
    }
}
