//! A Discord thread shown beside the open channel, in place of its member
//! list, as Discord opens a thread from a message: its timeline, history and
//! composer, beside the selected room's. Expanding it shows it in place of
//! the channel.
//!
//! Split view, a Yapfast addition behind its switch, shows any Discord
//! channel or DM there the same way, and keeps it while the main view goes
//! to other rooms.

use super::*;

/// The thread beside the open channel.
#[derive(Clone, Debug, Default)]
pub struct Aside {
    /// Its room id.
    pub room: String,
    /// Its timeline came.
    pub ready: bool,
    pub messages: Arc<Vec<Arc<ChatMessage>>>,
    pub loading_history: bool,
    /// Its first message is loaded.
    pub history_start: bool,
    pub history_failed: bool,
    /// The message its new ones follow, as [`AppState::seen`] is the
    /// selected room's: the last read when it showed, or the newest its
    /// first messages had.
    pub seen: Option<String>,
    /// It's a channel or DM in split view, which stays while other rooms
    /// show, rather than a thread of the open channel.
    pub split: bool,
}

impl AppState {
    /// The thread beside the open channel, as it lists.
    pub fn aside_room(&self) -> Option<&ChatRoom> {
        let aside = self.aside.as_ref()?;
        self.rooms
            .iter()
            .map(Arc::as_ref)
            .find(|room| room.id == aside.room)
    }

    /// What the composer of the thread beside holds.
    pub fn aside_draft(&self) -> &str {
        self.aside
            .as_ref()
            .and_then(|aside| self.drafts.get(&aside.room))
            .map_or("", String::as_str)
    }

    /// The message the thread beside's composer replies to or edits.
    pub fn aside_compose(&self) -> Option<&Compose> {
        self.compose.get(&self.aside.as_ref()?.room)
    }

    /// The files waiting in the thread beside's composer.
    pub fn aside_staged(&self) -> &[chat::Staged] {
        self.aside
            .as_ref()
            .and_then(|aside| self.staged.get(&aside.room))
            .map_or(&[], Vec::as_slice)
    }

    /// Whether files can be added to the thread beside's next message.
    pub fn aside_can_attach(&self) -> bool {
        self.aside.as_ref().is_some_and(|aside| {
            self.can_write_in(&aside.room)
                && !self.preview
                && !matches!(self.aside_compose(), Some(Compose::Edit { .. }))
                && self.aside_staged().len() < chat::MAX_FILES
        })
    }

    /// Why files dropped on the thread beside can't go with its next
    /// message, if they can't.
    pub fn aside_drop_refusal(&self, paths: &[PathBuf]) -> Option<chat::Refusal> {
        let limit = self.aside_room().map_or(0, |room| room.upload_limit);
        chat::Refusal::of(paths, self.aside_staged().len(), limit)
    }

    /// Whether the thread beside's next files can be marked as spoilers.
    pub fn aside_can_mark_spoilers(&self) -> bool {
        self.aside_room()
            .is_some_and(|room| room.capabilities.spoilers)
    }

    /// Whether split view can show a listed room beside the open one: a
    /// Discord channel, thread or DM with messages to show, other than the
    /// open one or the one split view shows already.
    pub fn can_split(&self, id: &str) -> bool {
        self.selected.as_deref() != Some(id)
            && self.aside.as_ref().is_none_or(|aside| aside.room != id)
            && self.splittable(id)
    }

    /// Whether the room in split view and the open one can change places:
    /// the open one is one split view can show.
    pub fn can_swap_split(&self) -> bool {
        self.aside.as_ref().is_some_and(|aside| aside.split)
            && self
                .selected
                .as_deref()
                .is_some_and(|selected| self.splittable(selected))
    }

    /// A listed Discord channel, thread or DM with messages to show, with
    /// split view on.
    pub(super) fn splittable(&self, id: &str) -> bool {
        self.split_view
            && !self.preview
            && self.rooms.iter().any(|room| {
                room.id == id
                    && room.network == Network::Discord
                    && !room.voice
                    && !room.forum
                    && !room.locked
                    && !room.request
            })
    }

    /// The room whose timeline shows an item: the thread beside the open
    /// channel, or the selected room.
    pub fn room_of(&self, item: &str) -> Option<&String> {
        match &self.aside {
            Some(aside) if aside.messages.iter().any(|message| message.item == item) => {
                Some(&aside.room)
            }
            _ => self.selected.as_ref(),
        }
    }

    /// Whether this account can write in a room on screen: the selected one
    /// or the thread beside it, once its timeline came.
    pub fn can_write_in(&self, room: &str) -> bool {
        let ready = match &self.aside {
            Some(aside) if aside.room == room => aside.ready,
            _ => self.selected.as_deref() == Some(room) && self.room_ready,
        };
        !self.busy
            && ready
            && self
                .rooms
                .iter()
                .find(|listed| listed.id == room)
                .is_some_and(|listed| self.writable(listed))
    }

    /// What reacting this account may do in a room on screen, once its
    /// timeline came, as [`Self::can_write_in`] asks of writing.
    pub fn reacts_in(&self, room: &str) -> Reacts {
        let ready = match &self.aside {
            Some(aside) if aside.room == room => aside.ready,
            _ => self.selected.as_deref() == Some(room) && self.room_ready,
        };
        match self.rooms.iter().find(|listed| listed.id == room) {
            Some(listed) if ready && !self.busy => self.reactable(listed),
            _ => Reacts::None,
        }
    }

    /// Whether the composer of a room on screen can send what it holds.
    pub fn can_send_in(&self, room: &str) -> bool {
        let draft = self.drafts.get(room).map_or("", String::as_str);
        let editing = matches!(self.compose.get(room), Some(Compose::Edit { .. }));
        let staged = self
            .staged
            .get(room)
            .is_some_and(|staged| !staged.is_empty());
        self.can_write_in(room)
            && self.sending.is_none()
            && (!draft.trim().is_empty() || (!editing && staged))
    }
}

impl Application {
    /// Shows a Discord thread beside the open channel, as clicking its link
    /// in a message does: at once when it's listed, else once Discord gives
    /// it. Anything but a thread opens as itself.
    pub(super) fn open_aside(&mut self, room: String) -> Vec<Effect> {
        if self.state.selected.as_ref() == Some(&room)
            || self
                .state
                .aside
                .as_ref()
                .is_some_and(|aside| aside.room == room)
        {
            return vec![];
        }
        match self.state.rooms.iter().find(|listed| listed.id == room) {
            Some(listed) if listed.parent.is_some() => self.show_aside(room),
            Some(_) => self.open(room, false),
            None => self.open_thread(room, true),
        }
    }

    pub(super) fn show_aside(&mut self, room: String) -> Vec<Effect> {
        self.state.new_thread = None;
        let seen = self
            .state
            .rooms
            .iter()
            .find(|listed| listed.id == room)
            .and_then(|listed| listed.last_read.clone());
        self.state.aside = Some(Aside {
            room: room.clone(),
            seen,
            ..Default::default()
        });
        self.aside_visible.clear();
        vec![Effect::Backend(Command::Aside(Some(room)))]
    }

    /// Shows a Discord channel, thread or DM beside the open room, in split
    /// view, in place of a thread there. One split view can't show opens as
    /// [`Action::SelectRoom`] does, so a Ctrl-click still goes somewhere.
    pub(super) fn open_split(&mut self, room: String) -> Vec<Effect> {
        if self.state.selected.as_deref() == Some(&room) && self.state.splittable(&room) {
            return self.split_open_room(room);
        }
        if !self.state.can_split(&room) {
            if self
                .state
                .aside
                .as_ref()
                .is_some_and(|aside| aside.split && aside.room == room)
            {
                return vec![];
            }
            return self.select_room(room);
        }
        let effects = self.show_aside(room);
        if let Some(aside) = &mut self.state.aside {
            aside.split = true;
        }
        effects
    }

    /// Shows the room in split view in the main view and the main view's in
    /// split view. The Discord channels stay open as they change places, so
    /// neither loads again.
    pub(super) fn swap_split(&mut self) -> Vec<Effect> {
        if !self.state.can_swap_split() {
            return vec![];
        }
        let (Some(side), Some(main)) = (
            self.state.aside.as_ref().map(|aside| aside.room.clone()),
            self.state.selected.clone(),
        ) else {
            return vec![];
        };
        let mut effects = self.close_aside();
        effects.extend(self.open(side, false));
        effects.extend(self.open_split(main));
        effects
    }

    /// Shows the room split view showed when Yapfast last closed, once it's
    /// listed and a room is open; one split view can't show then, such as
    /// the open one, is forgotten.
    pub(super) fn restore_split(&mut self) -> Vec<Effect> {
        let Some(room) = self.pending_split.clone() else {
            return vec![];
        };
        let listed = self.state.rooms.iter().any(|listed| listed.id == room);
        if self.state.selected.is_none() || !listed {
            return vec![];
        }
        self.pending_split = None;
        if !self.state.can_split(&room) {
            return vec![];
        }
        self.open_split(room)
    }

    /// Moves the room on screen to split view, and the conversation to the
    /// room shown before it; without one, to the first text channel of its
    /// server, or for a DM, to the friends list. So the same room never
    /// shows on both sides.
    fn split_open_room(&mut self, room: String) -> Vec<Effect> {
        let listed = |id: &str| self.state.rooms.iter().any(|listed| listed.id == id);
        let previous = self
            .previous_room
            .clone()
            .filter(|previous| *previous != room && listed(previous));
        let server = self
            .state
            .servers
            .iter()
            .find(|server| server.channels.contains(&room));
        let channel = server.and_then(|server| {
            server.channels.iter().find(|id| {
                **id != room
                    && self.state.rooms.iter().any(|listed| {
                        listed.id == **id
                            && !listed.voice
                            && !listed.forum
                            && !listed.locked
                            && listed.parent.is_none()
                    })
            })
        });
        let channel = channel.cloned();
        let direct = self
            .state
            .rooms
            .iter()
            .any(|listed| listed.id == room && listed.direct);
        let mut effects = self.close_aside();
        match (previous, channel) {
            (Some(other), _) | (None, Some(other)) => effects.extend(self.open(other, false)),
            (None, None) if direct => {
                effects.extend(self.navigate(Navigation::DirectMessages));
                effects.extend(self.open_dm_screen(DmScreen::Friends));
            }
            (None, None) => return effects,
        }
        if self.state.selected.as_deref() == Some(&room) {
            return effects;
        }
        effects.extend(self.open_split(room));
        effects
    }

    /// Turns split view on or off; off closes a room it shows.
    pub(super) fn set_split_view(&mut self, on: bool) -> Vec<Effect> {
        self.state.split_view = on;
        if !on && self.state.aside.as_ref().is_some_and(|aside| aside.split) {
            return self.close_aside();
        }
        vec![]
    }

    pub(super) fn close_aside(&mut self) -> Vec<Effect> {
        self.pending_aside = None;
        // A new thread's first message stays in its composer.
        self.first_message = None;
        if self.state.aside.take().is_none() {
            return vec![];
        }
        self.aside_visible.clear();
        vec![Effect::Backend(Command::Aside(None))]
    }

    /// Shows the thread beside the open channel in place of the channel.
    pub(super) fn expand_aside(&mut self) -> Vec<Effect> {
        let Some(room) = self.state.aside.as_ref().map(|aside| aside.room.clone()) else {
            return vec![];
        };
        let mut effects = self.close_aside();
        effects.extend(self.open(room, false));
        effects
    }

    /// Loads the page before the oldest message the thread beside shows.
    pub(super) fn aside_earlier(&mut self) -> Vec<Effect> {
        let Some(aside) = self.state.aside.as_mut() else {
            return vec![];
        };
        if !aside.ready || aside.loading_history || aside.history_start || self.state.busy {
            return vec![];
        }
        aside.loading_history = true;
        aside.history_failed = false;
        vec![Effect::Backend(Command::Earlier(aside.room.clone()))]
    }

    /// The thread beside shows its newest messages, and reads them.
    pub(super) fn aside_timeline(&mut self, mut messages: Vec<Arc<ChatMessage>>) -> Vec<Effect> {
        if self.state.hide_blocked {
            messages.retain(|message| !message.blocked);
        }
        let Some(aside) = self.state.aside.as_mut() else {
            return vec![];
        };
        if let Some(first) = aside.messages.first()
            && !messages.iter().any(|message| message.item == first.item)
        {
            aside.history_start = false;
        }
        aside.ready = true;
        // A thread is Discord's, ordered by snowflakes, so only one never
        // read needs its newest.
        if aside.seen.is_none() {
            aside.seen = messages.last().map(|newest| newest.item.clone());
        }
        aside.messages = Arc::new(messages);
        let room = aside.room.clone();
        let mut effects = self.mark_aside_read();
        // A thread just started gets the first message its form held.
        if self.first_message.take_if(|first| *first == room).is_some() {
            effects.extend(self.send_in(room));
        }
        effects
    }

    pub(super) fn aside_history(&mut self, direction: chat::Direction, history: chat::History) {
        let Some(aside) = self.state.aside.as_mut() else {
            return;
        };
        if direction != chat::Direction::Earlier {
            return;
        }
        aside.loading_history = false;
        match history {
            chat::History::Loaded { start, .. } => aside.history_start = start,
            chat::History::Failed => aside.history_failed = true,
        }
    }

    /// Reads the thread beside, as the open channel is read while it shows.
    pub(super) fn mark_aside_read(&self) -> Vec<Effect> {
        match (self.state.aside.as_ref(), self.state.aside_room()) {
            (Some(aside), Some(room)) if aside.ready && (room.unread > 0 || room.mentions > 0) => {
                vec![Effect::Backend(Command::MarkRead {
                    room: room.id.clone(),
                    sync: self.state.sync_discord_reads,
                })]
            }
            _ => vec![],
        }
    }

    /// Sends what the thread beside's composer holds.
    pub(super) fn send_aside(&mut self) -> Vec<Effect> {
        match self.state.aside.as_ref().map(|aside| aside.room.clone()) {
            Some(room) => self.send_in(room),
            None => vec![],
        }
    }
}
