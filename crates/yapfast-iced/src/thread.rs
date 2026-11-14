//! A Discord thread beside the open channel, in place of its member list,
//! as the Slint client's `ThreadPanel` and Discord open a thread from a
//! message: its header (More, Open in full view, Close), its timeline,
//! and a composer of its own with the reply's bar and files; and Discord's
//! "New Thread" form in the same place (`NewThreadPanel`): the message it
//! starts from, its name, whether it's private, and the first message.
//!
//! The thread's rows are the timeline's own (`timeline_row`), numbered
//! from [`ASIDE`] so the messages they send say which timeline they're
//! in. Its state goes when it closes, and with it its pictures' copies.

use std::{sync::Arc, time::Instant};

use iced::{
    Alignment, Border, Element, Length, Padding, Task,
    widget::{
        Id, Space, column, container, mouse_area, operation, row, scrollable, scrollable::Viewport,
        stack, text,
    },
};
use yapfast_core::application::{Action, Compose, ThreadFrom};

use crate::{
    app::{App, Message, Messages, key},
    composer::{Event as Write, Writing},
    controls::{self, Kind, bar},
    elide::elide,
    field::{self, Draft, Field},
    fonts::BOLD,
    form,
    pictures::Icon,
    popover::{Popover, Side},
    style::{corners, rounded, scrollbar},
    timeline::{ASIDE, Edge, MARGIN, Marks, RowLook},
    widgets::{Found, Rows, Watch},
};

/// The panel's header, its least and most width, and the width it opens at.
pub const HEAD: f32 = 48.0;
pub const WIDTH: f32 = 420.0;
const LEAST: f32 = 320.0;
const MOST: f32 = 800.0;
/// The thread's list and composer, for the app's scroll and focus tasks.
pub const LIST: &str = "aside";
pub const COMPOSER: &str = "aside-composer";
/// The New Thread form's name field and first message.
pub const NAME: &str = "new-thread-name";
pub const FIRST: &str = "new-thread-first";
/// How long between reports of the rows in view, as the timeline's.
const REPORT_EVERY: std::time::Duration = std::time::Duration::from_millis(50);
/// Rows reported either side of those in view.
const REPORT_MARGIN: usize = 6;

/// The thread beside the channel, as the window shows it.
pub struct Aside {
    pub room: String,
    pub name: String,
    pub ready: bool,
    pub loading: bool,
    pub start: bool,
    pub failed: bool,
    pub messages: Messages,
    pub looks: Vec<RowLook>,
    pub heights: Vec<f32>,
    pub tops: Vec<f32>,
    pub found: Found,
    /// Its viewport, as its scrollable (anchored to the bottom) reports it.
    pub from_end: f32,
    pub height: f32,
    pub width: f32,
    reported: Option<Instant>,
}

impl Aside {
    fn new(room: String, width: f32) -> Self {
        Self {
            room,
            name: String::new(),
            ready: false,
            loading: false,
            start: false,
            failed: false,
            messages: Messages::default(),
            looks: Vec::new(),
            heights: Vec::new(),
            tops: vec![0.0],
            found: Found::default(),
            from_end: 0.0,
            height: 400.0,
            width,
            reported: None,
        }
    }

    pub fn total_height(&self) -> f32 {
        self.tops.last().copied().unwrap_or(0.0)
    }

    /// Where the view's top is among the rows.
    pub fn view_top(&self) -> f32 {
        (self.total_height() - self.height - self.from_end).max(0.0)
    }

    /// The rows in view, and `margin` beyond.
    fn in_view(&self, margin: f32) -> (usize, usize) {
        let top = self.view_top();
        let bottom = top + self.height;
        let count = self.heights.len();
        let first = self.tops[1..].partition_point(|end| *end <= top - margin);
        let last = self.tops[..count].partition_point(|start| *start < bottom + margin);
        (first.min(count), last.max(first).min(count))
    }

    fn place_rows(&mut self) {
        self.tops.clear();
        let mut top = 0.0;
        self.tops.push(top);
        for height in &self.heights {
            top += height;
            self.tops.push(top);
        }
    }

    fn edge(&self) -> Edge {
        Edge {
            ready: self.ready,
            loading: self.loading,
            failed: self.failed,
            start: self.start,
            thread: true,
            name: self.name.clone(),
            ..Edge::default()
        }
    }
}

/// The composers of the thread beside and of the New Thread form.
#[derive(Default)]
pub struct Drafts {
    pub aside: Draft,
    pub aside_known: String,
    pub first: Draft,
    pub first_known: String,
    /// The form's name, as typed, and the message it starts from.
    pub name: String,
    pub starter: Option<crate::found::Found>,
    pub focused: bool,
}

#[derive(Clone, Debug)]
pub enum Event {
    Scrolled(Viewport),
    Measured(Vec<(u64, f32)>),
    Earlier,
    Close,
    Expand,
    More,
    Send,
    Attach,
    CancelReply,
    TogglePing,
    /// A thread to show beside its channel: View Thread, a thread's link,
    /// a list's row with Shift.
    Open(String),
    /// Create Thread from a message's menu.
    Start(String),
    /// The form: its name, private or not, Create, its close and the
    /// question before leaving it.
    Name(String),
    Private(bool),
    Create,
    CloseForm,
    Leave,
    Stay,
    Jump(String),
}

impl App {
    pub fn thread_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Scrolled(viewport) => {
                let Some(aside) = &mut self.aside else {
                    return Task::none();
                };
                aside.from_end = viewport.absolute_offset().y;
                aside.height = viewport.bounds().height;
                let width = viewport.bounds().width + 1.0;
                if (width - aside.width).abs() > 1.0 {
                    aside.width = width;
                    aside.heights.clear();
                    let same = aside.messages.clone();
                    self.measure_aside(&same, true);
                }
                Task::batch([self.aside_report(), self.ask()])
            }
            Event::Measured(heights) => {
                self.aside_found(heights);
                Task::none()
            }
            Event::Earlier => {
                self.client.dispatch(Action::AsideEarlier);
                self.sync()
            }
            Event::Close => {
                self.client.dispatch(Action::CloseAside);
                Task::batch([self.sync(), self.refocus()])
            }
            Event::Expand => {
                self.client.dispatch(Action::ExpandAside);
                Task::batch([self.sync(), self.refocus()])
            }
            Event::More => {
                let Some(room) = self.aside.as_ref().map(|aside| aside.room.clone()) else {
                    return Task::none();
                };
                let at = self.pressed.get();
                self.open_thread_more(&room, iced::Point::new(at.x - 200.0, at.y + 20.0))
            }
            Event::Send => self.send_aside(),
            Event::Attach => self.attach_aside(),
            Event::CancelReply => {
                self.client.dispatch(Action::CancelAsideCompose);
                Task::batch([self.sync(), operation::focus(COMPOSER)])
            }
            Event::TogglePing => {
                self.client
                    .dispatch(Action::ToggleReplyPing { aside: true });
                Task::batch([self.sync(), operation::focus(COMPOSER)])
            }
            Event::Open(room) => {
                self.client.dispatch(Action::OpenAside(room));
                Task::batch([self.sync(), operation::focus(COMPOSER)])
            }
            Event::Start(item) => {
                self.client.dispatch(Action::NewThread {
                    message: Some(item),
                    from: ThreadFrom::Message,
                });
                Task::batch([self.sync(), operation::focus(FIRST)])
            }
            Event::Name(name) => {
                self.drafts.name = name.clone();
                self.client.dispatch(Action::WriteNewThread {
                    name: Some(name),
                    body: None,
                });
                self.sync()
            }
            Event::Private(private) => {
                self.client.dispatch(Action::SetThreadPrivate(private));
                self.sync()
            }
            Event::Create => {
                self.client.dispatch(Action::WriteNewThread {
                    name: Some(self.drafts.name.clone()),
                    body: Some(self.drafts.first.text()),
                });
                self.client.dispatch(Action::CreateThread);
                self.sync()
            }
            Event::CloseForm => {
                self.client.dispatch(Action::CloseNewThread);
                Task::batch([self.sync(), self.refocus()])
            }
            Event::Leave => {
                self.client.dispatch(Action::LeaveNewThread);
                Task::batch([self.sync(), self.refocus()])
            }
            Event::Stay => {
                self.client.dispatch(Action::StayInNewThread);
                Task::batch([self.sync(), operation::focus(FIRST)])
            }
            Event::Jump(item) => {
                self.client.dispatch(Action::Jump(item));
                self.sync()
            }
        }
    }

    /// Takes in the core's thread beside the channel and New Thread form:
    /// a new thread starts from nothing, one gone lets go of its rows.
    pub fn thread_sync(&mut self) -> Task<Message> {
        let state = self.client.state();
        let Some(core) = state.aside.as_ref() else {
            self.aside = None;
            self.drafts.aside.reset("");
            self.drafts.aside_known.clear();
            return self.new_thread_sync();
        };
        let width = self.layout.aside_width.unwrap_or(WIDTH).clamp(LEAST, MOST);
        if self
            .aside
            .as_ref()
            .is_none_or(|aside| aside.room != core.room)
        {
            self.aside = Some(Box::new(Aside::new(core.room.clone(), width)));
            let draft = state.aside_draft().to_owned();
            self.drafts.aside.reset(&draft);
            self.drafts.aside_known = draft;
        }
        let name = state
            .aside_room()
            .map(|room| room.name.clone())
            .unwrap_or_default();
        let (ready, loading, start, failed) = (
            core.ready,
            core.loading_history,
            core.history_start,
            core.history_failed,
        );
        let messages = core.messages.clone();
        let seen = core.seen.clone();
        let draft = state.aside_draft().to_owned();
        if draft != self.drafts.aside_known {
            if draft != self.drafts.aside.text() {
                self.drafts.aside.reset(&draft);
            }
            self.drafts.aside_known = draft;
        }
        let Some(aside) = &mut self.aside else {
            return Task::none();
        };
        let edge_changed = (aside.ready, aside.loading, aside.start, aside.failed)
            != (ready, loading, start, failed);
        aside.name = name;
        aside.ready = ready;
        aside.loading = loading;
        aside.start = start;
        aside.failed = failed;
        if edge_changed || !Arc::ptr_eq(&aside.messages, &messages) {
            let old = std::mem::replace(&mut aside.messages, messages);
            self.seen_aside = seen;
            self.measure_aside(&old, edge_changed);
        }
        Task::batch([self.aside_earlier(), self.new_thread_sync()])
    }

    /// The New Thread form's first message follows the core's.
    fn new_thread_sync(&mut self) -> Task<Message> {
        match self.client.state().new_thread() {
            Some(form) => {
                if form.body != self.drafts.first_known {
                    if form.body != self.drafts.first.text() {
                        self.drafts.first.reset(&form.body);
                    }
                    self.drafts.first_known = form.body.clone();
                }
                if form.name != self.drafts.name {
                    self.drafts.name = form.name.clone();
                }
                let item = form.message.clone();
                if self
                    .drafts
                    .starter
                    .as_ref()
                    .map(|found| &found.message.item)
                    != item.as_ref()
                {
                    let message = item.and_then(|item| self.client.state().message(&item).cloned());
                    self.drafts.starter = message.map(|message| self.found(Arc::new(message)));
                }
            }
            None => {
                if !self.drafts.first_known.is_empty() || !self.drafts.first.is_empty() {
                    self.drafts.first.reset("");
                    self.drafts.first_known.clear();
                }
                self.drafts.name.clear();
                self.drafts.starter = None;
            }
        }
        Task::none()
    }

    /// Works out the thread's rows as the timeline's are (`measured_rows`).
    fn measure_aside(&mut self, old: &Messages, edge_changed: bool) {
        let Some(aside) = &self.aside else {
            return;
        };
        let marks = Marks {
            seen: self.seen_aside.as_deref(),
            first_unread: None,
        };
        let (looks, heights) = self.measured_rows(crate::sync::Measuring {
            old,
            old_looks: &aside.looks,
            old_heights: &aside.heights,
            messages: &aside.messages,
            marks,
            width: aside.width - 1.0,
            edge: if aside.ready {
                crate::timeline::EDGE
            } else {
                0.0
            },
            edge_changed,
            foot: 0.0,
        });
        if let Some(aside) = &mut self.aside {
            aside.looks = looks;
            aside.heights = heights;
            aside.place_rows();
        }
    }

    /// Heights the thread's rows found at their last layout.
    fn aside_found(&mut self, heights: Vec<(u64, f32)>) {
        let Some(aside) = &mut self.aside else {
            return;
        };
        let (first, last) = aside.in_view(MARGIN);
        let mut changed = false;
        for (row, height) in heights {
            if let Some(index) =
                (first..last).find(|&index| key(&aside.messages[index].item) == row)
                && (aside.heights[index] - height).abs() > 0.5
            {
                aside.heights[index] = height;
                changed = true;
            }
        }
        if changed {
            aside.place_rows();
        }
    }

    /// Heights found since the last message, taken in before the next.
    pub fn take_aside_found(&mut self) {
        let Some(found) = self
            .aside
            .as_ref()
            .map(|aside| std::mem::take(&mut *aside.found.borrow_mut()))
        else {
            return;
        };
        if !found.is_empty() {
            self.aside_found(found);
        }
    }

    /// Earlier messages load as the list nears its top, a page at a time,
    /// until the first; after a failure they wait for Retry.
    fn aside_earlier(&mut self) -> Task<Message> {
        let busy = self.client.state().busy;
        let Some(aside) = &self.aside else {
            return Task::none();
        };
        let wants = aside.ready
            && !busy
            && !aside.messages.is_empty()
            && !aside.loading
            && !aside.start
            && !aside.failed
            && aside.view_top() < aside.height;
        if wants {
            self.client.dispatch(Action::AsideEarlier);
        }
        Task::none()
    }

    /// Tells the core which of the thread's rows are in view, at most every
    /// 50 ms, so their pictures load and they're read.
    fn aside_report(&mut self) -> Task<Message> {
        let Some(aside) = &mut self.aside else {
            return Task::none();
        };
        if aside.messages.is_empty()
            || aside
                .reported
                .is_some_and(|reported| reported.elapsed() < REPORT_EVERY)
        {
            return self.aside_earlier();
        }
        aside.reported = Some(Instant::now());
        let (first, last) = aside.in_view(0.0);
        let end = aside.messages.len() - 1;
        let items = aside.messages
            [first.saturating_sub(REPORT_MARGIN).min(end)..=(last + REPORT_MARGIN).min(end)]
            .iter()
            .map(|message| message.item.clone())
            .collect();
        let room = aside.room.clone();
        self.client.dispatch(Action::VisibleMessages {
            room,
            items,
            at_end: false,
        });
        Task::batch([self.aside_earlier(), self.sync()])
    }

    /// The rows of the thread in view, for the pictures they want.
    pub fn aside_rows_in_view(&self) -> std::ops::Range<usize> {
        match &self.aside {
            Some(aside) => {
                let (first, last) = aside.in_view(MARGIN);
                first..last
            }
            None => 0..0,
        }
    }

    /// Whether this account can write in the thread beside.
    pub fn aside_writes(&self) -> bool {
        self.aside
            .as_ref()
            .is_some_and(|aside| self.client.state().can_write_in(&aside.room))
    }

    fn send_aside(&mut self) -> Task<Message> {
        self.client
            .dispatch(Action::SetAsideDraft(self.drafts.aside.text()));
        self.client.dispatch(Action::SendAside);
        self.suggest.clear(&mut self.client);
        let to_end = operation::scroll_to(
            LIST,
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(0.0),
            },
        );
        Task::batch([self.sync(), to_end])
    }

    fn attach_aside(&mut self) -> Task<Message> {
        Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Attach files")
                    .pick_files()
                    .await
                    .map(|files| files.iter().map(|file| file.path().to_owned()).collect())
                    .unwrap_or_default()
            },
            |files: Vec<std::path::PathBuf>| {
                Message::Write(Write::Uploads(crate::uploads::Event::ChosenAside(files)))
            },
        )
    }

    /// Whether the panel beside the conversation is the thread's or the
    /// New Thread form.
    pub fn aside_shown(&self) -> bool {
        (self.aside.is_some() || self.client.state().new_thread().is_some()) && !self.search_shown()
    }

    /// The panel's width, as dragged and kept.
    pub fn aside_width(&self) -> f32 {
        self.aside.as_ref().map_or_else(
            || self.layout.aside_width.unwrap_or(WIDTH).clamp(LEAST, MOST),
            |aside| aside.width,
        )
    }

    /// The thread beside the channel, or the New Thread form.
    pub fn aside_view(&self) -> Option<Element<'_, Message>> {
        if let Some(form) = self.new_thread_view() {
            return Some(self.panel(form));
        }
        let aside = self.aside.as_ref()?;
        let theme = &self.theme;
        let head = row![
            self.glyph(Icon::Thread, 18, theme.muted),
            container(
                elide(aside.name.as_str())
                    .size(15)
                    .font(BOLD)
                    .color(theme.text)
            )
            .width(Length::Fill),
            self.icon_button(
                Icon::Ellipsis,
                "More",
                28.0,
                18,
                Message::Thread(Event::More),
                true
            ),
            self.icon_button(
                Icon::Maximize,
                "Open in full view",
                28.0,
                18,
                Message::Thread(Event::Expand),
                true
            ),
            self.icon_button(
                Icon::X,
                "Close thread",
                28.0,
                18,
                Message::Thread(Event::Close),
                true
            ),
        ];
        let body = column![
            self.panel_head(head),
            self.aside_list(aside),
            self.aside_composer(aside),
        ];
        Some(self.panel(body.into()))
    }

    /// The panel's frame: the edge on its left, its ground.
    fn panel<'a>(&'a self, body: Element<'a, Message>) -> Element<'a, Message> {
        let ground = self.theme.chat;
        row![
            bar(self.theme.edge, 1, Length::Fill),
            container(body)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| crate::style::fill(ground)),
        ]
        .width(self.aside_width())
        .height(Length::Fill)
        .into()
    }

    /// A panel's header: its row, and the line under it.
    fn panel_head<'a>(&'a self, head: iced::widget::Row<'a, Message>) -> Element<'a, Message> {
        column![
            container(head.spacing(8).align_y(Alignment::Center))
                .padding(Padding::ZERO.left(16).right(10))
                .height(HEAD - 1.0)
                .align_y(Alignment::Center),
            bar(self.theme.line, Length::Fill, 1),
        ]
        .into()
    }

    fn aside_list<'a>(&'a self, aside: &'a Aside) -> Element<'a, Message> {
        let theme = &self.theme;
        if aside.messages.is_empty() {
            let words: Element<'_, Message> = if aside.ready {
                text(format!("This is the start of the {} thread.", aside.name))
                    .size(14)
                    .color(theme.muted)
                    .center()
                    .into()
            } else {
                text("Loading...").size(14).color(theme.muted).into()
            };
            return container(words)
                .padding([0, 16])
                .center(Length::Fill)
                .into();
        }
        let (first, last) = aside.in_view(MARGIN);
        let mut rows = Rows::new(aside.total_height()).measured(
            |heights| Message::Thread(Event::Measured(heights)),
            aside.found.clone(),
            None,
        );
        let lead = (aside.height - aside.total_height()).max(0.0);
        for index in first..last {
            rows.push(
                key(&aside.messages[index].item),
                aside.tops[index],
                aside.heights[index],
                self.timeline_row(ASIDE + index),
            );
        }
        let list = scrollable(column![Space::new().height(lead), rows])
            .id(LIST)
            .anchor_bottom()
            .on_scroll(|viewport| Message::Thread(Event::Scrolled(viewport)))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(scrollbar(theme));
        Watch::new(
            list,
            self.selection.is_some().then_some(Message::SelectClear),
            self.selecting.then_some(Message::SelectEnd),
        )
        .into()
    }

    /// The thread's history edge, at its first row.
    pub fn aside_edge(&self) -> Element<'_, Message> {
        match &self.aside {
            Some(aside) => self.edge_line(&aside.edge(), Message::Thread(Event::Earlier)),
            None => Space::new().into(),
        }
    }

    /// The thread's composer: the reply's bar over it, the files waiting,
    /// Attach, the field and Send.
    fn aside_composer<'a>(&'a self, aside: &'a Aside) -> Element<'a, Message> {
        let state = self.client.state();
        let writes = state.can_write_in(&aside.room);
        let reply = match state.aside_compose() {
            Some(Compose::Reply {
                author, body, ping, ..
            }) => Some((
                format!(
                    "Replying to {author}: {}",
                    body.lines().next().unwrap_or_default()
                ),
                *ping,
            )),
            _ => None,
        };
        let placeholder = if reply.is_some() {
            "Write a reply".to_owned()
        } else {
            format!("Message {}", aside.name)
        };
        let can_send = state.can_send_in(&aside.room);
        let sending = state.sending.is_some();
        let mut composer = SideComposer::new(Writing::Aside, placeholder)
            .enabled(writes)
            .send(
                if sending { "Sending..." } else { "Send" },
                can_send.then_some(Message::Thread(Event::Send)),
                can_send,
            );
        if let Some((label, ping)) = reply {
            composer = composer.reply(label, ping);
        }
        if let Some(tray) = self.aside_tray() {
            composer = composer.tray(tray);
        }
        composer = composer.attach(
            state
                .aside_can_attach()
                .then_some(Message::Thread(Event::Attach)),
        );
        composer.view(self)
    }

    /// Discord's New Thread form, beside the channel.
    fn new_thread_view(&self) -> Option<Element<'_, Message>> {
        let form = self.client.state().new_thread()?;
        let theme = &self.theme;
        let head = row![
            self.glyph(Icon::Thread, 18, theme.muted),
            container(elide("New Thread").size(15).font(BOLD).color(theme.text))
                .width(Length::Fill),
            self.icon_button(
                Icon::X,
                "Close",
                28.0,
                18,
                Message::Thread(Event::CloseForm),
                true
            ),
        ];
        let mut fields = column![].spacing(16).padding(16).width(Length::Fill);
        if let Some(starter) = &self.drafts.starter {
            fields = fields.push(self.found_view(
                starter,
                Message::Thread(Event::Jump(starter.message.item.clone())),
            ));
        }
        let placeholder = if form.default_name.is_empty() {
            "New Thread"
        } else {
            form.default_name.as_str()
        };
        let mut name = form::entry(theme, NAME, placeholder, &self.drafts.name);
        if !form.starting {
            name = name
                .on_input(|name| Message::Thread(Event::Name(name)))
                .on_submit(Message::Focus(FIRST));
        }
        fields = fields.push(
            column![
                form::heading(
                    theme,
                    if form.message.is_some() {
                        "THREAD NAME (OPTIONAL)"
                    } else {
                        "THREAD NAME"
                    }
                ),
                name,
            ]
            .spacing(8),
        );
        if form.can_private {
            let words = if form.private {
                "Only people you invite and moderators can see. You can invite new people by @mentioning them."
            } else {
                "Only people you invite and moderators can see"
            };
            fields = fields.push(
                form::switch(
                    "Private Thread",
                    form.private,
                    (!form.only_private && !form.starting)
                        .then_some(Message::Thread(Event::Private(!form.private))),
                )
                .description(words)
                .view(self),
            );
        }
        if let Some(error) = &form.error {
            fields = fields.push(text(error.as_str()).size(14).color(theme.danger));
        }
        let composer = SideComposer::new(
            Writing::NewThread,
            "Enter a message to start the conversation!".to_owned(),
        )
        .enabled(!form.starting)
        .send(
            if form.starting {
                "Creating..."
            } else {
                "Create Thread"
            },
            (!form.starting).then_some(Message::Thread(Event::Create)),
            !form.starting && !self.drafts.first.is_empty(),
        )
        .view(self);
        let body: Element<'_, Message> = column![
            self.panel_head(head),
            scrollable(fields)
                .height(Length::Fill)
                .style(scrollbar(theme)),
            composer,
        ]
        .into();
        if !form.leaving {
            return Some(body);
        }
        Some(
            stack![
                body,
                self.leave_question(Message::Thread(Event::Stay), Message::Thread(Event::Leave))
            ]
            .into(),
        )
    }

    /// Discord's question before leaving what was written: Back to editing,
    /// or Exit, over the panel or form it's about.
    pub fn leave_question(&self, stay: Message, leave: Message) -> Element<'_, Message> {
        let theme = &self.theme;
        let (ground, edge) = (theme.sidebar, theme.edge);
        let card = container(
            column![
                text("Are you sure you want to go?")
                    .size(17)
                    .font(BOLD)
                    .color(theme.text),
                text("Any changes you made will be lost if you exit now.")
                    .size(14)
                    .color(theme.secondary),
                container(
                    row![
                        controls::action(theme, "Back to editing", Kind::Ghost, Some(stay)),
                        controls::action(theme, "Exit", Kind::Danger, Some(leave)),
                    ]
                    .spacing(8)
                )
                .width(Length::Fill)
                .padding(Padding::ZERO.top(6))
                .align_x(Alignment::End),
            ]
            .spacing(10),
        )
        .padding(16)
        .max_width(440)
        .style(move |_| container::Style {
            border: Border {
                color: edge,
                width: 1.0,
                radius: 10.0.into(),
            },
            ..rounded(ground, 10)
        });
        mouse_area(
            container(card)
                .padding([0, 16])
                .center(Length::Fill)
                .style(|_| crate::style::fill(DIM)),
        )
        .on_press(Message::Nothing)
        .into()
    }
}

/// The dim under the question before leaving.
const DIM: iced::Color = iced::Color::from_rgba(0.0, 0.0, 0.0, 0xa0 as f32 / 255.0);

/// A composer of a panel beside the conversation, as the Slint client's
/// thread and forms have: a reply's bar over it, the files waiting, the
/// Attach button, the field (one of [`Writing`]'s), and Send.
pub struct SideComposer<'a> {
    which: Writing,
    placeholder: String,
    enabled: bool,
    reply: Option<(String, bool)>,
    attach: Option<Option<Message>>,
    send: Option<(&'static str, Option<Message>, bool)>,
    tray: Option<Element<'a, Message>>,
}

impl<'a> SideComposer<'a> {
    pub fn new(which: Writing, placeholder: String) -> Self {
        Self {
            which,
            placeholder,
            enabled: true,
            reply: None,
            attach: None,
            send: None,
            tray: None,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The reply's bar over it, and whether the reply mentions.
    pub fn reply(mut self, label: String, ping: bool) -> Self {
        self.reply = Some((label, ping));
        self
    }

    /// The Attach button, greyed without a message.
    pub fn attach(mut self, message: Option<Message>) -> Self {
        self.attach = Some(message);
        self
    }

    /// The Send button: its tip, what it sends, and whether it's lit.
    pub fn send(mut self, label: &'static str, message: Option<Message>, lit: bool) -> Self {
        self.send = Some((label, message, lit));
        self
    }

    /// The files waiting, over the field.
    pub fn tray(mut self, tray: Element<'a, Message>) -> Self {
        self.tray = Some(tray);
        self
    }

    pub fn view(self, app: &'a App) -> Element<'a, Message> {
        let theme = &app.theme;
        let draft = app.writing(self.which);
        let field = Field::new(
            draft,
            Id::new(field_id(self.which)),
            self.placeholder,
            on_field(self.which),
            field::Colors {
                ground: iced::Color::TRANSPARENT,
                text: theme.text,
                placeholder: theme.placeholder,
                selection: theme.selection,
            },
        )
        .padding(Padding {
            top: 13.0,
            right: 6.0,
            bottom: 13.0,
            left: 6.0,
        })
        .enabled(self.enabled)
        .suggesting(app.suggesting(self.which));
        let lift = |button: Element<'static, Message>| -> Element<'a, Message> {
            container(button).padding(Padding::ZERO.bottom(8)).into()
        };
        let mut line = row![].spacing(6).align_y(Alignment::End);
        if let Some(attach) = self.attach {
            line = line.push(lift(app.composer_button(
                Icon::CirclePlus,
                "Attach files",
                22,
                theme.secondary,
                attach,
            )));
        }
        line = line.push(field);
        if let Some((label, message, lit)) = self.send {
            let tint = if lit { theme.accent } else { theme.secondary };
            line = line.push(lift(app.composer_button(
                Icon::Send,
                label,
                20,
                tint,
                message,
            )));
        }
        let replying = self.reply.is_some();
        let radius = if replying {
            corners([0.0, 0.0, 10.0, 10.0])
        } else {
            10.0.into()
        };
        let mut inside = column![];
        if let Some(tray) = self.tray {
            inside = inside.push(tray);
        }
        let ground = theme.field;
        let frame = container(inside.push(container(line).padding(Padding::ZERO.left(8).right(8))))
            .width(Length::Fill)
            .style(move |_| rounded(ground, radius));
        let frame = Popover::new(frame, app.suggestions_view(self.which), Side::Above);
        let mut compose = column![];
        if let Some((label, ping)) = self.reply {
            compose = compose.push(app.reply_bar_for(
                label,
                ping,
                Message::Thread(Event::TogglePing),
                Message::Thread(Event::CancelReply),
            ));
        }
        container(compose.push(frame))
            .padding(Padding {
                top: 0.0,
                right: 12.0,
                bottom: 12.0,
                left: 12.0,
            })
            .into()
    }
}

/// The id of a side composer's field.
pub fn field_id(which: Writing) -> &'static str {
    match which {
        Writing::Aside => COMPOSER,
        Writing::NewThread => FIRST,
        Writing::NewPost => crate::forum::BODY,
        Writing::Draft => crate::composer::COMPOSER,
        Writing::Edit => crate::composer::EDIT,
    }
}

fn on_field(which: Writing) -> fn(field::Event) -> Message {
    match which {
        Writing::Aside => |event| Message::Write(Write::Field(Writing::Aside, event)),
        Writing::NewThread => |event| Message::Write(Write::Field(Writing::NewThread, event)),
        Writing::NewPost => |event| Message::Write(Write::Field(Writing::NewPost, event)),
        Writing::Draft => |event| Message::Write(Write::Field(Writing::Draft, event)),
        Writing::Edit => |event| Message::Write(Write::Field(Writing::Edit, event)),
    }
}

/// Starts a thread from the composer's + menu or a list's Create.
pub fn start_from(from: ThreadFrom) -> Action {
    Action::NewThread {
        message: None,
        from,
    }
}
