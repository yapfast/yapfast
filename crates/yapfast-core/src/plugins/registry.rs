//! What is enabled, and what each plugin wants to be told about.
//!
//! The registry is the parent's memory of the plugin system: one entry per
//! installed plugin, holding its manifest, whether the person turned it on, the
//! events and hooks it asked for, its status and its log lines. It is built once
//! from the lock file and the manifests, and changed only when the person turns
//! something on or off.
//!
//! Two things it exists for are on hot paths, so they are compiled here once
//! rather than worked out per event: each plugin's subscriptions become a bitmask
//! of event kinds, and the union of those masks says whether an event has any
//! subscriber at all. An event nobody wants is one load of that number. The other
//! is [`Registry::chain`]: the plugins a hook runs through, in dependency order,
//! worked out when the enabled set changes rather than while a message waits to
//! be sent.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use yapfast_plugin_api::capability::{Capability, Permissions};
use yapfast_plugin_api::event::{EventEnvelope, EventKind, SubscriptionFilter};
use yapfast_plugin_api::graph::{self, GraphNode};
use yapfast_plugin_api::hook::HookKind;
use yapfast_plugin_api::message::{LogRecord, PluginStatus};
use yapfast_plugin_api::{PluginId, PluginManifest, limits};

use crate::plugins::store::{Module, Store};

/// How many log lines a plugin's ring keeps.
///
/// A plugin that logs in a loop must not grow the parent's memory: the page
/// shows the recent past, and the old lines are gone.
pub const LOG_RING: usize = 200;

/// The events and filters one plugin asked for, compiled.
#[derive(Clone, Debug, Default)]
pub struct Subscriptions {
    /// One bit per [`EventKind`], in the order [`EventKind::ALL`] lists them.
    mask: u64,
    /// What narrows the bitmask: a room, or leaving the account's own messages
    /// out. A manifest can't express either yet, so this is the default until a
    /// plugin subscribes at runtime.
    filter: SubscriptionFilter,
}

impl Subscriptions {
    /// The subscriptions a manifest declares, from its `event:` triggers.
    pub fn of(manifest: &PluginManifest) -> Subscriptions {
        let mut subscriptions = Subscriptions::default();
        if let Some(activation) = &manifest.activation {
            for name in activation.events() {
                if let Some(kind) = EventKind::ALL
                    .into_iter()
                    .find(|kind| kind.as_str() == name)
                {
                    subscriptions.mask |= Subscriptions::bit(kind);
                }
            }
        }
        subscriptions
    }

    /// The bit that stands for one event kind.
    ///
    /// A kind's discriminant is its place in [`EventKind::ALL`], because the enum
    /// is declared in wire order; a bitmask of twelve kinds fits one word, which
    /// is why the summary of every plugin's subscriptions is one atomic load.
    pub fn bit(kind: EventKind) -> u64 {
        1 << (kind as u64)
    }

    /// The kinds this plugin asked for, as one number.
    pub fn mask(&self) -> u64 {
        self.mask
    }

    /// Whether the plugin asked for this kind of event.
    pub fn wants(&self, kind: EventKind) -> bool {
        self.mask & Subscriptions::bit(kind) != 0
    }

    /// Whether an event of a wanted kind also passes the filters.
    pub fn allows(&self, event: &EventEnvelope) -> bool {
        self.filter.allows(event)
    }
}

/// One installed plugin, as the parent knows it without running anything.
pub struct Entry {
    pub manifest: PluginManifest,
    /// Whether the person has it on. Its [`PluginStatus`] says what it is doing
    /// about that, which is not the same thing: an enabled plugin whose module
    /// failed to verify is not running.
    pub enabled: bool,
    pub status: PluginStatus,
    pub subscriptions: Subscriptions,
    /// Its verified module, or nothing when it has no `.wasm` or the file didn't
    /// check out. An entry without one contributes no events and no hooks: there
    /// is nothing to hand them to.
    pub module: Option<Module>,
    /// The topics it subscribed to from its own code, by name. Empty for a
    /// plugin that only listens to events, which is most of them.
    pub topics: BTreeSet<String>,
    /// The recent lines it logged, oldest first.
    pub log: VecDeque<LogRecord>,
}

impl Entry {
    /// Whether this plugin can be woken at all.
    pub fn runs(&self) -> bool {
        self.enabled && self.module.is_some()
    }
}

/// What the hot paths read about the whole registry, as one plain value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Whether any plugin is enabled.
    pub enabled: bool,
    /// The union of the enabled plugins' subscriptions.
    pub wanted: u64,
    /// Whether an enabled plugin changes or stops outgoing messages, which is
    /// the only reason the send path has to wait for anything.
    pub intercepts: bool,
}

/// One installed plugin, copied out of the registry for the settings page
/// that lists them: the registry itself is read behind a lock, so the page
/// can't hold a borrow of it.
#[derive(Clone, Debug)]
pub struct Listed {
    pub manifest: PluginManifest,
    pub enabled: bool,
    pub status: PluginStatus,
    /// Its recent log lines, oldest first.
    pub log: Vec<LogRecord>,
}

/// Every installed plugin, and the order they run in.
#[derive(Default)]
pub struct Registry {
    entries: BTreeMap<PluginId, Entry>,
    /// The enabled plugins, dependencies first and then by id: the order a hook
    /// chain runs in and the order events are delivered in.
    order: Vec<PluginId>,
    summary: Summary,
}

impl Registry {
    /// Reads the lock file and the manifest of every installed plugin.
    ///
    /// A plugin whose manifest or module doesn't check out is left out with a
    /// warning: one broken install must not take the plugin system down with it,
    /// and the person can see the problem in the log.
    pub fn load(store: &Store) -> Registry {
        let mut registry = Registry::default();
        let lock = match store.lock() {
            Ok(lock) => lock,
            Err(error) => {
                tracing::warn!(%error, "could not read the plugin lock file");
                return registry;
            }
        };
        for (plugin, installed) in &lock.plugins {
            let manifest = match store.manifest(installed) {
                Ok(manifest) => manifest,
                Err(error) => {
                    tracing::warn!(%plugin, %error, "could not read a plugin's manifest");
                    continue;
                }
            };
            let module = match store.module(installed, &manifest) {
                Ok(module) => Some(module),
                Err(reason) => {
                    if manifest.wasm.is_some() {
                        tracing::warn!(%plugin, %reason, "a plugin's module isn't usable");
                    }
                    None
                }
            };
            let enabled = installed.enabled;
            let status = if enabled {
                if module.is_some() {
                    PluginStatus::Enabled
                } else {
                    PluginStatus::Suspended {
                        reason: "its module doesn't check out".to_owned(),
                    }
                }
            } else {
                PluginStatus::Installed
            };
            registry.entries.insert(
                plugin.clone(),
                Entry {
                    subscriptions: Subscriptions::of(&manifest),
                    enabled,
                    status,
                    module,
                    manifest,
                    topics: BTreeSet::new(),
                    log: VecDeque::new(),
                },
            );
        }
        registry.recompute();
        registry
    }

    /// Turns a plugin on or off, and with it the plugins it needs.
    ///
    /// Enabling brings in everything its manifest requires and refuses the whole
    /// thing when one of those is missing or the wrong version: half a plugin's
    /// world running is worse than a sentence saying what is missing.
    ///
    /// Disabling stops the plugins that need it as well, outermost first, and
    /// answers with everything that changed: a plugin left enabled over a
    /// dependency that is off would be one running against nothing the moment it
    /// asked it anything.
    pub fn set_enabled(
        &mut self,
        plugin: &PluginId,
        on: bool,
        store: &Store,
    ) -> anyhow::Result<Vec<PluginId>> {
        if !self.entries.contains_key(plugin) {
            anyhow::bail!("{plugin} isn't installed");
        }
        let turning: Vec<PluginId> = if on {
            let nodes: Vec<GraphNode> = self.entries.values().map(node).collect();
            graph::closure(plugin, &nodes).map_err(|error| anyhow::anyhow!("{error}"))?
        } else {
            let mut off = self.dependents(plugin);
            off.push(plugin.clone());
            off
        };
        for id in &turning {
            store.set_enabled(id, on)?;
        }
        for id in &turning {
            if let Some(entry) = self.entries.get_mut(id) {
                entry.enabled = on;
                entry.status = if !on {
                    PluginStatus::Installed
                } else if entry.module.is_some() {
                    PluginStatus::Enabled
                } else {
                    PluginStatus::Suspended {
                        reason: "its module doesn't check out".to_owned(),
                    }
                };
            }
        }
        self.recompute();
        Ok(turning)
    }

    /// Records a plugin's subscription to a topic, or its leaving one.
    ///
    /// A topic is the publishing plugins' own name, so nothing checks it against
    /// a list: what is checked is the shape `plugin.publish` and
    /// `plugin.subscribe` both take, and how many topics one plugin may hold.
    pub fn subscribe(
        &mut self,
        plugin: &PluginId,
        topic: &str,
        on: bool,
    ) -> Result<bool, String> {
        if topic.is_empty() || topic.len() > limits::MAX_TOPIC_NAME_BYTES {
            return Err(format!(
                "a topic's name is 1 to {} bytes",
                limits::MAX_TOPIC_NAME_BYTES
            ));
        }
        let Some(entry) = self.entries.get_mut(plugin) else {
            return Err(format!("{plugin} isn't installed"));
        };
        if on {
            if entry.topics.len() >= limits::MAX_TOPICS && !entry.topics.contains(topic) {
                return Err(format!(
                    "{plugin} already subscribes to {} topics",
                    limits::MAX_TOPICS
                ));
            }
            entry.topics.insert(topic.to_owned());
        } else {
            entry.topics.remove(topic);
        }
        Ok(entry.topics.contains(topic))
    }

    /// The plugins that want a topic, in activation order.
    ///
    /// The publisher is left out of its own topic: a plugin that publishes what
    /// it also listens to would answer itself, and a loop of those has nothing
    /// to stop it.
    pub fn subscribers_to(&self, topic: &str, from: &PluginId) -> Vec<PluginId> {
        self.order
            .iter()
            .filter(|id| **id != *from)
            .filter(|id| {
                self.entries
                    .get(*id)
                    .is_some_and(|entry| entry.runs() && entry.topics.contains(topic))
            })
            .cloned()
            .collect()
    }

    /// Whether a plugin has a module and is on, which is what a call needs of
    /// its target.
    pub fn callable(&self, plugin: &PluginId) -> bool {
        self.entries.get(plugin).is_some_and(Entry::runs)
    }

    /// Whether `caller` declares `callee`, which is the only edge a call may
    /// follow.
    ///
    /// A required dependency and an optional one both count: the manifest says
    /// the two plugins belong together in an order, which is what makes a call
    /// between them able to wait without either deadlocking.
    pub fn declares(&self, caller: &PluginId, callee: &PluginId) -> bool {
        self.entries.get(caller).is_some_and(|entry| {
            entry.manifest.dependencies.contains_key(callee)
                || entry.manifest.optional_dependencies.contains_key(callee)
        })
    }

    /// The plugins that need `plugin`, directly or through others, outermost
    /// first.
    ///
    /// Only enabled plugins are in it, because only they would run: the answer
    /// is the order to stop them in, and the reverse of it is the order to start
    /// them again once what they need is back.
    pub fn dependents(&self, plugin: &PluginId) -> Vec<PluginId> {
        let nodes: Vec<GraphNode> = self
            .entries
            .values()
            .filter(|entry| entry.enabled)
            .map(node)
            .collect();
        let Ok(layers) = graph::layers(&nodes) else {
            // A graph that can't be laid out is one `set_enabled` refuses; the
            // safest answer for a swap is that nothing may be holding on.
            return Vec::new();
        };
        // Each plugin's place in the order, so dependents can be ranked by how
        // far from the one being swapped they are.
        let mut rank: BTreeMap<&PluginId, usize> = BTreeMap::new();
        for (index, layer) in layers.iter().enumerate() {
            for id in layer {
                rank.insert(id, index);
            }
        }
        let mut needed_by: BTreeMap<&PluginId, Vec<&PluginId>> = BTreeMap::new();
        for node in &nodes {
            let edges = node.dependencies.keys().chain(
                node.optional_dependencies
                    .keys()
                    .filter(|id| nodes.iter().any(|other| other.id == **id)),
            );
            for needs in edges {
                needed_by.entry(needs).or_default().push(&node.id);
            }
        }
        let mut found: BTreeSet<&PluginId> = BTreeSet::new();
        let mut queue = VecDeque::from([plugin]);
        while let Some(id) = queue.pop_front() {
            for dependent in needed_by.get(id).into_iter().flatten() {
                if found.insert(dependent) {
                    queue.push_back(dependent);
                }
            }
        }
        let mut dependents: Vec<PluginId> = found.into_iter().cloned().collect();
        dependents.sort_by_key(|id| (std::cmp::Reverse(rank.get(id).copied().unwrap_or(0)), id.clone()));
        dependents
    }

    /// What the hot paths need to know about every plugin at once.
    pub fn summary(&self) -> Summary {
        self.summary
    }

    /// Every installed plugin, in the order the page lists them: by id.
    pub fn listed(&self) -> Vec<Listed> {
        self.entries
            .values()
            .map(|entry| Listed {
                manifest: entry.manifest.clone(),
                enabled: entry.enabled,
                status: entry.status.clone(),
                log: entry.log.iter().cloned().collect(),
            })
            .collect()
    }

    /// One plugin's entry.
    pub fn entry(&self, plugin: &PluginId) -> Option<&Entry> {
        self.entries.get(plugin)
    }

    /// One plugin's entry, to change as the helper reports.
    pub fn entry_mut(&mut self, plugin: &PluginId) -> Option<&mut Entry> {
        self.entries.get_mut(plugin)
    }

    /// Records a plugin's new status.
    pub fn set_status(&mut self, plugin: &PluginId, status: PluginStatus) {
        if let Some(entry) = self.entries.get_mut(plugin) {
            entry.status = status;
        }
    }

    /// Keeps one of a plugin's log lines, dropping the oldest past the ring.
    pub fn note_log(&mut self, plugin: &PluginId, record: LogRecord) {
        let Some(entry) = self.entries.get_mut(plugin) else {
            return;
        };
        if entry.log.len() == LOG_RING {
            entry.log.pop_front();
        }
        entry.log.push_back(record);
    }

    /// The plugins an event goes to, in activation order.
    ///
    /// This is where an event is decided: the kind has to be one the plugin
    /// subscribed to, its manifest has to hold the capability that kind needs,
    /// and its filter has to let the event through. All of it runs in the parent
    /// before anything is serialized or woken, so a plugin that asked for one
    /// room, or that reads no messages, costs the others nothing.
    pub fn subscribers(&self, event: &EventEnvelope) -> Vec<PluginId> {
        let needed = crate::plugins::permissions::event_capability(event.kind);
        self.order
            .iter()
            .filter(|id| {
                self.entries.get(*id).is_some_and(|entry| {
                    entry.runs()
                        && entry.subscriptions.wants(event.kind)
                        && needed
                            .is_none_or(|capability| entry.manifest.permissions.allows(capability))
                        && entry.subscriptions.allows(event)
                })
            })
            .cloned()
            .collect()
    }

    /// The plugins a hook runs through, in activation order.
    ///
    /// A plugin takes part when its manifest holds the capability that hook
    /// needs: intercepting a send is not something a plugin can do by accident,
    /// and the person saw it in the review dialog.
    pub fn chain(&self, hook: HookKind) -> Vec<PluginId> {
        let capability = hook.capability();
        self.order
            .iter()
            .filter(|id| {
                self.entries.get(*id).is_some_and(|entry| {
                    entry.runs() && entry.manifest.permissions.allows(capability)
                })
            })
            .cloned()
            .collect()
    }

    /// Every plugin's state, for the page that lists them.
    /// Every command the enabled plugins with a module declare, in the order
    /// the plugins run.
    ///
    /// A plugin without a module can't answer one, so its commands aren't
    /// offered: the composer would send a `/` into nothing.
    pub fn commands(&self) -> Vec<(PluginId, yapfast_plugin_api::manifest::CommandDecl)> {
        self.order
            .iter()
            .filter_map(|plugin| self.entries.get(plugin))
            .filter(|entry| entry.enabled && entry.module.is_some())
            .flat_map(|entry| {
                entry
                    .manifest
                    .commands
                    .iter()
                    .cloned()
                    .map(|command| (entry.manifest.id.clone(), command))
            })
            .collect()
    }

    pub fn statuses(&self) -> Vec<(PluginId, PluginStatus)> {
        self.entries
            .iter()
            .map(|(id, entry)| (id.clone(), entry.status.clone()))
            .collect()
    }

    /// Whether anything is enabled.
    pub fn enabled(&self) -> bool {
        self.summary.enabled
    }

    /// Rebuilds the order and the summary after the enabled set changed.
    fn recompute(&mut self) {
        let nodes: Vec<GraphNode> = self
            .entries
            .values()
            .filter(|entry| entry.enabled)
            .map(node)
            .collect();
        self.order = match graph::layers(&nodes) {
            Ok(layers) => layers.into_iter().flatten().collect(),
            Err(error) => {
                // The graph refuses a missing or incompatible dependency, which
                // enabling should have prevented. Falling back to id order keeps
                // the plugins running, in an order the person can predict.
                tracing::warn!(%error, "the enabled plugins can't be ordered by their dependencies");
                nodes.iter().map(|node| node.id.clone()).collect()
            }
        };
        let mut summary = Summary::default();
        for entry in self.entries.values().filter(|entry| entry.runs()) {
            summary.enabled = true;
            // Only the kinds this plugin may actually see: a plugin that
            // subscribed to messages without `messages.read` must not make the
            // parent serialize every message for nothing.
            summary.wanted |=
                entry.subscriptions.mask() & permitted_mask(&entry.manifest.permissions);
            summary.intercepts |= entry
                .manifest
                .permissions
                .allows(Capability::MessagesIntercept);
        }
        self.summary = summary;
    }
}

/// The event kinds a manifest's capabilities let it receive, as one mask.
fn permitted_mask(permissions: &Permissions) -> u64 {
    EventKind::ALL
        .into_iter()
        .filter(|kind| {
            crate::plugins::permissions::event_capability(*kind)
                .is_none_or(|capability| permissions.allows(capability))
        })
        .map(Subscriptions::bit)
        .fold(0, |mask, bit| mask | bit)
}

/// One plugin as the dependency graph sees it.
fn node(entry: &Entry) -> GraphNode {
    GraphNode {
        id: entry.manifest.id.clone(),
        version: entry.manifest.version,
        dependencies: entry.manifest.dependencies.clone(),
        optional_dependencies: entry.manifest.optional_dependencies.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yapfast_plugin_api::event::Network;

    fn id(text: &str) -> PluginId {
        PluginId::parse(text).expect("a plugin id")
    }

    /// An entry as the registry holds one, with a module that only exists as a
    /// shape: what a plugin's subscriptions and capabilities are about.
    fn entry(manifest: &str) -> Entry {
        let manifest = PluginManifest::parse(manifest).expect("a manifest");
        Entry {
            subscriptions: Subscriptions::of(&manifest),
            enabled: true,
            status: PluginStatus::Enabled,
            module: Some(Module::stub()),
            manifest,
            topics: BTreeSet::new(),
            log: VecDeque::new(),
        }
    }

    fn registry(entries: &[&str]) -> Registry {
        let mut registry = Registry::default();
        for text in entries {
            let one = entry(text);
            registry.entries.insert(one.manifest.id.clone(), one);
        }
        registry.recompute();
        registry
    }

    fn event(kind: EventKind, room: Option<&str>, own: bool) -> EventEnvelope {
        EventEnvelope {
            kind,
            network: Network::Discord,
            at: 0,
            room: room.map(str::to_owned),
            author: None,
            own,
            payload: serde_json::Value::Null,
        }
    }

    #[test]
    fn every_kind_has_its_own_bit() {
        let mut seen = 0u64;
        for kind in EventKind::ALL {
            let bit = Subscriptions::bit(kind);
            assert_eq!(bit & seen, 0, "{kind} shares a bit with another kind");
            seen |= bit;
        }
        assert_eq!(seen.count_ones(), EventKind::ALL.len() as u32);
    }

    #[test]
    fn a_manifest_subscribes_through_its_event_triggers() {
        let one = entry(
            r#"{"api":1,"id":"com.example.events","name":"E","version":"1.0.0","wasm":"plugin.wasm",
                "activation":{"on":["event:message_create","event:typing","command"]}}"#,
        );
        assert!(one.subscriptions.wants(EventKind::MessageCreate));
        assert!(one.subscriptions.wants(EventKind::Typing));
        assert!(!one.subscriptions.wants(EventKind::Presence));
        assert!(one.subscriptions.wants(EventKind::MessageCreate));
    }

    #[test]
    fn the_summary_says_whether_an_event_has_any_subscriber() {
        let registry = registry(&[
            r#"{"api":1,"id":"com.example.a","name":"A","version":"1.0.0","wasm":"plugin.wasm",
                "permissions":{"capabilities":["messages.read"]},
                "activation":{"on":["event:message_create"]}}"#,
        ]);
        assert!(registry.summary().enabled);
        assert_ne!(
            registry.summary().wanted & Subscriptions::bit(EventKind::MessageCreate),
            0
        );
        assert_eq!(
            registry.summary().wanted & Subscriptions::bit(EventKind::AppFocus),
            0
        );
        assert!(!registry.summary().intercepts);
    }

    #[test]
    fn a_subscription_without_the_capability_is_wanted_by_nobody() {
        // Reading what people say is `messages.read`; a plugin that subscribed
        // without it must not make the parent serialize a single message.
        let registry = registry(&[
            r#"{"api":1,"id":"com.example.blind","name":"B","version":"1.0.0","wasm":"plugin.wasm",
                "activation":{"on":["event:message_create"]}}"#,
        ]);
        assert_eq!(
            registry.summary().wanted & Subscriptions::bit(EventKind::MessageCreate),
            0
        );
        assert!(
            registry
                .subscribers(&event(EventKind::MessageCreate, None, false))
                .is_empty()
        );
    }

    #[test]
    fn a_chain_runs_dependencies_first_and_only_interceptors() {
        let plugged = registry(&[
            r#"{"api":1,"id":"com.example.top","name":"T","version":"1.0.0","wasm":"plugin.wasm",
                "permissions":{"capabilities":["messages.intercept"]},
                "dependencies":{"com.example.base":"^1"}}"#,
            r#"{"api":1,"id":"com.example.base","name":"B","version":"1.2.0","wasm":"plugin.wasm",
                "permissions":{"capabilities":["messages.intercept"]}}"#,
            r#"{"api":1,"id":"com.example.quiet","name":"Q","version":"1.0.0","wasm":"plugin.wasm",
                "activation":{"on":["event:message_create"]}}"#,
        ]);
        assert_eq!(
            plugged.chain(HookKind::MessagePreSend),
            vec![id("com.example.base"), id("com.example.top")]
        );
        assert!(plugged.summary().intercepts);

        // Dependency order comes before id order: `aardvark` sorts first and
        // still runs after `zed`, which it depends on. Within a layer the chain
        // is by id, so `mid` sits between them.
        let ordered = registry(&[
            r#"{"api":1,"id":"com.example.aardvark","name":"A","version":"1.0.0","wasm":"plugin.wasm",
                "permissions":{"capabilities":["messages.intercept"]},
                "dependencies":{"com.example.zed":"^1"}}"#,
            r#"{"api":1,"id":"com.example.zed","name":"Z","version":"1.2.0","wasm":"plugin.wasm",
                "permissions":{"capabilities":["messages.intercept"]}}"#,
            r#"{"api":1,"id":"com.example.mid","name":"M","version":"1.0.0","wasm":"plugin.wasm",
                "permissions":{"capabilities":["messages.intercept"]}}"#,
        ]);
        assert_eq!(
            ordered.chain(HookKind::MessagePreSend),
            vec![
                id("com.example.mid"),
                id("com.example.zed"),
                id("com.example.aardvark")
            ],
            "dependencies first, then by id"
        );
    }

    #[test]
    fn only_subscribers_of_a_wanted_kind_are_woken() {
        let registry = registry(&[
            r#"{"api":1,"id":"com.example.typing","name":"T","version":"1.0.0","wasm":"plugin.wasm",
                "permissions":{"capabilities":["messages.read"]},
                "activation":{"on":["event:typing"]}}"#,
        ]);
        assert_eq!(
            registry.subscribers(&event(EventKind::Typing, Some("room-1"), false)),
            vec![id("com.example.typing")]
        );
        assert!(
            registry
                .subscribers(&event(EventKind::MessageCreate, Some("room-1"), false))
                .is_empty()
        );
    }

    #[test]
    fn a_disabled_plugin_is_woken_by_nothing() {
        let mut registry = registry(&[
            r#"{"api":1,"id":"com.example.off","name":"O","version":"1.0.0","wasm":"plugin.wasm",
                "activation":{"on":["event:message_create"]},
                "permissions":{"capabilities":["messages.intercept"]}}"#,
        ]);
        let plugin = id("com.example.off");
        registry
            .entries
            .get_mut(&plugin)
            .expect("the entry")
            .enabled = false;
        registry.recompute();
        assert!(!registry.summary().enabled);
        assert!(registry.chain(HookKind::MessagePreSend).is_empty());
        assert!(
            registry
                .subscribers(&event(EventKind::MessageCreate, None, false))
                .is_empty()
        );
    }

    #[test]
    fn the_log_ring_forgets_the_oldest_line() {
        let mut registry = registry(&[
            r#"{"api":1,"id":"com.example.log","name":"L","version":"1.0.0","wasm":"plugin.wasm"}"#,
        ]);
        let plugin = id("com.example.log");
        for index in 0..(LOG_RING + 5) {
            registry.note_log(
                &plugin,
                LogRecord {
                    level: yapfast_plugin_api::message::LogLevel::Info,
                    target: "plugin".to_owned(),
                    message: index.to_string(),
                    at: 0,
                },
            );
        }
        let entry = registry.entry(&plugin).expect("the entry");
        assert_eq!(entry.log.len(), LOG_RING);
        assert_eq!(
            entry.log.front().map(|line| line.message.as_str()),
            Some("5")
        );
    }

    /// A registry whose plugins are ordered by their dependencies, as a real
    /// one is after `load`.
    fn ordered(manifests: &[&str]) -> Registry {
        registry(manifests)
    }

    #[test]
    fn dependents_come_outermost_first_and_the_reverse_starts_them() {
        let registry = ordered(&[
            r#"{"api":1,"id":"b.base","name":"B","version":"1.0.0","wasm":"plugin.wasm"}"#,
            r#"{"api":1,"id":"c.mid","name":"C","version":"1.0.0","wasm":"plugin.wasm",
                "dependencies":{"b.base":"^1"}}"#,
            r#"{"api":1,"id":"z.top","name":"Z","version":"1.0.0","wasm":"plugin.wasm",
                "dependencies":{"c.mid":"^1"}}"#,
            r#"{"api":1,"id":"d.other","name":"D","version":"1.0.0","wasm":"plugin.wasm"}"#,
        ]);
        assert_eq!(
            registry.dependents(&id("b.base")),
            vec![id("z.top"), id("c.mid")],
            "the one furthest from it is stopped first, and started last"
        );
        assert!(registry.dependents(&id("d.other")).is_empty());
    }

    #[test]
    fn a_plugin_declares_its_dependencies_and_only_those() {
        let registry = ordered(&[
            r#"{"api":1,"id":"b.base","name":"B","version":"1.0.0","wasm":"plugin.wasm"}"#,
            r#"{"api":1,"id":"c.mid","name":"C","version":"1.0.0","wasm":"plugin.wasm",
                "optional_dependencies":{"b.base":"^1"}}"#,
        ]);
        assert!(registry.declares(&id("c.mid"), &id("b.base")));
        assert!(registry.callable(&id("b.base")));
        assert!(
            !registry.declares(&id("b.base"), &id("c.mid")),
            "an edge is declared by the plugin that calls along it"
        );
    }

    #[test]
    fn a_topic_reaches_whoever_asked_for_it_and_not_the_publisher() {
        let mut registry = ordered(&[
            r#"{"api":1,"id":"b.base","name":"B","version":"1.0.0","wasm":"plugin.wasm"}"#,
            r#"{"api":1,"id":"c.mid","name":"C","version":"1.0.0","wasm":"plugin.wasm"}"#,
        ]);
        registry
            .subscribe(&id("b.base"), "notes", true)
            .expect("a topic");
        assert_eq!(
            registry.subscribers_to("notes", &id("c.mid")),
            vec![id("b.base")]
        );
        assert!(
            registry.subscribers_to("notes", &id("b.base")).is_empty(),
            "a plugin doesn't hear its own topic"
        );
        registry
            .subscribe(&id("b.base"), "notes", false)
            .expect("a topic");
        assert!(registry.subscribers_to("notes", &id("c.mid")).is_empty());
    }

    #[test]
    fn a_topic_costs_a_name_of_its_own_shape() {
        let mut registry = ordered(&[
            r#"{"api":1,"id":"b.base","name":"B","version":"1.0.0","wasm":"plugin.wasm"}"#,
        ]);
        assert!(registry.subscribe(&id("b.base"), "", true).is_err());
        assert!(
            registry
                .subscribe(&id("b.base"), &"x".repeat(limits::MAX_TOPIC_NAME_BYTES + 1), true)
                .is_err()
        );
    }
}
