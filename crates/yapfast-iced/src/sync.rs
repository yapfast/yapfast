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
use yapfast_core::application::Navigation;

use crate::{
    after,
    app::{App, Message, Messages},
    client::REPORT_EVERY,
    looks::{self, MemberLine},
    members::{PITCH, TOP},
    rail::rail,
    sidebar::sidebar,
    theme::rgb,
    timeline::{self, Edge, MARGIN, RowLook, TIMELINE, row_height},
};

/// A list's viewport, as its scrollable last reported it.
#[derive(Clone, Copy, Debug)]
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

/// A timeline's rows before and now, for [`App::measured_rows`].
pub struct Measuring<'a> {
    pub old: &'a Messages,
    pub old_looks: &'a [RowLook],
    pub old_heights: &'a [f32],
    pub messages: &'a Messages,
    pub marks: timeline::Marks<'a>,
    pub width: f32,
    /// The history's edge's height over the first row, and whether it
    /// changed since; and the newer messages' under the last.
    pub edge: f32,
    pub edge_changed: bool,
    pub foot: f32,
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
        state
            .dm_screen
            .map(|screen| screen == yapfast_core::application::DmScreen::Friends),
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
            state.detached,
            state.loading_later,
            state.later_failed,
            state.voice_chat,
        ),
    )
        .hash(&mut hasher);
    (
        &state.friends,
        &state.discord_friends,
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
        let side = sidebar(
            state,
            &categories,
            &self.layout.collapsed,
            &self.filter,
            self.theme.dark,
        );
        if self.side != side {
            self.side = side;
        }
        let discord = match &state.navigation {
            Navigation::Server(server) => state
                .servers
                .iter()
                .any(|known| known.discord && &known.id == server),
            Navigation::DirectMessages => false,
        };
        let places = crate::arrange::places(&self.side, discord, !self.filter.is_empty());
        if self.side_places != places {
            self.side_places = places;
            // What's dragged may have gone, or moved to another place.
            self.arranging = None;
        }
        let head = crate::sidebar::side_header(&self.client.app);
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
        // Someone dragged may drop where their menu's Move to offers, of
        // the channels listed now.
        self.moving_targets();
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
        let mut tasks = vec![self.profile_sync(), self.status_sync()];
        self.friends_sync();
        self.active_now_sync();
        self.reactions_page();
        let state = self.client.state();
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
        // The feed came or changed whom it may show.
        let state = self.client.state();
        let feed = (
            Arc::as_ptr(&state.activity_feed) as *const () as usize,
            state
                .activity_people
                .as_ref()
                .map_or(0, |(_, people)| Arc::as_ptr(people) as *const () as usize),
        );
        if feed != self.activity_for {
            self.activity_for = feed;
            self.make_activity_cards();
        }
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
        self.pictures.prune();
        self.call_sync();
        self.viewer_sync();
        tasks.push(self.thread_sync());
        self.popovers_sync();
        self.search_sync();
        self.forum_sync();
        self.modal_sync();
        tasks.push(self.menu_apps_sync());
        if self.client.state().browse.is_none() && !self.browse_filter.is_empty() {
            self.browse_filter.clear();
        }
        self.picker_sync();
        self.person_sync();
        tasks.push(self.server_settings_sync());
        tasks.push(self.server_dialogs_sync());
        tasks.push(self.channel_settings_sync());
        tasks.push(self.ask());
        self.sync_sign_in();
        tasks.push(self.shell_sync());
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
        self.make_activity_cards();
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
        let state = self.client.state();
        let marks = timeline::Marks {
            seen: state.seen.as_deref(),
            first_unread: state.first_unread.as_deref(),
        };
        let (looks, heights) = self.measured_rows(Measuring {
            old,
            old_looks: &self.looks,
            old_heights: &self.heights,
            messages: &self.messages,
            marks,
            width: self.width,
            edge: self.edge.height(),
            edge_changed,
            foot: self.edge.foot(),
        });
        self.looks = looks;
        self.heights = heights;
        self.place_rows();
    }

    /// A timeline's rows' looks and heights for its messages now, keeping
    /// what's the same from its rows before (see [`App::measure`]).
    pub fn measured_rows(&self, rows: Measuring<'_>) -> (Vec<RowLook>, Vec<f32>) {
        let Measuring {
            old,
            old_looks,
            old_heights,
            messages,
            marks,
            width,
            edge,
            edge_changed,
            foot,
        } = rows;
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
        let prefs = &self.layout.prefs;
        let clock = timeline::Clock {
            twelve_hour: prefs.twelve_hour_clock,
            seconds: prefs.clock_seconds,
        };
        let looks = timeline::row_looks(
            messages,
            marks,
            clock,
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
        let heights: Vec<f32> = looks
            .iter()
            .enumerate()
            .map(|(index, look)| {
                // The last row is measured each time, as it may carry the
                // newer messages' edge, or have carried it.
                let last = index + 1 == looks.len();
                let same = old_at(index).filter(|&before| {
                    before < old_heights.len()
                        && old_looks[before].same_place(look)
                        && (before == 0) == (index == 0)
                        && !(index == 0 && edge_changed)
                        && !last
                        && before + 1 != old_heights.len()
                });
                match same {
                    Some(before) => old_heights[before],
                    None => {
                        row_height(
                            &messages[index],
                            look,
                            width,
                            if index == 0 { edge } else { 0.0 },
                        ) + if last { foot } else { 0.0 }
                    }
                }
            })
            .collect();
        (looks, heights)
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
                && !self.client.app.picture_requested(url)
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
                want(room.decoration.as_deref());
                want(room.tag.as_ref().and_then(|(_, badge)| badge.as_deref()));
                if room.selected || self.pointed_side.as_deref() == Some(room.id.as_str()) {
                    if let Some((plate, _)) = &room.nameplate {
                        want(Some(plate));
                        want(Some(&looks::animated_url(plate)));
                    }
                    want(
                        room.decoration
                            .as_deref()
                            .map(looks::animated_url)
                            .as_deref(),
                    );
                }
                for person in &room.people {
                    want(person.avatar.as_deref());
                }
            }
            top += height;
        }
        for url in self.info_card_pictures() {
            want(Some(&url));
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
        // The account card's decoration and nameplate, moving under the
        // pointer.
        let decoration = cosmetics.decoration(state.account_decoration.as_deref());
        let plate = state
            .account_nameplate
            .as_ref()
            .filter(|_| !state.hide_nameplates)
            .map(|plate| &*plate.picture);
        want(decoration);
        want(plate);
        if self.account_pointed {
            want(decoration.map(looks::animated_url).as_deref());
            want(plate.map(looks::animated_url).as_deref());
        }
        // The open server's banner, over its channels.
        if self.side_header.server.is_some() {
            want(self.side_header.banner.as_deref());
        }
        want(self.header.avatar.as_deref());
        want(self.account.status.emoji.as_deref());
        let (first, last) = self.in_view(MARGIN);
        let main = (first..last).map(|index| (&self.messages[index], self.looks.get(index)));
        let aside = self.aside.iter().flat_map(|aside| {
            self.aside_rows_in_view()
                .map(move |index| (&aside.messages[index], aside.looks.get(index)))
        });
        for (message, look) in main.chain(aside) {
            if let Some(reply) = &message.reply {
                want(reply.avatar.as_deref());
            }
            if let Some(used) = &message.used {
                want(used.avatar.as_deref());
            }
            for emoji in &message.emoji {
                want(Some(&emoji.picture));
            }
            if !look.is_some_and(|look| look.first) {
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
        for url in self.activity_pictures() {
            want(Some(url));
        }
        for url in self
            .popover_pictures()
            .into_iter()
            .chain(self.forum_pictures())
        {
            want(Some(url));
        }
        if self.stage_shown() {
            for person in &state.voice.participants {
                want(person.avatar.as_deref());
            }
        }
        for url in self.profile_pictures() {
            want(Some(&url));
        }
        for url in self.friends_pictures() {
            want(Some(&url));
        }
        for url in self.reactions_pictures() {
            want(Some(&url));
        }
        for url in self.server_settings_pictures() {
            want(Some(&url));
        }
        for url in self
            .server_dialog_pictures()
            .into_iter()
            .chain(self.link_dialog_pictures())
        {
            want(Some(&url));
        }
        for url in self
            .channel_settings_pictures()
            .into_iter()
            .chain(self.entrance_pictures())
        {
            want(Some(&url));
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
        let urls = self.wanted();
        self.client.want_pictures(urls);
        self.sync()
    }
}
