//! What the application asks of the plugins about the account's own
//! surfaces: the keybind actions Settings → Keybinds offers, and the sounds a
//! plugin plays in place of Yapfast's.

use super::*;

/// A keybind action an enabled plugin offers, as the Keybinds page lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeybindAction {
    pub plugin: PluginId,
    /// The plugin's name, which the page shows before the label.
    pub name: String,
    pub action: String,
    pub label: String,
}

impl Bridge {
    /// Every keybind action the enabled plugins with `keybinds` offer, in the
    /// registry's order and then the manifest's.
    pub fn keybind_actions(&self) -> Vec<KeybindAction> {
        self.shared.keybind_actions()
    }

    /// The sound an enabled plugin with `sounds.replace` plays for `event`
    /// (one of [`yapfast_plugin_api::account::SOUND_EVENTS`]), read from its
    /// package, and which plugin it is. The first in the registry's order
    /// wins, so which sound plays doesn't depend on which loaded first.
    pub fn sound_for(&self, event: &str) -> Option<(PluginId, Vec<u8>)> {
        self.shared.sound_for(event)
    }

    /// The sound events a plugin's manifest replaces, each with whether the
    /// person picked a file of their own for it: what its page lists.
    pub fn replaced_sounds(&self, plugin: &PluginId) -> Vec<(String, bool)> {
        let events: Vec<String> = self
            .shared
            .registry()
            .entry(plugin)
            .filter(|entry| entry.manifest.permissions.allows(Capability::SoundsReplace))
            .map(|entry| entry.manifest.sounds.keys().cloned().collect())
            .unwrap_or_default();
        let folder = self.shared.picked_sounds(plugin);
        events
            .into_iter()
            .map(|event| {
                let picked = folder.join(&event).is_file();
                (event, picked)
            })
            .collect()
    }

    /// The person's own file for one of the events a plugin replaces, in place
    /// of the plugin's sound, or none to go back to the plugin's. Kept in the
    /// plugin's data folder, so uninstalling with its data takes it too.
    pub fn set_picked_sound(
        &self,
        plugin: &PluginId,
        event: &str,
        file: Option<Vec<u8>>,
    ) -> Result<(), String> {
        let mapped = self.shared.registry().entry(plugin).is_some_and(|entry| {
            entry.manifest.permissions.allows(Capability::SoundsReplace)
                && entry.manifest.sounds.contains_key(event)
        });
        if !mapped {
            return Err(format!("{plugin} doesn't replace the {event} sound"));
        }
        let path = self.shared.picked_sounds(plugin).join(event);
        let Some(bytes) = file else {
            let _ = std::fs::remove_file(&path);
            return Ok(());
        };
        if bytes.len() as u64 > PICKED_SOUND_BYTES {
            return Err(format!(
                "a sound is at most {} MiB",
                PICKED_SOUND_BYTES >> 20
            ));
        }
        if !yapfast_plugin_api::assets::AssetKind::sniff(&bytes).is_some_and(|kind| kind.is_sound())
        {
            return Err("that file isn't a WAV, Ogg or MP3 sound".to_owned());
        }
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        std::fs::write(&path, bytes).map_err(|error| error.to_string())
    }
}

/// The most a sound the person picks may hold: it is decoded whole to play.
const PICKED_SOUND_BYTES: u64 = 16 << 20;

impl Shared {
    /// Whether `plugin` declares `keybinds` and the keybind `action`.
    pub(super) fn declares_keybind(&self, plugin: &PluginId, action: &str) -> bool {
        self.registry().entry(plugin).is_some_and(|entry| {
            entry.manifest.permissions.allows(Capability::Keybinds)
                && entry
                    .manifest
                    .keybinds
                    .iter()
                    .any(|keybind| keybind.action == action)
        })
    }

    pub(super) fn keybind_actions(&self) -> Vec<KeybindAction> {
        let registry = self.registry();
        registry
            .enabled_ids()
            .filter_map(|id| registry.entry(id))
            .filter(|entry| entry.manifest.permissions.allows(Capability::Keybinds))
            .flat_map(|entry| {
                entry.manifest.keybinds.iter().map(|keybind| KeybindAction {
                    plugin: entry.manifest.id.clone(),
                    name: entry.manifest.name.clone(),
                    action: keybind.action.clone(),
                    label: keybind.label.trim().to_owned(),
                })
            })
            .collect()
    }

    /// Where the person's own picks for a plugin's sounds are kept.
    pub(super) fn picked_sounds(&self, plugin: &PluginId) -> std::path::PathBuf {
        self.store().data_dir(plugin).join("sounds")
    }

    pub(super) fn sound_for(&self, event: &str) -> Option<(PluginId, Vec<u8>)> {
        let wanted: Vec<(PluginId, String)> = {
            let registry = self.registry();
            registry
                .enabled_ids()
                .filter_map(|id| registry.entry(id))
                .filter(|entry| entry.manifest.permissions.allows(Capability::SoundsReplace))
                .filter_map(|entry| {
                    let asset = entry.manifest.sounds.get(event)?;
                    Some((entry.manifest.id.clone(), asset.clone()))
                })
                .collect()
        };
        wanted.into_iter().find_map(|(plugin, asset)| {
            // A file the person picked for this event comes first.
            if let Ok(bytes) = std::fs::read(self.picked_sounds(&plugin).join(event)) {
                return Some((plugin, bytes));
            }
            match self.asset(&plugin, &asset) {
                Ok((kind, bytes)) if kind.is_sound() => Some((plugin, bytes)),
                Ok(_) => None,
                Err(error) => {
                    tracing::debug!(%plugin, %asset, %error, "a replaced sound couldn't be read");
                    None
                }
            }
        })
    }
}
