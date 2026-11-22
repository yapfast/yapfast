use super::*;

impl Bridge {
    /// Asks every plugin that draws under messages what it draws under each row
    /// on screen.
    ///
    /// Only the rows the person can see are asked about, which is what keeps a
    /// long channel from costing one call per message, and a request that
    /// arrives while the helper has none is answered by the row staying plain.
    pub fn decorate(&mut self, rows: &[yapfast_plugin_api::message::DecorateRequest]) {
        // What a message says is `messages.read`'s, here as for code blocks
        // and events (PLAN.md §14: a plugin without it receives no message):
        // a plugin that decorates without it is asked with the body left out,
        // or one that also holds a network host could send every row on
        // screen away.
        let (plugins, enabled): (Vec<(PluginId, bool, u64)>, u64) = {
            let registry = self.shared.registry();
            let plugins = registry
                .decorators()
                .into_iter()
                .filter_map(|plugin| {
                    let entry = registry.entry(&plugin)?;
                    let reads = entry
                        .manifest
                        .permissions
                        .allows(yapfast_plugin_api::capability::Capability::MessagesRead);
                    // The version that answers is part of the key an answer is
                    // kept under, so an update is asked again.
                    let version = decorate_cache::version_key(
                        &entry.manifest.version.to_string(),
                        entry.module.as_ref().map(|module| module.sha.as_str()),
                    );
                    Some((plugin, reads, version))
                })
                .collect();
            (plugins, registry.interface_key())
        };
        if plugins.is_empty() || rows.is_empty() {
            return;
        }
        let now = Instant::now();
        if self.decorate_asked.len() > MAX_DECORATE_ASKED {
            self.decorate_asked
                .retain(|_, asked| now.duration_since(*asked) < DECORATE_AGAIN_AFTER);
        }
        // One pass asks about a screenful, and a screenful is the most a person
        // can see: more than this is a report the interface got wrong.
        for row in rows.iter().take(MAX_DECORATE_ROWS) {
            for (plugin, reads, version) in &plugins {
                // Two layers, each with its own saving. A scroll reports the
                // screen every 50 ms: a row that stayed on it is looked at
                // once a second, so a pass over a still screen costs a hash
                // per row and no notice at all.
                if !due(
                    &mut self.decorate_asked,
                    question(enabled, plugin, row, *reads),
                    now,
                ) {
                    continue;
                }
                let mut request = row.clone();
                if !reads {
                    request.body.clear();
                }
                // A row asked about before, with the same text and the same
                // version of the plugin, is answered from what was kept: a
                // scroll back costs a lookup, not a call into the guest.
                let look = self.shared.decorated().look(plugin, *version, &request);
                match look {
                    decorate_cache::Look::Ask => {
                        self.send(Request::Decorate {
                            plugin: plugin.clone(),
                            request,
                        });
                    }
                    decorate_cache::Look::Waiting => {}
                    decorate_cache::Look::Answered(Some(output)) => {
                        self.shared.announce(Notice::Slot(*output));
                    }
                    decorate_cache::Look::Answered(None) => {}
                }
            }
        }
    }

    /// Lets the rows on screen be asked again at once, for a plugin that said
    /// its decorations changed: the once-a-second window can't be told apart
    /// by plugin, so it all goes, as a settings change does.
    pub fn decorate_again(&mut self) {
        self.decorate_asked.clear();
    }

    /// How many decorations the cache answered without asking a plugin, and
    /// how many it keeps: what the measurements read.
    pub fn decorate_cache_stats(&self) -> (u64, usize) {
        let cache = self.shared.decorated();
        (cache.hits, cache.len())
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

    /// See [`Registry::interface_key`].
    pub fn interface_key(&self) -> u64 {
        self.shared.registry().interface_key()
    }

    /// The enabled plugins' manifests as the interface reads them: their
    /// colours, hidden and resized parts, named parts and buttons. Read from the
    /// store, which is where a plugin installed from Settings keeps them; asked
    /// for only when [`Self::interface_key`] moved.
    pub fn interfaces(&self) -> Vec<crate::Manifest> {
        let enabled: Vec<PluginId> = self.shared.registry().enabled_ids().cloned().collect();
        if enabled.is_empty() {
            return Vec::new();
        }
        let store = self.shared.store();
        let Ok(lock) = store.lock() else {
            return Vec::new();
        };
        enabled
            .iter()
            .filter_map(|plugin| {
                let entry = lock.plugins.get(plugin)?;
                let text = store.manifest_text(entry).ok()?;
                match crate::Manifest::parse(&text) {
                    Ok(manifest) => Some(manifest),
                    Err(error) => {
                        tracing::warn!(%plugin, %error, "an enabled plugin's manifest couldn't be read for the interface");
                        None
                    }
                }
            })
            .collect()
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
        Some(crate::view::form(&mut store, plugin, &manifest, &entry))
    }

    /// Hands a settings change to the instance that asked to hear about them.
    ///
    /// A change only reaches an instance that is running: starting a helper to
    /// tell a plugin something it reads when it starts would cost a process for
    /// nothing.
    pub fn settings_news(&mut self, plugin: &PluginId, news: SettingsNews) {
        // What it drew under messages may depend on what changed — a
        // translator's language is a setting — so its kept answers go, running
        // or not, and the rows on screen ask again.
        self.shared.decorated().forget(plugin);
        // The once-a-second window is keyed by a hash that can't be told
        // apart by plugin, and a settings change is rare: it all goes.
        self.decorate_asked.clear();
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

/// Which question a row is for one plugin, as a hash: the row as the plugin
/// would see it, and the enabled plugins' versions. An edit, an update, or a
/// plugin turned off and on again is a new question, asked at once.
fn question(
    enabled: u64,
    plugin: &PluginId,
    row: &yapfast_plugin_api::message::DecorateRequest,
    reads: bool,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    enabled.hash(&mut hasher);
    plugin.as_str().hash(&mut hasher);
    row.room.hash(&mut hasher);
    row.message.hash(&mut hasher);
    row.author.hash(&mut hasher);
    row.own.hash(&mut hasher);
    // A plugin that can't read messages is asked with the body left out, so a
    // change to it is no change to the question.
    if reads {
        row.body.hash(&mut hasher);
    }
    hasher.finish()
}

/// Whether `question` is to be asked now, noting that it was when it is: one
/// asked under [`DECORATE_AGAIN_AFTER`] ago isn't.
fn due(asked: &mut std::collections::HashMap<u64, Instant>, question: u64, now: Instant) -> bool {
    match asked.entry(question) {
        std::collections::hash_map::Entry::Occupied(last)
            if now.duration_since(*last.get()) < DECORATE_AGAIN_AFTER =>
        {
            false
        }
        entry => {
            entry.insert_entry(now);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_plugin_api::message::DecorateRequest;

    fn row(body: &str) -> DecorateRequest {
        DecorateRequest {
            room: "room-1".to_owned(),
            message: "message-1".to_owned(),
            author: "2".to_owned(),
            body: body.to_owned(),
            own: false,
        }
    }

    /// A scroll reports the screen every 50 ms; a row that stayed on it is
    /// asked once, and again only after the window.
    #[test]
    fn a_row_that_stays_on_screen_is_asked_once_a_second() {
        let plugin = PluginId::parse("com.example.badge").unwrap();
        let mut asked = std::collections::HashMap::new();
        let start = Instant::now();
        let key = question(1, &plugin, &row("hi"), true);
        assert!(due(&mut asked, key, start), "the first time is asked");
        for step in 1..20 {
            let now = start + Duration::from_millis(50 * step);
            assert!(!due(&mut asked, key, now), "not again at {step}");
        }
        assert!(
            due(&mut asked, key, start + DECORATE_AGAIN_AFTER),
            "asked again once the window has passed"
        );
    }

    /// What changes the question asks it again at once.
    #[test]
    fn an_edit_an_update_or_another_plugin_is_a_new_question() {
        let plugin = PluginId::parse("com.example.badge").unwrap();
        let other = PluginId::parse("com.example.other").unwrap();
        let key = question(1, &plugin, &row("hi"), true);
        assert_ne!(key, question(1, &plugin, &row("hi, edited"), true));
        assert_ne!(key, question(2, &plugin, &row("hi"), true));
        assert_ne!(key, question(1, &other, &row("hi"), true));
        // Without messages.read the body isn't sent, so it isn't the question.
        assert_eq!(
            question(1, &plugin, &row("hi"), false),
            question(1, &plugin, &row("hi, edited"), false)
        );
    }
}
