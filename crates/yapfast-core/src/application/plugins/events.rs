//! What the plugin system reports back, and what the application's own events
//! look like to a plugin.
//!
//! [`Application::plugins_news`] is where a notice becomes an effect: a held
//! send, a plugin's status, a fetched marketplace page, the answer to a call.
//! The rest are the taps on the event path — a timeline row that came, a person
//! whose presence moved, a voice state that changed — which build the envelope a
//! subscribed plugin is sent, and nothing at all while none is.

use super::*;

impl Application {
    /// A plugin's name as its manifest gives it, or its id.
    pub(in crate::application) fn plugin_name(&self, plugin: &PluginId) -> String {
        self.state
            .plugins
            .as_ref()
            .and_then(|plugins| plugins.name(plugin))
            .unwrap_or_else(|| plugin.to_string())
    }

    /// [`Effect::Interface`] when the enabled plugins changed what the window
    /// draws since it last built their changes, and nothing otherwise: a
    /// status that only says a plugin went idle costs one hash of the enabled
    /// entries and no manifest read.
    pub(in crate::application) fn plugins_interface_moved(&mut self) -> Option<AppEffect> {
        let key = self.state.plugins.as_ref()?.interface_key();
        if key == self.plugin_interface {
            return None;
        }
        self.plugin_interface = key;
        Some(AppEffect::Plugin(Effect::Interface))
    }

    /// Applies what the plugin system sent, from [`crate::chat::Event::Plugin`].
    pub(in crate::application) fn plugins_news(&mut self, news: News) -> Vec<AppEffect> {
        match news {
            News::Notice(Notice::SendReady { draft, .. }) => {
                // A draft held for an edit is applied to that edit, not sent as
                // a message: the edit's own field is what it changes.
                if self
                    .state
                    .edit
                    .as_ref()
                    .is_some_and(|edit| edit.room == draft.room)
                {
                    self.state.sending = None;
                    if let Some(edit) = self.state.edit.as_mut() {
                        edit.text = draft.body;
                    }
                    return self.save_edit_now();
                }
                self.dispatch(AppAction::Plugin(Action::SendHeld(draft)))
            }
            News::Notice(Notice::SendBlocked { reason, .. }) => self.plugins_send_blocked(reason),
            // A call only the application can carry out: it is done here, and
            // the answer goes back under the guest's own number, so the plugin's
            // instance never waited on the interface.
            // `files.save` waits on a person: its answer comes when the dialog
            // closes, not now.
            News::Notice(Notice::Call {
                plugin,
                call,
                request: crate::plugins::state::AppCall::SaveFile { name, bytes },
            }) => self.plugins_save_file(plugin, call, name, bytes),
            // A recording answers its call when it ends.
            News::Notice(Notice::Call {
                plugin,
                call,
                request:
                    crate::plugins::state::AppCall::Record {
                        format,
                        max_seconds,
                    },
            }) => self.plugins_record(plugin, call, format, max_seconds),
            News::Notice(Notice::RecordingEnded { plugin }) => {
                self.plugins_recording_ended(&plugin)
            }
            // Discord or Spotify answers these, later, under the same call.
            News::Notice(Notice::Call {
                plugin,
                call,
                request,
            }) if account::answered_later(&request) => self.plugins_ask(plugin, call, request),
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
            // A plugin's sign-in page goes through the leaving dialog, which
            // names the plugin, as any page a plugin opens does.
            News::Notice(Notice::SignIn {
                plugin,
                provider,
                url,
            }) => {
                let name = self.plugin_name(&plugin);
                vec![AppEffect::ConfirmOpen {
                    url,
                    plugin: format!("{name} ({plugin}), to sign in to {provider}"),
                }]
            }
            // What a plugin's overlay shows: the window draws it over
            // everything, and paces its frames with the ticker.
            News::Notice(Notice::Overlay { plugin, layer }) => {
                match layer {
                    Some(layer) => {
                        if self.state.plugin_overlay_ticker.is_none() {
                            self.state.plugin_overlay_ticker = self
                                .state
                                .plugins
                                .as_mut()
                                .and_then(|plugins| plugins.overlay_ticker());
                        }
                        self.state
                            .plugin_overlays
                            .insert(plugin.to_string(), Arc::new(layer));
                    }
                    None => {
                        self.state.plugin_overlays.remove(plugin.as_str());
                    }
                }
                vec![AppEffect::Plugin(Effect::Overlay)]
            }
            // A plugin said its decorations changed: the rows on screen are
            // asked again.
            News::Notice(Notice::Redecorate { .. }) => {
                if let Some(plugins) = self.state.plugins.as_mut() {
                    plugins.decorate_again();
                }
                if let Some(room) = self.state.selected.clone() {
                    let items = self.visible_messages.clone();
                    self.plugins_decorate_visible(&room, &items);
                }
                vec![]
            }
            // A plugin said how someone looks changed: what was kept goes, and
            // a card that shows them asks again.
            News::Notice(Notice::IdentityRefresh { person, .. }) => {
                if let Some(plugins) = self.state.plugins.as_mut() {
                    match &person {
                        Some(person) => plugins.forget_identity(person),
                        None => plugins.forget_identities(),
                    };
                }
                let room = self.state.selected.clone().unwrap_or_default();
                for panel in [false, true] {
                    let shown = if panel {
                        self.state.panel_profile.clone()
                    } else {
                        self.state.profile.clone()
                    };
                    let Some(view) = shown else {
                        continue;
                    };
                    if person
                        .as_ref()
                        .is_some_and(|person| *person != view.profile.id)
                    {
                        continue;
                    }
                    let mut profile = view.profile.clone();
                    self.plugins_identify(&room, &mut profile);
                }
                redraw_identity()
            }
            News::Notice(Notice::UploadHeld { id, plugin, name }) => {
                self.plugins_upload_held(id, &plugin, &name)
            }
            News::Notice(Notice::UploadFinished {
                id,
                file,
                name,
                insert,
            }) => self.plugins_upload_finished(id, file, name, insert),
            News::Notice(Notice::Status { plugin, status }) => {
                let mut effects = vec![AppEffect::Plugin(Effect::Status { plugin, status })];
                effects.extend(self.plugins_interface_moved());
                effects
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
            // A powerful capability's first use: the person is asked in a red
            // dialog of its own, wherever they are, as a host is.
            News::Notice(Notice::Powerful { plugin, capability }) => {
                let (name, sentence) = self
                    .state
                    .plugins
                    .as_ref()
                    .map(|plugins| {
                        (
                            plugins.name(&plugin).unwrap_or_else(|| plugin.to_string()),
                            plugins
                                .permissions(&plugin)
                                .map(|permissions| permissions.powerful_sentence(capability))
                                .unwrap_or_else(|| capability.summary().to_owned()),
                        )
                    })
                    .unwrap_or_else(|| (plugin.to_string(), capability.summary().to_owned()));
                self.state.plugin_powerful = Some(PowerfulAsk {
                    plugin,
                    name,
                    capability,
                    sentence,
                });
                vec![AppEffect::Plugin(Effect::Page)]
            }
            // A plugin sent as an agent: the call itself set the indicator,
            // which the window draws from the state.
            News::Notice(Notice::AgentSent { .. }) => vec![AppEffect::Plugin(Effect::Slots)],
            // An offer for the link the leaving dialog is about; one for a
            // link that has gone is let go.
            News::Notice(Notice::LinkOffer {
                id,
                plugin,
                label,
                uri,
            }) => {
                let name = self.plugin_name(&plugin);
                let Some(link) = self.state.plugin_link.as_mut() else {
                    return Vec::new();
                };
                if !link.ids.contains(&id) || link.offers.len() >= LinkAsk::MOST_OFFERS {
                    return Vec::new();
                }
                link.offers.push(LinkOfferShown {
                    plugin,
                    name,
                    label,
                    uri,
                });
                vec![AppEffect::Plugin(Effect::Slots)]
            }
            News::Notice(Notice::SwitcherRanked {
                plugin,
                query,
                order,
            }) => {
                if self.state.switcher.as_deref() != Some(query.as_str()) {
                    return Vec::new();
                }
                let name = self.plugin_name(&plugin);
                self.state.plugin_switcher = Some(SwitcherRanking {
                    plugin,
                    name,
                    query,
                    order,
                });
                vec![AppEffect::Plugin(Effect::Slots)]
            }
            News::Notice(Notice::WebPicture {
                plugin,
                url,
                picture,
            }) => {
                if let Some(plugins) = self.state.plugins.as_mut() {
                    let state = match picture {
                        Ok(picture) => crate::plugins::state::WebPictureState::Ready(picture),
                        Err(_) => crate::plugins::state::WebPictureState::Failed,
                    };
                    plugins.web_pictures.arrived(plugin.as_str(), &url, state);
                }
                vec![AppEffect::Plugin(Effect::Slots)]
            }
            // One plugin's words show at a time: the first to set them keeps
            // the title until it takes them away or stops.
            News::Notice(Notice::WindowTitle { plugin, suffix }) => {
                let owned_by_other = self
                    .state
                    .plugin_title
                    .as_ref()
                    .is_some_and(|shown| shown.plugin != plugin);
                if owned_by_other {
                    return Vec::new();
                }
                let name = self.plugin_name(&plugin);
                self.state.plugin_title = suffix.map(|suffix| TitleShown {
                    plugin,
                    name,
                    suffix,
                });
                vec![AppEffect::Plugin(Effect::Slots)]
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
                let decorated = match &output {
                    crate::plugins::slots::Output::Decorate { message, .. } => {
                        Some(message.clone())
                    }
                    _ => None,
                };
                let changed = self
                    .state
                    .plugin_slots
                    .as_mut()
                    .is_some_and(|slots| slots.apply(output));
                // A decoration is drawn in its row, which the interface lays out
                // and measures with the timeline: a new timeline `Arc`, holding
                // the same messages, is what tells it to look at its rows again.
                // Only the row whose decoration moved is laid out anew.
                if changed
                    && let Some(message) = decorated
                    && self
                        .state
                        .messages
                        .iter()
                        .any(|shown| shown.item == message)
                {
                    self.state.messages = Arc::new(self.state.messages.as_ref().clone());
                }
                redraw_if(changed)
            }
            // A plugin that was turned off or suspended draws nothing, so
            // every tree it left goes with it. An instance evicted for being
            // idle keeps its trees: it is still enabled.
            News::Notice(Notice::SlotsGone(plugin)) => {
                if let Some(plugins) = self.state.plugins.as_mut() {
                    plugins.web_pictures.forget(plugin.as_str());
                }
                // A plugin that stopped draws no overlay, and records nothing.
                self.state.plugin_overlays.remove(plugin.as_str());
                let _ = self.plugins_recording_ended(&plugin);
                // A plugin that stopped adds nothing to the title any more.
                if self
                    .state
                    .plugin_title
                    .as_ref()
                    .is_some_and(|shown| shown.plugin == plugin)
                {
                    self.state.plugin_title = None;
                }
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
                let mut effects = redraw_if(gone || identities);
                effects.extend(self.plugins_interface_moved());
                // A plugin that stopped shows no activity on the account.
                effects.extend(self.plugins_activity_gone(&plugin));
                effects
            }
            // The store changed under the instances that drew what's on
            // screen, so all of it goes.
            News::Notice(Notice::SlotsReset) => {
                self.state.plugin_title = None;
                self.state.plugin_overlays.clear();
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
                // An uninstall of the last plugin announces no status, so this
                // is where its colours and parts are noticed gone.
                let mut effects = redraw_if(gone || identities);
                effects.extend(self.plugins_interface_moved());
                effects
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
                // Kept by the block's hash. A row reads its block's colours from
                // here when its text is laid out, so one laid out before the
                // answer is laid out again.
                let changed = self
                    .state
                    .plugins
                    .as_mut()
                    .is_some_and(|plugins| plugins.note_code_block(&block, rank, spans));
                redraw_code_blocks_if(changed)
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
    /// with no plugin enabled this is one check of the registry.
    pub(in crate::application) fn plugins_sync_world(&mut self) {
        // The plugin system is allowed whenever the feature is in and
        // `--no-plugins` wasn't passed, with or without a plugin, so the
        // `Option` alone let this compare four locked parts of the world after
        // every keystroke with nothing installed. Nothing reads the world
        // while no plugin is on; the lists' generation is what makes the
        // first sync after one is enabled publish everything.
        if !self.plugins_on() {
            return;
        }
        if !self.plugin_sent_pending.is_empty() {
            self.plugins_attribute_sent();
        }
        let Some(plugins) = self.state.plugins.as_ref() else {
            return;
        };
        let world = plugins.world();
        // Presence is opt-in and coalesced, so it is asked once here: while no
        // plugin wants it there is nothing to compare, nothing kept, and the
        // room, the member list and the account stay where they are rather than
        // being cloned out of the borrow below.
        let wants_presence = plugins.subscribes(EventKind::Presence);
        let wants_roster =
            plugins.subscribes(EventKind::MemberJoin) || plugins.subscribes(EventKind::MemberLeave);
        let wants_places = plugins.subscribes(EventKind::ServerJoin)
            || plugins.subscribes(EventKind::ServerRemove)
            || plugins.subscribes(EventKind::GroupDmRemove);
        let wants_game =
            plugins.subscribes(EventKind::GameStart) || plugins.subscribes(EventKind::GameStop);
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
        if world.friends_at() != Some(state.people_changes) {
            world.publish_friends(
                state.people_changes,
                state
                    .friends
                    .iter()
                    .map(|friend| yapfast_plugin_api::event::PersonView {
                        id: friend.id.clone(),
                        name: friend.name.clone(),
                        display_name: friend.nickname.clone(),
                        own: false,
                        bot: false,
                    })
                    .collect(),
            );
        }
        // The call is published as it changes, by the voice tap; this is the
        // first sync after a plugin was turned on in the middle of one.
        if !world.voice_published() {
            world.publish_voice(crate::plugins::world::voice_view(&state.voice));
        }
        if let Some(room) = state.selected.as_deref() {
            // A plugin reads people as Yapfast made them, not as plugins
            // dressed them.
            let made = self.plugin_dress.as_ref();
            let messages = made
                .filter(|kept| Arc::ptr_eq(&kept.messages_shown, &state.messages))
                .map_or(&state.messages, |kept| &kept.messages_made);
            world.publish_timeline(room, messages);
            if let Some(Ok(members)) = &state.members {
                let members = made
                    .filter(|kept| {
                        kept.members_shown
                            .as_ref()
                            .is_some_and(|shown| Arc::ptr_eq(shown, members))
                    })
                    .and_then(|kept| kept.members_made.as_ref())
                    .unwrap_or(members);
                world.publish_members(room, members);
            }
        }
        if plugins.subscribes(EventKind::RelationshipAdd)
            || plugins.subscribes(EventKind::RelationshipRemove)
            || plugins.subscribes(EventKind::RelationshipRequest)
        {
            self.plugins_relationships();
        } else {
            self.plugin_relationships = None;
        }
        if wants_places {
            self.plugins_places();
        } else {
            self.plugin_places = None;
        }
        if wants_game {
            self.plugins_game();
        } else {
            self.plugin_game = None;
        }
        if wants_presence {
            self.plugins_friend_presence();
        } else {
            self.plugin_friend_presences = None;
        }
        self.plugins_dress();
        if !wants_presence && !wants_roster {
            // Nobody wants presence: nothing is compared, nothing is kept, and
            // the room, the member list and the account stay where they are
            // rather than being cloned out of the borrow below. What the last
            // list said is still there, and a plugin that subscribes now is
            // told the difference against the list in front of it, which is
            // what it would have been told had it been listening all along.
            return;
        }
        // The room and its members, out of the borrow above: the tap below has
        // to ask the plugin system, which the borrow holds.
        let members = self
            .state
            .members
            .as_ref()
            .and_then(|members| members.as_ref().ok())
            .cloned();
        let room = self.state.selected.clone();
        let account = self.state.account.clone();
        if wants_roster {
            self.plugins_roster(room.clone(), members.clone(), account.clone());
        }
        if wants_presence {
            self.plugins_presence(room, members, account);
        }
    }

    /// Tells the plugins with `relationships.read` who became a friend, who
    /// stopped being one, and which friend requests came and went: the
    /// friends list and the requests Yapfast keeps, compared when either
    /// changes. The first read after a plugin starts listening says nothing,
    /// as the member list's does.
    fn plugins_relationships(&mut self) {
        use crate::plugins::presence::{RelationshipChange, Relationships};
        let at = self.state.people_changes;
        if self
            .plugin_relationships
            .as_ref()
            .is_some_and(|kept| kept.at == at)
        {
            return;
        }
        let now = Relationships::read(
            at,
            &self.state.discord_friends,
            &self.state.friend_requests,
            &self.state.friends,
        );
        let Some(before) = self.plugin_relationships.replace(now.clone()) else {
            return;
        };
        let network = yapfast_plugin_api::event::Network::Discord;
        for change in before.changes(&now) {
            let named = |person: &str| now.name_of(&before, person).map(str::to_owned);
            let (kind, person, payload) = match change {
                RelationshipChange::Added(person) => (
                    EventKind::RelationshipAdd,
                    person.clone(),
                    serde_json::json!({ "person": person, "name": named(&person) }),
                ),
                RelationshipChange::Removed { person, was } => (
                    EventKind::RelationshipRemove,
                    person.clone(),
                    serde_json::json!({ "person": person, "was": was, "name": named(&person) }),
                ),
                RelationshipChange::Requested { person, incoming } => (
                    EventKind::RelationshipRequest,
                    person.clone(),
                    serde_json::json!({ "person": person, "incoming": incoming, "name": named(&person) }),
                ),
            };
            self.plugins_observe(envelope(kind, network, None, Some(person), false, payload));
        }
    }

    /// Tells the plugins with `servers.read` which Discord servers the
    /// account joined and which left its list, and which group DMs went: the
    /// lists compared when they change, and only while Discord is online, so
    /// signing in, a reconnect or an outage (which keeps a server listed as
    /// unavailable) says nothing. The first read after a plugin starts
    /// listening says nothing either.
    fn plugins_places(&mut self) {
        use crate::plugins::presence::{PlaceChange, Places};
        if !self.state.discord_online {
            self.plugin_places = None;
            return;
        }
        let at = self.state.list_changes;
        if self
            .plugin_places
            .as_ref()
            .is_some_and(|kept| kept.at == at)
        {
            return;
        }
        let now = Places::read(at, &self.state.servers, &self.state.rooms);
        let Some(before) = self.plugin_places.replace(now.clone()) else {
            return;
        };
        let network = yapfast_plugin_api::event::Network::Discord;
        for change in before.changes(&now) {
            let (kind, room, payload) = match change {
                PlaceChange::ServerJoined { id, name } => (
                    EventKind::ServerJoin,
                    None,
                    serde_json::json!({ "server": id, "name": name }),
                ),
                PlaceChange::ServerRemoved { id, name } => (
                    EventKind::ServerRemove,
                    None,
                    serde_json::json!({ "server": id, "name": name }),
                ),
                PlaceChange::GroupRemoved { id, name } => (
                    EventKind::GroupDmRemove,
                    Some(id.clone()),
                    serde_json::json!({ "room": id, "name": name }),
                ),
            };
            self.plugins_observe(envelope(kind, network, room, None, true, payload));
        }
    }

    /// The member list as Yapfast made it, before plugins dressed anyone.
    pub(in crate::application) fn members_made(
        &self,
    ) -> Option<&Arc<Vec<crate::chat::ChatMember>>> {
        let now = self
            .state
            .members
            .as_ref()
            .and_then(|members| members.as_ref().ok())?;
        Some(
            self.plugin_dress
                .as_ref()
                .filter(|kept| {
                    kept.members_shown
                        .as_ref()
                        .is_some_and(|shown| Arc::ptr_eq(shown, now))
                })
                .and_then(|kept| kept.members_made.as_ref())
                .unwrap_or(now),
        )
    }

    /// Puts how plugins say people look on every surface that shows a person:
    /// the timeline's authors and mentions, the member list, the voice list
    /// and the typing strip. The people on screen are asked about first (a
    /// fresh answer costs nothing), and only rows a plugin changed are
    /// copied. What Yapfast made is kept beside, so an answer that goes puts
    /// Yapfast's own back.
    fn plugins_dress(&mut self) {
        let identifies = self
            .state
            .plugins
            .as_ref()
            .is_some_and(|plugins| plugins.identifies());
        let members_now = self
            .state
            .members
            .as_ref()
            .and_then(|members| members.as_ref().ok())
            .cloned();
        if !identifies {
            // Nothing dresses anyone: what is shown dressed goes back.
            if let Some(kept) = self.plugin_dress.take() {
                if Arc::ptr_eq(&self.state.messages, &kept.messages_shown) {
                    self.state.messages = kept.messages_made;
                }
                if let (Some(shown), Some(now), Some(made)) =
                    (&kept.members_shown, &members_now, kept.members_made)
                    && Arc::ptr_eq(shown, now)
                {
                    self.state.members = Some(Ok(made));
                }
            }
            return;
        }
        let changes = self
            .state
            .plugins
            .as_ref()
            .map_or(0, |plugins| plugins.identity_changes());
        let kept = self.plugin_dress.take().unwrap_or_default();
        let messages_new = !Arc::ptr_eq(&self.state.messages, &kept.messages_shown);
        let members_new = match (&members_now, &kept.members_shown) {
            (Some(now), Some(shown)) => !Arc::ptr_eq(now, shown),
            (None, None) => false,
            _ => true,
        };
        let messages_made = if messages_new {
            Arc::clone(&self.state.messages)
        } else {
            kept.messages_made
        };
        let members_made = if members_new {
            members_now.clone()
        } else {
            kept.members_made
        };
        let server = self.state.selected.as_deref().and_then(plugin_server);
        // The people on screen are asked about: a person with a fresh answer
        // costs a lookup.
        if messages_new || members_new {
            let me = self.state.discord_user.clone();
            let mut asked: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut requests = Vec::new();
            for message in messages_made.iter().rev().take(60) {
                let Some(person) = crate::discord::sender_user(&message.sender) else {
                    continue;
                };
                if person.is_empty() || !asked.insert(person.to_owned()) {
                    continue;
                }
                requests.push(yapfast_plugin_api::identity::IdentityRequest {
                    person: person.to_owned(),
                    server: server.clone(),
                    name: message.author.clone(),
                    username: None,
                    own: me.as_deref() == Some(person),
                });
            }
            for member in members_made
                .iter()
                .flat_map(|members| members.iter())
                .take(200)
            {
                if !asked.insert(member.id.clone()) {
                    continue;
                }
                requests.push(yapfast_plugin_api::identity::IdentityRequest {
                    person: member.id.clone(),
                    server: server.clone(),
                    name: member.name.clone(),
                    username: member.username.clone(),
                    own: me.as_deref() == Some(member.id.as_str()),
                });
            }
            if let Some(plugins) = self.state.plugins.as_mut() {
                for request in requests {
                    plugins.ask_identity(request);
                }
            }
        }
        let Some(plugins) = self.state.plugins.as_ref() else {
            return;
        };
        let look = |person: &str| plugins.identity_of(person, server.as_deref());
        let redress = messages_new || members_new || changes != kept.changes;
        let messages_shown = if redress {
            let mut changed = false;
            let dressed: Vec<Arc<crate::chat::ChatMessage>> = messages_made
                .iter()
                .map(
                    |message| match crate::plugins::dress::message(message, &look) {
                        Some(dressed) => {
                            changed = true;
                            Arc::new(dressed)
                        }
                        None => Arc::clone(message),
                    },
                )
                .collect();
            if changed {
                Arc::new(dressed)
            } else {
                Arc::clone(&messages_made)
            }
        } else {
            kept.messages_shown
        };
        let members_shown = if redress {
            members_made.as_ref().map(|members| {
                let mut changed = false;
                let dressed: Vec<crate::chat::ChatMember> = members
                    .iter()
                    .map(
                        |member| match crate::plugins::dress::member(member, &look) {
                            Some(dressed) => {
                                changed = true;
                                dressed
                            }
                            None => member.clone(),
                        },
                    )
                    .collect();
                if changed {
                    Arc::new(dressed)
                } else {
                    Arc::clone(members)
                }
            })
        } else {
            kept.members_shown
        };
        // The voice list and the typing strip are made again by every change
        // Yapfast hears, so they are dressed where they stand.
        for person in &mut self.state.voice.participants {
            if let Some(name) =
                look(&person.person).and_then(|identity| identity.display_name.clone())
            {
                person.user = name;
            }
        }
        if let (Some(room), Some(members)) = (self.state.selected.clone(), members_made.as_ref())
            && let Some(typing) = self.state.typing.get_mut(&room)
        {
            for name in typing.iter_mut() {
                let renamed = members
                    .iter()
                    .find(|member| member.name == *name)
                    .and_then(|member| look(&member.id))
                    .and_then(|identity| identity.display_name.clone());
                if let Some(renamed) = renamed {
                    *name = renamed;
                }
            }
        }
        if !Arc::ptr_eq(&self.state.messages, &messages_shown) {
            self.state.messages = Arc::clone(&messages_shown);
        }
        if let (Some(shown), Some(now)) = (&members_shown, &members_now)
            && !Arc::ptr_eq(shown, now)
        {
            self.state.members = Some(Ok(Arc::clone(shown)));
        }
        self.plugin_dress = Some(super::Dressed {
            changes,
            messages_made,
            messages_shown,
            members_made,
            members_shown,
        });
    }

    /// Tells the plugins with `people.read` that subscribed to `presence` when a
    /// friend's presence moved, wherever they are and whatever is on screen:
    /// the friends list Yapfast keeps from Discord's presences, compared when
    /// the people lists change. The event has no room, and `"friend": true`.
    /// The first read after a plugin starts listening says nothing.
    fn plugins_friend_presence(&mut self) {
        let at = self.state.people_changes;
        if self
            .plugin_friend_presences
            .as_ref()
            .is_some_and(|(kept, _)| *kept == at)
        {
            return;
        }
        let now: std::collections::BTreeMap<String, &'static str> = self
            .state
            .friends
            .iter()
            .map(|friend| {
                let presence = friend.presence.unwrap_or(crate::chat::Presence::Offline);
                (friend.id.clone(), crate::plugins::presence::name(presence))
            })
            .collect();
        let Some((_, before)) = self.plugin_friend_presences.replace((at, now.clone())) else {
            return;
        };
        let network = yapfast_plugin_api::event::Network::Discord;
        for (person, presence) in &now {
            if before.get(person) == Some(presence) || !before.contains_key(person) {
                continue;
            }
            let name = self
                .state
                .friends
                .iter()
                .find(|friend| friend.id == *person)
                .map(|friend| friend.name.clone());
            self.plugins_observe(envelope(
                EventKind::Presence,
                network,
                None,
                Some(person.clone()),
                false,
                serde_json::json!({ "person": person, "presence": presence, "name": name, "friend": true }),
            ));
        }
    }

    /// Tells the plugins with `activity.read` when the game Yapfast sees this
    /// account playing starts and stops: one changing to another is a stop and
    /// a start. The first read after a plugin starts listening says nothing.
    fn plugins_game(&mut self) {
        let playing = self.state.discord_playing.as_ref();
        let now = playing.map(|playing| playing.name.clone());
        let Some(before) = self.plugin_game.replace(now.clone()) else {
            return;
        };
        if before == now {
            return;
        }
        let network = yapfast_plugin_api::event::Network::Discord;
        if let Some(name) = before {
            self.plugins_observe(envelope(
                EventKind::GameStop,
                network,
                None,
                None,
                true,
                serde_json::json!({ "name": name }),
            ));
        }
        if let Some(name) = now {
            let started = self
                .state
                .discord_playing
                .as_ref()
                .and_then(|playing| playing.started_ms);
            self.plugins_observe(envelope(
                EventKind::GameStart,
                network,
                None,
                None,
                true,
                serde_json::json!({ "name": name, "started_ms": started }),
            ));
        }
    }

    /// Tells the plugins with `drafts.read` that subscribed to
    /// `composer_change` what a composer holds now, coalesced like typing:
    /// nothing is built while none listens.
    pub(in crate::application) fn plugins_composer(&mut self, room: &str, text: &str) {
        if !self.plugins_want(EventKind::ComposerChange) {
            return;
        }
        let network = plugin_network(Network::of(room));
        self.plugins_observe(envelope(
            EventKind::ComposerChange,
            network,
            Some(room.to_owned()),
            None,
            true,
            serde_json::json!({ "room": room, "text": text }),
        ));
    }

    /// Finds the messages plugins sent among the account's own in the open
    /// room, and says under each which plugin sent it. A send nobody has seen
    /// arrive in ten minutes is let go.
    fn plugins_attribute_sent(&mut self) {
        let now = crate::clock::now_ms();
        self.plugin_sent_pending
            .retain(|(_, _, _, at)| now.saturating_sub(*at) < 10 * 60 * 1000);
        let Some(room) = self.state.selected.clone() else {
            return;
        };
        let mut found = false;
        let messages = Arc::clone(&self.state.messages);
        for message in messages.iter().rev().take(50) {
            if !message.own || self.state.plugin_sent_by.contains_key(&message.item) {
                continue;
            }
            let Some(at) = self
                .plugin_sent_pending
                .iter()
                .position(|(sent_in, body, _, _)| *sent_in == room && *body == message.body)
            else {
                continue;
            };
            let (_, _, name, _) = self.plugin_sent_pending.remove(at);
            self.state.plugin_sent_by.insert(message.item.clone(), name);
            found = true;
        }
        if found {
            // The rows that now say who sent them are laid out again.
            self.state.messages = Arc::new(self.state.messages.as_ref().clone());
        }
    }

    /// Tells the plugins subscribed to `member_join` and `member_leave` who
    /// appeared in the open room's member list and who left it, one event per
    /// person, coalesced like presence.
    ///
    /// The list is the one Yapfast holds for the conversation on screen: in a
    /// DM, a group or a small server that is everyone in it, and in a large
    /// server it is the part of the list Discord has sent. The first list of a
    /// conversation says nothing, as presence's does.
    pub(super) fn plugins_roster(
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
        let (joined, left) = plugins.roster_changes(&room, &members);
        if joined.is_empty() && left.is_empty() {
            return;
        }
        let network = plugin_network(Network::of(&room));
        let own = |person: &str| account.as_deref() == Some(person);
        let mut envelopes: Vec<EventEnvelope> = Vec::with_capacity(joined.len() + left.len());
        for (person, name) in joined {
            envelopes.push(envelope(
                EventKind::MemberJoin,
                network,
                Some(room.clone()),
                Some(person.clone()),
                own(&person),
                serde_json::json!({ "person": person, "name": name }),
            ));
        }
        for person in left {
            envelopes.push(envelope(
                EventKind::MemberLeave,
                network,
                Some(room.clone()),
                Some(person.clone()),
                own(&person),
                serde_json::json!({ "person": person }),
            ));
        }
        for event in envelopes {
            self.plugins_observe(event);
        }
    }

    /// Tells the plugins subscribed to `presence` who came online, went idle or
    /// left, one event per person whose presence moved.
    ///
    /// `presence` is opt-in and coalesced, so a member list that arrives with
    /// twenty changes is one batch: the bridge gathers a burst of them into a
    /// single call, the way it does typing. Nothing is built while nobody
    /// subscribed.
    pub(super) fn plugins_presence(
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
        // Asked before anything is compared: the member list is a `BTreeMap` of
        // every person in the server, and nobody paying for it is nobody to
        // tell.
        if !plugins.subscribes(EventKind::Presence) {
            return;
        }
        let changes = plugins.presence_changes(&members);
        if changes.is_empty() {
            return;
        }
        let network = plugin_network(Network::of(&room));
        let envelopes: Vec<EventEnvelope> = changes
            .into_iter()
            .map(|(person, presence)| {
                let own = account.as_deref() == Some(person.as_str());
                let name = members
                    .iter()
                    .find(|member| member.id == person)
                    .map(|member| member.name.clone());
                envelope(
                    EventKind::Presence,
                    network,
                    Some(room.clone()),
                    Some(person.clone()),
                    own,
                    serde_json::json!({ "person": person, "presence": presence, "name": name }),
                )
            })
            .collect();
        for event in envelopes {
            self.plugins_observe(event);
        }
    }

    /// Asks the plugins that draw under messages what they draw under the rows
    /// on screen.
    ///
    /// Only the rows the person can see are asked about, and only when one of
    /// them draws: a channel with no such plugin costs one load of a flag.
    pub(in crate::application) fn plugins_decorate_visible(
        &mut self,
        room: &str,
        items: &[String],
    ) {
        if !self.plugins_on() {
            return;
        }
        let Some(plugins) = self.state.plugins.as_ref() else {
            return;
        };
        if !plugins.decorates() {
            return;
        }
        // An answer lands in the slots, and with none there is nowhere to
        // draw it: the interface makes them once it has built the enabled
        // plugins, and until then asking would cost a helper round trip per
        // row on screen for an answer that is dropped.
        if self.state.plugin_slots.is_none() {
            return;
        }
        // The timeline is the open room's, which is the room these rows are in:
        // `Action::VisibleMessages` only reports the selected conversation.
        // As Yapfast made it, not as a plugin's identity dressed it.
        let timeline = self
            .plugin_dress
            .as_ref()
            .filter(|kept| Arc::ptr_eq(&kept.messages_shown, &self.state.messages))
            .map_or(&self.state.messages, |kept| &kept.messages_made);
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
                    pinned: message.pinned,
                    stickers: crate::plugins::world::chat_stickers(message)
                        .into_iter()
                        .map(|sticker| sticker.id)
                        .collect(),
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
    pub(in crate::application) fn plugins_identify(
        &mut self,
        room: &str,
        profile: &mut crate::chat::Profile,
    ) {
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
    pub(super) fn plugins_identity_arrived(
        &mut self,
        person: &str,
        server: Option<&str>,
    ) -> Vec<AppEffect> {
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
                .map(Arc::make_mut)
                .filter(|view| view.profile.id == person)
            {
                decorate_profile(&mut view.profile, &identity);
            }
            if let Some(view) = self
                .state
                .panel_profile
                .as_mut()
                .map(Arc::make_mut)
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
    pub(in crate::application) fn plugins_colour_code_visible(
        &mut self,
        _room: &str,
        items: &[String],
    ) {
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
    pub(in crate::application) fn plugins_decorate_members(&mut self, room: &str, ids: &[String]) {
        if !self.plugins_on() {
            return;
        }
        // The server's owner, role names and role permissions, which a badge
        // that marks owners, admins or a role reads.
        let guild = self.discord_place(room).map(|(server, _, _)| {
            (
                server.owner_id.clone(),
                Arc::clone(&server.role_names),
                Arc::clone(&server.roles),
                server.id.clone(),
            )
        });
        // The people as Yapfast made them, not as a plugin's identity dressed
        // them.
        let Some(members) = self.members_made().cloned() else {
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
                            role_names: guild
                                .as_ref()
                                .map(|(_, names, _, _)| {
                                    member
                                        .roles
                                        .iter()
                                        .map(|role| names.get(role).cloned().unwrap_or_default())
                                        .collect()
                                })
                                .unwrap_or_default(),
                            owner: guild.as_ref().is_some_and(|(owner, _, _, _)| {
                                owner.as_deref() == Some(member.id.as_str())
                            }),
                            permissions: guild
                                .as_ref()
                                .map(|(_, _, bits, everyone)| {
                                    let everyone = bits.get(everyone).copied().unwrap_or(0);
                                    member
                                        .roles
                                        .iter()
                                        .filter_map(|role| bits.get(role))
                                        .fold(everyone, |all, bits| all | bits)
                                        .to_string()
                                })
                                .unwrap_or_default(),
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

    /// Whether an enabled plugin subscribed to `kind`: checked before an
    /// event's payload is built, so a kind nobody listens to costs no JSON.
    fn plugins_want(&self, kind: EventKind) -> bool {
        self.plugins_on()
            && self
                .state
                .plugins
                .as_ref()
                .is_some_and(|plugins| plugins.subscribes(kind))
    }

    /// Hands one event to the plugins that subscribed to it. The bridge
    /// applies each subscription's filter and writes the payload once, so a
    /// tap only has to say what happened.
    pub(in crate::application) fn plugins_observe(&mut self, mut envelope: EventEnvelope) {
        if self.state.plugins.is_none() {
            return;
        }
        if !self
            .state
            .plugins
            .as_ref()
            .is_some_and(|plugins| plugins.subscribes(envelope.kind))
        {
            return;
        }
        // The room's name as the sidebar shows it, so a plugin that logs or
        // notifies needs no `rooms.get` for it.
        if let (Some(room), serde_json::Value::Object(payload)) =
            (envelope.room.as_deref(), &mut envelope.payload)
            && !payload.contains_key("room_name")
            && let Some(listed) = self.state.rooms.iter().find(|listed| listed.id == room)
        {
            payload.insert(
                "room_name".to_owned(),
                serde_json::Value::String(listed.name.clone()),
            );
        }
        if let Some(plugins) = self.state.plugins.as_mut() {
            plugins.observe(envelope);
        }
    }

    /// The account opened a conversation, or the same one again to retry it.
    /// What the sidebar shows of it goes along, so a plugin can tell a DM from
    /// a channel without asking.
    pub(in crate::application) fn plugins_room_selected(&mut self, room: &str) {
        if self.state.selected.as_deref() == Some(room)
            || !self.plugins_want(EventKind::RoomSelected)
        {
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
            nsfw: listed.mark.age_restricted,
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
    pub(in crate::application) fn plugins_focus(&mut self, focused: bool) {
        if !self.plugins_want(EventKind::AppFocus) {
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
    pub(in crate::application) fn plugins_voice(
        &mut self,
        before: &crate::voice::State,
        after: &crate::voice::State,
    ) {
        if !self.plugins_on() {
            return;
        }
        // `voice.state` answers from the world, so the call is published
        // whether or not anyone subscribed to its changes.
        let view = crate::plugins::world::voice_view(after);
        if let Some(plugins) = self.state.plugins.as_ref() {
            plugins.world().publish_voice(view.clone());
        }
        // Speaking moves many times a second, so it is its own coalesced event
        // and is compared before the early return below: a call where only the
        // speaking changed is exactly the case this tap exists for.
        self.plugins_speaking(before, after);
        if !self.plugins_want(EventKind::VoiceState) {
            return;
        }
        let same_call = before.room == after.room
            && before.phase == after.phase
            && before.muted == after.muted
            && before.deafened == after.deafened
            && before.sharing.is_some() == after.sharing.is_some()
            && before.camera_phase == after.camera_phase
            && before.watching.as_ref().map(|watching| &watching.user)
                == after.watching.as_ref().map(|watching| &watching.user)
            && before.participants.len() == after.participants.len()
            && before
                .participants
                .iter()
                .zip(&after.participants)
                .all(|(was, now)| {
                    was.person == now.person
                        && was.muted == now.muted
                        && was.deafened == now.deafened
                        && was.server_muted == now.server_muted
                        && was.streaming == now.streaming
                        && was.video.is_some() == now.video.is_some()
                });
        if same_call {
            return;
        }
        // The call as `voice.state` answers it; a call that ended says so with
        // no room and nobody in it.
        let payload = match &view {
            Some(view) => serde_json::to_value(view).unwrap_or_default(),
            None => serde_json::json!({ "room": null, "people": [] }),
        };
        self.plugins_observe(envelope(
            EventKind::VoiceState,
            after.room.as_deref().map_or_else(
                || self.on_screen_network(),
                |room| plugin_network(Network::of(room)),
            ),
            after.room.clone(),
            None,
            false,
            payload,
        ));
    }

    /// Tells the plugins subscribed to `speaking` who started and who stopped,
    /// which is what `discord_audio` hears in the call's frames.
    ///
    /// One event per person who changed, gathered into one batch by the bridge:
    /// a room where four people talk is four events a quarter second, not four
    /// hundred. Nothing is built while nobody subscribed.
    pub(super) fn plugins_speaking(
        &mut self,
        before: &crate::voice::State,
        after: &crate::voice::State,
    ) {
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

    /// How long this start took, once a session, for the plugins with
    /// `app.timing`: sent when Discord's gateway first says the account is
    /// online, and, for a plugin turned on later, never again this session.
    /// Nothing in it is about a person.
    pub(in crate::application) fn plugins_startup_timing(&mut self) {
        if self.plugin_timing_sent || !self.plugins_want(EventKind::StartupTiming) {
            return;
        }
        self.plugin_timing_sent = true;
        let timing = crate::startup::timing();
        self.plugins_observe(envelope(
            EventKind::StartupTiming,
            self.on_screen_network(),
            None,
            None,
            false,
            serde_json::json!({
                "window_ms": timing.window_ms,
                "ready_ms": timing.ready_ms,
            }),
        ));
    }

    /// The account signed in and its lists are here, so the calls that read
    /// them can answer.
    pub(in crate::application) fn plugins_account_ready(&mut self, account: &str, trusted: bool) {
        if !self.plugins_want(EventKind::AccountReady) {
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
}
