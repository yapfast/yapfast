//! The application's side of the plugin system: what the interface asks the
//! plugins to do, what they report back, and where the events the application
//! already sees reach the plugins that subscribed to them.
//!
//! Nothing here runs while no plugin is enabled. [`super::AppState::plugins`]
//! is `None` then, which is the one check every tap below makes first, and the
//! helper process doesn't exist at all. A tap that costs more than that —
//! building an event, serializing a message — happens only past it.

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{CommandInvocation, HostReturn, ReactionCount};
use yapfast_plugin_api::event::{EventEnvelope, EventKind, Network as PluginNetwork, RoomView};
use yapfast_plugin_api::hook::{AttachmentPending, Draft};

use yapfast_plugin_api::manifest::{CommandDecl, OptionKind};
use yapfast_plugin_api::message::PluginStatus;

use super::{Action as AppAction, Application, Compose, Effect as AppEffect};
use crate::chat::Command;
use crate::chat::Event;
use crate::network::Network;
use crate::plugins::PluginState;
use crate::plugins::state::{Notice, PreSend};

/// Something a person or a plugin asks the plugin system to do.
pub enum Action {
    /// Turns a plugin on, which reads its manifest, grants what it asks for
    /// and starts its instance, or off, which drops it; no restart either way.
    SetEnabled { plugin: PluginId, enabled: bool },
    /// Answers a plugin's request to reach a site nobody has allowed: the
    /// dialog's two buttons and its "Don't ask again".
    AnswerConsent {
        plugin: PluginId,
        host: String,
        allow: bool,
        /// Written to `grants.json`, rather than kept for this run.
        remember: bool,
    },
    /// Runs a command a person typed in the composer, as the manifest
    /// declares it.
    ///
    /// The plugin system has no call that runs one yet, so this reaches the
    /// application and stops there: see `Application::plugins_run`.
    Command {
        plugin: PluginId,
        invocation: CommandInvocation,
    },
    /// The `message.pre_send` chain answered the send a hook held: this draft
    /// goes out in the conversation it names.
    SendHeld(Draft),
    /// The person answered a powerful capability's first-use dialog; `folder`
    /// is the one they picked for `fs.folder`.
    AnswerPowerful {
        plugin: PluginId,
        capability: yapfast_plugin_api::capability::Capability,
        allow: bool,
        folder: Option<std::path::PathBuf>,
    },
    /// The switch in Advanced that lets any plugin use a powerful capability.
    SetPowerful(bool),
    /// Stop that plugin sending as an agent: the indicator's own button, which
    /// takes `messages.send.agent` back.
    StopAgent(PluginId),
    /// A web link the person clicked: the plugins that offer to open links on
    /// its host are asked, and while any is, [`AppState::plugin_link`] says so
    /// and the leaving dialog shows what they offer.
    AskLink(String),
    /// The leaving dialog closed, and what was offered for its link goes.
    LinkClosed,
    /// Pictures from the web that plugins' drawings on screen show, by plugin
    /// id and address: each is fetched once, through its plugin's gate.
    WantPictures(Vec<(String, String)>),
    /// The save dialog a plugin's `files.save` opened was closed: the file name
    /// the person saved under, nothing when they cancelled, or why writing it
    /// failed. The folder never travels back to the plugin.
    Saved {
        plugin: PluginId,
        call: u64,
        saved: Result<Option<String>, String>,
    },
}

/// Something the plugin system did, for the interface to show.
pub enum Effect {
    /// A plugin's state changed: installed, enabled, active, faulted or
    /// suspended. The page that lists them redraws its row.
    Status {
        plugin: PluginId,
        status: PluginStatus,
    },
    /// The page's own state moved — a fetch finished, a review opened, an
    /// install wrote something — so it draws itself again. Nothing a plugin did.
    Page,
    /// A plugin asks to reach a site: the dialog to draw, or `None` once it has
    /// its answer. It is the application's own state, so the question outlives
    /// Settings, and it is asked wherever the person is.
    Consent(Option<ConsentAsk>),
    /// A plugin drew something in a slot or under a message, or what it drew
    /// was taken away: the interface shows the slots that moved, and no others.
    Slots,
    /// How the person on the profile card looks changed, so the card is drawn
    /// again from what the application now holds.
    Identity,
    /// What a plugin's overlay shows changed: the window draws the layer
    /// again, and its pictures are read once each.
    Overlay,
    /// A plugin coloured a code block, or stopped colouring it: a row that shows
    /// the block lays its text out again, and draws the block in the tones the
    /// application keeps for it.
    CodeBlocks,
    /// The enabled plugins changed — one was turned on or off, installed,
    /// updated or removed — so the interface builds its plugins' colours,
    /// parts and names again from [`Application::plugin_interfaces`].
    Interface,
    /// A plugin asks the person to save a file: the window opens the system's
    /// save dialog, titled with the plugin's name, writes the bytes where the
    /// person chose, and answers with [`Action::Saved`].
    SaveFile(SaveAsk),
}

/// A powerful capability's first use, waiting for the person's answer in a red
/// dialog of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerfulAsk {
    pub plugin: PluginId,
    /// The manifest's name, which the dialog shows.
    pub name: String,
    pub capability: yapfast_plugin_api::capability::Capability,
    /// What it would be able to do, in plain words.
    pub sentence: String,
}

pub use account::ActivityShown;

/// A plugin sending as an agent, for the indicator the window shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentShown {
    pub plugin: PluginId,
    pub name: String,
    /// When it last sent, in milliseconds since the Unix epoch.
    pub at: u64,
}

impl AgentShown {
    /// How long the indicator stays after the last send.
    pub const SHOWN_MS: u64 = 2 * 60 * 1000;

    /// Whether it sent recently enough to show.
    pub fn recent(&self, now: u64) -> bool {
        now.saturating_sub(self.at) < Self::SHOWN_MS
    }
}

/// A link on its way to the leaving dialog, the plugins asked about it, and
/// what they offered: each a button beside Visit site, named as theirs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkAsk {
    pub url: String,
    /// The questions still out, by the ids the bridge answered.
    pub ids: Vec<u64>,
    pub offers: Vec<LinkOfferShown>,
}

/// One app a plugin offers to open a link in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkOfferShown {
    pub plugin: PluginId,
    /// The manifest's name, which the dialog shows under the button.
    pub name: String,
    pub label: String,
    pub uri: String,
}

impl LinkAsk {
    /// The most offers the dialog shows, so no crowd of plugins pushes its
    /// own buttons away.
    pub const MOST_OFFERS: usize = 3;
}

/// The quick switcher's places in one plugin's order, for one search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitcherRanking {
    pub plugin: PluginId,
    /// The manifest's name, which the switcher's foot shows.
    pub name: String,
    pub query: String,
    /// The places it kept, by id, in its order.
    pub order: Vec<String>,
}

/// The words a plugin adds after the window's title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleShown {
    pub plugin: PluginId,
    pub name: String,
    pub suffix: String,
}

/// A plugin's sound playing, for the small line that says whose it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayingShown {
    pub plugin: PluginId,
    pub name: String,
    /// When it started, in milliseconds since the Unix epoch.
    pub at: u64,
}

impl PlayingShown {
    /// A plugin's sound lasts at most ten seconds; the line stays as long.
    pub const SHOWN_MS: u64 = 10 * 1000;

    pub fn recent(&self, now: u64) -> bool {
        now.saturating_sub(self.at) < Self::SHOWN_MS
    }
}

/// A file a plugin hands the person, waiting for the save dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveAsk {
    /// The plugin that asked, for the answer to go back to.
    pub plugin: PluginId,
    /// The guest's own number for the call, which the answer goes back under.
    pub call: u64,
    /// The name the dialog's title shows, which is the manifest's and not the
    /// id: the person sees whose file they are saving.
    pub plugin_name: String,
    /// The file name the dialog suggests, already checked to be no path.
    pub name: String,
    pub bytes: Arc<[u8]>,
}

/// A plugin waiting for the person to say whether it may reach a site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsentAsk {
    /// The plugin that asked, for the answer to go back to.
    pub plugin: PluginId,
    /// The name the dialog shows, which is the manifest's and not the id.
    pub name: String,
    /// The host, as the request named it and the grant would be written.
    pub host: String,
}

/// What the plugin system tells the application, arriving as
/// [`crate::chat::Event::Plugin`] by the same path as every other event.
pub enum News {
    /// From the bridge: a hook's answer, or a plugin's state.
    Notice(Notice),
    /// From a tap: something the plugins subscribed to saw.
    Observed(EventEnvelope),
}

mod account;
pub(crate) mod calls;
mod compose;
mod events;
pub(crate) mod page;

impl Application {
    /// Reads the plugin store and starts what must run at the start of a
    /// session, which is what a plugin that intercepts outgoing messages
    /// must: the first message can't go out before it is ready.
    ///
    /// The data directory is where the store and the enabled manifests are.
    /// `events` is the stream a notice goes into as
    /// [`crate::chat::Event::Plugin`], which is how a hook's answer comes
    /// back: the application doesn't poll for one. It outlives a session, so
    /// the window opens the system once and hands it the same stream again
    /// when a session after a sign-out opens it again.
    ///
    /// The bridge runs the helper as a task on the plugin system's own runtime,
    /// so this can be called from anywhere. The window calls it when a session starts
    /// and again when the page asks for something with no system open, so a
    /// plugin installed with nothing running installs rather than being
    /// refused; with it open already, this is one check and opens nothing.
    /// Before it, and in a build or a run without plugins, every tap is one
    /// check of [`super::AppState::plugins`].
    /// `version` is the program's own version, which a plugin reads as
    /// `app.version`; only the root package knows it.
    pub fn open_plugins(
        &mut self,
        data_dir: &Path,
        version: &str,
        events: UnboundedSender<Event>,
    ) -> Vec<AppEffect> {
        // `--no-plugins` reads nothing and starts nothing, as a build without
        // the feature does.
        if !crate::cli::plugins_allowed() {
            return vec![];
        }
        // The bridge's work runs on a runtime of its own, made on first use
        // (`crate::plugins::tasks`), and nothing here blocks on a runtime, so
        // this may be called with another runtime's handle entered, as iced
        // enters the background runtime around every `update`.
        let wake: Arc<dyn Fn(Notice) + Send + Sync> = Arc::new(move |notice| {
            let _ = events.send(Event::Plugin(News::Notice(notice)));
        });
        let mut plugins = match PluginState::open(data_dir, version, wake) {
            Ok(plugins) => plugins,
            Err(error) => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "could not read the plugin store; starting without plugins"
                );
                // The page says what can't be done, and this says why.
                self.state.notice = Some(format!("The plugin system couldn't start: {error:#}"));
                return vec![];
            }
        };
        plugins.start();
        let statuses = plugins.statuses();
        let watching = plugins.enabled();
        self.state.plugins = Some(Box::new(plugins));
        self.watch_gateway(watching);
        let mut effects: Vec<AppEffect> = statuses
            .into_iter()
            .map(|(plugin, status)| AppEffect::Plugin(Effect::Status { plugin, status }))
            .collect();
        // What the plugins installed from Settings change in the window, which
        // the interface builds once it has the store to read them from.
        effects.extend(self.plugins_interface_moved());
        effects
    }

    /// Whether the plugin system is on, which every tap asks before it builds
    /// anything: one check of an `Option` and one of a field.
    fn plugins_on(&self) -> bool {
        self.state
            .plugins
            .as_ref()
            .is_some_and(|plugins| plugins.enabled())
    }

    /// Tells the Discord gateway's tap whether a plugin may be watching, and
    /// which kinds one subscribed to.
    ///
    /// The gateway reports every message and reaction the account may read,
    /// whether or not a conversation shows it, so that tap is the only place
    /// a plugin hears of a conversation that isn't on screen. It is off
    /// unless the plugin system says otherwise, which keeps that path at one
    /// relaxed load, and the mask it sends along keeps an event nobody
    /// subscribed to at one more.
    fn watch_gateway(&self, watching: bool) {
        #[cfg(feature = "discord")]
        crate::discord_text::watch_messages(watching, self.plugin_kinds());
        #[cfg(not(feature = "discord"))]
        let _ = watching;
    }

    /// The kinds of event the plugins that run subscribed to, as the bridge's
    /// own one-bit-per-kind mask.
    ///
    /// It is read from the same number the bridge reads before it writes an
    /// event, so the gateway builds a payload for exactly the kinds that would
    /// be handed on. The enabled set is the only thing that changes it, and the
    /// two places that change that call [`Self::watch_gateway`].
    #[cfg(feature = "discord")]
    fn plugin_kinds(&self) -> u64 {
        let Some(plugins) = self.state.plugins.as_deref() else {
            return 0;
        };
        EventKind::ALL
            .into_iter()
            .filter(|kind| plugins.subscribes(*kind))
            .map(crate::plugins::registry::Subscriptions::bit)
            .fold(0, |mask, bit| mask | bit)
    }

    /// Carries out what the interface asked of the plugin system.
    pub(super) fn plugins_action(&mut self, action: Action) -> Vec<AppEffect> {
        match action {
            Action::SetEnabled { plugin, enabled } => self.plugins_set_enabled(plugin, enabled),
            Action::AnswerConsent {
                plugin,
                host,
                allow,
                remember,
            } => self.plugins_answer_consent(&plugin, &host, allow, remember),
            Action::Command { plugin, invocation } => self.plugins_run(plugin, invocation),
            Action::SendHeld(draft) => self.plugins_send_held(draft),
            Action::AnswerPowerful {
                plugin,
                capability,
                allow,
                folder,
            } => {
                if self
                    .state
                    .plugin_powerful
                    .as_ref()
                    .is_some_and(|ask| ask.plugin == plugin && ask.capability == capability)
                {
                    self.state.plugin_powerful = None;
                }
                if let Some(plugins) = self.state.plugins.as_mut()
                    && let Err(error) = plugins.answer_powerful(&plugin, capability, allow, folder)
                {
                    self.state.notice = Some(format!("Couldn't keep that answer: {error:#}"));
                }
                vec![AppEffect::Plugin(Effect::Page)]
            }
            Action::SetPowerful(enabled) => {
                if let Some(plugins) = self.state.plugins.as_mut()
                    && let Err(error) = plugins.set_powerful_enabled(enabled)
                {
                    self.state.notice = Some(format!("Couldn't change that: {error:#}"));
                }
                vec![AppEffect::Plugin(Effect::Page)]
            }
            Action::StopAgent(plugin) => {
                if let Some(plugins) = self.state.plugins.as_mut()
                    && let Err(error) = plugins.revoke_powerful(
                        &plugin,
                        yapfast_plugin_api::capability::Capability::MessagesSendAgent,
                    )
                {
                    self.state.notice = Some(format!("Couldn't stop it: {error:#}"));
                }
                if self
                    .state
                    .plugin_agent
                    .as_ref()
                    .is_some_and(|shown| shown.plugin == plugin)
                {
                    self.state.plugin_agent = None;
                }
                vec![AppEffect::Plugin(Effect::Page)]
            }
            Action::Saved {
                plugin,
                call,
                saved,
            } => {
                self.plugin_saves.remove(&plugin);
                let answer = match saved {
                    Ok(name) => match serde_json::to_value(yapfast_plugin_api::call::FileSaved {
                        saved: name.is_some(),
                        name,
                    }) {
                        Ok(value) => HostReturn::Ok(value),
                        Err(error) => HostReturn::Error {
                            message: error.to_string(),
                        },
                    },
                    Err(message) => HostReturn::Error { message },
                };
                if let Some(plugins) = self.state.plugins.as_mut() {
                    plugins.answer_call(&plugin, call, answer);
                }
                Vec::new()
            }
            Action::AskLink(url) => {
                self.state.plugin_link = None;
                if let Some(plugins) = self.state.plugins.as_mut() {
                    let ids = plugins.link_open(&url);
                    if !ids.is_empty() {
                        self.state.plugin_link = Some(LinkAsk {
                            url,
                            ids,
                            offers: Vec::new(),
                        });
                    }
                }
                Vec::new()
            }
            Action::LinkClosed => {
                self.state.plugin_link = None;
                Vec::new()
            }
            Action::WantPictures(wanted) => {
                if let Some(plugins) = self.state.plugins.as_mut() {
                    plugins.want_pictures(&wanted);
                }
                Vec::new()
            }
        }
    }

    /// Asks the plugin that ranks the quick switcher about what it found for
    /// the search now in it, when one does.
    pub(in crate::application) fn plugins_rank_switcher(&mut self) {
        let Some(plugins) = self.state.plugins.as_ref() else {
            return;
        };
        if !plugins.ranks_switcher() {
            return;
        }
        let Some(query) = self.state.switcher.clone() else {
            return;
        };
        use yapfast_plugin_api::hook::{SwitcherPending, SwitcherPlace};
        let places: Vec<SwitcherPlace> = self
            .state
            .switcher_places()
            .iter()
            .take(SwitcherPending::MAX_PLACES)
            .map(|place| match place {
                crate::application::Place::Room(room) => SwitcherPlace {
                    id: room.id.clone(),
                    name: room.name.clone(),
                    kind: if room.direct {
                        "dm"
                    } else if room.voice {
                        "voice"
                    } else {
                        "channel"
                    }
                    .to_owned(),
                },
                crate::application::Place::Server(server) => SwitcherPlace {
                    id: server.id.clone(),
                    name: server.name.clone(),
                    kind: "server".to_owned(),
                },
            })
            .collect();
        if let Some(plugins) = self.state.plugins.as_mut() {
            plugins.switcher_rank(SwitcherPending { query, places });
        }
    }

    /// Asks the window to open a save dialog for a plugin's `files.save`, or
    /// answers why not: one dialog per plugin at a time.
    pub(super) fn plugins_save_file(
        &mut self,
        plugin: PluginId,
        call: u64,
        name: String,
        bytes: Vec<u8>,
    ) -> Vec<AppEffect> {
        if self.plugin_saves.contains(&plugin) {
            if let Some(plugins) = self.state.plugins.as_mut() {
                plugins.answer_call(
                    &plugin,
                    call,
                    HostReturn::Denied {
                        reason: format!(
                            "{plugin} already has a save dialog open: one at a time, and the \
                             person answers it first"
                        ),
                    },
                );
            }
            return Vec::new();
        }
        let plugin_name = self
            .state
            .plugins
            .as_ref()
            .and_then(|plugins| plugins.name(&plugin))
            .unwrap_or_else(|| plugin.to_string());
        self.plugin_saves.insert(plugin.clone());
        vec![AppEffect::Plugin(Effect::SaveFile(SaveAsk {
            plugin,
            call,
            plugin_name,
            name,
            bytes: bytes.into(),
        }))]
    }

    /// Turns a plugin on or off. The state it settles in comes back as a
    /// [`Notice::Status`], which the page shows, so nothing is guessed here.
    fn plugins_set_enabled(&mut self, plugin: PluginId, enabled: bool) -> Vec<AppEffect> {
        let (result, watching) = match self.state.plugins.as_mut() {
            Some(plugins) => (plugins.set_enabled(&plugin, enabled), plugins.enabled()),
            None => return vec![],
        };
        if let Err(error) = result {
            self.state.notice = Some(format!(
                "Couldn't {} {plugin}: {error}",
                if enabled { "enable" } else { "disable" }
            ));
            return vec![];
        }
        // Turning the last plugin off takes the gateway's tap with it.
        self.watch_gateway(watching);
        // Turning a plugin off closes its dialog: the request it was asking
        // about has already been refused, and the question is no longer one.
        if !enabled
            && self
                .state
                .plugin_consent
                .as_ref()
                .is_some_and(|ask| ask.plugin == plugin)
        {
            return self.close_consent();
        }
        vec![]
    }

    /// Starts a plugin again: a development folder's from its folder, which
    /// reads any change in it, and any other by turning it off and on, which
    /// clears a suspension.
    pub(super) fn plugins_restart(&mut self, plugin: &str) -> Vec<AppEffect> {
        let Ok(id) = PluginId::parse(plugin) else {
            return Vec::new();
        };
        let folder = self.state.plugins.as_ref().and_then(|plugins| {
            plugins
                .package_folder(plugin)
                .map(std::path::Path::to_owned)
        });
        if let (Some(folder), Some(plugins)) = (folder, self.state.plugins.as_mut()) {
            if let Err(error) = plugins.load_folder(&folder) {
                self.state.notice = Some(format!("Couldn't reload {plugin}: {error:#}"));
            }
            return vec![AppEffect::Plugin(Effect::Page)];
        }
        let mut effects = self.plugins_set_enabled(id.clone(), false);
        effects.extend(self.plugins_set_enabled(id, true));
        effects.push(AppEffect::Plugin(Effect::Page));
        effects
    }

    /// Answers a plugin's request to reach a host, which is what completes the
    /// `http.fetch` kept waiting for it.
    ///
    /// The answer is kept first and the requests let go second: a request let go
    /// before its answer is written asks the same question again.
    fn plugins_answer_consent(
        &mut self,
        plugin: &PluginId,
        host: &str,
        allow: bool,
        remember: bool,
    ) -> Vec<AppEffect> {
        let pattern = match yapfast_plugin_api::capability::HostPattern::parse(host) {
            Ok(pattern) => pattern,
            Err(error) => {
                self.state.notice = Some(format!("Couldn't answer about {host}: {error}"));
                return self.close_consent();
            }
        };
        let lasts = if remember {
            crate::plugins::grants::Lasts::Stored
        } else {
            crate::plugins::grants::Lasts::Session
        };
        if let Some(plugins) = self.state.plugins.as_ref()
            && let Err(error) = plugins.answer_consent(plugin, &pattern, allow, lasts)
        {
            self.state.notice = Some(format!("Couldn't answer about {host}: {error:#}"));
        }
        self.close_consent()
    }

    /// Takes the ask off the screen, because it has its answer or because
    /// nothing is left to ask about it.
    pub(super) fn close_consent(&mut self) -> Vec<AppEffect> {
        if self.state.plugin_consent.take().is_some() {
            return vec![AppEffect::Plugin(Effect::Consent(None))];
        }
        vec![]
    }

    /// Runs a command the plugin system declares.
    ///
    /// The answer arrives as [`Notice::Command`] on the application's own event
    /// path, which is why nothing is returned here: the composer is already
    /// empty by the time the plugin replies.
    fn plugins_run(
        &mut self,
        plugin: PluginId,
        mut invocation: CommandInvocation,
    ) -> Vec<AppEffect> {
        // A command is typed in the room on screen, which is the one its
        // answer goes to.
        if invocation.room.is_none() {
            invocation.room.clone_from(&self.state.selected);
        }
        if let Some(plugins) = self.state.plugins.as_mut() {
            plugins.run_command(&plugin, invocation);
        }
        vec![]
    }

    /// Tells the plugins whether the interface is dark, which `app.theme_mode`
    /// answers. The theme is the window's, so this is called from there.
    pub fn plugins_theme(&mut self, dark: bool) {
        if let Some(plugins) = self.state.plugins.as_ref() {
            plugins.world().publish_theme(dark);
        }
    }

    /// The network of the conversation on screen, for the events that are
    /// about Yapfast rather than about a room: a plugin watching one network
    /// still hears that the window lost focus.
    fn on_screen_network(&self) -> PluginNetwork {
        self.state.selected.as_deref().map_or_else(
            || {
                plugin_network(Network::of(
                    self.state.account.as_deref().unwrap_or_default(),
                ))
            },
            |room| plugin_network(Network::of(room)),
        )
    }
}

/// The commands the enabled plugins declare, which a `/` in the composer is
/// matched against.
///
/// The network an event came from, as the plugin API names it.
fn plugin_network(network: Network) -> PluginNetwork {
    match network {
        Network::Discord => PluginNetwork::Discord,
        Network::Matrix => PluginNetwork::Matrix,
    }
}

/// The server a room id belongs to, as the plugin API names one: nothing for a
/// DM or a room no server holds, which is one answer for that person
/// everywhere.
fn plugin_server(room: &str) -> Option<String> {
    crate::discord::parse_room(room)
        .map(|(server, _)| server.to_owned())
        .filter(|server| server != crate::discord::DM_GUILD)
}

/// Draws a profile as a plugin says its person looks.
///
/// A field the plugin left out stays as Yapfast had it, which is why every
/// surface can apply an answer over what it already knows. The name colour is a
/// style of one colour, the way the profile editor writes one.
fn decorate_profile(
    profile: &mut crate::chat::Profile,
    identity: &yapfast_plugin_api::identity::Identity,
) {
    if let Some(name) = &identity.display_name {
        profile.name.clone_from(name);
    }
    if let Some(colour) = identity.name_colour {
        profile.style = Some(Arc::new(crate::chat::NameStyle::new(0, 1, &[colour])));
    }
    if let Some(avatar) = &identity.avatar {
        profile.avatar = Some(avatar.clone());
    }
    if let Some(banner) = &identity.banner {
        profile.banner = Some(banner.clone());
    }
    if let Some(decoration) = &identity.decoration {
        profile.decoration = Some(Arc::from(decoration.as_str()));
    }
    if let Some(pronouns) = &identity.pronouns {
        profile.pronouns.clone_from(pronouns);
    }
}

/// The interface redraws the slots plugins draw in, or nothing does.
///
/// A tree that came again unchanged, or a slot emptied twice, draws nothing: the
/// interface is left alone until something it shows has changed.
fn redraw_if(changed: bool) -> Vec<AppEffect> {
    if changed {
        vec![AppEffect::Plugin(Effect::Slots)]
    } else {
        Vec::new()
    }
}

/// The interface redraws the profile card, because how its person looks has
/// changed under it.
fn redraw_identity() -> Vec<AppEffect> {
    vec![AppEffect::Plugin(Effect::Identity)]
}

/// The interface lays a row's text out again, because a code block on screen
/// was coloured, coloured differently, or lost its colours.
fn redraw_code_blocks_if(changed: bool) -> Vec<AppEffect> {
    if changed {
        vec![AppEffect::Plugin(Effect::CodeBlocks)]
    } else {
        Vec::new()
    }
}

/// One event, with its payload written out here: the bridge hands the same
/// bytes to every subscriber, so this is the only place it's written.
pub(crate) fn envelope(
    kind: EventKind,
    network: PluginNetwork,
    room: Option<String>,
    author: Option<String>,
    own: bool,
    payload: Value,
) -> EventEnvelope {
    EventEnvelope {
        kind,
        network,
        at: crate::clock::now_ms(),
        room,
        author,
        own,
        payload,
    }
}

/// A Discord message as a plugin sees it, and whether the account wrote it.
#[cfg(feature = "discord")]
pub(crate) fn message_view(
    discord: &yapfast_discord::service::Discord,
    room: &str,
    message: &yapfast_discord::message::Message,
) -> (yapfast_plugin_api::event::MessageView, bool) {
    use yapfast_plugin_api::event::{MessageView, PersonView};
    let snapshot = discord.snapshots().borrow().clone();
    let me = snapshot.user_id.clone().unwrap_or_default();
    let guild = crate::discord::parse_room(room)
        .map_or(yapfast_discord::ready::PRIVATE, |(guild, _)| guild);
    let server = snapshot.servers.iter().find(|server| server.id == guild);
    let own = message.author_id == me;
    let author = PersonView {
        id: message.author_id.clone(),
        name: discord
            .name(guild, &message.author_id)
            .unwrap_or_else(|| message.author.clone()),
        display_name: None,
        own,
        bot: message.bot,
    };
    // The message as the timeline names it, `d:<id>`, which is what
    // `messages.get`, a decoration, a menu row and every call about a message
    // use, so one id goes everywhere.
    let view = MessageView {
        id: format!("d:{}", message.id),
        room: room.to_owned(),
        author,
        // As Discord wrote it, its mentions still as `<@id>`: a plugin that
        // wants names calls `people.get`, rather than guessing at Markdown.
        body: message.content.clone(),
        at: message.timestamp() * 1000,
        edited: message.edited,
        deleted: false,
        // Each file with its address, which `media.get` reads once Yapfast
        // has the bytes: a plugin gets no token, and the address is Discord's
        // signed CDN link, as the message itself carries it.
        attachments: message
            .attachments
            .iter()
            .enumerate()
            .map(|(index, file)| yapfast_plugin_api::event::AttachmentView {
                id: format!("{}:{index}", message.id),
                name: file.name.clone(),
                bytes: file.size.unwrap_or_default(),
                media: file.url.to_string(),
                content_type: content_type(&file.name),
            })
            .collect(),
        reply_to: match &message.reply {
            Some(yapfast_discord::message::Reply::Message { id, .. }) => Some(format!("d:{id}")),
            Some(yapfast_discord::message::Reply::Unknown { id }) if !id.is_empty() => {
                Some(format!("d:{id}"))
            }
            _ => None,
        },
        mentions_me: message.mentions_me(&me, server.map(|server| &*server.marks)),
    };
    (view, own)
}

/// The command `/name option…` names among those `declared`, with its options
/// as the manifest types them.
///
/// Options are `key=value` pairs separated by spaces, and a value may be
/// quoted to keep its spaces: `/note text="two words"`. An option the
/// declaration doesn't list, one whose value isn't its type, or a required one
/// left out leaves the whole line as text, because a `/` the composer doesn't
/// understand has always gone to the conversation as it was written.
fn parse_command(
    text: &str,
    declared: &[(PluginId, CommandDecl)],
) -> Option<(PluginId, CommandInvocation)> {
    let rest = text.trim_start().strip_prefix('/')?;
    let (name, rest) = match rest.find(char::is_whitespace) {
        Some(cut) => (&rest[..cut], &rest[cut..]),
        None => (rest, ""),
    };
    let (plugin, command) = declared.iter().find(|(_, command)| command.name == name)?;
    let mut options = serde_json::Map::new();
    for (key, value) in options_of(rest)? {
        let declared = command.options.iter().find(|option| option.name == key)?;
        if options.contains_key(key) {
            return None;
        }
        options.insert(key.to_owned(), typed(value, declared.kind)?);
    }
    if command
        .options
        .iter()
        .any(|option| option.required && !options.contains_key(&option.name))
    {
        return None;
    }
    Some((
        plugin.clone(),
        CommandInvocation {
            name: command.name.clone(),
            options,
            room: None,
        },
    ))
}

/// The `key=value` pairs written after a command's name, with the quotes taken
/// off a value that has them.
fn options_of(mut rest: &str) -> Option<Vec<(&str, &str)>> {
    let mut pairs = Vec::new();
    rest = rest.trim();
    while !rest.is_empty() {
        let (key, after) = rest.split_once('=')?;
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            return None;
        }
        let after = after.trim_start();
        if let Some(quoted) = after.strip_prefix('"') {
            let (value, after) = quoted.split_once('"')?;
            pairs.push((key, value));
            rest = after.trim_start();
        } else {
            rest = match after.find(char::is_whitespace) {
                Some(cut) => {
                    pairs.push((key, &after[..cut]));
                    &after[cut..]
                }
                None => {
                    pairs.push((key, after));
                    ""
                }
            }
            .trim_start();
        }
    }
    Some(pairs)
}

/// One option's value, as the type its declaration gives it.
fn typed(value: &str, kind: OptionKind) -> Option<Value> {
    match kind {
        OptionKind::String | OptionKind::Choice | OptionKind::Person | OptionKind::Channel => {
            Some(Value::String(value.to_owned()))
        }
        OptionKind::Bool => match value.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" => Some(Value::Bool(true)),
            "false" | "no" | "off" => Some(Value::Bool(false)),
            _ => None,
        },
        OptionKind::Integer => value.parse::<i64>().ok().map(Value::from),
        OptionKind::Number => value
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number),
    }
}

/// The type a file names, from its extension, for the
/// `attachment.pre_upload` chain: the same name the network would guess, said
/// once here so a plugin can decide without reading the file.
pub(crate) fn content_type(name: &str) -> Option<String> {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())?;
    let kind = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "zip" => "application/zip",
        _ => return None,
    };
    Some(kind.to_owned())
}

/// A plugin's picture where the reader that prepares an emoji or a sticker
/// takes one: a file, as a person's own picker hands it over, removed as soon
/// as it's read.
#[cfg(feature = "discord")]
fn with_brought_picture<T>(
    image: &[u8],
    read: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "yapfast-plugin-{}-{}.{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        // The reader makes the picture's name part of what it says when it
        // refuses one, so the name says what the picture is.
        match image {
            [0x89, b'P', b'N', b'G', ..] => "png",
            [b'G', b'I', b'F', ..] => "gif",
            [0xff, 0xd8, ..] => "jpg",
            [
                b'R',
                b'I',
                b'F',
                b'F',
                _,
                _,
                _,
                _,
                b'W',
                b'E',
                b'B',
                b'P',
                ..,
            ] => "webp",
            _ => "img",
        }
    ));
    std::fs::write(&path, image)
        .map_err(|error| format!("could not read that picture: {error}"))?;
    let read = read(&path);
    let _ = std::fs::remove_file(&path);
    read
}

/// A call the application carried out, from what it came to.
fn answered(outcome: Result<Vec<AppEffect>, String>) -> (Vec<AppEffect>, HostReturn) {
    match outcome {
        Ok(effects) => (effects, HostReturn::Ok(serde_json::Value::Bool(true))),
        Err(message) => (Vec::new(), HostReturn::Error { message }),
    }
}

/// The answer to a plugin that named something this account can't see.
fn not_listed(place: &str) -> HostReturn {
    HostReturn::Error {
        message: format!("{place} isn't a conversation or server this account can see"),
    }
}

/// The name a renamed upload goes under: the last part of what the chain
/// answered, so a plugin can't send a file called `../../x`, and the file's own
/// name when what it answered has no usable part.
fn upload_name(answered: &str, fallback: &str) -> String {
    let last = answered
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim();
    if last.is_empty() || last == "." || last == ".." {
        return fallback.to_owned();
    }
    // A name is bytes on every filesystem Yapfast writes to, and one answer is
    // one frame: a plugin can't hand over a name no file can have.
    let mut end = last.len().min(255);
    while end > 0 && !last.is_char_boundary(end) {
        end -= 1;
    }
    last[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{Action as AppAction, preview::preview_room};
    use crate::chat::{Command, RoomIndex};
    use yapfast_plugin_api::manifest::CommandOption;

    /// `files.save` opens one dialog per plugin at a time, titled with the
    /// plugin, and its answer names the file and never the folder.
    #[test]
    fn a_plugin_s_save_opens_one_named_dialog_at_a_time() {
        let mut app = connected();
        let plugin = yapfast_plugin_api::PluginId::parse("dev.local.saver").expect("an id");
        let effects = app.plugins_save_file(plugin.clone(), 7, "notes.txt".into(), b"hi".to_vec());
        let [AppEffect::Plugin(Effect::SaveFile(ask))] = effects.as_slice() else {
            panic!("the window is asked to open the dialog");
        };
        assert_eq!(ask.call, 7);
        assert_eq!(ask.name, "notes.txt");
        assert_eq!(&ask.bytes[..], b"hi");
        assert!(
            !ask.plugin_name.is_empty(),
            "the dialog names whose file it is"
        );
        assert!(
            app.plugins_save_file(plugin.clone(), 8, "again.txt".into(), Vec::new())
                .is_empty(),
            "a second dialog while the first is open is refused"
        );
        app.plugins_action(Action::Saved {
            plugin: plugin.clone(),
            call: 7,
            saved: Ok(Some("notes.txt".into())),
        });
        assert_eq!(
            app.plugins_save_file(plugin, 9, "later.txt".into(), Vec::new())
                .len(),
            1,
            "once it closed, the plugin may ask again"
        );
    }

    /// An application whose one conversation is open and may take a message.
    fn connected() -> Application {
        let mut app = Application::default();
        app.apply(Event::Connected {
            user: "@alice:test".into(),
            trusted: true,
            storage: "test",
        });
        app.apply(Event::Sync(true));
        app.apply(Event::Rooms(RoomIndex {
            rooms: vec![preview_room("dm", "DM", true, 2)],
            servers: vec![],
        }));
        app.apply(Event::Timeline {
            room: "dm".into(),
            messages: vec![],
        });
        app
    }

    /// A call the application carries out, answered with what it did.
    #[test]
    fn a_plugin_sends_a_message_and_keeps_the_persons_draft() {
        let mut app = connected();
        app.state.drafts.insert("dm".into(), "mine".into());
        let plugin = yapfast_plugin_api::PluginId::parse("dev.local.sender").expect("an id");
        let (effects, answer) = app.plugins_call(
            &plugin,
            crate::plugins::state::AppCall::Send {
                room: "dm".into(),
                body: "from the plugin".into(),
                files: Vec::new(),
                made: Vec::new(),
                agent: false,
            },
        );
        assert_eq!(sent(&effects).as_deref(), Some("from the plugin"));
        assert!(
            matches!(answer, yapfast_plugin_api::call::HostReturn::Ok(_)),
            "the plugin is told it went"
        );
        assert_eq!(
            app.state.drafts.get("dm").map(String::as_str),
            Some("mine"),
            "what the person was writing is still there"
        );
    }

    /// A call the application carries out, answered with what it did.
    #[test]
    fn a_plugin_reads_and_writes_the_composer_and_opens_a_page() {
        let mut app = connected();
        app.state.drafts.insert("dm".into(), "hello".into());
        let plugin = yapfast_plugin_api::PluginId::parse("dev.local.sender").expect("an id");

        let (_, answer) = app.plugins_call(
            &plugin,
            crate::plugins::state::AppCall::GetDraft { room: "dm".into() },
        );
        assert_eq!(
            answer,
            yapfast_plugin_api::call::HostReturn::Ok(serde_json::Value::String("hello".into())),
            "the draft as the person left it"
        );

        app.plugins_call(
            &plugin,
            crate::plugins::state::AppCall::Insert {
                room: "dm".into(),
                text: " there".into(),
            },
        );
        assert_eq!(
            app.state.drafts.get("dm").map(String::as_str),
            Some("hello there")
        );

        app.plugins_call(
            &plugin,
            crate::plugins::state::AppCall::SetDraft {
                room: "dm".into(),
                body: "replaced".into(),
            },
        );
        assert_eq!(
            app.state.drafts.get("dm").map(String::as_str),
            Some("replaced")
        );

        let (effects, _) = app.plugins_call(
            &plugin,
            crate::plugins::state::AppCall::Copy {
                text: "copied".into(),
            },
        );
        assert!(
            matches!(effects.as_slice(), [AppEffect::CopyText(text)] if text == "copied"),
            "the clipboard is the application's to write"
        );
        let (effects, _) = app.plugins_call(
            &plugin,
            crate::plugins::state::AppCall::Open {
                url: "https://example.com/".into(),
            },
        );
        assert!(
            matches!(effects.as_slice(), [AppEffect::ConfirmOpen { url, .. }] if url.ends_with("example.com/")),
            "the person is asked before a plugin's link opens"
        );
    }

    /// What a plugin adds to the title, the switcher's order and a link's
    /// offers are kept only while they are about what is on screen.
    #[test]
    fn the_title_the_switcher_and_a_links_offers_keep_to_what_is_on_screen() {
        let mut app = connected();
        let one = yapfast_plugin_api::PluginId::parse("dev.local.one").expect("an id");
        let two = yapfast_plugin_api::PluginId::parse("dev.local.two").expect("an id");
        let title = |plugin: &yapfast_plugin_api::PluginId, suffix: Option<&str>| {
            Event::Plugin(News::Notice(Notice::WindowTitle {
                plugin: plugin.clone(),
                suffix: suffix.map(str::to_owned),
            }))
        };
        app.apply(title(&one, Some("3 unread")));
        app.apply(title(&two, Some("mine")));
        assert_eq!(
            app.state
                .plugin_title
                .as_ref()
                .map(|shown| shown.suffix.as_str()),
            Some("3 unread"),
            "the first plugin keeps the title"
        );
        app.apply(title(&one, None));
        assert!(app.state.plugin_title.is_none());

        app.state.switcher = Some("ge".into());
        app.apply(Event::Plugin(News::Notice(Notice::SwitcherRanked {
            plugin: one.clone(),
            query: "old".into(),
            order: vec!["a".into()],
        })));
        assert!(app.state.plugin_switcher.is_none(), "about another search");
        app.apply(Event::Plugin(News::Notice(Notice::SwitcherRanked {
            plugin: one.clone(),
            query: "ge".into(),
            order: vec!["a".into()],
        })));
        assert_eq!(
            app.state
                .plugin_switcher
                .as_ref()
                .map(|ranked| ranked.order.clone()),
            Some(vec!["a".to_owned()])
        );

        app.apply(Event::Plugin(News::Notice(Notice::LinkOffer {
            id: 4,
            plugin: one.clone(),
            label: "Open in Spotify".into(),
            uri: "spotify:track:1".into(),
        })));
        assert!(app.state.plugin_link.is_none(), "no dialog is asking");
        app.state.plugin_link = Some(LinkAsk {
            url: "https://open.spotify.com/track/1".into(),
            ids: vec![4],
            offers: Vec::new(),
        });
        for id in [5, 4] {
            app.apply(Event::Plugin(News::Notice(Notice::LinkOffer {
                id,
                plugin: one.clone(),
                label: "Open in Spotify".into(),
                uri: "spotify:track:1".into(),
            })));
        }
        let offers = &app.state.plugin_link.as_ref().expect("asking").offers;
        assert_eq!(offers.len(), 1, "only the question this dialog asked");
        assert_eq!(offers[0].name, "dev.local.one");
        app.dispatch(AppAction::Plugin(Action::LinkClosed));
        assert!(app.state.plugin_link.is_none());
    }

    /// The text of the message an effect sends, if one does.
    /// The account's calls that Discord or Spotify answer go to the account's
    /// session as one ask each, and are answered later; without an account
    /// they are answered now, with why.
    #[test]
    fn an_account_call_is_asked_of_discord_and_answered_later() {
        use crate::chat::{DiscordCommand, PluginAsk};
        use crate::plugins::state::AppCall;
        let plugin = yapfast_plugin_api::PluginId::parse("dev.local.spotify").expect("an id");
        let mut app = connected();
        let effects = app.plugins_ask(
            plugin.clone(),
            4,
            AppCall::Spotify(yapfast_plugin_api::account::SpotifyAction::Next),
        );
        assert!(
            effects.is_empty(),
            "no Discord account, nothing asked: {}",
            effects.len()
        );
        app.state.discord_user = Some("1".into());
        let activity = yapfast_plugin_api::account::ActivitySet {
            name: "Pomodoro".into(),
            ..Default::default()
        };
        let effects = app.plugins_ask(
            plugin.clone(),
            5,
            AppCall::SetPluginActivity {
                activity: Some(Box::new(activity)),
            },
        );
        let [
            AppEffect::Backend(Command::Discord(DiscordCommand::PluginAsk {
                plugin: asked,
                call: 5,
                ask,
            })),
        ] = effects.as_slice()
        else {
            panic!("one ask of the session");
        };
        assert!(matches!(**ask, PluginAsk::Activity(Some(_))));
        assert_eq!(asked, plugin.as_str());
        assert_eq!(
            app.state
                .plugin_activity
                .as_ref()
                .map(|shown| shown.name.as_str()),
            Some("Pomodoro"),
            "the window can say which plugin shows it"
        );
        // A plugin that stops takes its activity with it.
        let gone = app.plugins_activity_gone(&plugin);
        assert!(matches!(
            gone,
            Some(AppEffect::Backend(Command::Discord(
                DiscordCommand::PluginAsk { ask, .. }
            ))) if matches!(*ask, PluginAsk::Activity(None))
        ));
        assert!(app.state.plugin_activity.is_none());
        // Only the custom status: the status goes the person's way and the
        // profile isn't touched.
        let effects = app.plugins_ask(
            plugin.clone(),
            6,
            AppCall::SetProfile(yapfast_plugin_api::account::ProfileSet {
                custom_status: Some(yapfast_plugin_api::account::CustomStatusSet {
                    text: "focusing".into(),
                    emoji: None,
                }),
                ..Default::default()
            }),
        );
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                AppEffect::Backend(Command::Discord(DiscordCommand::SetCustomStatus(Some(_))))
            )),
            "the status is set"
        );
        assert!(!effects.iter().any(|effect| matches!(
            effect,
            AppEffect::Backend(Command::Discord(DiscordCommand::PluginAsk { .. }))
        )));
    }

    #[test]
    fn drafts_are_read_without_the_empty_ones() {
        let mut app = connected();
        app.state
            .drafts
            .insert("dm".into(), "half a thought".into());
        app.state.drafts.insert("other".into(), "  ".into());
        let plugin = yapfast_plugin_api::PluginId::parse("dev.local.drafts").expect("an id");
        let (_, answer) = app.plugins_call(&plugin, crate::plugins::state::AppCall::Drafts);
        let HostReturn::Ok(drafts) = answer else {
            panic!("{answer:?}");
        };
        assert_eq!(
            drafts,
            serde_json::json!([{ "room": "dm", "body": "half a thought" }])
        );
    }

    fn sent(effects: &[AppEffect]) -> Option<String> {
        effects.iter().find_map(|effect| match effect {
            AppEffect::Backend(Command::Send { body, .. }) => Some(body.clone()),
            _ => None,
        })
    }

    /// A draft as a chain answers with it.
    fn draft(body: &str) -> Draft {
        Draft {
            room: "dm".into(),
            body: body.into(),
            reply_to: None,
            attachments: Vec::new(),
        }
    }

    #[test]
    fn a_send_with_no_interceptor_is_unchanged() {
        let mut app = connected();
        app.dispatch(AppAction::SetDraft("hello".into()));
        let effects = app.dispatch(AppAction::Send);
        assert_eq!(sent(&effects).as_deref(), Some("hello"));
        assert_eq!(app.state.sending.as_deref(), Some("dm"));
    }

    #[test]
    fn a_held_send_goes_when_the_chain_answers() {
        let mut app = connected();
        app.dispatch(AppAction::SetDraft("hello".into()));
        assert!(app.plugins_send("dm").is_none(), "nothing intercepts here");
        let held = app.plugins_answer("dm", PreSend::Held { id: 1 });
        assert_eq!(held.map(|effects| effects.len()), Some(0));
        assert_eq!(app.state.draft(), "", "the composer empties at once");
        assert!(
            app.dispatch(AppAction::Send).is_empty(),
            "one send at a time"
        );
        let effects = app.apply(Event::Plugin(News::Notice(Notice::SendReady {
            id: 1,
            draft: draft("hello there"),
        })));
        assert_eq!(sent(&effects).as_deref(), Some("hello there"));
        app.apply(Event::SendResult {
            room: "dm".into(),
            body: "hello there".into(),
            result: Ok(()),
        });
        assert_eq!(app.state.draft(), "");
        assert!(app.state.sending.is_none());
    }

    #[test]
    fn a_blocked_send_does_not_send_and_says_why() {
        let mut app = connected();
        app.dispatch(AppAction::SetDraft("hello".into()));
        app.plugins_answer("dm", PreSend::Held { id: 2 });
        let effects = app.apply(Event::Plugin(News::Notice(Notice::SendBlocked {
            id: 2,
            reason: "That word isn't allowed here.".into(),
        })));
        assert!(sent(&effects).is_none());
        assert_eq!(
            app.state.notice.as_deref(),
            Some("That word isn't allowed here.")
        );
        assert_eq!(app.state.draft(), "hello", "the text comes back");
        assert!(app.state.sending.is_none(), "the room may send again");
    }

    #[test]
    fn a_send_the_person_typed_over_keeps_their_text() {
        let mut app = connected();
        app.dispatch(AppAction::SetDraft("hello".into()));
        app.plugins_answer("dm", PreSend::Held { id: 3 });
        app.dispatch(AppAction::SetDraft("what I said next".into()));
        let effects = app.apply(Event::Plugin(News::Notice(Notice::SendReady {
            id: 3,
            draft: draft("hello"),
        })));
        assert_eq!(sent(&effects).as_deref(), Some("hello"));
        assert_eq!(app.state.draft(), "what I said next");
    }

    /// The text of the edit an effect saves, if one does.
    fn edited(effects: &[AppEffect]) -> Option<String> {
        effects.iter().find_map(|effect| match effect {
            AppEffect::Backend(Command::Edit { body, .. }) => Some(body.clone()),
            _ => None,
        })
    }

    #[test]
    fn an_edit_chain_that_answers_at_once_saves_its_text() {
        let mut app = with_message(a_message("m1"));
        app.dispatch(AppAction::Edit("m1".into()));
        assert_eq!(
            app.state.edit().map(|edit| edit.text.as_str()),
            Some("something")
        );
        let answer = app.plugins_edit_answer(PreSend::Send(draft("something else")));
        assert!(answer.is_none(), "the edit goes on as its answer says");
        assert_eq!(
            app.state.edit().map(|edit| edit.text.as_str()),
            Some("something else"),
            "the chain's text is what the edit holds"
        );
        let effects = app.dispatch(AppAction::SaveEdit);
        assert_eq!(edited(&effects).as_deref(), Some("something else"));
    }

    #[test]
    fn a_held_edit_goes_when_the_chain_answers() {
        let mut app = with_message(a_message("m1"));
        app.dispatch(AppAction::Edit("m1".into()));
        let held = app.plugins_edit_answer(PreSend::Held { id: 4 });
        assert_eq!(held.map(|effects| effects.len()), Some(0));
        assert_eq!(
            app.state.edit().map(|edit| edit.text.as_str()),
            Some("something"),
            "the edit stays as it was while the chain runs"
        );
        assert!(
            app.dispatch(AppAction::SaveEdit).is_empty(),
            "one save at a time"
        );
        let effects = app.apply(Event::Plugin(News::Notice(Notice::SendReady {
            id: 4,
            draft: draft("something else"),
        })));
        assert_eq!(edited(&effects).as_deref(), Some("something else"));
        assert!(app.state.sending.is_some(), "the network has it now");
        assert_eq!(app.state.draft(), "", "the room's own draft is untouched");
        app.apply(Event::EditResult {
            room: "dm".into(),
            item: "m1".into(),
            result: Ok(()),
        });
        assert!(app.state.edit().is_none(), "the saved edit closes");
        assert!(app.state.sending.is_none());
    }

    #[test]
    fn a_blocked_edit_stays_open_and_says_why() {
        let mut app = with_message(a_message("m1"));
        app.dispatch(AppAction::Edit("m1".into()));
        app.plugins_edit_answer(PreSend::Held { id: 5 });
        let effects = app.apply(Event::Plugin(News::Notice(Notice::SendBlocked {
            id: 5,
            reason: "That word isn't allowed here.".into(),
        })));
        assert!(effects.is_empty());
        assert_eq!(
            app.state.notice.as_deref(),
            Some("That word isn't allowed here.")
        );
        assert_eq!(
            app.state.edit().map(|edit| edit.text.as_str()),
            Some("something"),
            "the edit is still there to change"
        );
        assert!(app.state.sending.is_none(), "the edit may be saved again");
    }

    #[test]
    fn a_slash_command_parses_into_the_invocation_its_manifest_declares() {
        let commands = [(
            PluginId::parse("com.example.dice").unwrap(),
            CommandDecl {
                name: "roll".into(),
                description: Some("Rolls dice.".into()),
                options: vec![
                    CommandOption {
                        name: "sides".into(),
                        kind: OptionKind::Integer,
                        required: true,
                        description: None,
                        choices: Vec::new(),
                    },
                    CommandOption {
                        name: "note".into(),
                        kind: OptionKind::String,
                        required: false,
                        description: None,
                        choices: Vec::new(),
                    },
                ],
            },
        )];
        let (plugin, invocation) =
            parse_command("/roll sides=20 note=\"two words\"", &commands).unwrap();
        assert_eq!(plugin.as_str(), "com.example.dice");
        assert_eq!(invocation.name, "roll");
        assert_eq!(invocation.options["sides"], serde_json::json!(20));
        assert_eq!(invocation.options["note"], serde_json::json!("two words"));
        // Without options the required one is missing, so it isn't one.
        assert!(parse_command("/roll", &commands).is_none());
    }

    #[test]
    fn a_slash_that_names_no_command_stays_text() {
        let commands = [(
            PluginId::parse("com.example.dice").unwrap(),
            CommandDecl {
                name: "roll".into(),
                description: None,
                options: vec![CommandOption {
                    name: "sides".into(),
                    kind: OptionKind::Integer,
                    required: true,
                    description: None,
                    choices: Vec::new(),
                }],
            },
        )];
        // An unknown name, a value of the wrong type, and no slash at all.
        assert!(parse_command("/nothing", &commands).is_none());
        assert!(parse_command("/roll sides=many", &commands).is_none());
        assert!(parse_command("roll sides=20", &commands).is_none());
        // The composer leaves it alone: nothing claims it.
        let mut app = connected();
        app.dispatch(AppAction::SetDraft("/nothing".into()));
        assert!(app.plugins_typed_command("dm").is_none());
        assert_eq!(app.state.draft(), "/nothing");
    }

    #[test]
    fn a_quoted_option_keeps_its_spaces_and_bare_ones_end_at_a_space() {
        assert_eq!(
            options_of(" a=1 b=\"two words\" c=3 ").unwrap(),
            [("a", "1"), ("b", "two words"), ("c", "3")]
        );
        assert_eq!(options_of("").unwrap(), Vec::new());
        assert!(options_of("bare").is_none());
    }

    /// iced runs `update` with the background runtime entered, and a session
    /// that starts there (a first sign-in) opens the plugin system from
    /// inside it: it opens, and closing it again doesn't panic either.
    #[test]
    fn the_plugin_system_opens_with_a_runtime_entered() {
        let directory = tempfile::tempdir().expect("a data directory");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("a runtime");
        let _entered = runtime.enter();
        let mut app = Application::default();
        let (events, _received) = tokio::sync::mpsc::unbounded_channel();
        app.open_plugins(directory.path(), "0.3.0", events);
        assert!(
            app.state.plugins.is_some(),
            "the system opens inside an entered runtime"
        );
        app.state.plugins = None;
    }

    /// A person's answer reaches the runtime, closes the question, and is what
    /// the gate finds when the plugin asks again.
    #[test]
    fn answering_the_dialog_records_the_answer_and_closes_the_question() {
        let directory = tempfile::tempdir().expect("a data directory");
        let mut app = Application::default();
        let (events, _received) = tokio::sync::mpsc::unbounded_channel();
        app.open_plugins(directory.path(), "0.3.0", events);
        let plugin = PluginId::parse("com.example.weather").expect("a plugin id");
        let host = "api.example.com";
        app.state.plugin_consent = Some(ConsentAsk {
            plugin: plugin.clone(),
            name: "Weather".to_owned(),
            host: host.to_owned(),
        });

        let effects = app.dispatch(AppAction::Plugin(Action::AnswerConsent {
            plugin: plugin.clone(),
            host: host.to_owned(),
            allow: true,
            remember: true,
        }));

        assert!(
            app.state.plugin_consent.is_none(),
            "the question has its answer"
        );
        assert!(
            matches!(
                effects.as_slice(),
                [AppEffect::Plugin(Effect::Consent(None))]
            ),
            "the dialog is taken away"
        );
        let hosts = yapfast_plugin_api::capability::HostPattern::parse(host).expect("a host");
        assert_eq!(
            app.state
                .plugins
                .as_ref()
                .expect("the plugin system is open")
                .grants()
                .ask(&plugin, &hosts),
            crate::plugins::grants::Ask::Allowed,
            "the answer is what the gate finds on the plugin's next request"
        );
    }

    /// A conversation whose timeline holds `message`, with a Discord account
    /// linked behind it.
    fn with_message(message: crate::chat::ChatMessage) -> Application {
        let mut app = connected();
        app.state.discord_user = Some("1".into());
        app.apply(Event::Timeline {
            room: "dm".into(),
            messages: vec![Arc::new(message)],
        });
        app
    }

    /// A message of the open conversation, as a timeline sends one.
    fn a_message(item: &str) -> crate::chat::ChatMessage {
        crate::chat::ChatMessage {
            id: item.into(),
            item: item.into(),
            author: "Bob".into(),
            body: "something".into(),
            can_reply: true,
            can_edit: true,
            can_react: true,
            ..Default::default()
        }
    }

    fn plugin() -> yapfast_plugin_api::PluginId {
        yapfast_plugin_api::PluginId::parse("dev.local.actions").expect("an id")
    }

    /// §17.2's presence group: what a plugin sets is what the person's own
    /// switches set.
    #[test]
    fn a_plugins_presence_goes_where_the_persons_own_switches_go() {
        let mut app = connected();
        app.state.discord_user = Some("1".into());

        let (effects, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetStatus {
                status: "dnd".into(),
            },
        );
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Backend(Command::Discord(
                crate::chat::DiscordCommand::SetStatus(status)
            ))] if status == "dnd"
        ));
        assert!(matches!(answer, HostReturn::Ok(_)));

        let (effects, _) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetCustomStatus {
                emoji: Some("🐛".into()),
                text: " debugging ".into(),
                clear_after: Some(600),
            },
        );
        let [
            AppEffect::Backend(Command::Discord(crate::chat::DiscordCommand::SetCustomStatus(
                Some(status),
            ))),
        ] = effects.as_slice()
        else {
            panic!("a custom status is one command");
        };
        assert_eq!(status.text, "debugging");
        assert_eq!(status.emoji.as_deref(), Some("🐛"));
        assert!(status.expires_at_ms.is_some(), "it clears when asked");

        // Nothing written and no emoji clears it, as the person's own field
        // does.
        let (effects, _) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetCustomStatus {
                emoji: None,
                text: "   ".into(),
                clear_after: None,
            },
        );
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Backend(Command::Discord(
                crate::chat::DiscordCommand::SetCustomStatus(None)
            ))]
        ));

        // The game Discord sees this account play is the one activity Yapfast
        // has, so it is the one a plugin shows and clears.
        let (effects, _) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetActivity { enabled: true },
        );
        assert!(app.state.discord_show_game);
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Backend(Command::Discord(
                crate::chat::DiscordCommand::ShowGame(true)
            ))]
        ));
        let (effects, _) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetActivity { enabled: false },
        );
        assert!(!app.state.discord_show_game);
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Backend(Command::Discord(
                crate::chat::DiscordCommand::ShowGame(false)
            ))]
        ));

        // Without an account behind it, nothing is claimed.
        let mut unlinked = connected();
        let (_, answer) = unlinked.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetStatus {
                status: "idle".into(),
            },
        );
        assert!(matches!(answer, HostReturn::Error { .. }));
    }

    /// A conversation with something unread, and a Discord DM beside it.
    fn unread_and_a_discord_dm() -> Application {
        let mut app = Application::default();
        app.state.discord_user = Some("1".into());
        app.apply(Event::Connected {
            user: "@alice:test".into(),
            trusted: true,
            storage: "test",
        });
        app.apply(Event::Sync(true));
        app.apply(Event::Rooms(crate::chat::RoomIndex {
            rooms: vec![
                crate::chat::ChatRoom {
                    unread: 3,
                    ..preview_room("dm", "DM", true, 2)
                },
                crate::chat::ChatRoom {
                    id: "discord:@me:7".into(),
                    network: crate::network::Network::Discord,
                    person: Some("7".into()),
                    ..preview_room("discord:@me:7", "Sam", true, 2)
                },
            ],
            servers: vec![],
        }));
        app
    }

    /// §17.2's reading state group: the same commands the person's own menu
    /// sends, and a place nobody can see is refused.
    #[test]
    fn a_plugin_marks_a_conversation_read_and_mutes_it() {
        let mut app = unread_and_a_discord_dm();
        let (effects, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::MarkRead { room: "dm".into() },
        );
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Backend(Command::MarkRead { room, .. })] if room == "dm"
        ));
        assert!(matches!(answer, HostReturn::Ok(_)));

        let (effects, _) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::SetMute {
                room: "discord:@me:7".into(),
                muting: yapfast_plugin_api::call::Mute::Minutes(60),
            },
        );
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Backend(Command::Discord(crate::chat::DiscordCommand::Mute {
                room,
                muting,
            }))] if room == "discord:@me:7"
                && *muting == crate::chat::Muting::Minutes(60)
        ));

        let (effects, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::MarkRead {
                room: "nowhere".into(),
            },
        );
        assert!(effects.is_empty());
        assert!(
            matches!(answer, HostReturn::Error { .. }),
            "a place this account can't see is refused"
        );
    }

    /// §17.2's navigation group opens a room where a click in the sidebar
    /// does, and refuses one nobody can see.
    #[test]
    fn a_plugin_opens_a_room() {
        let mut app = unread_and_a_discord_dm();
        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::OpenRoom {
                room: "discord:@me:7".into(),
            },
        );
        assert!(matches!(answer, HostReturn::Ok(_)));
        assert_eq!(app.state.selected.as_deref(), Some("discord:@me:7"));

        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::OpenRoom {
                room: "nowhere".into(),
            },
        );
        assert!(matches!(answer, HostReturn::Error { .. }));
    }

    /// §17.2's navigation group, which writes nothing itself: it leaves the
    /// composer in the state a click on Reply or Edit leaves it in.
    #[test]
    fn a_plugin_starts_a_reply_and_an_edit() {
        let mut app = with_message(a_message("m1"));
        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::StartReply {
                room: "dm".into(),
                message: "m1".into(),
            },
        );
        assert!(matches!(answer, HostReturn::Ok(_)));
        assert!(matches!(
            app.state.compose.get("dm"),
            Some(Compose::Reply { item, .. }) if item == "m1"
        ));

        app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::StartEdit {
                room: "dm".into(),
                message: "m1".into(),
            },
        );
        assert!(matches!(
            app.state.edit(),
            Some(edit) if edit.item == "m1"
        ));

        // A message the open conversation doesn't hold is refused rather than
        // guessed at.
        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::StartReply {
                room: "dm".into(),
                message: "not-here".into(),
            },
        );
        assert!(matches!(answer, HostReturn::Error { .. }));
    }

    /// §17.2's messages group: the reactions Yapfast holds for a message.
    #[test]
    fn a_plugin_reads_the_reactions_the_open_timeline_holds() {
        let mut message = a_message("m1");
        message.reactions = vec![crate::chat::Reaction {
            key: "👍".into(),
            count: 2,
            mine: true,
        }];
        let mut app = with_message(message);
        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::Reactors {
                room: "dm".into(),
                message: "m1".into(),
            },
        );
        assert_eq!(
            answer,
            HostReturn::Ok(serde_json::json!([
                { "emoji": "👍", "count": 2, "me": true }
            ]))
        );

        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::Reactors {
                room: "dm".into(),
                message: "elsewhere".into(),
            },
        );
        assert!(matches!(answer, HostReturn::Error { .. }));
    }

    /// §17.2's servers group: Yapfast's own permissions decide, so a plugin
    /// gets no further than the person would.
    #[test]
    fn a_plugin_changes_a_server_only_where_the_person_may() {
        let mut app = connected();
        app.apply(Event::Rooms(crate::chat::RoomIndex {
            rooms: vec![preview_room("dm", "DM", true, 2)],
            servers: vec![crate::chat::ChatServer {
                id: "discord:10".into(),
                name: "Friends".into(),
                discord: true,
                ..Default::default()
            }],
        }));
        for request in [
            crate::plugins::state::AppCall::PauseInvites {
                server: "discord:10".into(),
                invites: true,
                dms: false,
                hours: 1,
            },
            crate::plugins::state::AppCall::AddEmoji {
                server: "discord:10".into(),
                name: "bug".into(),
                image: b"\x89PNG\r\n\x1a\n".to_vec(),
            },
            crate::plugins::state::AppCall::AddSticker {
                server: "discord:10".into(),
                name: "bug".into(),
                tags: "🐛".into(),
                description: String::new(),
                image: b"\x89PNG\r\n\x1a\n".to_vec(),
            },
        ] {
            let (effects, answer) = app.plugins_call(&plugin(), request);
            assert!(effects.is_empty(), "nothing is asked of Discord");
            assert!(
                matches!(answer, HostReturn::Error { .. }),
                "no permission there, so no change"
            );
        }
    }

    /// Audio: what isn't a sound, and words that aren't any, are refused here
    /// rather than at the speakers.
    #[test]
    fn a_sound_that_isnt_one_is_refused_and_so_is_silence_spoken() {
        let mut app = connected();
        let (effects, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::PlaySound {
                sound: b"not a sound".to_vec(),
                volume: None,
            },
        );
        assert!(effects.is_empty());
        assert!(matches!(answer, HostReturn::Error { .. }));

        let (_, answer) = app.plugins_call(
            &plugin(),
            crate::plugins::state::AppCall::Speak {
                text: "   ".into(),
                voice: None,
                rate: None,
                volume: None,
            },
        );
        assert!(matches!(answer, HostReturn::Error { .. }));
    }
}
