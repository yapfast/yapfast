use super::*;

impl Shared {
    /// Keeps one request waiting until the person answers about `host`, or
    /// says why it can't be kept.
    ///
    /// A held request is not in flight: it holds no place in the gate's limits
    /// and no socket. What it does hold is its body, so how many a plugin may
    /// have waiting is capped as tightly as the requests it may have in flight.
    pub(super) fn hold(
        &self,
        plugin: &PluginId,
        host: &str,
        answer: tokio::sync::oneshot::Sender<Answered>,
    ) -> Result<(), String> {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        let waiting: usize = held
            .iter()
            .filter(|((owner, _), _)| owner == plugin)
            .map(|(_, waiting)| waiting.len())
            .sum();
        if waiting >= limits::MAX_HTTP_IN_FLIGHT {
            return Err(format!(
                "{plugin} already has {waiting} requests waiting on an answer about a site; \
                 ask again once one of them has one"
            ));
        }
        held.entry((plugin.clone(), host.to_owned()))
            .or_default()
            .push(answer);
        Ok(())
    }

    /// Lets go every request waiting on `host` for `plugin`, with the answer the
    /// person gave; how many that was, for a line in the log.
    pub(super) fn answer_held(&self, plugin: &PluginId, host: &str, answered: Answered) -> usize {
        let waiting = self
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&(plugin.clone(), host.to_owned()))
            .unwrap_or_default();
        let count = waiting.len();
        for answer in waiting {
            // The task that asked may be gone, which is its own business: a
            // request whose guest went away has nothing left to answer.
            let _ = answer.send(answered.clone());
        }
        count
    }

    /// Refuses everything `plugin` has waiting, for when nothing will ever
    /// answer: it was turned off, or removed.
    pub(super) fn release_plugin(&self, plugin: &PluginId, reason: &str) {
        let waiting: Vec<_> = {
            let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
            let keys: Vec<(PluginId, String)> = held
                .keys()
                .filter(|(owner, _)| owner == plugin)
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|key| held.remove(&key))
                .flatten()
                .collect()
        };
        for answer in waiting {
            let _ = answer.send(Answered::Denied(reason.to_owned()));
        }
    }

    /// The same for every plugin, when the helper goes or the app is exiting.
    pub(super) fn release_held(&self, reason: &str) {
        let waiting: Vec<_> =
            std::mem::take(&mut *self.held.lock().unwrap_or_else(PoisonError::into_inner))
                .into_values()
                .flatten()
                .collect();
        for answer in waiting {
            let _ = answer.send(Answered::Denied(reason.to_owned()));
        }
    }

    /// Tells the gate what a plugin's manifest declares, which is what every
    /// request from it is checked against.
    pub(super) fn declare(&self, plugin: &PluginId) {
        let Some(gate) = self.gate() else {
            return;
        };
        let permissions = self
            .registry()
            .entry(plugin)
            .map(|entry| entry.manifest.permissions.clone());
        match permissions {
            Some(permissions) => gate.declare(plugin, permissions),
            // A plugin that is off has nothing to declare, and any grant it had
            // goes with it.
            None => gate.forget(plugin),
        }
    }

    /// Stops the plugin system with a reason the page can show.
    ///
    /// Every enabled plugin is marked suspended rather than installed: the
    /// person turned it on, and the page has to say why it isn't running instead
    /// of pretending nothing was asked for.
    pub(super) fn give_up(&self, reason: String) {
        tracing::warn!(%reason, "plugins are off");
        // Nothing is left to answer a question a plugin asked about a site.
        self.release_held(&reason);
        *self.off.lock().unwrap_or_else(PoisonError::into_inner) = Some(reason.clone());
        let changed: Vec<PluginId> = {
            let mut registry = self.registry_mut();
            let changed: Vec<PluginId> = registry
                .statuses()
                .into_iter()
                .filter(|(_, status)| !matches!(status, PluginStatus::Installed))
                .map(|(plugin, _)| plugin)
                .collect();
            for plugin in &changed {
                registry.note_log(plugin, failure(&reason));
                registry.set_status(
                    plugin,
                    PluginStatus::Suspended {
                        reason: reason.clone(),
                    },
                );
            }
            changed
        };
        for plugin in changed {
            self.announce(Notice::Status {
                plugin,
                status: PluginStatus::Suspended {
                    reason: reason.clone(),
                },
            });
        }
        // A suspended plugin draws nothing: every tree it left is taken away.
        self.announce(Notice::SlotsReset);
        // The flags the send and event paths read say there is nothing to wait
        // for any more: without this, every message is still serialized and
        // every hook chain still built, for a helper that is gone.
        self.enabled.store(false, Ordering::Relaxed);
        self.wanted.store(0, Ordering::Relaxed);
        self.intercepts.store(false, Ordering::Relaxed);
        self.decorates.store(false, Ordering::Relaxed);
        self.decorates_members.store(false, Ordering::Relaxed);
    }
}
