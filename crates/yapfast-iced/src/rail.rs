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
    Alignment, Color, Element, Length, Padding, Task,
    widget::{
        Space, column, container, mouse_area, row, scrollable::Viewport, stack, text, tooltip,
    },
};
use yapfast_core::{
    application::{Action, AppState, Navigation},
    chat::{ChatRoom, ChatServer, MarkKind, VoicePerson},
    discord,
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
    widgets::{Pic, RightPress, Rows},
};

/// The server rail: its DM home, its rows and its viewport.
pub struct Rail {
    pub home: Home,
    pub items: Vec<RailItem>,
    pub scroll: crate::sync::Scroll,
}

/// The rail, down the window's left edge.
pub fn view<'a>(rail: &'a Rail, cx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    View { s: rail, cx }.rail_view()
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
        }
    }
}

pub const WIDTH: f32 = 72.0;
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
/// Room left above each tile for the call speaker, which stands 3 px above
/// it, and below it for the badge, 4 px past it.
const ABOVE: f32 = 3.0;
const BELOW: f32 = 4.0;
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
    Folder(String),
    /// An unread DM on the rail opens.
    Dm(String),
    Join,
    /// A right click on a server, a folder or an unread DM: its menu, where
    /// the pointer was pressed.
    Menu(String, iced::Point),
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
    pub fn height(&self, index: usize) -> f32 {
        match &self.kind {
            Kind::Divider if index == 0 => 0.0,
            Kind::Divider => 1.0 + GAP,
            Kind::Folder { open: true, .. } => FOLDER + 4.0,
            Kind::Folder { open: false, .. } => FOLDER + GAP,
            Kind::Nested { last, .. } => TILE + if *last { 4.0 } else { 0.0 } + GAP,
            Kind::Server | Kind::Dm | Kind::Add => TILE + GAP,
        }
    }
}

/// The DM home's marks, from the core's state: selected, any DM unread, and
/// the DM mentions with the friend requests waiting, as Discord counts them.
pub fn home(state: &AppState) -> Home {
    let mentions: u64 = state
        .rooms
        .iter()
        .filter(|room| room.direct)
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
            .any(|room| room.direct && room.unread > 0),
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

/// What a tile shows around it.
#[derive(Default)]
struct Marks<'a> {
    selected: bool,
    unread: bool,
    badge: &'a str,
    /// Its speaker, and whether this account is in a call there.
    speaker: Option<(Media, bool)>,
    pointed: bool,
    open_folder: bool,
    /// A right click on it opens its menu.
    menu: bool,
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
        Event::Home => app
            .client
            .dispatch(Action::Navigate(Navigation::DirectMessages)),
        Event::Server(id) => app
            .client
            .dispatch(Action::Navigate(Navigation::Server(id))),
        Event::Folder(id) => {
            app.lists_shown = None;
            if !app.layout.open_folders.remove(&id) {
                app.layout.open_folders.insert(id);
            }
            crate::layout::store(app.client.root(), "open_folders", &app.layout.open_folders);
            app.rail.items = rail(app.client.state(), &app.layout.open_folders);
            return app.ask();
        }
        Event::Dm(id) => app.client.dispatch(Action::Open(id)),
        Event::Join => app.client.dispatch(Action::OpenJoinServer),
        Event::Menu(id, at) => {
            return crate::place_menus::open_place_menu(app, &id, at, false);
        }
    }
    app.sync()
}

impl<'a> View<'a> {
    pub fn rail_view(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let viewport = self.s.scroll;
        let mut top = 0.0;
        let mut rows = Rows::new(
            self.s
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| item.height(i))
                .sum(),
        );
        for (index, item) in self.s.items.iter().enumerate() {
            let height = item.height(index);
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
                self.rail_row(index, item, height),
            );
        }
        let list = scroll_view(rows)
            .on_scroll(|viewport| Message::Rail(Event::Scrolled(viewport)))
            .height(Length::Fill)
            .style(scrollbar(theme));
        let line = container(bar(theme.outline, 32, 1)).center_x(WIDTH);
        let items = column![self.home_tile(), line, list]
            .spacing(GAP)
            .width(WIDTH);
        container(items.padding(Padding {
            bottom: 12.0,
            ..Padding::ZERO
        }))
        .width(WIDTH)
        .height(Length::Fill)
        .into()
    }

    /// The DM home: the app's logo, as Discord puts its own there.
    fn home_tile(self) -> Element<'a, Message> {
        let home = &self.s.home;
        let pointed = self.cx.pointed.rail.as_deref() == Some("");
        let face = self.glyph_tile(Icon::Logo, home.selected || pointed);
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
            controls::tip(self.cx.look.theme, "Direct messages"),
            face,
            (TILE, 0.0),
            marks,
            Message::Rail(Event::Home),
        ))
        .height(TILE)
        .into()
    }

    /// A tile with one of the icons on the raised ground, in the accent;
    /// lit, on the accent.
    fn glyph_tile(self, icon: Icon, lit: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let (ground, ink) = if lit {
            (theme.accent, theme.on_accent)
        } else {
            (theme.raised, theme.accent)
        };
        self.cx
            .look
            .glyph(icon, 24, ink)
            .boxed(TILE)
            .style(move |_| rounded(ground, RADIUS))
            .into()
    }

    /// The add-a-server tile: the raised ground, a step toward the rail's
    /// while at rest, with a disc of the text's colour and the rail's plus
    /// cut from it.
    fn add_face(self, lit: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
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

    /// A server's or a DM's face: its picture, else its initials on the
    /// raised ground (the accent while lit); faded while out of reach.
    fn server_face(self, item: &RailItem, lit: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let side = TILE as u32;
        let opacity = if item.unavailable { 0.5 } else { 1.0 };
        if let Some(picture) = item
            .icon
            .as_deref()
            .and_then(|url| self.cx.picture(url, side, side, true))
        {
            return Pic::square(picture, TILE)
                .radius(RADIUS)
                .opacity(opacity)
                .into();
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
        container(text(tile_initials(item)).size(16).font(MEDIUM).color(ink))
            .center(TILE)
            .style(move |_| rounded(ground, RADIUS))
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
    ) -> Element<'a, Message> {
        let tint = hex(tint);
        if open {
            return self
                .cx
                .look
                .glyph(Icon::Folder, 24, tint)
                .boxed(FOLDER)
                .into();
        }
        const SMALL: f32 = 19.0;
        const STEP: f32 = 21.0;
        let raised = self.cx.look.theme.raised;
        let mut faces = stack![];
        for (index, (initial, icon)) in preview.iter().enumerate() {
            // Each one's outer corner is round, as the folder's.
            let mut radius = [4.0; 4];
            radius[[0, 1, 3, 2][index]] = 13.0;
            let face: Element<'a, Message> = match icon
                .as_deref()
                .and_then(|url| self.cx.picture(url, SMALL as u32, SMALL as u32, true))
            {
                Some(picture) => Pic::square(picture, SMALL).radius_each(radius).into(),
                None => container(
                    text(initial.clone())
                        .size(8)
                        .font(BOLD)
                        .color(self.cx.look.theme.text),
                )
                .center(SMALL)
                .style(move |_| rounded(raised, corners(radius)))
                .into(),
            };
            let (x, y) = (
                4.0 + (index % 2) as f32 * STEP,
                4.0 + (index / 2) as f32 * STEP,
            );
            faces = faces.push(controls::placed(face, x, y));
        }
        let ground = Color {
            a: if pointed { 0.55 } else { 0.4 },
            ..tint
        };
        container(faces)
            .width(FOLDER)
            .height(FOLDER)
            .style(move |_| rounded(ground, FOLDER_RADIUS))
            .into()
    }

    fn rail_row(self, index: usize, item: &'a RailItem, height: f32) -> Element<'a, Message> {
        let pointed = self.cx.pointed.rail.as_deref() == Some(item.id.as_str());
        if item.kind == Kind::Divider {
            return if index == 0 {
                Space::new().into()
            } else {
                controls::placed(
                    bar(self.cx.look.theme.outline, 32, 1),
                    (WIDTH - 32.0) / 2.0,
                    ABOVE,
                )
            };
        }
        let lit = item.selected || pointed;
        let (face, side, message) = match &item.kind {
            Kind::Folder {
                open,
                tint,
                preview,
            } => (
                self.folder_face(*open, *tint, preview, pointed),
                FOLDER,
                Message::Rail(Event::Folder(item.id.clone())),
            ),
            Kind::Dm => (
                self.server_face(item, lit),
                TILE,
                Message::Rail(Event::Dm(item.id.clone())),
            ),
            Kind::Add => (self.add_face(pointed), TILE, Message::Rail(Event::Join)),
            _ => (
                self.server_face(item, lit),
                TILE,
                Message::Rail(Event::Server(item.id.clone())),
            ),
        };
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
            open_folder,
            menu: item.kind != Kind::Add,
        };
        // A server out of reach in a Discord outage says so, besides
        // fading.
        let name = if item.unavailable {
            Cow::Owned(format!("{} (unavailable in a Discord outage)", item.name))
        } else {
            Cow::Borrowed(item.name.as_str())
        };
        // Who is in which voice channel, made only under the pointer.
        let tip = if pointed && !item.busy.0.is_empty() {
            self.voice_card(name, &item.busy.0)
        } else {
            controls::tip(self.cx.look.theme, name)
        };
        let cell = self.tile_cell(item.id.clone(), tip, face, (side, ABOVE), marks, message);
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
        if let Some((tint, top, last)) = strip {
            let ground = Color {
                a: 0.16,
                ..hex(tint)
            };
            let round = |on: bool| if on { RADIUS } else { 0.0 };
            let shape = [round(top), round(top), round(last), round(last)];
            let strip_height = if last { TILE + 4.0 } else { height };
            layers = layers.push(controls::placed(
                container(Space::new())
                    .width(FOLDER)
                    .height(strip_height)
                    .style(move |_| rounded(ground, corners(shape))),
                (WIDTH - FOLDER) / 2.0,
                ABOVE,
            ));
        }
        layers.push(cell).width(WIDTH).height(height + ABOVE).into()
    }

    /// A tile of `side` in its cell across the rail, `above` px under the
    /// cell's top: the pill at the rail's edge, the tile under the pointer's
    /// watch, its badge and speaker, its tooltip.
    fn tile_cell(
        self,
        id: String,
        tip: Element<'a, Message>,
        face: Element<'a, Message>,
        (side, above): (f32, f32),
        marks: Marks<'a>,
        message: Message,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let left = (WIDTH - side) / 2.0;
        let pill = if marks.open_folder {
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
        let ink = theme.text;
        let mut layers = stack![];
        if pill > 0.0 {
            layers = layers.push(controls::placed(
                container(Space::new())
                    .width(4)
                    .height(pill)
                    .style(move |_| rounded(ink, corners([0.0, 4.0, 4.0, 0.0]))),
                0.0,
                above + (side - pill) / 2.0,
            ));
        }
        let tile = mouse_area(face)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_enter(Message::Rail(Event::Point(id.clone(), true)))
            .on_exit(Message::Rail(Event::Point(id.clone(), false)))
            .on_press(message);
        // Where in the window it was pressed: the rail scrolls.
        let tile: Element<'a, Message> = if marks.menu {
            let spot = self.cx.pressed;
            RightPress::new(tile, move |_| {
                Message::Rail(Event::Menu(id.clone(), spot.get()))
            })
            .into()
        } else {
            tile.into()
        };
        let tile = tooltip(tile, tip, tooltip::Position::Right)
            .gap(8)
            .snap_within_viewport(true);
        layers = layers.push(controls::placed(tile, left, above));
        if let Some((media, joined)) = marks.speaker {
            layers = layers.push(controls::placed(
                self.speaker(media, joined),
                left + side + 3.0 - 20.0,
                above - 3.0,
            ));
        }
        if !marks.badge.is_empty() {
            // Its right and bottom edges 4 px past the tile's.
            layers = layers.push(
                container(count_pill(theme, marks.badge, RAIL_COUNT, Some(theme.rail)))
                    .width(left + side + BELOW)
                    .height(above + side + BELOW)
                    .align_x(Alignment::End)
                    .align_y(Alignment::End),
            );
        }
        layers.width(WIDTH).height(above + side + BELOW).into()
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
