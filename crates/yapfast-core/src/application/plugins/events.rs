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
    pub(in crate::application) fn plugins_sync_world(&mut self) {
        let Some(plugins) = self.state.plugins.as_ref() else {
            return;
        };
        let world = plugins.world();
        // Presence is opt-in and coalesced, so it is asked once here: while no
        // plugin wants it there is nothing to compare, nothing kept, and the
        // room, the member list and the account stay where they are rather than
        // being cloned out of the borrow below.
        let wants_presence = plugins.subscribes(EventKind::Presence);
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
        if !wants_presence {
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
        self.plugins_presence(room, members, account);
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
    pub(in crate::application) fn plugins_observe(&mut self, envelope: EventEnvelope) {
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
    pub(in crate::application) fn plugins_room_selected(&mut self, room: &str) {
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
    pub(in crate::application) fn plugins_focus(&mut self, focused: bool) {
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
    pub(in crate::application) fn plugins_voice(
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

    /// The account signed in and its lists are here, so the calls that read
    /// them can answer.
    pub(in crate::application) fn plugins_account_ready(&mut self, account: &str, trusted: bool) {
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
}
