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

use std::collections::{BTreeMap, VecDeque};

use yapfast_plugin_api::capability::{Capability, Permissions};
use yapfast_plugin_api::event::{EventEnvelope, EventKind, SubscriptionFilter};
use yapfast_plugin_api::graph::{self, GraphNode};
use yapfast_plugin_api::hook::HookKind;
use yapfast_plugin_api::message::{LogRecord, PluginStatus};
use yapfast_plugin_api::{PluginId, PluginManifest};

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
    pub fn set_enabled(
        &mut self,
        plugin: &PluginId,
        on: bool,
        store: &Store,
    ) -> anyhow::Result<()> {
        if !self.entries.contains_key(plugin) {
            anyhow::bail!("{plugin} isn't installed");
        }
        let turning_on: Vec<PluginId> = if on {
            let nodes: Vec<GraphNode> = self.entries.values().map(node).collect();
            graph::closure(plugin, &nodes).map_err(|error| anyhow::anyhow!("{error}"))?
        } else {
            vec![plugin.clone()]
        };
        for id in &turning_on {
            store.set_enabled(id, on)?;
        }
        for id in &turning_on {
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
        Ok(())
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
}
