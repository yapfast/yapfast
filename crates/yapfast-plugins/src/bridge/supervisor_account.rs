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
}

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
        wanted
            .into_iter()
            .find_map(|(plugin, asset)| match self.asset(&plugin, &asset) {
                Ok((kind, bytes)) if kind.is_sound() => Some((plugin, bytes)),
                Ok(_) => None,
                Err(error) => {
                    tracing::debug!(%plugin, %asset, %error, "a replaced sound couldn't be read");
                    None
                }
            })
    }
}
