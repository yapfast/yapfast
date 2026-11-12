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
use yapfast_plugin_api::call::CommandInvocation;
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
                redraw_if(gone)
            }
            // The store changed under the instances that drew what's on
            // screen, so all of it goes.
            News::Notice(Notice::SlotsReset) => {
                let gone = self
                    .state
                    .plugin_slots
                    .as_mut()
                    .is_some_and(|slots| slots.clear_all());
                redraw_if(gone)
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
        }
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
                    .map(|member| yapfast_plugin_api::message::DecorateMemberRequest {
                        server: server.clone(),
                        room: room.to_owned(),
                        member: member.id.clone(),
                        name: member.name.clone(),
                        own: self.state.discord_user.as_deref() == Some(member.id.as_str()),
                        bot: member.bot,
                        roles: member.roles.to_vec(),
                    })
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
}
