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

impl Application {
    /// Reads the plugin store and starts what must run at the start of a
    /// session, which is what a plugin that intercepts outgoing messages
    /// must: the first message can't go out before it is ready.
    ///
    /// The data directory is where the store and the enabled manifests are.
    /// `events` is the channel the rest of the application's events arrive
    /// on, which is how a hook's answer comes back as
    /// [`crate::chat::Event::Plugin`]: the application doesn't poll for one.
    ///
    /// The bridge runs the helper as a task, so this must be called with the
    /// background runtime entered. It is called once, at startup: before it,
    /// and in a build or a run without plugins, every tap is one check of
    /// [`super::AppState::plugins`].
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
        // The bridge starts a runtime of its own for the helper's task, and a
        // runtime can't be started from inside one: this is called from the
        // window's thread, which is outside every runtime.
        if tokio::runtime::Handle::try_current().is_ok() {
            tracing::warn!("the plugin system starts outside the background runtime");
            return vec![];
        }
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
                return vec![];
            }
        };
        plugins.start();
        let statuses = plugins.statuses();
        let watching = plugins.enabled();
        self.state.plugins = Some(Box::new(plugins));
        self.watch_gateway(watching);
        statuses
            .into_iter()
            .map(|(plugin, status)| AppEffect::Plugin(Effect::Status { plugin, status }))
            .collect()
    }

    /// Whether the plugin system is on, which every tap asks before it builds
    /// anything: one check of an `Option` and one of a field.
    fn plugins_on(&self) -> bool {
        self.state
            .plugins
            .as_ref()
            .is_some_and(|plugins| plugins.enabled())
    }

    /// Tells the Discord gateway's tap whether a plugin may be watching.
    ///
    /// The gateway reports every message and reaction the account may read,
    /// whether or not a conversation shows it, so that tap is the only place
    /// a plugin hears of a conversation that isn't on screen. It is off
    /// unless the plugin system says otherwise, which keeps that path at one
    /// relaxed load.
    fn watch_gateway(&self, watching: bool) {
        #[cfg(feature = "discord")]
        crate::discord_text::watch_messages(watching);
        #[cfg(not(feature = "discord"))]
        let _ = watching;
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
        }
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
    fn plugins_run(&mut self, plugin: PluginId, invocation: CommandInvocation) -> Vec<AppEffect> {
        if let Some(plugins) = self.state.plugins.as_mut() {
            plugins.run_command(&plugin, invocation);
        }
        vec![]
    }

    /// The send path's interception: hands a composer's draft to the
    /// `message.pre_send` chain, before any of the send itself is composed.
    ///
    /// `None` leaves the send exactly as it was, which is what a build
    /// without plugins, a run without one enabled, and a chain that answered
    /// without changing anything all do.
    pub(super) fn plugins_send(&mut self, room: &str) -> Option<Vec<AppEffect>> {
        // A preview has no account and no helper to ask, and a plugin system
        // with nothing intercepting has nothing to ask either. A send a plugin
        // asked for isn't intercepted: the chain is for what the person wrote.
        if self.state.preview || !self.plugins_on() || self.plugin_sending {
            return None;
        }
        if !self
            .state
            .plugins
            .as_ref()
            .is_some_and(|plugins| plugins.intercepts_sends())
        {
            return None;
        }
        // An edit has its own chain, and its own answer: a `Changed` draft
        // replaces the text the person is editing, and a held chain keeps the
        // edit back until it answers rather than sending it as a message.
        if matches!(self.state.compose.get(room), Some(Compose::Edit { .. })) {
            let draft = self.plugins_draft(room);
            let decision = self.state.plugins.as_mut()?.pre_edit(draft);
            return match decision {
                PreSend::Send(draft) => {
                    self.state.drafts.insert(room.to_owned(), draft.body);
                    None
                }
                PreSend::Held { .. } => {
                    // The answer applies the same edit; nothing is sent here.
                    self.state.sending = Some(room.to_owned());
                    Some(Vec::new())
                }
            };
        }
        let draft = self.plugins_draft(room);
        let decision = self.state.plugins.as_mut()?.pre_send(draft);
        self.plugins_answer(room, decision)
    }

    /// The draft a hook sees: the composer's text as the person typed it,
    /// before its mentions are wired and its `@silent` taken off.
    fn plugins_draft(&self, room: &str) -> Draft {
        Draft {
            room: room.to_owned(),
            body: self.state.drafts.get(room).cloned().unwrap_or_default(),
            reply_to: match self.state.compose.get(room) {
                Some(Compose::Reply { item, .. }) => Some(item.clone()),
                _ => None,
            },
            // A hook changes text; files are `attachment.pre_upload`'s, and
            // they stay with the composer until the send they belong to.
            attachments: Vec::new(),
        }
    }

    /// Carries out what a chain decided about a send.
    ///
    /// Separate from [`Self::plugins_send`] so that the answer can be replayed
    /// without a bridge: the helper is another process, and this half is what
    /// the application does with what it says.
    fn plugins_answer(&mut self, room: &str, decision: PreSend) -> Option<Vec<AppEffect>> {
        match decision {
            // Nothing intercepts sends, or the chain is done in one step: the
            // message goes as it always does, carrying whatever the chain made
            // of it. A chain that changed nothing leaves no trace at all.
            PreSend::Send(draft) => {
                let typed = self.state.drafts.get(room).cloned().unwrap_or_default();
                if draft.body == typed {
                    return None;
                }
                // The answered text goes out in place of the typed one, and
                // the composer clears as it does after any send: this is what
                // a send that fails keeps, so the person sends what was
                // attempted.
                self.state.drafts.insert(room.to_owned(), draft.body);
                Some(self.send_composed_in(room.to_owned()))
            }
            // Interceptors are running: the composer empties now, so the next
            // message can be written while they think, and the send waits for
            // their answer.
            PreSend::Held { .. } => {
                let typed = self.state.drafts.remove(room).unwrap_or_default();
                // `sending` is what keeps the room's next message behind this
                // one until it goes, and what tells the answer which room it
                // was held for.
                self.sent_draft = Some(typed);
                self.state.sending = Some(room.to_owned());
                self.state.notice = None;
                Some(vec![])
            }
        }
    }

    /// The `typing.pre_send` chain, asked before a typing indicator goes.
    ///
    /// True when a plugin may withhold it: the indicator then goes when the
    /// chain answers, as [`Notice::Typing`], so the composer never waits on a
    /// plugin. A chain that runs out of time lets it go, as every hook does.
    pub(super) fn plugins_typing(&mut self, room: &str) -> bool {
        if self.state.preview || !self.plugins_on() {
            return false;
        }
        self.state
            .plugins
            .as_mut()
            .and_then(|plugins| plugins.typing(room))
            .is_some()
    }

    /// The `attachment.pre_upload` chain, asked about a file the composer is
    /// taking.
    ///
    /// True when a plugin may rename or withhold it: the file stays here, in
    /// [`Application::pending_uploads`], until [`Notice::Attachment`] says what
    /// became of it, and the composer keeps working in the meantime.
    pub(super) fn plugins_upload(&mut self, room: &str, file: &crate::chat::Staged) -> bool {
        if self.state.preview || !self.plugins_on() {
            return false;
        }
        let pending = AttachmentPending {
            room: room.to_owned(),
            name: file.name.clone(),
            bytes: file.size,
            content_type: content_type(&file.name),
        };
        let held = self
            .state
            .plugins
            .as_mut()
            .and_then(|plugins| plugins.attachment(pending));
        match held {
            Some(id) => {
                self.pending_uploads
                    .insert(id, (room.to_owned(), file.clone()));
                true
            }
            None => false,
        }
    }

    /// Carries out what the `attachment.pre_upload` chain said about a file it
    /// held: the name it gives it, or the reason it doesn't go.
    fn plugins_upload_answered(
        &mut self,
        id: u64,
        name: Option<String>,
        blocked: Option<String>,
    ) -> Vec<AppEffect> {
        let Some((room, mut file)) = self.pending_uploads.remove(&id) else {
            return vec![];
        };
        if let Some(reason) = blocked {
            self.state.notice = Some(reason);
            return vec![];
        }
        if let Some(name) = name {
            file.name = upload_name(&name, &file.name);
        }
        let staged = self.state.staged.entry(room).or_default();
        if staged.len() >= crate::chat::MAX_FILES {
            self.state.notice = Some(format!(
                "A message carries up to {} files.",
                crate::chat::MAX_FILES
            ));
            return vec![];
        }
        staged.push(file);
        vec![]
    }

    /// Sends the draft a held chain answered with, once its answer comes.
    fn plugins_send_held(&mut self, draft: Draft) -> Vec<AppEffect> {
        // The hold left `sending` set so that nothing overtook the answer; the
        // send sets it again for as long as the network has the message.
        if self.state.sending.as_deref() == Some(draft.room.as_str()) {
            self.state.sending = None;
        }
        self.sent_draft = None;
        // Anything written in that room while the chain ran is the person's,
        // and stays where it is.
        let typed = self.state.drafts.insert(draft.room.clone(), draft.body);
        let effects = self.send_composed_in(draft.room.clone());
        if let Some(typed) = typed.filter(|typed| !typed.is_empty()) {
            self.state.drafts.insert(draft.room, typed);
        }
        effects
    }

    /// A chain stopped the send: the text comes back to the composer and the
    /// reason shows, as a send the network turned down does.
    fn plugins_send_blocked(&mut self, reason: String) -> Vec<AppEffect> {
        // Only one send can wait on a chain, so the room it waits in is the
        // one `sending` names.
        let room = self.state.sending.take();
        let text = self.sent_draft.take();
        // An edit that was blocked leaves the compose as it was: there is
        // nothing to put back, only the reason to show.
        if room
            .as_ref()
            .is_some_and(|room| matches!(self.state.compose.get(room), Some(Compose::Edit { .. })))
        {
            self.state.notice = Some(reason);
            return vec![];
        }
        if let (Some(room), Some(text)) = (room, text)
            && self.state.drafts.get(&room).is_none_or(String::is_empty)
        {
            self.state.drafts.insert(room, text);
        }
        self.state.notice = Some(reason);
        vec![]
    }

    /// Runs a `/name option…` the composer holds, leaving the composer empty
    /// when it does.
    ///
    /// `None` leaves a `/` as the text it is, which is what the composer has
    /// always sent when nothing claims what was typed.
    pub(super) fn plugins_typed_command(&mut self, room: &str) -> Option<Vec<AppEffect>> {
        if self.state.preview
            || !self.plugins_on()
            || matches!(self.state.compose.get(room), Some(Compose::Edit { .. }))
        {
            return None;
        }
        let declared = self.state.plugins.as_ref()?.commands();
        let (plugin, invocation) = parse_command(self.state.drafts.get(room)?, &declared)?;
        // A command isn't a message: its text leaves the composer, which saves
        // the empty draft the next keystroke would have saved anyway.
        let mut effects = self.set_draft_in(room.to_owned(), String::new());
        effects.extend(self.dispatch(AppAction::Plugin(Action::Command { plugin, invocation })));
        Some(effects)
    }

    /// Applies what the plugin system sent, from [`crate::chat::Event::Plugin`].
    pub(super) fn plugins_news(&mut self, news: News) -> Vec<AppEffect> {
        match news {
            News::Notice(Notice::SendReady { draft, .. }) => {
                // A draft held for an edit is applied to that edit, not sent as
                // a message: the compose still holds the message it changes.
                if matches!(
                    self.state.compose.get(&draft.room),
                    Some(Compose::Edit { .. })
                ) {
                    self.state.sending = None;
                    self.state.drafts.insert(draft.room.clone(), draft.body);
                    let room = draft.room;
                    return self.send_composed_in(room);
                }
                self.dispatch(AppAction::Plugin(Action::SendHeld(draft)))
            }
            News::Notice(Notice::SendBlocked { reason, .. }) => self.plugins_send_blocked(reason),
            // A call only the application can carry out: it is done here, and
            // the answer goes back under the guest's own number, so the plugin's
            // instance never waited on the interface.
            News::Notice(Notice::Call {
                plugin,
                call,
                request,
            }) => {
                let (effects, answer) = self.plugins_call(&plugin, request);
                if let Some(plugins) = self.state.plugins.as_mut() {
                    plugins.answer_call(&plugin, call, answer);
                }
                effects
            }
            News::Notice(Notice::Typing { room, .. }) => {
                vec![AppEffect::Backend(Command::Typing { room, typing: true })]
            }
            News::Notice(Notice::Attachment { id, name, blocked }) => {
                self.plugins_upload_answered(id, name, blocked)
            }
            News::Notice(Notice::Status { plugin, status }) => {
                vec![AppEffect::Plugin(Effect::Status { plugin, status })]
            }
            // A host the plugin wants and nobody has allowed: the person is
            // asked about it here, wherever they are, and the plugin's request
            // waits for the answer. The dialog is application state, so closing
            // Settings doesn't close the question.
            News::Notice(Notice::Consent { plugin, host }) => {
                let name = self
                    .state
                    .plugins
                    .as_ref()
                    .and_then(|plugins| plugins.name(&plugin))
                    .unwrap_or_else(|| plugin.to_string());
                let ask = ConsentAsk { plugin, name, host };
                self.state.plugin_consent = Some(ask.clone());
                vec![AppEffect::Plugin(Effect::Consent(Some(ask)))]
            }
            // A command's answer is a line in the conversation it was typed in,
            // and a failure says why where the person is looking.
            News::Notice(Notice::Fetched { url, result }) => {
                self.plugins_fetched(url, result);
                vec![AppEffect::Plugin(Effect::Page)]
            }
            // The marketplace was read: its listings and their icons are held
            // until the page closes, which drops them.
            News::Notice(Notice::Catalog { source, result }) => {
                self.plugins_catalog_read(source, result);
                vec![AppEffect::Plugin(Effect::Page)]
            }
            News::Notice(Notice::Command { text, .. }) => {
                self.state.notice = Some(text);
                vec![]
            }
            // What a plugin drew: the interface keeps it and shows the slots
            // that moved. Only a real change redraws anything: the same tree
            // again costs nothing.
            News::Notice(Notice::Slot(output)) => {
                let changed = self
                    .state
                    .plugin_slots
                    .as_mut()
                    .is_some_and(|slots| slots.apply(output));
                redraw_if(changed)
            }
            // A plugin that was turned off or suspended draws nothing, so
            // every tree it left goes with it. An instance evicted for being
            // idle keeps its trees: it is still enabled.
            News::Notice(Notice::SlotsGone(plugin)) => {
                let gone = self
                    .state
                    .plugin_slots
                    .as_mut()
                    .is_some_and(|slots| slots.forget(plugin.as_ref()));
                let identities = self
                    .state
                    .plugins
                    .as_mut()
                    .is_some_and(|plugins| plugins.forget_identities());
                redraw_if(gone || identities)
            }
            // The store changed under the instances that drew what's on
            // screen, so all of it goes.
            News::Notice(Notice::SlotsReset) => {
                let gone = self
                    .state
                    .plugin_slots
                    .as_mut()
                    .is_some_and(|slots| slots.clear_all());
                let identities = self
                    .state
                    .plugins
                    .as_mut()
                    .is_some_and(|plugins| plugins.forget_identities());
                redraw_if(gone || identities)
            }
            // How a plugin says somebody looks: what is on screen that shows
            // them is drawn again, and nothing else.
            // How a plugin says somebody looks: what is on screen that shows
            // them is drawn again, and nothing else.
            News::Notice(Notice::Identity {
                person,
                server,
                rank,
                identity,
                ..
            }) => {
                let kept = self.state.plugins.as_mut().is_some_and(|plugins| {
                    plugins.note_identity(&person, server.as_deref(), rank, identity)
                });
                if !kept {
                    return Vec::new();
                }
                self.plugins_identity_arrived(&person, server.as_deref())
            }
            News::Notice(Notice::CodeBlock {
                block, rank, spans, ..
            }) => {
                // Kept by the block's hash. No frame is asked for: a row reads
                // its block's colours from the application's state when it is
                // drawn, and the interface doesn't draw them yet, which
                // `interface.md` says.
                if let Some(plugins) = self.state.plugins.as_mut() {
                    plugins.note_code_block(&block, rank, spans);
                }
                Vec::new()
            }
            News::Observed(envelope) => {
                self.plugins_observe(envelope);
                vec![]
            }
        }
    }

    /// Hands the plugin system what a guest's reads answer from, when it has
    /// changed: the account, the lists, the open room's timeline and members.
    ///
    /// This runs on every event and every action, so each part compares
    /// against what the world already holds and publishes nothing when nothing
    /// moved. The timeline and the member list are `Arc`s the application
    /// already shares, so publishing one is a count rather than a copy, and
    /// with no plugin enabled this is one check of an `Option`.
    pub(super) fn plugins_sync_world(&mut self) {
        let Some(plugins) = self.state.plugins.as_ref() else {
            return;
        };
        let world = plugins.world();
        let state = &self.state;
        world.publish_account(
            state.account.as_deref(),
            state.account_name.as_deref(),
            state.account_username.as_deref(),
            state.account_avatar.as_deref(),
            state.account_status.as_deref(),
            state.trusted,
        );
        if world.lists() != Some(state.list_changes) {
            world.publish_lists(
                state.list_changes,
                state
                    .rooms
                    .iter()
                    .map(|room| crate::plugins::world::room_view(room))
                    .collect(),
            );
        }
        world.publish_selected(state.selected.as_deref());
        if let Some(room) = state.selected.as_deref() {
            world.publish_timeline(room, &state.messages);
            if let Some(Ok(members)) = &state.members {
                world.publish_members(room, members);
            }
        }
        // The room and its members, out of the borrow above: the tap below has
        // to ask the plugin system, which the borrow holds.
        let members = state
            .members
            .as_ref()
            .and_then(|members| members.as_ref().ok())
            .cloned();
        let room = state.selected.clone();
        let account = state.account.clone();
        self.plugins_presence(room, members, account);
    }

    /// Tells the plugins subscribed to `presence` who came online, went idle or
    /// left, one event per person whose presence moved.
    ///
    /// `presence` is opt-in and coalesced, so a member list that arrives with
    /// twenty changes is one batch: the bridge gathers a burst of them into a
    /// single call, the way it does typing. Nothing is built while nobody
    /// subscribed.
    fn plugins_presence(
        &mut self,
        room: Option<String>,
        members: Option<Arc<Vec<crate::chat::ChatMember>>>,
        account: Option<String>,
    ) {
        let (Some(room), Some(members)) = (room, members) else {
            return;
        };
        let Some(plugins) = self.state.plugins.as_mut() else {
            return;
        };
        let changes = plugins.presence_changes(&members);
        if changes.is_empty() || !plugins.subscribes(EventKind::Presence) {
            return;
        }
        let network = plugin_network(Network::of(&room));
        let envelopes: Vec<EventEnvelope> = changes
            .into_iter()
            .map(|(person, presence)| {
                let own = account.as_deref() == Some(person.as_str());
                envelope(
                    EventKind::Presence,
                    network,
                    Some(room.clone()),
                    Some(person.clone()),
                    own,
                    serde_json::json!({ "person": person, "presence": presence }),
                )
            })
            .collect();
        for event in envelopes {
            self.plugins_observe(event);
        }
    }

    /// Tells the plugins whether the interface is dark, which `app.theme_mode`
    /// answers. The theme is the window's, so this is called from there.
    pub fn plugins_theme(&mut self, dark: bool) {
        if let Some(plugins) = self.state.plugins.as_ref() {
            plugins.world().publish_theme(dark);
        }
    }

    /// Carries out a call only the application can, and says what it answered.
    ///
    /// Each one wraps something Yapfast already does: the token, the rate
    /// limiter and the confirmation of a link stay where they are, and the
    /// plugin only names what it wants.
    fn plugins_call(
        &mut self,
        plugin: &yapfast_plugin_api::PluginId,
        request: crate::plugins::state::AppCall,
    ) -> (Vec<AppEffect>, yapfast_plugin_api::call::HostReturn) {
        use crate::plugins::state::AppCall;
        let done = yapfast_plugin_api::call::HostReturn::Ok(serde_json::Value::Bool(true));
        match request {
            AppCall::Send { room, body } => {
                if !self.state.rooms.iter().any(|listed| listed.id == room) {
                    return (
                        Vec::new(),
                        yapfast_plugin_api::call::HostReturn::Error {
                            message: format!("{room} isn't a conversation this account can see"),
                        },
                    );
                }
                let effects = self.plugins_send_a_message(&room, body);
                (effects, done)
            }
            AppCall::Edit {
                room,
                message,
                body,
            } => (
                vec![AppEffect::Backend(Command::Edit {
                    room,
                    item: message,
                    body,
                })],
                done,
            ),
            AppCall::Delete { room, message } => (
                vec![AppEffect::Backend(Command::Delete {
                    room,
                    item: message,
                })],
                done,
            ),
            AppCall::React {
                room,
                message,
                emoji,
            } => (
                vec![AppEffect::Backend(Command::React {
                    room,
                    item: message,
                    key: emoji,
                })],
                done,
            ),
            AppCall::JumpTo { room, message } => (
                self.dispatch(AppAction::JumpInRoom {
                    room,
                    item: message,
                }),
                done,
            ),
            AppCall::GetDraft { room } => (
                Vec::new(),
                yapfast_plugin_api::call::HostReturn::Ok(serde_json::Value::String(
                    self.state.drafts.get(&room).cloned().unwrap_or_default(),
                )),
            ),
            AppCall::SetDraft { room, body } => (self.set_draft_in(room, body), done),
            AppCall::Insert { room, text } => {
                // Insert is what a widget does to the composer: after whatever
                // the person has already written, never in place of it.
                let mut draft = self.state.drafts.get(&room).cloned().unwrap_or_default();
                draft.push_str(&text);
                (self.set_draft_in(room, draft), done)
            }
            AppCall::Notify { text } => (
                vec![AppEffect::Notify(crate::chat::Alert {
                    room: self.state.selected.clone().unwrap_or_default(),
                    title: plugin.to_string(),
                    body: text,
                    picture: None,
                    call: false,
                    mention: false,
                })],
                done,
            ),
            AppCall::Copy { text } => (vec![AppEffect::CopyText(text)], done),
            // The link goes through the same confirmation a message's does: a
            // plugin asks, and the person still sees where it goes.
            AppCall::Open { url } => (vec![AppEffect::OpenPage(url)], done),
            // The curated actions of PLAN §17.2. Each goes where the person's
            // own click goes, so the token, the rate limiter and this
            // account's permissions stay where they are.
            AppCall::SetStatus { status } => match self.ready_for_discord() {
                Ok(()) => (self.dispatch(AppAction::SetStatus(status)), done),
                Err(message) => (Vec::new(), HostReturn::Error { message }),
            },
            AppCall::SetCustomStatus {
                emoji,
                text,
                clear_after,
            } => answered(self.plugin_status(emoji, text, clear_after)),
            // The only activity Yapfast has is the game Discord sees this
            // account play, so that is the one a plugin can show or clear.
            AppCall::SetActivity { enabled } => match self.ready_for_discord() {
                Ok(()) => (self.dispatch(AppAction::ShowGame(enabled)), done),
                Err(message) => (Vec::new(), HostReturn::Error { message }),
            },
            AppCall::MarkRead { room } => match self.listed_place(&room) {
                true => (self.dispatch(AppAction::MarkRead(room)), done),
                false => (Vec::new(), not_listed(&room)),
            },
            AppCall::SetMute { room, muting } => {
                if !self.listed_place(&room) {
                    return (Vec::new(), not_listed(&room));
                }
                // A conversation is muted through its own menu, and a channel
                // or a server through its notifications, as each does when a
                // person picks Mute.
                let mut effects = self.dispatch(AppAction::MuteDm {
                    room: room.clone(),
                    muting,
                });
                if effects.is_empty() {
                    effects = self.dispatch(AppAction::ChangeNotifications {
                        place: room,
                        change: crate::chat::NotificationChange::Mute(muting),
                    });
                }
                (effects, done)
            }
            AppCall::OpenRoom { room } => match self.listed_place(&room) {
                true => (self.dispatch(AppAction::Open(room)), done),
                false => (Vec::new(), not_listed(&room)),
            },
            AppCall::StartReply { room, message } => {
                answered(self.plugin_compose(&room, &message, false))
            }
            AppCall::StartEdit { room, message } => {
                answered(self.plugin_compose(&room, &message, true))
            }
            AppCall::Reactors { room, message } => {
                // The reactions a message has are held for the conversation on
                // screen, which is the one whose messages Yapfast keeps.
                if self.state.message(&message).is_none()
                    || self.state.room_of(&message).map(String::as_str) != Some(room.as_str())
                {
                    return (
                        Vec::new(),
                        HostReturn::Error {
                            message: format!(
                                "{message} isn't a message the open conversation holds"
                            ),
                        },
                    );
                }
                let reactions: Vec<ReactionCount> = self
                    .state
                    .message(&message)
                    .map(|held| {
                        held.reactions
                            .iter()
                            .map(|reaction| ReactionCount {
                                emoji: reaction.key.clone(),
                                count: reaction.count as u64,
                                me: reaction.mine,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                match serde_json::to_value(reactions) {
                    Ok(value) => (Vec::new(), HostReturn::Ok(value)),
                    Err(error) => (
                        Vec::new(),
                        HostReturn::Error {
                            message: format!("could not write the reactions: {error}"),
                        },
                    ),
                }
            }
            AppCall::PauseInvites {
                server,
                invites,
                dms,
                hours,
            } => answered(self.plugin_pause_invites(&server, invites, dms, hours)),
            AppCall::AddEmoji {
                server,
                name,
                image,
            } => answered(self.plugin_add_emoji(&server, &name, &image)),
            AppCall::AddSticker {
                server,
                name,
                tags,
                description,
                image,
            } => answered(self.plugin_add_sticker(&server, &name, &tags, &description, &image)),
            AppCall::PlaySound { sound, volume } => {
                match crate::plugins::audio::play(sound, volume) {
                    Ok(()) => (Vec::new(), done),
                    Err(error) => (
                        Vec::new(),
                        HostReturn::Error {
                            message: error.to_string(),
                        },
                    ),
                }
            }
            AppCall::Speak { text } => match crate::plugins::audio::speak(&text) {
                Ok(()) => (Vec::new(), done),
                Err(error) => (
                    Vec::new(),
                    HostReturn::Error {
                        message: error.to_string(),
                    },
                ),
            },
        }
    }

    /// Whether an id names something the sidebar lists: a conversation, a
    /// server, a folder or a category. A plugin can only act on a place the
    /// person can see.
    fn listed_place(&self, place: &str) -> bool {
        self.state.rooms.iter().any(|room| room.id == place)
            || self.state.servers.iter().any(|server| server.id == place)
            || self.state.is_folder(place)
            || self.state.is_category(place)
    }

    /// Whether this Yapfast can change a Discord account at all: it is linked,
    /// and the window isn't showing the preview that has no account behind it.
    fn ready_for_discord(&self) -> Result<(), String> {
        if self.state.preview || self.state.discord_user.is_none() {
            return Err("this Yapfast has no Discord account signed in".to_owned());
        }
        Ok(())
    }

    /// The custom status a plugin asked for, sent without the person's status
    /// editor being open: that editor is a dialog nobody opened here.
    fn plugin_status(
        &mut self,
        emoji: Option<String>,
        text: String,
        clear_after: Option<u64>,
    ) -> Result<Vec<AppEffect>, String> {
        self.ready_for_discord()?;
        let text = text.trim().to_owned();
        let emoji = emoji.filter(|emoji| !emoji.is_empty());
        let command = |status: Option<crate::chat::CustomStatus>| {
            AppEffect::Backend(Command::Discord(
                crate::chat::DiscordCommand::SetCustomStatus(status),
            ))
        };
        if text.is_empty() && emoji.is_none() {
            return Ok(vec![command(None)]);
        }
        let (emoji, emoji_id) = match emoji {
            Some(name) if name.contains(':') => {
                let found = self.status_emoji(&name).map_err(str::to_owned)?;
                (
                    Some(found.text.trim_matches(':').to_owned()),
                    Some(found.id),
                )
            }
            emoji => (emoji, None),
        };
        Ok(vec![command(Some(crate::chat::CustomStatus {
            text,
            emoji,
            emoji_id,
            expires_at_ms: clear_after
                .map(|seconds| crate::clock::now_ms().saturating_add(seconds.saturating_mul(1000))),
        }))])
    }

    /// Starts a reply to, or an edit of, a message the open conversation holds.
    fn plugin_compose(
        &mut self,
        room: &str,
        message: &str,
        edit: bool,
    ) -> Result<Vec<AppEffect>, String> {
        if self.state.message(message).is_none() {
            return Err(format!(
                "{message} isn't a message the open conversation holds"
            ));
        }
        let Some(open) = self.state.room_of(message) else {
            return Err(format!(
                "{message} isn't a message the open conversation holds"
            ));
        };
        if open != room {
            return Err(format!("{message} is in {open}, not {room}"));
        }
        let message = message.to_owned();
        Ok(if edit {
            self.dispatch(AppAction::Edit(message))
        } else {
            self.dispatch(AppAction::Reply(message))
        })
    }

    /// Pauses a server's invites, or resumes them, as Security Actions does.
    fn plugin_pause_invites(
        &mut self,
        server: &str,
        invites: bool,
        dms: bool,
        hours: u8,
    ) -> Result<Vec<AppEffect>, String> {
        if !self.state.servers.iter().any(|listed| listed.id == server) {
            return Err(format!("{server} isn't a server this account is in"));
        }
        // The lengths Discord's own dialog offers; anything else takes the
        // longest, which is what pausing for good looks like there.
        let offered = super::server_settings::PAUSE_HOURS;
        let hours = if offered.contains(&hours) {
            hours
        } else {
            offered[offered.len() - 1]
        };
        if !self.may(server, crate::chat::Permission::ManageServer) {
            return Err(format!(
                "you don't have Manage Server in {server}, so a plugin can't pause its invites"
            ));
        }
        Ok(vec![AppEffect::Backend(Command::Discord(
            crate::chat::DiscordCommand::Invites {
                server: server.to_owned(),
                action: crate::chat::InvitesAction::Pause {
                    invites,
                    dms,
                    hours,
                },
            },
        ))])
    }

    /// Adds a server's emoji, through the reader a person's own picture goes
    /// through, and then the upload a person's does.
    #[cfg(feature = "discord")]
    fn plugin_add_emoji(
        &mut self,
        server: &str,
        name: &str,
        image: &[u8],
    ) -> Result<Vec<AppEffect>, String> {
        self.may_add_expressions(server)?;
        let name = super::emoji_name(name.trim());
        if name.chars().count() < 2 {
            return Err("an emoji's name is at least two characters".to_owned());
        }
        let picture = with_brought_picture(image, crate::discord_text::expressions::read_emoji)?;
        Ok(vec![AppEffect::Backend(Command::Discord(
            crate::chat::DiscordCommand::Emoji {
                server: server.to_owned(),
                action: crate::chat::EmojiAction::Upload { name, picture },
            },
        ))])
    }

    #[cfg(not(feature = "discord"))]
    fn plugin_add_emoji(
        &mut self,
        _server: &str,
        _name: &str,
        _image: &[u8],
    ) -> Result<Vec<AppEffect>, String> {
        Err("this build has no Discord, so it has no emoji to add".to_owned())
    }

    /// The same for a server's stickers.
    #[cfg(feature = "discord")]
    fn plugin_add_sticker(
        &mut self,
        server: &str,
        name: &str,
        tags: &str,
        description: &str,
        image: &[u8],
    ) -> Result<Vec<AppEffect>, String> {
        self.may_add_expressions(server)?;
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err("a sticker needs a name".to_owned());
        }
        let file = with_brought_picture(image, crate::discord_text::expressions::read_sticker)?;
        Ok(vec![AppEffect::Backend(Command::Discord(
            crate::chat::DiscordCommand::Stickers {
                server: server.to_owned(),
                action: Box::new(crate::chat::StickerAction::Upload {
                    form: crate::chat::StickerForm {
                        name,
                        tags: tags.trim().to_owned(),
                        description: description.trim().to_owned(),
                    },
                    file,
                }),
            },
        ))])
    }

    #[cfg(not(feature = "discord"))]
    fn plugin_add_sticker(
        &mut self,
        _server: &str,
        _name: &str,
        _tags: &str,
        _description: &str,
        _image: &[u8],
    ) -> Result<Vec<AppEffect>, String> {
        Err("this build has no Discord, so it has no stickers to add".to_owned())
    }

    /// Whether this account may add an emoji or a sticker to a server: what
    /// Emoji's and Stickers' own buttons ask for.
    fn may_add_expressions(&self, server: &str) -> Result<(), String> {
        if !self.state.servers.iter().any(|listed| listed.id == server) {
            return Err(format!("{server} isn't a server this account is in"));
        }
        let rules = self.expression_rules(server);
        if !(rules.manage || rules.create) {
            return Err(format!(
                "you may not add expressions in {server}, so a plugin can't either"
            ));
        }
        Ok(())
    }

    /// Sends a plugin's text in a room, leaving the person's own draft where it
    /// was.
    fn plugins_send_a_message(&mut self, room: &str, body: String) -> Vec<AppEffect> {
        let saved = self.state.drafts.get(room).cloned();
        self.state.drafts.insert(room.to_owned(), body);
        self.plugin_sending = true;
        let effects = self.send_composed_in(room.to_owned());
        self.plugin_sending = false;
        match saved {
            Some(draft) => {
                self.state.drafts.insert(room.to_owned(), draft);
            }
            None => {
                self.state.drafts.remove(room);
            }
        }
        effects
    }

    /// Asks the plugins that draw under messages what they draw under the rows
    /// on screen.
    ///
    /// Only the rows the person can see are asked about, and only when one of
    /// them draws: a channel with no such plugin costs one load of a flag.
    pub(super) fn plugins_decorate_visible(&mut self, room: &str, items: &[String]) {
        if !self.plugins_on() {
            return;
        }
        let Some(plugins) = self.state.plugins.as_ref() else {
            return;
        };
        if !plugins.decorates() {
            return;
        }
        // The timeline is the open room's, which is the room these rows are in:
        // `Action::VisibleMessages` only reports the selected conversation.
        let timeline = &self.state.messages;
        let rows: Vec<yapfast_plugin_api::message::DecorateRequest> = items
            .iter()
            .filter_map(|item| {
                let message = timeline.iter().find(|message| &message.item == item)?;
                Some(yapfast_plugin_api::message::DecorateRequest {
                    room: room.to_owned(),
                    message: message.item.clone(),
                    author: message.author.clone(),
                    body: message.body.clone(),
                    own: message.own,
                })
            })
            .collect();
        if rows.is_empty() {
            return;
        }
        if let Some(plugins) = self.state.plugins.as_mut() {
            plugins.decorate(&rows);
        }
    }

    /// Applies what plugins say about how one person looks, asking for an
    /// answer when nothing fresh is kept for them.
    ///
    /// §17.2 asks for one answer per (person, server) used by every surface
    /// that shows that person. The profile card is the surface that hosts it
    /// today: the card a message, a member list, a DM or a friend opens. The
    /// answer changes how that card draws them, and nothing else.
    pub(super) fn plugins_identify(&mut self, room: &str, profile: &mut crate::chat::Profile) {
        let Some(plugins) = self.state.plugins.as_mut() else {
            return;
        };
        if !plugins.identifies() {
            return;
        }
        let server = plugin_server(room);
        let person = profile.id.clone();
        if let Some(identity) = plugins.identity(&person, server.as_deref()) {
            decorate_profile(profile, &identity);
            return;
        }
        plugins.ask_identity(yapfast_plugin_api::identity::IdentityRequest {
            person,
            server,
            name: profile.name.clone(),
            username: Some(profile.handle.clone()).filter(|handle| !handle.is_empty()),
            own: self.state.account.as_deref() == Some(profile.id.as_str()),
        });
    }

    /// A plugin answered about somebody: the surfaces on screen that show them
    /// are drawn again, and nothing else.
    fn plugins_identity_arrived(&mut self, person: &str, server: Option<&str>) -> Vec<AppEffect> {
        let Some(plugins) = self.state.plugins.as_mut() else {
            return Vec::new();
        };
        let shown = self
            .state
            .profile
            .as_ref()
            .is_some_and(|view| view.profile.id == person)
            || self
                .state
                .panel_profile
                .as_ref()
                .is_some_and(|view| view.profile.id == person);
        if !shown {
            // Nothing on screen shows them, so the answer is kept for the next
            // card that does and no frame is spent on it.
            return Vec::new();
        }
        let known = plugins.identity(person, server);
        if let Some(identity) = known {
            if let Some(view) = self
                .state
                .profile
                .as_mut()
                .filter(|view| view.profile.id == person)
            {
                decorate_profile(&mut view.profile, &identity);
            }
            if let Some(view) = self
                .state
                .panel_profile
                .as_mut()
                .filter(|view| view.profile.id == person)
            {
                decorate_profile(&mut view.profile, &identity);
            }
        }
        redraw_identity()
    }

    /// Asks the plugins that colour code blocks about the fences in the rows on
    /// screen, and about none of the blocks it already has an answer for.
    ///
    /// A block is named by its own content hash, which is what makes two
    /// messages quoting the same snippet one question, and a block that scrolls
    /// away and back no question at all. The hash covers the language as well as
    /// the text, because a language is what colours it.
    pub(super) fn plugins_colour_code_visible(&mut self, _room: &str, items: &[String]) {
        let Some(plugins) = self.state.plugins.as_ref() else {
            return;
        };
        if !plugins.colours_code() {
            return;
        }
        let mut asked: Vec<yapfast_plugin_api::code::CodeBlockRequest> = Vec::new();
        for item in items {
            let Some(message) = self
                .state
                .messages
                .iter()
                .find(|message| &message.item == item)
            else {
                continue;
            };
            for (language, text) in crate::plugins::code_block::fences(&message.body) {
                let block = crate::plugins::code_block::hash(language.as_deref(), &text);
                if plugins.knows_code_block(&block) {
                    continue;
                }
                asked.push(yapfast_plugin_api::code::CodeBlockRequest {
                    block,
                    language,
                    text,
                });
            }
        }
        if asked.is_empty() {
            return;
        }
        if let Some(plugins) = self.state.plugins.as_mut() {
            plugins.ask_code_blocks(&asked);
        }
    }

    /// Asks the plugins that badge people in the member list about the rows on
    /// screen, and no others: a large server costs what its list shows.
    ///
    /// A person every decorating plugin has answered about isn't asked again,
    /// so scrolling back up costs nothing after the first pass. The cache goes
    /// when the member list's snapshot changes.
    pub(super) fn plugins_decorate_members(&mut self, room: &str, ids: &[String]) {
        if !self.plugins_on() {
            return;
        }
        let Some(members) = self
            .state
            .members
            .as_ref()
            .and_then(|members| members.as_ref().ok())
        else {
            return;
        };
        let Some(slots) = self.state.plugin_slots.as_deref_mut() else {
            return;
        };
        // Another room's list is another set of people: what a plugin said
        // about the last one's is dropped here rather than asked about again.
        if self.badges_rooms.as_deref() != Some(room) {
            slots.members_changed();
            self.badges_rooms = Some(room.to_owned());
        }
        let Some(plugins) = self.state.plugins.as_ref() else {
            return;
        };
        if !plugins.decorates_members() {
            return;
        }
        let decorators = plugins.member_decorator_count();
        if decorators == 0 {
            return;
        }
        let server = crate::discord::parse_room(room)
            .map(|(guild, _)| guild.to_owned())
            .filter(|guild| guild != crate::discord::DM_GUILD)
            .unwrap_or_default();
        let asked: Vec<yapfast_plugin_api::message::DecorateMemberRequest> = ids
            .iter()
            .filter(|id| slots.members_asked(id) < decorators)
            .filter_map(|id| {
                members
                    .iter()
                    .find(|member| &member.id == id)
                    .map(
                        |member| yapfast_plugin_api::message::DecorateMemberRequest {
                            server: server.clone(),
                            room: room.to_owned(),
                            member: member.id.clone(),
                            name: member.name.clone(),
                            own: self.state.discord_user.as_deref() == Some(member.id.as_str()),
                            bot: member.bot,
                            roles: member.roles.to_vec(),
                        },
                    )
            })
            .collect();
        if asked.is_empty() {
            return;
        }
        if let Some(plugins) = self.state.plugins.as_mut() {
            plugins.decorate_members(&asked);
        }
    }

    /// Hands one event to the plugins that subscribed to it. The bridge
    /// applies each subscription's filter and writes the payload once, so a
    /// tap only has to say what happened.
    pub(super) fn plugins_observe(&mut self, envelope: EventEnvelope) {
        if self.state.plugins.is_none() {
            return;
        }
        if let Some(plugins) = self.state.plugins.as_mut()
            && plugins.subscribes(envelope.kind)
        {
            plugins.observe(envelope);
        }
    }

    /// The account opened a conversation, or the same one again to retry it.
    /// What the sidebar shows of it goes along, so a plugin can tell a DM from
    /// a channel without asking.
    pub(super) fn plugins_room_selected(&mut self, room: &str) {
        if self.state.selected.as_deref() == Some(room) || !self.plugins_on() {
            return;
        }
        let Some(listed) = self.state.rooms.iter().find(|listed| listed.id == room) else {
            return;
        };
        let network = plugin_network(listed.network);
        let server = crate::discord::parse_room(&listed.id)
            .map(|(guild, _)| guild.to_owned())
            .filter(|guild| guild != crate::discord::DM_GUILD);
        let view = RoomView {
            id: listed.id.clone(),
            name: listed.name.clone(),
            network,
            server,
            dm: listed.direct,
        };
        self.plugins_observe(envelope(
            EventKind::RoomSelected,
            network,
            Some(listed.id.clone()),
            None,
            false,
            serde_json::to_value(view).unwrap_or_default(),
        ));
    }

    /// The window gained or lost focus, which is the only sign a plugin gets
    /// that the person is there.
    pub(super) fn plugins_focus(&mut self, focused: bool) {
        if !self.plugins_on() {
            return;
        }
        self.plugins_observe(envelope(
            EventKind::AppFocus,
            self.on_screen_network(),
            self.state.selected.clone(),
            None,
            false,
            serde_json::json!({ "focused": focused }),
        ));
    }

    /// The call this account is in changed: it started, ended, muted, or
    /// someone joined or left it.
    ///
    /// Who is speaking changes many times a second and is `speaking`, its own
    /// coalesced event, so it isn't what this reports: the comparison below
    /// is what keeps a busy call from flooding a plugin.
    pub(super) fn plugins_voice(
        &mut self,
        before: &crate::voice::State,
        after: &crate::voice::State,
    ) {
        if !self.plugins_on() {
            return;
        }
        // Speaking moves many times a second, so it is its own coalesced event
        // and is compared before the early return below: a call where only the
        // speaking changed is exactly the case this tap exists for.
        self.plugins_speaking(before, after);
        let same_call = before.room == after.room
            && before.muted == after.muted
            && before.deafened == after.deafened
            && before.participants.len() == after.participants.len()
            && before
                .participants
                .iter()
                .zip(&after.participants)
                .all(|(was, now)| was.person == now.person && was.muted == now.muted);
        if same_call {
            return;
        }
        let people: Vec<Value> = after
            .participants
            .iter()
            .map(|person| {
                serde_json::json!({
                    "id": person.person,
                    "muted": person.muted,
                    "deafened": person.deafened,
                })
            })
            .collect();
        self.plugins_observe(envelope(
            EventKind::VoiceState,
            after.room.as_deref().map_or_else(
                || self.on_screen_network(),
                |room| plugin_network(Network::of(room)),
            ),
            after.room.clone(),
            None,
            false,
            serde_json::json!({
                "room": after.room,
                "name": after.name,
                "muted": after.muted,
                "deafened": after.deafened,
                "people": people,
            }),
        ));
    }

    /// Tells the plugins subscribed to `speaking` who started and who stopped,
    /// which is what `discord_audio` hears in the call's frames.
    ///
    /// One event per person who changed, gathered into one batch by the bridge:
    /// a room where four people talk is four events a quarter second, not four
    /// hundred. Nothing is built while nobody subscribed.
    fn plugins_speaking(&mut self, before: &crate::voice::State, after: &crate::voice::State) {
        let Some(plugins) = self.state.plugins.as_ref() else {
            return;
        };
        if !plugins.subscribes(EventKind::Speaking) {
            return;
        }
        let changes: Vec<(String, bool, bool)> = after
            .participants
            .iter()
            .filter_map(|person| {
                let was = before
                    .participants
                    .iter()
                    .find(|was| was.person == person.person);
                // A person who joined mid-sentence is a change: nothing said
                // they were silent before.
                let changed = was.is_none_or(|was| was.speaking != person.speaking);
                changed.then(|| (person.person.clone(), person.speaking, person.local))
            })
            .collect();
        if changes.is_empty() {
            return;
        }
        let network = after.room.as_deref().map_or_else(
            || self.on_screen_network(),
            |room| plugin_network(Network::of(room)),
        );
        let envelopes: Vec<EventEnvelope> = changes
            .into_iter()
            .map(|(person, speaking, own)| {
                envelope(
                    EventKind::Speaking,
                    network,
                    after.room.clone(),
                    Some(person.clone()),
                    own,
                    serde_json::json!({ "person": person, "speaking": speaking }),
                )
            })
            .collect();
        for event in envelopes {
            self.plugins_observe(event);
        }
    }

    /// The account signed in and its lists are here, so the calls that read
    /// them can answer.
    pub(super) fn plugins_account_ready(&mut self, account: &str, trusted: bool) {
        if !self.plugins_on() {
            return;
        }
        self.plugins_observe(envelope(
            EventKind::AccountReady,
            self.on_screen_network(),
            None,
            Some(account.to_owned()),
            true,
            serde_json::json!({ "account": account, "trusted": trusted }),
        ));
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
        profile.name = name.clone();
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
        profile.pronouns = pronouns.clone();
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
    let view = MessageView {
        id: message.id.clone(),
        room: room.to_owned(),
        author,
        // As Discord wrote it, its mentions still as `<@id>`: a plugin that
        // wants names calls `people.get`, rather than guessing at Markdown.
        body: message.content.clone(),
        at: message.timestamp() * 1000,
        edited: message.edited,
        deleted: false,
        // Files are `media.get`'s, and a plugin gets no token with which to
        // fetch one: an event names a message, not its files' addresses.
        attachments: Vec::new(),
        reply_to: match &message.reply {
            Some(yapfast_discord::message::Reply::Message { id, .. }) => Some(id.clone()),
            Some(yapfast_discord::message::Reply::Unknown { id }) if !id.is_empty() => {
                Some(id.clone())
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
fn content_type(name: &str) -> Option<String> {
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
            matches!(effects.as_slice(), [AppEffect::OpenPage(url)] if url.ends_with("example.com/")),
            "the page opens the way a message's link does"
        );
    }

    /// The text of the message an effect sends, if one does.
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
                muting: crate::chat::Muting::Minutes(60),
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
            app.state.compose.get("dm"),
            Some(Compose::Edit { item, .. }) if item == "m1"
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
            crate::plugins::state::AppCall::Speak { text: "   ".into() },
        );
        assert!(matches!(answer, HostReturn::Error { .. }));
    }
}
