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
            })
        };
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
        let remaining = held.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            tracing::debug!(
                hook = %held.hook,
                "a hook chain was already out of time when the helper had it; the message goes unchanged"
            );
            held.let_go(shared);
            return;
        }
        // Frames are handled in order, so loading each plugin in the chain first
        // means the chain arrives at instances that are already up. A plugin
        // that can't be loaded leaves the chain rather than holding the send.
        held.chain
            .retain(|plugin| self.activate(shared, plugin).runs());
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
