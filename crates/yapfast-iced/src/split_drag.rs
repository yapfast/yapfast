//! Dragging a channel's or a DM's row out of the sidebar onto the
//! conversation's right half opens it in split view, with Split view and
//! its Drag rows to open switch on (`split_drag`). A row whose press opens
//! it arms the drag as it's pressed (`Event::Press`); once the pointer has
//! moved [`START`] px with the button still down, a faint copy of the row
//! follows it and the right half lights as a drop zone while the pointer is
//! over it. Let go there, the room opens beside the open one (the room
//! already open moves there, as the core's `open_split` does); let go
//! anywhere else, or Escape, nothing opens. A row that drags to rearrange
//! the channels goes the same way: `arrange` asks [`over_zone`] as it's let
//! go. The window follows the pointer only while a drag is armed.

use iced::{
    Alignment, Element, Length, Point, Subscription, Task,
    widget::{Space, column, container, row, stack},
};
use yapfast_core::application::Action;

use crate::{
    app::{App, Message},
    controls::placed,
    elide::elide,
    fonts::{BOLD, MEDIUM},
    pictures::Icon,
    style::{faded, rounded},
};

/// How far the pointer goes with the button down before a press is a drag.
const START: f32 = 8.0;
/// How faint the copy following the pointer is.
const GHOST: f32 = 0.7;

/// A row pressed while dragging rows to split view is on.
#[derive(Clone, Debug)]
pub struct SplitDrag {
    pub room: String,
    name: String,
    icon: Icon,
    /// Where it was pressed, and where the pointer is now, in the window.
    from: Point,
    at: Point,
    /// It moved far enough to be a drag.
    pub dragging: bool,
}

#[derive(Clone)]
pub enum Event {
    /// The row of `room` was pressed: the drag is armed, and the press
    /// does what it did.
    Press(String, Box<Message>),
    /// The pointer moved, in the window.
    At(Point),
    /// The button went up.
    Drop,
    /// Escape: nothing opens.
    Cancel,
}

/// Rows drag to split view: it's on, and so is its switch.
pub fn drags(layout: &crate::layout::Layout) -> bool {
    layout.split_view && layout.split_drag.unwrap_or(true)
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Press(room, then) => {
            let state = app.client.state();
            let listed = state.rooms.iter().find(|listed| listed.id == room);
            if drags(&app.layout)
                && (state.can_split(&room) || state.selected.as_deref() == Some(room.as_str()))
                && let Some(listed) = listed
            {
                let at = app.pressed.get();
                app.split_drag = Some(SplitDrag {
                    room: room.clone(),
                    name: listed.name.clone(),
                    icon: if listed.direct {
                        Icon::Dm
                    } else {
                        crate::sidebar::room_icon(listed)
                    },
                    from: at,
                    at,
                    dragging: false,
                });
            }
            return app.update(*then);
        }
        Event::At(at) => {
            if let Some(drag) = &mut app.split_drag {
                drag.at = at;
                drag.dragging |= drag.from.distance(at) >= START;
            }
        }
        Event::Drop => {
            let Some(drag) = app.split_drag.take() else {
                return Task::none();
            };
            if drag.dragging && over_zone(app, drag.at) {
                app.client.dispatch(Action::OpenSplit(drag.room));
                return app.sync();
            }
        }
        Event::Cancel => app.split_drag = None,
    }
    Task::none()
}

/// The conversation's right half, where a row dropped opens in split view:
/// left, top, width and height in the window.
fn zone(app: &App) -> (f32, f32, f32, f32) {
    let left = crate::conversation::chat_left(app);
    let right = crate::conversation::chat_right(app).max(left);
    let middle = left + (right - left) / 2.0;
    let top = app.shell.frame.height - app.window.height;
    (middle, top, right - middle, app.window.height)
}

/// Whether `at`, in the window, is over the drop zone.
pub fn over_zone(app: &App, at: Point) -> bool {
    let (left, top, width, height) = zone(app);
    at.x >= left && at.x <= left + width && at.y >= top && at.y <= top + height
}

/// The pointer's moves and the button's release while a drag is armed, and
/// Escape.
pub fn subscription(app: &App) -> Option<Subscription<Message>> {
    app.split_drag.as_ref()?;
    Some(iced::event::listen_with(|event, _, _| match event {
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
            Some(Message::SplitDrag(Event::At(position)))
        }
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
            Some(Message::SplitDrag(Event::Drop))
        }
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            ..
        }) => Some(Message::SplitDrag(Event::Cancel)),
        _ => None,
    }))
}

/// While a row is dragged toward split view: the drop zone over the
/// conversation's right half, lit while the pointer is over it, and the
/// faint copy of the row under the pointer. A channel dragged to rearrange
/// shows the zone too once the pointer leaves the sidebar
/// (`arrange::split_target`).
pub fn view(app: &App) -> Option<Element<'_, Message>> {
    let (at, name, icon) = match &app.split_drag {
        Some(drag) if drag.dragging => (drag.at, drag.name.as_str(), drag.icon),
        _ => crate::arrange::split_target(app)?,
    };
    let ctx = app.ctx();
    let theme = ctx.look.theme;
    let lit = over_zone(app, at);
    let (left, top, width, height) = zone(app);
    let accent = theme.accent;
    let words = column![
        ctx.look.glyph(Icon::ArrowsUpDown, 28, theme.text),
        elide("Open in split view")
            .size(16)
            .font(BOLD)
            .color(theme.text),
    ]
    .spacing(8)
    .align_x(Alignment::Center);
    let drop = container(words)
        .center_x(width)
        .center_y(height)
        .style(move |_| iced::widget::container::Style {
            border: iced::Border {
                color: accent,
                width: 2.0,
                radius: 8.0.into(),
            },
            ..rounded(faded(accent, if lit { 0.28 } else { 0.1 }), 8)
        });
    let ink = faded(theme.text, GHOST);
    let ground = faded(theme.active, GHOST);
    let ghost = container(
        row![
            ctx.look.glyph(icon, 18, ink),
            elide(name).size(15).font(MEDIUM).color(ink),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([0, 10])
    .height(30)
    .width(Length::Shrink)
    .align_y(Alignment::Center)
    .style(move |_| rounded(ground, 4));
    Some(
        stack![
            Space::new().width(Length::Fill).height(Length::Fill),
            placed(drop, left, top),
            placed(ghost, at.x + 12.0, at.y + 12.0),
        ]
        .into(),
    )
}
