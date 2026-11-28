//! The header's popovers: the open conversation's Pinned Messages, a
//! channel's Threads, Select Friends to add to a DM, and a channel's topic
//! in full under its line. Each opens under its button, its right edge
//! level with the button's, and closes on Escape, a click outside, or what
//! it was for (a pin jumped to, a thread opened). The core says which is
//! open; the window keeps only where it opens from and the rows it built,
//! which go as it closes.

use crate::widgets::scroll_view;
use std::hash::{Hash, Hasher};

use iced::{
    Alignment, Element, Length, Padding, Task,
    widget::{Space, button, column, container, operation, row, stack, text},
};
use yapfast_core::{
    application::{Action, AppState},
    chat::{ListedThread, Permission, Presence},
};

use crate::{
    app::{App, Message},
    controls::{self, Kind, bar, placed},
    ctx::{Ctx, Cx, Focus, Request},
    elide::elide,
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    found::Found,
    markdown::{self, Body},
    pictures::{Face, Icon},
    style::{hex, raised_shadow, scrollbar},
    theme::rgb,
};

/// A popover's head, taller with a line under its heading.
const HEAD: f32 = 48.0;
const HEAD_DETAIL: f32 = 64.0;
/// The frame's corners.
const RADIUS: f32 = 10.0;
/// The gap under the header, and kept to the window's edges.
const GAP: f32 = 6.0;
const MARGIN: f32 = 8.0;
const PINS_WIDTH: f32 = 420.0;
const PINS_MOST: f32 = 560.0;
const PEOPLE_WIDTH: f32 = 440.0;
const PEOPLE_MOST: f32 = 520.0;
const TOPIC_WIDTH: f32 = 440.0;
const TOPIC_MOST: f32 = 400.0;
/// The topic's text and its line.
const TOPIC_TEXT: f32 = 15.0;
const TOPIC_LINE: f32 = 20.0;
/// A thread's card in its slot, and a section's heading over its first.
const THREAD_SLOT: f32 = 80.0;
const THREAD_CARD: f32 = 72.0;
/// How near the end a scroll asks for older threads.
const NEAR_END: f32 = 200.0;
const PERSON_ROW: f32 = 44.0;
/// The threads list's search field.
pub const THREADS_SEARCH: &str = "threads-search";
pub const PEOPLE_FIELD: &str = "add-people-field";

/// What the header's popovers keep while they show.
#[derive(Default)]
pub struct Popovers {
    /// The right edge of the button the open one came from.
    pub right: f32,
    pins: Vec<Found>,
    threads: Vec<ThreadRow>,
    threads_shown: Option<u64>,
    pub threads_query: String,
    people: Vec<PickRow>,
    people_shown: Option<u64>,
    pub people_query: String,
    topic: Option<TopicShown>,
}

impl Popovers {
    /// Lets go of what's laid out in the old theme's inks: the pins' cards
    /// and the topic.
    pub fn forget_laid(&mut self) {
        self.pins.clear();
        self.topic = None;
    }

    /// What's done in the popovers; one opening opens with its right edge
    /// at `right`, the topic under `under`. A thread's More is the root's
    /// (`header_popovers::popover_event`).
    pub fn update(
        &mut self,
        event: Event,
        right: f32,
        under: iced::Rectangle,
        cx: &mut Cx<'_>,
    ) -> Task<Message> {
        // Focus moves only as a popover opens or closes: focusing a field
        // puts its caret at the end, so a keystroke or a scroll mustn't.
        let shown = |popovers: &Self, state: &AppState| {
            (state.thread_list().is_some(), popovers.is_open(state))
        };
        let before = shown(self, cx.state());
        match event {
            Event::Pins => {
                let open = pins_open(cx.state());
                self.close(cx);
                if !open {
                    self.right = right;
                    cx.dispatch(Action::ShowPins(true));
                }
            }
            Event::Threads => {
                let open = cx.state().thread_list().is_some();
                self.close(cx);
                if !open {
                    self.right = right;
                    self.threads_query.clear();
                    cx.dispatch(Action::ShowThreads(true));
                }
            }
            Event::AddPeople => {
                let open = people_open(cx.state());
                self.close(cx);
                if !open {
                    self.right = right;
                    self.people_query.clear();
                    cx.dispatch(Action::OpenAddPeople);
                }
            }
            Event::Topic => {
                let open = self.topic.is_some();
                self.close(cx);
                let state = cx.state();
                if !open
                    && let (Some(room), Some(topic)) =
                        (state.selected.clone(), state.topic.as_ref())
                {
                    let body = markdown::parse(
                        &topic.body,
                        &topic.mentions,
                        rgb(cx.theme.mention),
                        "topic",
                        |_| true,
                    );
                    self.topic = Some(TopicShown { room, under, body });
                }
            }
            Event::Close => self.close(cx),
            Event::Jump(item) => {
                self.close(cx);
                cx.dispatch(Action::Jump(item));
            }
            Event::OpenThread(room) => {
                // Opening first, as it may use what the list found.
                cx.dispatch(if cx.modifiers.shift() {
                    Action::OpenAside(room)
                } else {
                    Action::Open(room)
                });
                cx.dispatch(Action::ShowThreads(false));
            }
            Event::ThreadsQuery(query) => {
                let cleared = query.is_empty() && !self.threads_query.is_empty();
                self.threads_query = query;
                if cleared {
                    cx.dispatch(Action::SearchThreads(String::new()));
                }
            }
            Event::SearchThreads => {
                let query = self.threads_query.clone();
                cx.dispatch(Action::SearchThreads(query));
            }
            Event::ThreadsScrolled(top, seen, whole) => {
                let more = cx
                    .state()
                    .thread_list()
                    .is_some_and(|list| list.more && list.search.is_none() && !list.loading);
                // A scroll that asks for nothing more changes nothing to
                // take in.
                if !(more && top > 0.0 && top + seen >= whole - NEAR_END) {
                    return Task::none();
                }
                cx.dispatch(Action::MoreThreads);
            }
            Event::CreateThread => {
                cx.dispatch(Action::ShowThreads(false));
                cx.dispatch(crate::thread::start_from(
                    yapfast_core::application::ThreadFrom::List,
                ));
                return operation::focus(crate::thread::FIRST);
            }
            Event::PeopleQuery(query) => {
                self.people_query.clone_from(&query);
                cx.dispatch(Action::AddPeopleQuery(query));
            }
            Event::Choose(id) => cx.dispatch(Action::ChoosePerson(id)),
            Event::Add => cx.dispatch(Action::AddPeople),
            Event::ThreadMore => {}
        }
        cx.touch();
        if shown(self, cx.state()) != before {
            cx.request(Request::Focus(Focus::Default));
        }
        Task::none()
    }

    /// Closes whichever popover is open.
    pub fn close(&mut self, cx: &mut Cx<'_>) {
        self.topic = None;
        if pins_open(cx.state()) {
            cx.dispatch(Action::ShowPins(false));
        }
        if cx.state().thread_list().is_some() {
            cx.dispatch(Action::ShowThreads(false));
        }
        if people_open(cx.state()) {
            cx.dispatch(Action::CloseAddPeople);
        }
    }

    /// Whether a popover is open, for Escape.
    pub fn is_open(&self, state: &AppState) -> bool {
        pins_open(state)
            || people_open(state)
            || state.thread_list().is_some()
            || self.topic.is_some()
    }
}

impl Popovers {
    fn threads_sync(&mut self, app: &yapfast_core::application::Application) {
        let Some(list) = app.state.thread_list() else {
            if self.threads_shown.take().is_some() {
                self.threads = Vec::new();
            }
            return;
        };
        let sections = app.thread_sections().unwrap_or_default();
        let found = list.search.as_ref().map(|search| &search.found);
        let now = yapfast_core::clock::now_ms();
        let mut hasher = std::hash::DefaultHasher::new();
        (&sections, found, now / 60_000).hash(&mut hasher);
        let shown = hasher.finish();
        if self.threads_shown == Some(shown) {
            return;
        }
        self.threads_shown = Some(shown);
        let mut rows = Vec::new();
        if let Some(Some(Ok(found))) = found {
            rows.extend(found.iter().map(|thread| thread_row(thread, now)));
        } else {
            let headed = [
                (&sections.joined, "joined thread", "joined threads"),
                (
                    &sections.other,
                    "other active thread",
                    "other active threads",
                ),
            ];
            for (threads, one, many) in headed {
                let heading = match threads.len() {
                    1 => format!("1 {one}"),
                    count => format!("{count} {many}"),
                };
                section(&mut rows, threads, heading.to_uppercase(), now);
            }
            section(&mut rows, &sections.older, "OLDER THREADS".into(), now);
        }
        self.threads = rows;
    }

    fn people_sync(&mut self, state: &AppState) {
        let picker = state
            .add_people
            .as_ref()
            .filter(|picker| state.selected.as_ref() == Some(&picker.room));
        let Some(picker) = picker else {
            if self.people_shown.take().is_some() {
                self.people = Vec::new();
            }
            return;
        };
        // Friends' presence changes often; the rows are built again only
        // when what they show changed.
        let friends = state.addable_friends();
        let mut hasher = std::hash::DefaultHasher::new();
        (&friends, &picker.chosen).hash(&mut hasher);
        let shown = Some(hasher.finish());
        if shown == self.people_shown {
            return;
        }
        let rows = friends
            .into_iter()
            .map(|friend| PickRow {
                id: friend.id.clone(),
                name: friend.name.clone(),
                avatar: friend.avatar.clone(),
                presence: friend.presence,
                chosen: picker.chosen.contains(&friend.id),
            })
            .collect();
        self.people = rows;
        self.people_shown = shown;
    }
}

/// Whether the open conversation's pins show.
fn pins_open(state: &AppState) -> bool {
    state
        .pins
        .as_ref()
        .is_some_and(|pins| state.selected.as_ref() == Some(&pins.room))
}

/// Whether Select Friends shows for the open conversation.
fn people_open(state: &AppState) -> bool {
    state
        .add_people
        .as_ref()
        .is_some_and(|picker| state.selected.as_ref() == Some(&picker.room))
}

/// The topic in full, while it shows: whose, where its line was, and its
/// Markdown with its line breaks, laid out once as it opens.
struct TopicShown {
    room: String,
    under: iced::Rectangle,
    body: Body,
}

/// A thread as its list shows it: an active one with its newest message,
/// an archived one with who started it, as Discord's client does.
struct ThreadRow {
    room: String,
    heading: Option<String>,
    name: String,
    private: bool,
    archived: bool,
    /// Who wrote its newest message, or started it: name, avatar, colour.
    person: Option<(String, String, Option<u32>)>,
    lead: &'static str,
    text: String,
    time: String,
    members: u32,
    avatars: Vec<String>,
}

/// A friend who can be added.
struct PickRow {
    id: String,
    name: String,
    avatar: Option<String>,
    presence: Option<Presence>,
    chosen: bool,
}

#[derive(Clone, Debug)]
pub enum Event {
    Pins,
    Threads,
    AddPeople,
    /// The header's line of a channel's topic was clicked.
    Topic,
    Close,
    Jump(String),
    /// A listed thread: in its place, or with Shift beside the channel.
    OpenThread(String),
    ThreadsQuery(String),
    SearchThreads,
    ThreadsScrolled(f32, f32, f32),
    CreateThread,
    PeopleQuery(String),
    Choose(String),
    Add,
    /// The header's More, for a thread open in its place.
    ThreadMore,
}

/// The header's button the last press was on, if it was on one.
pub fn header_button(app: &App) -> Option<iced::Rectangle> {
    let button = app.header_spot.get();
    button.contains(app.pressed.get()).then_some(button)
}

/// The popovers': each opens from the header's button pressed, its right
/// edge, or from the press if opened from elsewhere; a thread's More opens
/// with the menus.
pub fn popover_event(app: &mut App, event: Event) -> Task<Message> {
    if let Event::ThreadMore = event {
        let Some(room) = app.client.state().selected.clone() else {
            return Task::none();
        };
        return crate::place_menus::open_thread_more(app, &room, app.pressed.get());
    }
    let right = header_button(app).map_or(
        app.pressed.get().x + crate::header::BUTTON / 2.0,
        |button| button.x + button.width,
    );
    let under = app.call.spot.get();
    let mut cx = crate::ctx::cx!(app);
    app.popovers.update(event, right, under, &mut cx)
}

/// Whether a popover is open, for Escape.
pub fn popover_open(app: &App) -> bool {
    app.popovers.is_open(app.client.state())
}

/// Builds the open popover's rows when what they show changed, and lets
/// go of a closed one's.
pub fn popovers_sync(app: &mut App) {
    let pinned: &[yapfast_core::chat::ChatMessage] = match pins_open(app.client.state())
        .then(|| app.client.state().pins.as_ref())
        .flatten()
        .and_then(|pins| pins.messages.as_ref())
    {
        Some(Ok(pinned)) => pinned,
        _ => &[],
    };
    let kept = app
        .popovers
        .pins
        .iter()
        .zip(pinned)
        // The same message as it reads now: a pin edited since is laid
        // out again.
        .take_while(|(found, message)| *found.message == **message)
        .count();
    if kept < app.popovers.pins.len() || kept < pinned.len() {
        let fresh: Vec<_> = pinned[kept..]
            .iter()
            .map(|message| std::sync::Arc::new(message.clone()))
            .collect();
        app.popovers.pins.truncate(kept);
        for message in fresh {
            let found = crate::found::found(app, message);
            app.popovers.pins.push(found);
        }
    }
    app.popovers.threads_sync(&app.client.app);
    app.popovers.people_sync(app.client.state());
    // The topic goes with its room, or with the topic.
    let state = app.client.state();
    if app.popovers.topic.as_ref().is_some_and(|topic| {
        state.selected.as_deref() != Some(topic.room.as_str()) || state.topic.is_none()
    }) {
        app.popovers.topic = None;
    }
}

/// The pictures the open popover and the search's results show: found
/// messages' authors, threads' people, friends to pick.
pub fn popover_pictures(app: &App) -> Vec<&str> {
    let found = app.popovers.pins.iter().map(|found| &found.message);
    let results = crate::search::searching_messages(app);
    let mut urls: Vec<&str> = found
        .chain(results)
        .filter_map(|message| message.avatar.as_deref())
        .collect();
    for entry in &app.popovers.threads {
        urls.extend(entry.person.as_ref().map(|(_, avatar, _)| avatar.as_str()));
        urls.extend(entry.avatars.iter().map(String::as_str));
    }
    urls.extend(
        app.popovers
            .people
            .iter()
            .filter_map(|person| person.avatar.as_deref()),
    );
    urls
}

/// The open popover, placed under its button.
pub fn view<'a>(
    popovers: &'a Popovers,
    name: &'a str,
    cx: Ctx<'a>,
) -> Option<Element<'a, Message>> {
    View {
        s: popovers,
        name,
        cx,
    }
    .layer()
}

/// The popovers' view: their state, the open room's name and the view
/// context.
#[derive(Clone, Copy)]
struct View<'a> {
    s: &'a Popovers,
    name: &'a str,
    cx: Ctx<'a>,
}

impl<'a> View<'a> {
    fn layer(self) -> Option<Element<'a, Message>> {
        if let Some(topic) = &self.s.topic {
            return Some(self.topic_layer(topic));
        }
        let (panel, width, most) = if pins_open(self.cx.state) {
            (self.pins_view(), PINS_WIDTH, PINS_MOST)
        } else if self.cx.state.thread_list().is_some() {
            // As Discord's: 35% of the window wide, from 480 to 600 px.
            let width = (self.cx.look.window.width * 0.35).clamp(480.0, 600.0);
            (self.threads_view(), width, self.threads_height())
        } else if people_open(self.cx.state) {
            (self.people_view(), PEOPLE_WIDTH, PEOPLE_MOST)
        } else {
            return None;
        };
        let width = width.min(self.cx.look.window.width - 2.0 * MARGIN);
        let top = crate::header::HEIGHT + GAP;
        let x = (self.s.right - width)
            .min(self.cx.look.window.width - width - MARGIN)
            .max(MARGIN);
        let height = most.min(self.cx.look.window.height - top - 2.0 * MARGIN);
        let panel = container(panel).width(width).max_height(height);
        let panel = if self.cx.state.thread_list().is_some() || people_open(self.cx.state) {
            panel.height(height)
        } else {
            panel
        };
        Some(
            stack![
                controls::outside(Message::Popover(Event::Close), false),
                placed(crate::widgets::motion::grow(panel), x, top),
            ]
            .into(),
        )
    }

    /// A popover's frame: its icon, heading and detail over a line, Close,
    /// and what it holds; as tall as what it holds, or as its place with
    /// `fill`.
    pub fn frame(
        self,
        fill_place: bool,
        icon: Icon,
        heading: Element<'a, Message>,
        detail: Option<String>,
        end: Option<Element<'a, Message>>,
        body: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let radius = theme.surface_radius("popout", RADIUS);
        let tall = detail.is_some();
        let mut words = column![heading].spacing(2);
        if let Some(detail) = detail {
            words = words.push(elide(detail).size(13).color(theme.muted));
        }
        let mut head = row![
            self.cx.look.glyph(icon, 20, theme.secondary),
            container(words).width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if let Some(end) = end {
            head = head.push(end);
        }
        head = head.push(
            self.cx
                .look
                .icon_button(Icon::X, "Close", Message::Popover(Event::Close))
                .below(),
        );
        let panel_ground = theme.panel;
        let head = column![
            container(head)
                .padding(Padding::ZERO.left(16).right(10))
                .height(if tall { HEAD_DETAIL } else { HEAD } - 1.0)
                .align_y(Alignment::Center)
                .style(move |_| {
                    crate::style::rounded(
                        panel_ground,
                        crate::style::corners([radius, radius, 0.0, 0.0]),
                    )
                }),
            bar(theme.edge, Length::Fill, 1),
        ];
        let surface = yapfast_core::themes::package::art::ArtSurface::Popout;
        let decorated = self.cx.themes.decorates(&theme.id, surface)
            || theme.package_paint(surface.ground()).is_some();
        let (ground, edge) = (
            if decorated {
                iced::Color::TRANSPARENT
            } else {
                theme.sidebar
            },
            theme.edge,
        );
        let panel = container(column![head, body])
            .height(if fill_place {
                Length::Fill
            } else {
                Length::Shrink
            })
            .clip(true)
            .style(move |_| container::Style {
                shadow: theme.surface_shadow("popout", raised_shadow()),
                ..crate::style::rounded(ground, radius)
            });
        let panel = crate::theme_art::surface(panel, self.cx, surface);
        iced::widget::opaque(controls::edged(panel, edge, radius))
    }

    fn heading(self, words: &'a str) -> Element<'a, Message> {
        elide(words)
            .size(16)
            .font(BOLD)
            .color(self.cx.look.theme.text)
            .into()
    }

    /// A note in a popover's place: while it loads, why it failed.
    fn note(self, icon: Option<Icon>, note: String) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut note_column = column![].spacing(12).align_x(Alignment::Center);
        if let Some(icon) = icon {
            note_column = note_column.push(self.cx.look.glyph(icon, 40, theme.faint));
        }
        note_column = note_column.push(text(note).size(14).color(theme.muted).center());
        container(note_column)
            .padding(24)
            .width(Length::Fill)
            .height(172)
            .center_x(Length::Fill)
            .center_y(172)
            .into()
    }

    /// Discord's answer to a channel with no pins: a big pin and a line,
    /// and its tip on a lighter band under them.
    fn no_pins(self, note: String) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let place = container(
            column![
                self.cx.look.glyph(Icon::Pin, 80, theme.faint),
                text(note).size(16).font(BOLD).color(theme.text).center(),
            ]
            .spacing(20)
            .align_x(Alignment::Center),
        )
        .padding(32)
        .width(Length::Fill)
        .height(228)
        .center_x(Length::Fill)
        .center_y(228);
        let ground = if theme.bonfire {
            crate::style::hex(0x313137)
        } else {
            theme.panel
        };
        let tip = container(
            column![
                text(crate::i18n::tr!(self.cx.i18n, "Protip:"))
                    .size(14)
                    .font(BOLD)
                    .color(crate::style::hex(0x5eb479)),
                text(crate::i18n::tr!(self.cx.i18n, "Users with the \"Pin Messages\" permission can pin a message from its context menu."))
                    .size(14)
                    .color(theme.text)
                    .center(),
            ]
            .spacing(2)
            .align_x(Alignment::Center),
        )
        .padding(Padding {
            top: 16.0,
            right: 16.0,
            bottom: 17.0,
            left: 16.0,
        })
        .width(Length::Fill)
        .height(103)
        .center_x(Length::Fill)
        .center_y(103)
        .style(move |_| container::Style {
            background: Some(ground.into()),
            ..Default::default()
        });
        column![place, tip].into()
    }

    fn pins_view(self) -> Element<'a, Message> {
        let state = self.cx.state;
        let pins = state.pins.as_ref().map(|pins| &pins.messages);
        let note = match pins {
            Some(None) => Some("Loading pinned messages...".to_owned()),
            Some(Some(Err(error))) => Some(error.clone()),
            Some(Some(Ok(pinned))) if pinned.is_empty() => Some(
                if state.selected_room().is_some_and(|room| room.direct) {
                    "This conversation doesn't have any pinned messages yet."
                } else {
                    "This channel doesn't have any pinned messages yet."
                }
                .to_owned(),
            ),
            _ => None,
        };
        let none = matches!(pins, Some(Some(Ok(pinned))) if pinned.is_empty());
        let body = match note {
            Some(note) if none => self.no_pins(note),
            Some(note) => self.note(Some(Icon::Pin), note),
            None => {
                let cards = self.s.pins.iter().map(|found| {
                    crate::found::found_view(
                        self.cx,
                        found,
                        Message::Popover(Event::Jump(found.message.item.clone())),
                    )
                });
                scroll_view(column(cards).spacing(8).padding(12))
                    .style(scrollbar(self.cx.look.theme))
                    .into()
            }
        };
        self.frame(
            false,
            Icon::Pin,
            self.heading("Pinned Messages"),
            None,
            None,
            body,
        )
    }

    /// The topic in full, under its line, its left edge level with the
    /// line's; its text scrolls past 400 px or the window's room.
    fn topic_layer(self, topic: &'a TopicShown) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let width = TOPIC_WIDTH.min(self.cx.look.window.width - 2.0 * MARGIN);
        let x = topic
            .under
            .x
            .min(self.cx.look.window.width - width - MARGIN)
            .max(MARGIN);
        let top = topic.under.y + topic.under.height + GAP;
        let most = TOPIC_MOST.min(self.cx.look.window.height - top - 16.0);
        let look = crate::markdown::Look {
            size: TOPIC_TEXT,
            line_height: TOPIC_LINE,
            ink: theme.text,
        };
        let body =
            scroll_view(container(self.cx.body_view(&topic.body, usize::MAX, 0, look)).padding(16))
                .style(scrollbar(theme));
        let name = self.name;
        let panel = self.frame(
            false,
            Icon::Hash,
            self.heading(name),
            None,
            None,
            body.into(),
        );
        stack![
            controls::outside(Message::Popover(Event::Close), false),
            placed(
                crate::widgets::motion::grow(container(panel).width(width).max_height(most)),
                x,
                top,
            ),
        ]
        .into()
    }

    fn threads_view(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let app = &self.cx.app;
        let state = self.cx.state;
        let Some(list) = state.thread_list() else {
            return Space::new().into();
        };
        let searchable = app.may(&list.room, Permission::ReadHistory);
        let (public, private) = app.thread_starts(&list.room);
        let can_create = public || private;
        let mut heading = row![
            text(crate::i18n::tr!(self.cx.i18n, "Threads"))
                .size(16)
                .font(BOLD)
                .color(theme.text)
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if searchable {
            heading = heading
                .push(bar(theme.line, 1, 24))
                .push(crate::search::search_field(
                    self.cx,
                    THREADS_SEARCH,
                    "Search for Thread Name",
                    &self.s.threads_query,
                    |query| Message::Popover(Event::ThreadsQuery(query)),
                    Message::Popover(Event::SearchThreads),
                    Message::Popover(Event::ThreadsQuery(String::new())),
                ));
        }
        let create = can_create.then(|| {
            controls::action(
                theme,
                crate::i18n::tr!(self.cx.i18n, "Create"),
                Kind::Primary,
                Some(Message::Popover(Event::CreateThread)),
            )
            .small()
            .into()
        });
        let sections_empty = self.s.threads.is_empty();
        let found = list.search.as_ref().map(|search| &search.found);
        let note = match found {
            Some(None) => Some("Searching...".to_owned()),
            Some(Some(Err(error))) => Some(error.clone()),
            Some(Some(Ok(_))) => None,
            None if sections_empty && list.loading => Some("Loading threads...".into()),
            None if sections_empty => list.error.clone(),
            None => None,
        };
        let body: Element<'a, Message> = if let Some(note) = note {
            self.note(None, note)
        } else if sections_empty {
            let mut empty = column![
                self.cx.look.glyph(Icon::Thread, 48, theme.faint),
                text(crate::i18n::tr!(self.cx.i18n, "There are no threads."))
                    .size(17)
                    .font(BOLD)
                    .color(theme.text),
                text(crate::i18n::tr!(
                    self.cx.i18n,
                    "Stay focused on a conversation with a thread - a temporary text channel."
                ))
                .size(14)
                .color(theme.muted)
                .center(),
            ]
            .spacing(8)
            .align_x(Alignment::Center);
            if can_create {
                empty = empty.push(
                    container(controls::action(
                        theme,
                        crate::i18n::tr!(self.cx.i18n, "Create Thread"),
                        Kind::Primary,
                        Some(Message::Popover(Event::CreateThread)),
                    ))
                    .padding(Padding::ZERO.top(8)),
                );
            }
            container(empty).padding(32).center_x(Length::Fill).into()
        } else {
            let mut rows = column![];
            for (index, entry) in self.s.threads.iter().enumerate() {
                if let Some(heading) = &entry.heading {
                    let above = if index == 0 { 8.0 } else { 40.0 };
                    rows = rows.push(
                        container(
                            elide(heading.as_str())
                                .size(12)
                                .font(BOLD)
                                .color(theme.muted),
                        )
                        .padding(Padding::ZERO.top(above).left(16).right(16))
                        .height(above + 24.0)
                        .align_y(Alignment::End),
                    );
                } else if index == 0 {
                    rows = rows.push(Space::new().height(16));
                }
                rows = rows.push(self.thread_card(entry));
            }
            if list.loading {
                rows = rows.push(
                    container(
                        text(crate::i18n::tr!(self.cx.i18n, "Loading..."))
                            .size(13)
                            .color(theme.muted),
                    )
                    .padding(8)
                    .center_x(Length::Fill),
                );
            }
            scroll_view(rows)
                .on_scroll(|viewport| {
                    Message::Popover(Event::ThreadsScrolled(
                        viewport.absolute_offset().y,
                        viewport.bounds().height,
                        viewport.content_bounds().height,
                    ))
                })
                .style(scrollbar(theme))
                .into()
        };
        self.frame(true, Icon::Thread, heading.into(), None, create, body)
    }

    /// The threads list's height, as Discord's: as tall as its rows, from
    /// 400 px to 80% of the window.
    fn threads_height(self) -> f32 {
        let rows = &self.s.threads;
        let headings = rows.iter().filter(|row| row.heading.is_some()).count();
        let loading = self.cx.state.thread_list().is_some_and(|list| list.loading);
        let fit = HEAD
            + 1.0
            + if headings > 0 {
                32.0 + (headings - 1) as f32 * 64.0
            } else {
                16.0
            }
            + rows.len() as f32 * THREAD_SLOT
            + if loading { 34.0 } else { 0.0 };
        fit.max(400.0).min(self.cx.look.window.height * 0.8)
    }

    /// A thread's card in the list.
    fn thread_card(self, entry: &'a ThreadRow) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut name = row![].spacing(6).align_y(Alignment::Center);
        if entry.private {
            name = name.push(self.cx.look.glyph(Icon::Lock, 14, theme.muted));
        }
        name = name.push(
            elide(entry.name.as_str())
                .size(15)
                .font(SEMIBOLD)
                .color(theme.text),
        );
        let mut line = row![].align_y(Alignment::Center);
        if let Some((person, avatar, _)) = &entry.person {
            line = line.push(self.cx.face(Face::new(
                (!avatar.is_empty()).then_some(avatar.as_str()),
                person,
                16.0,
            )));
            line = line.push(Space::new().width(6));
        } else if entry.archived {
            line = line
                .push(self.cx.look.glyph(Icon::Thread, 14, theme.muted))
                .push(Space::new().width(6));
        }
        if !entry.lead.is_empty() {
            line = line.push(text(entry.lead).size(13).color(theme.muted));
        }
        let mut said = row![].align_y(Alignment::Center);
        if let Some((person, _, color)) = &entry.person {
            said = said.push(
                elide(person.as_str())
                    .size(13)
                    .font(SEMIBOLD)
                    .color(color.map_or(theme.secondary, hex))
                    .shrink(),
            );
        }
        let mut words = String::new();
        if !entry.text.is_empty() {
            if entry.person.is_some() {
                words.push_str(": ");
            }
            words.push_str(&entry.text);
        }
        if entry.person.is_some() || !entry.text.is_empty() {
            words.push_str(" • ");
        }
        words.push_str(&entry.time);
        said = said.push(elide(words).size(13).color(theme.muted));
        line = line.push(container(said).width(Length::Fill));
        let mut content = row![column![name, line].spacing(6).width(Length::Fill)]
            .spacing(16)
            .align_y(Alignment::Center);
        if entry.members > 0 {
            let mut faces = stack![
                Space::new()
                    .width(12.0 * entry.avatars.len() as f32 + 8.0)
                    .height(20)
            ];
            for (index, avatar) in entry.avatars.iter().enumerate() {
                faces = faces.push(placed(
                    self.cx.face(Face::new(Some(avatar.as_str()), "", 18.0)),
                    index as f32 * 12.0 + 1.0,
                    1.0,
                ));
            }
            content = content.push(
                row![
                    faces,
                    text(entry.members.to_string())
                        .size(13)
                        .font(SEMIBOLD)
                        .color(theme.muted)
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        let (rest, lit, edge, edge_lit) = (theme.hover, theme.raised, theme.line, theme.active);
        let card = button(
            container(content)
                .padding([0, 16])
                .height(THREAD_CARD)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::Popover(Event::OpenThread(entry.room.clone())))
        .style(crate::controls::outlined_lit_style(
            rest, lit, edge, edge_lit, 8.0,
        ));
        container(card)
            .padding(
                Padding::ZERO
                    .left(16)
                    .right(16)
                    .bottom(THREAD_SLOT - THREAD_CARD),
            )
            .into()
    }

    fn people_view(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let state = self.cx.state;
        let Some(picker) = state.add_people.as_ref() else {
            return Space::new().into();
        };
        let room = state.group_room();
        let chosen = picker.chosen.len();
        let left = room.saturating_sub(chosen);
        let detail = match left {
            0 => "This group has no room for more.".to_owned(),
            1 => "You can add 1 more friend.".to_owned(),
            left => format!("You can add {left} more friends."),
        };
        let field = controls::field(
            theme,
            PEOPLE_FIELD,
            "Type the name of a friend",
            &self.s.people_query,
            false,
            true,
        )
        .size(15)
        .padding([8, 10])
        .on_input(|query| Message::Popover(Event::PeopleQuery(query)));
        let field = if chosen > 0 && !picker.adding {
            field.on_submit(Message::Popover(Event::Add))
        } else {
            field
        };
        let mut list = column![].spacing(2);
        for person in &self.s.people {
            let can_choose = !picker.adding && (person.chosen || chosen < room);
            list = list.push(self.pick_row(person, can_choose));
        }
        if self.s.people.is_empty() {
            let words = if self.s.people_query.is_empty() {
                "All your friends are here already."
            } else {
                "No friends match."
            };
            list = list.push(container(text(words).size(14).color(theme.secondary)).padding(8));
        }
        let mut body = column![
            field,
            scroll_view(list)
                .height(Length::Fill)
                .style(scrollbar(theme)),
        ]
        .spacing(8)
        .padding(12);
        if let Some(error) = &picker.error {
            body = body.push(crate::form::error(theme, error.as_str()));
        }
        let label = if picker.adding {
            "Adding..."
        } else if state.one_to_one().is_some() {
            "Create Group DM"
        } else {
            "Add to Group"
        };
        body = body.push(
            controls::action(
                theme,
                label,
                Kind::Primary,
                (chosen > 0 && !picker.adding).then_some(Message::Popover(Event::Add)),
            )
            .wide(38.0),
        );
        self.frame(
            true,
            Icon::UserPlus,
            self.heading("Select Friends"),
            Some(detail),
            None,
            body.height(Length::Fill).into(),
        )
    }

    /// A friend to choose: avatar with presence, name, and a box ticked
    /// once chosen.
    fn pick_row(self, person: &'a PickRow, can_choose: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let face = self.cx.face(
            Face::new(person.avatar.as_deref(), &person.name, 32.0).dot(
                person
                    .presence
                    .filter(|presence| *presence != Presence::Offline)
                    .map(|presence| {
                        (
                            presence,
                            crate::theme::rgb(theme.sidebar),
                            iced::Point::new(20.0, 20.0),
                        )
                    }),
            ),
        );
        let content = row![
            face,
            container(elide(person.name.as_str()).size(16).font(MEDIUM).color(
                if can_choose || person.chosen {
                    theme.text
                } else {
                    theme.faint
                }
            ))
            .width(Length::Fill),
            self.cx.look.checkbox(person.chosen, false),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let row = button(
            container(content)
                .padding([0, 8])
                .height(PERSON_ROW)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .width(Length::Fill)
        .on_press_maybe(can_choose.then(|| Message::Popover(Event::Choose(person.id.clone()))))
        .style(controls::lit_style(None, theme.hover, 6.0));
        row.into()
    }
}

/// A section of a list of threads: its heading over the first.
fn section(rows: &mut Vec<ThreadRow>, threads: &[ListedThread], heading: String, now: u64) {
    for (index, thread) in threads.iter().enumerate() {
        let mut row = thread_row(thread, now);
        if index == 0 {
            row.heading = Some(heading.clone());
        }
        rows.push(row);
    }
}

fn thread_row(thread: &ListedThread, now: u64) -> ThreadRow {
    let ago = ago(thread.active_ms, now);
    let (person, lead, words, time) = match (&thread.recent, &thread.owner, thread.archived) {
        (Some(recent), _, false) => (Some(&recent.author), "", recent.text.as_str(), ago),
        (None, _, false) => (None, "", "No recent messages", ago),
        (_, Some(owner), true) => (Some(owner), "Started by ", "", format!("Last active {ago}")),
        (_, None, true) => (None, "", "", format!("Last active {ago}")),
    };
    ThreadRow {
        room: thread.room.clone(),
        heading: None,
        name: thread.name.clone(),
        private: thread.private,
        archived: thread.archived,
        person: person.map(|person| (person.name.clone(), person.avatar.clone(), person.color)),
        lead,
        text: words.to_owned(),
        time,
        members: thread.members,
        avatars: thread.member_avatars.clone(),
    }
}

/// How long ago, as a thread's row says it.
pub fn ago(at_ms: u64, now_ms: u64) -> String {
    let seconds = now_ms.saturating_sub(at_ms) / 1000;
    match seconds {
        seconds if seconds < 60 => format!("{seconds}s ago"),
        seconds if seconds < 3600 => format!("{}m ago", seconds / 60),
        seconds if seconds < 86400 => format!("{}h ago", seconds / 3600),
        seconds if seconds <= 30 * 86400 => format!("{}d ago", seconds / 86400),
        seconds if seconds < 365 * 86400 => ">30d ago".into(),
        _ => ">1y ago".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::ago;

    #[test]
    fn a_threads_time_reads_as_discords() {
        let now = 400 * 86_400_000;
        assert_eq!(ago(now - 5_000, now), "5s ago");
        assert_eq!(ago(now - 120_000, now), "2m ago");
        assert_eq!(ago(now - 3 * 3_600_000, now), "3h ago");
        assert_eq!(ago(now - 30 * 86_400_000, now), "30d ago");
        assert_eq!(ago(now - 31 * 86_400_000, now), ">30d ago");
        assert_eq!(ago(0, now), ">1y ago");
    }
}
