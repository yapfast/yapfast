use super::*;

impl Bridge {
    /// Installs a plugin from a folder on this computer and watches the folder.
    ///
    /// This is the development loop: the folder's manifest and module are read
    /// into the store, the plugin is turned on, and every later save in that
    /// folder is one `stat` a second until the plugin is removed. Its storage and
    /// settings live under `data/<id>/` and are kept across reloads, as they are
    /// across an update.
    pub fn load_folder(&mut self, folder: &Path) -> anyhow::Result<PluginId> {
        let fetched = crate::plugins::installing::from_folder(folder)
            .map_err(|reason| anyhow::anyhow!(reason))?;
        let plugin = fetched.id.clone();
        {
            let mut store = self.shared.store();
            store.install(&fetched.package())?;
            store.set_enabled(&plugin, true)?;
        }
        self.shared.clear_off();
        self.refresh(&plugin);
        self.send(Request::Watch {
            plugin: plugin.clone(),
            folder: folder.to_owned(),
        });
        self.announce(&plugin);
        Ok(plugin)
    }

    /// Reads the store again and puts what it holds for one plugin in place.
    ///
    /// The plugin's dependents are worked out from the registry that is running
    /// now, before the new manifests are read: they are the ones that were
    /// ordered against the version being replaced. They are stopped in reverse
    /// layer order, the plugin is swapped, and they are started again in layer
    /// order, the whole sequence asking the helper to confirm each step.
    pub(super) fn refresh(&mut self, plugin: &PluginId) {
        let stop: Vec<PluginId> = self.shared.registry().dependents(plugin);
        let registry = Registry::load(&self.shared.store());
        // A dependent whose requirement the new version no longer satisfies
        // can't be started against it: it stays off, and the page says so.
        let start: Vec<PluginId> = stop.iter().rev().cloned().collect();
        *self.shared.registry_mut() = registry;
        self.shared.publish();
        if self.supervisor.is_some() && self.enabled() {
            self.send(Request::Swap {
                plugin: plugin.clone(),
                stop,
                start,
            });
        }
        for (plugin, status) in self.shared.statuses() {
            self.shared.announce(Notice::Status { plugin, status });
        }
    }

    /// Reads the lock file and every installed manifest again, which is what an
    /// uninstall changes, and tells the page.
    ///
    /// The helper goes with it: its instances were loaded from versions the store
    /// no longer holds. That is the one case a swap can't cover, because there is
    /// nothing to swap in.
    pub(super) fn reload(&mut self) {
        self.close();
        let registry = Registry::load(&self.shared.store());
        *self.shared.registry_mut() = registry;
        self.shared.publish();
        // What was drawn came from the code that just went; what is drawn next
        // comes from what the store holds now.
        self.shared.announce(Notice::SlotsReset);
        for (plugin, status) in self.shared.statuses() {
            self.shared.announce(Notice::Status { plugin, status });
        }
    }
}
