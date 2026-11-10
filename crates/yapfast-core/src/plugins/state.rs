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
    pub fn open(data_dir: &Path, wake: Arc<dyn Fn(Notice) + Send + Sync>) -> anyhow::Result<Self> {
        Ok(PluginState {
            bridge: bridge::Bridge::open(data_dir, wake)?,
        })
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

    /// Turns a plugin on or off: the registry entry, the helper's instances and
    /// the interface all follow, without a restart.
    pub fn set_enabled(&mut self, plugin: &PluginId, enabled: bool) -> anyhow::Result<()> {
        self.bridge.set_enabled(plugin, enabled)
    }

    /// Every plugin's state, for the page that lists them.
    pub fn statuses(&self) -> Vec<(PluginId, PluginStatus)> {
        self.bridge.statuses()
    }

    /// Stops the helper, if it is running. Called when the application exits.
    pub fn shutdown(&mut self) {
        self.bridge.shutdown();
    }
}
