//! The member list beside a server channel's timeline: the Activity panel
//! of Discord's feed over it, then each group's heading and its members
//! with their looks (`looks::MemberLook`), virtualized. A click opens
//! someone's card, a right click their menu (`person_menu.rs`).

use crate::widgets::scroll_view;
use iced::{
    Alignment, Color, Element, Length, Padding, Point, Rectangle, Size, Task, Vector,
    widget::{Space, column, container, mouse_area, row, scrollable::Viewport, text, tooltip},
};
use yapfast_core::{
    application::{Action, AppState},
    chat::{ActivityKind, ChatMember, Presence},
    i18n::{self, tr},
};

use crate::{
    app::{App, Message, key, point},
    controls::{self, bar, lit_style},
    ctx::Ctx,
    fonts::{MEDIUM, SEMIBOLD},
    header,
    looks::{self, Chip, MemberLine, MemberLook},
    people::StatusInk,
    people::{NameInk, Person},
    pictures::{Face, Icon},
    profile,
    style::{
        CHIP, CROWN, HEADING, MEMBER_HOVER, MEMBER_OPEN, faded, fill, hex, rounded, scrollbar,
    },
    sync::Scroll,
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
    /// The width the list had as its edge was pressed, while it's dragged.
    pub drag: Option<f32>,
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
            drag: None,
        }
    }
}

pub const WIDTH: f32 = 265.0;
/// The least and the most its edge drags it to.
const LEAST: f32 = 200.0;
const MOST: f32 = 480.0;

/// The member list's width: as its edge was dragged, else [`WIDTH`].
pub fn width(layout: &crate::layout::Layout) -> f32 {
    layout
        .members_width
        .map_or(WIDTH, |width| width.clamp(LEAST, MOST))
}

/// `width` within what the list may take: 200 to 480 px, and no more than
/// leaves the conversation 420 px.
fn room(app: &App, width: f32) -> f32 {
    let most = app.window.width - crate::conversation::chat_left(app) - crate::thread::CHAT_KEPT;
    width.clamp(LEAST, most.clamp(LEAST, MOST))
}
/// Where each line starts after the last, and the room above the first.
pub const PITCH: f32 = 44.0;
pub const TOP: f32 = 8.0;
/// A line's height.
const ROW: f32 = 42.0;
/// Discord's boosting pink, the same in every theme.
const BOOST: u32 = 0xff73fa;
/// The Activity panel: its header, a card and the room each card keeps
/// under it, the room above and below the panel, and how many cards show
/// until it's opened.
const ACTIVITY_HEAD: f32 = 32.0;
const CARD: f32 = 70.0;
const CARD_PITCH: f32 = CARD + 8.0;
const PANEL_TOP: f32 = 8.0;
const PANEL_BOTTOM: f32 = 4.0;
const FIRST_CARDS: usize = 3;

/// What's done in the member list and its Activity panel.
#[derive(Clone, Debug)]
pub enum Event {
    /// The list's viewport, as its scrollable reports it.
    Scrolled(Viewport),
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
}

/// A card of the Activity panel, from Discord's feed.
#[derive(Clone, Debug, PartialEq)]
pub struct ActivityCard {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    /// The member's role colour, kept with the feed instead of looked up
    /// again on each view.
    pub color: Option<u32>,
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
        Event::Point(id, entered) => {
            point(&mut app.pointed.member, id, entered);
            app.ask()
        }
        Event::Open(id) => open_member(app, &id),
        Event::Resize(by) => {
            let from = *app.roster.drag.get_or_insert(width(&app.layout));
            app.layout.members_width = Some(room(app, from - by));
            Task::none()
        }
        Event::Resized => {
            app.roster.drag = None;
            crate::layout::store(app.client.root(), "members_width", width(&app.layout));
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
            let left = app.window.width - width(&app.layout);
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
    let top = header::HEIGHT + 1.0 + activity_height(app) + TOP + index as f32 * PITCH
        - app.roster.scroll.offset;
    let left = app.window.width - width(&app.layout);
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
    use yapfast_core::{discord::parse_room, yapfast_discord::activity_feed::Kind};
    let Some((asked, people)) = &state.activity_people else {
        return Vec::new();
    };
    let server = |room| parse_room(room).map(|(server, _)| server);
    if server(asked) != server(room) {
        return Vec::new();
    }
    state
        .activity_feed
        .iter()
        .filter_map(|entry| {
            let person = people.iter().find(|person| person.id == entry.author)?;
            let live = entry.live_at(now_ms);
            let member = members.iter().find(|member| *member.id == *entry.author);
            // Discord's client leaves out what someone offline "is" doing.
            if live && member.is_some_and(|member| member.presence == Some(Presence::Offline)) {
                return None;
            }
            // Borrow the matching presence activity; copying it also copied
            // its artwork and rich-presence strings for every feed entry.
            let activity = member.filter(|_| live).and_then(|member| {
                member.activities.iter().find(|activity| match entry.kind {
                    Kind::Game | Kind::Launched => {
                        activity.kind == ActivityKind::Playing && activity.name == entry.title
                    }
                    Kind::Listened => activity.kind == ActivityKind::Listening,
                    Kind::Watched => activity.kind == ActivityKind::Watching,
                    Kind::TopGame | Kind::TopArtist => false,
                })
            });
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
                color: member.and_then(|member| member.color),
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
pub fn account_plate<'a>(
    ctx: crate::ctx::Ctx<'a>,
    height: f32,
    hot: bool,
    play: bool,
) -> Option<(Element<'a, Message>, Option<Color>)> {
    let state = ctx.state;
    let plate = state
        .account_nameplate
        .as_ref()
        .filter(|_| !state.hide_nameplates)?;
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
    Some((picture, palette))
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
    let strength = if pointed || selected { 0.9 } else { 0.6 } * opacity;
    plate_picture(ctx, url, height, radius, strength, pointed)
}

impl<'a> View<'a> {
    pub fn members_view(self) -> Element<'a, Message> {
        let (first, last) = self.s.scroll.range(self.s.lines.len(), PITCH, TOP);
        let total = TOP + self.s.lines.len() as f32 * PITCH + 10.0;
        let mut rows = Rows::new(total);
        for (index, line) in self.s.lines.iter().enumerate().take(last).skip(first) {
            let top = TOP + index as f32 * PITCH;
            let (id, row) = match line {
                MemberLine::Heading { text, icon } => {
                    (text.as_str(), self.heading_row(text, icon.as_deref()))
                }
                MemberLine::Member(look) => (look.id.as_str(), self.member_row(look)),
            };
            rows.push(key(id), top, ROW, container(row).padding([0, 8]));
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
                    .size(13)
                    .color(self.cx.look.theme.muted)
                    .width(Length::Fill),
            )
            .padding(16)
        });
        let list: Element<'a, Message> = column![]
            .push(self.activity_panel())
            .push(note)
            .push(list)
            .into();
        let list = container(row![bar(edge, 1, Length::Fill), list])
            .width(width(self.cx.layout))
            .height(Length::Fill)
            .style(move |_| fill(ground));
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
        iced::widget::stack![list, splitter].into()
    }

    /// A group's heading: 13 px medium in the channel grey, 20 px above,
    /// its role's icon before it.
    fn heading_row(self, heading: &str, icon: Option<&str>) -> Element<'a, Message> {
        let mut line = row![].spacing(8).align_y(Alignment::Center);
        if let Some(icon) = icon.and_then(|icon| self.cx.picture(icon, 20, 20, false)) {
            line = line.push(Pic::square(icon, 20.0));
        }
        line = line.push(
            text(heading.to_owned())
                .size(13)
                .font(MEDIUM)
                .color(hex(HEADING)),
        );
        container(line)
            .padding(Padding {
                top: 20.0,
                right: 0.0,
                bottom: 4.0,
                left: 8.0,
            })
            .height(ROW)
            .align_y(Alignment::Center)
            .into()
    }

    /// A member's row: the face with its decoration (playing while pointed
    /// at) and presence, the name in its style's font and its role's
    /// colour, the role icon, server tag, APP tag, crown, boosting gem and
    /// timeout's clock, the activity line, and the nameplate behind it all.
    fn member_row(self, look: &'a MemberLook) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
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
        let face = Face::new(look.avatar.as_deref(), &look.name, 32.0)
            .opacity(avatar_opacity)
            .decoration(look.decoration.as_deref(), pointed)
            .dot(
                look.presence
                    .filter(|presence| *presence != Presence::Offline)
                    .map(|presence| (presence, rgb(theme.sidebar), Point::new(20.0, 20.0))),
            );
        let color = faded(look.color.map_or(theme.secondary, hex), opacity);
        let name = self.cx.person_name(
            &look.name,
            NameInk::new(16.0, 500, color).style(look.style.as_ref()),
        );
        let mut name_line = row![name].spacing(4).align_y(Alignment::Center);
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
                    size: 12.0,
                    font: SEMIBOLD,
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
            let crown = self.cx.look.icon(Icon::Crown, 14, CROWN);
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
        if !look.status.is_empty() {
            person = person.sub(self.cx.status_line(
                &look.status,
                StatusInk::under_name(self.cx.look.theme, opacity),
            ));
        }
        let plate = look
            .nameplate
            .as_ref()
            .and_then(|(url, _)| nameplate(self.cx, url, (ROW, 8.0), pointed, selected, opacity));
        let content = self
            .cx
            .person(person)
            .width(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: if look.nameplate.is_some() { 12.0 } else { 8.0 },
                bottom: 0.0,
                left: 8.0,
            })
            .height(ROW);
        // The ground the nameplate fades into.
        let ground = if selected {
            theme.active
        } else if pointed {
            theme.hover
        } else {
            theme.sidebar
        };
        let palette = look
            .nameplate
            .as_ref()
            .and_then(|(_, palette)| *palette)
            .map(hex);
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
        let area = mouse_area(
            container(plated)
                .width(Length::Fill)
                .height(ROW)
                .clip(true)
                .style(move |_| container::Style {
                    background: if selected {
                        Some(MEMBER_OPEN.into())
                    } else {
                        pointed.then(|| MEMBER_HOVER.into())
                    },
                    border: crate::style::round(8),
                    ..Default::default()
                }),
        )
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
        Vector::new(
            self.cx.look.window.width - width(self.cx.layout) + 1.0,
            // Drawn only while the list shows.
            header::HEIGHT + 1.0 + self.s.activity_height(self.cx.state, true),
        )
    }

    /// The Activity panel over the list: its heading, hide/show control and
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
            .size(13)
            .font(MEDIUM)
            .color(theme.channel_read),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let (toggle, toggle_label) = if hidden {
            (Icon::EyeOff, tr("Show Activity Cards"))
        } else {
            (Icon::Settings, tr("Hide Activity Cards"))
        };
        head = head.push(
            self.cx
                .look
                .icon_button(toggle, toggle_label, Message::Members(Event::HideActivity))
                .size(16.0, 12)
                .ink(theme.channel_read),
        );
        head = head.push(Space::new().width(Length::Fill));
        if opens {
            let icon = if self.s.activity_open && !hidden {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            };
            head = head.push(self.cx.look.glyph(icon, 16, theme.channel_read));
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
                .size(12)
                .line_height(1.0)
                .color(tone)
                .wrapping(text::Wrapping::None),
        );
        let who = row![
            self.cx
                .avatar(card.avatar.as_deref(), &card.name, 16.0, 8.0, 1.0),
            crate::elide::elide(card.name.as_str())
                .size(14)
                .line_height(18.0)
                .font(MEDIUM)
                .color(card.color.map_or(theme.secondary, hex)),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let words = column![
            who,
            crate::elide::elide(card.game.as_str())
                .size(14)
                .line_height(18.0)
                .font(SEMIBOLD)
                .color(theme.text),
            when,
        ]
        .spacing(1)
        .width(Length::Fill);
        let mut body = row![words].spacing(8).align_y(Alignment::Center);
        if let Some(picture) = card
            .thumb
            .as_deref()
            .and_then(|thumb| self.cx.picture(thumb, 48, 48, true))
        {
            body = body.push(Pic::square(picture, 48.0).radius(8.0));
        } else if matches!(card.icon, 1 | 5) {
            body = body.push(self.cx.look.drawn(Icon::UnknownGame, 48));
        } else if card.thumb.is_some() {
            let raised = theme.raised;
            body = body.push(
                container(Space::new())
                    .width(48)
                    .height(48)
                    .style(move |_| rounded(raised, 8)),
            );
        }
        let (rest, lit, line) = (theme.wash(0.06), theme.wash(0.12), theme.line);
        let face = iced::widget::button(
            container(body)
                .padding(8)
                .height(CARD)
                .align_y(Alignment::Center)
                .clip(true),
        )
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::Members(Event::OpenActivity(card.id.clone())))
        .style(move |_, status| iced::widget::button::Style {
            background: Some(if controls::lights(status) { lit } else { rest }.into()),
            border: crate::style::edge(line, 8.0),
            ..Default::default()
        });
        // The panel is in no scrollable: the point is the window's already.
        let user = card.id.clone();
        RightPress::new(face, move |at| Message::PersonMenu(user.clone(), at)).into()
    }
}

#[cfg(test)]
mod tests {
    use super::{finished_ago, running_for};

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
}
