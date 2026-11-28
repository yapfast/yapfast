//! The member list beside a server channel's timeline: the Activity panel
//! of Discord's feed over it, then each group's heading and its members
//! with their looks (`looks::MemberLook`), virtualized. A click opens
//! someone's card, a right click their menu (`person_menu.rs`).

use crate::widgets::scroll_view;
use iced::{
    Alignment, Color, Element, Length, Padding, Point, Rectangle, Size, Task, Vector,
    widget::{
        Space, column, container, mouse_area, row, scrollable::Viewport, stack, text, tooltip,
    },
};
use yapfast_core::{
    application::{Action, AppState},
    chat::{ActivityKind, ChatMember, Presence},
    i18n::{self, tr},
    themes::content::Nameplates,
};

use crate::{
    app::{App, Message, key, point},
    controls::{self, bar, lit_style},
    ctx::Ctx,
    fonts::{MEDIUM, REGULAR, SEMIBOLD},
    header,
    looks::{self, Chip, MemberLine, MemberLook},
    people::StatusInk,
    people::{NameInk, Person},
    pictures::{Face, Icon},
    profile,
    style::{
        CHIP, CROWN, HEADING, MEMBER_HOVER, MEMBER_OPEN, faded, fill, hex, rounded, scrollbar,
    },
    sync::{MARGIN, Scroll},
    theme::rgb,
    widgets::{Pic, PlateRow, RightPress, Rows},
};

/// The member list: the members as shown, which change at most once per
/// `ROSTER_PAUSE`, their lines and what their looks were made with, the
/// Activity panel's cards over them, and the list's viewport.
pub struct Roster {
    pub members: Option<std::sync::Arc<Vec<yapfast_core::chat::ChatMember>>>,
    pub lines: Vec<crate::looks::MemberLine>,
    pub made: Option<crate::looks::LooksMadeWith>,
    /// When the members shown were taken, and whether newer ones wait for
    /// the pause to end.
    pub shown: Option<std::time::Instant>,
    pub waiting: bool,
    /// The Activity panel's cards, and whether it's opened past its first
    /// three.
    pub activity_cards: Vec<ActivityCard>,
    pub activity_open: bool,
    /// The feed and its people the cards were made from, held so a new
    /// one is told by its address.
    pub activity_for: (
        std::sync::Arc<[yapfast_core::chat::FeedEntry]>,
        Option<std::sync::Arc<[yapfast_core::chat::FeedPerson]>>,
    ),
    pub scroll: crate::sync::Scroll,
    /// The header's toggle hid the list.
    pub hidden: bool,
    /// A collapsed theme strip expands while the pointer is over it.
    pub collapsed_open: bool,
    /// The width the list had as its edge was pressed, while it's dragged.
    pub drag: Option<f32>,
    /// The grid tile last pressed, from the press (`App::pressed_spot`):
    /// its card opens level with it, as Slint's from the tile's own place.
    pub tile_spot: std::cell::Cell<iced::Rectangle>,
    /// The room a theme's grid was last laid out in, which its pictures
    /// are asked for by.
    pub grid_room: std::cell::Cell<f32>,
    /// A collapsed theme strip's viewport.
    pub strip: crate::sync::Scroll,
}

/// The member list beside the conversation, while it shows.
pub fn view<'a>(roster: &'a Roster, cx: Ctx<'a>) -> Element<'a, Message> {
    View { s: roster, cx }.members_view()
}

/// The member list's view: the roster and the view context.
#[derive(Clone, Copy)]
struct View<'a> {
    s: &'a Roster,
    cx: Ctx<'a>,
}

enum GridLine<'a> {
    Heading(&'a str, Option<&'a str>),
    Members(Vec<&'a MemberLook>),
}

/// A grid's measures at `room` wide: as many columns as whole tiles, their
/// frames and their shadows fit, up to the theme's; the tiles' and
/// headings' heights and the gap after each; the margins; and how far the
/// tiles' shadows reach right and down.
struct Grid {
    columns: usize,
    tile_height: f32,
    gap: f32,
    heading_height: f32,
    left: f32,
    right: f32,
    reach_x: f32,
    reach_y: f32,
}

impl Grid {
    fn at(
        theme: &crate::theme::Theme,
        package: &yapfast_core::themes::layout::Layout,
        room: f32,
    ) -> Self {
        let themed = package.member_rows.as_ref();
        let gap = themed.and_then(|rows| rows.gap).unwrap_or(4.0);
        let (left, right) = themed.and_then(|rows| rows.margin).unwrap_or((8.0, 8.0));
        // What a tile's shadow reaches past its frame, on the right and
        // below, kept inside the list.
        let outlined = themed.is_some_and(|rows| rows.tile_outline);
        let shadow = if outlined {
            theme.surface_shadow("tile", iced::Shadow::default())
        } else {
            iced::Shadow::default()
        };
        let reach_x = (shadow.offset.x.max(0.0) + shadow.blur_radius).ceil();
        let reach_y = (shadow.offset.y.max(0.0) + shadow.blur_radius).ceil();
        let border = if outlined {
            theme
                .surface_border("tile", 8.0, 0.0, Color::TRANSPARENT)
                .width
        } else {
            0.0
        };
        // A tile needs its face and a little room each side of it.
        let least = package.grid.size + 16.0 + 2.0 * border;
        let inner = (room - left - right - reach_x).max(0.0);
        let fits = ((inner + gap) / (least + gap)).floor().max(1.0) as usize;
        Self {
            columns: fits.min(package.grid.columns.max(1) as usize),
            tile_height: themed
                .and_then(|rows| rows.height)
                .unwrap_or(package.grid.size + 34.0),
            gap,
            heading_height: themed.and_then(|rows| rows.headers.height).unwrap_or(42.0),
            left,
            right,
            reach_x,
            reach_y,
        }
    }
}

/// The grid's lines: each heading, and its members `columns` to a row.
fn grid_lines(lines: &[MemberLine], columns: usize) -> Vec<GridLine<'_>> {
    let mut visual = Vec::new();
    let mut pending = Vec::with_capacity(columns);
    for line in lines {
        match line {
            MemberLine::Heading { text, icon } => {
                if !pending.is_empty() {
                    visual.push(GridLine::Members(std::mem::take(&mut pending)));
                }
                visual.push(GridLine::Heading(text, icon.as_deref()));
            }
            MemberLine::Member(look) => {
                pending.push(look.as_ref());
                if pending.len() == columns {
                    visual.push(GridLine::Members(std::mem::take(&mut pending)));
                }
            }
        }
    }
    if !pending.is_empty() {
        visual.push(GridLine::Members(pending));
    }
    visual
}

/// Whether a grid line at `at`, `height` tall, is built: in view of
/// `scroll` or [`MARGIN`] beyond.
fn grid_line_in_view(at: f32, height: f32, scroll: &crate::sync::Scroll) -> bool {
    at + height >= scroll.offset - MARGIN && at <= scroll.offset + scroll.height + MARGIN
}

/// The strip's face pitch and the room over its first face.
const STRIP_PITCH: f32 = 46.0;
const STRIP_TOP: f32 = 8.0;

/// The strip's height with `count` faces: 40 px each, 6 px apart, 8 px
/// over and under them.
fn strip_total(count: usize) -> f32 {
    (STRIP_TOP + count as f32 * STRIP_PITCH + STRIP_TOP - 6.0).max(0.0)
}

/// The strip's faces in view of `scroll`, and [`MARGIN`] beyond. Its
/// offset is held to the faces there are, as the scrollable holds it: a
/// strip that fits never reports, so one left far down another server's
/// would show none.
fn strip_range(scroll: &Scroll, count: usize) -> (usize, usize) {
    let most = (strip_total(count) - scroll.height).max(0.0);
    Scroll {
        offset: scroll.offset.min(most),
        height: scroll.height,
    }
    .range(count, STRIP_PITCH, STRIP_TOP)
}

/// The member lines whose pictures show: those the list, the grid or the
/// collapsed strip builds, as each lays them out.
pub fn lines_shown(app: &App) -> Vec<&MemberLine> {
    let lines = &app.roster.lines;
    let package = app.themes.package_layout(&app.theme.id);
    if package.and_then(|layout| layout.members)
        == Some(yapfast_core::themes::layout::MembersSide::Collapsed)
        && !app.roster.collapsed_open
    {
        let members: Vec<&MemberLine> = lines
            .iter()
            .filter(|line| matches!(line, MemberLine::Member(_)))
            .collect();
        let (first, last) = strip_range(&app.roster.strip, members.len());
        return members[first..last].to_vec();
    }
    if let Some(package) = package.filter(|layout| {
        layout.members_style == Some(yapfast_core::themes::layout::MembersStyle::Grid)
    }) {
        // The room the grid was last laid out in, else the list's less
        // its outline.
        let room = Some(app.roster.grid_room.get())
            .filter(|room| *room > 0.0)
            .unwrap_or_else(|| width(&app.layout, &app.theme) - 1.0);
        let grid = Grid::at(&app.theme, package, room);
        // Each grid line's first member line, to give back the lines it
        // holds.
        let mut shown = Vec::new();
        let mut top = TOP;
        let mut at = 0;
        for line in grid_lines(lines, grid.columns) {
            let (height, count) = match &line {
                GridLine::Heading(..) => (grid.heading_height, 1),
                GridLine::Members(members) => (grid.tile_height, members.len()),
            };
            if grid_line_in_view(top, height, &app.roster.scroll) {
                shown.extend(&lines[at..at + count]);
            }
            top += height + grid.gap;
            at += count;
        }
        return shown;
    }
    let tops = Tops::of(&app.theme, app.themes.member_rows(&app.theme.id), lines);
    let (first, last) = tops.in_view(&app.roster.scroll);
    lines[first..last].iter().collect()
}

impl Roster {
    /// How many of the Activity panel's cards show: none while Discord's
    /// "Hide Activity Cards" is on, all once it's opened, else the first
    /// three.
    pub fn cards_shown(&self, state: &yapfast_core::application::AppState) -> usize {
        if state.activity_cards_hidden {
            0
        } else if self.activity_open {
            self.activity_cards.len()
        } else {
            self.activity_cards.len().min(FIRST_CARDS)
        }
    }

    /// The Activity panel's height over a list that shows (`shown`),
    /// which the list starts below.
    pub fn activity_height(&self, state: &yapfast_core::application::AppState, shown: bool) -> f32 {
        if self.activity_cards.is_empty() || !shown {
            return 0.0;
        }
        PANEL_TOP + ACTIVITY_HEAD + self.cards_shown(state) as f32 * CARD_PITCH + PANEL_BOTTOM
    }

    pub fn new(height: f32) -> Self {
        Self {
            members: None,
            lines: Vec::new(),
            made: None,
            shown: None,
            waiting: false,
            activity_cards: Vec::new(),
            activity_open: false,
            activity_for: (std::sync::Arc::from([]), None),
            scroll: crate::sync::Scroll::new(height),
            hidden: false,
            collapsed_open: false,
            drag: None,
            tile_spot: Default::default(),
            grid_room: Default::default(),
            strip: crate::sync::Scroll::new(height),
        }
    }
}

pub const WIDTH: f32 = 265.0;
/// The least and the most its edge drags it to.
const LEAST: f32 = 200.0;
const MOST: f32 = 480.0;

/// The member list's width: as its edge was dragged, else [`WIDTH`].
pub fn width(layout: &crate::layout::Layout, theme: &crate::theme::Theme) -> f32 {
    layout
        .members_width
        .or(theme.spacing().members_width)
        .map_or(WIDTH, |width| width.clamp(LEAST, MOST))
}

pub fn docked_width(app: &App) -> f32 {
    let collapsed = app
        .themes
        .package_layout(&app.theme.id)
        .and_then(|layout| layout.members)
        == Some(yapfast_core::themes::layout::MembersSide::Collapsed);
    if collapsed && !app.roster.collapsed_open {
        56.0
    } else {
        width(&app.layout, &app.theme)
    }
}

/// `width` within what the list may take: 200 to 480 px, and no more than
/// leaves the conversation 420 px.
fn room(app: &App, width: f32) -> f32 {
    // `chat_left`/`chat_right` describe the chat after the currently drawn
    // member list has taken its share. Put that share back before working
    // out how wide the dragged list may become. This is the same calculation
    // for a list docked on either side of the chat.
    let current = self::width(&app.layout, &app.theme);
    let most = crate::conversation::chat_right(app) - crate::conversation::chat_left(app) + current
        - crate::thread::CHAT_KEPT;
    width.clamp(LEAST, most.clamp(LEAST, MOST))
}
/// Where each line starts after the last, and the room above the first.
pub const PITCH: f32 = 44.0;
pub const TOP: f32 = 8.0;
/// A line's height.
const ROW: f32 = 42.0;
/// How far a member's presence dot sits past its avatar's corner.
const DOT_OUT: f32 = 4.0;

fn row_height(theme: &crate::theme::Theme) -> f32 {
    theme.spacing().member_row_height.unwrap_or(ROW)
}

pub(crate) fn pitch(theme: &crate::theme::Theme, gap: Option<f32>) -> f32 {
    row_height(theme) + gap.unwrap_or(PITCH - ROW)
}

/// Where the list's lines start. Every line one pitch after the last
/// (stock, and a theme whose headings keep the rows' pitch) is worked out
/// for the lines in view alone, as a big server's list has thousands; each
/// line's own top is kept only when a theme's headings take a height of
/// their own.
pub(crate) enum Tops {
    Even { pitch: f32, count: usize },
    Each(Vec<f32>),
}

impl Tops {
    pub(crate) fn new(lines: &[MemberLine], pitch: f32, heading: Option<f32>) -> Self {
        match heading.filter(|height| height + 2.0 != pitch) {
            None => Self::Even {
                pitch,
                count: lines.len(),
            },
            Some(_) => Self::Each(line_tops(lines, pitch, heading)),
        }
    }

    /// The list's tops under the theme's member rows.
    pub(crate) fn of(
        theme: &crate::theme::Theme,
        rows: Option<&yapfast_core::themes::lists::MemberRows>,
        lines: &[MemberLine],
    ) -> Self {
        Self::new(
            lines,
            pitch(theme, rows.and_then(|rows| rows.gap)),
            heading_height(rows),
        )
    }

    /// Where line `index` starts; the list's end at its count.
    pub(crate) fn top(&self, index: usize) -> f32 {
        match self {
            Self::Even { pitch, .. } => TOP + index as f32 * pitch,
            Self::Each(tops) => tops[index],
        }
    }

    /// Where the last line ends.
    pub(crate) fn end(&self) -> f32 {
        match self {
            Self::Even { count, .. } => self.top(*count),
            Self::Each(tops) => tops.last().copied().unwrap_or(TOP),
        }
    }

    /// The lines in view of `scroll`, and [`MARGIN`] beyond.
    pub(crate) fn in_view(&self, scroll: &crate::sync::Scroll) -> (usize, usize) {
        match self {
            Self::Even { pitch, count } => scroll.range(*count, *pitch, TOP),
            Self::Each(tops) => lines_in_view(tops, scroll),
        }
    }
}

/// What a member's row takes from the lines over it, as a theme draws it:
/// how many groups have begun, the last one's heading, the member's place
/// in it from 0, and their place among those online from 1 (0 while
/// offline). Walked down the lines once, not again for each row.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Walk<'l> {
    groups: usize,
    heading: &'l str,
    place: usize,
    online: usize,
}

impl<'l> Walk<'l> {
    /// The walk after `lines`.
    fn to(lines: &'l [MemberLine]) -> Self {
        let mut walk = Self::default();
        for line in lines {
            walk.step(line);
        }
        walk
    }

    /// Takes `line` in, and gives what it takes from those over it.
    fn step(&mut self, line: &'l MemberLine) -> Self {
        match line {
            MemberLine::Heading { text, .. } => {
                self.groups += 1;
                self.heading = text;
                self.place = 0;
                *self
            }
            MemberLine::Member(look) => {
                let mut at = *self;
                if look.offline() {
                    at.online = 0;
                } else {
                    self.online += 1;
                    at.online = self.online;
                }
                self.place += 1;
                at
            }
        }
    }
}

/// Where each of the list's lines starts, and its end: a row takes the
/// pitch, a heading its theme's own height (Zine's 42 px over 56 px rows,
/// as Slint lays them) and the 2 px Slint keeps between it and the first
/// member under it, else the pitch.
fn line_tops(lines: &[MemberLine], pitch: f32, heading: Option<f32>) -> Vec<f32> {
    let mut tops = Vec::with_capacity(lines.len() + 1);
    let mut top = TOP;
    tops.push(top);
    for line in lines {
        top += match line {
            MemberLine::Heading { .. } => heading.map_or(pitch, |height| height + 2.0),
            MemberLine::Member(_) => pitch,
        };
        tops.push(top);
    }
    tops
}

/// The lines in view of `scroll`, from [`line_tops`].
fn lines_in_view(tops: &[f32], scroll: &crate::sync::Scroll) -> (usize, usize) {
    let count = tops.len().saturating_sub(1);
    let from = scroll.offset - crate::sync::MARGIN;
    let to = scroll.offset + scroll.height + crate::sync::MARGIN;
    let first = tops[1..].partition_point(|end| *end <= from).min(count);
    let last = tops[..count]
        .partition_point(|start| *start < to)
        .max(first);
    (first, last)
}

/// A theme's one ink for the list's names (`ink.name`): over everyone's
/// own colour only while the person lets the theme style people's names,
/// else only over the stock ink of a name with no colour of its own.
fn list_name_ink(
    cx: Ctx<'_>,
    rows: Option<&yapfast_core::themes::lists::MemberRows>,
    look: &MemberLook,
) -> Option<Color> {
    let ink = rows?.name_ink?;
    let own = look.color.is_some()
        || look
            .style
            .as_ref()
            .is_some_and(|style| !style.colors.is_empty());
    takes_name_ink(own, cx.theme_content().is_some()).then(|| crate::themes::argb(ink))
}

/// Whether a theme's list ink goes over a name: one with no colour of its
/// own, or any while the theme may restyle people's names.
fn takes_name_ink(own_colour: bool, content_allowed: bool) -> bool {
    !own_colour || content_allowed
}

/// Whether a member's row keeps their avatar decoration: a theme that draws
/// presence as words or a glyph leaves it out only where the person lets
/// it restyle people's avatars.
fn keeps_decoration(presence_dot: bool, content_allowed: bool) -> bool {
    presence_dot || !content_allowed
}

/// Where a member's presence dot sits on a face `face` wide: `dot` wide
/// (16 px stock), `out` past its corner (Discord's 4 px, at 20, 20).
fn dot_at(face: f32, dot: Option<f32>, out: Option<f32>) -> Point {
    let at = face - dot.unwrap_or(16.0) + out.unwrap_or(DOT_OUT);
    Point::new(at, at)
}

/// A themed list's heading height, when it has its own.
pub(crate) fn heading_height(
    theme_rows: Option<&yapfast_core::themes::lists::MemberRows>,
) -> Option<f32> {
    theme_rows.and_then(|rows| rows.headers.height)
}
/// Discord's boosting pink, the same in every theme.
const BOOST: u32 = 0xff73fa;
/// The Activity panel: its header, a card and the room each card keeps
/// under it, the room above and below the panel, and how many cards show
/// until it's opened.
const ACTIVITY_HEAD: f32 = 32.0;
const CARD: f32 = 68.0;
const CARD_PITCH: f32 = 76.0;
const PANEL_TOP: f32 = 8.0;
const PANEL_BOTTOM: f32 = 4.0;
const FIRST_CARDS: usize = 3;

/// What's done in the member list and its Activity panel.
#[derive(Clone, Debug)]
pub enum Event {
    /// The list's viewport, as its scrollable reports it.
    Scrolled(Viewport),
    /// A collapsed theme strip's viewport.
    StripScrolled(Viewport),
    /// The strip's size as laid out, which a strip whose faces all fit
    /// never says by scrolling.
    StripSized(iced::Size),
    /// The pointer came onto or left a member's row, by their id.
    Point(String, bool),
    /// A member's line was clicked, by their id.
    Open(String),
    /// The held member list's changes are due.
    Due,
    /// The header's toggle: hide the list, or show it.
    Toggle,
    /// The list's edge was dragged this far from where it was pressed, let
    /// go, or double-clicked, which gives it its own width back.
    Resize(f32),
    Resized,
    ResetWidth,
    /// The Activity panel's header: open or close it (or show the cards
    /// again).
    ToggleActivity,
    /// The Activity panel's eye: hide the cards, or show them.
    HideActivity,
    /// A card of the Activity panel was clicked, by whose it is: the
    /// cards may have moved since.
    OpenActivity(String),
    /// The pointer entered or left a collapsed theme member strip.
    Collapsed(bool),
}

/// A card of the Activity panel, from Discord's feed.
#[derive(Clone, Debug, PartialEq)]
pub struct ActivityCard {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    /// The game, artist or show, the words about when, and its picture.
    pub game: String,
    pub since: String,
    pub thumb: Option<String>,
    /// The kind's icon, as `Icon::activity` takes it.
    pub icon: u8,
    /// Going on now, which its icon and time show in green.
    pub live: bool,
}

/// How many of the Activity panel's cards show.
/// The Activity panel's height, which the list starts below.
pub fn activity_height(app: &App) -> f32 {
    if app
        .themes
        .member_rows(&app.theme.id)
        .is_some_and(|rows| !rows.activity)
    {
        return 0.0;
    }
    app.roster
        .activity_height(app.client.state(), crate::header::members_shown(app))
}

/// What's done in the member list and its Activity panel.
pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Scrolled(viewport) => {
            app.roster.scroll = Scroll::of(viewport);
            app.ask()
        }
        Event::StripScrolled(viewport) => {
            app.roster.strip = Scroll::of(viewport);
            app.ask()
        }
        Event::StripSized(size) => {
            app.roster.strip.height = size.height;
            app.ask()
        }
        Event::Point(id, entered) => {
            point(&mut app.pointed.member, id, entered);
            app.ask()
        }
        Event::Open(id) => open_member(app, &id),
        Event::Resize(by) => {
            let from = *app
                .roster
                .drag
                .get_or_insert(width(&app.layout, &app.theme));
            app.layout.members_width = Some(room(app, from - by));
            Task::none()
        }
        Event::Resized => {
            app.roster.drag = None;
            crate::layout::store(
                app.client.root(),
                "members_width",
                width(&app.layout, &app.theme),
            );
            app.ask()
        }
        Event::ResetWidth => {
            app.roster.drag = None;
            app.layout.members_width = None;
            crate::layout::store(app.client.root(), "members_width", None::<f32>);
            app.ask()
        }
        Event::Due => {
            app.roster.waiting = false;
            app.show_roster(true);
            app.ask()
        }
        Event::Toggle => {
            app.roster.hidden = !app.roster.hidden;
            // Hidden, the list lets its lines and pictures go.
            Task::batch([app.sync(), app.ask()])
        }
        Event::Collapsed(open) => {
            app.roster.collapsed_open = open;
            app.ask()
        }
        Event::ToggleActivity => {
            if app.client.state().activity_cards_hidden {
                return hide_activity(app, false);
            }
            app.roster.activity_open = !app.roster.activity_open;
            app.ask()
        }
        Event::HideActivity => {
            let hidden = !app.client.state().activity_cards_hidden;
            hide_activity(app, hidden)
        }
        Event::OpenActivity(id) => {
            let Some((index, card)) = app
                .roster
                .activity_cards
                .iter()
                .enumerate()
                .find(|(_, card)| card.id == id)
            else {
                return Task::none();
            };
            let top = header::HEIGHT + 1.0 + PANEL_TOP + ACTIVITY_HEAD + index as f32 * CARD_PITCH;
            let left = app.window.width - width(&app.layout, &app.theme);
            let at = Point::new(left - 8.0 - profile::WIDTH, top - 16.0);
            app.open_card(card.id.clone(), at)
        }
    }
}

/// Hides the Activity panel's cards or shows them, and keeps it.
fn hide_activity(app: &mut App, hidden: bool) -> Task<Message> {
    crate::layout::store(app.client.root(), "activity_cards_hidden", hidden);
    app.client.dispatch(Action::ActivityCardsHidden(hidden));
    Task::batch([app.sync(), app.ask()])
}

/// Makes the Activity panel's cards again from the core's feed, keeping
/// them when they come out the same.
pub fn make_activity_cards(app: &mut App) {
    let state = app.client.state();
    let cards = match (&state.selected, &app.roster.members) {
        (Some(room), Some(members)) if app.server => {
            activity_cards(state, room, members, yapfast_core::clock::now_ms())
        }
        _ => Vec::new(),
    };
    if cards != app.roster.activity_cards {
        app.roster.activity_cards = cards;
    }
}

/// A member's line was clicked: their card opens beside the list, its
/// top 16 px above the row's.
pub fn open_member(app: &mut App, id: &str) -> Task<Message> {
    // Found by id: the lines may have moved since the click.
    let Some((index, look)) = app
        .roster
        .lines
        .iter()
        .enumerate()
        .find_map(|(index, line)| match line {
            MemberLine::Member(look) if look.id == id => Some((index, look)),
            _ => None,
        })
    else {
        return Task::none();
    };
    let tops = Tops::of(
        &app.theme,
        app.themes.member_rows(&app.theme.id),
        &app.roster.lines,
    );
    let package = app.themes.package_layout(&app.theme.id);
    let grid = package.and_then(|layout| layout.members_style)
        == Some(yapfast_core::themes::layout::MembersStyle::Grid);
    let tile = app.roster.tile_spot.get();
    let top = if grid && tile.width > 0.0 {
        // A grid's tiles sit in rows of their own: the tile pressed.
        app.pressed_spot(tile).y
    } else {
        header::HEIGHT + 1.0 + activity_height(app) + tops.top(index) - app.roster.scroll.offset
    };
    // Beside the list where it's laid out, as Slint's: a theme's right
    // rail and its panes' gap move it too.
    let left = app.window.width
        - width(&app.layout, &app.theme)
        - if package.and_then(|layout| layout.members)
            == Some(yapfast_core::themes::layout::MembersSide::Left)
        {
            0.0
        } else {
            crate::conversation::right_rail(package, app.themes.chrome(&app.theme.id))
                + app.themes.glass_gap(&app.theme.id)
        };
    let at = Point::new(left - 8.0 - profile::WIDTH, top - 16.0);
    let user = look.id.clone();
    app.open_card(user, at)
}

/// The pictures the Activity panel's cards show.
pub fn activity_pictures(app: &App) -> Vec<&str> {
    app.roster
        .activity_cards
        .iter()
        .take(app.roster.cards_shown(app.client.state()))
        .flat_map(|card| [card.avatar.as_deref(), card.thumb.as_deref()])
        .flatten()
        .collect()
}

/// How far a nameplate's palette reaches over its row: further pointed at,
/// furthest selected.
pub fn plate_end(pointed: bool, selected: bool) -> f32 {
    if selected {
        0.5
    } else if pointed {
        0.3
    } else {
        0.2
    }
}

/// What the channel's Activity panel shows: Discord's feed, in its order,
/// of those `state.activity_people` says it may show. Between two channels
/// of a server the last answer shows until this one's comes.
pub fn activity_cards(
    state: &AppState,
    room: &str,
    members: &[ChatMember],
    now_ms: u64,
) -> Vec<ActivityCard> {
    use yapfast_core::{
        chat::FeedEntry, discord::parse_room, yapfast_discord::activity_feed::Kind,
    };
    let Some((asked, people)) = &state.activity_people else {
        return Vec::new();
    };
    let server = |room| parse_room(room).map(|(server, _)| server);
    if server(asked) != server(room) {
        return Vec::new();
    }
    // What their presence says of it, which is more than the feed does.
    let doing = |member: &ChatMember, entry: &FeedEntry| {
        member
            .activities
            .iter()
            .find(|activity| match entry.kind {
                Kind::Game | Kind::Launched => {
                    activity.kind == ActivityKind::Playing && activity.name == entry.title
                }
                Kind::Listened => activity.kind == ActivityKind::Listening,
                Kind::Watched => activity.kind == ActivityKind::Watching,
                Kind::TopGame | Kind::TopArtist => false,
            })
            .cloned()
    };
    state
        .activity_feed
        .iter()
        .filter_map(|entry| {
            let person = people.iter().find(|person| person.id == entry.author)?;
            let live = entry.live_at(now_ms);
            let member = live
                .then(|| members.iter().find(|member| *member.id == *entry.author))
                .flatten();
            // Discord's client leaves out what someone offline "is" doing.
            if member.is_some_and(|member| member.presence == Some(Presence::Offline)) {
                return None;
            }
            let activity = member.and_then(|member| doing(member, entry));
            let since = match (entry.hours, entry.tracks) {
                (Some(hours), _) if entry.kind == Kind::TopGame => {
                    i18n::fill(tr("Most played: {}h"), &[&hours])
                }
                (_, Some(tracks)) if entry.kind == Kind::TopArtist => {
                    i18n::fill(tr("{} tracks this week"), &[&tracks])
                }
                _ if live => running_for(
                    activity
                        .as_ref()
                        .and_then(|activity| activity.start)
                        .unwrap_or(entry.at_ms),
                    now_ms,
                ),
                _ => finished_ago(entry.at_ms, now_ms),
            };
            let picture = activity
                .as_ref()
                .and_then(|activity| activity.large_image.as_deref())
                .or(entry.picture.as_deref())
                .unwrap_or_default();
            let thumb = yapfast_core::yapfast_discord::activity::small_picture(picture);
            Some(ActivityCard {
                id: person.id.to_string(),
                name: person.name.to_string(),
                avatar: (!person.avatar.is_empty()).then(|| person.avatar.to_string()),
                icon: match entry.kind {
                    Kind::Game | Kind::Launched => 1,
                    Kind::Listened => 2,
                    Kind::Watched => 3,
                    Kind::TopGame | Kind::TopArtist => 5,
                },
                live,
                game: entry.title.to_string(),
                since,
                thumb: (!thumb.is_empty()).then(|| thumb.to_string()),
            })
        })
        .collect()
}

/// A running time as the activity panel words it: "for 12 min".
fn running_for(started_ms: u64, now_ms: u64) -> String {
    let minutes = now_ms.saturating_sub(started_ms) / 60_000;
    match minutes {
        0 => tr("just started").to_owned(),
        1..=59 => i18n::fill(tr("for {} min"), &[&minutes]),
        60..=1439 => i18n::fill(tr("for {} hr"), &[&(minutes / 60)]),
        _ => i18n::fill(tr("for {} d"), &[&(minutes / 1440)]),
    }
}

/// How long ago something finished as the activity panel words it, in
/// Discord's steps: "3h ago", "2d ago".
pub fn finished_ago(at_ms: u64, now_ms: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    let seconds = now_ms.saturating_sub(at_ms) / 1000;
    match seconds {
        0..MINUTE => i18n::fill(tr("{}s ago"), &[&seconds]),
        MINUTE..HOUR => i18n::fill(tr("{}m ago"), &[&(seconds / MINUTE)]),
        _ if seconds < 12 * HOUR => i18n::fill(tr("{}h ago"), &[&(seconds / HOUR)]),
        _ if seconds < 9 * DAY => i18n::fill(tr("{}d ago"), &[&(seconds / DAY).max(1)]),
        _ if seconds < 28 * DAY => i18n::fill(tr("{}w ago"), &[&(seconds / (7 * DAY))]),
        _ => i18n::fill(tr("{}mo ago"), &[&(seconds / (30 * DAY)).max(1)]),
    }
}

/// The account's own nameplate behind its card or a tile `height` tall,
/// with its palette's colour for the theme: .4 strong, .6 when `hot`,
/// playing when `play`. None without one, or with Show nameplates off.
fn nameplate_policy(ctx: crate::ctx::Ctx<'_>) -> Nameplates {
    ctx.themes
        .content(
            &ctx.look.theme.id,
            ctx.layout.theme_content.contains(&ctx.look.theme.id),
        )
        .map_or(Nameplates::Keep, |content| content.names.nameplates)
}

pub(crate) fn show_nameplate(policy: Nameplates, hidden: bool) -> bool {
    !hidden && policy != Nameplates::Hide
}

fn policy_palette(policy: Nameplates, palette: Option<Color>, accent: Color) -> Option<Color> {
    match policy {
        Nameplates::Keep => palette,
        Nameplates::Hide => None,
        Nameplates::Tint => Some(accent),
    }
}

/// A nameplate's palette after the opted-in theme's Hide/Tint policy. Tint
/// gives the row wash the theme accent; the plate image itself still uses
/// the renderer's stock image path.
pub fn themed_nameplate_palette(ctx: crate::ctx::Ctx<'_>, palette: Option<Color>) -> Option<Color> {
    policy_palette(nameplate_policy(ctx), palette, ctx.look.theme.accent)
}

pub fn account_plate<'a>(
    ctx: crate::ctx::Ctx<'a>,
    height: f32,
    hot: bool,
    play: bool,
) -> Option<(Element<'a, Message>, Option<Color>)> {
    let state = ctx.state;
    let policy = nameplate_policy(ctx);
    let plate = state
        .account_nameplate
        .as_ref()
        .filter(|_| show_nameplate(policy, state.hide_nameplates))?;
    let picture = plate_picture(
        ctx,
        &plate.picture,
        height,
        8.0,
        if hot { 0.6 } else { 0.4 },
        play,
    )?;
    let palette = plate
        .colors
        .map(|[dark, light]| hex(if ctx.look.theme.dark { dark } else { light }));
    Some((
        picture,
        policy_palette(policy, palette, ctx.look.theme.accent),
    ))
}

/// A nameplate's picture at a row's right end, `height` tall with the
/// row's corners of `radius`, at `strength`, its moving picture asked for
/// and played when `play`.
fn plate_picture<'a>(
    ctx: crate::ctx::Ctx<'a>,
    url: &str,
    height: f32,
    radius: f32,
    strength: f32,
    play: bool,
) -> Option<Element<'a, Message>> {
    let picture = ctx.state.pictures.get(url)?;
    let width = (height * picture.width as f32 / picture.height.max(1) as f32).round();
    let size = Size::new(width, height);
    let moving = play
        .then(|| {
            ctx.moving(
                &looks::animated_url(url),
                size,
                Rectangle::with_size(size),
                false,
            )
        })
        .flatten();
    Some(match moving {
        Some(moving) => moving.radius(radius).opacity(strength).into(),
        None => Element::from(
            Pic::new(
                ctx.fit(picture, size.width as u32, size.height as u32, false)?,
                size,
            )
            .radius(radius)
            .opacity(strength),
        ),
    })
}

/// A nameplate's picture behind a row `height` tall whose corners are of
/// `radius`, playing while the row is pointed at, stronger pointed at or
/// selected.
pub fn nameplate<'a>(
    ctx: crate::ctx::Ctx<'a>,
    url: &str,
    (height, radius): (f32, f32),
    pointed: bool,
    selected: bool,
    opacity: f32,
) -> Option<Element<'a, Message>> {
    if !show_nameplate(nameplate_policy(ctx), ctx.state.hide_nameplates) {
        return None;
    }
    let strength = if pointed || selected { 0.9 } else { 0.6 } * opacity;
    plate_picture(ctx, url, height, radius, strength, pointed)
}

impl<'a> View<'a> {
    pub fn members_view(self) -> Element<'a, Message> {
        let collapsed = self
            .cx
            .themes
            .package_layout(&self.cx.look.theme.id)
            .and_then(|layout| layout.members)
            == Some(yapfast_core::themes::layout::MembersSide::Collapsed);
        if collapsed && !self.s.collapsed_open {
            return self.collapsed_view();
        }
        if self
            .cx
            .themes
            .package_layout(&self.cx.look.theme.id)
            .and_then(|layout| layout.members_style)
            == Some(yapfast_core::themes::layout::MembersStyle::Grid)
        {
            return self.grid_view(collapsed);
        }
        let themed_rows = self.cx.themes.member_rows(&self.cx.look.theme.id);
        let row_height = row_height(self.cx.look.theme);
        let tops = Tops::of(self.cx.look.theme, themed_rows, &self.s.lines);
        let (first, last) = tops.in_view(&self.s.scroll);
        let total = tops.end() + 10.0 + themed_rows.map_or(0.0, |rows| rows.reserve);
        // As Slint: a theme's art after the last row, its own height and
        // 8 px more, scrolling with the list.
        let foot_height = themed_rows
            .filter(|rows| rows.reserve <= 0.0)
            .and_then(|rows| rows.foot.as_ref())
            .map_or(0.0, |foot| {
                foot.size.map_or(48.0, |(_, height)| height) + 8.0
            });
        let total = total + foot_height;
        let mut rows = Rows::new(total);
        // A theme's file tree and marks by group: each member's group's
        // place, and whether it ends its group.
        let tree =
            crate::theme_chrome::tree(self.cx, yapfast_core::themes::chrome::TreeList::Members);
        let by_role = self
            .cx
            .themes
            .chrome(&self.cx.look.theme.id)
            .map(|chrome| chrome.member_by_role.as_slice())
            .filter(|marks| !marks.is_empty());
        let status = self
            .cx
            .themes
            .chrome(&self.cx.look.theme.id)
            .map(|chrome| &chrome.status)
            .filter(|status| !status.is_empty());
        // What a theme's rows take from the lines over them, walked from
        // the first line drawn; a stock list takes none of it.
        let mut walk = if themed_rows.is_some() || by_role.is_some() || status.is_some() {
            Walk::to(&self.s.lines[..first])
        } else {
            Walk::default()
        };
        for (index, line) in self.s.lines.iter().enumerate().take(last).skip(first) {
            let top = tops.top(index);
            let at = walk.step(line);
            let line_height = match line {
                MemberLine::Heading { .. } => heading_height(themed_rows).unwrap_or(row_height),
                MemberLine::Member(_) => row_height,
            };
            let (id, row) = match line {
                MemberLine::Heading { text, icon } => {
                    (text.as_str(), self.heading_row(text, icon.as_deref()))
                }
                MemberLine::Member(look) => {
                    let mark = by_role.map(|marks| {
                        marks[at.groups.saturating_sub(1).min(marks.len() - 1)].as_str()
                    });
                    // The member's group, as Slint's `group-name`: the
                    // heading above it, without its count.
                    let group = yapfast_core::themes::chrome::split_group(at.heading).0;
                    let row = self.member_row(look, mark, &group_name(group), at.online);
                    // A theme's status column, by presence; `{n}` the row's
                    // place in its group, from 0.
                    let row = match status {
                        Some(status) => {
                            use yapfast_core::themes::chrome::Presence as Column;
                            let column = match look.presence {
                                Some(yapfast_core::chat::Presence::Idle) => Column::Idle,
                                Some(yapfast_core::chat::Presence::DoNotDisturb) => Column::Dnd,
                                Some(yapfast_core::chat::Presence::Online)
                                | Some(yapfast_core::chat::Presence::Mobile) => Column::Online,
                                _ => Column::Offline,
                            };
                            let place = at.place.to_string();
                            let words = status.get(&column).map_or_else(String::new, |words| {
                                yapfast_core::themes::chrome::fill(words, &[("n", place.as_str())])
                            });
                            row![
                                container(row).width(Length::Fill),
                                text(words)
                                    .font(crate::theme_chrome::mono(self.cx))
                                    .size(12)
                                    .color(self.cx.look.theme.faint)
                            ]
                            .align_y(Alignment::Center)
                            .into()
                        }
                        None => row,
                    };
                    let row = if tree {
                        let ends =
                            !matches!(self.s.lines.get(index + 1), Some(MemberLine::Member(_)));
                        crate::theme_chrome::tree_row(self.cx, ends, row)
                    } else {
                        row
                    };
                    (look.id.as_str(), row)
                }
            };
            let (left, right) = themed_rows
                .and_then(|rows| rows.margin)
                .unwrap_or((8.0, 8.0));
            let row = container(row).padding(Padding {
                left,
                right,
                ..Padding::ZERO
            });
            let row: Element<'a, Message> = if matches!(line, MemberLine::Member(_))
                && let Some(stripe) = self
                    .cx
                    .look
                    .theme
                    .surface_stripe("row")
                    .filter(|stripe| stripe.members && index % 2 == 1)
            {
                container(row)
                    .width(Length::Fill)
                    .style(move |_| fill(stripe.colour))
                    .into()
            } else {
                row.into()
            };
            // As Slint, a list's separator lies over a member that follows
            // another of its group.
            let follows =
                index > 0 && matches!(self.s.lines.get(index - 1), Some(MemberLine::Member(_)));
            let row: Element<'a, Message> = if matches!(line, MemberLine::Member(_)) && follows {
                match themed_rows.and_then(|rows| rows.separator.as_ref()) {
                    Some(yapfast_core::themes::lists::Separator::Line {
                        colour,
                        width,
                        inset: (line_left, line_right),
                    }) => {
                        let line =
                            container(bar(crate::themes::argb(*colour), Length::Fill, *width))
                                .padding(Padding {
                                    left: *line_left,
                                    right: *line_right,
                                    ..Padding::ZERO
                                });
                        stack![row, crate::controls::placed(line, 0.0, 0.0)].into()
                    }
                    Some(yapfast_core::themes::lists::Separator::Art { art, height }) => {
                        match crate::theme_art::tile_art(
                            self.cx,
                            yapfast_core::themes::package::art::ArtSurface::Members,
                            art,
                            width(self.cx.layout, self.cx.look.theme),
                            *height,
                        ) {
                            Some(separator) => stack![
                                row,
                                crate::controls::placed(
                                    separator,
                                    0.0,
                                    (row_height - *height).max(0.0)
                                )
                            ]
                            .into(),
                            None => row,
                        }
                    }
                    None => row,
                }
            } else {
                row
            };
            rows.push(key(id), top, line_height.max(0.0), row);
        }
        if foot_height > 0.0
            && let Some(foot) = themed_rows.and_then(|rows| rows.foot.as_ref())
            && let Some(art) = crate::theme_art::tile_art(
                self.cx,
                yapfast_core::themes::package::art::ArtSurface::Members,
                foot,
                width(self.cx.layout, self.cx.look.theme),
                foot_height,
            )
        {
            rows.push(
                key("theme:member-foot"),
                total - foot_height,
                foot_height,
                art,
            );
        }
        if themed_rows.is_some_and(|rows| rows.reserve > 0.0)
            && let Some(rows_theme) = themed_rows
            && let Some(foot) = rows_theme.foot.as_ref()
            && let Some(art) = crate::theme_art::tile_art(
                self.cx,
                yapfast_core::themes::package::art::ArtSurface::Members,
                foot,
                width(self.cx.layout, self.cx.look.theme),
                rows_theme.reserve,
            )
        {
            rows.push(
                key("theme:member-foot"),
                total - rows_theme.reserve,
                rows_theme.reserve,
                art,
            );
        }
        // The chat's ground, an outline down its left edge.
        let (ground, edge) = (self.cx.look.theme.chat, self.cx.look.theme.outline_chat);
        let list = crate::widgets::Settle::new(
            scroll_view(rows)
                .on_scroll(|viewport| Message::Members(Event::Scrolled(viewport)))
                .height(Length::Fill)
                .style(scrollbar(self.cx.look.theme)),
            |message| match message {
                Message::Members(Event::Scrolled(viewport)) => Some(viewport),
                _ => None,
            },
        );
        // What the list is waiting for, or why it's empty, in a note over
        // it.
        let note = match &self.cx.state.members {
            None => Some(tr("Loading members...")),
            Some(Err(error)) => Some(*error),
            Some(Ok(members)) if members.is_empty() => Some(tr("No one else has joined yet.")),
            Some(Ok(_)) => None,
        };
        let note = note.map(|note| {
            container(
                text(note)
                    .size(self.cx.look.theme.type_size("body", 13.0))
                    .font(self.cx.look.theme.font("body", REGULAR))
                    .color(self.cx.look.theme.muted)
                    .width(Length::Fill),
            )
            .padding(16)
        });
        let mut contents = column![];
        if let Some(title) = self.title_line() {
            contents = contents.push(title);
        }
        if themed_rows.is_none_or(|rows| rows.activity) {
            contents = contents.push(self.activity_panel());
        }
        contents = contents.push(note).push(list);
        if let Some(footer) = self.footer_art() {
            contents = contents.push(footer);
        }
        let list: Element<'a, Message> = contents.into();
        // A theme's art or glass under the pane (`theme_art::surface`) is
        // its ground, as in Slint; a solid one here would hide it.
        let bare = self.cx.themes.decorates(
            &self.cx.look.theme.id,
            yapfast_core::themes::package::art::ArtSurface::Members,
        );
        let list = container(row![bar(edge, 1, Length::Fill), list])
            .width(width(self.cx.layout, self.cx.look.theme))
            .height(Length::Fill)
            .style(move |_| {
                if bare {
                    container::Style::default()
                } else {
                    fill(ground)
                }
            });
        // Its edge drags it wider or narrower, over the line.
        let theme = self.cx.look.theme;
        let splitter = crate::widgets::Splitter::new(
            iced::Color::TRANSPARENT,
            theme.line,
            theme.faint,
            |by| Message::Members(Event::Resize(by)),
            Message::Members(Event::Resized),
            Message::Members(Event::ResetWidth),
        );
        let panel: Element<'a, Message> = iced::widget::stack![list, splitter].into();
        if collapsed {
            mouse_area(panel)
                .on_exit(Message::Members(Event::Collapsed(false)))
                .into()
        } else {
            panel
        }
    }

    /// A collapsed member pane is a 56 px strip of avatars. Entering it
    /// opens the ordinary pane over its share; leaving closes it again.
    fn collapsed_view(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        // Only the faces in view and near it, as the list's rows.
        let members: Vec<&MemberLook> = self
            .s
            .lines
            .iter()
            .filter_map(|line| match line {
                MemberLine::Member(look) => Some(look.as_ref()),
                MemberLine::Heading { .. } => None,
            })
            .collect();
        let (first, last) = strip_range(&self.s.strip, members.len());
        let mut faces = Rows::new(strip_total(members.len()));
        for (index, look) in members.into_iter().enumerate().take(last).skip(first) {
            let id = look.id.clone();
            let menu_id = look.id.clone();
            let origin = self.member_list_origin();
            let pointed = self.cx.pointed.member.as_ref() == Some(&look.id);
            let face = self.cx.face(
                Face::new(look.avatar.as_deref(), &look.name, 40.0)
                    .opacity(if look.offline() { 0.4 } else { 1.0 })
                    .decoration(look.decoration.as_deref(), pointed)
                    .animate(pointed),
            );
            let area = mouse_area(face)
                .interaction(iced::mouse::Interaction::Pointer)
                .on_enter(Message::Members(Event::Point(id.clone(), true)))
                .on_exit(Message::Members(Event::Point(id.clone(), false)))
                .on_press(Message::Members(Event::Open(id)));
            let area = RightPress::new(area, move |at| {
                Message::PersonMenu(menu_id.clone(), at + origin)
            });
            faces.push(
                key(&look.id),
                STRIP_TOP + index as f32 * STRIP_PITCH,
                40.0,
                container(
                    tooltip(
                        area,
                        controls::tip(theme, look.name.clone()),
                        tooltip::Position::Left,
                    )
                    .gap(6),
                )
                .padding([0, 8]),
            );
        }
        let list = crate::widgets::Settle::new(
            scroll_view(faces)
                .on_scroll(|viewport| Message::Members(Event::StripScrolled(viewport)))
                .height(Length::Fill)
                .style(scrollbar(theme)),
            |message| match message {
                Message::Members(Event::StripScrolled(viewport)) => Some(viewport),
                _ => None,
            },
        );
        // Its size, for the faces in view while they all fit.
        let list =
            iced::widget::sensor(list).on_resize(|size| Message::Members(Event::StripSized(size)));
        let panel = container(list)
            .width(56)
            .height(Length::Fill)
            .style(move |_| fill(theme.chat));
        mouse_area(panel)
            .on_enter(Message::Members(Event::Collapsed(true)))
            .into()
    }

    fn grid_view(self, collapsed: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let package = self
            .cx
            .themes
            .package_layout(&theme.id)
            .expect("grid members belong to an installed package");
        let themed = package.member_rows.as_ref();
        // The columns follow the room the list really has (a theme's pane
        // can be narrower than the list, and the list narrower than the
        // theme's columns need), so whole tiles always fit.
        let list = iced::widget::responsive(move |size| self.grid_list(size.width));

        let note = match &self.cx.state.members {
            None => Some("Loading members..."),
            Some(Err(error)) => Some(*error),
            Some(Ok(members)) if members.is_empty() => Some("No one else has joined yet."),
            Some(Ok(_)) => None,
        }
        .map(|note| {
            container(
                text(note)
                    .size(theme.type_size("body", 13.0))
                    .font(theme.font("body", REGULAR))
                    .color(theme.muted),
            )
            .padding(16)
        });
        let mut contents = column![];
        if let Some(title) = self.title_line() {
            contents = contents.push(title);
        }
        if themed.is_none_or(|rows| rows.activity) {
            contents = contents.push(self.activity_panel());
        }
        contents = contents.push(note).push(list);
        if let Some(footer) = self.footer_art() {
            contents = contents.push(footer);
        }
        let list: Element<'a, Message> = contents.into();
        // A theme's art or glass under the pane is its ground (see the
        // list's view).
        let bare = self.cx.themes.decorates(
            &theme.id,
            yapfast_core::themes::package::art::ArtSurface::Members,
        );
        let panel = container(row![bar(theme.outline_chat, 1, Length::Fill), list])
            .width(width(self.cx.layout, theme))
            .height(Length::Fill)
            .style(move |_| {
                if bare {
                    container::Style::default()
                } else {
                    fill(theme.chat)
                }
            });
        let splitter = crate::widgets::Splitter::new(
            Color::TRANSPARENT,
            theme.line,
            theme.faint,
            |by| Message::Members(Event::Resize(by)),
            Message::Members(Event::Resized),
            Message::Members(Event::ResetWidth),
        );
        let panel: Element<'a, Message> = stack![panel, splitter].into();
        if collapsed {
            mouse_area(panel)
                .on_exit(Message::Members(Event::Collapsed(false)))
                .into()
        } else {
            panel
        }
    }

    /// The grid's rows at `room` wide: as many columns as whole tiles,
    /// their frames and their shadows fit, up to the theme's.
    fn grid_list(self, room: f32) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let Some(package) = self.cx.themes.package_layout(&theme.id) else {
            return Space::new().into();
        };
        self.s.grid_room.set(room);
        let themed = package.member_rows.as_ref();
        let Grid {
            columns,
            tile_height,
            gap,
            heading_height,
            left,
            right,
            reach_x,
            reach_y,
        } = Grid::at(theme, package, room);
        let visual = grid_lines(&self.s.lines, columns);

        let reserve = themed.map_or(0.0, |rows| rows.reserve);
        let total = TOP
            + visual
                .iter()
                .map(|line| match line {
                    GridLine::Heading(..) => heading_height + gap,
                    GridLine::Members(..) => tile_height + gap,
                })
                .sum::<f32>()
            + 10.0
            + reserve;
        // As Slint: a theme's art after the last row when it reserves no
        // room of its own.
        let foot_height = themed
            .filter(|rows| rows.reserve <= 0.0)
            .and_then(|rows| rows.foot.as_ref())
            .map_or(0.0, |foot| {
                foot.size.map_or(48.0, |(_, height)| height) + 8.0
            });
        let total = total + foot_height;
        let viewport = self.s.scroll;
        let mut rows = Rows::new(total);
        let mut top = TOP;
        for line in visual {
            let height = match &line {
                GridLine::Heading(..) => heading_height,
                GridLine::Members(..) => tile_height,
            };
            let at = top;
            top += height + gap;
            if !grid_line_in_view(at, height, &viewport) {
                continue;
            }
            match line {
                GridLine::Heading(heading, icon) => {
                    rows.push(key(heading), at, height, self.heading_row(heading, icon));
                }
                GridLine::Members(members) => {
                    let first = members.first().map_or("grid", |look| look.id.as_str());
                    let mut tiles = row![].spacing(gap).height(height);
                    let count = members.len();
                    for look in members {
                        tiles = tiles.push(
                            container(self.grid_tile(look, package.grid.size, height))
                                .width(Length::FillPortion(1)),
                        );
                    }
                    for _ in count..columns {
                        tiles = tiles.push(Space::new().width(Length::FillPortion(1)));
                    }
                    // The row reaches under the gap for the tiles' shadows.
                    rows.push(
                        key(first),
                        at,
                        height + reach_y.min(gap),
                        container(tiles).padding(Padding {
                            left,
                            right: right + reach_x,
                            bottom: reach_y.min(gap),
                            ..Padding::ZERO
                        }),
                    );
                }
            }
        }
        if reserve > 0.0
            && let Some(foot) = themed.and_then(|rows| rows.foot.as_ref())
            && let Some(art) = crate::theme_art::tile_art(
                self.cx,
                yapfast_core::themes::package::art::ArtSurface::Members,
                foot,
                width(self.cx.layout, theme),
                reserve,
            )
        {
            rows.push(key("theme:member-grid-foot"), total - reserve, reserve, art);
        }
        if foot_height > 0.0
            && let Some(foot) = themed.and_then(|rows| rows.foot.as_ref())
            && let Some(art) = crate::theme_art::tile_art(
                self.cx,
                yapfast_core::themes::package::art::ArtSurface::Members,
                foot,
                width(self.cx.layout, theme),
                foot_height,
            )
        {
            rows.push(
                key("theme:member-grid-foot"),
                total - foot_height,
                foot_height,
                art,
            );
        }

        crate::widgets::Settle::new(
            scroll_view(rows)
                .on_scroll(|viewport| Message::Members(Event::Scrolled(viewport)))
                .height(Length::Fill)
                .style(scrollbar(theme)),
            |message| match message {
                Message::Members(Event::Scrolled(viewport)) => Some(viewport),
                _ => None,
            },
        )
        .into()
    }

    /// A package's optional line above the member list, such as `$ who`.
    /// Stock themes do not grow a new header: the row exists only when the
    /// installed package supplied `chrome.labels.members_title`.
    fn title_line(self) -> Option<Element<'a, Message>> {
        let theme = self.cx.look.theme;
        let chrome = self.cx.themes.chrome(&theme.id)?;
        let template = chrome
            .labels
            .get(&yapfast_core::themes::chrome::Label::MembersTitle)?;
        let count = self
            .s
            .lines
            .iter()
            .filter(|line| matches!(line, MemberLine::Member(_)))
            .count()
            .to_string();
        let words = yapfast_core::themes::chrome::fill(template, &[("n", count.as_str())]);
        let listed = self.cx.themes.member_rows(&theme.id).map(|rows| rows.title);
        let role = listed
            .and_then(|title| title.font)
            .map(yapfast_core::themes::lists::FontRole::name)
            .or_else(|| chrome.font.map(yapfast_core::themes::package::Role::name))
            .unwrap_or("labels");
        let size = listed
            .and_then(|title| title.size)
            .unwrap_or_else(|| theme.type_size(role, 14.0));
        let ink = listed
            .and_then(|title| title.colour)
            .map(crate::themes::argb)
            .unwrap_or(theme.muted);
        let shadow_light = crate::themes::offset_shadow(listed.and_then(|title| title.shadow));
        let line: Element<'a, Message> = if let Some((base, count_part)) =
            words.split_once(" \u{00b7} ")
            && listed.is_some_and(|t| {
                t.count_font.is_some() || t.count_size.is_some() || t.count_colour.is_some()
            }) {
            let t = listed.unwrap();
            let count_role = t
                .count_font
                .map_or(role, yapfast_core::themes::lists::FontRole::name);
            let count_size = t.count_size.unwrap_or(size);
            let count_ink = t.count_colour.map(crate::themes::argb).unwrap_or(ink);
            row![
                crate::elide::elide(base.to_owned())
                    .size(size)
                    .font(theme.font(role, MEDIUM))
                    .color(ink)
                    .light(shadow_light)
                    .shrink(),
                crate::elide::elide(format!(" \u{00b7} {count_part}"))
                    .size(count_size)
                    .font(theme.font(count_role, MEDIUM))
                    .color(count_ink)
                    .light(shadow_light)
                    .shrink(),
            ]
            .align_y(Alignment::Center)
            .into()
        } else {
            crate::elide::elide(words)
                .size(size)
                .font(theme.font(role, MEDIUM))
                .color(ink)
                .light(shadow_light)
                .shrink()
                .into()
        };
        let has_rule = listed.and_then(|t| t.rule).is_some();
        let height = if has_rule { 44.0 } else { 32.0 };
        let mut title = container(line)
            .padding(Padding {
                left: 16.0,
                right: 8.0,
                ..Padding::ZERO
            })
            .height(if has_rule { 34.0 } else { 32.0 })
            .align_y(Alignment::End);
        if listed.is_some_and(|title| title.centre) {
            title = title.width(Length::Fill).align_x(Alignment::Center);
        }
        if let Some((rule_colour, dotted)) = listed.and_then(|t| t.rule) {
            let colour = crate::themes::argb(rule_colour);
            let rule_el: Element<'a, Message> = if dotted {
                let mut dots = row![].spacing(2);
                for _ in 0..48 {
                    dots = dots.push(
                        container(Space::new())
                            .width(2)
                            .height(2)
                            .style(move |_| fill(colour)),
                    );
                }
                container(dots)
                    .padding([0, 12])
                    .width(Length::Fill)
                    .clip(true)
                    .into()
            } else {
                container(bar(colour, Length::Fill, 2.0))
                    .padding([0, 12])
                    .into()
            };
            Some(column![title, rule_el].spacing(4).height(height).into())
        } else {
            Some(title.into())
        }
    }

    /// Theme stickers or blinkies fixed at the member list's foot,
    /// outside its scrolling rows, matching the Slint Trim footer.
    fn footer_art(self) -> Option<Element<'a, Message>> {
        let theme = self.cx.look.theme;
        let height = self.cx.themes.footer_height(&theme.id)?;
        Some(crate::theme_art::surface(
            Space::new().width(Length::Fill).height(height),
            self.cx,
            yapfast_core::themes::package::art::ArtSurface::Footer,
        ))
    }

    fn grid_tile(self, look: &'a MemberLook, avatar: f32, height: f32) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let rows = self.cx.themes.member_rows(&theme.id);
        let pointed = self.cx.pointed.member.as_ref() == Some(&look.id);
        let selected = self
            .cx
            .profiles
            .card
            .as_ref()
            .is_some_and(|card| card.user == look.id);
        let offline = look.offline();
        let frame =
            rows.and_then(|rows| crate::theme_art::picked_tile(&rows.avatar.frame, &look.id));
        let face = self.cx.face(
            Face::new(look.avatar.as_deref(), &look.name, avatar)
                .opacity(if offline { 0.4 } else { 1.0 })
                .decoration(look.decoration.as_deref(), pointed)
                .animate(pointed)
                .frame(frame)
                .illuminated_member(pointed),
        );
        let stock = look.color.map_or_else(
            || {
                if selected {
                    theme.token_colour("row-selected-text", theme.text)
                } else {
                    theme.text
                }
            },
            hex,
        );
        let themed_ink = list_name_ink(self.cx, rows, look);
        let ink = themed_ink.unwrap_or(stock);
        let name = crate::themes::list_case(rows.and_then(|rows| rows.case), &look.name);
        let size = rows
            .and_then(|rows| rows.size)
            .unwrap_or_else(|| theme.type_size("names", 13.0));
        let weight = rows.and_then(|rows| rows.weight).unwrap_or(600.0).round() as u16;
        let name_role = rows
            .and_then(|rows| rows.font)
            .map_or("names", yapfast_core::themes::lists::FontRole::name);
        let mut name_layers = stack![];
        if let Some(glow) = rows.and_then(|rows| rows.name_glow) {
            let glow = crate::themes::argb(glow).scale_alpha(0.4);
            // Slint's eight turns, 1.5 px out (all moved 1.5 px in, as a
            // placement can't be negative; the name is too).
            for turn in 0..8 {
                let angle = turn as f32 * std::f32::consts::FRAC_PI_4;
                // Laid out as the name itself is, so the copies sit on it.
                let copy = self.cx.person_name(
                    name.clone().into_owned(),
                    NameInk::new(size, weight, glow)
                        .font_role(name_role)
                        .style(look.style.as_ref())
                        .fixed(true),
                );
                name_layers = name_layers.push(controls::placed(
                    container(copy)
                        .width(Length::Fill)
                        .height(20)
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center),
                    1.5 + 1.5 * angle.cos(),
                    1.5 + 1.5 * angle.sin(),
                ));
            }
        }
        let main_name = self.cx.person_name(
            name.into_owned(),
            NameInk::new(size, weight, faded(ink, if offline { 0.5 } else { 1.0 }))
                .font_role(name_role)
                .style(look.style.as_ref())
                .key(&look.id)
                .fixed(themed_ink.is_some()),
        );
        let glowing = rows.and_then(|rows| rows.name_glow).is_some();
        name_layers = name_layers.push(controls::placed(
            container(main_name)
                .width(Length::Fill)
                .height(20)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
            if glowing { 1.5 } else { 0.0 },
            if glowing { 1.5 } else { 0.0 },
        ));
        let mut name_line = row![name_layers]
            .width(Length::Fill)
            .align_y(Alignment::Center);
        if look.owner {
            let crown = rows.and_then(|rows| rows.crown_ink).unwrap_or(CROWN);
            name_line =
                name_line.push(Pic::square(self.cx.look.icon(Icon::Crown, 14, crown), 14.0));
        }
        // As Slint's `ListedMemberTile`: the face on the raised paint with
        // the avatar shape's shadow, its border over the picture, and the
        // name 4 px over the face's foot.
        let radius = theme.avatar_radius(avatar, avatar / 2.0);
        let avatar_shadow = theme.surface_shadow("avatar", iced::Shadow::default());
        let mut avatar_border = theme.surface_border("avatar", radius, 0.0, Color::TRANSPARENT);
        avatar_border.radius = radius.into();
        let raised = theme.surface_paint("raised");
        let face: Element<'a, Message> =
            if rows.is_some() {
                iced::widget::Stack::with_children([face])
                    .push_under(container(Space::new()).width(avatar).height(avatar).style(
                        move |_| container::Style {
                            background: Some(raised),
                            border: iced::Border {
                                radius: radius.into(),
                                ..Default::default()
                            },
                            shadow: avatar_shadow,
                            ..Default::default()
                        },
                    ))
                    .push(
                        container(Space::new())
                            .width(avatar)
                            .height(avatar)
                            .style(move |_| container::Style {
                                border: avatar_border,
                                ..Default::default()
                            }),
                    )
                    .into()
            } else {
                face
            };
        // A theme's presence glyph under the name, as Slint's `MemberGlyph`.
        let glyph: Option<Element<'a, Message>> = rows
            .filter(|rows| rows.status == Some(yapfast_core::themes::lists::StatusStyle::Glyph))
            .and_then(|rows| {
                let index = match look.presence.unwrap_or(Presence::Offline) {
                    Presence::Online | Presence::Mobile => 0,
                    Presence::Idle => 1,
                    Presence::DoNotDisturb => 2,
                    Presence::Offline => 3,
                };
                match rows.status_glyphs[index].as_ref()? {
                    yapfast_core::themes::lists::Glyph::Art(art) => {
                        let (w, h) = art.size.unwrap_or((12.0, 12.0));
                        crate::theme_art::tile_art(
                            self.cx,
                            yapfast_core::themes::package::art::ArtSurface::Members,
                            art,
                            w,
                            h,
                        )
                        .map(|art| container(art).width(w).height(h).into())
                    }
                    yapfast_core::themes::lists::Glyph::Text(words) => Some(
                        text(words.clone())
                            .size(12)
                            .color(theme.accent_text())
                            .into(),
                    ),
                }
            });
        let content: Element<'a, Message> = if rows.is_some() {
            // The tile's whole height: a stack as tall as its face alone
            // squeezed the name placed under it up over the face, and cut
            // the status glyph off.
            let mut layers = iced::widget::stack![
                container(face).center_x(Length::Fill),
                controls::placed(name_line, 0.0, avatar - 4.0),
            ]
            .width(Length::Fill)
            .height(Length::Fill);
            if let Some(glyph) = glyph {
                layers = layers.push(controls::placed(
                    container(glyph).center_x(Length::Fill),
                    0.0,
                    avatar - 4.0 + 20.0 + 1.0,
                ));
            }
            layers.into()
        } else {
            column![face, name_line]
                .spacing(3)
                .align_x(Alignment::Center)
                .width(Length::Fill)
                .into()
        };
        let outlined = rows.is_some_and(|rows| rows.tile_outline);
        let mut border = if outlined {
            theme.surface_border("tile", 8.0, 0.0, Color::TRANSPARENT)
        } else {
            iced::Border::default()
        };
        border.radius = theme.surface_radius("tile", 4.0).into();
        let outline_fallback = border;
        if theme.outline("tile").is_some() {
            border.width = 0.0;
        }
        let shadow = if outlined {
            theme.surface_shadow("tile", iced::Shadow::default())
        } else {
            iced::Shadow::default()
        };
        let ground = if selected {
            Some(theme.token_colour("row-selected", theme.active).into())
        } else if pointed {
            Some(theme.hover.into())
        } else if outlined {
            Some(theme.raised.into())
        } else {
            None
        };
        let body = container(content)
            .padding(Padding {
                top: 7.0,
                ..Padding::ZERO
            })
            .width(Length::Fill)
            .height(height)
            .clip(true)
            .style(move |_| container::Style {
                background: ground,
                border,
                shadow,
                ..Default::default()
            });
        let body = crate::theme_art::outlined(
            body,
            self.cx,
            yapfast_core::themes::layout::BorderPlace::Tile,
            "tile",
            4.0,
            outline_fallback.width,
            outline_fallback.color,
        );
        let body = crate::theme_effects::foil(
            body,
            self.cx,
            yapfast_core::themes::package::art::FoilSurface::Tile,
        );
        let id = look.id.clone();
        let menu_id = look.id.clone();
        let origin = self.member_list_origin();
        let area = mouse_area(body)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_enter(Message::Members(Event::Point(id.clone(), true)))
            .on_exit(Message::Members(Event::Point(id.clone(), false)))
            .on_press(Message::Members(Event::Open(id)));
        let area = crate::widgets::Spot::new(area, &self.s.tile_spot).by_press();
        RightPress::new(area, move |at| {
            Message::PersonMenu(menu_id.clone(), at + origin)
        })
        .into()
    }

    /// A group's heading: 14 px medium in the channel grey, 20 px above,
    /// its role's icon before it.
    fn heading_row(self, heading: &str, icon: Option<&str>) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let (role, count) = yapfast_core::themes::chrome::split_group(heading);
        let heading = self.cx.themes.label(
            &theme.id,
            yapfast_core::themes::chrome::Label::MembersGroup,
            heading,
            &[("role", role), ("n", count)],
        );
        let headers = self
            .cx
            .themes
            .member_rows(&theme.id)
            .map(|rows| &rows.headers);
        let heading =
            headers
                .filter(|headers| !headers.text.is_empty())
                .map_or(heading, |headers| {
                    yapfast_core::themes::chrome::fill(
                        &headers.text,
                        &[("role", role), ("n", count)],
                    )
                });
        let heading = crate::themes::list_case(headers.and_then(|headers| headers.case), &heading);
        let heading = theme.surface_words("label", &heading).into_owned();
        let ink = headers
            .and_then(|headers| headers.colour)
            .map(crate::themes::argb)
            .or_else(|| theme.surface_text("label"))
            .unwrap_or_else(|| hex(HEADING));
        let mut line = row![].spacing(8).align_y(Alignment::Center);
        if let Some(icon) = icon.and_then(|icon| self.cx.picture(icon, 20, 20, false)) {
            line = line.push(Pic::square(icon, 20.0));
        }
        // As Slint's group heading: the label face and size only on a
        // theme's label bar (`Trim.label`), else the window's.
        let label_bar = headers.is_none_or(|headers| *headers == Default::default())
            && theme.surface_ground("label").is_some();
        let stock_role = if label_bar { "labels" } else { "body" };
        let header_role = headers
            .and_then(|h| h.font)
            .map_or(stock_role, yapfast_core::themes::lists::FontRole::name);
        let size = headers.and_then(|headers| headers.size).unwrap_or_else(|| {
            if label_bar || headers.and_then(|h| h.font).is_some() {
                theme.type_size(header_role, 14.0)
            } else {
                14.0
            }
        });
        let header_font = crate::themes::list_font(
            theme,
            headers.and_then(|h| h.font),
            stock_role,
            headers.and_then(|h| h.weight),
            MEDIUM,
        );
        let header_shadow = crate::themes::offset_shadow(headers.and_then(|h| h.shadow));
        // As Slint's member heading: the list's own heading shadow only;
        // the theme's `labels` light dresses Settings' section labels.
        let light = header_shadow;
        line = line.push(
            crate::elide::elide(heading)
                .size(size)
                .font(header_font)
                .color(ink)
                .light(light)
                .shrink(),
        );
        let border = theme.surface_border("label", 0.0, 0.0, Color::TRANSPARENT);
        let shadow = theme.surface_shadow("label", iced::Shadow::default());
        let surface = yapfast_core::themes::package::art::ArtSurface::Label;
        let decorated = self.cx.themes.decorates(&theme.id, surface);
        let background = (!decorated)
            .then(|| theme.surface_ground("label"))
            .flatten();
        let height = headers
            .and_then(|headers| headers.height)
            .unwrap_or_else(|| row_height(self.cx.look.theme));
        let fill_width = headers.is_some_and(|h| h.fill);
        if let Some(frame) = headers.and_then(|headers| headers.frame.as_ref()) {
            // As Slint: the words 12 px into a frame 22 px past them (8 px
            // into one across the list), centred in the heading.
            // Slint lays it 8 px in, in a box from 22 px above the
            // heading's foot to 4 px above it.
            let body = crate::theme_art::framed_heading(
                self.cx,
                yapfast_core::themes::package::art::ArtSurface::Members,
                frame,
                line.into(),
                (height.min(22.0) - 4.0).max(0.0),
                if fill_width { 22.0 } else { 24.0 },
                fill_width,
                Padding {
                    top: 0.0,
                    right: if fill_width { 8.0 } else { 10.0 },
                    bottom: 0.0,
                    left: if fill_width { 8.0 } else { 12.0 },
                },
            );
            let body = container(body)
                .padding(Padding {
                    top: (height - 22.0).max(0.0),
                    bottom: 4.0,
                    left: 8.0,
                    right: 0.0,
                })
                .height(height);
            return crate::theme_art::surface(body, self.cx, surface);
        }
        let mut body = container(line)
            .padding(Padding {
                top: if fill_width { 4.0 } else { 20.0 },
                right: if fill_width { 6.0 } else { 0.0 },
                bottom: 4.0,
                left: headers
                    .and_then(|headers| headers.inset)
                    .unwrap_or(if fill_width { 6.0 } else { 8.0 }),
            })
            .height(height)
            .align_y(Alignment::Center)
            .style(move |_| container::Style {
                background,
                border,
                shadow,
                ..Default::default()
            });
        if fill_width {
            body = body.width(Length::Fill);
        }
        let body: Element<'a, Message> = body.into();
        let body = match headers.and_then(|headers| headers.frame.as_ref()) {
            Some(frame) => match crate::theme_art::tile_art(
                self.cx,
                yapfast_core::themes::package::art::ArtSurface::Members,
                frame,
                width(self.cx.layout, theme),
                height,
            ) {
                Some(art) => stack![art, body].width(Length::Fill).height(height).into(),
                None => body,
            },
            None => body,
        };
        crate::theme_art::surface(body, self.cx, surface)
    }

    /// A member's row: the face with its decoration (playing while pointed
    /// at) and presence, the name in its style's font and its role's
    /// colour, the role icon, server tag, APP tag, crown, boosting gem and
    /// timeout's clock, the activity line, and the nameplate behind it all.
    fn member_row(
        self,
        look: &'a MemberLook,
        mark: Option<&str>,
        group: &str,
        online: usize,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let themed = self.cx.themes.member_rows(&theme.id);
        let pointed = self.cx.pointed.member.as_ref() == Some(&look.id);
        // The member whose card is open.
        let selected = self
            .cx
            .profiles
            .card
            .as_ref()
            .is_some_and(|card| card.user == look.id);
        let offline = look.offline();
        let (avatar_opacity, opacity) = if offline { (0.4, 0.5) } else { (1.0, 1.0) };
        let avatar_size = if crate::theme_chrome::hides(
            self.cx,
            yapfast_core::themes::chrome::Hide::MemberAvatars,
        ) {
            0.0
        } else {
            themed.and_then(|rows| rows.avatar.size).unwrap_or(32.0)
        };
        let face_size = avatar_size.max(1.0);
        let status_style = themed.and_then(|rows| rows.status);
        let show_presence_dot = status_style.is_none()
            || status_style == Some(yapfast_core::themes::lists::StatusStyle::Dot);
        let avatar_frame =
            themed.and_then(|rows| crate::theme_art::picked_tile(&rows.avatar.frame, &look.id));
        let dot_size = themed.and_then(|rows| rows.avatar.dot);
        let dot_at = dot_at(
            face_size,
            dot_size,
            themed.and_then(|rows| rows.avatar.dot_out),
        );
        let dot_ring = themed
            .and_then(|rows| rows.avatar.dot_ring)
            .unwrap_or_else(|| rgb(theme.sidebar));
        let presence_name = match look.presence.unwrap_or(Presence::Offline) {
            Presence::Online => "online",
            Presence::Mobile => "mobile",
            Presence::Idle => "idle",
            Presence::DoNotDisturb => "dnd",
            Presence::Offline => "offline",
        };
        let ring = themed
            .and_then(|rows| rows.avatar.ring.as_ref())
            .filter(|ring| ring.presences.iter().any(|shown| shown == presence_name))
            .map(|ring| (ring.colour, ring.width, ring.glow));
        let face = Face::new(look.avatar.as_deref(), &look.name, face_size)
            .opacity(if avatar_size == 0.0 {
                0.0
            } else {
                avatar_opacity
            })
            .decoration(
                (avatar_size > 0.0
                    && keeps_decoration(show_presence_dot, self.cx.theme_content().is_some()))
                .then_some(look.decoration.as_deref())
                .flatten(),
                pointed,
            )
            .animate(pointed && avatar_size > 0.0)
            .dot(
                (avatar_size > 0.0)
                    .then(|| {
                        look.presence
                            .filter(|presence| *presence != Presence::Offline)
                            .map(|presence| (presence, dot_ring, dot_at))
                    })
                    .flatten(),
            )
            .dot_size(dot_size)
            .ring(ring)
            .frame(avatar_frame)
            // As Slint's `MemberLists.lean`: the print leans one way, by
            // half to all of the theme's tilt, from the member's seed.
            .turn(
                themed
                    .map(|rows| rows.avatar.tilt)
                    .filter(|tilt| *tilt != 0.0)
                    .map_or(0.0, |tilt| {
                        let (performance, _) = self.cx.themes.art_motion(&theme.id);
                        if performance == yapfast_core::themes::Performance::Light {
                            0.0
                        } else {
                            tilt * (0.5
                                + (crate::themes::slint_seed(&look.id) % 101) as f32 / 200.0)
                        }
                    }),
            )
            .illuminated_member(pointed);
        let stock_color = faded(
            look.color.map_or_else(
                || {
                    if selected {
                        theme.token_colour("row-selected-text", theme.secondary)
                    } else {
                        theme.secondary
                    }
                },
                hex,
            ),
            opacity,
        );
        // A theme's list ink beats its content colours, as on a grid tile.
        let themed_ink = list_name_ink(self.cx, themed, look);
        let color = themed_ink.map_or(stock_color, |colour| faded(colour, opacity));
        let marked = self.cx.themes.marked(
            &theme.id,
            yapfast_core::themes::chrome::Place::Member,
            &look.name,
        );
        // A theme's mark by the member's group, such as `@` and `+`.
        let marked = match mark {
            Some(mark) => format!("{mark}{marked}"),
            None => marked,
        };
        let marked = crate::themes::list_case(themed.and_then(|rows| rows.case), &marked);
        let name_role = themed
            .and_then(|rows| rows.font)
            .map_or("names", yapfast_core::themes::lists::FontRole::name);
        let name = self.cx.person_name(
            marked.into_owned(),
            NameInk::new(
                themed
                    .and_then(|rows| rows.size)
                    .unwrap_or_else(|| theme.type_size(name_role, 16.0)),
                themed.and_then(|rows| rows.weight).unwrap_or(500.0).round() as u16,
                color,
            )
            .font_role(name_role)
            .style(look.style.as_ref())
            .key(&look.id)
            .fixed(themed_ink.is_some()),
        );
        // A gradient role colour runs across the name, letter by letter,
        // unless the theme inks names itself.
        let name: Element<'a, Message> = match look
            .color_end
            .filter(|_| look.color.is_some())
            .filter(|_| themed_ink.is_none() && self.cx.theme_content().is_none())
        {
            Some(end) => {
                let stock = crate::fonts::style_font(
                    look.style.as_ref().and_then(crate::looks::NameLook::font),
                    500,
                );
                let font = if look.style.is_some() {
                    stock
                } else {
                    theme.font(name_role, stock)
                };
                let size = themed
                    .and_then(|rows| rows.size)
                    .unwrap_or_else(|| theme.type_size(name_role, 16.0));
                let spans: Vec<iced::widget::text::Span<'a, ()>> = crate::looks::gradient_letters(
                    &look.name,
                    [color, faded(hex(end.0), opacity)],
                    end.1.map(|third| faded(hex(third), opacity)),
                    size * 0.6,
                )
                .into_iter()
                .map(|(letter, ink)| iced::widget::span(letter).color(ink).font(font))
                .collect();
                container(
                    iced::widget::rich_text(spans)
                        .size(
                            themed
                                .and_then(|rows| rows.size)
                                .unwrap_or_else(|| theme.type_size(name_role, 16.0)),
                        )
                        .wrapping(iced::widget::text::Wrapping::None),
                )
                .clip(true)
                .into()
            }
            None => name,
        };
        // A theme's ransom-note names, for someone with no name style of
        // their own (as Slint).
        let name: Element<'a, Message> = match look
            .style
            .is_none()
            .then(|| {
                crate::theme_chrome::name_ransom(
                    self.cx,
                    &look.name,
                    // As Slint's member rows: the rows' own size, else the
                    // names' (Zine's 22 px), whole.
                    match themed {
                        Some(rows) => rows
                            .size
                            .unwrap_or_else(|| theme.type_size(name_role, 16.0)),
                        None => theme.type_size(name_role, 16.0) * 0.9,
                    },
                )
            })
            .flatten()
        {
            Some(ransom) => ransom,
            None => name,
        };
        let mut name_line = row![name].spacing(4).align_y(Alignment::Center);
        if let Some(presence) = look.presence
            && matches!(
                status_style,
                Some(yapfast_core::themes::lists::StatusStyle::Glyph)
                    | Some(yapfast_core::themes::lists::StatusStyle::Text)
            )
        {
            let (index, words) = match presence {
                Presence::Online => (0, "online"),
                Presence::Mobile => (0, "mobile"),
                Presence::Idle => (1, "idle"),
                Presence::DoNotDisturb => (2, "dnd"),
                Presence::Offline => (3, "offline"),
            };
            let glyph = themed.and_then(|rows| rows.status_glyphs[index].as_ref());
            if status_style == Some(yapfast_core::themes::lists::StatusStyle::Glyph)
                && let Some(yapfast_core::themes::lists::Glyph::Art(art)) = glyph
                && let Some(art) = crate::theme_art::tile_art(
                    self.cx,
                    yapfast_core::themes::package::art::ArtSurface::Members,
                    art,
                    12.0,
                    12.0,
                )
            {
                name_line = name_line.push(art);
            } else {
                let shown = if status_style == Some(yapfast_core::themes::lists::StatusStyle::Glyph)
                {
                    match glyph {
                        Some(yapfast_core::themes::lists::Glyph::Text(words)) => words.as_str(),
                        _ => words,
                    }
                } else {
                    words
                };
                name_line = name_line.push(
                    text(shown.to_owned())
                        .size(theme.type_size("small", 10.0))
                        .font(theme.font("small", REGULAR))
                        .color(theme.muted),
                );
            }
        }
        if let Some(icon) = look
            .role_icon
            .as_deref()
            .and_then(|icon| self.cx.picture(icon, 16, 16, false))
        {
            name_line = name_line.push(Pic::square(icon, 16.0).opacity(opacity));
        }
        if let Some((letters, badge)) = &look.tag {
            name_line = name_line.push(self.cx.tag_chip(
                letters,
                badge.as_deref(),
                Chip {
                    size: theme.type_size("small", 12.0),
                    font: theme.font("small", SEMIBOLD),
                    height: 16.0,
                    pad: 4.0,
                    radius: 4.0,
                    fill: CHIP,
                    ink: theme.text,
                },
                opacity,
            ));
        }
        if look.bot {
            name_line = name_line.push(self.cx.bot_tag(opacity));
        }
        if look.owner {
            let crown_ink = themed.and_then(|rows| rows.crown_ink).unwrap_or(CROWN);
            let crown = self.cx.look.icon(Icon::Crown, 14, crown_ink);
            name_line = name_line.push(Pic::square(crown, 14.0).opacity(opacity));
        }
        if let Some(boosting) = &look.boosting
            && self.cx.layout.prefs.boost_gems.unwrap_or(true)
        {
            let gem = Pic::square(self.cx.look.icon(Icon::Gem, 14, BOOST), 14.0);
            name_line = name_line.push(
                tooltip(
                    gem,
                    controls::tip(theme, boosting.as_str()),
                    tooltip::Position::Top,
                )
                .gap(6),
            );
        }
        if look.timed_out {
            name_line = name_line.push(self.cx.look.glyph(Icon::Clock, 14, theme.danger));
        }
        let mut person = Person::new(face, name_line);
        person.gap = themed
            .and_then(|rows| rows.avatar.gap)
            .unwrap_or(person.gap);
        if avatar_size == 0.0 {
            person.gap = 0.0;
        }
        let sub_role =
            themed.filter(|rows| rows.sub == Some(yapfast_core::themes::lists::SubLine::Role));
        if let Some(rows) = sub_role {
            // As Slint: the member's group typed out under the name, from
            // the theme's template ("{role} · {activity}"), 11 px muted.
            let template = if rows.sub_text.is_empty() {
                "{role}"
            } else {
                rows.sub_text.as_str()
            };
            let words = sub_line(
                template,
                &[("role", group), ("activity", look.status.words.as_str())],
            );
            if !words.is_empty() {
                let font = rows.meta.font.map_or(theme.font("body", REGULAR), |role| {
                    theme.font(role.name(), REGULAR)
                });
                person = person.sub(
                    text(words)
                        .size(11)
                        .font(font)
                        .color(faded(theme.muted, opacity))
                        .wrapping(text::Wrapping::None),
                );
            }
        } else if !look.status.is_empty()
            && themed.and_then(|rows| rows.sub) != Some(yapfast_core::themes::lists::SubLine::None)
        {
            person = person.sub(self.cx.status_line(
                &look.status,
                StatusInk::under_name(self.cx.look.theme, opacity),
            ));
        }
        let plate = look.nameplate.as_ref().and_then(|(url, _)| {
            nameplate(
                self.cx,
                url,
                (row_height(self.cx.look.theme), 8.0),
                pointed,
                selected,
                opacity,
            )
        });
        let (left, right) = themed.and_then(|rows| rows.inset).unwrap_or((8.0, 8.0));
        let mut person = self.cx.person(person).width(Length::Fill);
        if let Some(rows) = themed {
            let activity = look.status.words.as_str();
            let meta = match look.status.icon {
                1 => rows.meta.playing.as_deref(),
                2 => rows.meta.listening.as_deref(),
                3 => rows.meta.watching.as_deref(),
                4 => rows.meta.streaming.as_deref(),
                _ => match look.presence.unwrap_or(Presence::Offline) {
                    Presence::Online | Presence::Mobile => rows.meta.online.as_deref(),
                    Presence::Idle => rows.meta.idle.as_deref(),
                    Presence::DoNotDisturb => rows.meta.dnd.as_deref(),
                    Presence::Offline => rows.meta.offline.as_deref(),
                },
            };
            if let Some(meta) = meta {
                let words = yapfast_core::themes::chrome::fill(
                    meta,
                    &[("activity", activity), ("status", activity)],
                );
                let meta_role = rows
                    .meta
                    .font
                    .map_or("small", yapfast_core::themes::lists::FontRole::name);
                let words = text(words)
                    .size(
                        rows.meta
                            .size
                            .unwrap_or_else(|| theme.type_size(meta_role, 11.0)),
                    )
                    .font(theme.font(meta_role, crate::fonts::REGULAR))
                    .color(
                        rows.meta
                            .colour
                            .map(crate::themes::argb)
                            .unwrap_or(theme.muted),
                    )
                    .wrapping(text::Wrapping::None);
                // As Slint: the meta hugs its words at the row's end, at
                // most `max_width`, the name taking the rest.
                let mut words = container(words).clip(true);
                if rows.meta.max_width > 0.0 {
                    words = words.max_width(rows.meta.max_width);
                }
                person = person.push(words);
            }
            if let Some(badge) = &rows.badge
                && (!badge.online_only || !offline)
            {
                let online = online.to_string();
                let words =
                    yapfast_core::themes::chrome::fill(&badge.text, &[("online", online.as_str())]);
                let ground = crate::themes::argb(badge.ground);
                let badge_role = badge.font.name();
                person = person.push(
                    container(
                        text(words)
                            .size(theme.type_size(badge_role, 10.0))
                            .font(theme.font(badge_role, MEDIUM))
                            .color(crate::themes::argb(badge.ink)),
                    )
                    .padding([2, 4])
                    .style(move |_| rounded(ground, 4)),
                );
            }
        }
        let content = person
            .padding(Padding {
                top: 0.0,
                right: if plate.is_some() {
                    right.max(12.0)
                } else {
                    right
                },
                bottom: 0.0,
                left,
            })
            .height(row_height(self.cx.look.theme));
        // The ground the nameplate fades into.
        let ground = if selected {
            theme.token_colour("row-selected", theme.active)
        } else if pointed {
            theme.hover
        } else {
            theme.sidebar
        };
        let palette = themed_nameplate_palette(
            self.cx,
            look.nameplate
                .as_ref()
                .and_then(|(_, palette)| *palette)
                .map(hex),
        );
        let plated = PlateRow::new(
            content,
            plate,
            palette,
            ground,
            plate_end(pointed, selected),
            opacity,
        );
        let id = look.id.clone();
        let menu_id = look.id.clone();
        let open_id = look.id.clone();
        let origin = self.member_list_origin();
        let area = mouse_area({
            let mut border = theme.surface_border("row", 8.0, 0.0, Color::TRANSPARENT);
            border.radius = theme.row_radius(8.0).into();
            let shadow = theme.surface_shadow("row", iced::Shadow::default());
            let row_height = row_height(self.cx.look.theme);
            let frame = themed.and_then(|rows| {
                if selected {
                    rows.selected_frame.as_ref()
                } else {
                    None
                }
                .or_else(|| crate::theme_art::picked_tile(&rows.frame, &look.id))
            });
            let selected_surface = yapfast_core::themes::package::art::ArtSurface::RowSelected;
            let selected_decorated = selected
                && frame.is_none()
                && self.cx.themes.decorates(&theme.id, selected_surface);
            let body: Element<'a, Message> = container(plated)
                .width(Length::Fill)
                .height(row_height)
                .clip(true)
                .style(move |_| container::Style {
                    background: if selected {
                        frame.is_none().then(|| {
                            if selected_decorated {
                                Color::TRANSPARENT
                            } else {
                                theme.token_colour("row-selected", MEMBER_OPEN)
                            }
                            .into()
                        })
                    } else {
                        pointed.then(|| MEMBER_HOVER.into())
                    },
                    border,
                    shadow,
                    ..Default::default()
                })
                .into();
            let mut body = iced::widget::stack![body]
                .width(Length::Fill)
                .height(row_height);
            if let Some(frame) = frame
                && let Some(art) = crate::theme_art::tile_art_opacity(
                    self.cx,
                    yapfast_core::themes::package::art::ArtSurface::Members,
                    frame,
                    width(self.cx.layout, theme),
                    row_height,
                    if pointed && !selected {
                        themed
                            .and_then(|rows| rows.hover_frame_opacity)
                            .unwrap_or(1.0)
                    } else {
                        1.0
                    },
                    0.0,
                )
            {
                body = iced::widget::stack![art, body]
                    .width(Length::Fill)
                    .height(row_height);
            }
            if selected
                && let Some(over) = themed.and_then(|rows| rows.selected_over.as_ref())
                && let Some(art) = crate::theme_art::tile_art(
                    self.cx,
                    yapfast_core::themes::package::art::ArtSurface::Members,
                    over,
                    width(self.cx.layout, theme),
                    row_height,
                )
            {
                body = body.push(art);
            }
            let body: Element<'a, Message> = body.into();
            if selected && frame.is_none() {
                crate::theme_art::surface(body, self.cx, selected_surface)
            } else {
                body
            }
        })
        .interaction(iced::mouse::Interaction::Pointer)
        .on_enter(Message::Members(Event::Point(id.clone(), true)))
        .on_exit(Message::Members(Event::Point(id, false)))
        .on_press(Message::Members(Event::Open(open_id)));
        RightPress::new(area, move |at| {
            Message::PersonMenu(menu_id.clone(), at + origin)
        })
        .into()
    }

    /// Where the member list's rows start in the window, which a right
    /// click's point is taken from.
    fn member_list_origin(self) -> Vector {
        let package = self.cx.themes.package_layout(&self.cx.look.theme.id);
        let left = if package.and_then(|layout| layout.members)
            == Some(yapfast_core::themes::layout::MembersSide::Left)
        {
            crate::conversation::navigation_left_ctx(self.cx)
        } else {
            self.cx.look.window.width
                - width(self.cx.layout, self.cx.look.theme)
                - crate::conversation::right_rail(
                    package,
                    self.cx.themes.chrome(&self.cx.look.theme.id),
                )
                - self.cx.themes.glass_gap(&self.cx.look.theme.id)
        };
        let activity = if self
            .cx
            .themes
            .member_rows(&self.cx.look.theme.id)
            .is_some_and(|rows| !rows.activity)
        {
            0.0
        } else {
            self.s.activity_height(self.cx.state, true)
        };
        Vector::new(
            left + 1.0,
            // Drawn only while the list shows.
            header::HEIGHT + 1.0 + activity,
        )
    }

    /// The Activity panel over the list: "Activity — N" with its eye and
    /// chevron, then the first three cards (all of them once it's opened),
    /// as Discord's: 8 px corners and padding, an outline, the name, the
    /// title in bold, the kind's icon with the time, the picture.
    fn activity_panel(self) -> Option<Element<'a, Message>> {
        if self.s.activity_cards.is_empty() {
            return None;
        }
        let theme = self.cx.look.theme;
        let hidden = self.cx.state.activity_cards_hidden;
        let opens = self.s.activity_cards.len() > FIRST_CARDS || hidden;
        let mut head = row![
            text(i18n::fill(
                tr("Activity \u{2014} {}"),
                &[&self.s.activity_cards.len()],
            ))
            .size(14.0)
            .font(MEDIUM)
            .color(theme.muted)
            .width(Length::Fill),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let (eye, eye_label) = if hidden {
            (Icon::EyeOff, tr("Show Activity Cards"))
        } else {
            (Icon::Eye, tr("Hide Activity Cards"))
        };
        head = head.push(
            self.cx
                .look
                .icon_button(eye, eye_label, Message::Members(Event::HideActivity))
                .size(20.0, 16),
        );
        if opens {
            let icon = if self.s.activity_open && !hidden {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            };
            head = head.push(self.cx.look.glyph(icon, 16, theme.muted));
        }
        let lit = theme.wash(0.06);
        let head = iced::widget::button(
            container(head)
                .padding([0, 8])
                .height(ACTIVITY_HEAD)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .on_press_maybe(opens.then_some(Message::Members(Event::ToggleActivity)))
        .style(lit_style(None, lit, 4.0));
        let mut panel = column![head].padding(Padding {
            top: PANEL_TOP,
            right: 8.0,
            bottom: PANEL_BOTTOM,
            left: 8.0,
        });
        for card in self
            .s
            .activity_cards
            .iter()
            .take(self.s.cards_shown(self.cx.state))
        {
            panel = panel.push(container(self.activity_card(card)).padding(Padding {
                bottom: CARD_PITCH - CARD,
                ..Padding::ZERO
            }));
        }
        Some(panel.into())
    }

    fn activity_card(self, card: &'a ActivityCard) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let tone = if card.live { theme.live } else { theme.muted };
        let mut when = row![].spacing(4).align_y(Alignment::Center);
        if let Some(icon) = Icon::activity(card.icon) {
            when = when.push(self.cx.look.glyph(icon, 16, tone));
        }
        when = when.push(
            text(card.since.as_str())
                .size(theme.type_size("small", 12.0))
                .font(theme.font("small", REGULAR))
                .color(tone)
                .wrapping(text::Wrapping::None),
        );
        let who = row![
            self.cx
                .avatar(card.avatar.as_deref(), &card.name, 16.0, 8.0, 1.0),
            crate::elide::elide(card.name.as_str())
                .size(theme.type_size("names", 14.0))
                .font(theme.font("names", MEDIUM))
                .color(theme.secondary),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let words = column![
            who,
            crate::elide::elide(card.game.as_str())
                .size(theme.type_size("body", 14.0))
                .font(theme.font("body", SEMIBOLD))
                .color(theme.text),
            when,
        ]
        .spacing(2)
        .width(Length::Fill);
        let mut body = row![words].spacing(8).align_y(Alignment::Center);
        if let Some(thumb) = &card.thumb {
            let raised = theme.raised;
            body = body.push(match self.cx.picture(thumb, 48, 48, true) {
                Some(picture) => Element::from(Pic::square(picture, 48.0).radius(8.0)),
                None => container(Space::new())
                    .width(48)
                    .height(48)
                    .style(move |_| rounded(raised, 8))
                    .into(),
            });
        }
        let activity_themed = self
            .cx
            .themes
            .content(&theme.id, self.cx.layout.theme_content.contains(&theme.id))
            .is_some_and(|c| c.activity == yapfast_core::themes::content::Follow::Theme);
        let (rest, lit, line) = (theme.wash(0.06), theme.wash(0.12), theme.line);
        let border = if activity_themed {
            theme.surface_border("card", 8.0, 1.0, line)
        } else {
            crate::style::edge(line, 8.0)
        };
        let face = iced::widget::button(container(body).padding(8).height(CARD).clip(true))
            .padding(0)
            .width(Length::Fill)
            .on_press(Message::Members(Event::OpenActivity(card.id.clone())))
            .style(move |_, status| iced::widget::button::Style {
                background: Some(if controls::lights(status) { lit } else { rest }.into()),
                border,
                ..Default::default()
            });
        // The panel is in no scrollable: the point is the window's already.
        let user = card.id.clone();
        RightPress::new(face, move |at| Message::PersonMenu(user.clone(), at)).into()
    }
}
/// A member's line from a theme's template, as Slint's `fill`: each part
/// between " · " is kept when it names nothing, or something with words.
fn sub_line(template: &str, values: &[(&str, &str)]) -> String {
    template
        .split(" · ")
        .filter_map(|part| {
            let mut out = part.to_owned();
            let (mut any, mut filled) = (false, false);
            for (key, value) in values {
                let mark = format!("{{{key}}}");
                if out.contains(&mark) {
                    any = true;
                    filled |= !value.is_empty();
                    out = out.replace(&mark, value);
                }
            }
            (!any || filled).then_some(out)
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A group's name as Slint's member line types it: the stock headings are
/// in capitals ("ADMINS"), the group's name is not ("Admins").
fn group_name(heading: &str) -> String {
    if heading.chars().any(char::is_lowercase) {
        return heading.to_owned();
    }
    let mut letters = heading.chars();
    letters.next().map_or_else(String::new, |first| {
        let mut name = first.to_string();
        name.push_str(&letters.as_str().to_lowercase());
        name
    })
}

#[cfg(test)]
mod tests {
    use iced::Color;
    use yapfast_core::themes::content::Nameplates;

    use super::{
        dot_at, finished_ago, keeps_decoration, policy_palette, running_for, show_nameplate,
        takes_name_ink,
    };

    #[test]
    fn a_themes_list_ink_and_presence_leave_peoples_own_looks_unless_let() {
        // Off: role colours and decorations stay; a plain name takes the ink.
        assert!(!takes_name_ink(true, false));
        assert!(takes_name_ink(false, false));
        assert!(keeps_decoration(false, false));
        // Let: the theme's ink over everyone, and its presence words over
        // the decorations.
        assert!(takes_name_ink(true, true));
        assert!(!keeps_decoration(false, true));
        assert!(keeps_decoration(true, true));
    }

    #[test]
    fn an_even_list_finds_its_lines_in_view_as_the_stock_list_did() {
        use super::{MemberLine, PITCH, TOP, Tops, line_tops, lines_in_view};
        let lines: Vec<MemberLine> = (0..5000)
            .map(|at| MemberLine::Heading {
                text: format!("Group {at}"),
                icon: None,
            })
            .collect();
        let scroll = crate::sync::Scroll {
            offset: 44_000.0,
            height: 700.0,
        };
        // Stock, and a theme whose headings keep the pitch: no tops kept.
        for heading in [None, Some(PITCH - 2.0)] {
            let tops = Tops::new(&lines, PITCH, heading);
            assert!(matches!(tops, Tops::Even { .. }));
            assert_eq!(tops.in_view(&scroll), scroll.range(lines.len(), PITCH, TOP));
            assert_eq!(tops.top(1000), TOP + 1000.0 * PITCH);
            assert_eq!(tops.end(), TOP + 5000.0 * PITCH);
        }
        // Headings of their own height: each line's top.
        let tops = Tops::new(&lines, PITCH, Some(30.0));
        let each = line_tops(&lines, PITCH, Some(30.0));
        assert!(matches!(tops, Tops::Each(_)));
        assert_eq!(tops.in_view(&scroll), lines_in_view(&each, &scroll));
        assert_eq!(tops.top(10), TOP + 10.0 * 32.0);
    }

    #[test]
    fn a_walk_down_the_list_gives_each_member_its_group_and_places() {
        use super::{MemberLine, Walk};
        let lines = [
            MemberLine::Heading {
                text: "ADMINS — 1".into(),
                icon: None,
            },
            MemberLine::Heading {
                text: "ONLINE — 2".into(),
                icon: None,
            },
        ];
        let mut walk = Walk::to(&lines[..1]);
        assert_eq!(walk.groups, 1);
        let at = walk.step(&lines[1]);
        assert_eq!((at.groups, at.heading, at.place), (2, "ONLINE — 2", 0));
    }

    #[test]
    fn a_members_dot_sits_where_discords_does_unless_a_theme_moves_it() {
        // Stock: 4 px past the 32 px face's corner, as in Friends and DMs.
        assert_eq!(dot_at(32.0, None, None), iced::Point::new(20.0, 20.0));
        // A theme's own dot and overhang.
        assert_eq!(
            dot_at(40.0, Some(12.0), Some(0.0)),
            iced::Point::new(28.0, 28.0)
        );
        assert_eq!(dot_at(32.0, None, Some(-2.0)), iced::Point::new(14.0, 14.0));
    }

    #[test]
    fn the_activity_panels_times_read_in_discords_steps() {
        let now = 100 * 24 * 3_600_000;
        let ago = |seconds: u64| finished_ago(now - seconds * 1000, now);
        assert_eq!(ago(42), "42s ago");
        assert_eq!(ago(5 * 60), "5m ago");
        assert_eq!(ago(3 * 3600), "3h ago");
        // From twelve hours a day, at least one.
        assert_eq!(ago(13 * 3600), "1d ago");
        assert_eq!(ago(10 * 86_400), "1w ago");
        assert_eq!(ago(40 * 86_400), "1mo ago");
        let since = |minutes: u64| running_for(now - minutes * 60_000, now);
        assert_eq!(since(0), "just started");
        assert_eq!(since(25), "for 25 min");
        assert_eq!(since(130), "for 2 hr");
        assert_eq!(since(3 * 1440), "for 3 d");
    }

    #[test]
    fn owner_nameplate_policy_hides_or_tints_without_changing_keep() {
        let palette = Color::from_rgb(0.1, 0.2, 0.3);
        let accent = Color::from_rgb(0.8, 0.4, 0.2);
        assert!(show_nameplate(Nameplates::Keep, false));
        assert!(!show_nameplate(Nameplates::Keep, true));
        assert!(!show_nameplate(Nameplates::Hide, false));
        assert_eq!(
            policy_palette(Nameplates::Keep, Some(palette), accent),
            Some(palette)
        );
        assert_eq!(
            policy_palette(Nameplates::Tint, Some(palette), accent),
            Some(accent)
        );
        assert_eq!(
            policy_palette(Nameplates::Hide, Some(palette), accent),
            None
        );
    }
}
