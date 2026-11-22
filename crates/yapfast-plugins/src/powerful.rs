//! The powerful tier's switch and answers: whether the person lets any plugin
//! use a powerful capability at all, and which of them they allowed, plugin by
//! plugin, with the folder each `fs.folder` was given and the token each local
//! endpoint asks for.
//!
//! A powerful capability is three gates deep. The manifest declares it, and the
//! install dialog shows it in red with a box the person ticks. The switch in
//! Settings → Plugins → Advanced, off until they turn it on, lets any plugin use
//! one. And the first time a plugin uses each one, the person is asked again,
//! in a dialog of its own, and their answer is kept here, in `powerful.json`
//! beside the lock file. Revoking it on the plugin's page takes it out of the
//! file, and the next use asks again.
//!
//! The file is written atomically, like `grants.json`, and a file this build
//! can't read is moved aside and nothing is allowed: an answer nobody can read
//! is a reason to ask again, never one to guess.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::capability::Capability;

use crate::json_file::write_atomically;

/// The file's name, in the plugins directory.
pub const FILE: &str = "powerful.json";

const VERSION: u32 = 1;

/// One plugin's answers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Granted {
    /// The powerful capabilities the person allowed it, by their names.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed: BTreeSet<String>,
    /// The folder `fs.folder` was given, as the person picked it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    /// The secret a local app sends to reach its `net.listen` endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    version: u32,
    /// The switch in Advanced: no plugin uses a powerful capability while it
    /// is off.
    #[serde(default)]
    enabled: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    plugins: BTreeMap<PluginId, Granted>,
}

impl Default for Stored {
    fn default() -> Self {
        Stored {
            version: VERSION,
            enabled: false,
            plugins: BTreeMap::new(),
        }
    }
}

/// Why a powerful call can't go ahead now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gate {
    /// The switch in Advanced is off.
    Off,
    /// The person hasn't answered for this capability yet: ask them.
    Ask,
}

impl Gate {
    /// The sentence a refused call is answered with.
    pub fn reason(&self, plugin: &PluginId, capability: Capability) -> String {
        match self {
            Gate::Off => format!(
                "{plugin} needs `{capability}`, a powerful permission, and powerful plugins are \
                 off: the person turns them on in Settings → Plugins → Advanced"
            ),
            Gate::Ask => format!(
                "{plugin} asked to use `{capability}` for the first time, and the person is being \
                 asked; call again once they have answered"
            ),
        }
    }
}

/// The switch and the answers, read once and kept.
pub struct Powerful {
    path: PathBuf,
    stored: RwLock<Stored>,
}

impl Powerful {
    /// Reads `<plugins>/powerful.json`, or starts with nothing allowed.
    pub fn load(plugins: &Path) -> Self {
        let path = plugins.join(FILE);
        let stored = match std::fs::read(&path) {
            Err(_) => Stored::default(),
            Ok(bytes) => match serde_json::from_slice::<Stored>(&bytes) {
                Ok(stored) if stored.version == VERSION => stored,
                _ => {
                    let aside = path.with_extension("json.invalid");
                    tracing::warn!(path = %path.display(), "powerful.json isn't one this build reads; it is moved aside and nothing powerful is allowed");
                    let _ = std::fs::rename(&path, aside);
                    Stored::default()
                }
            },
        };
        Powerful {
            path,
            stored: RwLock::new(stored),
        }
    }

    fn save(&self, stored: &Stored) -> Result<()> {
        let text = serde_json::to_vec_pretty(stored)?;
        write_atomically(&self.path, &text)
    }

    fn edit(&self, change: impl FnOnce(&mut Stored)) -> Result<()> {
        let mut stored = self.stored.write().unwrap_or_else(PoisonError::into_inner);
        change(&mut stored);
        self.save(&stored)
    }

    /// Whether the switch in Advanced is on.
    pub fn enabled(&self) -> bool {
        self.stored
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .enabled
    }

    /// Turns the switch on or off. Off keeps the answers, so turning it on
    /// again doesn't ask about what was already allowed.
    pub fn set_enabled(&self, enabled: bool) -> Result<()> {
        self.edit(|stored| stored.enabled = enabled)
    }

    /// Whether `plugin` may use `capability` now.
    pub fn check(&self, plugin: &PluginId, capability: Capability) -> Result<(), Gate> {
        let stored = self.stored.read().unwrap_or_else(PoisonError::into_inner);
        // The switch is the powerful tier's; a capability that is only asked
        // about the first time (`profile.write`) waits for the answer alone.
        if !stored.enabled && capability.is_powerful() {
            return Err(Gate::Off);
        }
        let allowed = stored
            .plugins
            .get(plugin)
            .is_some_and(|granted| granted.allowed.contains(capability.as_str()));
        if !allowed {
            return Err(Gate::Ask);
        }
        if capability == Capability::FsFolder
            && stored
                .plugins
                .get(plugin)
                .is_none_or(|granted| granted.folder.is_none())
        {
            return Err(Gate::Ask);
        }
        Ok(())
    }

    /// Records the person's yes, with the folder they picked for `fs.folder`.
    pub fn allow(
        &self,
        plugin: &PluginId,
        capability: Capability,
        folder: Option<PathBuf>,
    ) -> Result<()> {
        self.edit(|stored| {
            let granted = stored.plugins.entry(plugin.clone()).or_default();
            granted.allowed.insert(capability.as_str().to_owned());
            if folder.is_some() {
                granted.folder = folder;
            }
        })
    }

    /// Takes one capability back: the next use asks again. Taking `fs.folder`
    /// back forgets the folder too, and `net.listen` its token.
    pub fn revoke(&self, plugin: &PluginId, capability: Capability) -> Result<()> {
        self.edit(|stored| {
            if let Some(granted) = stored.plugins.get_mut(plugin) {
                granted.allowed.remove(capability.as_str());
                match capability {
                    Capability::FsFolder => granted.folder = None,
                    Capability::NetListen => granted.token = None,
                    _ => {}
                }
                if granted.allowed.is_empty() && granted.folder.is_none() {
                    stored.plugins.remove(plugin);
                }
            }
        })
    }

    /// Forgets everything about a plugin, as an uninstall does.
    pub fn forget(&self, plugin: &PluginId) -> Result<()> {
        self.edit(|stored| {
            stored.plugins.remove(plugin);
        })
    }

    /// What the person allowed one plugin.
    pub fn granted(&self, plugin: &PluginId) -> Granted {
        self.stored
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .plugins
            .get(plugin)
            .cloned()
            .unwrap_or_default()
    }

    /// The folder `fs.folder` was given.
    pub fn folder(&self, plugin: &PluginId) -> Option<PathBuf> {
        self.granted(plugin).folder
    }

    /// The local endpoint's token, made the first time it is asked for and
    /// kept, so the address and the secret a person pasted into another app
    /// survive a restart. 244 random bits, as two version-4 UUIDs.
    pub fn token(&self, plugin: &PluginId) -> Result<String> {
        if let Some(token) = self.granted(plugin).token {
            return Ok(token);
        }
        let token = format!(
            "yf_{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        self.edit(|stored| {
            stored.plugins.entry(plugin.clone()).or_default().token = Some(token.clone());
        })?;
        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin() -> PluginId {
        PluginId::parse("com.example.mcp").expect("an id")
    }

    #[test]
    fn nothing_powerful_runs_until_the_switch_is_on_and_the_person_said_yes() {
        let directory = tempfile::tempdir().expect("a directory");
        let powerful = Powerful::load(directory.path());
        assert_eq!(
            powerful.check(&plugin(), Capability::NetListen),
            Err(Gate::Off),
            "off by default"
        );
        powerful.set_enabled(true).expect("saved");
        assert_eq!(
            powerful.check(&plugin(), Capability::NetListen),
            Err(Gate::Ask),
            "on, and not answered yet"
        );
        powerful
            .allow(&plugin(), Capability::NetListen, None)
            .expect("saved");
        assert_eq!(powerful.check(&plugin(), Capability::NetListen), Ok(()));
        assert_eq!(
            powerful.check(&plugin(), Capability::NetAny),
            Err(Gate::Ask),
            "each capability is its own question"
        );
        // Kept across a restart, and the switch with it.
        let again = Powerful::load(directory.path());
        assert_eq!(again.check(&plugin(), Capability::NetListen), Ok(()));
        again.set_enabled(false).expect("saved");
        assert_eq!(
            again.check(&plugin(), Capability::NetListen),
            Err(Gate::Off),
            "the switch outranks an answer"
        );
    }

    #[test]
    fn a_folder_is_needed_for_fs_folder_and_revoking_forgets_it() {
        let directory = tempfile::tempdir().expect("a directory");
        let powerful = Powerful::load(directory.path());
        powerful.set_enabled(true).expect("saved");
        powerful
            .allow(&plugin(), Capability::FsFolder, None)
            .expect("saved");
        assert_eq!(
            powerful.check(&plugin(), Capability::FsFolder),
            Err(Gate::Ask),
            "a yes with no folder picked is no folder"
        );
        powerful
            .allow(
                &plugin(),
                Capability::FsFolder,
                Some("/tmp/pictures".into()),
            )
            .expect("saved");
        assert_eq!(powerful.check(&plugin(), Capability::FsFolder), Ok(()));
        powerful
            .revoke(&plugin(), Capability::FsFolder)
            .expect("saved");
        assert_eq!(powerful.folder(&plugin()), None);
        assert_eq!(
            powerful.check(&plugin(), Capability::FsFolder),
            Err(Gate::Ask)
        );
    }

    #[test]
    fn a_token_is_made_once_kept_and_forgotten_with_its_capability() {
        let directory = tempfile::tempdir().expect("a directory");
        let powerful = Powerful::load(directory.path());
        let token = powerful.token(&plugin()).expect("a token");
        assert!(token.len() >= 64, "{token}");
        assert_eq!(powerful.token(&plugin()).expect("the same"), token);
        powerful
            .allow(&plugin(), Capability::NetListen, None)
            .expect("saved");
        powerful
            .revoke(&plugin(), Capability::NetListen)
            .expect("saved");
        assert_ne!(
            powerful.token(&plugin()).expect("a new one"),
            token,
            "revoking the endpoint retires its secret"
        );
    }

    #[test]
    fn a_file_this_build_cant_read_allows_nothing() {
        let directory = tempfile::tempdir().expect("a directory");
        std::fs::write(directory.path().join(FILE), b"{ \"enabled\": true").expect("written");
        let powerful = Powerful::load(directory.path());
        assert!(!powerful.enabled());
        assert!(directory.path().join("powerful.json.invalid").exists());
    }
}
