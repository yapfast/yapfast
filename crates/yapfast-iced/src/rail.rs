//! The server rail down the window's left, with its Discord look: the DM
//! home (the app's logo), a line, the DMs with unread messages, then each
//! server and folder, and the button to join a server at the foot. Each
//! tile has its pill at the rail's edge (tall while open, half under the
//! pointer, a dot while unread), its mentions in a red badge, a speaker
//! while this account is in a call there, and its name in a tooltip to the
//! right.

use crate::widgets::scroll_view;
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};

use iced::{
    Alignment, Color, Element, Length, Padding, Task,
    widget::{Space, column, container, mouse_area, scrollable::Viewport, stack, text, tooltip},
};
use yapfast_core::{
    application::{Action, AppState, Navigation},
    chat::{ChatRoom, ChatServer},
    discord,
};

use crate::{
    app::{App, Message, key},
    controls::{self, RAIL_COUNT, bar, count_badge, count_pill},
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

/// What's done on the rail.
#[derive(Clone, Debug)]
pub enum Event {
    Scrolled(Viewport),
    /// The pointer came onto a tile or left it, by its id ("" for the home,
    /// "+" for the join button).
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
    /// This account is in a call in one of its voice channels.
    pub call: bool,
    /// Out of reach in a Discord outage, shown faded.
    pub unavailable: bool,
}

impl RailItem {
    fn server(server: &ChatServer, selected: bool, (unread, mentions): (bool, u64)) -> Self {
        Self {
            id: server.id.clone(),
            name: server.name.clone(),
            icon: server.icon.clone(),
            kind: Kind::Server,
            selected,
            unread,
            badge: count_badge(mentions),
            call: false,
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
            Kind::Server | Kind::Dm => TILE + GAP,
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
    let rooms: HashMap<&str, &ChatRoom> = state
        .rooms
        .iter()
        .map(|room| (room.id.as_str(), &**room))
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
        unavailable: false,
    });
    // A server's channels: whether any is unread, and its mentions. One
    // its channel list leaves out, or a Discord voice channel's chat, marks
    // it only by mentions, as in Discord's client (GuildReadStateStore).
    let marks = |server: &ChatServer| {
        let channels = || {
            server
                .channels
                .iter()
                .filter_map(|channel| rooms.get(channel.as_str()))
                .filter(|room| !room.direct)
        };
        (
            channels().any(|room| {
                room.mentions > 0
                    || (room.unread > 0
                        && !server.muted
                        && !room.unlisted
                        && !(server.discord && room.voice))
            }),
            channels().map(|room| room.mentions).sum::<u64>(),
        )
    };
    let selected = |server: &ChatServer| state.navigation == Navigation::Server(server.id.clone());
    let call = state
        .voice
        .active()
        .then_some(state.voice.room.as_deref())
        .flatten();
    let in_call = |server: &ChatServer| {
        call.is_some_and(|room| server.channels.iter().any(|channel| channel == room))
    };
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
            .fold((false, 0), |(unread, mentions), (one, more)| {
                (unread || one, mentions + more)
            });
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
            call: inside.iter().any(|server| in_call(server)),
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
    items
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
    call: bool,
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
        let mut items = column![self.home_tile(), line, list]
            .spacing(GAP)
            .width(WIDTH);
        let state = self.cx.state;
        if state.discord_online || state.preview {
            items = items.push(self.action_tile(
                "+",
                Icon::Plus,
                "Join a server",
                Message::Rail(Event::Join),
            ));
        }
        // The buttons plugins put at the rail's foot, under its own.
        #[cfg(feature = "plugins")]
        if let Some(overrides) = self.cx.plugins_ui {
            let target = state.selected_room().map_or("", |room| room.id.as_str());
            for (button, _) in &overrides.rail {
                if let Some(input) = crate::plugins_ui::chosen(overrides, &button.action, target) {
                    items =
                        items.push(self.plugin_tile(button.label.clone(), Message::Plugin(input)));
                }
            }
        }
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
            "Direct messages".into(),
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

    /// A tile at the rail's foot that does something rather than open a
    /// place.
    fn action_tile(
        self,
        id: &'static str,
        icon: Icon,
        tip: &'static str,
        message: Message,
    ) -> Element<'a, Message> {
        let pointed = self.cx.pointed.rail.as_deref() == Some(id);
        let face = self.glyph_tile(icon, pointed);
        let marks = Marks {
            pointed,
            ..Marks::default()
        };
        container(self.tile_cell(id.into(), tip.into(), face, (TILE, 0.0), marks, message))
            .height(TILE)
            .into()
    }

    /// A button a plugin adds to the rail's foot, named by its manifest.
    #[cfg(feature = "plugins")]
    fn plugin_tile(self, label: String, message: Message) -> Element<'a, Message> {
        let id = format!("plugin-{label}");
        let pointed = self.cx.pointed.rail.as_deref() == Some(id.as_str());
        let face = self.glyph_tile(Icon::Plugins, pointed);
        let marks = Marks {
            pointed,
            ..Marks::default()
        };
        container(self.tile_cell(id, label.into(), face, (TILE, 0.0), marks, message))
            .height(TILE)
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
            call: item.call && !open_folder,
            pointed,
            open_folder,
            menu: true,
        };
        // A server out of reach in a Discord outage says so, besides
        // fading.
        let tip = if item.unavailable {
            Cow::Owned(format!("{} (unavailable in a Discord outage)", item.name))
        } else {
            Cow::Borrowed(item.name.as_str())
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
        tip: Cow<'a, str>,
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
        let tile = tooltip(tile, controls::tip(theme, tip), tooltip::Position::Right).gap(8);
        layers = layers.push(controls::placed(tile, left, above));
        if marks.call {
            layers = layers.push(controls::placed(
                self.call_bubble(),
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

    /// A call going on: a white speaker on green, on a ring of the rail's
    /// colour, with a dark copy a pixel lower as its shadow, which the light
    /// green needs at this size.
    fn call_bubble(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let (rail, live) = (theme.rail, theme.live);
        let shadow = Color {
            a: 0.45,
            ..Color::BLACK
        };
        let speaker = stack![
            controls::placed(self.cx.look.glyph(Icon::Speaker, 12, shadow), 0.0, 1.0),
            self.cx.look.glyph(Icon::Speaker, 12, Color::WHITE),
        ];
        container(
            container(speaker)
                .center(16)
                .style(move |_| rounded(live, 8)),
        )
        .center(20)
        .style(move |_| rounded(rail, 10))
        .into()
    }
}
