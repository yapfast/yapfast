//! The server rail down the window's left, with its Discord look: the DM
//! home (the app's logo), a line, the DMs with unread messages, then each
//! server and folder, then the tile that adds a server. Each
//! tile has its pill at the rail's edge (tall while open, half under the
//! pointer, a dot while unread), its mentions in a red badge, a speaker
//! when people are in its voice channels (green while this account is in
//! a call there), and its name in a tooltip to the right, with who is in
//! which voice channel beneath.

use crate::widgets::scroll_view;
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use iced::{
    Alignment, Color, Element, Length, Padding, Size, Task,
    widget::{
        Space, column, container, mouse_area, row, scrollable::Viewport, stack, text, tooltip,
    },
};
use yapfast_core::{
    application::{Action, AppState, Navigation},
    chat::{ChatRoom, ChatServer, MarkKind, VoicePerson},
    discord,
    themes::lists::{LabelPlace, PillMode, RailShape, RailTiles},
};

use crate::{
    app::{App, Message, key},
    controls::{self, RAIL_COUNT, bar, count_badge, count_pill},
    elide::elide,
    fonts::{BOLD, MEDIUM},
    pictures::{Icon, initials},
    style::{corners, hex, rounded, scrollbar},
    sync::MARGIN,
    sync::Scroll,
    widgets::{Drags, Pic, RightPress, Rows},
};

/// The server rail: its DM home, its rows and its viewport.
pub struct Rail {
    pub home: Home,
    pub items: Vec<RailItem>,
    pub scroll: crate::sync::Scroll,
    dragging: Option<Drag>,
}

struct Drag {
    server: String,
    at: iced::Point,
    grab: iced::Point,
}

struct Destination<'a> {
    item: &'a RailItem,
    target: &'a str,
    before: Option<bool>,
}

/// The rail, down the window's left edge.
pub fn view<'a>(rail: &'a Rail, cx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    View { s: rail, cx }.rail_view()
}

/// The same server picker laid across the window's top. The item faces and
/// theme tile art stay on the same path as the vertical rail; only the
/// scroll axis and edge marks change.
pub fn top_view<'a>(rail: &'a Rail, cx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    View { s: rail, cx }.top_view()
}

/// The rail's view: its rows and the view context.
#[derive(Clone, Copy)]
struct View<'a> {
    s: &'a Rail,
    cx: crate::ctx::Ctx<'a>,
}

impl Rail {
    pub fn new(height: f32) -> Self {
        Self {
            home: Home::default(),
            items: Vec::new(),
            scroll: crate::sync::Scroll::new(height),
            dragging: None,
        }
    }
}

pub const WIDTH: f32 = 72.0;

/// The rail's resolved width. An installed package may need a little more
/// room for card, ticket or tall tiles; stock and colour-only themes keep
/// Wired's exact 72 px rail.
pub fn themed_width(package: Option<&yapfast_core::themes::layout::Layout>) -> f32 {
    package
        .and_then(|layout| layout.rail_tiles.as_ref())
        .map_or(WIDTH, RailTiles::rail_width)
}

/// The rail width after ordinary chrome is considered. A package's text
/// rail owns its checked width; icon and tile rails keep the layout width.
pub fn resolved_width(
    package: Option<&yapfast_core::themes::layout::Layout>,
    chrome: Option<&yapfast_core::themes::chrome::Chrome>,
) -> f32 {
    chrome
        .filter(|chrome| chrome.rail_text())
        .and_then(|chrome| chrome.rail_width)
        .unwrap_or_else(|| themed_width(package))
}

/// A top rail is 64 px in stock themes. Theme tiles may make it taller, but
/// never shorter than Wired's usable hit area.
pub fn themed_top_height(package: Option<&yapfast_core::themes::layout::Layout>) -> f32 {
    package
        .and_then(|layout| layout.rail_tiles.as_ref())
        .map_or(64.0, |tiles| {
            let label = if tiles.label == LabelPlace::Below {
                tiles.label_size + 2.0
            } else {
                0.0
            };
            (tiles.height + label + 16.0).max(64.0)
        })
}
/// A server's tile, a folder's, and the gap after each, as Discord's.
pub const TILE: f32 = 40.0;
pub const FOLDER: f32 = 48.0;
pub const GAP: f32 = 8.0;
/// A tile's corners: a server's, and a folder's.
const RADIUS: f32 = 12.0;
/// The add-a-server tile's id, and the disc and plus drawn on it.
const ADD: &str = "+";
const ADD_DISC: f32 = 18.0;
const ADD_PLUS: u32 = 14;
const FOLDER_RADIUS: f32 = 16.0;
/// Room above each tile for the insertion line in the middle of the gap.
const INSERTION_LINE: f32 = 2.0;
const ABOVE: f32 = (GAP + INSERTION_LINE) / 2.0;
/// Room below the tile for its badge.
const BELOW: f32 = 4.0;
const LIST_TOP: f32 = TILE + 2.0 * GAP + 1.0;
const DROP_EDGE: f32 = 6.0;
/// How opaque the dragged tile's face is.
const DRAGGED: f32 = 0.5;
/// Discord's colour for a folder that wasn't given one.
const FOLDER_COLOR: u32 = 0x5865f2;
/// The pill at the rail's edge: open, pointed at, unread.
const PILL_OPEN: f32 = 40.0;
const PILL_POINTED: f32 = 20.0;
const PILL_UNREAD: f32 = 8.0;
/// A server's tooltip lists this many busy voice channels at most, each
/// with this many faces, and counts the rest.
const CARD_ROOMS: usize = 4;
const CARD_FACES: usize = 6;
const CARD_WIDTH: f32 = 220.0;

/// What's done on the rail.
#[derive(Clone, Debug)]
pub enum Event {
    Scrolled(Viewport),
    /// The pointer came onto a tile or left it, by its id ("" for the home,
    /// "+" for the add-a-server tile).
    Point(String, bool),
    Home,
    Server(String),
    Drag(String, iced::Point, iced::Point),
    DragAt(iced::Point),
    Drop,
    CancelDrag,
    Folder(String),
    /// An unread DM on the rail opens.
    Dm(String),
    Join,
    /// A right click on a server, a folder or an unread DM: its menu, where
    /// the pointer was pressed.
    Menu(String, iced::Point),
    /// A middle click on a server or a folder marks it all read, as its
    /// menu's Mark as read does.
    MarkRead(String),
}

/// The DM home's marks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Home {
    pub selected: bool,
    pub unread: bool,
    /// DM mentions and incoming friend requests.
    pub badge: String,
}

/// What a row of the rail is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Server,
    /// An unread DM, above the servers.
    Dm,
    /// The line between the unread DMs and the servers.
    Divider,
    /// A folder, open or closed (with its first four servers' initials and
    /// icons), in its colour.
    Folder {
        open: bool,
        tint: u32,
        preview: Vec<(String, Option<String>)>,
    },
    /// A server in an open folder, and whether it's the folder's last.
    Nested {
        tint: u32,
        last: bool,
    },
    /// Discord's "Add a server" tile, after the last server.
    Add,
}

/// One row of the rail.
#[derive(Clone, Debug, PartialEq)]
pub struct RailItem {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub kind: Kind,
    pub selected: bool,
    pub unread: bool,
    /// Mentions, as the badge shows them ("3", "99+"); empty for none.
    pub badge: String,
    /// This account is in a call in one of its voice channels, which
    /// turns its speaker green.
    pub call: bool,
    /// The speaker at its top right, if it shows one.
    pub media: Option<Media>,
    /// Its voice channels with people in them, which its tooltip lists
    /// under the pointer.
    pub busy: Busy,
    /// Out of reach in a Discord outage, shown faded.
    pub unavailable: bool,
}

/// A server's voice channels with people in them, in its channel list's
/// order: the core's rooms, shared.
#[derive(Clone, Default, PartialEq)]
pub struct Busy(pub Vec<Arc<ChatRoom>>);

impl std::fmt::Debug for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.0.iter().map(|room| &room.id))
            .finish()
    }
}

/// What a server's speaker shows, as Discord's server list picks it:
/// someone sharing their screen over someone on camera over anyone in
/// voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Media {
    Voice,
    Video,
    Screen,
}

/// A server's marks from its channels: whether any is unread, its
/// mentions, its speaker and its busy voice channels.
#[derive(Default)]
struct ServerMarks {
    unread: bool,
    mentions: u64,
    media: Option<Media>,
    busy: Busy,
}

impl RailItem {
    fn server(server: &ChatServer, selected: bool, marks: ServerMarks) -> Self {
        Self {
            id: server.id.clone(),
            name: server.name.clone(),
            icon: server.icon.clone(),
            kind: Kind::Server,
            selected,
            unread: marks.unread,
            badge: count_badge(marks.mentions),
            call: false,
            media: marks.media,
            busy: marks.busy,
            unavailable: server.unavailable,
        }
    }

    /// The row's height, its gap included.
    pub fn height(&self, index: usize, tiles: Option<&RailTiles>) -> f32 {
        let tile = tiles.map_or(TILE, |tiles| tiles.height);
        let folder = tiles.map_or(FOLDER, |tiles| tiles.height);
        let gap = tiles.map_or(GAP, |tiles| tiles.gap + 2.0 * tiles.pad);
        let label = tiles.map_or(0.0, |tiles| {
            if tiles.label == LabelPlace::Below {
                tiles.label_size + 2.0
            } else {
                0.0
            }
        });
        match &self.kind {
            Kind::Divider if index == 0 => 0.0,
            Kind::Divider => {
                tiles
                    .and_then(|tiles| tiles.divider)
                    .map_or(1.0, |divider| divider.size.1)
                    + gap
            }
            Kind::Folder { open: true, .. } => folder + label + 4.0,
            Kind::Folder { open: false, .. } => folder + label + gap,
            Kind::Nested { last, .. } => tile + label + if *last { 4.0 } else { 0.0 } + gap,
            Kind::Server | Kind::Dm | Kind::Add => tile + label + gap,
        }
    }
}

/// Whether `room` is the conversation someone reads: open, its newest
/// message in view and the window in front. Discord's client marks what
/// comes there read as it comes, so the room never shows as unread on the
/// rail, nor in the DM home's count, though the core counts it until its
/// read reaches Discord.
fn read_now(state: &AppState, room: &ChatRoom) -> bool {
    state.reading && !state.detached && state.selected.as_deref() == Some(room.id.as_str())
}

/// The DM home's marks, from the core's state: selected, any DM unread, and
/// the DM mentions with the friend requests waiting, as Discord counts them.
pub fn home(state: &AppState) -> Home {
    let mentions: u64 = state
        .rooms
        .iter()
        .filter(|room| room.direct && !read_now(state, room))
        .map(|room| room.mentions)
        .sum();
    let incoming = state
        .friend_requests
        .iter()
        .filter(|request| request.incoming && !request.spam)
        .count() as u64;
    Home {
        selected: state.navigation == Navigation::DirectMessages,
        unread: state
            .rooms
            .iter()
            .any(|room| room.direct && room.unread > 0 && !read_now(state, room)),
        badge: count_badge(mentions + incoming),
    }
}

/// The rail's rows, from the core's state: the unread DMs (in the core's
/// order, latest first), a line, then each server, and each folder with its
/// servers after it while it's open.
pub fn rail(state: &AppState, open_folders: &BTreeSet<String>) -> Vec<RailItem> {
    let rooms: HashMap<&str, &Arc<ChatRoom>> = state
        .rooms
        .iter()
        .map(|room| (room.id.as_str(), room))
        .collect();
    let mut items: Vec<RailItem> = state
        .rooms
        .iter()
        .filter(|room| {
            room.direct
                && !room.request
                && room.category != discord::SPAM
                && !read_now(state, room)
                && (room.unread > 0 || room.mentions > 0)
        })
        .map(|room| RailItem {
            id: room.id.clone(),
            name: room.name.clone(),
            icon: room.avatar.clone(),
            kind: Kind::Dm,
            selected: false,
            unread: true,
            // Discord counts each of a DM's messages; Matrix counts its
            // unread messages apart from its mentions.
            badge: count_badge(room.mentions.max(room.unread)),
            call: false,
            media: None,
            busy: Busy::default(),
            unavailable: false,
        })
        .collect();
    items.push(RailItem {
        id: "divider".into(),
        name: String::new(),
        icon: None,
        kind: Kind::Divider,
        selected: false,
        unread: false,
        badge: String::new(),
        call: false,
        media: None,
        busy: Busy::default(),
        unavailable: false,
    });
    let call = state
        .voice
        .active()
        .then_some(state.voice.room.as_deref())
        .flatten();
    let in_call = |server: &ChatServer| {
        call.is_some_and(|room| server.channels.iter().any(|channel| channel == room))
    };
    // The speaker of the call this account is in, as Discord's
    // GuildMediaStateStore has it: its own screen shared, else anyone on
    // camera in its channel.
    let call_media = || {
        if state.voice.sharing.is_some() {
            Media::Screen
        } else if call
            .and_then(|room| rooms.get(room))
            .is_some_and(|room| room.people.iter().any(|person| person.video))
        {
            Media::Video
        } else {
            Media::Voice
        }
    };
    let hidden = hidden_people(state);
    // A server's channels in one pass: whether any is unread, and its
    // mentions; one its channel list leaves out, or a Discord voice
    // channel's chat, marks it only by mentions, as in Discord's client
    // (GuildReadStateStore). And who is in its voice channels, which a
    // muted server's speaker leaves out unless this account is in a call
    // there.
    let marks = |server: &ChatServer| {
        let mut marks = ServerMarks::default();
        let voice = server.discord && !server.unavailable;
        for room in server
            .channels
            .iter()
            .filter_map(|channel| rooms.get(channel.as_str()))
            .filter(|room| !room.direct)
        {
            marks.unread = marks.unread
                || room.mentions > 0
                || (room.unread > 0
                    && !server.muted
                    && !room.unlisted
                    && !(server.discord && room.voice));
            marks.mentions += room.mentions;
            if voice
                && counted(server, room)
                && let Some(media) = room_media(room, &hidden)
            {
                marks.media = marks.media.max(Some(media));
                marks.busy.0.push(Arc::clone(room));
            }
        }
        if in_call(server) {
            marks.media = Some(call_media());
        } else if server.muted {
            marks.media = None;
        }
        marks
    };
    let selected = |server: &ChatServer| state.navigation == Navigation::Server(server.id.clone());
    let mut servers = state.servers.iter().peekable();
    while let Some(server) = servers.next() {
        let Some(folder) = &server.folder else {
            items.push(RailItem {
                call: in_call(server),
                ..RailItem::server(server, selected(server), marks(server))
            });
            continue;
        };
        let mut inside = vec![server];
        while let Some(next) = servers.next_if(|next| next.folder.as_ref() == Some(folder)) {
            inside.push(next);
        }
        let open = open_folders.contains(&folder.id);
        let tint = folder.color.unwrap_or(FOLDER_COLOR);
        let (unread, mentions) = inside
            .iter()
            .map(|server| marks(server))
            .fold((false, 0), |(unread, mentions), one| {
                (unread || one.unread, mentions + one.mentions)
            });
        // A closed folder's speaker is only for the call this account is
        // in, as Discord's is, never for others in its servers' channels.
        let call = inside.iter().any(|server| in_call(server));
        let name = folder.name.clone().unwrap_or_else(|| {
            let mut names: Vec<_> = inside
                .iter()
                .take(3)
                .map(|server| server.name.as_str())
                .collect();
            if inside.len() > 3 {
                names.push("…");
            }
            names.join(", ")
        });
        items.push(RailItem {
            id: folder.id.clone(),
            name,
            icon: None,
            kind: Kind::Folder {
                open,
                tint,
                preview: inside
                    .iter()
                    .take(4)
                    .map(|server| (server_initials(&server.name), server.icon.clone()))
                    .collect(),
            },
            selected: inside.iter().any(|server| selected(server)),
            unread,
            badge: count_badge(mentions),
            call,
            media: call.then(call_media),
            busy: Busy::default(),
            unavailable: false,
        });
        if open {
            let last = inside.len() - 1;
            for (index, server) in inside.into_iter().enumerate() {
                items.push(RailItem {
                    kind: Kind::Nested {
                        tint,
                        last: index == last,
                    },
                    call: in_call(server),
                    ..RailItem::server(server, selected(server), marks(server))
                });
            }
        }
    }
    if state.discord_linked
        && (state.favorites_visible || state.navigation == Navigation::Favorites)
    {
        // As Discord's client marks the tile: the mentions of the channels
        // saved and of their threads, and unread unless Favorites are muted.
        let saved: HashSet<&str> = state
            .channel_favorites
            .iter()
            .filter(|entry| entry.reference)
            .map(|entry| entry.id.as_str())
            .collect();
        let is_saved =
            |room: &str| discord::parse_room(room).is_some_and(|(_, id)| saved.contains(id));
        let (unread, mentions) = state
            .rooms
            .iter()
            .filter(|room| {
                !room.locked
                    && !room.request
                    && (is_saved(&room.id) || room.parent.as_deref().is_some_and(is_saved))
            })
            .fold((false, 0), |(unread, mentions), room| {
                (
                    unread
                        || room.mentions > 0
                        || (room.unread > 0 && !room.muted && !(room.voice && !room.direct)),
                    mentions + room.mentions,
                )
            });
        items.insert(
            0,
            RailItem {
                id: "@favorites".into(),
                name: "Favorites".into(),
                icon: None,
                kind: Kind::Server,
                selected: state.navigation == Navigation::Favorites,
                unread: unread && !state.favorites_muted,
                badge: count_badge(mentions),
                call: false,
                media: None,
                busy: Busy::default(),
                unavailable: false,
            },
        );
    }
    if state.discord_online || state.preview {
        items.push(RailItem {
            id: ADD.into(),
            name: "Add a server".into(),
            icon: None,
            kind: Kind::Add,
            selected: false,
            unread: false,
            badge: String::new(),
            call: false,
            media: None,
            busy: Busy::default(),
            unavailable: false,
        });
    }
    items
}

/// The people the rail leaves out of who is in voice: those this Discord
/// account blocked or ignored, as Discord's server list does.
fn hidden_people(state: &AppState) -> HashSet<&str> {
    state
        .discord_blocked
        .iter()
        .map(|blocked| blocked.id.as_str())
        .chain(state.discord_ignored.iter().map(String::as_str))
        .collect()
}

/// A voice channel whose people count on its server's speaker and
/// tooltip, as Discord's: one this account may see, not a Stage, not the
/// inactive channel, not muted.
fn counted(server: &ChatServer, room: &ChatRoom) -> bool {
    room.voice
        && !room.locked
        && !room.muted
        && room.mark.kind != MarkKind::Stage
        && server.afk_channel.as_deref() != Some(room.id.as_str())
}

/// What a voice channel's people show on its server's speaker, if anyone
/// shown is in it.
fn room_media(room: &ChatRoom, hidden: &HashSet<&str>) -> Option<Media> {
    room.people
        .iter()
        .filter(|person| !hidden.contains(person.id.as_str()))
        .map(|person| {
            if person.streaming {
                Media::Screen
            } else if person.video {
                Media::Video
            } else {
                Media::Voice
            }
        })
        .max()
}

/// A server's letters without its icon: the first letter of up to three
/// words, as "Co-op crew" makes "CC".
pub fn server_initials(name: &str) -> String {
    let shown: String = name
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(3)
        .flat_map(char::to_uppercase)
        .collect();
    if shown.is_empty() { "?".into() } else { shown }
}

/// A tile's letters: a server's initials, a DM's first letter.
fn tile_initials(item: &RailItem) -> String {
    match item.kind {
        Kind::Dm => initials(&item.name),
        _ => server_initials(&item.name),
    }
}

/// A tree row's branch: `└──` for a list's last, else `├──`.
fn branch(last: bool) -> String {
    if last { "└──" } else { "├──" }.to_owned()
}

/// What a tile shows around it.
#[derive(Default)]
struct Marks<'a> {
    selected: bool,
    unread: bool,
    badge: &'a str,
    /// Its speaker, and whether this account is in a call there.
    speaker: Option<(Media, bool)>,
    pointed: bool,
    folder: bool,
    open_folder: bool,
    /// A right click on it opens its menu.
    menu: bool,
    draggable: bool,
    /// A middle click on it marks it read: a server's or a folder's tile.
    read: bool,
}

impl Rail {
    fn destination(&self, above: f32) -> Option<Destination<'_>> {
        let drag = self.dragging.as_ref()?;
        let y = drag.at.y - above - LIST_TOP;
        // The gap above the first tile is its, as each gap is the tile's
        // below it, so a server can go first.
        if !(0.0..WIDTH).contains(&drag.at.x) || !(-GAP / 2.0..self.scroll.height).contains(&y) {
            return None;
        }
        let y = y + self.scroll.offset;
        let moving_folder = discord::parse_folder(&drag.server).is_some();
        let mut open_folder = None;
        let mut top = 0.0;
        for (index, item) in self.items.iter().enumerate() {
            if matches!(item.kind, Kind::Folder { open: true, .. }) {
                open_folder = Some(item.id.as_str());
            }
            let height = item.height(index, None);
            let local = y - top;
            top += height;
            if !(-GAP / 2.0..height - GAP / 2.0).contains(&local) {
                continue;
            }
            if moving_folder {
                let (target, before) = match item.kind {
                    Kind::Folder { open, .. } => (item.id.as_str(), open || local < FOLDER / 2.0),
                    Kind::Server if discord::parse_server(&item.id).is_some() => {
                        (item.id.as_str(), local < TILE / 2.0)
                    }
                    Kind::Nested { last: true, .. } if local >= TILE - DROP_EDGE => {
                        (open_folder?, false)
                    }
                    _ => return None,
                };
                return (target != drag.server).then_some(Destination {
                    item,
                    target,
                    before: Some(before),
                });
            }
            let side = match item.kind {
                Kind::Folder { .. } => FOLDER,
                Kind::Server | Kind::Nested { .. } if discord::parse_server(&item.id).is_some() => {
                    TILE
                }
                _ => return None,
            };
            if item.id == drag.server {
                return None;
            }
            let before = if matches!(item.kind, Kind::Nested { .. }) {
                // As Discord's, a server joins a folder only from the rail's
                // top level: one inside a folder takes a drop above or below.
                Some(local < side / 2.0)
            } else if local < DROP_EDGE {
                Some(true)
            } else if local >= side - DROP_EDGE
                && !matches!(item.kind, Kind::Folder { open: true, .. })
            {
                Some(false)
            } else {
                None
            };
            return Some(Destination {
                item,
                target: &item.id,
                before,
            });
        }
        None
    }
}

/// The dragged tile's face, above the rest of the window.
pub fn drag_face<'a>(rail: &'a Rail, cx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let drag = rail.dragging.as_ref()?;
    let item = rail.items.iter().find(|item| item.id == drag.server)?;
    let view = View { s: rail, cx };
    // Half seen through, as the drag image Discord's client shows, so the
    // line or outline under the pointer stays in sight.
    let face = match &item.kind {
        Kind::Folder {
            open,
            tint,
            preview,
        } => view.folder_face(*open, *tint, preview, true, DRAGGED),
        _ => view.server_face_at(item, true, DRAGGED),
    };
    Some(controls::placed(
        face,
        (drag.at.x - drag.grab.x).max(0.0),
        (drag.at.y - cx.above - drag.grab.y).max(0.0),
    ))
}

pub fn drag_subscription(app: &App) -> Option<iced::Subscription<Message>> {
    app.rail.dragging.as_ref()?;
    Some(iced::event::listen_with(|event, _, _| {
        let event = match event {
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                Event::DragAt(position)
            }
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Event::Drop
            }
            iced::Event::Mouse(iced::mouse::Event::CursorLeft)
            | iced::Event::Window(iced::window::Event::Unfocused)
            | iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            }) => Event::CancelDrag,
            _ => return None,
        };
        Some(Message::Rail(event))
    }))
}

/// What's done on the rail.
pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Scrolled(viewport) => {
            app.rail.scroll = Scroll::of(viewport);
            return app.ask();
        }
        Event::Point(id, entered) => {
            crate::app::point(&mut app.pointed.rail, id, entered);
            return Task::none();
        }
        Event::Drag(server, at, grab) => {
            app.rail.dragging = Some(Drag { server, at, grab });
            return Task::none();
        }
        Event::DragAt(at) => {
            if let Some(drag) = &mut app.rail.dragging {
                drag.at = at;
            }
            return Task::none();
        }
        Event::CancelDrag => {
            app.rail.dragging = None;
            return Task::none();
        }
        Event::Drop => {
            let view = View {
                s: &app.rail,
                cx: app.ctx(),
            };
            let destination = view
                .destination()
                .map(|drop| (drop.target.to_owned(), drop.before));
            if let (Some(drag), Some((target, before))) = (app.rail.dragging.take(), destination) {
                app.client.dispatch(Action::FileServer {
                    server: drag.server,
                    target,
                    before,
                });
            }
            return app.sync();
        }
        Event::Home => {
            app.client
                .dispatch(Action::Navigate(Navigation::DirectMessages));
        }
        Event::Server(id) => {
            app.client.dispatch(Action::Navigate(if id == "@favorites" {
                Navigation::Favorites
            } else {
                Navigation::Server(id)
            }));
        }
        Event::Folder(id) => {
            app.lists_shown = None;
            if crate::sidebar::tree::one_folder(app) {
                // A tree that opens one folder at a time keeps it apart:
                // the rail's open folders stay for the stock theme.
                let open = app.layout.tree_folder.take();
                if open.as_deref() != Some(id.as_str()) {
                    app.layout.tree_folder = Some(id);
                }
                crate::layout::store(app.client.root(), "tree_folder", &app.layout.tree_folder);
            } else {
                if !app.layout.open_folders.remove(&id) {
                    app.layout.open_folders.insert(id);
                }
                crate::layout::store(app.client.root(), "open_folders", &app.layout.open_folders);
            }
            app.rail.items = rail(
                app.client.state(),
                &crate::sidebar::tree::folders_shown(app),
            );
            return app.ask();
        }
        Event::Dm(id) => app.client.dispatch(Action::Open(id)),
        Event::Join => app.client.dispatch(Action::OpenJoinServer),
        Event::Menu(id, at) => {
            return crate::place_menus::open_place_menu(app, &id, at, false);
        }
        Event::MarkRead(id) => app.client.dispatch(Action::MarkRead(id)),
    }
    app.sync()
}

impl<'a> View<'a> {
    /// Drop hit testing uses the stock left rail's tile and list geometry.
    fn can_drag(self) -> bool {
        let theme = &self.cx.look.theme.id;
        self.tiles().is_none()
            && !self.top_rail()
            && !crate::conversation::navigation_right(self.cx.themes.package_layout(theme))
            && !self
                .cx
                .themes
                .chrome(theme)
                .is_some_and(yapfast_core::themes::chrome::Chrome::rail_text)
    }

    fn destination(self) -> Option<Destination<'a>> {
        self.can_drag()
            .then(|| self.s.destination(self.cx.above))
            .flatten()
    }

    fn tiles(self) -> Option<&'a RailTiles> {
        self.cx.themes.rail_tiles(&self.cx.look.theme.id)
    }

    fn rail_width(self) -> f32 {
        resolved_width(
            self.cx.themes.package_layout(&self.cx.look.theme.id),
            self.cx.themes.chrome(&self.cx.look.theme.id),
        )
    }

    fn top_rail(self) -> bool {
        self.cx
            .themes
            .package_layout(&self.cx.look.theme.id)
            .and_then(|layout| layout.rail)
            == Some(yapfast_core::themes::layout::RailSide::Top)
    }

    fn tile_size(self) -> (f32, f32) {
        self.tiles()
            .map_or((TILE, TILE), |tiles| (tiles.width, tiles.height))
    }

    fn folder_size(self) -> (f32, f32) {
        self.tiles()
            .map_or((FOLDER, FOLDER), |tiles| (tiles.width, tiles.height))
    }

    fn label_height(self) -> f32 {
        self.tiles().map_or(0.0, |tiles| {
            if tiles.label == LabelPlace::Below {
                tiles.label_size + 2.0
            } else {
                0.0
            }
        })
    }

    /// A tile drawn on the theme's frames (Zine's prints): no ring or
    /// shadow of its own, as Slint's.
    fn framed(self) -> bool {
        self.tiles().is_some_and(|tiles| !tiles.frame.is_empty())
    }

    /// A tile's seed, as Slint's: its id's, the home tile's 0.
    fn seed(id: &str) -> u64 {
        if id.is_empty() {
            0
        } else {
            crate::themes::slint_seed(id)
        }
    }

    /// Degrees a tile leans, from its seed within the theme's `tilt`, as
    /// Slint's `Rails.lean`; none at the Light level.
    fn lean(self, id: &str) -> f32 {
        let Some(tiles) = self.tiles().filter(|tiles| tiles.tilt != 0.0) else {
            return 0.0;
        };
        let (performance, _) = self.cx.themes.art_motion(&self.cx.look.theme.id);
        if performance == yapfast_core::themes::Performance::Light {
            return 0.0;
        }
        crate::themes::seeded_lean(Self::seed(id), tiles.tilt)
    }

    /// Of a theme's choices, the one a tile's seed picks.
    fn picked<'t>(
        choices: &'t [yapfast_core::themes::lists::TileArt],
        id: &str,
    ) -> Option<&'t yapfast_core::themes::lists::TileArt> {
        (!choices.is_empty()).then(|| &choices[Self::seed(id) as usize % choices.len()])
    }

    /// Where a framed face turns about, as `card_ground`'s pivot: the
    /// tile's centre, which the frame turns about, above the window's top.
    fn face_pivot(self, id: &str, selected: bool) -> f32 {
        let (_, tile_height) = self.tile_size();
        self.tiles()
            .and_then(|tiles| {
                selected
                    .then_some(tiles.selected.frame.as_ref())
                    .flatten()
                    .or_else(|| Self::picked(&tiles.frame, id))
            })
            .and_then(|frame| frame.window)
            .map_or(f32::NAN, |[_, y, _, _]| y - tile_height / 2.0)
    }

    /// The box a tile's face fills: its frame's window (the seed's frame,
    /// or the selected one), else the tile.
    fn face_box(self, id: &str, selected: bool) -> (f32, f32) {
        self.tiles()
            .and_then(|tiles| {
                selected
                    .then_some(tiles.selected.frame.as_ref())
                    .flatten()
                    .or_else(|| Self::picked(&tiles.frame, id))
            })
            .and_then(|frame| frame.window)
            .map_or_else(|| self.tile_size(), |[_, _, width, height]| (width, height))
    }

    fn tile_radius(self) -> f32 {
        self.tiles().map_or_else(
            || self.cx.look.theme.surface_radius("rail-icon", RADIUS),
            |tiles| {
                if tiles.shape == RailShape::Circle {
                    tiles.width.min(tiles.height) / 2.0
                } else {
                    tiles.radius
                }
            },
        )
    }

    pub fn rail_view(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        if self
            .cx
            .themes
            .chrome(&theme.id)
            .is_some_and(yapfast_core::themes::chrome::Chrome::rail_text)
        {
            return self.text_rail_view();
        }
        let tiles = self.tiles();
        let rail_width = self.rail_width();
        let viewport = self.s.scroll;
        let mut top = 0.0;
        let destination = self.destination();
        let mut rows = Rows::new(
            self.s
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| item.height(i, tiles))
                .sum(),
        );
        for (index, item) in self.s.items.iter().enumerate() {
            let height = item.height(index, tiles);
            let at = top;
            top += height;
            if at + height < viewport.offset - MARGIN
                || at > viewport.offset + viewport.height + MARGIN
            {
                continue;
            }
            // Each row reaches above its tile for the call speaker.
            rows.push(
                key(&item.id),
                at - ABOVE,
                height + ABOVE,
                self.rail_row(
                    index,
                    item,
                    height,
                    destination.as_ref().filter(|drop| drop.item.id == item.id),
                ),
            );
        }
        let list = scroll_view(rows)
            .on_scroll(|viewport| Message::Rail(Event::Scrolled(viewport)))
            .height(Length::Fill)
            .style(scrollbar(theme));
        // As Slint lays a theme's tiles: the home tile in a slot of its own
        // (the tile and `pad` round it), the theme's divider in its height
        // and 8 px, then the servers, each with the gap above its slot.
        let mut items = match tiles {
            Some(tiles) => {
                let divider: Element<'a, Message> = match tiles.divider {
                    Some(divider) => {
                        let colour = crate::themes::argb(divider.colour);
                        let (width, height) = divider.size;
                        // Dotted: dots a height apart, as Slint's.
                        let line: Element<'a, Message> = if divider.dotted {
                            let count = (width / (height * 2.0)).floor().max(1.0) as usize;
                            let mut dots = row![].spacing(height);
                            for _ in 0..count {
                                dots = dots.push(
                                    container(Space::new())
                                        .width(height)
                                        .height(height)
                                        .style(move |_| rounded(colour, height / 2.0)),
                                );
                            }
                            container(dots).width(width).into()
                        } else {
                            bar(colour, width, height).into()
                        };
                        container(line)
                            .center_x(rail_width)
                            .center_y(height + 8.0)
                            .into()
                    }
                    None => container(bar(theme.outline, 32, 1))
                        .center_x(rail_width)
                        .height(1)
                        .into(),
                };
                column![
                    container(self.home_tile())
                        .center_y(tiles.height + 2.0 * tiles.pad + self.label_height()),
                    divider,
                    list
                ]
                .spacing(0)
                .width(rail_width)
            }
            None => {
                let line = container(bar(theme.outline, 32, 1)).center_x(rail_width);
                column![self.home_tile(), line, list]
                    .spacing(GAP)
                    .width(rail_width)
            }
        };
        // As Slint: a theme's art at the rail's foot (a power light). The
        // add button is the list's last tile.
        if let Some(foot) = tiles.and_then(|tiles| tiles.foot.as_ref()) {
            let height = foot.size.map_or(48.0, |(_, height)| height);
            if let Some(art) = crate::theme_art::tile_art(
                self.cx,
                yapfast_core::themes::package::art::ArtSurface::Rail,
                foot,
                rail_width,
                height,
            ) {
                items = items.push(container(art).width(rail_width).height(height));
            }
        }
        // A glass rail keeps its tiles inside its edge, as Slint's
        // `pane-inset` does.
        // As Slint's `pane-inset`: a rail in a theme whose panes float
        // keeps its tiles 8 px in, and a glass rail's edge more.
        let inset = self.cx.themes.glass_style(&theme.id).map_or(0.0, |style| {
            let glass_rail = self
                .cx
                .themes
                .glass(
                    &theme.id,
                    yapfast_core::themes::package::art::ArtSurface::Rail,
                )
                .is_some();
            let edge = style.highlight.is_some_and(|colour| colour >> 24 != 0)
                || style.inset.as_ref().is_some_and(|inset| inset.blur > 0.0);
            let rim_width = style.rim.map_or(0.0, |(_, width)| width);
            let rim = if glass_rail {
                rim_width.max(if edge { 2.0 } else { 0.0 })
            } else {
                0.0
            };
            if style.gap > 0.0 || style.radius > 0.0 || rim > 0.0 {
                8.0 + rim
            } else {
                0.0
            }
        });
        let content: Element<'a, Message> = container(items.padding(Padding {
            top: if tiles.is_some() { inset } else { 0.0 },
            bottom: 12.0_f32.max(inset),
            ..Padding::ZERO
        }))
        .width(rail_width)
        .height(Length::Fill)
        .into();
        let mut layers = stack![];
        if let Some(tiles) = tiles {
            for ground in &tiles.ground {
                if let Some(art) = crate::theme_art::tile_art(
                    self.cx,
                    yapfast_core::themes::package::art::ArtSurface::Rail,
                    ground,
                    rail_width,
                    self.cx.look.window.height,
                ) {
                    layers = layers.push(art);
                }
            }
        }
        layers
            .push(content)
            .width(rail_width)
            .height(Length::Fill)
            .into()
    }

    /// A theme's text rail: the same navigation and menus as the tile rail,
    /// with only checked labels and marks replacing its visible words.
    fn text_rail_view(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let chrome = self
            .cx
            .themes
            .chrome(&theme.id)
            .expect("a text rail belongs to installed chrome");
        let width = self.rail_width();
        let role = chrome
            .font
            .map_or("small", yapfast_core::themes::package::Role::name);
        let fixed = |words: String| {
            text(words)
                .size(theme.type_size(role, 14.0))
                .font(theme.font(role, MEDIUM))
                .color(theme.muted)
        };
        let mut content = column![].spacing(1).padding(8).width(Length::Fill);
        if let Some(title) = chrome
            .labels
            .get(&yapfast_core::themes::chrome::Label::RailTitle)
        {
            content = content
                .push(container(fixed(title.clone())).padding(Padding::ZERO.left(8).bottom(4)));
        }
        let home = self.cx.themes.label(
            &theme.id,
            yapfast_core::themes::chrome::Label::RailHome,
            "Direct messages",
            &[],
        );
        let tree_rail =
            crate::theme_chrome::tree(self.cx, yapfast_core::themes::chrome::TreeList::Rail);
        content = content.push(self.tree_button(
            String::new(),
            if tree_rail {
                branch(self.s.items.is_empty())
            } else {
                String::new()
            },
            home,
            self.s.home.selected,
            self.s.home.unread,
            &self.s.home.badge,
            Message::Rail(Event::Home),
        ));

        let mut rows = column![].spacing(1).width(Length::Fill);
        let count = self.s.items.len();
        for (index, item) in self.s.items.iter().enumerate() {
            if item.kind == Kind::Divider {
                rows = rows.push(Space::new().height(6));
                continue;
            }
            // The text rail writes its own add line, below.
            if item.kind == Kind::Add {
                continue;
            }
            let (label, nested, selected, unread, badge, message) = match &item.kind {
                Kind::Folder { open, .. } => {
                    let label = chrome.folders.as_ref().map_or_else(
                        || format!("{} {}", if *open { "-" } else { "+" }, item.name),
                        |folders| {
                            format!(
                                "{}{}{}",
                                if *open {
                                    &folders.open
                                } else {
                                    &folders.closed
                                },
                                item.name,
                                folders.suffix
                            )
                        },
                    );
                    (
                        label,
                        false,
                        item.selected && !*open,
                        item.unread && !*open,
                        if *open { "" } else { item.badge.as_str() },
                        Message::Rail(Event::Folder(item.id.clone())),
                    )
                }
                Kind::Dm => (
                    format!("@{}", item.name),
                    false,
                    false,
                    true,
                    item.badge.as_str(),
                    Message::Rail(Event::Dm(item.id.clone())),
                ),
                Kind::Nested { .. } => (
                    self.cx.themes.marked(
                        &theme.id,
                        yapfast_core::themes::chrome::Place::Server,
                        &item.name,
                    ),
                    true,
                    item.selected,
                    item.unread,
                    item.badge.as_str(),
                    Message::Rail(Event::Server(item.id.clone())),
                ),
                Kind::Server => (
                    self.cx.themes.marked(
                        &theme.id,
                        yapfast_core::themes::chrome::Place::Server,
                        &item.name,
                    ),
                    false,
                    item.selected,
                    item.unread,
                    item.badge.as_str(),
                    Message::Rail(Event::Server(item.id.clone())),
                ),
                Kind::Divider | Kind::Add => unreachable!(),
            };
            let line = match (tree_rail, nested) {
                (true, true) => format!("│   {}", branch(index + 1 == count)),
                (true, false) => branch(index + 1 == count),
                (false, true) => "  ".to_owned(),
                (false, false) => String::new(),
            };
            rows = rows.push(self.tree_button(
                item.id.clone(),
                line,
                label,
                selected,
                unread,
                badge,
                message,
            ));
        }
        content = content.push(
            scroll_view(rows)
                .height(Length::Fill)
                .style(scrollbar(theme)),
        );
        if self.cx.state.discord_online || self.cx.state.preview {
            content = content.push(self.tree_button(
                "+".into(),
                String::new(),
                "+ join".into(),
                false,
                false,
                "",
                Message::Rail(Event::Join),
            ));
        }
        let selected =
            self.s.items.iter().find(|item| {
                item.selected && matches!(item.kind, Kind::Server | Kind::Nested { .. })
            });
        if let (Some(template), Some(server)) = (
            chrome
                .labels
                .get(&yapfast_core::themes::chrome::Label::RailFooter),
            selected,
        ) {
            let words =
                yapfast_core::themes::chrome::fill(template, &[("server", server.name.as_str())]);
            content = content.push(container(fixed(words)).padding(Padding::ZERO.left(8).top(4)));
        }
        container(content).width(width).height(Length::Fill).into()
    }

    pub fn top_view(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let tiles = self.tiles();
        let height = themed_top_height(self.cx.themes.package_layout(&theme.id));
        let cell_width = self.tile_size().0.max(self.folder_size().0).max(40.0) + 20.0;
        let mut cells = row![container(self.home_tile()).width(cell_width).height(height)]
            .align_y(Alignment::Center)
            .spacing(0);
        for (index, item) in self.s.items.iter().enumerate() {
            // The theme's own add button ends the row, below.
            if item.kind == Kind::Add {
                continue;
            }
            if item.kind == Kind::Divider {
                if index > 0 {
                    let (width, colour, dotted) = tiles.and_then(|tiles| tiles.divider).map_or(
                        (1.0, theme.outline, false),
                        |divider| {
                            (
                                divider.size.1.max(1.0),
                                crate::themes::argb(divider.colour),
                                divider.dotted,
                            )
                        },
                    );
                    let rule: Element<'a, Message> = if dotted {
                        let mut dots = column![].spacing(width);
                        let count = ((height - 24.0) / (2.0 * width)).floor().max(1.0) as usize;
                        for _ in 0..count {
                            dots = dots.push(
                                container(Space::new())
                                    .width(width)
                                    .height(width)
                                    .style(move |_| rounded(colour, width / 2.0)),
                            );
                        }
                        dots.into()
                    } else {
                        bar(colour, width, 32).into()
                    };
                    cells = cells.push(container(rule).center_x(9).center_y(height));
                }
                continue;
            }
            cells = cells.push(
                container(self.rail_row(index, item, height, None))
                    .width(cell_width)
                    .height(height),
            );
        }
        let state = self.cx.state;
        if state.discord_online || state.preview {
            if let Some(gap) = tiles.and_then(|tiles| tiles.add_gap) {
                cells = cells.push(Space::new().width(gap));
            }
            cells = cells.push(
                container(self.action_tile(
                    "+",
                    Icon::Plus,
                    "Join a server",
                    Message::Rail(Event::Join),
                ))
                .width(cell_width)
                .height(height),
            );
        }
        let content = crate::widgets::scroll_view(container(cells).padding([0.0, 6.0]))
            .across()
            .width(Length::Fill)
            .height(height)
            .style(scrollbar(theme));
        let mut layers = stack![];
        if let Some(tiles) = tiles {
            for ground in &tiles.ground {
                if let Some(art) = crate::theme_art::tile_art(
                    self.cx,
                    yapfast_core::themes::package::art::ArtSurface::Rail,
                    ground,
                    self.cx.look.window.width,
                    height,
                ) {
                    layers = layers.push(art);
                }
            }
        }
        layers
            .push(content)
            .width(Length::Fill)
            .height(height)
            .into()
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "each part it draws is its own input"
    )]
    fn tree_button(
        self,
        id: String,
        branch: String,
        label: String,
        selected: bool,
        unread: bool,
        badge: &str,
        message: Message,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let role = self
            .cx
            .themes
            .chrome(&theme.id)
            .and_then(|chrome| chrome.font)
            .map_or("body", yapfast_core::themes::package::Role::name);
        let pointed = self.cx.pointed.rail.as_deref() == Some(id.as_str());
        let ink = if selected {
            theme.on_accent
        } else if unread || pointed {
            theme.text
        } else {
            theme.secondary
        };
        let mut line = row![].align_y(Alignment::Center).spacing(4);
        if !branch.is_empty() {
            line = line.push(
                text(branch)
                    .font(crate::theme_chrome::mono(self.cx))
                    .size(14)
                    .color(if selected {
                        theme.on_accent
                    } else {
                        crate::theme_chrome::tree_ink(self.cx)
                    }),
            );
        }
        line = line.push(
            crate::elide::elide(label)
                .size(theme.type_size("small", 14.0))
                .font(if unread && !selected {
                    crate::fonts::BOLD
                } else {
                    theme.font(role, crate::fonts::REGULAR)
                })
                .color(ink),
        );
        // The count follows the name (a Fill spacer beside the name would
        // halve the room it may take).
        if !badge.is_empty() {
            let words = match self
                .cx
                .themes
                .chrome(&theme.id)
                .and_then(|chrome| chrome.counts.as_ref())
                .filter(|counts| !counts.mentions.is_empty())
            {
                Some(counts) => {
                    yapfast_core::themes::chrome::fill(&counts.mentions, &[("n", badge)])
                }
                None => format!("({badge})"),
            };
            line = line.push(
                text(words)
                    .size(14)
                    .font(crate::fonts::BOLD)
                    .color(if selected {
                        theme.on_accent
                    } else {
                        theme.danger
                    }),
            );
        }
        let ground = if selected {
            theme.accent
        } else if pointed {
            crate::style::faded(theme.text, 0.08)
        } else {
            Color::TRANSPARENT
        };
        let row = container(line)
            .height(22)
            .width(Length::Fill)
            .padding([0.0, 4.0])
            .style(move |_| crate::style::fill(ground));
        let hit = mouse_area(row)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_enter(Message::Rail(Event::Point(id.clone(), true)))
            .on_exit(Message::Rail(Event::Point(id.clone(), false)))
            .on_press(message);
        if id.is_empty() || id == "+" {
            hit.into()
        } else {
            let spot = self.cx.pressed;
            RightPress::new(hit, move |_| {
                Message::Rail(Event::Menu(id.clone(), spot.get()))
            })
            .into()
        }
    }

    /// The DM home: the app's logo, as Discord puts its own there.
    fn home_tile(self) -> Element<'a, Message> {
        let home = &self.s.home;
        let (width, height) = self.tile_size();
        let pointed = self.cx.pointed.rail.as_deref() == Some("");
        let face = self.glyph_tile(Icon::Logo, home.selected || pointed);
        let face = crate::theme_effects::rail_icon(face, self.cx, true, 0);
        let marks = Marks {
            selected: home.selected,
            unread: home.unread,
            badge: &home.badge,
            pointed,
            ..Marks::default()
        };
        // Its badge reaches into the gap below it.
        container(self.tile_cell(
            String::new(),
            (
                "Direct messages".into(),
                controls::tip(self.cx.look.theme, "Direct messages"),
            ),
            face,
            (width, height, 0.0),
            marks,
            Message::Rail(Event::Home),
        ))
        .height(height + self.label_height())
        .into()
    }

    /// A tile with one of the icons on the raised ground, in the accent;
    /// lit, on the accent.
    fn glyph_tile(self, icon: Icon, lit: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let (width, height) = self.tile_size();
        let radius = self.tile_radius();
        let mut border = theme.surface_border("rail-icon", RADIUS, 0.0, Color::TRANSPARENT);
        border.radius = radius.into();
        let shadow = theme.surface_shadow("rail-icon", iced::Shadow::default());
        let (ground, ink) = if lit {
            (theme.accent, theme.on_accent)
        } else {
            (theme.raised, theme.accent)
        };
        if self.framed() && icon == Icon::Logo {
            // As Slint's framed home tile: the logo in the theme's pink
            // ink on the raised ground in the print's window, turned with
            // the print (the logo itself stays straight).
            let ink = if lit {
                theme.on_accent
            } else {
                theme.token_colour("accent-text", theme.accent)
            };
            let ground = theme.raised;
            return crate::theme_art::card_ground(
                self.cx.look.glyph(icon, 24, ink).boxed(Length::Fill).into(),
                Some(ground),
                iced::Shadow::default(),
                self.lean(""),
                self.face_pivot("", lit && self.s.home.selected),
                0.0,
                crate::theme_art::CardMarks::default(),
            );
        }
        if self.framed() && icon == Icon::Plus {
            // Slint's add button on a framed rail: a plain 48 px box in the
            // rail icon's border and shadow, the plus in pink ink.
            let side = width.min(height).min(48.0);
            let ink = theme.token_colour("accent-text", theme.accent);
            let ground = theme.raised;
            return container(
                self.cx
                    .look
                    .glyph(Icon::Add, 24, ink)
                    .boxed(Length::Fill)
                    .width(side)
                    .height(side)
                    .style(move |_| container::Style {
                        background: Some(ground.into()),
                        border,
                        shadow,
                        ..Default::default()
                    }),
            )
            .center_x(width)
            .center_y(height)
            .into();
        }
        // A gel theme's home and action tiles are gel buttons too.
        if crate::theme_art::has_gel(self.cx) {
            let face = self
                .cx
                .look
                .glyph(icon, 24, ink)
                .boxed(Length::Fill)
                .width(width)
                .height(height);
            let tone = if lit {
                crate::theme_art::GelTone::Lit
            } else {
                crate::theme_art::GelTone::Plain
            };
            if let Some(gel) = crate::theme_art::gel_button(self.cx, face, radius, tone, 0.0, false)
            {
                return gel;
            }
        }
        // In a theme's tile window, as Slint's `TilePicture`: the raised
        // (or, lit, the accent) paint in the window's shape, without the
        // rail icon's border and shadow.
        let tiled = self.tiles().is_some();
        let paint: iced::Background = if !tiled {
            ground.into()
        } else if lit {
            theme.surface_paint("accent")
        } else {
            theme.surface_paint("raised")
        };
        self.cx
            .look
            .glyph(icon, 24, ink)
            .boxed(Length::Fill)
            .width(width)
            .height(height)
            .style(move |_| container::Style {
                background: Some(paint),
                border: if tiled {
                    iced::Border {
                        radius: border.radius,
                        ..Default::default()
                    }
                } else {
                    border
                },
                shadow: if tiled {
                    iced::Shadow::default()
                } else {
                    shadow
                },
                ..Default::default()
            })
            .into()
    }

    /// The add-a-server tile: the raised ground, a step toward the rail's
    /// while at rest, with a disc of the text's colour and the rail's plus
    /// cut from it. A theme's is Slint's `TileAddItem`: the stock "+" on the
    /// raised ground in the rail icon's shape, at most 48 px, no frame or
    /// label.
    fn add_face(self, lit: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        if self.tiles().is_some_and(|tiles| tiles.add_gap.is_some()) {
            let (width, height) = self.tile_size();
            let side = (width.min(48.0), height.min(48.0));
            let radius = theme.surface_radius("rail-icon", 12.0);
            let ground = if lit {
                theme.raised.into()
            } else {
                theme.surface_paint("raised")
            };
            let mut border = theme.surface_border("rail-icon", 12.0, 0.0, Color::TRANSPARENT);
            border.radius = radius.into();
            let shadow = theme.surface_shadow("rail-icon", iced::Shadow::default());
            return container(self.cx.look.glyph(Icon::Add, 24, theme.accent_text()))
                .center_x(side.0)
                .center_y(side.1)
                .style(move |_| container::Style {
                    background: Some(ground),
                    border,
                    shadow,
                    ..Default::default()
                })
                .into();
        }
        let ground = if lit {
            theme.raised
        } else {
            crate::theme::mix(theme.raised, theme.rail, 0.83)
        };
        let (disc, plus) = (theme.text, theme.rail);
        let glyph = self
            .cx
            .look
            .glyph(Icon::Plus, ADD_PLUS, plus)
            .boxed(ADD_DISC);
        container(
            container(glyph)
                .center(ADD_DISC)
                .style(move |_| rounded(disc, ADD_DISC / 2.0)),
        )
        .center(TILE)
        .style(move |_| rounded(ground, RADIUS))
        .into()
    }

    /// A tile at the rail's foot that does something rather than open a
    /// place.
    fn action_tile(
        self,
        id: &'static str,
        icon: Icon,
        tip: &'static str,
        message: Message,
    ) -> Element<'a, Message> {
        let (width, height) = self.tile_size();
        let pointed = self.cx.pointed.rail.as_deref() == Some(id);
        // A theme's add is Slint's `TileAddItem`: the stock "+" on the
        // raised ground in the rail icon's shape, at most 48 px, no frame or
        // label.
        if id == "+" && self.tiles().is_some_and(|tiles| tiles.add_gap.is_some()) {
            let theme = self.cx.look.theme;
            let side = (width.min(48.0), height.min(48.0));
            let radius = theme.surface_radius("rail-icon", 12.0);
            let ground = if pointed {
                theme.raised.into()
            } else {
                theme.surface_paint("raised")
            };
            let mut border = theme.surface_border("rail-icon", 12.0, 0.0, Color::TRANSPARENT);
            border.radius = radius.into();
            let shadow = theme.surface_shadow("rail-icon", iced::Shadow::default());
            // Slint's add draws the pack's plain plus.
            let _ = icon;
            let face = container(self.cx.look.glyph(Icon::Add, 24, theme.accent_text()))
                .center_x(side.0)
                .center_y(side.1)
                .style(move |_| container::Style {
                    background: Some(ground),
                    border,
                    shadow,
                    ..Default::default()
                });
            let face = iced::widget::button(face)
                .padding(0)
                .on_press(message)
                .style(|_, _| iced::widget::button::Style::default());
            let face = mouse_area(face)
                .interaction(iced::mouse::Interaction::Pointer)
                .on_enter(Message::Rail(Event::Point(id.into(), true)))
                .on_exit(Message::Rail(Event::Point(id.into(), false)));
            let face = iced::widget::tooltip(
                face,
                controls::tip(theme, tip),
                iced::widget::tooltip::Position::Left,
            )
            .gap(6);
            return container(face)
                .center_x(self.rail_width())
                .height(height)
                .align_y(Alignment::Center)
                .into();
        }
        let face = self.glyph_tile(icon, pointed);
        let marks = Marks {
            pointed,
            ..Marks::default()
        };
        let cell = self.tile_cell(
            id.into(),
            (tip.into(), controls::tip(self.cx.look.theme, tip)),
            face,
            (width, height, 0.0),
            marks,
            message,
        );
        // A theme's foot art lies above it (see `rail_view`).
        container(cell).height(height + self.label_height()).into()
    }

    /// The colour at a tile's foot, for its reflection: the gel bubble's
    /// last stop (own while lit, other else), or the accent or raised.
    fn tile_foot(self, lit: bool) -> Color {
        let theme = self.cx.look.theme;
        let bubbles = self
            .cx
            .themes
            .package_layout(&theme.id)
            .map(|layout| &layout.bubbles);
        let paint = if crate::theme_art::has_gel(self.cx) {
            bubbles.and_then(|bubbles| {
                if lit {
                    bubbles.own.as_ref()
                } else {
                    bubbles.other.as_ref()
                }
            })
        } else {
            None
        };
        match paint.map(crate::themes::paint) {
            Some(iced::Background::Color(colour)) => colour,
            Some(iced::Background::Gradient(iced::Gradient::Linear(linear))) => linear
                .stops
                .iter()
                .flatten()
                .last()
                .map_or(theme.raised, |stop| stop.color),
            _ => {
                if lit {
                    theme.accent
                } else {
                    theme.raised
                }
            }
        }
    }

    /// A server's or a DM's face: its picture, else its initials on the
    /// raised ground (the accent while lit); faded while out of reach.
    fn server_face(self, item: &RailItem, lit: bool) -> Element<'a, Message> {
        if item.id == "@favorites" {
            let theme = self.cx.look.theme;
            let ground = if lit { theme.raised } else { theme.rail };
            let (width, height) = self.tile_size();
            return container(self.cx.look.glyph(Icon::StarFilled, 20, theme.text))
                .center_x(width)
                .center_y(height)
                .style(move |_| rounded(ground, RADIUS))
                .into();
        }
        self.server_face_at(item, lit, 1.0)
    }

    /// [`Self::server_face`] at `opacity`, as the dragged tile shows.
    fn server_face_at(self, item: &RailItem, lit: bool, opacity: f32) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let (width, height) = self.tile_size();
        let radius = self.tile_radius();
        let picture_width = width.round() as u32;
        let picture_height = height.round() as u32;
        let opacity = if item.unavailable { 0.5 } else { 1.0 } * opacity;
        // A gel theme's server tiles are glossy gel buttons (as
        // asked): its own bubble's gel when selected, the other's else,
        // brightened under the pointer.
        let gel_tone = if item.selected {
            crate::theme_art::GelTone::Lit
        } else {
            crate::theme_art::GelTone::Plain
        };
        // The gel lights itself under the pointer.
        let gel_lift = 0.0;
        if let Some(picture) = item
            .icon
            .as_deref()
            .and_then(|url| self.cx.picture(url, picture_width, picture_height, true))
        {
            let (face_width, face_height) = self.face_box(&item.id, item.selected);
            let picture = Pic::new(picture, Size::new(face_width, face_height))
                .radius(radius)
                .opacity(opacity)
                .turn(if self.framed() {
                    self.lean(&item.id)
                } else {
                    0.0
                });
            if crate::theme_art::has_gel(self.cx) {
                let shell = container(picture).width(width).height(height);
                return crate::theme_art::gel_button(
                    self.cx, shell, radius, gel_tone, gel_lift, true,
                )
                .expect("a gel theme");
            }
            return picture.into();
        }
        let (ground, ink) = if lit {
            (theme.accent, theme.on_accent)
        } else {
            (theme.raised, theme.text)
        };
        let (ground, ink) = (
            crate::style::faded(ground, opacity),
            crate::style::faded(ink, opacity),
        );
        if self.framed() {
            // As Slint's framed tile: the initials on the raised ground (the
            // accent while lit) in the print's window, which turns with the
            // print; the letters stay straight.
            return crate::theme_art::card_ground(
                container(
                    text(tile_initials(item))
                        .size(16)
                        .font(theme.font("body", MEDIUM))
                        .color(ink),
                )
                .center(Length::Fill)
                .into(),
                Some(ground),
                iced::Shadow::default(),
                self.lean(&item.id),
                self.face_pivot(&item.id, item.selected),
                0.0,
                crate::theme_art::CardMarks::default(),
            );
        }
        // A theme's tile names its server as Slint's: 16 px at 600, in the
        // theme's body face.
        let font = if theme.packaged() {
            let mut font = theme.font("body", MEDIUM);
            font.weight = iced::font::Weight::Semibold;
            font
        } else {
            MEDIUM
        };
        if crate::theme_art::has_gel(self.cx) {
            let ink = if item.selected {
                theme.on_accent
            } else {
                theme.text
            };
            let face = container(
                text(tile_initials(item))
                    .size(16)
                    .font(font)
                    .color(crate::style::faded(ink, opacity)),
            )
            .center_x(width)
            .center_y(height);
            if let Some(gel) =
                crate::theme_art::gel_button(self.cx, face, radius, gel_tone, gel_lift, false)
            {
                return gel;
            }
        }
        let tiled = self.tiles().is_some();
        let paint: iced::Background = if !tiled {
            ground.into()
        } else if lit {
            theme.surface_paint("accent")
        } else {
            theme.surface_paint("raised")
        };
        container(text(tile_initials(item)).size(16).font(font).color(ink))
            .center_x(width)
            .center_y(height)
            .style(move |_| {
                let mut style = crate::style::rounded(ground, radius);
                style.background = Some(paint);
                if tiled {
                    // As Slint's `TilePicture`: no rail icon border or shadow
                    // in a theme's tile window.
                    return style;
                }
                style.border = theme.surface_border("rail-icon", RADIUS, 0.0, Color::TRANSPARENT);
                style.border.radius = radius.into();
                style.shadow = theme.surface_shadow("rail-icon", iced::Shadow::default());
                style
            })
            .into()
    }

    /// A closed folder's face: its colour, with its first four servers'
    /// icons in a square; open, the folder's own icon in its colour.
    fn folder_face(
        self,
        open: bool,
        tint: u32,
        preview: &[(String, Option<String>)],
        pointed: bool,
        opacity: f32,
    ) -> Element<'a, Message> {
        let tint = hex(tint);
        let theme = self.cx.look.theme;
        let (width, height) = self.folder_size();
        let radius = self.tile_radius();
        if open {
            return self
                .cx
                .look
                .glyph(Icon::Folder, 24, crate::style::faded(tint, opacity))
                .boxed(Length::Fill)
                .width(width)
                .height(height)
                .into();
        }
        let small = self
            .tiles()
            .and_then(|tiles| tiles.folder_mini)
            .unwrap_or_else(|| (width.min(height) - 10.0).max(16.0) / 2.0);
        let step = small + 2.0;
        let start_x = (width - (small * 2.0 + 2.0)) / 2.0;
        let start_y = (height - (small * 2.0 + 2.0)) / 2.0;
        let raised = crate::style::faded(self.cx.look.theme.raised, opacity);
        let mut faces = stack![].width(FOLDER).height(FOLDER);
        for (index, (initial, icon)) in preview.iter().take(4).enumerate() {
            // Each one's outer corner is round, as the folder's.
            let mut radius = [4.0; 4];
            radius[[0, 1, 3, 2][index]] = 13.0;
            let face: Element<'a, Message> = match icon
                .as_deref()
                .and_then(|url| self.cx.picture(url, small as u32, small as u32, true))
            {
                Some(picture) => Pic::square(picture, small)
                    .radius_each(radius)
                    .opacity(opacity)
                    .into(),
                None => container(
                    text(initial.clone())
                        .size(8)
                        .font(BOLD)
                        .color(crate::style::faded(self.cx.look.theme.text, opacity)),
                )
                .center(small)
                .style(move |_| rounded(raised, corners(radius)))
                .into(),
            };
            let (x, y) = (
                start_x + (index % 2) as f32 * step,
                start_y + (index / 2) as f32 * step,
            );
            faces = faces.push(controls::placed(face, x, y));
        }
        let ground = Color {
            a: if pointed { 0.55 } else { 0.4 } * opacity,
            ..tint
        };
        container(faces)
            .width(width)
            .height(height)
            .style(move |_| {
                let mut style = crate::style::rounded(ground, radius);
                style.border =
                    theme.surface_border("rail-icon", FOLDER_RADIUS, 0.0, Color::TRANSPARENT);
                style.border.radius = radius.into();
                style.shadow = theme.surface_shadow("rail-icon", iced::Shadow::default());
                style
            })
            .into()
    }

    fn rail_row(
        self,
        index: usize,
        item: &'a RailItem,
        height: f32,
        destination: Option<&Destination<'_>>,
    ) -> Element<'a, Message> {
        let tiles = self.tiles();
        let rail_width = if self.top_rail() {
            self.tile_size().0.max(self.folder_size().0).max(40.0) + 20.0
        } else {
            self.rail_width()
        };
        let (tile_width, tile_height) = self.tile_size();
        let (folder_width, folder_height) = self.folder_size();
        let pointed = self.cx.pointed.rail.as_deref() == Some(item.id.as_str());
        if item.kind == Kind::Divider {
            return if index == 0 {
                Space::new().into()
            } else {
                let (width, height, colour, dotted) = tiles.and_then(|tiles| tiles.divider).map_or(
                    (32.0, 1.0, self.cx.look.theme.outline, false),
                    |divider| {
                        (
                            divider.size.0,
                            divider.size.1,
                            crate::themes::argb(divider.colour),
                            divider.dotted,
                        )
                    },
                );
                let divider: Element<'a, Message> = if dotted {
                    let count = (width / (2.0 * height)).floor().max(1.0) as usize;
                    let mut dots = row![].spacing(height);
                    for _ in 0..count {
                        dots = dots.push(
                            container(Space::new())
                                .width(height)
                                .height(height)
                                .style(move |_| rounded(colour, height / 2.0)),
                        );
                    }
                    dots.into()
                } else {
                    bar(colour, width, height).into()
                };
                controls::placed(divider, (rail_width - width) / 2.0, ABOVE)
            };
        }
        let lit = item.selected || pointed;
        let (face, side, message) = match &item.kind {
            Kind::Folder {
                open,
                tint,
                preview,
            } => (
                self.folder_face(*open, *tint, preview, pointed, 1.0),
                (folder_width, folder_height),
                Message::Rail(Event::Folder(item.id.clone())),
            ),
            Kind::Dm => (
                self.server_face(item, lit),
                (tile_width, tile_height),
                Message::Rail(Event::Dm(item.id.clone())),
            ),
            Kind::Add => (
                self.add_face(pointed),
                (tile_width, tile_height),
                Message::Rail(Event::Join),
            ),
            _ => (
                self.server_face(item, lit),
                (tile_width, tile_height),
                Message::Rail(Event::Server(item.id.clone())),
            ),
        };
        let place = self.s.items[..index]
            .iter()
            .filter(|i| i.kind != Kind::Divider)
            .count();
        let face = crate::theme_effects::rail_icon(face, self.cx, false, place);
        let face = crate::widgets::motion::interactive_cued(
            face,
            self.cx.look.theme.motion(),
            self.cx.look.theme.accent,
            self.cx.look.theme.surface_radius("rail-icon", RADIUS),
            self.cx.look.theme.has_hover_cue(),
            self.cx.look.theme.has_press_cue(),
        );
        let open_folder = matches!(item.kind, Kind::Folder { open: true, .. });
        let marks = Marks {
            selected: item.selected,
            unread: item.unread,
            badge: if open_folder { "" } else { &item.badge },
            speaker: item
                .media
                .filter(|_| !open_folder)
                .map(|media| (media, item.call)),
            pointed,
            folder: matches!(item.kind, Kind::Folder { .. }),
            open_folder,
            menu: item.kind != Kind::Add,
            draggable: self.can_drag()
                && !self.cx.state.preview
                && self.cx.state.discord_user.is_some()
                && (matches!(item.kind, Kind::Folder { .. })
                    || (matches!(item.kind, Kind::Server | Kind::Nested { .. })
                        && discord::parse_server(&item.id).is_some())),
            read: !matches!(item.kind, Kind::Add | Kind::Dm | Kind::Divider),
        };
        // A server out of reach in a Discord outage says so, besides
        // fading.
        let name = if item.unavailable {
            Cow::Owned(format!("{} (unavailable in a Discord outage)", item.name))
        } else {
            Cow::Borrowed(item.name.as_str())
        };
        // Who is in which voice channel, made only under the pointer.
        let label = name.clone();
        let tip = if pointed && !item.busy.0.is_empty() {
            self.voice_card(name, &item.busy.0)
        } else {
            controls::tip(self.cx.look.theme, name)
        };
        // A theme's slot keeps its gap above it and its pad round the tile.
        let above = ABOVE + tiles.map_or(0.0, |tiles| tiles.gap + tiles.pad);
        let cell = self.tile_cell(
            item.id.clone(),
            (label, tip),
            face,
            (side.0, side.1, above),
            marks,
            message,
        );
        // An open folder's servers sit on a strip of its colour, from the
        // folder down to its last server.
        let strip = match &item.kind {
            Kind::Folder {
                open: true, tint, ..
            } => Some((*tint, true, false)),
            Kind::Nested { tint, last } => Some((*tint, false, *last)),
            _ => None,
        };
        let mut layers = stack![];
        // As Slint: a link only between two servers' tiles, not from the
        // divider or an unread DM.
        let linked = index > 0
            && !matches!(item.kind, Kind::Divider | Kind::Dm)
            && self
                .s
                .items
                .get(index - 1)
                .is_some_and(|before| !matches!(before.kind, Kind::Divider | Kind::Dm));
        if linked
            && let Some(tiles) = tiles
            && let Some(between) = tiles.between.as_ref()
            && let Some(art) = crate::theme_art::tile_art(
                self.cx,
                yapfast_core::themes::package::art::ArtSurface::Rail,
                between,
                rail_width,
                tiles.between_height,
            )
        {
            let gap = tiles.gap + 2.0 * tiles.pad;
            layers = layers.push(controls::placed(
                art,
                0.0,
                (gap - tiles.between_height).max(0.0) / 2.0,
            ));
        }
        if let Some((tint, top, last)) = strip {
            let ground = Color {
                a: 0.16,
                ..hex(tint)
            };
            let round = |on: bool| if on { RADIUS } else { 0.0 };
            let shape = [round(top), round(top), round(last), round(last)];
            let strip_width = tiles.map_or(FOLDER, |tiles| tiles.width);
            let strip_height = if last { tile_height + 4.0 } else { height };
            layers = layers.push(controls::placed(
                container(Space::new())
                    .width(strip_width)
                    .height(strip_height)
                    .style(move |_| rounded(ground, corners(shape))),
                (rail_width - strip_width) / 2.0,
                ABOVE,
            ));
        }
        let picked_up = self
            .s
            .dragging
            .as_ref()
            .is_some_and(|drag| drag.server == item.id);
        if !picked_up {
            layers = layers.push(cell);
        }
        if let Some(destination) = destination {
            let live = self.cx.look.theme.live;
            let cue = match destination.before {
                Some(before) => controls::placed(
                    bar(live, TILE, INSERTION_LINE),
                    (rail_width - TILE) / 2.0,
                    // The first row's gap lies above the list, which cuts
                    // it off: its line runs along the tile's top instead.
                    // (The divider before it is 0 px tall with no unread DMs.)
                    if !before {
                        height
                    } else if self.s.items[..index]
                        .iter()
                        .enumerate()
                        .all(|(i, item)| item.height(i, tiles) == 0.0)
                    {
                        ABOVE
                    } else {
                        0.0
                    },
                ),
                None => controls::placed(
                    container(Space::new())
                        .width(side.0 + 4.0)
                        .height(side.1 + 4.0)
                        .style(move |_| iced::widget::container::Style {
                            border: iced::Border {
                                color: live,
                                width: 2.0,
                                radius: (RADIUS + 2.0).into(),
                            },
                            ..Default::default()
                        }),
                    (rail_width - side.0) / 2.0 - 2.0,
                    ABOVE - 2.0,
                ),
            };
            layers = layers.push(cue);
        }
        layers.width(rail_width).height(height + ABOVE).into()
    }

    /// A tile in its cell across the rail, `above` px under the cell's top:
    /// the pill at the rail's edge, the tile under the pointer's watch, its
    /// optional theme label, badge and speaker, and its tooltip.
    fn tile_cell(
        self,
        id: String,
        (name, tip): (Cow<'a, str>, Element<'a, Message>),
        face: Element<'a, Message>,
        (tile_width, tile_height, above): (f32, f32, f32),
        marks: Marks<'a>,
        message: Message,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let tiles = self.tiles();
        let top_rail = self.top_rail();
        let rail_width = if top_rail {
            self.tile_size().0.max(self.folder_size().0).max(40.0) + 20.0
        } else {
            self.rail_width()
        };
        let right_rail = crate::conversation::navigation_right(
            self.cx.themes.package_layout(&self.cx.look.theme.id),
        );
        let edge_direction = if right_rail { 1.0 } else { -1.0 };
        let selected_offset = tiles
            .filter(|_| marks.selected)
            .map_or((0.0, 0.0), |tiles| tiles.selected.offset);
        let hover_lift = tiles.map_or(0.0, |tiles| {
            if marks.pointed && tiles.hover == yapfast_core::themes::lists::HoverKind::Lift {
                tiles.hover_amount
            } else {
                0.0
            }
        });
        let left = (rail_width - tile_width) / 2.0 + selected_offset.0 * edge_direction;
        let tile_top = if top_rail {
            let height = themed_top_height(self.cx.themes.package_layout(&theme.id));
            (height - tile_height - self.label_height()).max(0.0) / 2.0 + selected_offset.1
                - hover_lift
        } else {
            above + selected_offset.1 - hover_lift
        };
        let pill_allowed = tiles.is_none_or(|tiles| match tiles.pill {
            PillMode::On => true,
            PillMode::Off => false,
            PillMode::Unread => marks.unread,
        });
        let pill = if !pill_allowed || marks.open_folder {
            0.0
        } else if marks.selected {
            PILL_OPEN
        } else if marks.pointed {
            PILL_POINTED
        } else if marks.unread {
            PILL_UNREAD
        } else {
            0.0
        };
        let ink = theme.token_colour("pill", theme.text);
        let mut layers = stack![];
        if pill > 0.0 {
            let (pill_x, pill_y, pill_width, pill_height, pill_corners) = if top_rail {
                (
                    left + (tile_width - pill) / 2.0,
                    themed_top_height(self.cx.themes.package_layout(&theme.id)) - 4.0,
                    pill,
                    4.0,
                    [4.0, 4.0, 0.0, 0.0],
                )
            } else {
                (
                    if right_rail { rail_width - 4.0 } else { 0.0 },
                    tile_top + (tile_height - pill) / 2.0,
                    4.0,
                    pill,
                    if right_rail {
                        [4.0, 0.0, 0.0, 4.0]
                    } else {
                        [0.0, 4.0, 4.0, 0.0]
                    },
                )
            };
            let pill_el: Element<'a, Message> = container(Space::new())
                .width(pill_width)
                .height(pill_height)
                .style(move |_| rounded(ink, corners(pill_corners)))
                .into();
            // Always wrapped, so pointing at the tile holds the animation
            // instead of dropping its clock and restarting it.
            let pill_el = if marks.unread && !marks.selected {
                crate::theme_effects::slot_resting(
                    pill_el,
                    self.cx,
                    yapfast_core::themes::package::Slot::Unread,
                    Some(theme.rail),
                    marks.pointed,
                )
            } else {
                pill_el
            };
            layers = layers.push(controls::placed(pill_el, pill_x, pill_y));
        }
        // As Slint: the home tile's label is "DMs", not its tooltip.
        let name = if id.is_empty() { "DMs" } else { name.as_ref() };
        let label =
            crate::themes::list_case(tiles.map(|tiles| tiles.label_case), name).into_owned();
        let label_ink = tiles
            .and_then(|tiles| tiles.label_colour)
            .map(crate::themes::argb)
            .unwrap_or(theme.text);
        let label_ground = tiles
            .and_then(|tiles| tiles.label_ground)
            .map(crate::themes::argb);
        let label_size = tiles.map_or(8.0, |tiles| tiles.label_size);
        let label_box = tiles.and_then(|tiles| tiles.label_box);
        let label_font_role = tiles.map_or("small", |tiles| tiles.label_font.name());
        let label_font = theme.font(label_font_role, MEDIUM);
        let label_view = move |width: f32, height: f32| {
            // Elided to its box, centred, as Slint's.
            container(
                crate::elide::elide(label.clone())
                    .size(label_size)
                    .font(label_font)
                    .color(label_ink)
                    .centred(),
            )
            .width(width)
            .height(height)
            .clip(true)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_| container::Style {
                background: label_ground.map(Into::into),
                ..Default::default()
            })
        };
        let mut tile_face = stack![].width(tile_width).height(tile_height);
        if let Some(below) = tiles.and_then(|tiles| tiles.below.as_ref())
            && let Some(art) = crate::theme_art::tile_art(
                self.cx,
                yapfast_core::themes::package::art::ArtSurface::Rail,
                below,
                tile_width,
                tile_height,
            )
        {
            tile_face = tile_face.push(art);
        }
        let frame = tiles
            .and_then(|tiles| {
                if marks.folder {
                    if marks.open_folder {
                        tiles.folder_open_frame.as_ref()
                    } else {
                        tiles.folder_frame.as_ref()
                    }
                } else if marks.selected {
                    tiles.selected.frame.as_ref()
                } else {
                    None
                }
                .or_else(|| Self::picked(&tiles.frame, &id))
            })
            // Slint's add button is a plain box, never a print.
            .filter(|_| id != "+");
        let lean = if id == "+" { 0.0 } else { self.lean(&id) };
        let frame_art = frame.and_then(|frame| {
            crate::theme_art::tile_art_turned(
                self.cx,
                yapfast_core::themes::package::art::ArtSurface::Rail,
                frame,
                tile_width,
                tile_height,
                lean,
            )
        });
        // As Slint: a frame with a window (a polaroid, a ticket) lies under
        // the face, and the face fills the window; otherwise the frame lies
        // over the face.
        match (frame.and_then(|frame| frame.window), frame_art) {
            (Some([x, y, width, height]), Some(art)) => {
                tile_face = tile_face.push(art).push(controls::placed(
                    // A turned print's window isn't cut square, or its
                    // corners would be clipped off.
                    container(face)
                        .width(width)
                        .height(height)
                        .clip(lean == 0.0),
                    x,
                    y,
                ));
            }
            (_, art) => {
                tile_face = tile_face.push(face);
                if let Some(art) = art {
                    tile_face = tile_face.push(art);
                }
            }
        }
        if tiles.is_some_and(|tiles| tiles.label == LabelPlace::Inside) {
            let [x, y, width, height] = label_box.unwrap_or([
                2.0,
                (tile_height - label_size - 4.0).max(0.0),
                (tile_width - 4.0).max(1.0),
                label_size + 4.0,
            ]);
            tile_face = tile_face.push(controls::placed(label_view(width, height), x, y));
        }
        let below_label = tiles.is_some_and(|tiles| tiles.label == LabelPlace::Below);
        let label_height = if below_label { label_size + 2.0 } else { 0.0 };
        let mut tile_face = stack![tile_face]
            .width(tile_width)
            .height(tile_height + label_height);
        if below_label {
            tile_face = tile_face.push(controls::placed(
                label_view(tile_width, label_height),
                0.0,
                tile_height,
            ));
        }
        // As Slint: the seed's tape over every print, and the selected
        // stamp over that, both turning with the tile.
        if id != "+"
            && let Some(tiles) = tiles
        {
            let selected = tiles.selected.over.as_ref().filter(|_| marks.selected);
            // The seed's tape as a hand lays it on each print (as
            // asked): along the top edge left, centred or right, a little
            // longer or shorter, a few degrees either way, the same every
            // launch. The selected stamp stays where it's put.
            let tape = Self::picked(&tiles.over, &id).map(|over| {
                let jitter = if over.layer.position.1 <= 0.0 {
                    crate::theme_art::Jitter::tape(
                        (Self::seed(&id) as u32).rotate_left(5) ^ 0x9e37_79b9,
                    )
                } else {
                    crate::theme_art::Jitter::NONE
                };
                (over, jitter)
            });
            let stamp = selected.map(|over| (over, crate::theme_art::Jitter::NONE));
            for (over, jitter) in tape.into_iter().chain(stamp) {
                if let Some(art) = crate::theme_art::tile_art_jittered(
                    self.cx,
                    yapfast_core::themes::package::art::ArtSurface::Rail,
                    over,
                    tile_width,
                    tile_height + label_height,
                    lean,
                    jitter,
                ) {
                    tile_face = tile_face.push(art);
                }
            }
        }
        let hover = mouse_area(tile_face)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_enter(Message::Rail(Event::Point(id.clone(), true)))
            .on_exit(Message::Rail(Event::Point(id.clone(), false)));
        let hover = if marks.read {
            hover.on_middle_press(Message::Rail(Event::MarkRead(id.clone())))
        } else {
            hover
        };
        let tile: Element<'a, Message> = if marks.draggable {
            let server = id.clone();
            Drags::new(hover)
                .click(message)
                .drag_at(move |at, grab| Message::Rail(Event::Drag(server.clone(), at, grab)))
                .tracking(self.s.dragging.is_some(), |_| Message::Nothing)
                .into()
        } else {
            hover.on_press(message).into()
        };
        // Where in the window it was pressed: the rail scrolls.
        let tile: Element<'a, Message> = if marks.menu {
            let spot = self.cx.pressed;
            RightPress::new(tile, move |_| {
                Message::Rail(Event::Menu(id.clone(), spot.get()))
            })
            .into()
        } else {
            tile
        };
        let position = if top_rail {
            tooltip::Position::Bottom
        } else if right_rail {
            tooltip::Position::Left
        } else {
            tooltip::Position::Right
        };
        let tile = if self.s.dragging.is_some() {
            tile
        } else {
            tooltip(tile, tip, position)
                .gap(8)
                .snap_within_viewport(true)
                .into()
        };
        // A theme's reflection: the tile's foot mirrored under it in three
        // 4 px bands at 0.28, 0.14 and 0.05, as Slint's. Each band is the
        // width the round tile has at the rows it mirrors, in the colour
        // of its foot (the gel's, or the ground's).
        if let Some(tiles) = tiles.filter(|tiles| tiles.reflection && !top_rail) {
            let lit = marks.selected;
            let foot = self.tile_foot(lit);
            let round = tiles.shape == RailShape::Circle;
            for (band, opacity) in [0.28_f32, 0.14, 0.05].into_iter().enumerate() {
                let rows = 4.0 * band as f32 + 2.0;
                let half = tile_width / 2.0;
                let width = if round {
                    2.0 * (half * half - (half - rows).powi(2)).max(0.0).sqrt()
                } else {
                    tile_width
                };
                let colour = Color {
                    a: foot.a * opacity,
                    ..foot
                };
                layers = layers.push(controls::placed(
                    container(Space::new())
                        .width(width)
                        .height(4)
                        .style(move |_| rounded(colour, 2.0)),
                    left + (tile_width - width) / 2.0,
                    tile_top + tile_height + 4.0 * band as f32,
                ));
            }
        }
        layers = layers.push(controls::placed(tile, left, tile_top));
        if let Some((media, joined)) = marks.speaker {
            layers = layers.push(controls::placed(
                self.speaker(media, joined),
                left + tile_width + 3.0 - 20.0,
                tile_top - 3.0,
            ));
        }
        if !marks.badge.is_empty() {
            // Its right and bottom edges 4 px past the tile's.
            let badge = match tiles {
                Some(tiles)
                    if tiles.badge.ground.is_some()
                        || tiles.badge.ink.is_some()
                        || tiles.badge.radius.is_some() =>
                {
                    self.mention_badge(marks.badge, tiles.badge)
                }
                _ => count_pill(theme, marks.badge, RAIL_COUNT, Some(theme.rail)),
            };
            layers = layers.push(
                container(crate::theme_effects::slot(
                    badge,
                    self.cx,
                    yapfast_core::themes::package::Slot::Mention,
                    Some(theme.rail),
                ))
                .width(left + tile_width + BELOW)
                .height(tile_top + tile_height + BELOW)
                .align_x(Alignment::End)
                .align_y(Alignment::End),
            );
        }
        layers
            .width(rail_width)
            .height(if top_rail {
                themed_top_height(self.cx.themes.package_layout(&theme.id))
            } else {
                tile_top + tile_height + label_height + BELOW
            })
            .into()
    }

    fn mention_badge(
        self,
        count: &str,
        badge: yapfast_core::themes::lists::MentionBadge,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let ground = badge
            .ground
            .map(crate::themes::argb)
            .unwrap_or(theme.danger);
        let ink = badge
            .ink
            .map(crate::themes::argb)
            .unwrap_or(theme.on_danger);
        let radius = badge.radius.unwrap_or(RAIL_COUNT.height / 2.0);
        let number = text(count.to_owned())
            .size(RAIL_COUNT.size)
            .font(BOLD)
            .color(ink)
            .wrapping(text::Wrapping::None);
        let mut pill = container(number)
            .padding([0.0, RAIL_COUNT.pad])
            .height(RAIL_COUNT.height)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        if count.chars().count() < 2 {
            pill = pill.width(RAIL_COUNT.height);
        }
        let pill = pill.style(move |_| rounded(ground, radius));
        container(pill)
            .padding(3)
            .style(move |_| rounded(theme.rail, radius + 3.0))
            .into()
    }

    /// People in voice: a white speaker, camera or screen on grey, on a
    /// ring of the rail's colour; on green while this account is in the
    /// call, with a dark copy a pixel lower as its shadow, which the light
    /// green needs at this size.
    fn speaker(self, media: Media, joined: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let icon = match media {
            Media::Voice => Icon::Speaker,
            Media::Video => Icon::Video,
            Media::Screen => Icon::ScreenShare,
        };
        let rail = theme.rail;
        let ground = if joined {
            theme.live
        } else {
            theme.button_default
        };
        let glyph = self.cx.look.glyph(icon, 12, Color::WHITE);
        let face: Element<'a, Message> = if joined {
            let shadow = Color {
                a: 0.45,
                ..Color::BLACK
            };
            stack![
                controls::placed(self.cx.look.glyph(icon, 12, shadow), 0.0, 1.0),
                glyph,
            ]
            .into()
        } else {
            glyph.into()
        };
        container(
            container(face)
                .center(16)
                .style(move |_| rounded(ground, 8)),
        )
        .center(20)
        .style(move |_| rounded(rail, 10))
        .into()
    }

    /// A server's tooltip while people are in its voice channels: its
    /// name, then each busy channel's name over its people's faces (a
    /// screen for a channel someone shares theirs in, else a speaker), the
    /// first [`CARD_ROOMS`] channels and [`CARD_FACES`] faces each, and
    /// how many more. Discord's puts everyone in one row; by channel, it
    /// says who is together.
    fn voice_card(self, name: Cow<'a, str>, busy: &'a [Arc<ChatRoom>]) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let hidden = hidden_people(self.cx.state);
        let mut card = column![text(name).size(14).font(BOLD).color(theme.text)]
            .spacing(8)
            .width(CARD_WIDTH);
        for room in busy.iter().take(CARD_ROOMS) {
            let people: Vec<&'a VoicePerson> = room
                .people
                .iter()
                .filter(|person| !hidden.contains(person.id.as_str()))
                .collect();
            let icon = if people.iter().any(|person| person.streaming) {
                Icon::ScreenShare
            } else {
                Icon::Speaker
            };
            card = card.push(
                row![
                    self.cx.look.glyph(icon, 16, theme.muted),
                    column![
                        elide(room.name.as_str())
                            .size(12)
                            .font(BOLD)
                            .color(theme.secondary),
                        self.faces(&people),
                    ]
                    .spacing(4),
                ]
                .spacing(8),
            );
        }
        let more = busy.len().saturating_sub(CARD_ROOMS);
        if more > 0 {
            let label = if more == 1 {
                "1 more channel".to_owned()
            } else {
                format!("{more} more channels")
            };
            card = card.push(text(label).size(12).color(theme.muted));
        }
        container(card)
            .padding([10, 12])
            .style(controls::tip_style(theme))
            .into()
    }

    /// Overlapping faces, the first [`CARD_FACES`], and how many more, on
    /// rings of the tooltip's colour.
    fn faces(self, people: &[&'a VoicePerson]) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let (ring, sunken, secondary) = (theme.tooltip, theme.sunken, theme.secondary);
        let shown = people.len().min(CARD_FACES);
        let more = people.len() - shown;
        let count = shown + usize::from(more > 0);
        let mut layers = stack![Space::new().width(count as f32 * 20.0 + 6.0).height(26)];
        for (index, person) in people.iter().take(shown).enumerate() {
            layers = layers.push(controls::placed(
                container(
                    self.cx
                        .avatar(person.avatar.as_deref(), &person.name, 22.0, 11.0, 1.0),
                )
                .center(26)
                .style(move |_| rounded(ring, 13)),
                index as f32 * 20.0,
                0.0,
            ));
        }
        if more > 0 {
            layers = layers.push(controls::placed(
                container(
                    container(
                        text(format!("+{more}"))
                            .size(10)
                            .font(BOLD)
                            .color(secondary),
                    )
                    .center(22)
                    .style(move |_| rounded(sunken, 11)),
                )
                .center(26)
                .style(move |_| rounded(ring, 13)),
                shown as f32 * 20.0,
                0.0,
            ));
        }
        layers.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: String, kind: Kind) -> RailItem {
        RailItem {
            id,
            name: String::new(),
            icon: None,
            kind,
            selected: false,
            unread: false,
            badge: String::new(),
            call: false,
            media: None,
            busy: Busy::default(),
            unavailable: false,
        }
    }

    /// A server, an open folder of two and a last server, with `dragged`
    /// held at `y` in the window.
    fn drop_at(dragged: &str, y: f32) -> Option<(String, Option<bool>)> {
        let tint = FOLDER_COLOR;
        let mut rail = Rail::new(600.0);
        rail.items = vec![
            item(discord::server_id("1"), Kind::Server),
            item(
                "discord-folder:9".into(),
                Kind::Folder {
                    open: true,
                    tint,
                    preview: Vec::new(),
                },
            ),
            item(discord::server_id("2"), Kind::Nested { tint, last: false }),
            item(discord::server_id("3"), Kind::Nested { tint, last: true }),
            item(discord::server_id("4"), Kind::Server),
        ];
        rail.dragging = Some(Drag {
            server: dragged.into(),
            at: iced::Point::new(36.0, y),
            grab: iced::Point::ORIGIN,
        });
        rail.destination(0.0)
            .map(|drop| (drop.target.to_owned(), drop.before))
    }

    #[test]
    fn a_server_inside_a_folder_takes_a_drop_above_or_below_not_a_new_member() {
        let dragged = discord::server_id("4");
        let nested = discord::server_id("2");
        // The open folder's header row is FOLDER + 4 px tall, after the
        // first server's row.
        let top = LIST_TOP + (TILE + GAP) + (FOLDER + 4.0);
        assert_eq!(
            drop_at(&dragged, top + 2.0),
            Some((nested.clone(), Some(true)))
        );
        assert_eq!(
            drop_at(&dragged, top + 15.0),
            Some((nested.clone(), Some(true)))
        );
        assert_eq!(drop_at(&dragged, top + 25.0), Some((nested, Some(false))));
        // A server on the top level still makes a folder from its middle.
        let first = discord::server_id("1");
        assert_eq!(drop_at(&dragged, LIST_TOP + 20.0), Some((first, None)));
    }

    #[test]
    fn the_gap_above_the_first_server_takes_a_drop() {
        let first = discord::server_id("1");
        assert_eq!(
            drop_at(&discord::server_id("4"), LIST_TOP - 2.0),
            Some((first.clone(), Some(true)))
        );
        assert_eq!(
            drop_at("discord-folder:9", LIST_TOP - 2.0),
            Some((first, Some(true)))
        );
        assert_eq!(drop_at(&discord::server_id("4"), LIST_TOP - GAP), None);
    }
}
