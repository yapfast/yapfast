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

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{CommandInvocation, SettingsNews, WidgetInput};
use yapfast_plugin_api::event::{EventEnvelope, EventKind};
use yapfast_plugin_api::hook::{AttachmentPending, Draft};
use yapfast_plugin_api::manifest::CommandDecl;
use yapfast_plugin_api::message::PluginStatus;

use crate::plugins::bridge;
use crate::plugins::settings;

/// Everything the application knows about plugins.
pub struct PluginState {
    /// The bridge's half: the registry, the helper and its mailboxes.
    bridge: bridge::Bridge,
    /// What the person has allowed, which the gate asks and a page answers.
    grants: Arc<crate::plugins::grants::Grants>,
    /// The plugins' settings, written on a thread of its own so the interface
    /// never waits on the disk. A change is also handed to the instance that
    /// asked to hear of one, which is how a setting takes effect with no
    /// restart.
    settings: settings::Store,
    /// What a plugin's form refused, by plugin and by field, until that field
    /// changes. A refused value is never stored, so the page's own copy of the
    /// form — read back from the store — would otherwise forget why it refused.
    refused: BTreeMap<PluginId, BTreeMap<String, settings::Refused>>,
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
    /// A `typing.pre_send` chain let a typing indicator go: the application
    /// sends it now, which is the only way a plugin may withhold one without
    /// the composer waiting.
    Typing { id: u64, room: String },
    /// An `attachment.pre_upload` chain answered about a file the composer is
    /// taking: the name the upload should go under, or the reason it doesn't.
    Attachment {
        id: u64,
        name: Option<String>,
        blocked: Option<String>,
    },
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
    /// The marketplace was read, or why it couldn't be: its listings and
    /// icons, which are held only while the page that asked is open.
    Catalog {
        source: String,
        result: Result<Box<crate::plugins::marketplace::Catalog>, String>,
    },
    /// A plugin wants a host the person hasn't allowed, so a page can ask.
    ///
    /// The gate asks rather than holding the request open, and the plugin asks
    /// again once the answer is in; without this the ask had nowhere to go and
    /// the plugin waited on a dialog that was never shown.
    Consent { plugin: PluginId, host: String },
    /// What a plugin drew in a slot, or under a message, for the interface to
    /// keep and show. The runtime coalesces these, so a plugin that draws once
    /// per event costs one update per frame at most.
    Slot(crate::plugins::slots::Output),
    /// Take away everything one plugin drew, because it was turned off or
    /// suspended: a plugin that isn't running shows nothing.
    SlotsGone(PluginId),
    /// Take away everything every plugin drew, because the store changed under
    /// them: an install, an uninstall or a roll back.
    SlotsReset,
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
        Ok(PluginState {
            bridge,
            grants,
            settings: settings::Store::new(data_dir.join("plugins")),
            refused: BTreeMap::new(),
        })
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

    /// Applies one change to a plugin's settings, stores what changed, and
    /// tells a running instance through `settings.on_change`.
    ///
    /// Answers the keys whose values changed: none for an edit the form refused,
    /// or one that changed nothing, which is what a hidden or disabled setting
    /// gets. Nothing restarts.
    pub fn set_setting(&mut self, plugin: &PluginId, edit: settings::Edit) -> Vec<String> {
        let Some(mut form) = self.bridge.form(plugin) else {
            return Vec::new();
        };
        // A refusal from the last edit of this plugin's form comes with it, so
        // the page keeps saying why until that field changes.
        form.refused = self.refused.get(plugin).cloned().unwrap_or_default();
        let changed = form.edit(edit);
        if form.refused.is_empty() {
            self.refused.remove(plugin);
        } else {
            self.refused.insert(plugin.clone(), form.refused.clone());
        }
        if changed.is_empty() {
            return changed;
        }
        // The disk is the store's thread's business; the running instance hears
        // now, so a setting takes effect at once rather than at the next start.
        self.settings.save(plugin.as_str(), &form.values);
        let held = form.values.to_json();
        let values: BTreeMap<String, Value> = changed
            .iter()
            .filter_map(|key| Some((key.clone(), held.get(key)?.clone())))
            .collect();
        self.bridge.settings_news(
            plugin,
            SettingsNews {
                changed: values,
                widget: None,
            },
        );
        changed
    }

    /// Hands the input of a widget one of its settings draws to a running
    /// instance, the way a changed value is handed to it.
    ///
    /// A widget setting holds no value of its own, so nothing is stored: what
    /// the plugin does with the input is its own business.
    pub fn setting_widget(&mut self, plugin: &PluginId, key: &str, id: &str, value: &str) {
        self.bridge.settings_news(
            plugin,
            SettingsNews {
                changed: BTreeMap::new(),
                widget: Some(WidgetInput {
                    setting: key.to_owned(),
                    id: id.to_owned(),
                    value: value.to_owned(),
                }),
            },
        );
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

    /// Whether an enabled plugin draws under messages, which a visible row
    /// checks before it costs anything.
    pub fn decorates(&self) -> bool {
        self.bridge.decorates()
    }

    /// Asks each of them what it draws under the rows on screen.
    pub fn decorate(&mut self, rows: &[yapfast_plugin_api::message::DecorateRequest]) {
        self.bridge.decorate(rows);
    }

    /// Runs the `message.pre_send` chain, if anything intercepts sends.
    pub fn pre_send(&mut self, draft: Draft) -> PreSend {
        self.bridge.pre_send(draft)
    }

    /// Runs the chain in front of an edit, which is its own set of plugins.
    pub fn pre_edit(&mut self, draft: Draft) -> PreSend {
        self.bridge.pre_edit(draft)
    }

    /// Runs the chain in front of a typing indicator, and answers the id it is
    /// held under when a plugin may withhold it.
    pub fn typing(&mut self, room: &str) -> Option<u64> {
        self.bridge.typing(room)
    }

    /// Runs the chain in front of an upload, and answers the id it is held
    /// under when a plugin may rename or withhold it.
    pub fn attachment(&mut self, pending: AttachmentPending) -> Option<u64> {
        self.bridge.attachment(pending)
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

    /// Installs a plugin from a folder and watches it, which is the development
    /// loop: a save in that folder reloads it.
    pub fn load_folder(&mut self, folder: &Path) -> anyhow::Result<PluginId> {
        self.bridge.load_folder(folder)
    }

    /// Reads a plugin out of its repository, for the page: the answer comes back
    /// as [`Notice::Fetched`], so the window never waits.
    pub fn fetch(&mut self, url: String) {
        self.bridge.fetch(url);
    }

    /// Reads the marketplace's registry for the page: the answer comes back as
    /// [`Notice::Catalog`], so the window never waits on it either.
    pub fn catalog(&mut self, url: String) {
        self.bridge.catalog(url);
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
    /// the page reads it without touching a lock. What a form refused comes
    /// with it, because a refused value is never stored and the page's copy of
    /// the form is read back from the store.
    pub fn page(&self) -> crate::plugins::page::View {
        let mut view = self.bridge.page(&self.grants);
        for installed in &mut view.installed {
            let Ok(id) = PluginId::parse(&installed.id) else {
                continue;
            };
            if let Some(refused) = self.refused.get(&id) {
                installed.settings.refused = refused.clone();
            }
        }
        view
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
