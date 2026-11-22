use super::*;

impl Session {
    /// Loads and activates one plugin, if it isn't up already.
    ///
    /// Loading is what hands the module's path to the helper, so it happens here
    /// and only for a plugin this process hasn't been given: a restart therefore
    /// re-loads on the next trigger rather than eagerly.
    pub(super) fn activate(&mut self, shared: &Shared, plugin: &PluginId) -> Started {
        if self.active.contains(plugin) {
            return Started::Active;
        }
        // A trigger can outlive the plugin it was for — a queued event, a
        // timer that fired a moment before the person turned it off. Starting
        // it here would run a plugin nothing says should run.
        if !shared
            .registry()
            .entry(plugin)
            .is_some_and(|entry| entry.enabled)
        {
            return Started::Refused;
        }
        // The module travels with the activation when this process hasn't given
        // it yet. One request for the whole phase: a `Load` frame and then an
        // `Activate` left the helper holding a module with no instance in
        // between, which reads exactly like an idle plugin, and on a busy machine
        // it left in that gap. Marked as given before the answer because a fault
        // clears it again.
        let load = if self.loaded.contains(plugin) {
            None
        } else {
            let Some(module) = self.ready_module(shared, plugin) else {
                return Started::Refused;
            };
            self.loaded.insert(plugin.clone());
            Some(Activation {
                wasm: module.path,
                memory_pages: module.memory_pages,
                wasi: module.wasi,
                // What its last `yf_deactivate` handed on: the module may be a
                // new version, and the state is what crosses that change.
                state: self.handoff.remove(plugin).flatten(),
                hooks: in_a_hook_chain(shared, plugin),
            })
        };
        if load.is_some() {
            // Its settings and storage go first, so its first read is answered
            // in the helper.
            self.send_kept(shared, plugin);
        }
        let message = ToHelper::Activate {
            plugin: plugin.clone(),
            load,
        };
        let id = self.request(
            &message,
            ACTIVATE_TIMEOUT,
            Purpose::Agent {
                what: "activation",
                plugin: plugin.clone(),
                budget: ACTIVATE_TIMEOUT,
            },
        );
        Started::Asked(id)
    }

    /// Hands the helper a plugin's settings and, when it may keep things and
    /// they fit in one frame, its storage: the helper answers `settings.get`,
    /// `storage.get` and `storage.list` from them at once, so a read never
    /// waits on this process and never comes back `pending`.
    pub(super) fn send_kept(&mut self, shared: &Shared, plugin: &PluginId) {
        let Some(manifest) = shared
            .registry()
            .entry(plugin)
            .map(|entry| entry.manifest.clone())
        else {
            return;
        };
        let settings = match shared.store().settings(plugin, &manifest) {
            serde_json::Value::Object(settings) => settings,
            _ => serde_json::Map::new(),
        };
        let storage = if crate::permissions::check(
            &manifest.permissions,
            yapfast_plugin_api::capability::Capability::Storage,
        )
        .is_ok()
        {
            let (values, bytes) = shared.store().storage_all(plugin);
            // Half a frame, so the settings and the frame's own words fit.
            (bytes < (yapfast_plugin_api::frame::MAX_FRAME / 2) as u64).then_some(values)
        } else {
            None
        };
        let message = ToHelper::Kept {
            plugin: plugin.clone(),
            settings,
            storage,
        };
        self.send_message(&message, FrameKind::Request);
    }

    /// The module to hand the helper, if it still checks out.
    pub(super) fn ready_module(&self, shared: &Shared, plugin: &PluginId) -> Option<Module> {
        let module = shared.registry().entry(plugin)?.module.clone()?;
        if let Err(reason) = shared.store().recheck(&module) {
            tracing::warn!(%plugin, %reason, "a plugin's module changed; not running it");
            note_failure(shared, plugin, &reason);
            let status = PluginStatus::Suspended {
                reason: reason.clone(),
            };
            shared.registry_mut().set_status(plugin, status.clone());
            shared.announce(Notice::Status {
                plugin: plugin.clone(),
                status,
            });
            shared.announce(Notice::SlotsGone(plugin.clone()));
            return None;
        }
        Some(module)
    }

    /// Drops one plugin's instance, keeping what it hands over.
    pub(super) fn deactivate(&mut self, plugin: &PluginId) {
        // Before the early return: a plugin that faulted or was evicted is off
        // already, and its timers still have to go.
        self.forget_timers(plugin);
        if !self.active.contains(plugin) {
            return;
        }
        let message = ToHelper::Deactivate {
            plugin: plugin.clone(),
        };
        self.request(
            &message,
            DEACTIVATE_TIMEOUT,
            Purpose::Agent {
                what: "deactivation",
                plugin: plugin.clone(),
                budget: DEACTIVATE_TIMEOUT,
            },
        );
    }

    /// Drops one plugin's instance now, without its say.
    pub(super) fn evict(&mut self, plugin: &PluginId) {
        if !self.loaded.contains(plugin) {
            return;
        }
        let message = ToHelper::Evict {
            plugin: plugin.clone(),
        };
        self.request(
            &message,
            EVICT_TIMEOUT,
            Purpose::Agent {
                what: "eviction",
                plugin: plugin.clone(),
                budget: EVICT_TIMEOUT,
            },
        );
    }

    /// Runs a hook chain, or holds the send until it answers.
    pub(super) fn hold(&mut self, shared: &Arc<Shared>, mut held: HeldSend, request: HookRequest) {
        // Whatever is left of the person's patience: the helper's start, the
        // hello and a module's first compile all came out of the same budget.
        // A message the person sends is their gesture, for what the chain's
        // plugins then do to their own messages.
        if held.hook == yapfast_plugin_api::hook::HookKind::MessagePreSend {
            for plugin in &held.chain {
                shared.sent_through.record(plugin, Instant::now());
            }
        }
        let remaining = held.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            tracing::debug!(
                hook = %held.hook,
                "a hook chain was already out of time when the helper had it; the message goes unchanged"
            );
            let chain = held.chain.clone();
            held.fail(shared, &chain, "it ran out of time");
            return;
        }
        // Frames are handled in order, so loading each plugin in the chain first
        // means the chain arrives at instances that are already up. A plugin
        // that can't be loaded leaves the chain rather than holding the send.
        let asked = held.chain.clone();
        held.chain
            .retain(|plugin| self.activate(shared, plugin).runs());
        let left: Vec<PluginId> = asked
            .into_iter()
            .filter(|plugin| !held.chain.contains(plugin))
            .collect();
        if !left.is_empty() && held.blocker(shared, &left).is_some() {
            held.fail(shared, &left, "it couldn't be started");
            return;
        }
        if held.chain.is_empty() {
            held.let_go(shared);
            return;
        }
        let message = ToHelper::Hook {
            chain: held.chain.clone(),
            request,
        };
        self.request(&message, remaining, Purpose::Hook(Box::new(held)));
    }

    /// Runs a command a person typed.
    pub(super) fn command(
        &mut self,
        shared: &Shared,
        plugin: &PluginId,
        invocation: CommandInvocation,
    ) {
        self.activate(shared, plugin);
        let message = ToHelper::Command {
            plugin: plugin.clone(),
            command: invocation,
        };
        self.request(
            &message,
            COMMAND_TIMEOUT,
            Purpose::Agent {
                what: "command",
                plugin: plugin.clone(),
                budget: COMMAND_TIMEOUT,
            },
        );
    }
}

/// Whether a plugin's manifest puts it in one of the chains the parent runs,
/// which is what keeps its instance from being evicted as idle.
fn in_a_hook_chain(shared: &Shared, plugin: &PluginId) -> bool {
    shared.registry().entry(plugin).is_some_and(|entry| {
        yapfast_plugin_api::hook::HookKind::DISPATCHED
            .iter()
            .any(|hook| entry.manifest.permissions.allows(hook.capability()))
    })
}
