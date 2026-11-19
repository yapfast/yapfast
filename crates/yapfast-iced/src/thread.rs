//! A Discord thread beside the open channel, in place of its member list,
//! as Discord opens a thread from a message: its header (More, Open in
//! full view, Close), its timeline, and a composer of its own with the
//! reply's bar and files; and Discord's "New Thread" form in the same
//! place: the message it starts from, its name, whether it's private, and
//! the first message.
//!
//! The thread's rows are the timeline's own (`timeline_row`), numbered
//! from [`ASIDE`] so the messages they send say which timeline they're
//! in. Its state goes when it closes, and with it its pictures' copies.

use crate::widgets::scroll_view;
use std::{sync::Arc, time::Instant};

use iced::{
    Alignment, Element, Length, Padding, Task,
    widget::{
        Id, Space, column, container, mouse_area, operation, row, scrollable, scrollable::Viewport,
        stack, text,
    },
};
use yapfast_core::application::{Action, ThreadFrom};

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
    sync::MARGIN,
    timeline::{ASIDE, Edge, Marks, RowLook},
    widgets::{Found, Rows, Watch},
};

/// The panel's header, its least and most width, and the width it opens at.
pub const HEAD: f32 = 48.0;
pub const WIDTH: f32 = 420.0;
const LEAST: f32 = 320.0;
const MOST: f32 = 800.0;
/// The room the conversation keeps beside it.
pub const CHAT_KEPT: f32 = 420.0;
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
    /// A report held back by the throttle is due: the view as it is then
    /// goes, so where a scroll stopped is told.
    report_due: bool,
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
            report_due: false,
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
}

#[derive(Clone, Debug)]
pub enum Event {
    Scrolled(Viewport),
    /// A report the throttle held back fell due.
    Report,
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
    /// Its edge dragged this far since the press, let go, or double
    /// clicked back to the width it opens at.
    Resize(f32),
    Resized,
    ResetWidth,
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Scrolled(viewport) => {
            let Some(aside) = &mut app.aside else {
                return Task::none();
            };
            aside.from_end = viewport.absolute_offset().y;
            aside.height = viewport.bounds().height;
            let width = viewport.bounds().width + 1.0;
            if (width - aside.width).abs() > 1.0 {
                aside.width = width;
                aside.heights.clear();
                let same = aside.messages.clone();
                measure_aside(app, &same, true);
            }
            Task::batch([aside_report(app), app.ask()])
        }
        Event::Report => {
            if let Some(aside) = &mut app.aside {
                aside.report_due = false;
            }
            aside_report(app)
        }
        Event::Measured(heights) => {
            aside_found(app, heights);
            Task::none()
        }
        Event::Earlier => {
            app.client.dispatch(Action::AsideEarlier);
            app.sync()
        }
        Event::Close => {
            app.client.dispatch(Action::CloseAside);
            Task::batch([app.sync(), crate::composer::refocus(app)])
        }
        Event::Expand => {
            app.client.dispatch(Action::ExpandAside);
            Task::batch([app.sync(), crate::composer::refocus(app)])
        }
        Event::More => {
            let Some(room) = app.aside.as_ref().map(|aside| aside.room.clone()) else {
                return Task::none();
            };
            let at = app.pressed.get();
            crate::place_menus::open_thread_more(
                app,
                &room,
                iced::Point::new(at.x - 200.0, at.y + 20.0),
            )
        }
        Event::Send => send_aside(app),
        Event::Attach => attach_aside(),
        Event::CancelReply => {
            app.client.dispatch(Action::CancelAsideCompose);
            Task::batch([app.sync(), operation::focus(COMPOSER)])
        }
        Event::TogglePing => {
            app.client.dispatch(Action::ToggleReplyPing { aside: true });
            Task::batch([app.sync(), operation::focus(COMPOSER)])
        }
        Event::Open(room) => {
            app.client.dispatch(Action::OpenAside(room));
            Task::batch([app.sync(), operation::focus(COMPOSER)])
        }
        Event::Start(item) => {
            app.client.dispatch(Action::NewThread {
                message: Some(item),
                from: ThreadFrom::Message,
            });
            Task::batch([app.sync(), operation::focus(FIRST)])
        }
        Event::Name(name) => {
            app.drafts.name.clone_from(&name);
            app.client.dispatch(Action::WriteNewThread {
                name: Some(name),
                body: None,
            });
            app.sync()
        }
        Event::Private(private) => {
            app.client.dispatch(Action::SetThreadPrivate(private));
            app.sync()
        }
        Event::Create => {
            app.client.dispatch(Action::WriteNewThread {
                name: Some(app.drafts.name.clone()),
                body: Some(app.drafts.first.text()),
            });
            app.client.dispatch(Action::CreateThread);
            app.sync()
        }
        Event::CloseForm => {
            app.client.dispatch(Action::CloseNewThread);
            Task::batch([app.sync(), crate::composer::refocus(app)])
        }
        Event::Leave => {
            app.client.dispatch(Action::LeaveNewThread);
            Task::batch([app.sync(), crate::composer::refocus(app)])
        }
        Event::Stay => {
            app.client.dispatch(Action::StayInNewThread);
            Task::batch([app.sync(), operation::focus(FIRST)])
        }
        Event::Jump(item) => {
            app.client.dispatch(Action::Jump(item));
            app.sync()
        }
        Event::Resize(by) => {
            let from = *app.aside_drag.get_or_insert(aside_width(app));
            app.layout.aside_width = Some(aside_room(app, from - by));
            Task::none()
        }
        Event::Resized => {
            app.aside_drag = None;
            crate::layout::store(app.client.root(), "aside_width", aside_width(app));
            Task::none()
        }
        Event::ResetWidth => {
            app.aside_drag = None;
            app.layout.aside_width = Some(WIDTH);
            crate::layout::store(app.client.root(), "aside_width", WIDTH);
            Task::none()
        }
    }
}

/// `width` within what the panel may take: 320 to 800 px, and no more
/// than leaves the conversation 420 px.
fn aside_room(app: &App, width: f32) -> f32 {
    let most =
        (app.window.width - CHAT_KEPT - crate::conversation::chat_left(app)).clamp(LEAST, MOST);
    width.clamp(LEAST, most)
}

/// Takes in the core's thread beside the channel and New Thread form:
/// a new thread starts from nothing, one gone lets go of its rows.
pub fn thread_sync(app: &mut App) -> Task<Message> {
    let state = app.client.state();
    let Some(core) = state.aside.as_ref() else {
        // Once, as the thread closes: a new editor each sync takes the
        // font system's lock the renderer shapes with.
        if app.aside.take().is_some() || !app.drafts.aside.is_empty() {
            app.drafts.aside.reset("");
        }
        app.drafts.aside_known.clear();
        return new_thread_sync(app);
    };
    let width = aside_width(app);
    if app
        .aside
        .as_ref()
        .is_none_or(|aside| aside.room != core.room)
    {
        app.aside = Some(Box::new(Aside::new(core.room.clone(), width)));
        // Keys typed anywhere go to it from when it opens.
        app.composer.writing_last = Writing::Aside;
        let draft = state.aside_draft().to_owned();
        app.drafts.aside.reset(&draft);
        app.drafts.aside_known = draft;
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
    if draft != app.drafts.aside_known {
        if draft != app.drafts.aside.text() {
            app.drafts.aside.reset(&draft);
        }
        app.drafts.aside_known = draft;
    }
    let Some(aside) = &mut app.aside else {
        return Task::none();
    };
    let edge_changed =
        (aside.ready, aside.loading, aside.start, aside.failed) != (ready, loading, start, failed);
    aside.name = name;
    aside.ready = ready;
    aside.loading = loading;
    aside.start = start;
    aside.failed = failed;
    if edge_changed || !Arc::ptr_eq(&aside.messages, &messages) {
        let old = std::mem::replace(&mut aside.messages, messages);
        app.seen_aside = seen;
        measure_aside(app, &old, edge_changed);
    }
    Task::batch([aside_earlier(app), new_thread_sync(app)])
}

/// The New Thread form's first message follows the core's.
fn new_thread_sync(app: &mut App) -> Task<Message> {
    match app.client.state().new_thread() {
        Some(form) => {
            if form.body != app.drafts.first_known {
                if form.body != app.drafts.first.text() {
                    app.drafts.first.reset(&form.body);
                }
                app.drafts.first_known = form.body.clone();
            }
            if form.name != app.drafts.name {
                app.drafts.name = form.name.clone();
            }
            let item = form.message.clone();
            if app.drafts.starter.as_ref().map(|found| &found.message.item) != item.as_ref() {
                let message = item.and_then(|item| app.client.state().message(&item).cloned());
                app.drafts.starter =
                    message.map(|message| crate::found::found(app, Arc::new(message)));
            }
        }
        None => {
            if !app.drafts.first_known.is_empty() || !app.drafts.first.is_empty() {
                app.drafts.first.reset("");
                app.drafts.first_known.clear();
            }
            app.drafts.name.clear();
            app.drafts.starter = None;
        }
    }
    Task::none()
}

/// Works out the thread's rows as the timeline's are (`measured_rows`).
fn measure_aside(app: &mut App, old: &Messages, edge_changed: bool) {
    let Some(aside) = &app.aside else {
        return;
    };
    let marks = Marks {
        seen: app.seen_aside.as_deref(),
        first_unread: None,
    };
    let (looks, heights) = app.measured_rows(crate::sync::Measuring {
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
    if let Some(aside) = &mut app.aside {
        aside.looks = looks;
        aside.heights = heights;
        aside.place_rows();
    }
}

/// Heights the thread's rows found at their last layout.
fn aside_found(app: &mut App, heights: Vec<(u64, f32)>) {
    let Some(aside) = &mut app.aside else {
        return;
    };
    let (first, last) = aside.in_view(MARGIN);
    let mut changed = false;
    for (row, height) in heights {
        if let Some(index) = (first..last).find(|&index| key(&aside.messages[index].item) == row)
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
pub fn take_aside_found(app: &mut App) {
    let Some(found) = app
        .aside
        .as_ref()
        .map(|aside| std::mem::take(&mut *aside.found.borrow_mut()))
    else {
        return;
    };
    if !found.is_empty() {
        aside_found(app, found);
    }
}

/// Earlier messages load as the list nears its top, a page at a time,
/// until the first; after a failure they wait for Retry.
fn aside_earlier(app: &mut App) -> Task<Message> {
    let busy = app.client.state().busy;
    let Some(aside) = &app.aside else {
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
        app.client.dispatch(Action::AsideEarlier);
    }
    Task::none()
}

/// Tells the core which of the thread's rows are in view, at most every
/// 50 ms, so their pictures load and they're read.
fn aside_report(app: &mut App) -> Task<Message> {
    let Some(aside) = &mut app.aside else {
        return Task::none();
    };
    if aside.messages.is_empty() {
        return aside_earlier(app);
    }
    // At most one every REPORT_EVERY, the last after the throttle.
    if let Some(since) = aside.reported.map(|reported| reported.elapsed())
        && since < REPORT_EVERY
    {
        if aside.report_due {
            return aside_earlier(app);
        }
        aside.report_due = true;
        return Task::batch([
            aside_earlier(app),
            crate::after(REPORT_EVERY - since, Message::Thread(Event::Report)),
        ]);
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
    app.client.dispatch(Action::VisibleMessages {
        room,
        items,
        at_end: false,
    });
    Task::batch([aside_earlier(app), app.sync()])
}

/// The rows of the thread in view, for the pictures they want.
pub fn aside_rows_in_view(app: &App) -> std::ops::Range<usize> {
    match &app.aside {
        Some(aside) => {
            let (first, last) = aside.in_view(MARGIN);
            first..last
        }
        None => 0..0,
    }
}

fn send_aside(app: &mut App) -> Task<Message> {
    app.client
        .dispatch(Action::SetAsideDraft(app.drafts.aside.text()));
    app.client.dispatch(Action::SendAside);
    app.composer.suggest.clear(&mut app.client);
    let to_end = operation::scroll_to(
        LIST,
        scrollable::AbsoluteOffset {
            x: None,
            y: Some(0.0),
        },
    );
    Task::batch([app.sync(), to_end])
}

fn attach_aside() -> Task<Message> {
    // Cancelled, the field takes the keys again all the same.
    let chosen = |files| Message::Write(Write::Uploads(crate::uploads::Event::ChosenAside(files)));
    crate::uploads::pick_files(chosen, chosen(Vec::new()))
}

/// Whether the panel beside the conversation is the thread's or the
/// New Thread form.
pub fn aside_shown(app: &App) -> bool {
    (app.aside.is_some() || app.client.state().new_thread().is_some())
        && !crate::search::search_shown(app)
}

/// The panel's width, as dragged and kept.
pub fn aside_width(app: &App) -> f32 {
    aside_room(app, app.layout.aside_width.unwrap_or(WIDTH))
}

/// The dim under the question before leaving.
const DIM: iced::Color = iced::Color::from_rgba(0.0, 0.0, 0.0, 0xa0 as f32 / 255.0);

/// A composer of a panel beside the conversation, as the thread and forms
/// have: a reply's bar over it, the files waiting, the Attach button, the
/// field (one of [`Writing`]'s), and Send.
pub struct SideComposer<'a> {
    which: Writing,
    placeholder: String,
    enabled: bool,
    reply: Option<crate::composer::ReplyBar<'a>>,
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

    /// The reply's bar over it.
    pub fn reply(mut self, reply: crate::composer::ReplyBar<'a>) -> Self {
        self.reply = Some(reply);
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

    pub fn view(self, cx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
        let theme = cx.look.theme;
        let draft = crate::composer::writing(cx, self.which);
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
        .suggesting(crate::suggest::suggesting(cx, self.which));
        // Ctrl+V takes files and pictures where the thread takes them, as
        // the channel's composer does.
        let field = if self.which == Writing::Aside && cx.state.aside_can_attach() {
            field.pastes()
        } else {
            field
        };
        let lift = |button: Element<'static, Message>| -> Element<'a, Message> {
            container(button).padding(Padding::ZERO.bottom(8)).into()
        };
        let mut line = row![].spacing(6).align_y(Alignment::End);
        if let Some(attach) = self.attach {
            line = line.push(lift(crate::composer::composer_button(
                cx,
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
            line = line.push(lift(crate::composer::composer_button(
                cx,
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
        let frame = Popover::new(
            frame,
            crate::suggest::suggestions_view(cx, self.which),
            Side::Above,
        );
        let mut compose = column![];
        if let Some(reply) = self.reply {
            compose = compose.push(crate::composer::reply_bar_for(
                cx,
                reply,
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

/// Whether this account can write in the thread beside.
pub fn aside_writes(ctx: crate::ctx::Ctx<'_>) -> bool {
    ctx.lanes
        .aside
        .is_some_and(|aside| ctx.state.can_write_in(&aside.room))
}

/// The thread's history edge, at its first row.
pub fn aside_edge<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    match ctx.lanes.aside {
        Some(aside) => {
            crate::timeline::view::edge_line(ctx, &aside.edge(), Message::Thread(Event::Earlier))
        }
        None => Space::new().into(),
    }
}

/// The thread beside the channel, or the New Thread form.
pub fn aside_view<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    if let Some(form) = new_thread_view(ctx) {
        return Some(panel(ctx, form));
    }
    let aside = ctx.lanes.aside?;
    let theme = ctx.look.theme;
    let head = row![
        ctx.look.glyph(Icon::Thread, 18, theme.muted),
        container(
            elide(aside.name.as_str())
                .size(15)
                .font(BOLD)
                .color(theme.text)
        )
        .width(Length::Fill),
        ctx.look
            .icon_button(Icon::Ellipsis, "More", Message::Thread(Event::More))
            .below(),
        ctx.look
            .icon_button(
                Icon::Maximize,
                "Open in full view",
                Message::Thread(Event::Expand)
            )
            .below(),
        ctx.look
            .icon_button(Icon::X, "Close thread", Message::Thread(Event::Close))
            .below(),
    ];
    let body = column![
        panel_head(ctx, head),
        aside_list(ctx, aside),
        aside_composer(ctx, aside),
    ];
    Some(panel(ctx, body.into()))
}

/// The panel's frame: the edge on its left, its ground.
fn panel<'a>(ctx: crate::ctx::Ctx<'a>, body: Element<'a, Message>) -> Element<'a, Message> {
    let ground = ctx.look.theme.chat;
    let theme = ctx.look.theme;
    // Its edge drags it wider or narrower.
    let edge = crate::widgets::Splitter::new(
        theme.edge,
        theme.line,
        theme.faint,
        |by| Message::Thread(Event::Resize(by)),
        Message::Thread(Event::Resized),
        Message::Thread(Event::ResetWidth),
    );
    row![
        edge,
        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| crate::style::fill(ground)),
    ]
    .width(ctx.aside_width)
    .height(Length::Fill)
    .into()
}

/// A panel's header: its row, and the line under it.
pub fn panel_head<'a>(
    ctx: crate::ctx::Ctx<'a>,
    head: iced::widget::Row<'a, Message>,
) -> Element<'a, Message> {
    column![
        container(head.spacing(8).align_y(Alignment::Center))
            .padding(Padding::ZERO.left(16).right(10))
            .height(HEAD - 1.0)
            .align_y(Alignment::Center),
        bar(ctx.look.theme.line, Length::Fill, 1),
    ]
    .into()
}

fn aside_list<'a>(ctx: crate::ctx::Ctx<'a>, aside: &'a Aside) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    if aside.messages.is_empty() {
        // Still loading: rows the shape of messages, as the channel's.
        if !aside.ready {
            return crate::timeline::view::messages_skeleton(ctx);
        }
        let words = text(format!("This is the start of the {} thread.", aside.name))
            .size(14)
            .color(theme.muted)
            .center();
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
            crate::timeline::view::timeline_row(ctx, ASIDE + index),
        );
    }
    let list = scroll_view(column![Space::new().height(lead), rows])
        .id(LIST)
        .anchor_bottom()
        .on_scroll(|viewport| Message::Thread(Event::Scrolled(viewport)))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(scrollbar(theme));
    Watch::new(
        list,
        ctx.lanes
            .timeline
            .selection
            .is_some()
            .then_some(Message::SelectClear),
        ctx.lanes.timeline.selecting.then_some(Message::SelectEnd),
    )
    .into()
}

/// The thread's composer: the reply's bar over it, the files waiting,
/// Attach, the field and Send.
fn aside_composer<'a>(ctx: crate::ctx::Ctx<'a>, aside: &'a Aside) -> Element<'a, Message> {
    let state = ctx.state;
    let writes = state.can_write_in(&aside.room);
    let reply = crate::composer::reply_bar_info(state.aside_compose());
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
    if let Some(reply) = reply {
        composer = composer.reply(reply);
    }
    if let Some(tray) = crate::uploads::aside_tray(ctx) {
        composer = composer.tray(tray);
    }
    composer = composer.attach(
        state
            .aside_can_attach()
            .then_some(Message::Thread(Event::Attach)),
    );
    composer.view(ctx)
}

/// Discord's New Thread form, beside the channel.
fn new_thread_view<'a>(ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let form = ctx.state.new_thread()?;
    let theme = ctx.look.theme;
    let head = row![
        ctx.look.glyph(Icon::Thread, 18, theme.muted),
        container(elide("New Thread").size(15).font(BOLD).color(theme.text)).width(Length::Fill),
        ctx.look
            .icon_button(Icon::X, "Close", Message::Thread(Event::CloseForm))
            .below(),
    ];
    let mut fields = column![].spacing(16).padding(16).width(Length::Fill);
    if let Some(starter) = &ctx.drafts.starter {
        fields = fields.push(crate::found::found_view(
            ctx,
            starter,
            Message::Thread(Event::Jump(starter.message.item.clone())),
        ));
    }
    let placeholder = if form.default_name.is_empty() {
        "New Thread"
    } else {
        form.default_name.as_str()
    };
    let mut name = form::entry(theme, NAME, placeholder, &ctx.drafts.name);
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
            .view(ctx.look),
        );
    }
    if let Some(error) = &form.error {
        fields = fields.push(form::error(theme, error.as_str()));
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
        !form.starting && !ctx.drafts.first.is_empty(),
    )
    .view(ctx);
    let body: Element<'a, Message> = column![
        panel_head(ctx, head),
        scroll_view(fields)
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
            leave_question(
                ctx,
                Message::Thread(Event::Stay),
                Message::Thread(Event::Leave)
            )
        ]
        .into(),
    )
}

/// Discord's question before leaving what was written: Back to editing,
/// or Exit, over the panel or form it's about.
pub fn leave_question<'a>(
    ctx: crate::ctx::Ctx<'a>,
    stay: Message,
    leave: Message,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
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
    .style(move |_| crate::style::outlined(ground, edge, 10.0));
    mouse_area(
        container(card)
            .padding([0, 16])
            .center(Length::Fill)
            .style(|_| crate::style::fill(DIM)),
    )
    .on_press(Message::Nothing)
    .into()
}
