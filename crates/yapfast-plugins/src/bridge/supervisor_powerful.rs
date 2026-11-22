use super::*;

/// What the plugin's page shows of its local endpoint: where it listens and
/// the token an app sends to reach it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointView {
    pub port: u16,
    pub token: String,
}

impl Bridge {
    /// Whether the switch in Advanced lets any plugin use a powerful
    /// capability.
    pub fn powerful_enabled(&self) -> bool {
        self.shared.powerful.enabled()
    }

    /// Turns the switch on or off. Off stops every endpoint at once: nothing
    /// powerful runs while it is off.
    pub fn set_powerful_enabled(&mut self, enabled: bool) -> anyhow::Result<()> {
        self.shared.powerful.set_enabled(enabled)?;
        if !enabled {
            let plugins: Vec<PluginId> = self
                .shared
                .endpoints
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .keys()
                .cloned()
                .collect();
            for plugin in plugins {
                self.shared.stop_serving(&plugin);
            }
        }
        Ok(())
    }

    /// The person answered a powerful capability's first-use dialog: `folder`
    /// is the one they picked for `fs.folder`. A no is kept for this session
    /// only, as the question that is no longer on screen; the plugin's next
    /// use asks again.
    pub fn answer_powerful(
        &mut self,
        plugin: &PluginId,
        capability: Capability,
        allow: bool,
        folder: Option<PathBuf>,
    ) -> anyhow::Result<()> {
        self.shared
            .powerful_asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&(plugin.clone(), capability));
        let kept = if allow {
            self.shared.powerful.allow(plugin, capability, folder)
        } else {
            Ok(())
        };
        // The calls that waited on the question go ahead now, or are told no,
        // whether or not the answer could be kept.
        self.send(Request::PowerfulAnswered {
            plugin: plugin.clone(),
            capability,
            allow: allow && kept.is_ok(),
        });
        kept
    }

    /// Takes one powerful capability back: the next use asks again, and an
    /// endpoint stops at once.
    pub fn revoke_powerful(
        &mut self,
        plugin: &PluginId,
        capability: Capability,
    ) -> anyhow::Result<()> {
        self.shared.powerful.revoke(plugin, capability)?;
        if matches!(capability, Capability::NetListen | Capability::NetSocket) {
            self.shared.stop_serving(plugin);
        }
        Ok(())
    }

    /// What an installed plugin's manifest declares it may do.
    pub fn permissions(
        &self,
        plugin: &PluginId,
    ) -> Option<yapfast_plugin_api::capability::Permissions> {
        self.shared
            .registry()
            .entry(plugin)
            .map(|entry| entry.manifest.permissions.clone())
    }

    /// What the person allowed one plugin of the powerful tier.
    pub fn powerful_granted(&self, plugin: &PluginId) -> crate::powerful::Granted {
        self.shared.powerful.granted(plugin)
    }

    /// The plugin's local endpoint, while it serves.
    pub fn endpoint(&self, plugin: &PluginId) -> Option<EndpointView> {
        let port = self
            .shared
            .endpoints
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(plugin)
            .map(|endpoint| endpoint.port)?;
        let token = self.shared.powerful.granted(plugin).token?;
        Some(EndpointView { port, token })
    }
}

impl Shared {
    /// Stops a plugin's endpoint and lets go of the requests it held, and
    /// closes its connections: everything of the network it holds open.
    pub(super) fn stop_serving(&self, plugin: &PluginId) {
        let closed = self
            .sockets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(plugin);
        drop(closed);
        // Its calls waiting on a powerful question have no instance to go
        // back to.
        self.powerful_waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(owner, ..)| owner != plugin);
        self.endpoints
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(plugin);
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(owner, _), _| owner != plugin);
    }
}
