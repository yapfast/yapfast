//! Plugins. The manifest side, as far as Yapfast knows them without running
//! any: each one's manifest, what its `ui` section changes in the interface
//! ([`ui`]), its own settings ([`settings`]), and what the Plugins page in
//! settings shows ([`page`]). The points a manifest's `tweaks` set are in
//! `crate::tweaks`. None of those start a process or load WebAssembly; a
//! plugin that only changes the interface needs neither. See
//! `docs/plugins/interface.md` and `docs/plugins/settings.md`.
//!
//! The runtime side: a bridge to the helper process, the registry of what is
//! enabled, permissions, and the network gate. Nothing there runs while no
//! plugin is enabled: [`state::PluginState`] is `None` in the application's
//! state, and every tap point in the send path, the event path and the menus
//! checks that before it looks at anything else.
//!
//! The parent holds the Discord token, the network stack and the consent
//! dialogs; the helper holds only WebAssembly and its scheduler. A guest's host
//! call therefore crosses the pipe to be answered here.
//!
//! ```text
//! application  ──Action::Plugin──▶  PluginState  ──frames──▶  helper
//!      ▲                                │                        │
//!      └──────Effect::Plugin────────────┘◀───────frames───────────┘
//! ```

pub mod audio;
pub mod bridge;
pub mod code_block;
pub mod fetch;
pub mod grants;
pub mod identity;
pub mod installing;
pub mod marketplace;
pub mod media;
pub mod network;
pub mod page;
pub mod permissions;
pub mod presence;
pub mod registry;
pub mod retention;
pub mod settings;
pub mod slots;
pub mod spawn;
pub mod state;
pub mod store;
pub mod transport;
pub mod ui;
pub mod view;
pub mod world;

pub use state::PluginState;

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The plugin API version this Yapfast speaks; a manifest names it as `api`.
pub const API: u32 = 1;

/// What a plugin's folder names its manifest.
pub const MANIFEST: &str = "yapfast-plugin.json";

/// The variable naming a folder of plugins to load as they are, one
/// subfolder each, for writing and trying a plugin.
pub const FOLDER_VARIABLE: &str = "YAPFAST_PLUGINS_DIR";

/// Started with `--no-plugins`, the way back from an interface a plugin
/// made unusable: no plugin's `ui` section applies, and which plugins are
/// enabled stays. [`crate::cli::parse_no_plugins`] reads the flag.
pub fn safe_mode() -> bool {
    !crate::cli::plugins_allowed()
}

/// What the person did with something a plugin put in the interface, for
/// the plugin runtime to deliver to that plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// Chose the plugin's menu row `action`, in the menu of `target`: a
    /// person's, conversation's or server's id, as the menu names it.
    Menu {
        plugin: String,
        action: String,
        target: String,
    },
    /// Clicked or changed the widget `id` the plugin drew in `slot`, with
    /// its value: a toggle's `true` or `false`, a select's option, an
    /// input's text, a slider's number, a link's own value, and nothing for
    /// a button. `subject` is what the widget is drawn on: the message, for
    /// one under a message or on its hover bar; the open conversation, for
    /// the composer's; and nothing elsewhere.
    Widget {
        plugin: String,
        slot: slots::Slot,
        id: String,
        value: String,
        subject: String,
    },
    /// Closed the plugin's notice or toast, which leaves the interface.
    Dismissed { plugin: String, slot: slots::Slot },
}

/// The plugin runtime, as the interface reaches it. The runtime implements
/// it; until one is attached, input goes nowhere.
pub trait Runtime {
    fn input(&self, input: Input);
}

/// The parts of a plugin's manifest the interface reads. The file is strict
/// JSON; fields this struct doesn't name belong to the runtime and are
/// skipped here, but the `ui` section, its menu rows and each setting take
/// no field they don't know.
#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub api: u32,
    /// Reverse-DNS, stable forever, as `com.example.word-filter`.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    /// The module, if the plugin runs code; a plugin without one only changes
    /// what its manifest names.
    #[serde(default)]
    pub wasm: Option<String>,
    #[serde(default)]
    pub permissions: Permissions,
    /// Rows the plugin adds to menus.
    #[serde(default)]
    pub menus: Vec<ui::MenuItem>,
    /// Buttons it adds to the bottom of the server rail, from its manifest, so
    /// they show without it running.
    #[serde(default)]
    pub rail_items: Vec<ui::Button>,
    /// Buttons it adds to a conversation's header, the same way.
    #[serde(default)]
    pub header_buttons: Vec<ui::Button>,
    /// Changes to Yapfast's own interface.
    #[serde(default)]
    pub ui: Option<ui::Ui>,
    /// The plugin's own settings, checked as the manifest is read: one that
    /// can't be used refuses the manifest.
    #[serde(default)]
    pub settings: std::sync::Arc<settings::Schema>,
    /// Setting points it sets, by name; see `crate::tweaks`.
    #[serde(default)]
    pub tweaks: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub network: Vec<String>,
}

impl Manifest {
    /// A manifest from its file's text, refused with the reason when it
    /// isn't one this Yapfast can use.
    pub fn parse(text: &str) -> Result<Self, String> {
        let manifest: Self = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if manifest.api != API {
            return Err(format!(
                "it needs plugin API {}, and this Yapfast has {API}",
                manifest.api
            ));
        }
        let id_ok = manifest.id.len() <= 128
            && manifest.id.contains('.')
            && manifest.id.split('.').all(|part| {
                !part.is_empty()
                    && part.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || byte == b'-'
                            || byte == b'_'
                    })
            });
        if !id_ok {
            return Err(format!(
                "its id, {:?}, isn't reverse-DNS such as com.example.name",
                manifest.id
            ));
        }
        if manifest.name.trim().is_empty() || manifest.name.chars().count() > ui::LABEL_CHARS {
            return Err(format!(
                "its name must have 1 to {} characters",
                ui::LABEL_CHARS
            ));
        }
        Ok(manifest)
    }

    pub fn has(&self, capability: &str) -> bool {
        self.permissions
            .capabilities
            .iter()
            .any(|declared| declared == capability)
    }
}

/// A plugin found on disk, with its manifest read.
#[derive(Clone, Debug)]
pub struct Found {
    pub folder: PathBuf,
    pub manifest: Manifest,
}

/// The plugins in a folder: each subfolder holding a manifest, in the order
/// of their names, which is the order their changes win in. A folder whose
/// manifest can't be used is skipped and logged, as is a second plugin with
/// an id already found.
pub fn load_folder(folder: &Path) -> Vec<Found> {
    let mut folders: Vec<PathBuf> = match std::fs::read_dir(folder) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.join(MANIFEST).is_file())
            .collect(),
        Err(error) => {
            tracing::warn!(folder = %folder.display(), %error, "could not read the plugins folder");
            return Vec::new();
        }
    };
    folders.sort();
    let mut found: Vec<Found> = Vec::new();
    for folder in folders {
        let path = folder.join(MANIFEST);
        let read = std::fs::read_to_string(&path).map_err(|error| error.to_string());
        match read.and_then(|text| Manifest::parse(&text)) {
            Ok(manifest) if found.iter().any(|other| other.manifest.id == manifest.id) => {
                tracing::warn!(plugin = %manifest.id, folder = %folder.display(), "skipped a second plugin with this id");
            }
            Ok(manifest) => found.push(Found { folder, manifest }),
            Err(reason) => {
                tracing::warn!(manifest = %path.display(), reason, "skipped a plugin");
            }
        }
    }
    found
}

/// What a plugin's folder shows as it loads, from its `slots.json`: an
/// object from a slot's id to a widget tree, or from `message:` and a
/// message's id to what the plugin draws under that message. A stand-in for
/// the runtime's `ui.set_slot` and decorate replies while there is none,
/// for trying the widgets; a file that can't be used is logged and shows
/// nothing.
pub fn preview(found: &Found) -> Vec<slots::Output> {
    let path = found.folder.join("slots.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let trees: std::collections::BTreeMap<String, serde_json::Value> =
        match serde_json::from_str(&text) {
            Ok(trees) => trees,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "skipped a plugin's slots");
                return Vec::new();
            }
        };
    let plugin = &found.manifest.id;
    trees
        .into_iter()
        .filter_map(|(key, tree)| {
            let tree = serde_json::from_value::<ui::Tree>(tree)
                .map_err(|error| error.to_string())
                .and_then(ui::Tree::checked);
            let output = tree.and_then(|tree| match key.strip_prefix("message:") {
                Some(message) => Ok(slots::Output::Decorate {
                    hide: slots::Hidden::default(),
                    plugin: plugin.clone(),
                    message: message.to_owned(),
                    tree: Some(tree),
                }),
                None => match slots::Slot::from_id(&key) {
                    Some(slot) if slot != slots::Slot::Message => Ok(slots::Output::SetSlot {
                        plugin: plugin.clone(),
                        slot,
                        tree,
                    }),
                    _ => Err(format!("there's no slot {key:?}")),
                },
            });
            output
                .inspect_err(|reason| {
                    tracing::warn!(path = %path.display(), reason, "skipped a plugin's slot");
                })
                .ok()
        })
        .collect()
}

/// The plugins in the folder [`FOLDER_VARIABLE`] names: none when it names
/// none, or in safe mode.
pub fn load_from_environment() -> Vec<Found> {
    if safe_mode() {
        return Vec::new();
    }
    match std::env::var_os(FOLDER_VARIABLE) {
        Some(folder) if !folder.is_empty() => load_folder(Path::new(&folder)),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_is_strict_json_with_the_api_this_yapfast_speaks() {
        let good = r#"{ "api": 1, "id": "com.example.quiet", "name": "Quiet", "activation": {} }"#;
        let manifest = Manifest::parse(good).unwrap();
        assert_eq!(manifest.name, "Quiet");
        assert!(manifest.ui.is_none());
        // Comments and trailing commas are refused, as JSON refuses them.
        assert!(Manifest::parse(r#"{ "api": 1, "id": "a.b", "name": "x", }"#).is_err());
        assert!(Manifest::parse("{ // no\n \"api\": 1 }").is_err());
        assert!(
            Manifest::parse(r#"{ "api": 2, "id": "a.b", "name": "x" }"#)
                .unwrap_err()
                .contains("API 2")
        );
        assert!(Manifest::parse(r#"{ "api": 1, "id": "Quiet", "name": "x" }"#).is_err());
        assert!(Manifest::parse(r#"{ "api": 1, "id": "a.b", "name": " " }"#).is_err());
    }

    #[test]
    fn the_example_plugins_apply_whole() {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/plugins");
        let found = load_folder(&folder);
        assert!(!found.is_empty());
        let mut previews = 0;
        for plugin in found {
            let checked = ui::check(&plugin.manifest);
            assert!(checked.refused.is_empty(), "{:?}", checked.refused);
            let slots = plugin.folder.join("slots.json");
            if slots.exists() {
                let keys = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(
                    &std::fs::read_to_string(slots).unwrap(),
                )
                .unwrap()
                .len();
                assert_eq!(preview(&plugin).len(), keys, "every slot shows");
                previews += keys;
            }
        }
        assert!(previews > 0);
    }

    #[test]
    fn a_folder_loads_each_plugin_once_in_name_order() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/test-plugins-folder");
        let _ = std::fs::remove_dir_all(&root);
        let write = |folder: &str, text: &str| {
            std::fs::create_dir_all(root.join(folder)).unwrap();
            std::fs::write(root.join(folder).join(MANIFEST), text).unwrap();
        };
        write("b", r#"{ "api": 1, "id": "com.example.b", "name": "B" }"#);
        write("a", r#"{ "api": 1, "id": "com.example.a", "name": "A" }"#);
        write(
            "c",
            r#"{ "api": 1, "id": "com.example.a", "name": "Again" }"#,
        );
        write("d", "not json");
        let found = load_folder(&root);
        let names: Vec<_> = found
            .iter()
            .map(|found| found.manifest.name.as_str())
            .collect();
        assert_eq!(names, ["A", "B"]);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
