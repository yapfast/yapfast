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
        let fetched =
            crate::installing::from_folder(folder).map_err(|reason| anyhow::anyhow!(reason))?;
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
        self.dev_folders.insert(plugin.clone(), folder.to_owned());
        self.announce(&plugin);
        Ok(plugin)
    }

    /// The folder a development plugin was loaded from, which Restart loads
    /// again. Its pictures are read from the store, as an installed plugin's
    /// are ([`Self::picture`]).
    pub fn package_folder(&self, plugin: &str) -> Option<&Path> {
        self.dev_folders
            .iter()
            .find(|(id, _)| id.as_str() == plugin)
            .map(|(_, folder)| folder.as_path())
    }

    /// One of a plugin's own pictures, as `assets.get` reads it: from the
    /// store, checked again as it is read, and only when it is a picture. A
    /// widget tree's `asset:` image is drawn from this, so a cell shows what the
    /// plugin itself could read. The cell names the asset by its manifest name
    /// or by the path the manifest gives it (`asset:logo.png` for
    /// `"assets": { "logo": "logo.png" }`), since a cell's source must end in a
    /// picture's extension.
    pub fn picture(&self, plugin: &str, path: &str) -> Option<Vec<u8>> {
        let plugin = PluginId::parse(plugin).ok()?;
        if path.starts_with("held:") {
            // One its overlay was handed with `overlay.picture`.
            return self
                .shared
                .overlay_pictures()
                .get(&plugin)?
                .iter()
                .find(|(name, _)| name == path)
                .map(|(_, bytes)| bytes.to_vec());
        }
        let name = {
            let registry = self.shared.registry();
            let assets = &registry.entry(&plugin)?.manifest.assets;
            if assets.contains_key(path) {
                path.to_owned()
            } else {
                assets
                    .iter()
                    .find(|(_, file)| file.to_str() == Some(path))
                    .map(|(name, _)| name.clone())?
            }
        };
        match self.shared.asset(&plugin, &name) {
            Ok((kind, bytes)) if kind.is_picture() => Some(bytes),
            Ok(_) => None,
            Err(error) => {
                tracing::debug!(%plugin, %name, %error, "a picture a plugin drew isn't one of its assets");
                None
            }
        }
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
        self.close_soon();
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
