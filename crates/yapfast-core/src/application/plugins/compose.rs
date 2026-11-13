//! The send path's interception: the composer's draft, an edit's text, a typing
//! indicator and an upload, before any of them becomes a message.
//!
//! A chain that answers at once leaves no trace; one that holds clears the
//! composer and lets the answer finish the send, so nobody waits on a plugin.
//! Nothing here runs while no plugin intercepts, which is the first check each
//! function makes.

use super::*;

impl Application {
    /// The send path's interception: hands a composer's draft to the
    /// `message.pre_send` chain, before any of the send itself is composed.
    ///
    /// `None` leaves the send exactly as it was, which is what a build
    /// without plugins, a run without one enabled, and a chain that answered
    /// without changing anything all do.
    pub(in crate::application) fn plugins_send(&mut self, room: &str) -> Option<Vec<AppEffect>> {
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
        let draft = self.plugins_draft(room);
        let decision = self.state.plugins.as_mut()?.pre_send(draft);
        self.plugins_answer(room, decision)
    }

    /// The edit path's interception: hands an edit's text to the
    /// `message.pre_edit` chain, before any of the edit itself is composed.
    ///
    /// `None` leaves the edit exactly as it was, which is what a build without
    /// plugins, a run without one enabled, and a chain that answered without
    /// changing anything all do. An edit has its own chain and its own answer:
    /// a `Changed` draft replaces the text the person is editing, and a held
    /// chain keeps the edit back until it answers rather than sending it.
    #[cfg(feature = "plugins")]
    pub(in crate::application) fn plugins_edit(&mut self) -> Option<Vec<AppEffect>> {
        // A preview has no account and no helper to ask, and a plugin system
        // with nothing intercepting has nothing to ask either. An edit a plugin
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
        let edit = self.state.edit.clone()?;
        let draft = Draft {
            room: edit.room.clone(),
            body: edit.text,
            // An edit is its own field, apart from a reply the composer has
            // open, so the chain sees no reply to keep.
            reply_to: None,
            attachments: Vec::new(),
        };
        let decided = self.state.plugins.as_mut()?.pre_edit(draft);
        self.plugins_edit_answer(decided)
    }

    /// Carries out what a chain decided about an edit.
    ///
    /// Separate from [`Self::plugins_edit`], as [`Self::plugins_answer`] is
    /// from [`Self::plugins_send`], so that the answer can be replayed without
    /// a bridge: a `Changed` draft has already replaced the edit's text, and a
    /// hold leaves the edit where it is until the answer comes.
    pub(super) fn plugins_edit_answer(&mut self, decided: PreSend) -> Option<Vec<AppEffect>> {
        match decided {
            PreSend::Send(draft) => {
                if let Some(edit) = self.state.edit.as_mut() {
                    edit.text = draft.body;
                }
                None
            }
            PreSend::Held { .. } => {
                // The answer applies the same edit; nothing goes here.
                let room = self.state.edit.as_ref()?.room.clone();
                self.state.sending = Some(room);
                Some(Vec::new())
            }
        }
    }

    /// The draft a hook sees: the composer's text as the person typed it,
    /// before its mentions are wired and its `@silent` taken off.
    pub(super) fn plugins_draft(&self, room: &str) -> Draft {
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
    pub(super) fn plugins_answer(
        &mut self,
        room: &str,
        decision: PreSend,
    ) -> Option<Vec<AppEffect>> {
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
    pub(in crate::application) fn plugins_typing(&mut self, room: &str) -> bool {
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
    pub(in crate::application) fn plugins_upload(
        &mut self,
        room: &str,
        file: &crate::chat::Staged,
    ) -> bool {
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
    pub(super) fn plugins_upload_answered(
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
    pub(super) fn plugins_send_held(&mut self, draft: Draft) -> Vec<AppEffect> {
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
    pub(super) fn plugins_send_blocked(&mut self, reason: String) -> Vec<AppEffect> {
        // Only one send can wait on a chain, so the room it waits in is the
        // one `sending` names.
        let room = self.state.sending.take();
        let text = self.sent_draft.take();
        // An edit that was blocked leaves the edit as it was: there is
        // nothing to put back, only the reason to show.
        if room.as_ref().is_some_and(|room| {
            self.state
                .edit
                .as_ref()
                .is_some_and(|edit| &edit.room == room)
        }) {
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
    pub(in crate::application) fn plugins_typed_command(
        &mut self,
        room: &str,
    ) -> Option<Vec<AppEffect>> {
        if self.state.preview
            || !self.plugins_on()
            || self
                .state
                .edit
                .as_ref()
                .is_some_and(|edit| edit.room == room)
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
}
