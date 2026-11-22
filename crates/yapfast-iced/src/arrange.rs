//! Dragging a Discord server's channels and categories in the sidebar to
//! move them: each category and each channel showing outside a folded one
//! has a place, as Discord's client lists them to drop on (pinned channels
//! and threads stay put). A row pressed and moved 4 px starts a drag where
//! the core lets this account drag it (`Application::draggable`); the rows
//! it can't land among dim, it follows the pointer faintly, a line marks
//! where it would land, and the list scrolls near its ends. Let go over a
//! place it may drop on, it moves there (`Application::landing`,
//! `Action::MoveChannel`), which may first ask whether a channel takes its
//! new category's permissions (`server_dialogs`, the core's `MoveSync`).
//!
//! Someone listed under a voice channel drags the same way onto another
//! voice channel, which moves them as their menu's Move to does
//! (`Application::move_to`): the channels Move to offers stay bright and
//! the rest dim, the one under the pointer is outlined, they follow the
//! pointer just below it, and let go anywhere else, or Escape, nothing
//! moves.

use std::{collections::HashSet, time::Duration};

use iced::{
    Alignment, Element, Length, Point, Subscription, Task,
    widget::{container, operation, row, scrollable::AbsoluteOffset},
};
use yapfast_core::application::Action;
use yapfast_core::chat::VoicePerson;

use crate::{
    app::{App, Message},
    elide::elide,
    fonts::{BOLD, MEDIUM},
    pictures::Icon,
    sidebar::{self, SideRow},
    style::{faded, rounded},
};

/// How near the list's ends the pointer scrolls it, how far each step, and
/// how often.
const EDGE: f32 = 40.0;
const STEP: f32 = 8.0;
const EVERY: Duration = Duration::from_millis(16);
/// The line where it would land: its height, and its inset from the left.
pub const LINE: f32 = 4.0;
/// How faint the dragged row is, and the rows it can't land among.
const GHOST: f32 = 0.6;
pub const DIM: f32 = 0.3;

/// The places of the sidebar's rows, while it lists a Discord server's
/// channels in full.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Places {
    /// Each place's room id: a category's or a channel's.
    pub ids: Vec<String>,
    /// Each row's place, by the row's index.
    pub of_row: Vec<Option<usize>>,
}

/// A drag going on.
#[derive(Clone, Debug)]
pub struct Arranging {
    /// The dragged row's place, and where on it it was pressed.
    pub dragged: usize,
    grab: f32,
    /// The pointer's height in the window.
    y: f32,
    /// The place the pointer is over.
    pub hovered: Option<usize>,
    pub(crate) droppable: Vec<bool>,
    /// A category is dragged, or a voice channel.
    pub(crate) category: bool,
    pub(crate) voice: bool,
    /// What the faint copy following the pointer shows.
    name: String,
    icon: Icon,
    collapsed: bool,
    height: f32,
}

/// Someone being dragged from a voice channel to another.
#[derive(Clone, Debug)]
pub struct Moving {
    /// The voice channel they're in, and who.
    pub place: String,
    pub user: String,
    /// The voice channels they may drop on, of those listed.
    pub targets: HashSet<String>,
    /// The voice channel the pointer is over.
    pub hovered: Option<String>,
    /// The pointer, in the window.
    at: Point,
    /// What follows the pointer.
    name: String,
    avatar: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// The row at this place was pressed and moved, this far down it.
    Start(usize, f32),
    /// Someone under the voice channel was pressed and moved.
    Take(String, String),
    /// The pointer came onto the voice channel's row or left it, while
    /// someone is dragged.
    Onto(String, bool),
    /// Escape: nothing moves.
    Cancel,
    /// The pointer came onto the row at this place or left it.
    Over(usize, bool),
    /// The pointer moved, in the window.
    At(Point),
    /// The button went up.
    Drop,
    /// Near an end of the list, a step of scrolling.
    Scroll,
}

fn message(event: Event) -> Message {
    Message::Arrange(event)
}

/// The rows' places: none on the DM home, or in a server that isn't
/// Discord's.
pub fn places(rows: &[SideRow], discord: bool) -> Places {
    let mut places = Places {
        ids: Vec::new(),
        of_row: vec![None; rows.len()],
    };
    if !discord {
        return places;
    }
    // Pinned channels come first, before a line.
    let mut pinned = rows.iter().any(|row| matches!(row, SideRow::Pinned));
    let mut folded = false;
    for (index, row) in rows.iter().enumerate() {
        let id = match row {
            SideRow::PinnedEnd => {
                pinned = false;
                None
            }
            _ if pinned => None,
            SideRow::Heading {
                fold, collapsed, ..
            } => {
                folded = *collapsed;
                Some(fold)
            }
            SideRow::Room(room) if !room.thread && !folded => Some(&room.id),
            _ => None,
        };
        if let Some(id) = id {
            places.of_row[index] = Some(places.ids.len());
            places.ids.push(id.clone());
        }
    }
    places
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Start(place, grab) => return start_arranging(app, place, grab),
        Event::Take(place, user) => start_moving(app, place, user),
        Event::Onto(room, over) => {
            if let Some(moving) = &mut app.sidebar.moving_person {
                if over {
                    moving.hovered = Some(room);
                } else if moving.hovered.as_ref() == Some(&room) {
                    moving.hovered = None;
                }
            }
        }
        Event::Cancel => app.sidebar.moving_person = None,
        Event::Over(place, over) => {
            if let Some(arranging) = &mut app.sidebar.arranging {
                if over {
                    arranging.hovered = Some(place);
                } else if arranging.hovered == Some(place) {
                    arranging.hovered = None;
                }
            }
        }
        Event::At(at) => {
            if let Some(arranging) = &mut app.sidebar.arranging {
                arranging.y = at.y;
            }
            if let Some(moving) = &mut app.sidebar.moving_person {
                moving.at = at;
            }
        }
        Event::Drop if app.sidebar.moving_person.is_some() => return finish_moving(app),
        Event::Drop => return finish_arranging(app),
        Event::Scroll => {
            let Some(y) = drag_y(app) else {
                return Task::none();
            };
            let (top, bottom) = side_list_span(app);
            let by = if y < top + EDGE {
                -STEP
            } else if y > bottom - EDGE {
                STEP
            } else {
                return Task::none();
            };
            return operation::scroll_by(sidebar::LIST, AbsoluteOffset { x: 0.0, y: by });
        }
    }
    Task::none()
}

/// Starts dragging the row at `place`, if this account may move it.
fn start_arranging(app: &mut App, place: usize, grab: f32) -> Task<Message> {
    let Some(room) = app.sidebar.places.ids.get(place) else {
        return Task::none();
    };
    if app.sidebar.arranging.is_some()
        || app.sidebar.moving_person.is_some()
        || !app.client.app.draggable(room)
    {
        return Task::none();
    }
    let Some(row) = app
        .sidebar
        .places
        .of_row
        .iter()
        .position(|known| *known == Some(place))
        .and_then(|index| app.sidebar.rows.get(index))
    else {
        return Task::none();
    };
    let (name, icon, collapsed, category, voice, height) = match row {
        SideRow::Heading {
            name, collapsed, ..
        } => (
            name.clone(),
            Icon::ChevronDown,
            *collapsed,
            true,
            false,
            36.0,
        ),
        SideRow::Room(room) => (
            room.name.clone(),
            room.icon,
            false,
            false,
            room.voice,
            sidebar::ROW,
        ),
        _ => return Task::none(),
    };
    let droppable = app.client.app.droppable(&app.sidebar.places.ids, place);
    app.sidebar.arranging = Some(Arranging {
        dragged: place,
        grab,
        y: app.pressed.get().y,
        hovered: Some(place),
        droppable,
        category,
        voice,
        name,
        icon,
        collapsed,
        height,
    });
    // A busy voice channel's card would cover the list.
    app.sidebar.voice_card_off = true;
    Task::none()
}

/// Drops what's dragged on the place it's over, if it may land there.
fn finish_arranging(app: &mut App) -> Task<Message> {
    let Some(arranging) = app.sidebar.arranging.take() else {
        return Task::none();
    };
    let Some(onto) = arranging
        .hovered
        .filter(|onto| *onto != arranging.dragged && arranging.droppable[*onto])
    else {
        return Task::none();
    };
    let places = &app.sidebar.places.ids;
    let landing = app.client.app.landing(places, arranging.dragged, onto);
    if let Some(landing) = landing {
        let room = places[arranging.dragged].clone();
        app.client.dispatch(Action::MoveChannel { room, landing });
    }
    app.sync()
}

/// Starts dragging `user` from the voice channel `place`, if their
/// menu's Move to offers a channel listed.
fn start_moving(app: &mut App, place: String, user: String) {
    if app.sidebar.arranging.is_some() || app.sidebar.moving_person.is_some() {
        return;
    }
    let Some(person) = voice_person_listed(app, &place, &user) else {
        return;
    };
    let (name, avatar) = (person.name.clone(), person.avatar.clone());
    app.sidebar.moving_person = Some(Moving {
        place,
        user,
        targets: HashSet::new(),
        hovered: None,
        at: app.pressed.get(),
        name,
        avatar,
    });
    moving_targets(app);
    // A busy voice channel's card would cover the list.
    if app.sidebar.moving_person.is_some() {
        app.sidebar.voice_card_off = true;
    }
}

/// Who is listed under the voice channel `place`, by their id.
fn voice_person_listed<'a>(app: &'a App, place: &str, user: &str) -> Option<&'a VoicePerson> {
    app.sidebar.rows.iter().find_map(|row| match row {
        SideRow::Room(room) if room.id == place => {
            room.people.iter().find(|person| person.id == user)
        }
        _ => None,
    })
}

/// The voice channels listed that someone dragged may drop on, again
/// as the list changes; the drag stops once there's none, as when they
/// left the channel or another list shows.
pub fn moving_targets(app: &mut App) {
    let Some(moving) = &app.sidebar.moving_person else {
        return;
    };
    let offered = app.client.app.move_targets(&moving.place, &moving.user);
    let targets: HashSet<String> = offered
        .into_iter()
        .map(|(room, _)| room)
        .filter(|room| {
            app.sidebar
                .rows
                .iter()
                .any(|row| matches!(row, SideRow::Room(listed) if listed.id == *room))
        })
        .collect();
    let listed = voice_person_listed(app, &moving.place, &moving.user).is_some();
    match &mut app.sidebar.moving_person {
        Some(moving) if listed && !targets.is_empty() => moving.targets = targets,
        _ => app.sidebar.moving_person = None,
    }
}

/// Drops whoever is dragged on the voice channel the pointer is over,
/// within the list, if they may go there.
fn finish_moving(app: &mut App) -> Task<Message> {
    let Some(moving) = app.sidebar.moving_person.take() else {
        return Task::none();
    };
    let (top, bottom) = side_list_span(app);
    let package = app.themes.package_layout(&app.theme.id);
    let left = crate::conversation::sidebar_left(package, app.themes.chrome(&app.theme.id))
        + app.themes.glass_gap(&app.theme.id);
    let within = (left..left + sidebar::width(&app.layout, &app.theme)).contains(&moving.at.x)
        && (top..bottom).contains(&moving.at.y);
    let action = moving
        .hovered
        .filter(|room| within && moving.targets.contains(room))
        .and_then(|room| app.client.app.move_to(&moving.place, &moving.user, &room));
    match action {
        Some(action) => {
            app.client.dispatch(action);
            app.sync()
        }
        None => Task::none(),
    }
}

/// The pointer's height in the window while a drag is on.
fn drag_y(app: &App) -> Option<f32> {
    app.sidebar
        .arranging
        .as_ref()
        .map(|arranging| arranging.y)
        .or(app.sidebar.moving_person.as_ref().map(|moving| moving.at.y))
}

/// The pointer's moves and the button's release while a drag is on,
/// Escape while someone is dragged, and the list's scrolling while
/// it's near an end.
pub fn arrange_subscription(app: &App) -> Option<Subscription<Message>> {
    let y = drag_y(app)?;
    let follow = iced::event::listen_with(|event, _, _| match event {
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
            Some(message(Event::At(position)))
        }
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
            Some(message(Event::Drop))
        }
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            ..
        }) => Some(message(Event::Cancel)),
        _ => None,
    });
    let (top, bottom) = side_list_span(app);
    if y < top + EDGE || y > bottom - EDGE {
        let scroll = iced::time::every(EVERY).map(|_| message(Event::Scroll));
        return Some(Subscription::batch([follow, scroll]));
    }
    Some(follow)
}

/// Where the sidebar's list is in the window, from its top to its
/// bottom.
fn side_list_span(app: &App) -> (f32, f32) {
    app.sidebar
        .list_span(app.shell.frame.height - app.window.height)
}

impl<'a> crate::sidebar::View<'a> {
    /// Who is dragged, just below the pointer so the channel they'd drop
    /// on stays readable, and how far down the list.
    pub fn moving_view(self) -> Option<(Element<'a, Message>, f32)> {
        let moving = self.s.moving_person.as_ref()?;
        let theme = self.cx.look.theme;
        // Faint all through: each part fades by `GHOST`.
        let face = row![
            self.cx
                .avatar(moving.avatar.as_deref(), &moving.name, 22.0, 11.0, GHOST),
            elide(moving.name.as_str())
                .size(15)
                .font(MEDIUM)
                .color(faded(theme.text, GHOST)),
        ]
        .spacing(8)
        .padding(iced::Padding {
            left: 4.0,
            right: 8.0,
            ..iced::Padding::ZERO
        })
        .align_y(Alignment::Center);
        let ground = faded(theme.active, GHOST);
        let ghost = container(face)
            .width(sidebar::width(self.cx.layout, self.cx.look.theme) - 48.0)
            .height(30)
            .align_y(Alignment::Center)
            .style(move |_| rounded(ground, 4));
        let (top, _) = self.s.list_span(self.cx.above);
        Some((ghost.into(), moving.at.y - top + 12.0))
    }

    /// The line itself.
    pub fn drop_line_view(self) -> Element<'a, Message> {
        let live = self.cx.look.theme.live;
        container(iced::widget::Space::new())
            .width(Length::Fill)
            .height(LINE)
            .style(move |_| container::Style {
                shadow: iced::Shadow {
                    color: iced::Color::from_rgba8(0, 0, 0, 0.4),
                    offset: iced::Vector::ZERO,
                    blur_radius: 3.0,
                },
                ..rounded(live, 2)
            })
            .into()
    }

    /// The dragged row's faint copy, and how far down the list it is.
    pub fn arranged_view(self) -> Option<(Element<'a, Message>, f32)> {
        let arranging = self.s.arranging.as_ref()?;
        let theme = self.cx.look.theme;
        // Faint all through: each part fades by `GHOST`.
        let (secondary, ink) = (faded(theme.secondary, GHOST), faded(theme.text, GHOST));
        let face: Element<'a, Message> = if arranging.category {
            row![
                self.cx.look.glyph(
                    if arranging.collapsed {
                        Icon::ChevronRight
                    } else {
                        Icon::ChevronDown
                    },
                    12,
                    secondary
                ),
                elide(arranging.name.as_str())
                    .size(12)
                    .font(BOLD)
                    .color(secondary),
            ]
            .spacing(2)
            .padding([0, 2])
            .align_y(Alignment::Center)
            .into()
        } else {
            row![
                self.cx.look.glyph(arranging.icon, 20, secondary),
                elide(arranging.name.as_str())
                    .size(16)
                    .font(MEDIUM)
                    .color(ink),
            ]
            .spacing(8)
            .padding([0, 8])
            .align_y(Alignment::Center)
            .into()
        };
        let ground = faded(theme.active, GHOST);
        let radius = if arranging.category { 4 } else { 6 };
        let ghost = container(face)
            .width(Length::Fill)
            .height(arranging.height)
            .align_y(Alignment::Center)
            .style(move |_| rounded(ground, radius));
        let (top, _) = self.s.list_span(self.cx.above);
        Some((ghost.into(), arranging.y - top - arranging.grab))
    }
}
