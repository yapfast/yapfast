//! Dragging a Discord server's channels and categories in the sidebar to
//! move them, as the Slint client's `RoomList`: each category and each
//! channel showing outside a folded one has a place, as Discord's client
//! lists them to drop on (pinned channels and threads stay put). A row
//! pressed and moved 4 px starts a drag where the core lets this account
//! drag it (`Application::draggable`); the rows it can't land among dim,
//! it follows the pointer faintly, a line marks where it would land, and
//! the list scrolls near its ends. Let go over a place it may drop on, it
//! moves there (`Application::landing`, `Action::MoveChannel`), which may
//! first ask whether a channel takes its new category's permissions
//! (`server_dialogs`, the core's `MoveSync`).
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
    droppable: Vec<bool>,
    /// A category is dragged, or a voice channel.
    category: bool,
    voice: bool,
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

/// The rows' places: none on the DM home, in a server that isn't
/// Discord's, or while the list is filtered.
pub fn places(rows: &[SideRow], discord: bool, filtered: bool) -> Places {
    let mut places = Places {
        ids: Vec::new(),
        of_row: vec![None; rows.len()],
    };
    if !discord || filtered {
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

impl App {
    pub fn arrange_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Start(place, grab) => return self.start_arranging(place, grab),
            Event::Take(place, user) => self.start_moving(place, user),
            Event::Onto(room, over) => {
                if let Some(moving) = &mut self.moving_person {
                    if over {
                        moving.hovered = Some(room);
                    } else if moving.hovered.as_ref() == Some(&room) {
                        moving.hovered = None;
                    }
                }
            }
            Event::Cancel => self.moving_person = None,
            Event::Over(place, over) => {
                if let Some(arranging) = &mut self.arranging {
                    if over {
                        arranging.hovered = Some(place);
                    } else if arranging.hovered == Some(place) {
                        arranging.hovered = None;
                    }
                }
            }
            Event::At(at) => {
                if let Some(arranging) = &mut self.arranging {
                    arranging.y = at.y;
                }
                if let Some(moving) = &mut self.moving_person {
                    moving.at = at;
                }
            }
            Event::Drop if self.moving_person.is_some() => return self.finish_moving(),
            Event::Drop => return self.finish_arranging(),
            Event::Scroll => {
                let Some(y) = self.drag_y() else {
                    return Task::none();
                };
                let (top, bottom) = self.side_list_span();
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
    fn start_arranging(&mut self, place: usize, grab: f32) -> Task<Message> {
        let Some(room) = self.side_places.ids.get(place) else {
            return Task::none();
        };
        if self.arranging.is_some()
            || self.moving_person.is_some()
            || !self.client.app.draggable(room)
        {
            return Task::none();
        }
        let Some(row) = self
            .side_places
            .of_row
            .iter()
            .position(|known| *known == Some(place))
            .and_then(|index| self.side.get(index))
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
        let droppable = self.client.app.droppable(&self.side_places.ids, place);
        self.arranging = Some(Arranging {
            dragged: place,
            grab,
            y: self.pressed.get().y,
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
        self.voice_card_off = true;
        Task::none()
    }

    /// Drops what's dragged on the place it's over, if it may land there.
    fn finish_arranging(&mut self) -> Task<Message> {
        let Some(arranging) = self.arranging.take() else {
            return Task::none();
        };
        let Some(onto) = arranging
            .hovered
            .filter(|onto| *onto != arranging.dragged && arranging.droppable[*onto])
        else {
            return Task::none();
        };
        let places = &self.side_places.ids;
        let landing = self.client.app.landing(places, arranging.dragged, onto);
        if let Some(landing) = landing {
            let room = places[arranging.dragged].clone();
            self.client.dispatch(Action::MoveChannel { room, landing });
        }
        self.sync()
    }

    /// Starts dragging `user` from the voice channel `place`, if their
    /// menu's Move to offers a channel listed.
    fn start_moving(&mut self, place: String, user: String) {
        if self.arranging.is_some() || self.moving_person.is_some() {
            return;
        }
        let Some(person) = self.voice_person_listed(&place, &user) else {
            return;
        };
        let (name, avatar) = (person.name.clone(), person.avatar.clone());
        self.moving_person = Some(Moving {
            place,
            user,
            targets: HashSet::new(),
            hovered: None,
            at: self.pressed.get(),
            name,
            avatar,
        });
        self.moving_targets();
        // A busy voice channel's card would cover the list.
        if self.moving_person.is_some() {
            self.voice_card_off = true;
        }
    }

    /// Who is listed under the voice channel `place`, by their id.
    fn voice_person_listed(&self, place: &str, user: &str) -> Option<&VoicePerson> {
        self.side.iter().find_map(|row| match row {
            SideRow::Room(room) if room.id == place => {
                room.people.iter().find(|person| person.id == user)
            }
            _ => None,
        })
    }

    /// The voice channels listed that someone dragged may drop on, again
    /// as the list changes; the drag stops once there's none, as when they
    /// left the channel or another list shows.
    pub fn moving_targets(&mut self) {
        let Some(moving) = &self.moving_person else {
            return;
        };
        let offered = self.client.app.move_targets(&moving.place, &moving.user);
        let targets: HashSet<String> = offered
            .into_iter()
            .map(|(room, _)| room)
            .filter(|room| {
                self.side
                    .iter()
                    .any(|row| matches!(row, SideRow::Room(listed) if listed.id == *room))
            })
            .collect();
        let listed = self
            .voice_person_listed(&moving.place, &moving.user)
            .is_some();
        match &mut self.moving_person {
            Some(moving) if listed && !targets.is_empty() => moving.targets = targets,
            _ => self.moving_person = None,
        }
    }

    /// Drops whoever is dragged on the voice channel the pointer is over,
    /// within the list, if they may go there.
    fn finish_moving(&mut self) -> Task<Message> {
        let Some(moving) = self.moving_person.take() else {
            return Task::none();
        };
        let (top, bottom) = self.side_list_span();
        let left = crate::rail::WIDTH;
        let within = (left..left + sidebar::WIDTH).contains(&moving.at.x)
            && (top..bottom).contains(&moving.at.y);
        let action = moving
            .hovered
            .filter(|room| within && moving.targets.contains(room))
            .and_then(|room| self.client.app.move_to(&moving.place, &moving.user, &room));
        match action {
            Some(action) => {
                self.client.dispatch(action);
                self.sync()
            }
            None => Task::none(),
        }
    }

    /// The pointer's height in the window while a drag is on.
    fn drag_y(&self) -> Option<f32> {
        self.arranging
            .as_ref()
            .map(|arranging| arranging.y)
            .or(self.moving_person.as_ref().map(|moving| moving.at.y))
    }

    /// The pointer's moves and the button's release while a drag is on,
    /// Escape while someone is dragged, and the list's scrolling while
    /// it's near an end.
    pub fn arrange_subscription(&self) -> Option<Subscription<Message>> {
        let y = self.drag_y()?;
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
        let (top, bottom) = self.side_list_span();
        if y < top + EDGE || y > bottom - EDGE {
            let scroll = iced::time::every(EVERY).map(|_| message(Event::Scroll));
            return Some(Subscription::batch([follow, scroll]));
        }
        Some(follow)
    }

    /// Where the sidebar's list is in the window, from its top to its
    /// bottom.
    fn side_list_span(&self) -> (f32, f32) {
        let top = self.shell.frame.height - self.window.height + self.side_top_height();
        (top, top + self.side_scroll.height)
    }

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

    /// Who is dragged, just below the pointer so the channel they'd drop
    /// on stays readable, and how far down the list.
    pub fn moving_view(&self) -> Option<(Element<'_, Message>, f32)> {
        let moving = self.moving_person.as_ref()?;
        let theme = &self.theme;
        // Faint all through, as Slint's opacity on the whole row.
        let face = row![
            self.avatar(moving.avatar.as_deref(), &moving.name, 22.0, 11.0, GHOST),
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
            .width(sidebar::WIDTH - 48.0)
            .height(30)
            .align_y(Alignment::Center)
            .style(move |_| rounded(ground, 4));
        let (top, _) = self.side_list_span();
        Some((ghost.into(), moving.at.y - top + 12.0))
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

    /// The line itself, as the Slint client's `DropLine`.
    pub fn drop_line_view(&self) -> Element<'_, Message> {
        let live = self.theme.live;
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
    pub fn arranged_view(&self) -> Option<(Element<'_, Message>, f32)> {
        let arranging = self.arranging.as_ref()?;
        let theme = &self.theme;
        // Faint all through, as Slint's opacity on the whole row.
        let (secondary, ink) = (faded(theme.secondary, GHOST), faded(theme.text, GHOST));
        let face: Element<'_, Message> = if arranging.category {
            row![
                self.glyph(
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
                self.glyph(arranging.icon, 20, secondary),
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
        let (top, _) = self.side_list_span();
        Some((ghost.into(), arranging.y - top - arranging.grab))
    }
}
