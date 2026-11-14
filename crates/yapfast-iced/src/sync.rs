//! Taking in what changed in the core's state, and what follows from it:
//! the rows' heights, the member list held as the Slint client holds it,
//! the rows in view reported to the core, and the pictures the built rows
//! want, asked for in batches.
//!
//! Work per event touches only what changed: lists are compared with what
//! the window shows and replaced only when they differ, the timeline's
//! heights are kept for rows that are the same `Arc` as before, and
//! pictures the core let go of are let go of here too.

use std::{sync::Arc, time::Instant};

use iced::{
    Task,
    widget::{
        operation,
        scrollable::{AbsoluteOffset, Viewport},
    },
};
use yapfast_core::{application::Navigation, chat};

use crate::{
    after,
    app::{App, Message, Messages},
    call::Focus,
    client::REPORT_EVERY,
    looks::{self, MemberLine},
    members::{PITCH, TOP},
    rail::rail,
    sidebar::sidebar,
    theme::rgb,
    timeline::{self, Edge, MARGIN, TIMELINE, row_height},
};

/// A list's viewport, as its scrollable last reported it.
#[derive(Clone, Copy)]
pub struct Scroll {
    pub offset: f32,
    pub height: f32,
}

impl Scroll {
    pub fn new(height: f32) -> Self {
        Self {
            offset: 0.0,
            height,
        }
    }

    pub fn of(viewport: Viewport) -> Self {
        Self {
            offset: viewport.absolute_offset().y,
            height: viewport.bounds().height,
        }
    }

    /// The rows of `height` each, after `top`, to build out of `count`: those
    /// in view and [`MARGIN`] beyond.
    pub fn range(&self, count: usize, height: f32, top: f32) -> (usize, usize) {
        let first = ((self.offset - top - MARGIN) / height).floor().max(0.0) as usize;
        let last = (((self.offset - top + self.height + MARGIN) / height)
            .ceil()
            .max(0.0) as usize)
            .min(count);
        (first.min(last), last)
    }
}

/// What the rail, the sidebar, the header and the history's edge are made
/// from, as one number: the rooms' and servers' changes, where the window
/// is, the smaller lists the sidebar shows, the topic, the history's state.
pub fn lists_key(state: &yapfast_core::application::AppState, theme: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    (
        state.list_changes,
        state.category_changes,
        &state.navigation,
        &state.selected,
        state.discord_online,
        state.preview,
        &state.voice.room,
        state
            .topic
            .as_ref()
            .map(|topic| Arc::as_ptr(topic) as usize),
        (
            state.room_ready,
            state.loading_history,
            state.history_failed,
            state.history_start,
        ),
    )
        .hash(&mut hasher);
    (
        &state.friends,
        &state.friend_requests,
        &state.discord_blocked,
        &state.pinned,
        theme,
    )
        .hash(&mut hasher);
    hasher.finish()
}

impl App {
    /// Makes the rail, the sidebar, the header and the history's edge
    /// again, keeping each that comes out the same.
    fn make_lists(&mut self) {
        let state = self.client.state();
        let rail = rail(state, &self.layout.open_folders);
        if self.rail != rail {
            self.rail = rail;
        }
        let home = crate::rail::home(state);
        if self.rail_home != home {
            self.rail_home = home;
        }
        let categories = match &state.navigation {
            Navigation::Server(server) => self.client.app.categories(server),
            Navigation::DirectMessages => Vec::new(),
        };
        let state = self.client.state();
        let side = sidebar(state, &categories, &self.layout.collapsed, &self.filter);
        if self.side != side {
            self.side = side;
        }
        let head = crate::sidebar::side_header(state);
        if self.side_header != head {
            self.side_header = head;
        }
        let header = crate::header::header(state, &self.header, rgb(self.theme.mention));
        if self.header != header {
            self.header = header;
        }
        let edge = Edge::of(state);
        if self.edge != edge {
            self.edge = edge;
            self.edge_changed = true;
        }
    }

    /// Takes in what changed in the client's state; returns the work that
    /// follows (a scroll, pictures to ask for, rows to report).
    pub fn sync(&mut self) -> Task<Message> {
        let state = self.client.state();
        // The lists and the header change with the rooms and servers, not
        // with each message or presence: they're made again only then.
        let key = lists_key(state, &self.theme.id);
        if self.lists_shown != Some(key) {
            self.lists_shown = Some(key);
            self.make_lists();
        }
        let state = self.client.state();
        self.server = matches!(state.navigation, Navigation::Server(_));
        self.in_server = looks::in_server(state.selected.as_deref());
        let account = crate::account::account(state);
        if self.account != account {
            self.account = account;
        }
        // The card the core closed, or one showing someone else, goes.
        if self.card.is_some() && state.profile.as_ref().is_none_or(|view| view.large) {
            self.card = None;
        }
        let mut tasks = Vec::new();
        let switched = self.selected != state.selected;
        if switched && self.editing.is_some() {
            self.client
                .dispatch(yapfast_core::application::Action::CancelEdit);
        }
        let state = self.client.state();
        // The edit ended (saved, cancelled, or its room left): its field
        // goes.
        if self.editing.is_some() && state.edit().is_none() {
            self.editing = None;
            self.edit.reset("");
        }
        if switched {
            self.selected = state.selected.clone();
            self.from_end = 0.0;
            self.at_end = true;
            self.latest_due = false;
            // The composer shows the room's own draft, as the Slint client's.
            self.draft.reset(state.draft());
            self.draft_known = state.draft().to_owned();
            self.selection = None;
            self.selecting = false;
            self.menu = None;
            self.suggest.clear(&mut self.client);
            tasks.push(operation::scroll_to(
                TIMELINE,
                AbsoluteOffset {
                    x: None,
                    y: Some(0.0),
                },
            ));
        }
        // The core changed the draft itself, as when a message it sent went.
        let state = self.client.state();
        if !switched && state.draft() != self.draft_known {
            self.draft_known = state.draft().to_owned();
            if self.draft_known != self.draft.text() {
                self.draft.reset(&self.draft_known);
            }
        }
        if self.thumbnails.len() != self.client.state().staged().len() {
            tasks.push(self.previews());
        }
        tasks.push(self.roster(switched));
        let state = self.client.state();
        let marks = (state.first_unread.clone(), state.seen.clone());
        let edge_changed = std::mem::take(&mut self.edge_changed);
        let messages_changed = !Arc::ptr_eq(&self.messages, &self.client.state().messages);
        if messages_changed || edge_changed || self.marks_for != marks {
            self.marks_for = marks;
            // While someone reads earlier messages, what they read stays
            // put as messages come, go or change around it.
            let reading = if switched { None } else { self.reading() };
            let messages = self.client.state().messages.clone();
            let old = std::mem::replace(&mut self.messages, messages);
            self.measure(&old, edge_changed);
            tasks.push(self.keep_reading(reading));
            // Sent from a window of older messages, or Jump to present: the
            // newest show at the end once they've replaced the window.
            if messages_changed && !self.client.state().detached && self.latest_due {
                self.latest_due = false;
                tasks.push(self.go_to_end());
            }
            tasks.push(self.report());
        }
        tasks.push(self.follow_jumps());
        // Images of pictures the core let go of go too.
        self.previews.borrow_mut().prune(1);
        for converted in self.scaled.borrow_mut().values_mut() {
            converted.prune(0);
        }
        // A stream starts large, and leaves when watching it stops, as the
        // Slint client's `show_stage`.
        let watching = self
            .client
            .state()
            .voice
            .watching
            .as_ref()
            .map(|watching| watching.user.clone());
        if watching != self.watched {
            if let Some(user) = &watching {
                self.focus = Some(Focus::Stream(user.clone()));
            } else if matches!(&self.focus, Some(Focus::Stream(user)) if self.watched.as_ref() == Some(user))
            {
                self.focus = None;
            }
            self.watched = watching;
        }
        self.place_videos();
        self.picker_sync();
        tasks.push(self.ask());
        self.sync_sign_in();
        tasks.push(self.sync_mfa());
        tasks.push(self.follow_backend());
        Task::batch(tasks)
    }

    /// A backend started (signing in, linking Discord, a saved session) or
    /// linking began: their events come as messages, and whatever came from
    /// the one before stops, as the Slint client aborts its old listener.
    fn follow_backend(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        if let Some(events) = self.client.take_events() {
            let (task, handle) = Task::run(crate::app::batches(events), Message::Events)
                .chain(Task::done(Message::Stopped))
                .abortable();
            self.backend_events = Some(handle.abort_on_drop());
            tasks.push(task);
        }
        if let Some(progress) = self.client.take_linking() {
            let (task, handle) = Task::run(
                iced::futures::stream::unfold(progress, |mut progress| async move {
                    let step = progress.recv().await?;
                    Some((step, progress))
                }),
                Message::Linking,
            )
            .abortable();
            self.linking = Some(handle.abort_on_drop());
            tasks.push(task);
        }
        if self.client.link_stopped() {
            self.linking = None;
        }
        Task::batch(tasks)
    }

    /// The member list, held as the Slint client holds it while people
    /// change (`ROSTER_PAUSE`): the first change shows at once, those in the
    /// pause after it together at its end. A new room shows at once.
    fn roster(&mut self, switched: bool) -> Task<Message> {
        let state = self.client.state();
        let members = match &state.members {
            Some(Ok(members)) if self.server => Some(members),
            _ => None,
        };
        let same = match (members, &self.members) {
            (Some(new), Some(old)) => Arc::ptr_eq(new, old),
            (None, None) => true,
            _ => false,
        };
        if same || self.roster_waiting {
            return Task::none();
        }
        let since = self.roster_shown.map(|shown| shown.elapsed());
        if switched
            || self.members.is_none()
            || since.is_none_or(|since| since >= looks::ROSTER_PAUSE)
        {
            self.show_roster(false);
            return Task::none();
        }
        self.roster_waiting = true;
        after(
            looks::ROSTER_PAUSE - since.unwrap_or_default(),
            Message::Roster,
        )
    }

    /// Shows the member list as the core has it; `held`, at the end of a
    /// pause, only if it changed meanwhile.
    pub fn show_roster(&mut self, held: bool) {
        let state = self.client.state();
        let members = match &state.members {
            Some(Ok(members)) if self.server => Some(members.clone()),
            _ => None,
        };
        if held && members.as_ref().map(Arc::as_ptr) == self.members.as_ref().map(Arc::as_ptr) {
            return;
        }
        let lines = match &members {
            Some(members) => looks::member_lines(
                state,
                members,
                self.client.server_owner().as_deref(),
                self.theme.dark,
            ),
            None => Vec::new(),
        };
        if lines != self.lines {
            self.lines = lines;
        }
        self.members = members;
        self.roster_shown = Some(Instant::now());
    }

    /// The timeline's whole height.
    pub fn total_height(&self) -> f32 {
        self.tops.last().copied().unwrap_or(0.0)
    }

    /// Works out the rows' looks and heights for the current messages,
    /// keeping the text laid out for messages that are the same `Arc` as in
    /// `old`, and the heights of those laid out the same way; row 0's only
    /// while the history's edge is the same.
    pub fn measure(&mut self, old: &Messages, edge_changed: bool) {
        let messages = self.messages.clone();
        // Where the old rows are among the new: earlier history goes in
        // above them, and the oldest go as the newest come. The core makes
        // every message anew when names or roles change, so rows are found
        // by their item.
        let shift: Option<isize> = old
            .first()
            .and_then(|first| {
                messages
                    .iter()
                    .position(|message| message.item == first.item)
                    .map(|at| at as isize)
            })
            .or_else(|| {
                messages.first().and_then(|first| {
                    old.iter()
                        .position(|message| message.item == first.item)
                        .map(|at| -(at as isize))
                })
            });
        let old_looks = std::mem::take(&mut self.looks);
        let before = |index: usize| {
            shift
                .and_then(|shift| usize::try_from(index as isize - shift).ok())
                .filter(|&before| {
                    before < old.len()
                        && before < old_looks.len()
                        && old[before].item == messages[index].item
                })
        };
        // The same message: its look and height stay.
        let old_at = |index: usize| {
            before(index).filter(|&before| {
                Arc::ptr_eq(&old[before], &messages[index]) || old[before] == messages[index]
            })
        };
        // Its text the same: its text's layout stays.
        let same_text = |index: usize| {
            before(index).filter(|&before| timeline::same_text(&old[before], &messages[index]))
        };
        let (mention, ink, muted) = (
            rgb(self.theme.mention),
            rgb(self.theme.text),
            rgb(self.theme.muted),
        );
        let revealed = &self.revealed;
        let looks = timeline::row_looks(
            &messages,
            self.client.state(),
            |index| old_at(index).map(|before| &old_looks[before]),
            |index| {
                if let Some(before) = same_text(index) {
                    return old_looks[before].body.clone();
                }
                let message = &messages[index];
                Arc::new(timeline::body_of(message, mention, ink, muted, |spoiler| {
                    revealed.contains(&format!("{}/{spoiler}", message.item))
                }))
            },
        );
        let edge = self.edge.height();
        let heights: Vec<f32> = looks
            .iter()
            .enumerate()
            .map(|(index, look)| {
                let same = old_at(index).filter(|&before| {
                    before < self.heights.len()
                        && old_looks[before].same_place(look)
                        && (before == 0) == (index == 0)
                        && !(index == 0 && edge_changed)
                });
                match same {
                    Some(before) => self.heights[before],
                    None => row_height(
                        &messages[index],
                        look,
                        self.width,
                        if index == 0 { edge } else { 0.0 },
                    ),
                }
            })
            .collect();
        self.looks = looks;
        self.heights = heights;
        self.place_rows();
    }

    /// Where each row starts, from the rows' heights.
    pub fn place_rows(&mut self) {
        self.tops.clear();
        self.tops.reserve(self.heights.len() + 1);
        let mut top = 0.0;
        self.tops.push(top);
        for height in &self.heights {
            top += height;
            self.tops.push(top);
        }
    }

    /// The timeline's viewport moved or changed size.
    pub fn timeline_scrolled(&mut self, viewport: Viewport) -> Task<Message> {
        self.from_end = viewport.absolute_offset().y;
        self.timeline_height = viewport.bounds().height;
        self.at_end = self.from_end <= 2.0;
        let width = viewport.bounds().width;
        if (width - self.width).abs() > 1.0 {
            self.width = width;
            self.heights.clear();
            let same = self.messages.clone();
            self.measure(&same, true);
        }
        Task::batch([self.report(), self.ask()])
    }

    /// The timeline's rows in view, and `margin` beyond.
    pub fn in_view(&self, margin: f32) -> (usize, usize) {
        let top = (self.total_height() - self.timeline_height - self.from_end).max(0.0);
        let bottom = top + self.timeline_height;
        let count = self.heights.len();
        // The first row ending below the top, the last starting above the bottom.
        let first = self.tops[1..].partition_point(|end| *end <= top - margin);
        let last = self.tops[..count].partition_point(|start| *start < bottom + margin);
        (first.min(count), last.max(first).min(count))
    }

    /// Tells the core which rows are in view, at most every 50 ms.
    pub fn report(&mut self) -> Task<Message> {
        if self.reporting || self.messages.is_empty() {
            return Task::none();
        }
        let (first, last) = self.in_view(0.0);
        if !self
            .client
            .visible(first, last.saturating_sub(1).max(first), self.at_end)
        {
            self.reporting = true;
            return after(REPORT_EVERY, Message::Report);
        }
        // What the report changed (earlier history asked for) shows later,
        // through events; the state it set is taken in here.
        if Arc::ptr_eq(&self.messages, &self.client.state().messages) {
            Task::none()
        } else {
            self.sync()
        }
    }

    /// Pictures the rows built show but the core doesn't have yet: avatars
    /// and icons, decorations' and nameplates' stills, tag badges, and the
    /// moving versions of what's under the pointer or on the open card.
    fn wanted(&self) -> Vec<String> {
        let state = self.client.state();
        let cosmetics = state.cosmetics;
        let mut urls: Vec<String> = Vec::new();
        let mut want = |url: Option<&str>| {
            if let Some(url) = url
                && !url.is_empty()
                && !state.pictures.contains_key(url)
                && !self.asked.contains(url)
                && !urls.iter().any(|wanted| wanted == url)
            {
                urls.push(url.to_owned());
            }
        };
        for item in &self.rail {
            want(item.icon.as_deref());
            if let crate::rail::Kind::Folder {
                open: false,
                preview,
                ..
            } = &item.kind
            {
                for (_, icon) in preview {
                    want(icon.as_deref());
                }
            }
        }
        let mut top = 0.0;
        let (from, to) = (
            self.side_scroll.offset - MARGIN,
            self.side_scroll.offset + self.side_scroll.height + MARGIN,
        );
        for row in &self.side {
            let height = row.height();
            if let crate::sidebar::SideRow::Room(room) = row
                && top + height >= from
                && top <= to
            {
                want(room.avatar.as_deref());
                want(room.status.emoji.as_deref());
                for person in &room.people {
                    want(person.avatar.as_deref());
                }
            }
            top += height;
        }
        for url in self.suggestion_pictures() {
            want(Some(&url));
        }
        for url in self.picker_pictures() {
            want(Some(&url));
        }
        // A custom emoji among the reactions the hover bar and menus offer.
        if self.pointed_message.is_some() || self.menu.is_some() {
            for key in self.client.app.frequent_reactions(crate::menus::QUICK) {
                if let Some((_, id)) = key.split_once(':') {
                    want(self.custom_emoji(id).as_deref());
                }
            }
        }
        want(self.account.avatar.as_deref());
        want(self.header.avatar.as_deref());
        want(self.account.status.emoji.as_deref());
        let (first, last) = self.in_view(MARGIN);
        for index in first..last {
            let message = &self.messages[index];
            if let Some(reply) = &message.reply {
                want(reply.avatar.as_deref());
            }
            if let Some(used) = &message.used {
                want(used.avatar.as_deref());
            }
            for emoji in &message.emoji {
                want(Some(&emoji.picture));
            }
            if !self.looks.get(index).is_some_and(|look| look.first) {
                continue;
            }
            want(message.avatar.as_deref());
            let decoration = cosmetics.decoration(message.decoration.as_deref());
            want(decoration);
            want(
                cosmetics
                    .tag(message.tag.as_ref())
                    .and_then(|tag| tag.badge.as_deref()),
            );
            if self.pointed_message.as_ref() == Some(&message.item) {
                want(decoration.map(looks::animated_url).as_deref());
            }
        }
        let (first, last) = self.member_scroll.range(self.lines.len(), PITCH, TOP);
        for line in &self.lines[first..last] {
            match line {
                MemberLine::Heading { icon, .. } => want(icon.as_deref()),
                MemberLine::Member(look) => {
                    want(look.avatar.as_deref());
                    want(look.decoration.as_deref());
                    want(look.nameplate.as_ref().map(|(picture, _)| &**picture));
                    want(look.role_icon.as_deref());
                    want(look.tag.as_ref().and_then(|(_, badge)| badge.as_deref()));
                    want(look.status.emoji.as_deref());
                    if self.pointed_member.as_ref() == Some(&look.id) {
                        want(
                            look.decoration
                                .as_deref()
                                .map(looks::animated_url)
                                .as_deref(),
                        );
                        want(
                            look.nameplate
                                .as_ref()
                                .map(|(picture, _)| looks::animated_url(picture))
                                .as_deref(),
                        );
                    }
                }
            }
        }
        if self.stage_shown() {
            for person in &state.voice.participants {
                want(person.avatar.as_deref());
            }
        }
        if let (Some(card), Some(view)) = (&self.card, &state.profile) {
            let profile = &view.profile;
            want(profile.avatar.as_deref());
            let decoration = cosmetics.decoration(profile.decoration.as_deref());
            want(decoration);
            if card.pointed {
                want(decoration.map(looks::animated_url).as_deref());
            }
            want(profile.banner.as_deref());
            want(
                profile
                    .banner
                    .as_deref()
                    .and_then(chat::moving_banner)
                    .as_deref(),
            );
            want(
                cosmetics
                    .tag(profile.tag.as_ref())
                    .and_then(|tag| tag.badge.as_deref()),
            );
        }
        urls
    }

    /// Asks for missing pictures in one batch 50 ms from now, as the Slint
    /// client's `PictureStore` does.
    pub fn ask(&mut self) -> Task<Message> {
        if self.asking || self.wanted().is_empty() {
            return Task::none();
        }
        self.asking = true;
        after(timeline::ASK_EVERY, Message::AskPictures)
    }

    /// The batch fell due: asks for what the rows want now.
    pub fn ask_now(&mut self) -> Task<Message> {
        self.asking = false;
        let pictures = &self.client.state().pictures;
        self.asked.retain(|url| !pictures.contains_key(url));
        let urls = self.wanted();
        self.asked.extend(urls.iter().cloned());
        self.client.want_pictures(urls);
        self.sync()
    }
}
