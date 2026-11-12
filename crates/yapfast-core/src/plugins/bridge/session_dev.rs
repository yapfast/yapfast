use super::*;

/// What a folder's plugin files look like now: their modification times and
/// lengths, which a save changes and a build changes again.
pub(super) fn dev_stamp(folder: &Path) -> String {
    let mut stamp = String::new();
    for name in [crate::plugins::MANIFEST, "plugin.wasm"] {
        let path = folder.join(name);
        let (length, at) = match std::fs::metadata(&path) {
            Ok(metadata) => {
                let at = metadata
                    .modified()
                    .ok()
                    .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|since| since.as_nanos())
                    .unwrap_or(0);
                (metadata.len(), at)
            }
            Err(_) => (0, 0),
        };
        stamp.push_str(&format!("{name}:{length}:{at};"));
    }
    stamp
}

impl Session {
    /// Remembers a plugin's folder, so a save in it is noticed.
    pub(super) fn watch(&mut self, plugin: PluginId, folder: PathBuf) {
        let stamp = dev_stamp(&folder);
        self.dev.insert(
            plugin,
            DevWatch {
                folder,
                stamp,
                due: Instant::now() + DEV_POLL,
            },
        );
    }

    /// Looks at one folder plugin's files, and reloads it when they changed.
    pub(super) fn poll_dev(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        folder: &Path,
        now: Instant,
    ) {
        // A swap already running is one the reload would take the steps of: it
        // waits for the next look instead.
        if self.swapping() {
            if let Some(watch) = self.dev.get_mut(plugin) {
                watch.due = now + DEV_POLL;
            }
            return;
        }
        let stamp = dev_stamp(folder);
        let unchanged = self
            .dev
            .get(plugin)
            .is_some_and(|watch| watch.stamp == stamp);
        if unchanged {
            if let Some(watch) = self.dev.get_mut(plugin) {
                watch.due = now + DEV_POLL;
            }
            return;
        }
        match crate::plugins::installing::from_folder(folder) {
            Ok(fetched) => {
                let id = fetched.id.clone();
                if id != *plugin {
                    tracing::warn!(
                        %plugin,
                        %id,
                        "a folder's manifest names another plugin; not reloading it"
                    );
                    if let Some(watch) = self.dev.get_mut(plugin) {
                        watch.stamp = stamp;
                        watch.due = now + DEV_POLL;
                    }
                    return;
                }
                if let Err(error) = shared.store().install(&fetched.package()) {
                    tracing::warn!(%plugin, %error, "a folder plugin couldn't be reloaded");
                    if let Some(watch) = self.dev.get_mut(plugin) {
                        watch.stamp = stamp;
                        watch.due = now + DEV_POLL;
                    }
                    return;
                }
            }
            Err(reason) => {
                // A folder caught mid-save has a manifest that doesn't parse yet:
                // the next look reads it again rather than unloading the plugin.
                tracing::warn!(%plugin, %reason, "a folder plugin isn't readable; keeping the running one");
                if let Some(watch) = self.dev.get_mut(plugin) {
                    watch.stamp = stamp;
                    watch.due = now + DEV_POLL;
                }
                return;
            }
        }
        if let Some(watch) = self.dev.get_mut(plugin) {
            watch.stamp = stamp;
            watch.due = now + DEV_POLL;
        }
        self.reload_dev(shared, plugin);
    }

    /// Reads the store again and swaps a folder plugin's new version in, with the
    /// plugins that need it stopped first.
    pub(super) fn reload_dev(&mut self, shared: &Shared, plugin: &PluginId) {
        let dependents = shared.registry().dependents(plugin);
        *shared.registry_mut() = Registry::load(&shared.store());
        shared.publish();
        let start: Vec<PluginId> = dependents.iter().rev().cloned().collect();
        self.begin_swap(shared, plugin.clone(), dependents, start);
    }
}
