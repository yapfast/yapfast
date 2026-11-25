//! The title bar Yapfast draws in place of the desktop's, with its Discord
//! look: Back and Forward, where Yapfast is in the middle, Inbox and Help,
//! then minimize, maximize and close. Pressing and moving it moves the
//! window, a double click maximizes it, and its edges resize it. Where the
//! desktop draws the title bar, or none (a tiling compositor), the bar still
//! shows under it, as Discord's client always has one, without the window's
//! buttons, and neither moves nor resizes the window. The Inbox
//! opens its popout under it: Unreads lists the channels and conversations
//! with unread messages, and Mentions the messages that ping this account,
//! under Discord's own filter menu, each jumping to its message.
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
use yapfast_core::{
    application::{DmScreen, Navigation},
    chat::ChatMessage,
};

use crate::{
    app::{App, Message},
    controls::{self, lit_style},
    elide::elide,
    fonts::BOLD,
    pictures::{Face, GroupIcon, Icon, room_group_icon},
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
    /// The Mentions tab's Filter button: its menu opened.
    MentionsFilter,
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
/// tab's message cards and rooms with mentions but no message kept.
#[derive(Default)]
pub struct Inbox {
    pub tab: Tab,
    pub rows: Vec<InboxRow>,
    pub mentions: Vec<MentionRow>,
    pub more: Vec<InboxRow>,
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
    pub group_icon: Option<GroupIcon>,
    pub mentions: u64,
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

/// The Mentions tab's filter, as Discord's menu (navId "mentions-filter")
/// sets it: whether @everyone and @here mentions count, whether role
/// mentions do, and whether, inside a server, those of all servers list or
/// only that server's. All on at first, as Discord's client starts; it
/// keeps them on the device (`recentMentionFilterSettings` there,
/// `layout.json` here), not with the account.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MentionFilter {
    pub everyone: bool,
    pub roles: bool,
    pub all_servers: bool,
}

impl Default for MentionFilter {
    fn default() -> Self {
        Self {
            everyone: true,
            roles: true,
            all_servers: true,
        }
    }
}

/// The filter menu's boxes, by their rows' keys, as Discord's client words
/// them.
pub const INCLUDE_EVERYONE: (&str, &str) = ("everyone", "Include @everyone mentions");
pub const INCLUDE_ROLES: (&str, &str) = ("roles", "Include @role mentions");
pub const INCLUDE_ALL_SERVERS: (&str, &str) = ("all-servers", "Include all servers");

impl MentionFilter {
    /// What Discord is asked for under it, `server` being the one the open
    /// room is in: that server only while "Include all servers" is off.
    pub fn query(self, server: Option<&str>) -> yapfast_core::chat::MentionsQuery {
        yapfast_core::chat::MentionsQuery {
            everyone: self.everyone,
            roles: self.roles,
            server: server.filter(|_| !self.all_servers).map(str::to_owned),
        }
    }

    /// Whether a ping (`message`, in `room`) shows under it, as Discord's
    /// client filters the pings it keeps: one in another server than
    /// `server` goes while only this server's list; then one that names
    /// this account (`me`) or replies to it always shows, and one that
    /// calls on it only with @everyone or @here, or only through a role,
    /// shows while that kind is included. The message's text says which it
    /// names; one that names none of them pinged it some other way, which
    /// counts as naming it.
    pub fn shows(
        self,
        message: &ChatMessage,
        room: &str,
        me: Option<&str>,
        server: Option<&str>,
    ) -> bool {
        use yapfast_core::mention::Target;
        if !message.mentions_me && !message.replies_to_me {
            return false;
        }
        if !self.all_servers
            && let Some(server) = server
            && server_of(room).as_deref() != Some(server)
        {
            return false;
        }
        let names = |wanted: &dyn Fn(&Target) -> bool| {
            message
                .mentions
                .iter()
                .any(|mentioned| wanted(&mentioned.target))
        };
        let everyone = names(&|target| matches!(target, Target::Everyone));
        let role = names(&|target| matches!(target, Target::Role(_)));
        let named =
            names(&|target| matches!(target, Target::User(user) if Some(user.as_str()) == me));
        named
            || message.replies_to_me
            || (!everyone && !role)
            || (everyone && self.everyone)
            || (role && self.roles)
    }
}

/// The Discord server a room is in; none for a DM or a Matrix room.
pub fn server_of(room: &str) -> Option<String> {
    use yapfast_core::discord;
    discord::parse_room(room)
        .filter(|(guild, _)| *guild != discord::DM_GUILD)
        .map(|(guild, _)| discord::server_id(guild))
}

/// The Discord server the open room is in, where the filter menu offers
/// "Include all servers".
pub fn open_server(app: &App) -> Option<String> {
    app.client.state().selected.as_deref().and_then(server_of)
}

/// One ping the Mentions tab shows: its message's card, and where it came
/// from. The card is laid out once, when the tab's rows are made.
pub struct MentionRow {
    pub room: String,
    pub message: Arc<ChatMessage>,
    /// The channel, with its `#`, and its server.
    pub name: String,
    pub place: String,
    pub card: crate::found::Found,
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
                    scroll: crate::sync::Scroll::new(INBOX_MOST),
                    ..Inbox::default()
                }),
            };
            fetch_mentions(app);
            inbox_sync(app);
            Task::none()
        }
        Event::InboxTab(tab) => {
            if let Some(inbox) = &mut app.inbox {
                inbox.tab = tab;
            }
            app.layout.inbox_tab = Some(tab.key().into());
            crate::layout::store(app.client.root(), "inbox_tab", tab.key());
            fetch_mentions(app);
            inbox_sync(app);
            Task::none()
        }
        Event::MentionsFilter => open_filter_menu(app),
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

/// Asks for this account's recent mentions under the tab's filter, as
/// Discord's client does the first time its Mentions tab shows, again after
/// Discord went away and came back, as a box of its filter changes, and
/// while only one server's list, after another room opened (`sync.rs`).
/// Outside a server, only one server's turns back into all servers', as
/// Discord's client turns it back on asking without a server.
pub fn fetch_mentions(app: &mut App) {
    let shown = app
        .inbox
        .as_ref()
        .is_some_and(|inbox| inbox.tab == Tab::Mentions);
    if !shown || app.mentions_fetched || !app.client.state().discord_online {
        return;
    }
    let server = open_server(app);
    if !app.layout.mention_filter.all_servers && server.is_none() {
        app.layout.mention_filter.all_servers = true;
        store_filter(app);
    }
    app.mentions_fetched = true;
    let query = app.layout.mention_filter.query(server.as_deref());
    app.client
        .dispatch(yapfast_core::application::Action::LoadRecentMentions(query));
}

/// Keeps the filter in `layout.json`, as Discord's client keeps it on the
/// device.
fn store_filter(app: &App) {
    crate::layout::store(
        app.client.root(),
        MENTION_FILTER_KEY,
        app.layout.mention_filter,
    );
}

/// The filter's key in `layout.json`.
pub const MENTION_FILTER_KEY: &str = "recent_mention_filter";

/// Opens the Mentions tab's filter menu under its button, its right edge at
/// the button's: its boxes as the filter stands, "Include all servers" only
/// inside a server, as Discord's client offers them.
fn open_filter_menu(app: &mut App) -> Task<Message> {
    use crate::menus::{Menu, MenuAct, MenuRow};
    let filter = app.layout.mention_filter;
    let mut rows = vec![
        (INCLUDE_EVERYONE, filter.everyone),
        (INCLUDE_ROLES, filter.roles),
    ];
    if open_server(app).is_some() {
        rows.push((INCLUDE_ALL_SERVERS, filter.all_servers));
    }
    let rows = rows
        .into_iter()
        .map(|((key, label), on)| MenuRow {
            check: controls::Check::Box(on),
            ..MenuRow::plain(MenuAct::MentionsFilter, label).key(key)
        })
        .collect();
    // The button's top left, in the inbox's head at the panel's right.
    let right = app.window.width - 8.0 - INBOX_SIDE;
    let at = Point::new(
        right - FILTER_MENU,
        HEIGHT + 4.0 + (INBOX_HEAD - FILTER_BUTTON) / 2.0,
    );
    app.win.menu = Some(
        Menu::new(at, rows)
            .anchor(crate::widgets::Anchor::Span(FILTER_BUTTON))
            .width(FILTER_MENU),
    );
    iced::widget::operation::focus(crate::app::NOBODY)
}

/// One of the filter menu's boxes changed: the filter is kept, and the
/// mentions are asked for again under it, as Discord's client asks as soon
/// as its filter changes.
pub fn change_filter(app: &mut App, key: &str, on: bool) {
    let filter = &mut app.layout.mention_filter;
    match key {
        k if k == INCLUDE_EVERYONE.0 => filter.everyone = on,
        k if k == INCLUDE_ROLES.0 => filter.roles = on,
        k if k == INCLUDE_ALL_SERVERS.0 => filter.all_servers = on,
        _ => return,
    }
    store_filter(app);
    app.mentions_fetched = false;
    fetch_mentions(app);
    inbox_sync(app);
}

/// The inbox's head: its height, its padding at the sides, the Filter
/// button's side and the filter menu's width.
const INBOX_HEAD: f32 = 48.0;
const INBOX_SIDE: f32 = 16.0;
const FILTER_BUTTON: f32 = 28.0;
const FILTER_MENU: f32 = 240.0;

/// Keeps the pings the open rooms hold for the inbox's Mentions tab, and
/// says whether its list changed: a ping kept or edited, or a fetch of
/// recent mentions. It runs on every sync, so it costs nothing when
/// nothing pinged.
pub fn mentions_sync(app: &mut App) -> bool {
    let state = app.client.state();
    let hits = &mut app.mention_hits;
    // Another account's pings go with it.
    if app.mentions_of != state.discord_user {
        app.mentions_of.clone_from(&state.discord_user);
        hits.clear();
    }
    let fetched = !Arc::ptr_eq(&app.mentions_seen, &state.recent_mentions);
    if fetched {
        app.mentions_seen = Arc::clone(&state.recent_mentions);
    }
    let live = state
        .selected
        .iter()
        .flat_map(|room| state.messages.iter().map(move |message| (room, message)))
        .chain(state.aside.iter().flat_map(|aside| {
            aside
                .messages
                .iter()
                .map(move |message| (&aside.room, message))
        }))
        // As Discord's Mentions inbox lists them: what pinged this account,
        // not a reply that didn't ping it, nor its own message.
        .filter(|(_, message)| message.mentions_me && !message.own);
    let mut added = false;
    let mut edited = false;
    for (room, message) in live {
        match hits.iter().position(|hit| hit.message.item == message.item) {
            Some(at) if Arc::ptr_eq(&hits[at].message, message) => {}
            Some(at) => {
                hits[at].message = Arc::clone(message);
                edited = true;
            }
            // When full, one older than all kept would only go again.
            None if hits.len() >= MAX_MENTIONS
                && hits
                    .back()
                    .is_some_and(|oldest| oldest.message.timestamp >= message.timestamp) => {}
            None => {
                hits.push_back(MentionHit {
                    room: room.clone(),
                    message: Arc::clone(message),
                });
                added = true;
            }
        }
    }
    if added {
        // Newest first, by when each was sent: a ping a newly opened
        // channel brings sorts under the newer ones.
        hits.make_contiguous().sort_by(|a, b| {
            b.message
                .timestamp
                .cmp(&a.message.timestamp)
                .then_with(|| mention_order(&b.message.item).cmp(&mention_order(&a.message.item)))
        });
        hits.truncate(MAX_MENTIONS);
    }
    added || edited || fetched
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
    // The cards laid out before, which a ping still showing keeps.
    let (tab, mut laid) = {
        let inbox = app.inbox.as_mut().expect("open");
        (inbox.tab, std::mem::take(&mut inbox.mentions))
    };
    let (filter, server) = (app.layout.mention_filter, open_server(app));
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
        group_icon: room.direct.then(|| room_group_icon(room)).flatten(),
        mentions: room.mentions,
    };
    let rows: Vec<InboxRow> = state
        .rooms
        .iter()
        .filter(|room| !room.request && !room.unlisted)
        .filter(|room| room.mentions > 0 || (!mentions_only && room.unread > 0 && !room.muted))
        .map(|room| row_of(room.as_ref()))
        .collect();
    // The Mentions tab's pings: those the open rooms brought and those
    // Discord's Mentions inbox sent, once each, the newest first, under its
    // filter, and where each came from.
    let mut pinged: Vec<(String, Arc<ChatMessage>, String, String)> = Vec::new();
    let mut covered: std::collections::HashSet<&str> = Default::default();
    if mentions_only {
        // The open rooms' first, as they may hold a newer edit.
        let mut all: Vec<(&String, &Arc<ChatMessage>)> = app
            .mention_hits
            .iter()
            .map(|hit| (&hit.room, &hit.message))
            .chain(
                state
                    .recent_mentions
                    .iter()
                    .map(|(room, message)| (room, message)),
            )
            .collect();
        all.sort_by(|(_, a), (_, b)| {
            b.timestamp
                .cmp(&a.timestamp)
                .then_with(|| mention_order(&b.item).cmp(&mention_order(&a.item)))
        });
        let mut seen: std::collections::HashSet<&str> = Default::default();
        let me = state.discord_user.as_deref();
        for (room, message) in all {
            if !seen.insert(message.item.as_str()) {
                continue;
            }
            // What counts as kept doesn't depend on the filter: a filter
            // hides pings, it doesn't turn their rooms back into rows.
            covered.insert(room.as_str());
            if pinged.len() >= MENTION_ROWS {
                continue;
            }
            if !filter.shows(message, room, me, server.as_deref()) {
                continue;
            }
            let Some(listed) = rooms.get(room.as_str()) else {
                continue;
            };
            let row = row_of(listed);
            pinged.push((room.clone(), Arc::clone(message), row.name, row.place));
        }
    }
    // Rooms with mentions but no message kept open below the pings, as
    // rows.
    let more: Vec<InboxRow> = if mentions_only {
        rows.iter()
            .filter(|row| row.mentions > 0 && !covered.contains(row.id.as_str()))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    // The pings' cards, laid out once rather than every frame or sync: one
    // already laid out for the same message stays.
    let mut mentions: Vec<MentionRow> = Vec::with_capacity(pinged.len());
    for (room, message, name, place) in pinged {
        let card = match laid
            .iter()
            .position(|kept| kept.room == room && Arc::ptr_eq(&kept.message, &message))
        {
            Some(at) => laid.swap_remove(at).card,
            None => crate::found::found(app, Arc::clone(&message)),
        };
        mentions.push(MentionRow {
            room,
            message,
            name,
            place,
            card,
        });
    }
    let inbox = app.inbox.as_mut().expect("open");
    if rows != inbox.rows {
        inbox.rows = rows;
    }
    inbox.mentions = mentions;
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
    let server = match &state.navigation {
        Navigation::Server(id) if workspace => state.servers.iter().find(|server| &server.id == id),
        _ => None,
    };
    let ink = if !app.shell.focused {
        theme.faint
    } else if theme.dark {
        theme.nav_open
    } else {
        theme.secondary
    };
    let (place, crumb): (&str, Element<'_, Message>) = if let Some(server) = server {
        (
            server.name.as_str(),
            app.ctx()
                .avatar(server.icon.as_deref(), &server.name, 16.0, 4.0, 1.0),
        )
    } else {
        let (place, icon) = if !workspace {
            ("Yapfast", Icon::Dm)
        } else if let Some(screen) = crate::friends::dm_screen(app) {
            match screen {
                DmScreen::Friends => ("Friends", Icon::Users),
                DmScreen::Requests => ("Direct Messages", Icon::Dm),
                DmScreen::Nitro => ("Nitro", Icon::Gem),
                DmScreen::Shop => ("Shop", Icon::Shop),
                DmScreen::Quests => ("Quests", Icon::Trophy),
            }
        } else {
            ("Direct Messages", Icon::Dm)
        };
        (
            place,
            app.look()
                .glyph(icon, 16, theme.secondary)
                .boxed(16.0)
                .into(),
        )
    };
    let middle = row![crumb, elide(place).size(14).font(BOLD).color(ink).shrink()]
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

/// Whether the inbox popout shows: open, under Yapfast's own bar, over the
/// workspace.
pub fn inbox_shown(app: &App) -> bool {
    app.inbox.is_some() && bar_height(app) > 0.0 && crate::signin::workspace(app)
}

/// The inbox popout under the bar's Inbox button, as Discord's: 440 px
/// wide, Unreads and Mentions. Unreads lists the rooms with something
/// waiting, a click going there; Mentions its pings' own messages under a
/// filter, a click jumping to the message, and the rooms with mentions but
/// no message kept below.
pub fn inbox_view(app: &App) -> Option<Element<'_, Message>> {
    let inbox = app.inbox.as_ref()?;
    if !inbox_shown(app) {
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
    let mut head = row![
        text("Inbox").size(16).font(BOLD).color(theme.text),
        Space::new().width(Length::Fill),
        tab("Unreads", Tab::Unreads, 80.0),
        tab("Mentions", Tab::Mentions, 92.0),
    ]
    .spacing(8)
    .padding([0.0, INBOX_SIDE])
    .height(INBOX_HEAD)
    .align_y(Alignment::Center);
    // Discord's Mentions tab puts its Filter button at the head's right,
    // its menu opening under it.
    if inbox.tab == Tab::Mentions {
        head = head.push(
            app.look()
                .icon_button(
                    Icon::FiltersHorizontal,
                    "Filter",
                    Message::Bar(Event::MentionsFilter),
                )
                .size(FILTER_BUTTON, 18),
        );
    }
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
        if inbox.mentions.is_empty() && inbox.more.is_empty() {
            // As Discord's tab says it with nothing to list.
            column![
                text("You made it through everything!")
                    .size(16)
                    .font(BOLD)
                    .color(theme.text),
                text("Whenever someone mentions you it will be saved here for 7 days.")
                    .size(14)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(8)
            .padding(iced::Padding {
                top: 32.0,
                left: 16.0,
                right: 16.0,
                bottom: 0.0,
            })
            .width(Length::Fill)
            .align_x(Alignment::Center)
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
        }
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

/// A ping's message as the Mentions tab shows it: where it came from over
/// its card; a click jumps to it.
fn mention_card<'a>(app: &'a App, ping: &'a MentionRow) -> Element<'a, Message> {
    let theme = &app.theme;
    let mut where_ = row![elide(ping.name.as_str()).size(12).color(theme.secondary)];
    if !ping.place.is_empty() {
        where_ = where_
            .push(text("·").size(12).color(theme.muted))
            .push(elide(ping.place.as_str()).size(12).color(theme.muted));
    }
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
    let avatar = match &entry.group_icon {
        Some(group) => app
            .ctx()
            .face(Face::new(None, &entry.avatar_name, 32.0).group(Some(group.as_face()))),
        None => app.ctx().avatar_tinted(
            entry.picture.as_deref(),
            &entry.avatar_name,
            &entry.id,
            32.0,
            16.0,
            1.0,
        ),
    };
    let mut line = row![avatar, container(names.spacing(2)).width(Length::Fill)]
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
