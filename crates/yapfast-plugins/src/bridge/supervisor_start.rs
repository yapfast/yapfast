use super::*;

impl Bridge {
    /// Activates what must run at startup: plugins whose manifest asks for it,
    /// and the ones that intercept sends, since the first message of the session
    /// must not go out before they are ready.
    pub fn start(&mut self) {
        if !self.enabled() {
            return;
        }
        let plugins: Vec<PluginId> = {
            let registry = self.shared.registry();
            registry
                .statuses()
                .into_iter()
                .filter(|(_, status)| !matches!(status, PluginStatus::Installed))
                .filter(|(plugin, _)| {
                    registry
                        .entry(plugin)
                        .is_some_and(|entry| entry.manifest.activates_at_startup())
                })
                .map(|(plugin, _)| plugin)
                .collect()
        };
        if plugins.is_empty() {
            return;
        }
        self.send(Request::Activate { plugins });
    }
}
