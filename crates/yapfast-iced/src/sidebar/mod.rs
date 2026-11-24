//! The sidebar beside the rail, with its Discord look: on the DM home the
//! search field, Friends and Requests, the "Direct Messages" heading and
//! each DM with its person's presence and what they're up to; in a server
//! its name (which opens its menu), then its channels under their
//! categories, which fold, with who is in each voice channel under it. The
//! list is virtualized: only rows near its viewport are built.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Color, Element, Length, Padding, Point, Task,
    widget::{Space, column, container, row, scrollable::Viewport, stack},
};
use yapfast_core::application::{Action, DmScreen, MenuChoice, Navigation};

use crate::{
    app::{App, Message, key},
    controls::{self},
    style::{corners, rounded, scrollbar},
    sync::MARGIN,
    sync::Scroll,
    widgets::Rows,
};

mod channels;
mod dms;
mod rows;
mod top;
pub mod tree;
mod voice;

pub(crate) use channels::listed_row_face;
pub use rows::{Room, SideHeader, SideRow, room_icon, side_header, sidebar};

/// The sidebar: its header and rows, its viewport, what's dragged in it,
/// and the filters typed into it and into Browse Channels.
pub struct Sidebar {
    pub header: SideHeader,
    pub rows: Vec<SideRow>,
    pub scroll: crate::sync::Scroll,
    /// The busy voice channel's card under the pointer was put away by a
    /// click or its menu, until the pointer leaves the row.
    pub voice_card_off: bool,
    /// The rows' places to drag a channel or category among, and the drag
    /// going on.
    pub places: crate::arrange::Places,
    pub arranging: Option<crate::arrange::Arranging>,
    /// Someone being dragged from a voice channel to another.
    pub moving_person: Option<crate::arrange::Moving>,
    /// The width the sidebar had as its edge was pressed, while it's
    /// dragged.
    pub drag: Option<f32>,
    /// The DM search's filter, and where the rail went last, so the filter
    /// goes when it goes elsewhere.
    pub filter: String,
    pub filtered_in: Option<yapfast_core::application::Navigation>,
    /// Browse Channels' search.
    pub browse_filter: String,
    /// A theme's server tree: its rows, the row the keys are on while it
    /// has them, whether that row is outlined (once a key moved it), the
    /// press that gave it the keys, and the place and channel the window
    /// was last seen in, which it opens the way to as they change.
    pub tree: Vec<tree::TreeRow>,
    pub tree_cursor: Option<String>,
    pub tree_ring: bool,
    pub tree_press: Option<Point>,
    pub tree_seen: Option<(Navigation, Option<String>)>,
}

/// The sidebar beside the rail.
pub fn view<'a>(sidebar: &'a Sidebar, cx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    View { s: sidebar, cx }.sidebar_view()
}

/// A theme's server tree, in the sidebar under its header.
pub fn tree_view<'a>(sidebar: &'a Sidebar, cx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    View { s: sidebar, cx }.tree_view()
}

/// The sidebar's header alone: over a theme's server tree, Slint keeps it
/// above the tree.
pub fn header_view<'a>(sidebar: &'a Sidebar, cx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    View { s: sidebar, cx }.side_header_view()
}

/// The sidebar's view: its rows and what's dragged in it, and the view
/// context.
#[derive(Clone, Copy)]
pub(crate) struct View<'a> {
    pub(crate) s: &'a Sidebar,
    pub(crate) cx: crate::ctx::Ctx<'a>,
}

impl Sidebar {
    pub fn new(height: f32) -> Self {
        Self {
            header: SideHeader::default(),
            rows: Vec::new(),
            scroll: crate::sync::Scroll::new(height),
            voice_card_off: false,
            places: Default::default(),
            arranging: None,
            moving_person: None,
            drag: None,
            filter: String::new(),
            filtered_in: None,
            browse_filter: String::new(),
            tree: Vec::new(),
            tree_cursor: None,
            tree_ring: false,
            tree_press: None,
            tree_seen: None,
        }
    }
}

impl Sidebar {
    /// The rows that aren't the dragged one's kind dim: all of them under
    /// a category, and voice and text channels under each other; while
    /// someone is dragged, all but the voice channels they may drop on.
    pub fn arrange_dimmed(&self, room: Option<&str>, voice: bool) -> bool {
        if let Some(moving) = &self.moving_person {
            return !room.is_some_and(|room| moving.targets.contains(room));
        }
        self.arranging
            .as_ref()
            .is_some_and(|arranging| arranging.category || voice != arranging.voice)
    }

    /// Whether someone dragged would drop on this voice channel.
    pub fn move_aimed(&self, room: &str) -> bool {
        self.moving_person.as_ref().is_some_and(|moving| {
            moving.hovered.as_deref() == Some(room) && moving.targets.contains(room)
        })
    }

    /// A drag of either kind is on, and the rows follow it.
    pub fn dragging(&self) -> bool {
        self.arranging.is_some() || self.moving_person.is_some()
    }

    /// Whether the line showing where the dragged row would land is at the
    /// row at `place`, and whether below it (dragged down) or above.
    pub fn drop_line(&self, place: Option<usize>) -> Option<bool> {
        let arranging = self.arranging.as_ref()?;
        let place = place?;
        (arranging.hovered == Some(place)
            && place != arranging.dragged
            && arranging.droppable.get(place) == Some(&true))
        .then_some(place > arranging.dragged)
    }

    /// How tall the sidebar's top is over its list: the header (taller
    /// with a banner), the boost goal and the events row.
    /// Where the list of rows starts and ends in the window, `above` the
    /// content's top: under the header, as tall as its viewport.
    pub fn list_span(&self, above: f32) -> (f32, f32) {
        let top = above + self.top_height();
        (top, top + self.scroll.height)
    }

    pub fn top_height(&self) -> f32 {
        let head = &self.header;
        if head.server.is_none() {
            return HEADER;
        }
        HEADER
            + if head.banner.is_some() { BANNER } else { 0.0 }
            + if head.boosts.is_some() {
                BOOST_ROW
            } else {
                0.0
            }
            + if head.events.is_some() {
                EVENTS_ROW
            } else {
                0.0
            }
    }
}

/// The sidebar's width, as its Discord look sets it.
pub const WIDTH: f32 = 304.0;
/// The least and the most its edge drags it to.
const LEAST: f32 = 180.0;
const MOST: f32 = 380.0;

/// The sidebar's width: as its edge was dragged, else [`WIDTH`].
pub fn width(layout: &crate::layout::Layout, theme: &crate::theme::Theme) -> f32 {
    layout
        .sidebar_width
        .or(theme.spacing().sidebar_width)
        .map_or(WIDTH, |width| width.clamp(LEAST, MOST))
}

/// `width` within what the sidebar may take: 180 to 380 px, and no more
/// than leaves the conversation 420 px.
fn room(app: &App, width: f32) -> f32 {
    let package = app.themes.package_layout(&app.theme.id);
    let navigation = crate::conversation::right_rail(package, app.themes.chrome(&app.theme.id));
    let beside = app.window.width - crate::conversation::chat_right(app) - navigation;
    let most =
        app.window.width - crate::rail::themed_width(package) - crate::thread::CHAT_KEPT - beside;
    width.clamp(LEAST, most.clamp(LEAST, MOST))
}
/// Its header, the line under it included.
pub const HEADER: f32 = 50.0;
/// A channel's row, a DM's, a category's heading, someone in a voice
/// channel, and the gap after each row.
pub const ROW: f32 = 32.0;
const DM_ROW: f32 = 42.0;
/// The Friends and Requests rows, taller than a channel's.
const NAV_ROW: f32 = 38.0;

/// A link row's height (Friends and Requests `tall`, Browse Channels not),
/// as it's drawn and measured: Discord's whatever a theme's channels take.
pub(crate) fn link_height(tall: bool) -> f32 {
    if tall { NAV_ROW } else { ROW }
}
/// A row's corners.
const ROW_RADIUS: f32 = 6.0;
const HEADING: f32 = 38.0;
/// A row's own small button: a voice channel's Open chat, a category's "+".
const CHAT_BUTTON: f32 = 24.0;
const PERSON: f32 = 30.0;
pub(crate) const GAP: f32 = 2.0;
/// The space the list leaves after its last row.
const END: f32 = 12.0;
/// The list's inset from the sidebar's sides.
const INSET: f32 = 8.0;
/// The search field's id, and the list's.
pub const FILTER: &str = "sidebar-filter";
pub const LIST: &str = "sidebar-list";
/// How much taller the header is with the server's banner behind it, and
/// how far down the banner's shade reaches.
const BANNER: f32 = 81.0;
/// The boost goal's row, and the pill in it: the pill 16 px under the
/// header and 12 px above the line below.
const BOOST_ROW: f32 = 55.0;
const BOOST_PILL: f32 = 26.0;
/// The boosts the goal counts to, as Discord's bar.
const BOOST_GOAL: u32 = 33;
/// Discord's boost pink, which the goal's fill fades in.
const BOOST_PINK: Color = Color {
    r: 1.0,
    g: 0x73 as f32 / 255.0,
    b: 0xfa as f32 / 255.0,
    a: 1.0,
};
/// The events row.
const EVENTS_ROW: f32 = 58.0;
/// A busy voice channel's card beside its row: its width, how far from
/// the row, and the people it names before "and N more".
const VOICE_CARD_WIDTH: f32 = 232.0;
const VOICE_CARD_GAP: f32 = 8.0;
const VOICE_CARD_PEOPLE: usize = 8;

/// The header's button that invites people, and the icon in it.
const INVITE: f32 = 32.0;

/// What's done in the sidebar.
#[derive(Clone, Debug)]
pub enum Event {
    Scrolled(Viewport),
    /// The pointer came onto a row or left it, by its key.
    Point(String, bool),
    /// The sidebar's edge was dragged this far from where it was pressed,
    /// let go, or double-clicked, which gives it its own width back.
    Resize(f32),
    Resized,
    ResetWidth,
    Select(String),
    /// A room in any place, such as a friend's voice channel from the DM
    /// home: goes to its place and shows it.
    Open(String),
    /// A voice channel: joins its call and shows it.
    Voice(String),
    /// A category's heading, by what it folds by.
    Toggle(String),
    Filter(String),
    /// The DM list's "Find or start a conversation" button, shown instead
    /// of the filter when the setting is on: the quick switcher.
    FindConversation,
    Friends,
    Requests,
    Browse,
    /// The server's name: its menu, below it.
    Header,
    /// The header's Invite people.
    Invite,
    /// A channel row's Create invite, by its room id.
    InviteTo(String),
    /// The events row: the server's scheduled events, under it.
    Events,
    /// A right click on a row, by its room id, where the pointer is.
    Menu(String, Point),
    /// Wave, in an empty DM, by its person.
    Wave(String),
    /// A DM's close button, by its room id.
    CloseDm(String),
    /// A voice channel's chat, without joining its call.
    OpenChat(String),
    /// The voice channel open shows its chat (true) or its call.
    VoiceChat(bool),
    /// A category's "+": a channel made in it.
    CreateIn(String),
    /// A server tree's row, by its key: its open and closed mark (or a
    /// category's line), and the rest of a place's line.
    TreeToggle(String),
    TreeActivate(String),
}

/// What's done in the sidebar.
pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Scrolled(viewport) => {
            app.sidebar.scroll = Scroll::of(viewport);
            return app.ask();
        }
        Event::Resize(by) => {
            let from = *app
                .sidebar
                .drag
                .get_or_insert(width(&app.layout, &app.theme));
            app.layout.sidebar_width = Some(room(app, from + by));
            return Task::none();
        }
        Event::Resized => {
            app.sidebar.drag = None;
            crate::layout::store(
                app.client.root(),
                "sidebar_width",
                width(&app.layout, &app.theme),
            );
            return app.ask();
        }
        Event::ResetWidth => {
            app.sidebar.drag = None;
            app.layout.sidebar_width = None;
            crate::layout::store(app.client.root(), "sidebar_width", None::<f32>);
            return app.ask();
        }
        Event::Point(id, entered) => {
            if !entered {
                app.sidebar.voice_card_off = false;
            }
            crate::app::point(&mut app.pointed.side, id, entered);
            // Where the pointer came onto the row, for a peek's height.
            app.peek.placing = entered && app.layout.peek_channels;
            // A DM's nameplate and moving decoration come as it's
            // pointed at; a peek follows the row.
            return Task::batch([app.ask(), crate::peek::follow(app)]);
        }
        // A server tree lists other places' channels too, which go to
        // their place first.
        Event::Select(id) if !in_scope(app, &id) => app.client.dispatch(Action::Open(id)),
        Event::Select(id) => app.client.dispatch(Action::SelectRoom(id)),
        Event::Open(id) => app.client.dispatch(Action::Open(id)),
        // The core's `SelectRoom` joins a voice channel's call first, and
        // shows it once in it.
        Event::Voice(id) => {
            app.sidebar.voice_card_off = true;
            if !in_scope(app, &id) {
                app.client.dispatch(Action::Open(id.clone()));
            }
            app.client.dispatch(Action::SelectRoom(id));
        }
        Event::TreeToggle(key) => return tree::toggle(app, &key),
        Event::TreeActivate(key) => return tree::activate(app, &key),
        Event::Toggle(fold) => {
            app.lists_shown = None;
            let Navigation::Server(server) = app.client.state().navigation.clone() else {
                return Task::none();
            };
            let collapsed = app.layout.collapsed.entry(server.clone()).or_default();
            if !collapsed.remove(&fold) {
                collapsed.insert(fold);
            }
            if collapsed.is_empty() {
                app.layout.collapsed.remove(&server);
            }
            crate::layout::store(app.client.root(), "collapsed", &app.layout.collapsed);
        }
        Event::Filter(filter) => {
            app.lists_shown = None;
            app.sidebar.filter = filter;
        }
        Event::FindConversation => {
            return app.switcher_event(crate::switcher::Event::Toggle);
        }
        Event::Friends => app.client.dispatch(Action::OpenDmScreen(DmScreen::Friends)),
        Event::Requests => app
            .client
            .dispatch(Action::OpenDmScreen(DmScreen::Requests)),
        Event::Browse => app.client.dispatch(Action::BrowseChannels),
        Event::Header => {
            let Some(server) = app.sidebar.header.server.clone() else {
                return Task::none();
            };
            if app.win.menu.as_ref().is_some_and(|menu| menu.header) {
                app.win.menu = None;
                return Task::none();
            }
            let package = app.themes.package_layout(&app.theme.id);
            let left = crate::conversation::sidebar_left(package, app.themes.chrome(&app.theme.id))
                + app.themes.glass_gap(&app.theme.id);
            let at = Point::new(left + INSET, HEADER - 4.0);
            return crate::place_menus::open_place_menu(app, &server, at, true);
        }
        Event::Invite | Event::InviteTo(_) => {
            // A channel's invite is to that channel, the header's to the
            // open one or the server's first it may invite to.
            let place = match event {
                Event::InviteTo(room) => room,
                _ => match app.sidebar.header.server.clone() {
                    Some(server) => server,
                    None => return Task::none(),
                },
            };
            app.client.dispatch(Action::OpenInvite(place));
            return Task::batch([
                app.sync(),
                iced::widget::operation::focus(crate::app::NOBODY),
            ]);
        }
        Event::Events => {
            let Some(server) = app.sidebar.header.server.clone() else {
                return Task::none();
            };
            let top = app.sidebar.top_height() - EVENTS_ROW;
            let package = app.themes.package_layout(&app.theme.id);
            let left = crate::conversation::sidebar_left(package, app.themes.chrome(&app.theme.id))
                + app.themes.glass_gap(&app.theme.id);
            let at = Point::new(left + INSET, top + EVENTS_ROW - INSET);
            return crate::place_menus::open_events_menu(app, &server, at);
        }
        Event::Menu(id, at) => {
            app.sidebar.voice_card_off = true;
            return crate::place_menus::open_place_menu(app, &id, at, false);
        }
        // A group's x leaves it, asking first, as Discord's does.
        Event::CloseDm(room) => {
            let leave = app
                .client
                .state()
                .menu_choices(&room)
                .contains(&MenuChoice::LeaveGroup);
            app.client.dispatch(if leave {
                Action::AskLeaveGroup(room)
            } else {
                Action::CloseDm(room)
            });
        }
        Event::Wave(user) => app.client.dispatch(Action::MessageFromProfile {
            user,
            text: "\u{1F44B}".to_owned(),
        }),
        Event::OpenChat(room) => app.client.dispatch(Action::OpenChat(room)),
        Event::VoiceChat(chat) => app.client.dispatch(Action::ShowVoiceChat(chat)),
        Event::CreateIn(category) => {
            return crate::place_menus::choose_place(app, category, MenuChoice::CreateChannel);
        }
    }
    app.sync()
}

/// Whether room `id` is listed where the window is.
fn in_scope(app: &App, id: &str) -> bool {
    let state = app.client.state();
    state
        .rooms
        .iter()
        .find(|room| room.id == id)
        .is_none_or(|room| state.in_scope(room))
}

impl<'a> View<'a> {
    fn server_rule(self, rule: yapfast_core::themes::lists::Rule) -> Element<'a, Message> {
        use yapfast_core::themes::lists::RuleStyle;

        let colour = crate::themes::argb(rule.colour);
        let available = (width(self.cx.layout, self.cx.look.theme) - rule.inset * 2.0).max(1.0);
        let line: Element<'a, Message> = match rule.style {
            RuleStyle::Solid => controls::bar(colour, Length::Fill, rule.width).into(),
            RuleStyle::Dashed | RuleStyle::Dotted => {
                let segment = if rule.style == RuleStyle::Dotted {
                    rule.width
                } else {
                    rule.width * 4.0
                };
                let gap = rule.width * 2.0;
                let count = (available / (segment + gap)).ceil().max(1.0) as usize;
                let mut pieces = row![].spacing(gap);
                for _ in 0..count {
                    pieces = pieces.push(
                        container(Space::new())
                            .width(segment)
                            .height(rule.width)
                            .style(move |_| {
                                rounded(
                                    colour,
                                    if rule.style == RuleStyle::Dotted {
                                        rule.width / 2.0
                                    } else {
                                        0.0
                                    },
                                )
                            }),
                    );
                }
                container(pieces).width(Length::Fill).clip(true).into()
            }
        };
        container(line)
            .padding(Padding {
                left: rule.inset,
                right: rule.inset,
                ..Padding::ZERO
            })
            .width(Length::Fill)
            .height(rule.width)
            .into()
    }

    pub fn sidebar_view(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let gap = self
            .cx
            .themes
            .channel_rows(&theme.id)
            .and_then(|rows| rows.gap)
            .unwrap_or(GAP);
        let reserve = self
            .cx
            .themes
            .channel_rows(&theme.id)
            .map_or(0.0, |rows| rows.reserve);
        let total: f32 = self
            .s
            .rows
            .iter()
            .map(|row| row.height(theme, gap))
            .sum::<f32>()
            + END
            - gap
            + reserve;
        let tree =
            crate::theme_chrome::tree(self.cx, yapfast_core::themes::chrome::TreeList::Channels);
        let viewport = self.s.scroll;
        // As in Slint, a theme's art at the list's foot sits at the bottom
        // of the sidebar while the list is shorter than it.
        let total = if reserve > 0.0 {
            total.max(viewport.height)
        } else {
            total
        };
        let mut rows = Rows::new(total);
        let mut top = 0.0;
        for (index, row) in self.s.rows.iter().enumerate() {
            let height = row.height(theme, gap);
            let at = top;
            top += height;
            if at + height < viewport.offset - MARGIN
                || at > viewport.offset + viewport.height + MARGIN
            {
                continue;
            }
            let place = self.s.places.of_row.get(index).copied().flatten();
            // As Slint, a list's separator lies over a row that follows
            // another, not under the last of a category.
            let after_room =
                index > 0 && matches!(self.s.rows.get(index - 1), Some(SideRow::Room(_)));
            let shown = self.side_row(row, place, after_room);
            // A theme's file tree: a channel after `├──`, or `└──` when
            // the next row isn't one of its category's.
            let shown = match row {
                SideRow::Room(room) if tree && !room.thread => {
                    let ends = !self.s.rows[index + 1..]
                        .iter()
                        .find(|next| !matches!(next, SideRow::Room(next) if next.thread))
                        .is_some_and(|next| matches!(next, SideRow::Room(_)));
                    crate::theme_chrome::tree_row(self.cx, ends, shown)
                }
                _ => shown,
            };
            rows.push(key(&row.key()), at, height, shown);
        }
        if reserve > 0.0
            && let Some(foot) = self
                .cx
                .themes
                .channel_rows(&theme.id)
                .and_then(|rows| rows.foot.as_ref())
            && let Some(art) = crate::theme_art::tile_art(
                self.cx,
                yapfast_core::themes::package::art::ArtSurface::Sidebar,
                foot,
                width(self.cx.layout, theme),
                reserve,
            )
        {
            rows.push(key("theme:channel-foot"), total - reserve, reserve, art);
        }
        let list_height = Length::Fill;
        let list = scroll_view(rows)
            .id(LIST)
            .on_scroll(|viewport| Message::Side(Event::Scrolled(viewport)))
            .height(list_height)
            .style(scrollbar(theme));
        // What's dragged follows the pointer, faint, over the list;
        // someone dragged from a voice channel, below it.
        let list: Element<'a, Message> = match self.moving_view() {
            Some((ghost, y)) => stack![list, controls::placed(ghost, 40.0, y)].into(),
            None => list.into(),
        };
        let list: Element<'a, Message> = match self.arranged_view() {
            Some((ghost, y)) => stack![
                list,
                controls::placed(
                    container(ghost).padding([0.0, INSET]),
                    0.0,
                    y.clamp(
                        0.0,
                        (viewport.height - theme.spacing().channel_row_height.unwrap_or(ROW))
                            .max(0.0),
                    )
                )
            ]
            .into(),
            None => list,
        };
        // A sidebar with a theme's art or glass shows it: its own ground is
        // the surface's, under the art (as Slint's transparent sidebar).
        let ground = if self.cx.themes.decorates(
            &theme.id,
            yapfast_core::themes::package::art::ArtSurface::Sidebar,
        ) || self
            .cx
            .themes
            .glass(
                &theme.id,
                yapfast_core::themes::package::art::ArtSurface::Sidebar,
            )
            .is_some()
        {
            Color::TRANSPARENT
        } else {
            theme.sidebar
        };
        let header = self.side_header_view();
        let header: Element<'a, Message> = match self
            .cx
            .themes
            .channel_rows(&theme.id)
            .and_then(|rows| rows.server_rule)
        {
            Some(rule) => stack![
                header,
                container(self.server_rule(rule))
                    .height(Length::Fill)
                    .align_y(Alignment::End)
            ]
            .into(),
            None => header,
        };
        let mut top = column![header];
        if let Some(boosts) = self
            .s
            .header
            .boosts
            .filter(|_| self.s.header.server.is_some())
        {
            top = top.push(self.boost_goal(boosts));
        }
        if let Some(events) = self
            .s
            .header
            .events
            .filter(|_| self.s.header.server.is_some())
        {
            top = top.push(self.events_row(events));
        }
        let corner = self.cx.look.theme.radius(12.0);
        container(column![top, list])
            .width(width(self.cx.layout, self.cx.look.theme))
            .height(list_height)
            .clip(true)
            // Its top corner by the theme's radii (square on Zine), as
            // Slint's `Look.radius`.
            .style(move |_| rounded(ground, corners([corner, 0.0, 0.0, 0.0])))
            .into()
    }
}
