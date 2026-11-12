use super::*;

impl Bridge {
    /// Asks every plugin that draws under messages what it draws under each row
    /// on screen.
    ///
    /// Only the rows the person can see are asked about, which is what keeps a
    /// long channel from costing one call per message, and a request that
    /// arrives while the helper has none is answered by the row staying plain.
    pub fn decorate(&mut self, rows: &[yapfast_plugin_api::message::DecorateRequest]) {
        let plugins = self.shared.registry().decorators();
        if plugins.is_empty() || rows.is_empty() {
            return;
        }
        // One pass asks about a screenful, and a screenful is the most a person
        // can see: more than this is a report the interface got wrong.
        for row in rows.iter().take(MAX_DECORATE_ROWS) {
            for plugin in &plugins {
                self.send(Request::Decorate {
                    plugin: plugin.clone(),
                    request: row.clone(),
                });
            }
        }
    }

    /// Asks every plugin that badges people what it badges the members on
    /// screen with.
    ///
    /// As with a message's decoration, only the rows in sight are asked about,
    /// so a large server costs what its list shows and not its whole roster.
    pub fn decorate_members(
        &mut self,
        rows: &[yapfast_plugin_api::message::DecorateMemberRequest],
    ) {
        let plugins = self.shared.registry().member_decorators();
        if plugins.is_empty() || rows.is_empty() {
            return;
        }
        for row in rows.iter().take(MAX_DECORATE_ROWS) {
            for plugin in &plugins {
                self.send(Request::DecorateMember {
                    plugin: plugin.clone(),
                    request: row.clone(),
                });
            }
        }
    }

    /// Answers a call the application carried out, under the number the guest
    /// asked with, so the runtime hands it to the call that is waiting.
    pub fn answer_call(&mut self, plugin: &PluginId, call: u64, answer: HostReturn) {
        self.send(Request::CallAnswer {
            plugin: plugin.clone(),
            call,
            answer,
        });
    }

    /// Records that the person just did something: a plugin's send needs one
    /// behind it unless its manifest asks for `messages.send.background`.
    pub fn note_gesture(&self) {
        self.shared.gesture();
    }

    /// One plugin's settings form: the schema its manifest declares, and the
    /// values it holds now.
    ///
    /// Read fresh for each edit, from the manifest and the store, so the person
    /// edits what is written down and nothing is held between edits.
    pub fn form(&self, plugin: &PluginId) -> Option<settings::Form> {
        let shared = self.shared();
        let manifest = shared.registry().entry(plugin)?.manifest.clone();
        let entry = shared.store().lock().ok()?.plugins.get(plugin).cloned()?;
        let mut store = shared.store();
        Some(crate::plugins::view::form(
            &mut store, plugin, &manifest, &entry,
        ))
    }

    /// Hands a settings change to the instance that asked to hear about them.
    ///
    /// A change only reaches an instance that is running: starting a helper to
    /// tell a plugin something it reads when it starts would cost a process for
    /// nothing.
    pub fn settings_news(&mut self, plugin: &PluginId, news: SettingsNews) {
        if !matches!(self.helper(), HelperState::Running { .. }) {
            return;
        }
        self.send(Request::SettingsNews {
            plugin: plugin.clone(),
            news,
        });
    }

    /// The enabled plugins' commands, each with its plugin's own name: the
    /// composer's list says which plugin a row would run.
    pub fn commands_named(&self) -> Vec<(PluginId, String, CommandDecl)> {
        let registry = self.shared.registry();
        registry
            .commands()
            .into_iter()
            .filter_map(|(plugin, command)| {
                let name = registry.entry(&plugin)?.manifest.name.clone();
                Some((plugin, name, command))
            })
            .collect()
    }
}
