//! The title bar Yapfast draws in place of the desktop's, with its Discord
//! look: Back and Forward, where Yapfast is in the middle, Inbox and Help,
//! then minimize, maximize and close. Pressing and moving it moves the
//! window, a double click maximizes it, and its edges resize it. Where the
//! desktop draws the title bar, or none (a tiling compositor), the bar still
//! shows under it, as Discord's client always has one, without the window's
//! buttons, and neither moves nor resizes the window. The Inbox
//! opens its popout under it: Unreads lists the channels and conversations
//! with unread messages, and Mentions the messages that ping this account,
//! filterable by what kind of ping they hold, each jumping to its message.
//!
//! Whether Yapfast draws it is the desktop's way (`frame.rs`) until
//! Appearance's "Use the system title bar" chooses. A window is made with
//! the frame or without; changing it makes the window again.

use crate::widgets::scroll_view;
use std::sync::Arc;

use iced::{
    Alignment, Element, Length, Point, Size, Task,
    mouse::Interaction,
    widget::{Space, button, column, container, mouse_area, row, stack, text, tooltip},
    window,
};
use yapfast_core::{application::DmScreen, chat::ChatMessage};

use crate::{
    app::{App, Message},
    controls::{self, lit_style},
    elide::elide,
    fonts::BOLD,
    pictures::Icon,
    style::{faded, scrollbar},
    widgets::{Hover, Rows},
};

/// The bar's height, which moves the window's content down.
pub const HEIGHT: f32 = 32.0;

/// A caption button's width, and its glyph's size: Discord's frame draws
/// them as 1 px strokes in a 10 px square.
const CAPTION: f32 = 36.0;
const CAPTION_GLYPH: u32 = 10;

/// The inbox popout's width and tallest height, as Discord's.
const INBOX_WIDTH: f32 = 440.0;
const INBOX_MOST: f32 = 560.0;
const INBOX_ROW: f32 = 56.0;
/// Round the inbox's rows.
const INBOX_PAD: f32 = 4.0;

/// Where Discord's help opens.
const HELP: &str = "https://support.discord.com";

/// How far the pointer moves with the button down before the window moves,
/// so a double click still maximizes it.
const DRAG_AFTER: f32 = 3.0;

/// The edges' and corners' widths that resize a window without a frame.
const EDGE: f32 = 5.0;
const CORNER: f32 = 8.0;

/// Whether Yapfast draws the title bar, as `system` (Appearance's choice)
/// or else the desktop's way has it: on Linux where the desktop frames no
/// windows, and on Windows, where Discord's look draws its own.
pub fn own(system: Option<bool>) -> bool {
    offered() && !system.unwrap_or_else(system_by_default)
}

/// Where Yapfast can draw its own title bar.
pub fn offered() -> bool {
    cfg!(any(target_os = "linux", windows))
}

/// Whether the desktop draws the title bar unless the person chose: on a
/// desktop not known to give windows one, it's the desktop's, which may be
/// none at all, as a tiling compositor's. The tests lay the window out
/// without a bar wherever they run, unless one asks for it.
fn system_by_default() -> bool {
    cfg!(test) || (!cfg!(windows) && !crate::frame::desktop_frames())
}

#[derive(Clone, Debug)]
pub enum Event {
    Back,
    Forward,
    Inbox,
    InboxTab(Tab),
    InboxFilter(MentionFilter),
    InboxOpen(String),
    /// A ping's own message opened: its room shows that message.
    InboxOpenMessage {
        room: String,
        item: String,
    },
    InboxScrolled(crate::sync::Scroll),
    CloseInbox,
    Help,
    /// The bar pressed, the pointer moved on it, the button let go.
    Press,
    Moved(Point),
    Release,
    Maximize,
    Minimize,
    Close,
    /// An edge or corner pressed, by the direction it resizes.
    Resize(window::Direction),
    /// Whether the window is maximized now, and full screen (as a kiosk
    /// makes it), read at the size `Shell::sized` counts so.
    State(bool, bool, u32),
    /// What the desktop says; `None` (Wayland can't tell) leaves what
    /// Yapfast knows, such as its own Minimize.
    Minimized(Option<bool>),
}

/// The inbox's tabs, as Discord's first two.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Unreads,
    Mentions,
}

impl Tab {
    /// As `layout.json` keeps it.
    fn key(self) -> &'static str {
        match self {
            Tab::Unreads => "unreads",
            Tab::Mentions => "mentions",
        }
    }

    /// The tab a kept key names, or Unreads.
    fn of(key: Option<&str>) -> Tab {
        match key {
            Some("mentions") => Tab::Mentions,
            _ => Tab::Unreads,
        }
    }
}

/// The inbox popout while it's open: its tab, its rows, and the Mentions
/// tab's filter, message cards and rooms with mentions but no message kept.
#[derive(Default)]
pub struct Inbox {
    pub tab: Tab,
    pub rows: Vec<InboxRow>,
    pub mentions: Vec<MentionRow>,
    pub more: Vec<InboxRow>,
    pub filter: MentionFilter,
    /// Its list's viewport: only the rows near it are built.
    pub scroll: crate::sync::Scroll,
}

/// One line of the inbox: a channel or conversation with something waiting.
#[derive(Clone, Debug, PartialEq)]
pub struct InboxRow {
    pub id: String,
    /// As the row shows it (a channel's with its `#`), and the name its
    /// avatar's initial and tint are taken from.
    pub name: String,
    pub avatar_name: String,
    pub place: String,
    pub picture: Option<String>,
    pub mentions: u64,
}

/// What kind of ping a message holds for this account: a direct mention or
/// a reply to its message, one of its roles, or @everyone or @here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MentionKind {
    Direct,
    Role,
    Everyone,
}

impl MentionKind {
    /// As its message's card names it.
    pub fn tag(self) -> &'static str {
        match self {
            MentionKind::Direct => "direct",
            MentionKind::Role => "role",
            MentionKind::Everyone => "everyone",
        }
    }
}

/// A message that pings this account, kept while the window runs: the room
/// it came in and the message itself.
#[derive(Clone, Debug)]
pub struct MentionHit {
    pub room: String,
    pub message: Arc<ChatMessage>,
}

/// How many pings the inbox keeps, newest first.
pub const MAX_MENTIONS: usize = 100;

/// The Mentions tab's filter: every ping, or only one kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MentionFilter {
    #[default]
    All,
    Direct,
    Role,
    Everyone,
}

impl MentionFilter {
    /// Whether a ping of this kind shows under the filter.
    pub fn shows(self, kind: MentionKind) -> bool {
        match self {
            MentionFilter::All => true,
            MentionFilter::Direct => kind == MentionKind::Direct,
            MentionFilter::Role => kind == MentionKind::Role,
            MentionFilter::Everyone => kind == MentionKind::Everyone,
        }
    }

    /// The filter's chips, in the order the tab shows them.
    pub fn chips() -> [MentionFilter; 4] {
        [
            MentionFilter::All,
            MentionFilter::Direct,
            MentionFilter::Role,
            MentionFilter::Everyone,
        ]
    }

    /// As its chip names it.
    pub fn label(self) -> &'static str {
        match self {
            MentionFilter::All => "All",
            MentionFilter::Direct => "Direct",
            MentionFilter::Role => "@roles",
            MentionFilter::Everyone => "@everyone",
        }
    }

    /// As `layout.json` keeps it.
    fn key(self) -> &'static str {
        match self {
            MentionFilter::All => "all",
            MentionFilter::Direct => "direct",
            MentionFilter::Role => "role",
            MentionFilter::Everyone => "everyone",
        }
    }

    /// The filter a kept key names, or All.
    fn of(key: Option<&str>) -> MentionFilter {
        match key {
            Some("direct") => MentionFilter::Direct,
            Some("role") => MentionFilter::Role,
            Some("everyone") => MentionFilter::Everyone,
            _ => MentionFilter::All,
        }
    }

    /// As its empty state names it.
    pub fn empty(self) -> &'static str {
        match self {
            MentionFilter::All => "Nothing mentions you right now.",
            MentionFilter::Direct => "No direct mentions right now.",
            MentionFilter::Role => "No role mentions right now.",
            MentionFilter::Everyone => "No @everyone mentions right now.",
        }
    }
}

/// What kind of ping a message holds for this account, if any: @everyone
/// or @here first, then a role, then a direct mention or a reply. A
/// `#channel` mention names no one, so it counts as none.
pub fn mention_kind(message: &ChatMessage) -> Option<MentionKind> {
    use yapfast_core::mention::Target;
    let mut role = false;
    let mut direct = message.replies_to_me;
    for mentioned in &message.mentions {
        match mentioned.target {
            Target::Everyone => return Some(MentionKind::Everyone),
            Target::Role(_) => role = true,
            Target::User(_) => direct = true,
            Target::Channel(_) => {}
        }
    }
    if role {
        Some(MentionKind::Role)
    } else if direct || message.mentions_me {
        Some(MentionKind::Direct)
    } else {
        None
    }
}

/// One ping the Mentions tab shows: its message's card, and where it came
/// from. The card is laid out once, when the tab's rows are made.
pub struct MentionRow {
    pub room: String,
    pub message: Arc<ChatMessage>,
    /// The channel, with its `#`, and its server.
    pub name: String,
    pub place: String,
    pub kind: MentionKind,
    pub card: crate::found::Found,
}

impl MentionRow {
    /// Whether it's the same ping showing the same message.
    fn same(&self, room: &str, message: &ChatMessage) -> bool {
        self.room == room && self.message.as_ref() == message
    }
}

/// The press that may move the window, and where it was.
#[derive(Default)]
pub struct Drag {
    pub armed: Option<Option<Point>>,
}

/// Alt+Left and Alt+Right, and the mouse's back and forward buttons, go
/// back and forward through the conversations visited, as in Discord and
/// whichever bar the window has.
pub fn history_keys() -> iced::Subscription<Message> {
    use iced::keyboard::{self, Key, Modifiers, key::Named};
    iced::event::listen_with(|event, _, _| {
        let back = match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(named @ (Named::ArrowLeft | Named::ArrowRight)),
                modifiers: Modifiers::ALT,
                repeat: false,
                ..
            }) => named == Named::ArrowLeft,
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(button)) => match button {
                iced::mouse::Button::Back => true,
                iced::mouse::Button::Forward => false,
                _ => return None,
            },
            _ => return None,
        };
        Some(Message::Bar(if back {
            Event::Back
        } else {
            Event::Forward
        }))
    })
}

/// The room under the title bar is what the window's content lays out
/// in (`Below`), so it's the window's size to everything under it.
pub fn fit_window(app: &mut App) {
    let frame = app.shell.frame;
    let room = Size::new(frame.width, (frame.height - bar_height(app)).max(0.0));
    if room != app.window {
        app.window = room;
    }
}

/// The room the title bar takes at the window's top: Yapfast's own, or
/// Discord's bar under the desktop's.
pub fn bar_height(app: &App) -> f32 {
    if (app.shell.own_title_bar || app.shell.bar_under_frame) && !full_screen(app) {
        HEIGHT
    } else {
        0.0
    }
}

/// Something full screen covers the window and the bar, or the window
/// is full screen itself.
fn full_screen(app: &App) -> bool {
    app.shell.full_screen || app.playback.clip_full || crate::call::stage::full_shown(app)
}

/// Asks the window whether it's maximized or full screen, as its size
/// changed.
pub fn read_window_state(app: &App) -> Task<Message> {
    let Some(id) = app.shell.window else {
        return Task::none();
    };
    let sized = app.shell.sized;
    let state = window::is_maximized(id).then(move |maximized| {
        window::mode(id).map(move |mode| {
            Message::Bar(Event::State(
                maximized,
                mode == window::Mode::Fullscreen,
                sized,
            ))
        })
    });
    let minimized = window::is_minimized(id).map(|min| Message::Bar(Event::Minimized(min)));
    Task::batch([state, minimized])
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    let id = app.shell.window;
    match event {
        Event::Back | Event::Forward
            if !crate::signin::workspace(app) || app.win.settings.is_open() =>
        {
            Task::none()
        }
        Event::Back | Event::Forward => {
            app.client.dispatch(match event {
                Event::Back => yapfast_core::application::Action::NavigateBack,
                _ => yapfast_core::application::Action::NavigateForward,
            });
            app.sync()
        }
        Event::Inbox => {
            app.inbox = match app.inbox.take() {
                Some(_) => None,
                None => Some(Inbox {
                    tab: Tab::of(app.layout.inbox_tab.as_deref()),
                    filter: MentionFilter::of(app.layout.mention_filter.as_deref()),
                    scroll: crate::sync::Scroll::new(INBOX_MOST),
                    ..Inbox::default()
                }),
            };
            inbox_sync(app);
            Task::none()
        }
        Event::InboxTab(tab) => {
            if let Some(inbox) = &mut app.inbox {
                inbox.tab = tab;
            }
            app.layout.inbox_tab = Some(tab.key().into());
            crate::layout::store(app.client.root(), "inbox_tab", tab.key());
            inbox_sync(app);
            Task::none()
        }
        Event::InboxFilter(filter) => {
            if let Some(inbox) = &mut app.inbox {
                inbox.filter = filter;
            }
            app.layout.mention_filter = Some(filter.key().into());
            crate::layout::store(app.client.root(), "mention_filter", filter.key());
            inbox_sync(app);
            Task::none()
        }
        Event::InboxOpen(room) => {
            app.inbox = None;
            app.client
                .dispatch(yapfast_core::application::Action::SwitchTo(room));
            app.sync()
        }
        Event::InboxOpenMessage { room, item } => {
            app.inbox = None;
            app.client
                .dispatch(yapfast_core::application::Action::JumpInRoom { room, item });
            app.sync()
        }
        Event::InboxScrolled(scroll) => {
            if let Some(inbox) = &mut app.inbox {
                inbox.scroll = scroll;
            }
            Task::none()
        }
        Event::CloseInbox => {
            app.inbox = None;
            Task::none()
        }
        Event::Help => {
            crate::browser::open(HELP);
            Task::none()
        }
        Event::Press => {
            app.bar_drag.armed = Some(None);
            Task::none()
        }
        Event::Moved(at) => {
            let Some(armed) = &mut app.bar_drag.armed else {
                return Task::none();
            };
            let Some(from) = *armed else {
                *armed = Some(at);
                return Task::none();
            };
            if (at.x - from.x).abs() > DRAG_AFTER || (at.y - from.y).abs() > DRAG_AFTER {
                // The desktop takes the pointer for the move, and the
                // window may not hear the button let go.
                app.bar_drag.armed = None;
                return id.map_or_else(Task::none, window::drag);
            }
            Task::none()
        }
        Event::Release => {
            app.bar_drag.armed = None;
            Task::none()
        }
        Event::Maximize => {
            app.bar_drag.armed = None;
            id.map_or_else(Task::none, window::toggle_maximize)
        }
        Event::Minimize => {
            app.shell.minimized = true;
            id.map_or_else(Task::none, |id| window::minimize(id, true))
        }
        Event::Minimized(minimized) => {
            if let Some(minimized) = minimized {
                app.shell.minimized = minimized;
            }
            Task::none()
        }
        // As the desktop's close button: to the tray, or quit.
        Event::Close => app.close_requested(),
        Event::Resize(direction) => {
            id.map_or_else(Task::none, |id| window::drag_resize(id, direction))
        }
        Event::State(maximized, full, sized) => {
            // Read before the window took the size it has now, as one
            // asked while it opened and answered once it maximized: it
            // would keep the maximized size as the one to go back to. The
            // read that size asked for follows.
            if sized != app.shell.sized {
                return Task::none();
            }
            app.shell.maximized = maximized;
            app.shell.full_screen = full;
            app.keep_placement();
            Task::none()
        }
    }
}

/// Appearance's switch: the window is made again with the desktop's
/// frame or without it, where it was.
pub fn choose_title_bar(app: &mut App, system: bool) -> Task<Message> {
    let own = own(Some(system));
    if own == app.shell.own_title_bar {
        return Task::none();
    }
    app.shell.own_title_bar = own;
    app.inbox = None;
    if app.shell.window.is_none() {
        return Task::none();
    }
    // iced could take the frame off the window as it is, but where the
    // desktop draws none winit draws its own, and a maximized window
    // taking it or losing it grows or shrinks past the screen.
    // Settings, which the switch is in, stay open as they are.
    let closed = app.drop_window_keeping(crate::app::Keep { settings: true });
    let opened = app.open_window();
    Task::batch([closed, opened])
}

/// The inbox's rows for its tab, from what the client already knows:
/// the channels and conversations with unread messages, muted ones only
/// for a mention, and the pings the open rooms hold.
pub fn mentions_sync(app: &mut App) -> bool {
    // What the open rooms hold, and what the startup fetch brought that
    // isn't kept yet, without holding the state's borrow.
    let (fresh, fetched_new): (Vec<(String, Arc<ChatMessage>)>, usize) = {
        let state = app.client.state();
        let mut live = Vec::new();
        if let Some(room) = state.selected.clone() {
            live.extend(
                state
                    .messages
                    .iter()
                    .filter(|message| message.mentions_me || message.replies_to_me)
                    .map(|message| (room.clone(), Arc::clone(message))),
            );
        }
        if let Some(aside) = &state.aside {
            live.extend(
                aside
                    .messages
                    .iter()
                    .filter(|message| message.mentions_me || message.replies_to_me)
                    .map(|message| (aside.room.clone(), Arc::clone(message))),
            );
        }
        let fetched: Vec<(String, Arc<ChatMessage>)> = state
            .recent_mentions
            .iter()
            .filter(|(_, message)| {
                !app.mention_hits
                    .iter()
                    .any(|hit| hit.message.item == message.item)
            })
            .map(|(room, message)| (room.clone(), Arc::new(message.clone())))
            .collect();
        let fetched_new = fetched.len();
        (fetched.into_iter().chain(live).collect(), fetched_new)
    };
    if fetched_new > 0 {
        tracing::info!(fetched_new, "kept fetched pings");
    }
    let mut changed = false;
    for (room, message) in fresh {
        let at = app
            .mention_hits
            .iter()
            .position(|hit| hit.message.item == message.item);
        match at {
            Some(0)
                if app.mention_hits[0].room == room
                    && Arc::ptr_eq(&app.mention_hits[0].message, &message) => {}
            Some(index) => {
                app.mention_hits.remove(index);
                app.mention_hits.push_front(MentionHit { room, message });
                changed = true;
            }
            None => {
                app.mention_hits.push_front(MentionHit { room, message });
                changed = true;
            }
        }
    }
    // Newest first, by when each was sent: a ping a newly opened channel
    // brings sorts under the newer ones rather than jumping the queue.
    let before: Vec<String> = app
        .mention_hits
        .iter()
        .map(|hit| hit.message.item.clone())
        .collect();
    app.mention_hits.make_contiguous().sort_by(|a, b| {
        b.message
            .timestamp
            .cmp(&a.message.timestamp)
            .then_with(|| mention_order(&b.message.item).cmp(&mention_order(&a.message.item)))
    });
    if app.mention_hits.len() > MAX_MENTIONS {
        app.mention_hits.truncate(MAX_MENTIONS);
    }
    let after: Vec<String> = app
        .mention_hits
        .iter()
        .map(|hit| hit.message.item.clone())
        .collect();
    changed = changed || before != after;
    changed
}

/// Where a kept ping falls, newest first: its message's snowflake.
fn mention_order(item: &str) -> u64 {
    item.split_once(':')
        .and_then(|(_, id)| id.parse().ok())
        .unwrap_or_default()
}

/// How many of the kept pings the Mentions tab shows.
const MENTION_ROWS: usize = 30;

/// The inbox's rows for its tab, from what the client already knows:
/// the channels and conversations with unread messages, muted ones only
/// for a mention, or those that mention this account.
pub fn inbox_sync(app: &mut App) {
    if app.inbox.is_none() {
        return;
    }
    let (tab, filter) = {
        let inbox = app.inbox.as_ref().expect("open");
        (inbox.tab, inbox.filter)
    };
    let state = app.client.state();
    let mentions_only = tab == Tab::Mentions;
    // Each channel's server, found once.
    let places: std::collections::HashMap<&str, &str> = state
        .servers
        .iter()
        .flat_map(|server| {
            server
                .channels
                .iter()
                .map(|channel| (channel.as_str(), server.name.as_str()))
        })
        .collect();
    let rooms: std::collections::HashMap<&str, &yapfast_core::chat::ChatRoom> = state
        .rooms
        .iter()
        .map(|room| (room.id.as_str(), room.as_ref()))
        .collect();
    let row_of = |room: &yapfast_core::chat::ChatRoom| InboxRow {
        id: room.id.clone(),
        name: if room.direct {
            room.name.clone()
        } else {
            format!("#{}", room.name)
        },
        avatar_name: room.name.clone(),
        place: places
            .get(room.id.as_str())
            .map(|place| (*place).to_owned())
            .unwrap_or_default(),
        picture: room.direct.then(|| room.avatar.clone()).flatten(),
        mentions: room.mentions,
    };
    let rows: Vec<InboxRow> = state
        .rooms
        .iter()
        .filter(|room| !room.request && !room.unlisted)
        .filter(|room| room.mentions > 0 || (!mentions_only && room.unread > 0 && !room.muted))
        .map(|room| row_of(room.as_ref()))
        .collect();
    // The Mentions tab's pings, newest first, under its filter: where each
    // came from, and what kind of ping it holds.
    let mut pinged: Vec<(String, Arc<ChatMessage>, String, String, MentionKind)> = Vec::new();
    if mentions_only {
        for hit in &app.mention_hits {
            let Some(kind) = mention_kind(&hit.message) else {
                continue;
            };
            if !filter.shows(kind) {
                continue;
            }
            let Some(room) = rooms.get(hit.room.as_str()) else {
                continue;
            };
            let row = row_of(room);
            pinged.push((
                hit.room.clone(),
                Arc::clone(&hit.message),
                row.name,
                row.place,
                kind,
            ));
            if pinged.len() >= MENTION_ROWS {
                break;
            }
        }
    }
    // Rooms with mentions but no message kept open below the pings, as
    // rows. What counts as kept doesn't depend on the filter: a filter
    // hides pings, it doesn't turn their rooms back into rows.
    let covered: std::collections::HashSet<&str> = app
        .mention_hits
        .iter()
        .map(|hit| hit.room.as_str())
        .collect();
    let more: Vec<InboxRow> = if mentions_only {
        rows.iter()
            .filter(|row| row.mentions > 0 && !covered.contains(row.id.as_str()))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    // The pings' cards, laid out once here rather than every frame.
    let mut mentions: Vec<MentionRow> = Vec::with_capacity(pinged.len());
    for (room, message, name, place, kind) in pinged {
        let card = crate::found::found(app, Arc::clone(&message));
        mentions.push(MentionRow {
            room,
            message,
            name,
            place,
            kind,
            card,
        });
    }
    let inbox = app.inbox.as_mut().expect("open");
    if rows != inbox.rows {
        inbox.rows = rows;
    }
    let same = mentions.len() == inbox.mentions.len()
        && mentions
            .iter()
            .zip(inbox.mentions.iter())
            .all(|(made, kept)| kept.room == made.room && kept.same(&made.room, &made.message));
    if !same {
        inbox.mentions = mentions;
    }
    if more != inbox.more {
        inbox.more = more;
    }
}

/// The bar across the window's top: Yapfast's frame, or Discord's bar
/// under the desktop's.
pub fn window_bar(app: &App) -> Option<Element<'_, Message>> {
    if bar_height(app) == 0.0 {
        return None;
    }
    let theme = &app.theme;
    let workspace = crate::signin::workspace(app);
    let state = app.client.state();
    let settings_open = app.win.settings.is_open();
    let place = if !workspace {
        "Yapfast".to_owned()
    } else if app.sidebar.header.server.is_none() {
        "Direct Messages".to_owned()
    } else {
        app.sidebar.header.name.clone()
    };
    let ink = if !app.shell.focused {
        theme.faint
    } else if theme.dark {
        theme.nav_open
    } else {
        theme.secondary
    };
    // The Friends page names the bar "Friends", as Discord's does.
    let friends = workspace
        && app.sidebar.header.server.is_none()
        && crate::friends::dm_screen(app) == Some(DmScreen::Friends);
    let place = if friends { "Friends".to_owned() } else { place };
    let middle = row![
        app.look()
            .glyph(
                if friends { Icon::Users } else { Icon::Dm },
                16,
                theme.secondary
            )
            .boxed(16.0),
        elide(place).size(12).font(BOLD).color(ink).shrink()
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let back = workspace && !settings_open && state.can_go_back();
    let forward = workspace && !settings_open && state.can_go_forward();
    let left = row![
        bar_button(
            app,
            Icon::ArrowLeft,
            "Back",
            back.then_some(Event::Back),
            false,
            BarSize::Arrow
        ),
        bar_button(
            app,
            Icon::ArrowRight,
            "Forward",
            forward.then_some(Event::Forward),
            false,
            BarSize::Arrow
        ),
    ]
    .padding(iced::Padding {
        left: 12.0,
        ..iced::Padding::ZERO
    })
    .align_y(Alignment::Center);
    let mut right = row![].align_y(Alignment::Center);
    if workspace {
        right = right.push(
            row![
                bar_button(
                    app,
                    Icon::Inbox,
                    "Inbox",
                    Some(Event::Inbox),
                    app.inbox.is_some(),
                    BarSize::Tool
                ),
                bar_button(
                    app,
                    Icon::Help,
                    "Help",
                    Some(Event::Help),
                    false,
                    BarSize::Tool
                ),
            ]
            .spacing(4)
            .padding(iced::Padding {
                right: 11.5,
                ..iced::Padding::ZERO
            }),
        );
    }
    // The window's buttons, and a ground that moves it, only while Yapfast
    // draws the frame; under the desktop's the bar is Discord's alone.
    let framed = app.shell.own_title_bar;
    if framed {
        let maximized = app.shell.maximized;
        right = right
            .push(caption_button(
                app,
                Icon::CaptionMin,
                Event::Minimize,
                false,
            ))
            .push(caption_button(
                app,
                if maximized {
                    Icon::CaptionRestore
                } else {
                    Icon::CaptionMax
                },
                Event::Maximize,
                false,
            ))
            .push(caption_button(app, Icon::CaptionClose, Event::Close, true));
    } else if workspace {
        right = right.padding(iced::Padding {
            right: 4.0,
            ..iced::Padding::ZERO
        });
    }
    // The bar's ground moves the window; its buttons sit over it. Its
    // moves are messages only once pressed: a pointer passing over it
    // sends none.
    let ground: Element<'_, Message> = if framed {
        let mut ground = mouse_area(container(Space::new()).width(Length::Fill).height(HEIGHT))
            .interaction(Interaction::Idle)
            .on_press(Message::Bar(Event::Press))
            .on_release(Message::Bar(Event::Release))
            .on_double_click(Message::Bar(Event::Maximize));
        if app.bar_drag.armed.is_some() {
            ground = ground.on_move(|at| Message::Bar(Event::Moved(at)));
        }
        ground.into()
    } else {
        container(Space::new())
            .width(Length::Fill)
            .height(HEIGHT)
            .into()
    };
    let rail = theme.rail;
    let bar = stack![
        ground,
        container(middle)
            .center_x(Length::Fill)
            .height(HEIGHT)
            .align_y(Alignment::Center)
            .padding([0, 230]),
        row![left, Space::new().width(Length::Fill), right]
            .height(HEIGHT)
            .align_y(Alignment::Center),
    ];
    Some(
        container(bar)
            .width(Length::Fill)
            .height(HEIGHT)
            .style(move |_| crate::style::fill(rail))
            .into(),
    )
}

/// The bar's buttons: Discord draws the back and forward arrows 16 px in
/// a 24 px box and the inbox and help 18 px in a 32 px one.
#[derive(Clone, Copy)]
enum BarSize {
    Arrow,
    Tool,
}

impl BarSize {
    fn width(self) -> f32 {
        match self {
            BarSize::Arrow => 24.0,
            BarSize::Tool => 32.0,
        }
    }

    fn glyph(self) -> u32 {
        match self {
            BarSize::Arrow => 16,
            BarSize::Tool => 18,
        }
    }
}

/// A button in the bar's own row: a glyph on a 4 px rounded ground, its
/// label in a tip below.
fn bar_button<'a>(
    app: &'a App,
    icon: Icon,
    label: &'static str,
    event: Option<Event>,
    active: bool,
    size: BarSize,
) -> Element<'a, Message> {
    let theme = &app.theme;
    let enabled = event.is_some();
    let rest_ink = if !enabled {
        faded(theme.secondary, 0.3)
    } else if active {
        theme.text
    } else {
        theme.secondary
    };
    let glyph = size.glyph();
    let box_size = glyph as f32;
    let base = app.look().glyph(icon, glyph, rest_ink).boxed(box_size);
    let face: Element<'_, Message> = if enabled && !active {
        Hover::new(
            base,
            app.look().glyph(icon, glyph, theme.text).boxed(box_size),
        )
        .into()
    } else {
        base.into()
    };
    let rest = active.then(|| theme.wash(0.12));
    let lit = if !enabled {
        iced::Color::TRANSPARENT
    } else if active {
        theme.wash(0.12)
    } else {
        theme.wash(0.08)
    };
    let face = button(container(face).center(Length::Fill))
        .padding(0)
        .width(size.width())
        .height(24)
        .on_press_maybe(event.map(Message::Bar))
        .style(lit_style(rest, lit, 4.0));
    // What the click opens opens under the tip, which goes while it's open.
    if active {
        return face.into();
    }
    tooltip(face, controls::tip(theme, label), tooltip::Position::Bottom)
        .gap(6)
        .into()
}

/// The window's minimize, maximize and close: close lights red. The three
/// sit 2 px nearer the edge than their 36 px boxes put them, as Discord's
/// do, so the close box is 2 px narrower and its glyph keeps its place.
fn caption_button(app: &App, icon: Icon, event: Event, close: bool) -> Element<'_, Message> {
    let theme = &app.theme;
    let size = CAPTION_GLYPH as f32;
    let base = app
        .look()
        .glyph(icon, CAPTION_GLYPH, theme.secondary)
        .boxed(size);
    let face: Element<'_, Message> = if close {
        Hover::new(
            base,
            app.look()
                .glyph(icon, CAPTION_GLYPH, theme.on_danger)
                .boxed(size),
        )
        .into()
    } else {
        base.into()
    };
    let lit = if close { theme.danger } else { theme.hover };
    let (width, inset) = if close {
        (CAPTION - 2.0, 2.0)
    } else {
        (CAPTION, 0.0)
    };
    button(container(face).center(Length::Fill).padding(iced::Padding {
        left: inset,
        ..iced::Padding::ZERO
    }))
    .padding(0)
    .width(width)
    .height(HEIGHT)
    .on_press(Message::Bar(event))
    .style(lit_style(None, lit, 0.0))
    .into()
}

/// The edges and corners that resize a window without the desktop's
/// frame, while it isn't maximized: a layer over the window that only
/// they take the pointer in.
pub fn resize_edges(app: &App) -> Option<Element<'_, Message>> {
    if !app.shell.own_title_bar || bar_height(app) == 0.0 || app.shell.maximized {
        return None;
    }
    use window::Direction;
    let grip = |direction: Direction, width: Length, height: Length| -> Element<'_, Message> {
        let cursor = match direction {
            Direction::North | Direction::South => Interaction::ResizingVertically,
            Direction::East | Direction::West => Interaction::ResizingHorizontally,
            Direction::NorthWest | Direction::SouthEast => Interaction::ResizingDiagonallyDown,
            Direction::NorthEast | Direction::SouthWest => Interaction::ResizingDiagonallyUp,
        };
        mouse_area(container(Space::new()).width(width).height(height))
            .interaction(cursor)
            .on_press(Message::Bar(Event::Resize(direction)))
            .into()
    };
    let fill = Length::Fill;
    let top = row![
        grip(
            Direction::NorthWest,
            Length::Fixed(CORNER),
            Length::Fixed(CORNER)
        ),
        column![
            grip(Direction::North, fill, Length::Fixed(EDGE)),
            Space::new().height(CORNER - EDGE)
        ]
        .width(fill),
        grip(
            Direction::NorthEast,
            Length::Fixed(CORNER),
            Length::Fixed(CORNER)
        ),
    ]
    .height(CORNER);
    let sides = row![
        grip(Direction::West, Length::Fixed(EDGE), fill),
        Space::new().width(fill).height(fill),
        grip(Direction::East, Length::Fixed(EDGE), fill),
    ]
    .height(fill);
    let bottom = row![
        grip(
            Direction::SouthWest,
            Length::Fixed(CORNER),
            Length::Fixed(CORNER)
        ),
        column![
            Space::new().height(CORNER - EDGE),
            grip(Direction::South, fill, Length::Fixed(EDGE))
        ]
        .width(fill),
        grip(
            Direction::SouthEast,
            Length::Fixed(CORNER),
            Length::Fixed(CORNER)
        ),
    ]
    .height(CORNER);
    Some(column![top, sides, bottom].into())
}

/// The inbox popout under the bar's Inbox button, as Discord's: 440 px
/// wide, Unreads and Mentions. Unreads lists the rooms with something
/// waiting, a click going there; Mentions its pings' own messages under a
/// filter, a click jumping to the message, and the rooms with mentions but
/// no message kept below.
pub fn inbox_view(app: &App) -> Option<Element<'_, Message>> {
    let inbox = app.inbox.as_ref()?;
    if bar_height(app) == 0.0 || !crate::signin::workspace(app) {
        return None;
    }
    let theme = &app.theme;
    let tab = |label: &'static str, which: Tab, width: f32| -> Element<'_, Message> {
        let selected = inbox.tab == which;
        button(
            container(text(label).size(14).font(BOLD).color(if selected {
                theme.text
            } else {
                theme.secondary
            }))
            .center(Length::Fill),
        )
        .padding(0)
        .width(width)
        .height(32)
        .on_press(Message::Bar(Event::InboxTab(which)))
        .style(lit_style(
            selected.then(|| theme.wash(0.12)),
            theme.wash(if selected { 0.12 } else { 0.06 }),
            4.0,
        ))
        .into()
    };
    let head = row![
        text("Inbox").size(16).font(BOLD).color(theme.text),
        Space::new().width(Length::Fill),
        tab("Unreads", Tab::Unreads, 80.0),
        tab("Mentions", Tab::Mentions, 92.0),
    ]
    .spacing(8)
    .padding([0, 16])
    .height(48)
    .align_y(Alignment::Center);
    let body: Element<'_, Message> = if inbox.tab == Tab::Unreads {
        if inbox.rows.is_empty() {
            empty_inbox(
                theme,
                "Unread messages from your unmuted channels show up here.",
            )
        } else {
            // Only the rows near the view are built: the Unreads can list
            // hundreds.
            let (first, last) = inbox.scroll.range(inbox.rows.len(), INBOX_ROW, INBOX_PAD);
            let mut rows = Rows::new(inbox.rows.len() as f32 * INBOX_ROW + INBOX_PAD * 2.0);
            for (index, entry) in inbox.rows.iter().enumerate().take(last).skip(first) {
                rows.push(
                    crate::app::key(&entry.id),
                    INBOX_PAD + index as f32 * INBOX_ROW,
                    INBOX_ROW,
                    container(inbox_line(app, entry)).padding([0.0, INBOX_PAD]),
                );
            }
            scroll_view(rows)
                .on_scroll(|viewport| {
                    Message::Bar(Event::InboxScrolled(crate::sync::Scroll::of(viewport)))
                })
                .style(scrollbar(theme))
                .height(Length::Fill)
                .into()
        }
    } else {
        // The Mentions tab: the filter strip, always visible so every
        // filter stays reachable, even one that matches nothing, over the
        // pings' cards. The strip hugs its labels; the empty margin is
        // doing design work.
        let mut segments = row![].spacing(0).align_y(Alignment::Center);
        for chip in MentionFilter::chips() {
            let selected = inbox.filter == chip;
            segments = segments.push(
                button(text(chip.label()).size(12).font(BOLD).color(if selected {
                    theme.text
                } else {
                    theme.secondary
                }))
                .padding([3, 8])
                .on_press(Message::Bar(Event::InboxFilter(chip)))
                .style(lit_style(
                    selected.then(|| theme.wash(0.12)),
                    theme.wash(if selected { 0.12 } else { 0.06 }),
                    3.0,
                )),
            );
        }
        let line = theme.line;
        let strip = container(container(segments).padding(2).style(move |_| {
            iced::widget::container::Style {
                border: crate::style::edge(line, 4.0),
                ..Default::default()
            }
        }))
        .padding(iced::Padding {
            top: 8.0,
            left: 16.0,
            right: 16.0,
            bottom: 0.0,
        });
        let list: Element<'_, Message> = if inbox.mentions.is_empty() && inbox.more.is_empty() {
            container(text(inbox.filter.empty()).size(12).color(theme.muted))
                .padding(iced::Padding {
                    top: 12.0,
                    left: 16.0,
                    right: 16.0,
                    bottom: 0.0,
                })
                .into()
        } else {
            let mut list = column![].spacing(8).width(Length::Fill);
            for ping in &inbox.mentions {
                list = list.push(mention_card(app, ping));
            }
            if !inbox.more.is_empty() {
                list = list.push(
                    text("More channels with mentions")
                        .size(12)
                        .color(theme.muted),
                );
                for entry in &inbox.more {
                    list = list.push(inbox_line(app, entry));
                }
            }
            scroll_view(container(list).padding(iced::Padding {
                top: 8.0,
                left: 8.0,
                right: 8.0,
                bottom: 8.0,
            }))
            .on_scroll(|viewport| {
                Message::Bar(Event::InboxScrolled(crate::sync::Scroll::of(viewport)))
            })
            .style(scrollbar(theme))
            .height(Length::Fill)
            .into()
        };
        column![strip, list].into()
    };
    let surface = yapfast_core::themes::package::art::ArtSurface::Popout;
    let decorated =
        app.themes.decorates(&theme.id, surface) || theme.package_paint(surface.ground()).is_some();
    let (raised, line) = (
        if decorated {
            iced::Color::TRANSPARENT
        } else {
            theme.raised
        },
        theme.line,
    );
    let panel = container(column![
        head,
        controls::bar(theme.line, Length::Fill, 1),
        body
    ])
    .width(INBOX_WIDTH)
    // `window` is the room under the bar already: the panel starts 4 px
    // under it and stops 12 px over the window's foot.
    .height((app.window.height - 4.0 - 12.0).min(INBOX_MOST))
    .style(move |_| crate::style::surface_panel(theme, "popout", raised, line, 8.0));
    let panel = crate::theme_art::surface(panel, app.ctx(), surface);
    let at = Point::new(app.window.width - INBOX_WIDTH - 8.0, HEIGHT + 4.0);
    Some(
        stack![
            controls::outside(Message::Bar(Event::CloseInbox), false),
            controls::placed(mouse_area(panel).on_press(Message::Nothing), at.x, at.y),
        ]
        .into(),
    )
}

fn empty_inbox<'a>(theme: &'a crate::theme::Theme, words: &'a str) -> Element<'a, Message> {
    container(
        text(words)
            .size(14)
            .color(theme.muted)
            .align_x(Alignment::Center)
            .width(Length::Fill),
    )
    .padding(iced::Padding {
        top: 16.0,
        left: 16.0,
        right: 16.0,
        bottom: 0.0,
    })
    .height(136)
    .align_y(Alignment::Center)
    .into()
}

/// A ping's message as the Mentions tab shows it: where it came from and
/// what kind of ping it holds, over its card; a click jumps to it.
fn mention_card<'a>(app: &'a App, ping: &'a MentionRow) -> Element<'a, Message> {
    let theme = &app.theme;
    let mut where_ = row![elide(ping.name.as_str()).size(12).color(theme.secondary)];
    if !ping.place.is_empty() {
        where_ = where_
            .push(text("·").size(12).color(theme.muted))
            .push(elide(ping.place.as_str()).size(12).color(theme.muted));
    }
    where_ = where_
        .push(text("·").size(12).color(theme.muted))
        .push(text(ping.kind.tag()).size(12).color(theme.muted));
    let jump = Message::Bar(Event::InboxOpenMessage {
        room: ping.room.clone(),
        item: ping.message.item.clone(),
    });
    column![
        container(where_.spacing(4)).padding(iced::Padding {
            left: 12.0,
            right: 12.0,
            top: 0.0,
            bottom: 0.0,
        }),
        crate::found::found_view(app.ctx(), &ping.card, jump),
    ]
    .spacing(2)
    .into()
}

fn inbox_line<'a>(app: &'a App, entry: &'a InboxRow) -> Element<'a, Message> {
    let theme = &app.theme;
    let mut names = column![
        elide(entry.name.as_str())
            .size(14)
            .font(BOLD)
            .color(theme.text)
    ];
    if !entry.place.is_empty() {
        names = names.push(elide(entry.place.as_str()).size(12).color(theme.muted));
    }
    let mut line = row![
        app.ctx().avatar_tinted(
            entry.picture.as_deref(),
            &entry.avatar_name,
            &entry.id,
            32.0,
            16.0,
            1.0
        ),
        container(names.spacing(2)).width(Length::Fill)
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if entry.mentions > 0 {
        line = line.push(controls::count_pill(
            theme,
            &controls::count_badge(entry.mentions),
            controls::INBOX_COUNT,
            None,
        ));
    }
    button(line)
        .padding(8)
        .width(Length::Fill)
        .height(INBOX_ROW)
        .on_press(Message::Bar(Event::InboxOpen(entry.id.clone())))
        .style(lit_style(None, theme.wash(0.06), 4.0))
        .into()
}
