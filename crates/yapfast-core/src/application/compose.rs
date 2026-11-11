//! The composer: drafts, replies and edits, mentions, files and sending.

use super::*;

impl Application {
    pub(super) fn edit_last(&mut self) -> Vec<Effect> {
        // Up in an empty option of a command being written, or in the `/`
        // picker, is the command's.
        if self.state.command_draft().is_some() || self.state.slash_query().is_some() {
            return vec![];
        }
        let last = self
            .state
            .messages
            .iter()
            .rev()
            .find(|message| message.own && message.can_edit && !message.deleted)
            .map(|message| message.item.clone());
        match last {
            Some(item) => self.dispatch(Action::Edit(item)),
            None => vec![],
        }
    }

    pub(super) fn mention_query(&mut self, query: Option<(char, String)>) -> Vec<Effect> {
        self.state.mention_query = query.clone();
        // Discord servers list only some members; ask for the rest by name.
        let (Some(room), Some(('@', query))) = (self.state.selected.clone(), query) else {
            return vec![];
        };
        let in_server =
            discord::parse_room(&room).is_some_and(|(guild, _)| guild != discord::DM_GUILD);
        if self.state.preview
            || !in_server
            || query.is_empty()
            || self
                .state
                .mention_suggestions()
                .iter()
                .filter(|candidate| !candidate.group)
                .count()
                >= MENTION_SUGGESTIONS
            || !self.searched.insert(query.to_lowercase())
        {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::SearchMembers { room, query },
        ))]
    }

    pub(super) fn set_draft(&mut self, body: String) -> Vec<Effect> {
        let Some(id) = self.state.selected.clone() else {
            return vec![];
        };
        // `/` opens the app command picker; with a command being written,
        // the text is its focused option's. See application/slash.rs.
        match self.slash_draft(&id, &body) {
            Some(mut effects) if self.state.command_draft().is_none() => {
                effects.extend(self.set_draft_in(id, body));
                effects
            }
            Some(effects) => effects,
            None => self.set_draft_in(id, body),
        }
    }

    /// Changes the draft of a room on screen: the selected one, or the
    /// thread beside it.
    pub(super) fn set_draft_in(&mut self, id: String, body: String) -> Vec<Effect> {
        if self
            .state
            .drafts
            .get(&id)
            .map_or(body.is_empty(), |draft| draft == &body)
        {
            return vec![];
        }
        self.state.drafts.insert(id.clone(), body.clone());
        if self.state.preview {
            vec![]
        } else {
            let typing = !body.trim().is_empty();
            vec![
                Effect::Backend(Command::Typing {
                    room: id.clone(),
                    typing,
                }),
                Effect::Backend(Command::SaveDraft { room: id, body }),
            ]
        }
    }

    pub(super) fn reply(&mut self, item: String) -> Vec<Effect> {
        let (Some(id), Some(message)) = (
            self.state.room_of(&item).cloned(),
            self.state.message(&item),
        ) else {
            return vec![];
        };
        if !message.can_reply || !self.state.can_write_in(&id) {
            return vec![];
        }
        let reply = Compose::Reply {
            item,
            author: message.author.clone(),
            body: chat::plain_snippet(&message.body),
            ping: self.state.reply_ping,
        };
        self.state.compose.insert(id, reply);
        vec![]
    }

    /// Opens a message of this account's for editing in place, in a field of
    /// its own: the room's draft and any reply stay as they are.
    pub(super) fn begin_edit(&mut self, item: String) -> Vec<Effect> {
        let (Some(room), Some(message)) = (
            self.state.room_of(&item).cloned(),
            self.state.message(&item),
        ) else {
            return vec![];
        };
        if !message.can_edit || !self.state.can_write_in(&room) {
            return vec![];
        }
        let text = mention::as_typed(&message.body, &message.mentions);
        self.state.edit = Some(Editing {
            room,
            item,
            original: text.clone(),
            text,
        });
        vec![]
    }

    pub(super) fn set_edit_text(&mut self, text: String) -> Vec<Effect> {
        if let Some(edit) = &mut self.state.edit {
            edit.text = text;
        }
        vec![]
    }

    /// Closes the edit, and the question about deleting its message if that
    /// was asked.
    pub(super) fn cancel_edit(&mut self) -> Vec<Effect> {
        if let Some(edit) = self.state.edit.take()
            && self.state.confirm_delete.as_ref() == Some(&edit.item)
        {
            self.state.confirm_delete = None;
        }
        vec![]
    }

    /// Saves the edit. Text that didn't change closes it without asking
    /// Discord; no text at all asks whether to delete the message, unless
    /// files stay on it, which an empty text may be left with.
    pub(super) fn save_edit(&mut self) -> Vec<Effect> {
        let Some(edit) = self.state.edit.clone() else {
            return vec![];
        };
        if self.state.sending.is_some() || !self.state.can_write_in(&edit.room) {
            return vec![];
        }
        if edit.text.trim() == edit.original.trim() {
            self.state.edit = None;
            return vec![];
        }
        if edit.text.trim().is_empty()
            && !self
                .state
                .message(&edit.item)
                .is_some_and(|message| !message.attachments.is_empty())
        {
            return self.request_delete(edit.item);
        }
        let body = self.outgoing_body(&edit.room, &edit.text);
        if body.len() > chat::MAX_MESSAGE_BYTES {
            self.state.notice = Some("Keep messages under 16 KB.".into());
            return vec![];
        }
        if self.state.preview {
            self.update_sample(&edit.room, &edit.item, |message| {
                message.body = body.clone();
                message.formatted = has_markdown(&body);
                message.edited = true;
            });
            self.state.edit = None;
            return vec![];
        }
        self.state.sending = Some(edit.room.clone());
        self.state.notice = None;
        self.state.mention_query = None;
        vec![Effect::Backend(Command::Edit {
            room: edit.room,
            item: edit.item,
            body,
        })]
    }

    /// What a message carries on the wire for text as typed in a room: links
    /// without tracking, mentions and custom emoji written as Discord has them.
    fn outgoing_body(&self, room: &str, text: &str) -> String {
        let text = if self.state.strip_tracking {
            crate::links::without_tracking(text)
        } else {
            std::borrow::Cow::Borrowed(text)
        };
        if self.state.preview {
            return text.into_owned();
        }
        let wired = mention::wire(&text, &self.state.mention_candidates());
        if Network::of(room) == Network::Discord {
            let usable: Vec<chat::CustomEmoji> = self
                .usable_emoji()
                .into_iter()
                .flat_map(|set| set.emoji)
                .collect();
            chat::wire_emoji(&wired, &usable)
        } else {
            wired
        }
    }

    pub(super) fn unstage(&mut self, index: usize) -> Vec<Effect> {
        match self.state.selected.clone() {
            Some(id) => self.unstage_in(id, index),
            None => vec![],
        }
    }

    /// Takes the file at `index` out of a room's next message.
    pub(super) fn unstage_in(&mut self, room: String, index: usize) -> Vec<Effect> {
        if let Some(staged) = self.state.staged.get_mut(&room)
            && index < staged.len()
        {
            staged.remove(index);
        }
        vec![]
    }

    pub(super) fn toggle_spoiler(&mut self, index: usize) -> Vec<Effect> {
        match self.state.selected.clone() {
            Some(id) => self.toggle_spoiler_in(id, index),
            None => vec![],
        }
    }

    /// Marks the file at `index` of a room's next message as a spoiler, or
    /// not, where the room has them.
    pub(super) fn toggle_spoiler_in(&mut self, room: String, index: usize) -> Vec<Effect> {
        let spoilers = self
            .state
            .rooms
            .iter()
            .find(|listed| listed.id == room)
            .is_some_and(|listed| listed.capabilities.spoilers);
        if spoilers
            && let Some(file) = self
                .state
                .staged
                .get_mut(&room)
                .and_then(|staged| staged.get_mut(index))
        {
            file.spoiler = !file.spoiler;
        }
        vec![]
    }

    pub(super) fn send(&mut self) -> Vec<Effect> {
        match self.state.selected.clone() {
            Some(id) if self.state.command_draft().is_some() => self.run_written_command(&id),
            // Until the room's commands come, `/` and what follows may be one
            // of them: it isn't sent as a message.
            Some(id)
                if self.state.slash_query().is_some() && self.state.app_commands.loading(&id) =>
            {
                vec![]
            }
            Some(id) => self.send_in(id),
            None => vec![],
        }
    }

    /// Sends `text` in a room as its composer would, leaving what the
    /// composer holds as it was.
    pub(super) fn send_text_in(&mut self, room: String, text: String) -> Vec<Effect> {
        let draft = self.state.drafts.insert(room.clone(), text);
        let compose = self.state.compose.remove(&room);
        let staged = self.state.staged.remove(&room);
        let effects = self.send_in(room.clone());
        match draft {
            Some(draft) => self.state.drafts.insert(room.clone(), draft),
            None => self.state.drafts.remove(&room),
        };
        if let Some(compose) = compose {
            self.state.compose.insert(room.clone(), compose);
        }
        if let Some(staged) = staged {
            self.state.staged.insert(room, staged);
        }
        effects
    }

    /// Sends what the composer of a room on screen holds: the selected
    /// room's, or the thread's beside it.
    pub(super) fn send_in(&mut self, id: String) -> Vec<Effect> {
        if !self.state.can_send_in(&id) {
            return vec![];
        }
        let draft = self.state.drafts.get(&id).cloned().unwrap_or_default();
        let compose = self.state.compose.get(&id).cloned();
        // Discord's `@silent` at the start sends a message that notifies
        // nobody; Matrix has no such thing. An edit can't change it.
        let silent_text = draft
            .strip_prefix("@silent")
            .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
            .filter(|_| Network::of(&id) == Network::Discord);
        let silent = silent_text.is_some();
        let text = silent_text.map_or(draft.as_str(), str::trim_start);
        let body = self.outgoing_body(&id, text);
        if body.len() > chat::MAX_MESSAGE_BYTES {
            self.state.notice = Some("Keep messages under 16 KB.".into());
            return vec![];
        }
        if self.state.preview {
            let item = uuid::Uuid::new_v4().to_string();
            let messages = self.samples.entry(id.clone()).or_default();
            messages.push(Arc::new(ChatMessage {
                id: item.clone(),
                item,
                author: "You".into(),
                sender: "You".into(),
                formatted: has_markdown(&body),
                body,
                time: "Local preview".into(),
                own: true,
                reply: match compose {
                    Some(Compose::Reply {
                        item, author, body, ..
                    }) => Some(Reply {
                        author,
                        avatar: self
                            .state
                            .message(&item)
                            .and_then(|original| original.avatar.clone()),
                        color: self
                            .state
                            .message(&item)
                            .and_then(|original| original.color),
                        body,
                        item: Some(item),
                    }),
                    _ => None,
                },
                ..sample_actions()
            }));
            self.state.messages = Arc::new(messages.clone());
            self.state.drafts.remove(&id);
            self.state.compose.remove(&id);
            return vec![];
        }
        self.state.sending = Some(id.clone());
        self.state.notice = None;
        self.state.mention_query = None;
        self.sent_draft = Some(draft);
        let files: Vec<chat::OutgoingFile> = self
            .state
            .staged
            .get(&id)
            .into_iter()
            .flatten()
            .map(|file| chat::OutgoingFile {
                path: file.path.clone(),
                spoiler: file.spoiler,
            })
            .collect();
        self.sent_files = files.iter().map(|file| file.path.clone()).collect();
        // A message goes after the newest, which show again for it.
        let mut effects = if self.state.selected.as_ref() == Some(&id) {
            self.jump_to_present()
        } else {
            vec![]
        };
        effects.push(Effect::Backend(Command::Send {
            room: id,
            body,
            options: chat::SendOptions {
                quiet_reply: matches!(compose, Some(Compose::Reply { ping: false, .. })),
                silent,
            },
            reply: match compose {
                Some(Compose::Reply { item, .. }) => Some(item),
                _ => None,
            },
            files,
        }));
        effects
    }

    /// Adds files to the selected room's next message, saying why any can't go.
    pub(super) fn stage(&mut self, paths: Vec<PathBuf>) {
        // Checked here rather than with can_attach, which a full message also
        // turns off, so a paste that can't be added says why.
        if !self.state.can_write() || self.state.preview || paths.is_empty() {
            return;
        }
        // A forum's files go with a post, whose form they open, as Discord's
        // client opens it for files dropped on a forum.
        if self.state.forum().is_some() {
            if self.state.new_post().is_some_and(|post| post.posting) {
                return;
            }
            self.open_new_post();
            if self.state.new_post().is_none() {
                return;
            }
        }
        if let Some(id) = self.state.selected.clone() {
            self.stage_in(id, paths);
        }
    }

    /// Adds files to the thread beside's next message, saying why any can't go.
    pub(super) fn stage_aside(&mut self, paths: Vec<PathBuf>) {
        let Some(id) = self.state.aside.as_ref().map(|aside| aside.room.clone()) else {
            return;
        };
        if !self.state.can_write_in(&id) || self.state.preview || paths.is_empty() {
            return;
        }
        self.stage_in(id, paths);
    }

    /// Adds files to a room's next message, up to as many and as large as
    /// it takes, saying why any can't go.
    fn stage_in(&mut self, id: String, paths: Vec<PathBuf>) {
        let Some(limit) = self
            .state
            .rooms
            .iter()
            .find(|room| room.id == id)
            .map(|room| room.upload_limit)
        else {
            return;
        };
        let staged = self.state.staged.entry(id).or_default();
        let mut problem = None;
        for path in paths {
            if staged.len() >= chat::MAX_FILES {
                problem = Some(format!(
                    "A message carries up to {} files.",
                    chat::MAX_FILES
                ));
                break;
            }
            match chat::Staged::read(path, limit) {
                Ok(file) => staged.push(file),
                Err(error) => problem = problem.or(Some(error)),
            }
        }
        self.state.notice = problem;
    }

    /// Leave reply mode.
    pub(super) fn finish_compose(&mut self, room: &str) {
        self.state.compose.remove(room);
    }
}

/// Preview messages render as Markdown when they contain its punctuation.
pub(super) fn has_markdown(body: &str) -> bool {
    body.contains(['*', '_', '`', '~', '['])
}
