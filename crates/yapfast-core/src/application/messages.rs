//! Acting on messages: reactions, deleting, forwarding, retrying, attachments and the image viewer, and loading history.

use super::*;

impl Application {
    pub(super) fn react(&mut self, item: String, key: String) -> Vec<Effect> {
        let Some(room) = self.state.room_of(&item).cloned() else {
            return vec![];
        };
        if !self.state.can_write_in(&room)
            || !self
                .state
                .message(&item)
                .is_some_and(|message| message.can_react)
        {
            return vec![];
        }
        let adding = !self.state.message(&item).is_some_and(|message| {
            message
                .reactions
                .iter()
                .any(|reaction| reaction.key == key && reaction.mine)
        });
        if adding {
            *self.reaction_uses.entry(key.clone()).or_default() += 1;
        }
        if self.state.preview {
            self.update_sample(&room, &item, |message| {
                if let Some(index) = message
                    .reactions
                    .iter()
                    .position(|reaction| reaction.key == key)
                {
                    let reaction = &mut message.reactions[index];
                    reaction.mine = !reaction.mine;
                    reaction.count = if reaction.mine {
                        reaction.count + 1
                    } else {
                        reaction.count.saturating_sub(1)
                    };
                    if reaction.count == 0 {
                        message.reactions.remove(index);
                    }
                } else {
                    message.reactions.push(Reaction {
                        key,
                        count: 1,
                        mine: true,
                    });
                }
            });
            vec![]
        } else {
            vec![Effect::Backend(Command::React { room, item, key })]
        }
    }

    pub(super) fn request_delete(&mut self, item: String) -> Vec<Effect> {
        if self
            .state
            .message(&item)
            .is_some_and(|message| message.can_delete)
        {
            self.state.confirm_delete = Some(item);
        }
        vec![]
    }

    pub(super) fn pin_message(&mut self, item: String) -> Vec<Effect> {
        let Some(room) = self.state.room_of(&item).cloned() else {
            return vec![];
        };
        let Some(pinned) = self
            .state
            .message(&item)
            .filter(|message| message.can_pin)
            .map(|message| !message.pinned)
        else {
            return vec![];
        };
        if self.state.preview {
            self.update_sample(&room, &item, |message| message.pinned = pinned);
            vec![]
        } else {
            vec![Effect::Backend(Command::Pin { room, item, pinned })]
        }
    }

    pub(super) fn start_forward(&mut self, item: String) -> Vec<Effect> {
        let Some(room) = self.state.room_of(&item).cloned() else {
            return vec![];
        };
        // Matrix forwards only from a verified device.
        let discord = Network::of(&room) == Network::Discord;
        let allowed = self.state.preview || self.state.trusted || discord;
        self.state.forwarding = self
            .state
            .message(&item)
            .filter(|message| allowed && message.can_forward)
            .map(|message| Forwarding {
                quote: match message.attachments.first() {
                    Some(attachment) if message.body.is_empty() => attachment.name.clone(),
                    _ => chat::plain_snippet(&message.body),
                },
                room,
                item,
                ..Default::default()
            });
        // Discord's dialog ranks places by how much they're used, which
        // Discord's settings count.
        if self.state.forwarding.is_some() && discord {
            return self.ask_places_used();
        }
        vec![]
    }

    /// Chooses a conversation to forward to, up to [`FORWARD_LIMIT`] of
    /// them, or leaves one chosen out.
    pub(super) fn choose_forward(&mut self, target: String) -> Vec<Effect> {
        let allowed = self.state.forwarding.as_ref().is_some_and(|forwarding| {
            self.state
                .rooms
                .iter()
                .any(|room| room.id == target && self.state.can_forward_to(&forwarding.room, room))
        });
        if let Some(forwarding) = &mut self.state.forwarding {
            if let Some(at) = forwarding.chosen.iter().position(|id| *id == target) {
                forwarding.chosen.remove(at);
            } else if allowed && forwarding.chosen.len() < FORWARD_LIMIT {
                forwarding.chosen.push(target);
                // Choosing one clears the search, as in Discord's dialog,
                // which then lists those chosen first.
                if !forwarding.query.is_empty() {
                    forwarding.query.clear();
                    forwarding.pinned = forwarding.chosen.clone();
                }
            }
        }
        vec![]
    }

    /// Searches the forward dialog's places for `query`. Those chosen by
    /// then list first once it's empty again.
    pub(super) fn search_forward(&mut self, query: String) -> Vec<Effect> {
        if let Some(forwarding) = &mut self.state.forwarding
            && forwarding.query != query
        {
            forwarding.query = query;
            forwarding.pinned = forwarding.chosen.clone();
        }
        vec![]
    }

    pub(super) fn confirm_delete(&mut self) -> Vec<Effect> {
        let Some(item) = self.state.confirm_delete.take() else {
            return vec![];
        };
        let Some(room) = self.state.room_of(&item).cloned() else {
            return vec![];
        };
        if self
            .state
            .compose
            .get(&room)
            .is_some_and(|compose| match compose {
                Compose::Reply { item: target, .. } | Compose::Edit { item: target, .. } => {
                    target == &item
                }
            })
        {
            self.finish_compose(&room);
        }
        if self.state.preview {
            self.update_sample(&room, &item, |message| {
                message.body = "Message deleted".into();
                message.deleted = true;
                message.reactions.clear();
                message.can_reply = false;
                message.can_edit = false;
                message.can_delete = false;
                message.can_react = false;
                message.can_forward = false;
            });
            vec![]
        } else {
            vec![Effect::Backend(Command::Delete { room, item })]
        }
    }

    pub(super) fn save_attachment(
        &mut self,
        item: String,
        index: usize,
        path: PathBuf,
    ) -> Vec<Effect> {
        if self.state.preview
            || !self
                .state
                .message(&item)
                .is_some_and(|message| index < message.attachments.len())
        {
            return vec![];
        }
        let Some(room) = self.state.room_of(&item).cloned() else {
            return vec![];
        };
        vec![Effect::Backend(Command::SaveAttachment {
            room,
            item,
            index,
            path,
        })]
    }

    pub(super) fn copy_attachment(&mut self, item: String, index: usize) -> Vec<Effect> {
        if self.state.preview
            || !self
                .state
                .message(&item)
                .and_then(|message| message.attachments.get(index))
                .is_some_and(|attachment| attachment.url.is_some())
        {
            return vec![];
        }
        let Some(room) = self.state.room_of(&item).cloned() else {
            return vec![];
        };
        vec![Effect::Backend(Command::CopyAttachment {
            room,
            item,
            index,
        })]
    }

    pub(super) fn play_video(
        &mut self,
        item: String,
        index: usize,
        (width, height): (u32, u32),
    ) -> Vec<Effect> {
        let video = self
            .state
            .message(&item)
            .and_then(|message| message.attachments.get(index))
            .is_some_and(|attachment| attachment.video);
        let Some(room) = self.state.room_of(&item).cloned().filter(|_| video) else {
            return vec![];
        };
        vec![Effect::Backend(Command::PlayVideo {
            room,
            item,
            index,
            width,
            height,
            speakers: self.state.voice.speaker.clone(),
        })]
    }

    pub(super) fn view_image(&mut self, item: String, index: usize) -> Vec<Effect> {
        if !self
            .state
            .message(&item)
            .and_then(|message| message.attachments.get(index))
            .is_some_and(|attachment| attachment.image)
        {
            return vec![];
        }
        self.state.viewer = Some(Viewer {
            item,
            index,
            pictures: HashMap::new(),
        });
        self.load_image()
    }

    pub(super) fn step_image(&mut self, step: isize) -> Vec<Effect> {
        let Some(viewer) = &self.state.viewer else {
            return vec![];
        };
        let images: Vec<usize> = self
            .state
            .message(&viewer.item)
            .into_iter()
            .flat_map(|message| message.attachments.iter().enumerate())
            .filter(|(_, attachment)| attachment.image)
            .map(|(index, _)| index)
            .collect();
        let Some(at) = images.iter().position(|index| *index == viewer.index) else {
            return vec![];
        };
        let next = images[(at as isize + step).rem_euclid(images.len() as isize) as usize];
        if let Some(viewer) = &mut self.state.viewer {
            viewer.index = next;
        }
        self.load_image()
    }

    pub(super) fn earlier(&mut self) -> Vec<Effect> {
        // A window on its way has its own start.
        if self.state.preview
            || self.state.busy
            || !self.state.room_ready
            || self.focusing
            || self.state.loading_history
            || self.state.history_start
            || !self.state.shows_text()
        {
            return vec![];
        }
        self.state.loading_history = true;
        self.state.history_failed = false;
        vec![Effect::Backend(Command::Earlier(
            self.state.selected.clone().expect("selected room exists"),
        ))]
    }

    /// Forwards the message to the conversations chosen, then `note` to
    /// each as a message of its own, as Discord's client does: the dialog
    /// closes, and with one conversation chosen, it opens. Discord's slowmode
    /// channels get the forward alone.
    pub(super) fn forward(&mut self, note: String) -> Vec<Effect> {
        let Some(mut forwarding) = self
            .state
            .forwarding
            .take_if(|forwarding| !forwarding.chosen.is_empty())
        else {
            return vec![];
        };
        // Discord's `@silent` at the start notifies nobody of either.
        let silent_note = note
            .strip_prefix("@silent")
            .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
            .filter(|_| Network::of(&forwarding.room) == Network::Discord);
        let silent = silent_note.is_some();
        let added = silent_note.unwrap_or(&note).trim();
        let mut effects = vec![];
        for target in &forwarding.chosen {
            let slowed = self
                .state
                .rooms
                .iter()
                .any(|room| room.id == *target && room.slowed);
            let note = (!added.is_empty() && !slowed).then(|| added.to_owned());
            if self.state.preview {
                self.forward_sample(&forwarding, target, note);
            } else {
                effects.push(Effect::Backend(Command::Forward {
                    room: forwarding.room.clone(),
                    item: forwarding.item.clone(),
                    target: target.clone(),
                    note,
                    silent,
                }));
            }
        }
        if let [target] = forwarding.chosen.as_slice() {
            effects.extend(self.dispatch(Action::OpenChat(target.clone())));
        }
        forwarding.note = note;
        forwarding.error = None;
        if self.state.preview {
            self.state.notice = Some("Message forwarded.".into());
        } else {
            self.forwards.push(ForwardSent {
                waiting: forwarding.chosen.clone(),
                failed: vec![],
                notes_unsent: vec![],
                forwarding,
            });
        }
        effects
    }

    /// Adds the forward to the preview's samples of `target`, with `note`.
    fn forward_sample(&mut self, forwarding: &Forwarding, target: &str, note: Option<String>) {
        let Some(message) = self
            .samples
            .get(&forwarding.room)
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.item == forwarding.item)
            })
            .cloned()
        else {
            return;
        };
        let sample = |body: String, formatted, attachments, forwarded| {
            let item = uuid::Uuid::new_v4().to_string();
            Arc::new(ChatMessage {
                id: item.clone(),
                item,
                author: "You".into(),
                sender: "You".into(),
                time: "Local preview".into(),
                own: true,
                body,
                formatted,
                attachments,
                forwarded,
                ..sample_actions()
            })
        };
        let messages = self.samples.entry(target.to_owned()).or_default();
        messages.push(sample(
            message.body.clone(),
            message.formatted,
            message.attachments.clone(),
            true,
        ));
        if let Some(note) = note {
            messages.push(sample(note, true, vec![], false));
        }
        if self.state.selected.as_deref() == Some(target) {
            self.state.messages = Arc::new(messages.clone());
        }
    }

    /// Notes a forward's result in one conversation. Once each of them has
    /// answered, the dialog opens again for those it failed in, with why.
    pub(super) fn forwarded(&mut self, room: String, result: Result<(), ForwardFailure>) {
        let Some(at) = self
            .forwards
            .iter()
            .position(|sent| sent.waiting.contains(&room))
        else {
            return;
        };
        let sent = &mut self.forwards[at];
        sent.waiting.retain(|waiting| *waiting != room);
        match result {
            Ok(()) => {}
            Err(ForwardFailure::Forward(why)) => sent.failed.push((room, why)),
            Err(ForwardFailure::Note(why)) => sent.notes_unsent.push((room, why)),
        }
        if !sent.waiting.is_empty() {
            return;
        }
        let ForwardSent {
            mut forwarding,
            failed,
            notes_unsent,
            ..
        } = self.forwards.remove(at);
        let places = |rooms: &[(String, String)]| {
            rooms
                .iter()
                .map(|(id, _)| {
                    self.state
                        .rooms
                        .iter()
                        .find(|room| room.id == *id)
                        .map_or_else(|| id.clone(), |room| self.state.destination(room))
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let (unsent, unforwarded) = (places(&notes_unsent), places(&failed));
        if let Some((_, why)) = notes_unsent.first() {
            self.state.notice = Some(format!(
                "Forwarded, but the message you added didn't go to {unsent}: {why}"
            ));
        } else if failed.is_empty() {
            self.state.notice = Some("Message forwarded.".into());
        }
        let Some((_, why)) = failed.first() else {
            return;
        };
        forwarding.error = Some(format!(
            "The message couldn't be forwarded to {unforwarded}: {why}"
        ));
        forwarding.chosen = failed.into_iter().map(|(id, _)| id).collect();
        // They list first, as Discord's dialog opens again with them chosen.
        forwarding.query.clear();
        forwarding.pinned = forwarding.chosen.clone();
        // Another forward's dialog, opened since, stays.
        if self.state.forwarding.is_none() {
            self.state.forwarding = Some(forwarding);
        }
    }

    pub(super) fn queue_action(
        &mut self,
        item: String,
        command: impl FnOnce(String, String) -> Command,
    ) -> Vec<Effect> {
        if self.state.preview
            || !self
                .state
                .message(&item)
                .is_some_and(|message| message.delivery != chat::Delivery::Sent)
        {
            return vec![];
        }
        let Some(room) = self.state.room_of(&item).cloned() else {
            return vec![];
        };
        vec![Effect::Backend(command(room, item))]
    }

    pub(super) fn update_sample(
        &mut self,
        room: &str,
        item: &str,
        change: impl FnOnce(&mut ChatMessage),
    ) {
        if let Some(messages) = self.samples.get_mut(room)
            && let Some(message) = messages.iter_mut().find(|message| message.item == item)
        {
            change(Arc::make_mut(message));
            self.state.messages = Arc::new(messages.clone());
        }
    }

    /// Downloads the viewer's image at viewing size, unless it has it or is
    /// getting it. The preview shows until it arrives.
    pub(super) fn load_image(&mut self) -> Vec<Effect> {
        let Some(room) = self
            .state
            .viewer
            .as_ref()
            .and_then(|viewer| self.state.room_of(&viewer.item))
            .cloned()
        else {
            return vec![];
        };
        let Some(viewer) = &mut self.state.viewer else {
            return vec![];
        };
        if self.state.preview || viewer.pictures.contains_key(&viewer.index) {
            return vec![];
        }
        viewer.pictures.insert(viewer.index, None);
        vec![Effect::Backend(Command::ViewImage {
            room,
            item: viewer.item.clone(),
            index: viewer.index,
        })]
    }

    /// Brings a message of the selected room into view: at once when the
    /// timeline has it, else once the messages around it, which the backend
    /// loads in place of the timeline, come.
    pub(super) fn jump(&mut self, item: String, unread: bool) -> Vec<Effect> {
        self.seeking = Some(Seeking {
            item: item.clone(),
            unread,
        });
        if self.state.preview
            || self
                .state
                .messages
                .iter()
                .any(|message| message.item == item)
        {
            return self.seek();
        }
        self.focus(item)
    }

    /// Opens a room, unless it's open, and brings its message into view.
    pub(super) fn jump_in_room(&mut self, room: String, item: String) -> Vec<Effect> {
        let mut effects = vec![];
        // A voice channel's message shows in its chat.
        if self.state.selected.as_ref() != Some(&room) || !self.state.shows_text() {
            effects = self.open(room.clone(), true);
        }
        if self.state.selected.as_ref() == Some(&room) {
            effects.extend(self.jump(item, false));
        }
        effects
    }

    /// Asks for the messages around an item of the selected room, which show
    /// in place of its timeline. Pages on their way are for what shows now,
    /// which the backend drops.
    pub(super) fn focus(&mut self, item: String) -> Vec<Effect> {
        let Some(room) = self
            .state
            .selected_room()
            .filter(|room| self.state.shows_text() && !room.locked)
            .map(|room| room.id.clone())
        else {
            self.seeking = None;
            return vec![];
        };
        self.focusing = true;
        self.state.loading_history = false;
        self.state.history_failed = false;
        self.state.loading_later = false;
        self.state.later_failed = false;
        vec![Effect::Backend(Command::Around { room, item })]
    }

    /// Loads the page after a window's newest message, as the timeline does
    /// when scrolled near its end.
    pub(super) fn later(&mut self) -> Vec<Effect> {
        if self.state.preview
            || self.state.busy
            || !self.state.room_ready
            || !self.state.detached
            || self.state.loading_later
            || self.focusing
        {
            return vec![];
        }
        let Some(room) = self.state.selected.clone() else {
            return vec![];
        };
        self.state.loading_later = true;
        self.state.later_failed = false;
        vec![Effect::Backend(Command::Later(room))]
    }

    /// Shows the newest messages again in place of a window of older ones,
    /// or of the one on its way.
    pub(super) fn jump_to_present(&mut self) -> Vec<Effect> {
        if !self.state.detached && !self.focusing {
            return vec![];
        }
        let Some(room) = self.state.selected.clone() else {
            return vec![];
        };
        self.state.detached = false;
        self.focusing = false;
        self.seeking = None;
        self.state.loading_later = false;
        self.state.later_failed = false;
        self.state.loading_history = false;
        self.state.history_failed = false;
        // Where the newest messages start is known once they show: until
        // then the window does, and its start isn't theirs to load before.
        vec![Effect::Backend(Command::Present(room))]
    }

    /// Takes in how loading history for the selected room ended.
    pub(super) fn history_loaded(
        &mut self,
        direction: chat::Direction,
        history: chat::History,
    ) -> Vec<Effect> {
        match direction {
            chat::Direction::Latest => return self.latest_loaded(history),
            chat::Direction::Earlier => self.state.loading_history = false,
            chat::Direction::Later => self.state.loading_later = false,
            // The window comes next, as a timeline.
            chat::Direction::Around if !self.focusing => return vec![],
            chat::Direction::Around => self.focusing = false,
        }
        match history {
            chat::History::Loaded { start, present } => {
                self.state.history_start = start;
                self.state.detached = !present;
            }
            chat::History::Failed => match direction {
                chat::Direction::Earlier => self.state.history_failed = true,
                chat::Direction::Later => self.state.later_failed = true,
                chat::Direction::Around => self.seeking = None,
                chat::Direction::Latest => {}
            },
        }
        // What the window shows is said when it comes.
        if direction == chat::Direction::Around {
            return vec![];
        }
        let mut effects = self.place_unread();
        effects.extend(self.seek());
        effects
    }

    /// The newest messages came. When none after the last read one shows,
    /// as when they were deleted, are of a kind Yapfast doesn't show or are
    /// hidden as blocked, there's no divider to wait for: the channel
    /// reads like any other once its newest message is in view, as in
    /// Discord's client.
    fn latest_loaded(&mut self, history: chat::History) -> Vec<Effect> {
        if history == chat::History::Failed
            || self.state.detached
            || self.focusing
            || self.read_marker.is_none()
        {
            return vec![];
        }
        self.read_marker = None;
        self.read_latest()
    }

    /// Brings the message being sought into view once the timeline has it.
    /// One that isn't there once the messages around it came, as when it
    /// was deleted, is noted.
    pub(super) fn seek(&mut self) -> Vec<Effect> {
        if self.focusing {
            return vec![];
        }
        let Some(seeking) = self.seeking.take() else {
            return vec![];
        };
        if self
            .state
            .messages
            .iter()
            .any(|message| message.item == seeking.item)
        {
            if seeking.unread {
                self.state.unread_jump += 1;
            } else {
                let count = self.state.jump.as_ref().map_or(0, |(_, count)| count + 1);
                self.state.jump = Some((seeking.item, count));
            }
        } else if !seeking.unread {
            self.state.notice =
                Some("Yapfast couldn't find that message. It may have been deleted.".into());
        }
        vec![]
    }

    /// Settles where the selected room's new messages start, once its
    /// newest messages came: after the last one read when it opened, as
    /// Discord orders them even unloaded, else after the newest it has, so
    /// that only those coming while it's open are new. A room opened with
    /// nothing unread has no divider, but its replies are new the same way.
    fn place_seen(&mut self) {
        if !self.placing_seen || !self.state.room_ready || self.focusing || self.state.detached {
            return;
        }
        let messages = &self.state.messages;
        let Some(newest) = messages.last() else {
            return;
        };
        self.placing_seen = false;
        let known = self.state.seen.as_deref().is_some_and(|seen| {
            discord::item_order(seen).is_some()
                || messages.iter().any(|message| message.item == seen)
        });
        if !known {
            self.state.seen = Some(newest.item.clone());
        }
    }

    /// The newest message came into view or left it. Once it's read past
    /// the "New" divider, the bar back to the divider goes, and an unread
    /// Discord channel is read.
    pub(super) fn read_latest(&mut self) -> Vec<Effect> {
        if self.state.reading
            && self.state.room_ready
            && !self.state.detached
            && self.read_marker.is_none()
        {
            self.state.caught_up = true;
        }
        self.mark_discord_read()
    }

    /// Puts the "New" divider above the first message after the one last
    /// read when the room was opened, once the timeline has it. Discord's
    /// messages are ordered by their snowflakes, so the divider goes in even
    /// when the last read message is gone; when the timeline has no message
    /// that old, the messages around the last read one load in its place,
    /// as a window, unless they did already. A Matrix room's divider waits
    /// for the message its read receipt is on. The timeline then shows the
    /// divider at its top, once.
    pub(super) fn place_unread(&mut self) -> Vec<Effect> {
        self.place_seen();
        let Some((marker, asked)) = self.read_marker.take() else {
            return vec![];
        };
        let messages = Arc::clone(&self.state.messages);
        if !self.state.room_ready || messages.is_empty() || self.focusing {
            self.read_marker = Some((marker, asked));
            return vec![];
        }
        let unread = |message: &Arc<ChatMessage>| !message.own;
        // A message asked for since the room opened, as a search result in
        // another channel, keeps the view and the window around it.
        let jumping = self.state.jump.is_some()
            || self.seeking.as_ref().is_some_and(|seeking| !seeking.unread);
        let first = match discord::item_order(&marker) {
            Some(read) => {
                let after = |message: &Arc<ChatMessage>| {
                    discord::item_order(&message.item).is_some_and(|order| order > read)
                };
                let reached = messages.iter().any(|message| {
                    discord::item_order(&message.item).is_some_and(|order| order <= read)
                });
                if !reached && !self.state.history_start && !asked && !jumping {
                    self.read_marker = Some((marker.clone(), true));
                    return self.focus(marker);
                }
                // Past that, the divider goes above the oldest message loaded.
                messages
                    .iter()
                    .position(|message| after(message) && unread(message))
            }
            None => messages
                .iter()
                .position(|message| message.item == marker)
                .and_then(|at| {
                    let next = messages[at + 1..].iter().position(unread)?;
                    Some(at + 1 + next)
                }),
        };
        match first {
            Some(first) => {
                self.state.first_unread = Some(messages[first].item.clone());
                if !jumping {
                    self.state.unread_jump += 1;
                }
                // The view leaves the newest message for the divider, or a
                // message asked for; it says so once it's back.
                self.state.reading = false;
            }
            // Not loaded yet, or nothing is unread after all; newer messages
            // may still come with the rest of the timeline.
            None => self.read_marker = Some((marker, asked)),
        }
        vec![]
    }
}
