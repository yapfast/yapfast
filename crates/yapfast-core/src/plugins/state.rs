//! What the application holds while the plugin system is on, and nothing else.
//!
//! `AppState` carries one `Option<Box<PluginState>>`. With no plugin enabled it
//! is `None` and the taps in the send path, the event path and the menus cost
//! one `Option` check; the helper process doesn't exist and no thread or timer
//! runs on its behalf.
//!
//! The methods below are the whole interface between the application's actions
//! and the plugin bridge. Anything else the bridge needs — spawning the helper,
//! answering a guest's host call, evicting an idle instance — happens behind
//! them, in `bridge`, `spawn`, `registry` and `store`.

use std::path::Path;
use std::sync::Arc;

use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::CommandInvocation;
use yapfast_plugin_api::event::{EventEnvelope, EventKind};
use yapfast_plugin_api::hook::Draft;
use yapfast_plugin_api::manifest::CommandDecl;
use yapfast_plugin_api::message::PluginStatus;

use crate::plugins::bridge;

/// Everything the application knows about plugins.
pub struct PluginState {
    /// The bridge's half: the registry, the helper and its mailboxes.
    bridge: bridge::Bridge,
    /// What the person has allowed, which the gate asks and a page answers.
    grants: Arc<crate::plugins::grants::Grants>,
}

/// Something the plugin system did, for the application to act on.
///
/// The bridge hands these to the closure it was opened with, which the
/// application turns into `Event::Plugin`: that is how a hook's answer reaches
/// the composer without the application polling anything.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// A `message.pre_send` chain finished: send this draft.
    SendReady { id: u64, draft: Draft },
    /// A `message.pre_send` chain stopped the send, and why.
    SendBlocked { id: u64, reason: String },
    /// A plugin's state changed, for the page that lists them.
    Status {
        plugin: PluginId,
        status: PluginStatus,
    },
    /// A command answered with a line for the person, or failed with a reason.
    Command { plugin: PluginId, text: String },
    /// A fetch finished, for the page's Add from URL: the plugin it found, or
    /// the sentence saying why there is none.
    Fetched {
        url: String,
        result: Result<crate::plugins::installing::Fetched, String>,
    },
    /// A plugin wants a host the person hasn't allowed, so a page can ask.
    ///
    /// The gate asks rather than holding the request open, and the plugin asks
    /// again once the answer is in; without this the ask had nowhere to go and
    /// the plugin waited on a dialog that was never shown.
    Consent { plugin: PluginId, host: String },
}

/// What `pre_send` decided about an outgoing message.
pub enum PreSend {
    /// No plugin intercepts sends, or the chain already answered: send this
    /// draft unchanged, without waiting for anything.
    Send(Draft),
    /// Interceptors are running. The composer is cleared now and the send
    /// completes when the answer arrives as `Event::Plugin`; a spinner shows
    /// only past 100 ms.
    Held { id: u64 },
}

impl PluginState {
    /// Reads the store, the lock file and the enabled manifests, and starts
    /// nothing: the helper runs from the first activation onward.
    ///
    /// `wake` is called with every [`Notice`]; the application turns it into an
    /// event, so a hook's answer arrives without a poll.
    pub fn open(
        data_dir: &Path,
        version: &str,
        wake: Arc<dyn Fn(Notice) + Send + Sync>,
    ) -> anyhow::Result<Self> {
        let mut bridge = bridge::Bridge::open(data_dir, version, wake)?;
        // The gate is this side's to build. It holds the grants and opens the
        // socket, because the helper has neither a person to ask nor a client
        // to open one with; until it is installed here, every `http.fetch`
        // answers `unsupported` and no plugin can reach the network at all.
        let grants = Arc::new(crate::plugins::grants::Grants::load(
            data_dir.join("grants.json"),
        )?);
        let gate = Arc::new(
            crate::plugins::network::NetworkGate::new(
                Arc::clone(&grants),
                Arc::new(crate::plugins::transport::PinnedTransport),
                data_dir.join("plugins-http"),
            )
            .with_resolver(Arc::new(crate::plugins::network::SystemResolver)),
        );
        bridge.install_network(gate);
        Ok(PluginState { bridge, grants })
    }

    /// What the application knows, for it to publish into: the account, the
    /// lists, the open room's messages and members. The bridge answers a
    /// guest's reads from the same value, which is why it is cheap to publish
    /// into: an `Arc` clone per change, and nothing at all when nothing moved.
    pub fn world(&self) -> &Arc<crate::plugins::world::World> {
        self.bridge.world()
    }

    /// What the person has allowed, for the page that asks and revokes.
    pub fn grants(&self) -> &Arc<crate::plugins::grants::Grants> {
        &self.grants
    }

    /// Records the answer to a consent ask, so the plugin's next request is
    /// allowed or refused rather than waiting for a dialog nobody saw.
    ///
    /// The requests already waiting on that answer are let go here too: an
    /// allow makes its request again, and a refusal hands the guest the reason.
    pub fn answer_consent(
        &self,
        plugin: &PluginId,
        host: &yapfast_plugin_api::capability::HostPattern,
        allow: bool,
        lasts: crate::plugins::grants::Lasts,
    ) -> anyhow::Result<()> {
        self.grants.answer(plugin, host, allow, lasts)?;
        let waiting = self.bridge.answer_consent(plugin, host, allow);
        if waiting > 0 {
            tracing::debug!(
                %plugin,
                host = %host,
                waiting,
                "a consent answer let go the requests that were waiting on it"
            );
        }
        Ok(())
    }

    /// Takes a host back from a plugin: the gate refuses its next request for
    /// it, and the requests already waiting about it are refused now.
    pub fn revoke_host(
        &self,
        plugin: &PluginId,
        host: &yapfast_plugin_api::capability::HostPattern,
    ) -> anyhow::Result<()> {
        self.grants.revoke(plugin, host)?;
        self.bridge.refuse_host(plugin, host);
        Ok(())
    }

    /// Whether an enabled plugin intercepts outgoing messages.
    ///
    /// The send path asks this before it builds a draft, so a plugin system
    /// with nothing intercepting costs one atomic load per send rather than a
    /// draft, a copy of the composer's text and a frame.
    pub fn intercepts_sends(&self) -> bool {
        self.bridge.intercepts_sends()
    }

    /// Whether anything subscribed to this kind of event, so a tap that is
    /// about to build an envelope can ask first.
    pub fn subscribes(&self, kind: EventKind) -> bool {
        self.bridge.subscribes(kind)
    }

    /// The commands the enabled plugins declare, as the composer's `/` parser
    /// reads them. Cheap enough to ask per keystroke that starts a command.
    pub fn commands(&self) -> Vec<(PluginId, CommandDecl)> {
        self.bridge.commands()
    }

    /// The same, with each plugin's own name, for the composer's list: two
    /// plugins may declare `ping`, and so may a Discord app, so a row says which
    /// plugin it would run.
    pub fn commands_named(&self) -> Vec<(PluginId, String, CommandDecl)> {
        self.bridge.commands_named()
    }

    /// Runs one command: the helper calls the plugin, and its answer comes
    /// back as [`Notice::Command`].
    pub fn run_command(&mut self, plugin: &PluginId, invocation: CommandInvocation) {
        self.bridge.run_command(plugin, invocation);
    }

    /// Drops every instance now, keeping the helper: for a moment the parent
    /// knows is idle, such as the window going to the tray.
    pub fn evict(&mut self) {
        self.bridge.evict();
    }

    /// Whether any plugin is enabled. The tap points test this first.
    pub fn enabled(&self) -> bool {
        self.bridge.enabled()
    }

    /// Activates what must run at startup: plugins whose manifest says
    /// `startup`, and any that intercept outgoing messages, since the first
    /// message must not go out before they are ready.
    pub fn start(&mut self) {
        self.bridge.start();
    }

    /// Hands one event to the plugins subscribed to it.
    ///
    /// The event is serialized once, only if someone subscribed, and its filter
    /// is applied here rather than in the helper.
    pub fn observe(&mut self, event: EventEnvelope) {
        self.bridge.observe(event);
    }

    /// Runs the `message.pre_send` chain, if anything intercepts sends.
    pub fn pre_send(&mut self, draft: Draft) -> PreSend {
        self.bridge.pre_send(draft)
    }

    /// Runs the chain in front of an edit, which is its own set of plugins.
    pub fn pre_edit(&mut self, draft: Draft) -> PreSend {
        self.bridge.pre_edit(draft)
    }

    /// Turns a plugin on or off: the registry entry, the helper's instances and
    /// the interface all follow, without a restart.
    pub fn set_enabled(&mut self, plugin: &PluginId, enabled: bool) -> anyhow::Result<()> {
        self.bridge.set_enabled(plugin, enabled)
    }

    /// Every plugin's state, for the page that lists them.
    pub fn statuses(&self) -> Vec<(PluginId, PluginStatus)> {
        self.bridge.statuses()
    }

    /// One plugin's name as its manifest writes it, for the dialog that asks
    /// about it: a person answers about a name, not about an id.
    pub fn name(&self, plugin: &PluginId) -> Option<String> {
        self.bridge.name(plugin)
    }

    /// Every installed plugin's manifest, state and recent log, for the page
    /// that lists them.
    pub fn listed(&self) -> Vec<crate::plugins::registry::Listed> {
        self.bridge.listed()
    }

    /// Reads a plugin out of its repository, for the page: the answer comes back
    /// as [`Notice::Fetched`], so the window never waits.
    pub fn fetch(&mut self, url: String) {
        self.bridge.fetch(url);
    }

    /// Writes a fetched plugin into the store, once the person accepted its
    /// review, and tells the page what the store holds now.
    pub fn install(&mut self, package: &crate::plugins::store::Package) -> anyhow::Result<()> {
        self.bridge.install(package)
    }

    /// Removes a plugin, and its storage and settings when `delete_data`.
    pub fn uninstall(&mut self, plugin: &PluginId, delete_data: bool) -> anyhow::Result<()> {
        self.bridge.uninstall(plugin, delete_data)
    }

    /// Goes back to the version the last install replaced.
    pub fn roll_back(&mut self, plugin: &PluginId) -> anyhow::Result<()> {
        self.bridge.roll_back(plugin)
    }

    /// What Settings → Plugins shows: a copy of what the runtime knows, so
    /// the page reads it without touching a lock.
    pub fn page(&self) -> crate::plugins::page::View {
        self.bridge.page(&self.grants)
    }

    /// What the helper process is doing, for the page that says so.
    pub fn helper(&self) -> crate::plugins::bridge::HelperState {
        self.bridge.helper()
    }

    /// Why the plugin system is off, when it is.
    pub fn off(&self) -> Option<String> {
        self.bridge.off()
    }

    /// Stops the helper, if it is running. Called when the application exits.
    pub fn shutdown(&mut self) {
        self.bridge.shutdown();
    }
}
