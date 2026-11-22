use super::*;

impl Session {
    /// Delivers one event, or holds it for the coalescing window.
    pub(super) fn deliver(&mut self, shared: &Shared, plugin: PluginId, event: Arc<SharedEvent>) {
        if !event.kind.is_coalesced() {
            // An event is only worth sending to an instance that exists: a
            // plugin whose module doesn't check out hears about nothing.
            if self.activate(shared, &plugin).runs() {
                let payload = event_payload(&plugin, std::slice::from_ref(&event));
                self.write(FrameKind::Event, 0, payload);
            }
            return;
        }
        let full = {
            let batch = self.batch(&plugin);
            batch.bytes += event.json.len();
            batch.events.push(event);
            batch.events.len() + batch.topics.len() >= BATCH_MAX_EVENTS
                || batch.bytes >= BATCH_MAX_BYTES
        };
        if full {
            self.flush_batch(shared, &plugin);
        }
    }

    /// Holds one published topic for the same window events wait in.
    ///
    /// A publish is never waited on, so what a plugin's burst of them costs the
    /// parent is a place in a batch that was going out anyway; the receiving
    /// plugin pays one event batch's budget for the lot.
    pub(super) fn deliver_topic(
        &mut self,
        shared: &Shared,
        plugin: PluginId,
        topic: Arc<SharedTopic>,
    ) {
        let full = {
            let batch = self.batch(&plugin);
            batch.bytes += topic.json.len();
            batch.topics.push(topic);
            batch.events.len() + batch.topics.len() >= BATCH_MAX_EVENTS
                || batch.bytes >= BATCH_MAX_BYTES
        };
        if full {
            self.flush_batch(shared, &plugin);
        }
    }

    /// One plugin's batch, started when it has something to carry.
    pub(super) fn batch(&mut self, plugin: &PluginId) -> &mut Batch {
        self.batches.entry(plugin.clone()).or_insert_with(|| Batch {
            events: Vec::new(),
            topics: Vec::new(),
            bytes: 0,
            due: Instant::now() + Duration::from_millis(COALESCE_MS),
        })
    }

    /// Sends one plugin's held events, as one call into it.
    pub(super) fn flush_batch(&mut self, shared: &Shared, plugin: &PluginId) {
        let Some(batch) = self.batches.remove(plugin) else {
            return;
        };
        if (batch.events.is_empty() && batch.topics.is_empty())
            || !self.activate(shared, plugin).runs()
        {
            return;
        }
        let payload = frame_payload(plugin, &batch.events, &batch.topics);
        self.write(FrameKind::Event, 0, payload);
    }

    /// Hands the interface what plugins drew since the last frame, one message
    /// per drawing for the application to keep.
    pub(super) fn paint(&mut self, shared: &Arc<Shared>, now: Instant) {
        for output in self.painting.take(now) {
            shared.announce(Notice::Slot(output));
        }
    }

    /// Does the work whose moment has come: timers, batches, timeouts, saves.
    pub(super) fn on_tick(&mut self, shared: &Arc<Shared>) {
        let now = Instant::now();
        let expired: Vec<u64> = self
            .pending
            .iter()
            .filter(|(_, pending)| pending.deadline <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            self.expire(shared, id);
        }
        let due: Vec<PluginId> = self
            .batches
            .iter()
            .filter(|(_, batch)| batch.due <= now)
            .map(|(plugin, _)| plugin.clone())
            .collect();
        for plugin in due {
            self.flush_batch(shared, &plugin);
        }
        let fired: Vec<TimerKey> = self
            .timers
            .iter()
            .filter(|(_, timer)| timer.due <= now)
            .map(|(key, _)| key.clone())
            .collect();
        for key in fired {
            self.fire(shared, key);
        }
        let stale: Vec<(PluginId, PathBuf)> = self
            .dev
            .iter()
            .filter(|(_, watch)| watch.due <= now)
            .map(|(plugin, watch)| (plugin.clone(), watch.folder.clone()))
            .collect();
        for (plugin, folder) in stale {
            self.poll_dev(shared, &plugin, &folder, now);
        }
        if self.painting.due.is_some_and(|due| due <= now) {
            self.paint(shared, now);
        }
        shared.store().flush_due(now);
    }

    /// Tells a running instance what happened to its settings.
    ///
    /// Nothing is sent to a plugin that never asked: a change is read with
    /// `settings.get` when it next starts, and waking an instance to say that
    /// something it reads at its start has changed is work for nothing.
    pub(super) fn news(&mut self, plugin: &PluginId, news: SettingsNews) {
        let Some(calls) = self.watchers.get(plugin) else {
            return;
        };
        if calls.is_empty() {
            return;
        }
        let answer = match serde_json::to_value(&news) {
            Ok(value) => HostReturn::Ok(value),
            Err(error) => {
                tracing::warn!(%error, "could not write a settings change for a plugin");
                return;
            }
        };
        for call in calls.clone() {
            self.reply(plugin, call, answer.clone());
        }
    }

    /// Records that a plugin wants to hear about its settings, and answers the
    /// call `pending` so the guest isn't held while a person thinks.
    ///
    /// One instance asks once; a second ask is answered with the same, and the
    /// first stays, because a guest may retry a call it thinks it lost.
    pub(super) fn watch_settings(&mut self, plugin: &PluginId, call: u64) {
        let calls = self.watchers.entry(plugin.clone()).or_default();
        if !calls.contains(&call) {
            if calls.len() >= settings::MAX_WATCHERS {
                self.reply(
                    plugin,
                    call,
                    HostReturn::Denied {
                        reason: format!(
                            "{plugin} already asked {} times to hear of its settings",
                            settings::MAX_WATCHERS
                        ),
                    },
                );
                return;
            }
            calls.push(call);
        }
        self.reply(
            plugin,
            call,
            HostReturn::Pending {
                tag: call.to_string(),
            },
        );
    }
}
