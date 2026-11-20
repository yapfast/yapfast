//! Taking in what changed in the core's state, and what follows from it:
//! the rows' heights, the member list held while people change, the rows
//! in view reported to the core, and the pictures the built rows want,
//! asked for in batches.
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
    timeline::{self, Edge, RowLook, TIMELINE, row_height},
};

/// Rows built beyond each edge of a list's viewport.
pub const MARGIN: f32 = 300.0;

/// A list's viewport, as its scrollable last reported it.
#[derive(Clone, Copy, Debug, Default)]
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
/// is, the people lists' changes, the topic, the history's state.
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
    // What a call can be joined by, for the header's call buttons.
    let moving = matches!(
        state.voice.phase,
        yapfast_core::voice::Phase::Joining | yapfast_core::voice::Phase::Leaving
    );
    (moving, state.busy, state.online, state.trusted).hash(&mut hasher);
    (state.people_changes, theme).hash(&mut hasher);
    hasher.finish()
}

impl App {
    /// Makes the rail, the sidebar, the header and the history's edge
    /// again, keeping each that comes out the same.
    fn make_lists(&mut self) {
        let state = self.client.state();
        let rail = rail(state, &self.layout.open_folders);
        if self.rail.items != rail {
            self.rail.items = rail;
        }
        let home = crate::rail::home(state);
        if self.rail.home != home {
            self.rail.home = home;
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
            &self.sidebar.filter,
            self.theme.dark,
        );
        if self.sidebar.rows != side {
            self.sidebar.rows = side;
        }
        let discord = match &state.navigation {
            Navigation::Server(server) => state
                .servers
                .iter()
                .any(|known| known.discord && &known.id == server),
            Navigation::DirectMessages => false,
        };
        let places =
            crate::arrange::places(&self.sidebar.rows, discord, !self.sidebar.filter.is_empty());
        if self.sidebar.places != places {
            self.sidebar.places = places;
            // What's dragged may have gone, or moved to another place.
            self.sidebar.arranging = None;
        }
        let head = crate::sidebar::side_header(&self.client.app);
        if self.sidebar.header != head {
            self.sidebar.header = head;
        }
        let header = crate::header::header(state, &self.header, rgb(self.theme.mention));
        if self.header != header {
            self.header = header;
        }
        let edge = Edge::of(state);
        if self.timeline.edge != edge {
            self.timeline.edge = edge;
            self.timeline.edge_changed = true;
        }
        // Someone dragged may drop where their menu's Move to offers, of
        // the channels listed now.
        crate::arrange::moving_targets(self);
        self.timeline.clocks.calls = self.sidebar.rows.iter().any(
            |row| matches!(row, crate::sidebar::SideRow::Room(room) if room.started.is_some()),
        );
        // The open inbox follows the rooms' unread marks.
        crate::window_bar::inbox_sync(self);
    }

    /// Takes in what changed in the client's state; returns the work that
    /// follows (a scroll, pictures to ask for, rows to report).
    pub fn sync(&mut self) -> Task<Message> {
        let state = self.client.state();
        // The DM search's filter is the DM home's: it goes as the rail
        // goes to a server, or back home.
        if self.sidebar.filtered_in.as_ref() != Some(&state.navigation) {
            self.sidebar.filtered_in = Some(state.navigation.clone());
            if !self.sidebar.filter.is_empty() {
                self.sidebar.filter.clear();
                self.lists_shown = None;
            }
        }
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
        let author = looks::post_author(state);
        if self.post_author.as_deref() != author {
            self.post_author = author.map(str::to_owned);
        }
        let account = crate::account::account(state);
        if self.account != account {
            self.account = account;
        }
        let mut tasks = vec![
            crate::profile::profile_sync(self),
            self.status.sync(self.client.state()),
        ];
        self.cards.sync(&self.client.app, &self.profiles);
        crate::friends::friends_sync(self);
        self.quick.sync(self.client.state());
        crate::active_now::active_now_sync(self);
        {
            let height = self.window.height;
            let mut cx = crate::ctx::cx!(self);
            self.reactions.page(height, &mut cx);
        }
        // An account signing in opens each server where this device left
        // it: signing out forgot that, and the backend's first word of the
        // servers comes after this.
        if std::mem::take(&mut self.client.started) {
            self.client
                .app
                .remember_channels(self.layout.channels.clone());
        }
        let state = self.client.state();
        let switched = self.selected != state.selected;
        self.layout
            .keep_uses(self.client.root(), &self.client.app, switched);
        if switched && self.composer.editing.is_some() {
            self.client
                .dispatch(yapfast_core::application::Action::CancelEdit);
        }
        let state = self.client.state();
        // The edit ended (saved, cancelled, or its room left): its field
        // goes.
        if self.composer.editing.is_some() && state.edit().is_none() {
            self.composer.editing = None;
            self.composer.edit.reset("");
        }
        if switched {
            self.selected = state.selected.clone();
            self.timeline.from_end = 0.0;
            self.timeline.at_end = true;
            self.timeline.latest_due = false;
            self.timeline.left_at = None;
            // The composer shows the room's own draft.
            self.composer.draft.reset(state.draft());
            self.composer.draft_known = state.draft().to_owned();
            self.timeline.selection = None;
            self.timeline.selecting = false;
            self.win.menu = None;
            // An emoji's or sticker's card was on the room left.
            self.cards.expression = None;
            self.composer.suggest.clear(&mut self.client);
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
        if !switched && state.draft() != self.composer.draft_known {
            self.composer.draft_known = state.draft().to_owned();
            if self.composer.draft_known != self.composer.draft.text() {
                self.composer.draft.reset(&self.composer.draft_known);
            }
        }
        let seen = {
            let state = self.client.state();
            let (staged, aside) = (state.staged(), state.aside_staged());
            [
                staged.as_ptr().addr(),
                staged.len(),
                aside.as_ptr().addr(),
                aside.len(),
            ]
        };
        if seen != self.uploads.staged_seen {
            self.uploads.staged_seen = seen;
            tasks.push(crate::uploads::previews(self));
        }
        tasks.push(self.roster(switched));
        // The feed came or changed whom it may show.
        let state = self.client.state();
        let people = state.activity_people.as_ref().map(|(_, people)| people);
        let (feed, made_from) = &self.roster.activity_for;
        if !Arc::ptr_eq(feed, &state.activity_feed)
            || made_from.as_ref().map(Arc::as_ptr) != people.map(Arc::as_ptr)
        {
            self.roster.activity_for = (state.activity_feed.clone(), people.cloned());
            crate::members::make_activity_cards(self);
        }
        let state = self.client.state();
        let marks = (state.first_unread.clone(), state.seen.clone());
        let edge_changed = std::mem::take(&mut self.timeline.edge_changed);
        let messages_changed = !Arc::ptr_eq(&self.timeline.messages, &self.client.state().messages);
        if messages_changed || edge_changed || self.timeline.marks_for != marks {
            self.timeline.marks_for = marks;
            // While someone reads earlier messages, what they read stays
            // put as messages come, go or change around it.
            let reading = if switched {
                None
            } else {
                crate::timeline::scroll::reading(self)
            };
            let messages = self.client.state().messages.clone();
            let old = std::mem::replace(&mut self.timeline.messages, messages);
            self.measure(&old, edge_changed);
            tasks.push(crate::timeline::scroll::keep_reading(self, reading));
            // Sent from a window of older messages, or Jump to present: the
            // newest show at the end once they've replaced the window.
            if messages_changed && !self.client.state().detached && self.timeline.latest_due {
                self.timeline.latest_due = false;
                tasks.push(crate::timeline::scroll::go_to_end(self));
            }
            tasks.push(self.report());
        }
        tasks.push(crate::timeline::scroll::follow_jumps(self));
        // Images of pictures the core let go of go too.
        self.win.pictures.prune();
        crate::call::call_sync(self);
        {
            let mut cx = crate::ctx::cx!(self);
            tasks.push(self.viewer.sync(&mut cx));
        }
        tasks.push(crate::thread::thread_sync(self));
        crate::header_popovers::popovers_sync(self);
        crate::search::search_sync(self);
        crate::forum::forum_sync(self);
        self.modal.sync(self.client.state(), &self.theme);
        tasks.push(crate::menus::menu_apps_sync(self));
        if self.client.state().browse.is_none() && !self.sidebar.browse_filter.is_empty() {
            self.sidebar.browse_filter.clear();
        }
        {
            let mut cx = crate::ctx::cx!(self);
            if let Some(picker) = self.win.picker.get_mut() {
                picker.sync(&mut cx);
            }
        }
        {
            let mut cx = crate::ctx::cx!(self);
            if let Some(settings) = self.win.settings.get_mut() {
                settings.voice_sync(&mut cx);
            }
        }
        crate::suggest::mentions_sync(self);
        self.person.sync(self.client.state());
        tasks.push(self.server_settings_sync());
        tasks.push(self.server_dialogs.sync(self.client.state()));
        tasks.push(self.channel_settings_sync());
        tasks.push(self.ask());
        tasks.push(self.sign_in.sync(self.client.state(), self.client.root()));
        tasks.push(self.shell_sync());
        tasks.push(self.mfa.sync(&self.client.app));
        tasks.push(self.follow_backend());
        tasks.push(crate::timeline::clocks::arm_clocks(self));
        Task::batch(tasks)
    }

    /// A backend started (signing in, linking Discord, a saved session) or
    /// linking began: their events come as messages, and whatever came from
    /// the one before stops.
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

    /// The member list, held while people change (`ROSTER_PAUSE`): the
    /// first change shows at once, those in the pause after it together at
    /// its end. A new room shows at once.
    fn roster(&mut self, switched: bool) -> Task<Message> {
        let members = self.roster_source();
        let same = match (&members, &self.roster.members) {
            (Some(new), Some(old)) => Arc::ptr_eq(new, old),
            (None, None) => true,
            _ => false,
        };
        // A new room shows at once, even during a pause: the pending
        // timer's show then finds nothing changed.
        if switched {
            self.show_roster(false);
            return Task::none();
        }
        if same || self.roster.waiting {
            return Task::none();
        }
        let since = self.roster.shown.map(|shown| shown.elapsed());
        // Shown or hidden at once; only churn waits.
        if self.roster.members.is_none()
            || members.is_none()
            || since.is_none_or(|since| since >= looks::ROSTER_PAUSE)
        {
            self.show_roster(false);
            return Task::none();
        }
        self.roster.waiting = true;
        after(
            looks::ROSTER_PAUSE - since.unwrap_or_default(),
            Message::Members(crate::members::Event::Due),
        )
    }

    /// The core's members for the list, while it shows: a hidden list
    /// (its toggle off, a thread or search in its place) keeps no lines,
    /// cards or pictures, and builds none as the roster churns. A group
    /// DM's list shows as a server channel's does.
    fn roster_source(&self) -> Option<Arc<Vec<yapfast_core::chat::ChatMember>>> {
        match &self.client.state().members {
            Some(Ok(members)) if crate::header::members_shown(self) => Some(members.clone()),
            _ => None,
        }
    }

    /// Shows the member list as the core has it; `held`, at the end of a
    /// pause, only if it changed meanwhile.
    pub fn show_roster(&mut self, held: bool) {
        let members = self.roster_source();
        let state = self.client.state();
        if held
            && members.as_ref().map(Arc::as_ptr) == self.roster.members.as_ref().map(Arc::as_ptr)
        {
            return;
        }
        let owner = self.client.server_owner();
        let made = looks::LooksMadeWith::of(state, owner.clone(), self.theme.dark);
        // The looks of members who didn't change stay as made.
        let before = self
            .roster
            .members
            .as_deref()
            .filter(|_| self.roster.made.as_ref() == Some(&made))
            .map(|members| (members.as_slice(), self.roster.lines.as_slice()));
        self.roster.lines = match &members {
            Some(members) => {
                looks::member_lines(state, members, owner.as_deref(), self.theme.dark, before)
            }
            None => Vec::new(),
        };
        self.roster.made = members.is_some().then_some(made);
        self.roster.members = members;
        self.roster.shown = Some(Instant::now());
        crate::members::make_activity_cards(self);
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
            old_looks: &self.timeline.looks,
            old_heights: &self.timeline.heights,
            messages: &self.timeline.messages,
            marks,
            width: self.timeline.width,
            edge: self.timeline.edge.height(),
            edge_changed,
            foot: self.timeline.edge.foot(),
        });
        self.timeline.looks = looks;
        self.timeline.heights = heights;
        self.timeline.clocks.polls = crate::polls::open_poll(&self.timeline.messages, self.minute);
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
        let revealed = &self.timeline.revealed;
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
        self.timeline.tops.clear();
        self.timeline.tops.reserve(self.timeline.heights.len() + 1);
        let mut top = 0.0;
        self.timeline.tops.push(top);
        for height in &self.timeline.heights {
            top += height;
            self.timeline.tops.push(top);
        }
    }

    /// The timeline's viewport moved or changed size.
    pub fn timeline_scrolled(&mut self, viewport: Viewport) -> Task<Message> {
        self.timeline.from_end = viewport.absolute_offset().y;
        self.timeline.height = viewport.bounds().height;
        self.timeline.place.set(viewport.bounds());
        self.timeline.at_end = self.timeline.from_end <= 2.0;
        let width = viewport.bounds().width;
        if (width - self.timeline.width).abs() > 1.0 {
            self.timeline.width = width;
            self.timeline.heights.clear();
            let same = self.timeline.messages.clone();
            self.measure(&same, true);
        }
        // A window opened again, its list at the end: back to the earlier
        // messages the last one was closed on, before the end is reported
        // as read.
        if let Some(left_at) = self.timeline.left_at.take() {
            let back = crate::timeline::scroll::keep_reading(self, Some(left_at));
            self.timeline.at_end = self.timeline.from_end <= 2.0;
            return Task::batch([back, self.report(), self.ask()]);
        }
        Task::batch([self.report(), self.ask()])
    }

    /// Tells the core which rows are in view, at most every 50 ms.
    pub fn report(&mut self) -> Task<Message> {
        if self.reporting || self.timeline.messages.is_empty() {
            return Task::none();
        }
        let (first, last) = self.timeline.in_view(0.0);
        let reads = self.reads();
        if !self.client.visible(
            first,
            last.saturating_sub(1).max(first),
            self.timeline.at_end,
            reads,
        ) {
            self.reporting = true;
            return after(REPORT_EVERY, Message::Report);
        }
        // What the report changed (earlier history asked for) shows later,
        // through events; the state it set is taken in here.
        if Arc::ptr_eq(&self.timeline.messages, &self.client.state().messages) {
            Task::none()
        } else {
            self.sync()
        }
    }

    /// Pictures the rows built show but the core doesn't have yet: avatars
    /// and icons, decorations' and nameplates' stills, tag badges, and the
    /// moving versions of what's under the pointer or on the open card.
    fn wanted(&self) -> Vec<String> {
        self.pictures_shown(true)
    }

    /// Every picture the rows built show, or with `missing` only those the
    /// core doesn't have and hasn't asked for, each once. The whole list
    /// may name a picture twice: it's made to say what stays.
    pub fn pictures_shown(&self, missing: bool) -> Vec<String> {
        let state = self.client.state();
        let cosmetics = state.cosmetics;
        let mut urls: Vec<String> = Vec::new();
        let mut want = |url: Option<&str>| {
            if let Some(url) = url
                && !url.is_empty()
                && (!missing
                    || (!state.pictures.contains_key(url)
                        && !self.client.app.picture_requested(url)))
                && !(missing && urls.iter().any(|wanted| wanted == url))
            {
                urls.push(url.to_owned());
            }
        };
        for item in &self.rail.items {
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
            self.sidebar.scroll.offset - MARGIN,
            self.sidebar.scroll.offset + self.sidebar.scroll.height + MARGIN,
        );
        for row in &self.sidebar.rows {
            let height = row.height();
            if let crate::sidebar::SideRow::Room(room) = row
                && top + height >= from
                && top <= to
            {
                want(room.avatar.as_deref());
                want(room.status.emoji.as_deref());
                want(room.decoration.as_deref());
                want(room.tag.as_ref().and_then(|(_, badge)| badge.as_deref()));
                if room.selected || self.pointed.side.as_deref() == Some(room.id.as_str()) {
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
        for url in self.cards.pictures(&self.client.app, &self.profiles) {
            want(Some(&url));
        }
        if let Some(card) = self.win.tag_card.get() {
            for url in card.pictures(self.client.state()) {
                want(Some(&url));
            }
        }
        for url in crate::suggest::suggestion_pictures(self) {
            want(Some(&url));
        }
        for url in self
            .win
            .picker
            .get()
            .map(crate::picker::Picker::pictures)
            .unwrap_or_default()
        {
            want(Some(&url));
        }
        // A custom emoji among the reactions the hover bar and menus offer.
        if self.pointed.message.is_some() || self.win.menu.is_some() {
            for key in self.client.app.frequent_reactions(crate::menus::QUICK) {
                if let Some((_, id)) = key.split_once(':') {
                    want(crate::suggest::custom_emoji(self.ctx(), id).as_deref());
                }
            }
        }
        // A custom emoji picked for the custom status being written, or
        // for an answer of the poll being made.
        let answers = state.poll_draft.iter().flat_map(|poll| &poll.answers);
        let picked = self
            .status
            .draft
            .iter()
            .filter_map(|draft| draft.emoji.as_deref())
            .chain(answers.filter_map(|answer| answer.emoji.as_deref()));
        for key in picked {
            if let Some((_, id)) = key.split_once(':') {
                want(crate::suggest::custom_emoji(self.ctx(), id).as_deref());
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
        if self.sidebar.header.server.is_some() {
            want(self.sidebar.header.banner.as_deref());
        }
        want(self.header.avatar.as_deref());
        want(self.account.status.emoji.as_deref());
        for compose in [state.compose(), state.aside_compose()]
            .into_iter()
            .flatten()
        {
            let yapfast_core::application::Compose::Reply { emoji, .. } = compose;
            for emoji in emoji {
                want(Some(&emoji.picture));
            }
        }
        let (first, last) = self.timeline.in_view(MARGIN);
        let main = (first..last).map(|index| {
            (
                &self.timeline.messages[index],
                self.timeline.looks.get(index),
            )
        });
        let aside = self.aside.iter().flat_map(|aside| {
            crate::thread::aside_rows_in_view(self)
                .map(move |index| (&aside.messages[index], aside.looks.get(index)))
        });
        for (message, look) in main.chain(aside) {
            if let Some(reply) = &message.reply {
                want(reply.avatar.as_deref());
                for emoji in &reply.emoji {
                    want(Some(&emoji.picture));
                }
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
            if self.pointed.message.as_deref() == Some(message.item.as_str()) {
                want(decoration.map(looks::animated_url).as_deref());
            }
        }
        let (first, last) = self
            .roster
            .scroll
            .range(self.roster.lines.len(), PITCH, TOP);
        for line in &self.roster.lines[first..last] {
            match line {
                MemberLine::Heading { icon, .. } => want(icon.as_deref()),
                MemberLine::Member(look) => {
                    want(look.avatar.as_deref());
                    want(look.decoration.as_deref());
                    want(look.nameplate.as_ref().map(|(picture, _)| &**picture));
                    want(look.role_icon.as_deref());
                    want(look.tag.as_ref().and_then(|(_, badge)| badge.as_deref()));
                    want(look.status.emoji.as_deref());
                    if self.pointed.member.as_ref() == Some(&look.id) {
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
        for url in crate::members::activity_pictures(self) {
            want(Some(url));
        }
        for url in crate::header_popovers::popover_pictures(self)
            .into_iter()
            .chain(crate::forum::forum_pictures(self))
        {
            want(Some(url));
        }
        if crate::call::stage::stage_shown(self) {
            for person in &state.voice.participants {
                want(person.avatar.as_deref());
            }
        }
        for url in crate::search_popout::pictures(self)
            .into_iter()
            .chain(crate::search_filters::pictures(self))
        {
            want(Some(&url));
        }
        for url in crate::profile::profile_pictures(self.ctx()) {
            want(Some(&url));
        }
        for url in crate::friends::friends_pictures(self) {
            want(Some(&url));
        }
        for url in crate::reactions::pictures(self.client.state()) {
            want(Some(&url));
        }
        let server_settings = self
            .server_screen
            .get()
            .map(|screen| screen.pictures(self.client.state(), self.window.height))
            .unwrap_or_default();
        for url in server_settings {
            want(Some(&url));
        }
        for url in crate::server_dialogs::pictures(self.client.state())
            .into_iter()
            .chain(crate::link_dialogs::pictures(self.client.state()))
        {
            want(Some(&url));
        }
        for url in crate::channel_settings::pictures(self.client.state())
            .collect::<Vec<_>>()
            .into_iter()
            .chain(crate::settings::voice::entrance_pictures(self))
        {
            want(Some(&url));
        }
        urls
    }

    /// Asks for missing pictures in one batch 50 ms from now.
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
