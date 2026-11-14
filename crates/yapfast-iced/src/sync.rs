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
    sidebar::{ROW, sidebar},
    timeline::{self, MARGIN, TIMELINE, continues, row_height},
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

impl App {
    /// Takes in what changed in the client's state; returns the work that
    /// follows (a scroll, pictures to ask for, rows to report).
    pub fn sync(&mut self) -> Task<Message> {
        let state = self.client.state();
        let rail = rail(state);
        if self.rail != rail {
            self.rail = rail;
        }
        let side = sidebar(state);
        if self.side != side {
            self.side = side;
        }
        self.header = crate::header::room(state);
        self.server = matches!(state.navigation, Navigation::Server(_));
        self.in_server = looks::in_server(state.selected.as_deref());
        self.account = (
            state.account_name.clone().unwrap_or_default(),
            state.account_avatar.clone(),
        );
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
        // The edit ended (saved, cancelled, or its room left): the draft
        // comes back.
        if self.editing.is_some() && state.edit().is_none() {
            self.editing = None;
            if !switched {
                self.draft.reset(state.draft());
                self.draft_known = state.draft().to_owned();
            }
        }
        if switched {
            self.selected = state.selected.clone();
            self.from_end = 0.0;
            self.at_end = true;
            // The composer shows the room's own draft, as the Slint client's.
            self.draft.reset(state.draft());
            self.draft_known = state.draft().to_owned();
            self.selection = None;
            self.selecting = false;
            self.menu = None;
            self.mention_at = None;
            tasks.push(operation::scroll_to(
                TIMELINE,
                AbsoluteOffset {
                    x: None,
                    y: Some(0.0),
                },
            ));
        }
        // The core changed the draft itself, as when a message it sent went.
        if !switched && self.editing.is_none() && state.draft() != self.draft_known {
            self.draft_known = state.draft().to_owned();
            if self.draft_known != self.draft.text() {
                self.draft.reset(&self.draft_known);
            }
        }
        tasks.push(self.roster(switched));
        if !Arc::ptr_eq(&self.messages, &self.client.state().messages) {
            let messages = self.client.state().messages.clone();
            let old = std::mem::replace(&mut self.messages, messages);
            let old_total = self.total_height();
            let below = self.measure(&old);
            // Rows added below while the person reads earlier ones: keep
            // those rows in view (the scrollable keeps the distance from the
            // end, which suits rows added above and following the end).
            if !switched && !self.at_end && below > 0.0 && self.total_height() > old_total {
                self.from_end += below;
                tasks.push(operation::scroll_to(
                    TIMELINE,
                    AbsoluteOffset {
                        x: None,
                        y: Some(self.from_end),
                    },
                ));
            }
            tasks.push(self.report());
        }
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
        tasks.push(self.ask());
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
            Some(members) => {
                looks::member_lines(state, members, self.client.server_owner().as_deref())
            }
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

    /// Works out the rows' heights for the current messages, reusing those
    /// of rows that, with the row before them, are the same as in `old`.
    /// Returns the height of the rows added after `old`'s last.
    pub fn measure(&mut self, old: &Messages) -> f32 {
        let messages = &self.messages;
        let shift = old.first().and_then(|first| {
            messages
                .iter()
                .position(|message| Arc::ptr_eq(message, first))
        });
        let mut heights = Vec::with_capacity(messages.len());
        for index in 0..messages.len() {
            let same = shift
                .and_then(|shift| index.checked_sub(shift))
                .filter(|&before| {
                    before < old.len()
                        && before < self.heights.len()
                        && Arc::ptr_eq(&old[before], &messages[index])
                        && (index == 0
                            || (before > 0 && Arc::ptr_eq(&old[before - 1], &messages[index - 1])))
                });
            heights.push(match same {
                Some(before) => self.heights[before],
                None => row_height(&messages[index], continues(messages, index), self.width),
            });
        }
        let below = old
            .last()
            .and_then(|last| {
                messages
                    .iter()
                    .rposition(|message| message.item == last.item)
            })
            .map_or(0.0, |last| heights[last + 1..].iter().sum());
        self.tops.clear();
        self.tops.reserve(heights.len() + 1);
        let mut top = 0.0;
        self.tops.push(top);
        for height in &heights {
            top += height;
            self.tops.push(top);
        }
        self.heights = heights;
        below
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
            self.measure(&Messages::default());
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
        }
        let (first, last) = self.side_scroll.range(self.side.len(), ROW, 0.0);
        for item in &self.side[first..last] {
            want(item.avatar.as_deref());
        }
        want(self.account.1.as_deref());
        let (first, last) = self.in_view(MARGIN);
        for index in first..last {
            let message = &self.messages[index];
            if continues(&self.messages, index) {
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
                    want(look.activity_emoji.as_deref());
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
