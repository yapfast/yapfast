//! Peeking at a channel or a DM without opening it: with Peek at channels
//! on (Settings → Messages), holding Alt alone over a row of the sidebar
//! shows the room's newest messages beside the list. Nothing is opened or
//! read; letting go of Alt or leaving the row closes it and lets go of its
//! cards. The core holds the messages (`AppState::peek`).

use std::{sync::Arc, time::Duration};

use iced::{
    Alignment, Element, Length, Padding, Point, Subscription, Task,
    widget::{column, container, row},
};
use yapfast_core::application::Action;

use crate::{
    app::{App, Message},
    controls::{self, bar, placed},
    elide::elide,
    fonts::BOLD,
    found::Found,
    pictures::Icon,
    style::raised_shadow,
};

/// How long the pointer stays on a row before its messages are asked for,
/// so sweeping down the list asks for none of those it passes.
const SETTLE: Duration = Duration::from_millis(120);
const WIDTH: f32 = 420.0;
const RADIUS: f32 = 10.0;
const HEAD: f32 = 44.0;
/// From the sidebar's edge, and from the window's top and bottom.
const GAP: f32 = 8.0;

/// What peeking holds in the window.
#[derive(Default)]
pub struct Peeking {
    /// The room under the pointer while Alt is held, and where the pointer
    /// was as it came there, which the cards' top follows.
    pub wanted: Option<(String, f32)>,
    /// Where the pointer is while Alt is held with the switch on, or where
    /// it was as it came onto the row it's on.
    pub pointer: f32,
    /// The pointer came onto another sidebar row and where isn't known
    /// yet: its next move says, so a peek opened by Alt while it rests
    /// there still opens level with the row.
    pub placing: bool,
    /// The cards of the messages peeked at, made once as they came, and
    /// the room they're of.
    pub found: Vec<Found>,
    pub made_for: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// The pointer stayed on the row: ask for its messages.
    Due(String),
    /// The pointer moved, while Alt is held.
    At(Point),
}

/// Alt by itself: not with Ctrl, Shift or the logo key, which layouts and
/// shortcuts use.
fn alt_only(modifiers: iced::keyboard::Modifiers) -> bool {
    modifiers.alt() && !modifiers.control() && !modifiers.shift() && !modifiers.logo()
}

/// The room to peek at now: the sidebar's row under the pointer while Alt
/// alone is held and the window has the focus, unless it's the open one,
/// locked or a forum, whose messages are its posts'.
fn wanted(app: &App) -> Option<String> {
    if !app.layout.peek_channels || !alt_only(app.modifiers) || !app.shell.focused {
        return None;
    }
    let id = app.pointed.side.as_deref()?;
    let state = app.client.state();
    if state.selected.as_deref() == Some(id) {
        return None;
    }
    let room = state.rooms.iter().find(|room| room.id == id)?;
    (!room.locked && !room.forum).then(|| id.to_owned())
}

/// After Alt, the focus or the row under the pointer changed: the peek
/// follows. A new row is asked for once the pointer settles on it; one
/// left closes at once.
pub fn follow(app: &mut App) -> Task<Message> {
    let now = wanted(app);
    if now.as_deref() == app.peek.wanted.as_ref().map(|(id, _)| id.as_str()) {
        return Task::none();
    }
    match now {
        Some(id) => {
            app.peek.wanted = Some((id.clone(), app.peek.pointer));
            crate::after(SETTLE, Message::Peek(Event::Due(id)))
        }
        None => {
            app.peek.wanted = None;
            app.peek.found.clear();
            app.peek.made_for = None;
            if app.client.state().peek.is_some() {
                app.client.dispatch(Action::Peek(None));
                return app.sync();
            }
            Task::none()
        }
    }
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Due(id) => {
            if app
                .peek
                .wanted
                .as_ref()
                .is_some_and(|(wanted, _)| *wanted == id)
            {
                app.client.dispatch(Action::Peek(Some(id)));
                return app.sync();
            }
            Task::none()
        }
        Event::At(at) => {
            app.peek.pointer = at.y;
            app.peek.placing = false;
            Task::none()
        }
    }
}

/// Where the pointer is, while Alt alone is held with the switch on and
/// once after it comes onto a row, so the cards open level with the row.
pub fn subscription(app: &App) -> Option<Subscription<Message>> {
    (app.layout.peek_channels && (alt_only(app.modifiers) || app.peek.placing)).then(|| {
        iced::event::listen_with(|event, _, _| match event {
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                Some(Message::Peek(Event::At(position)))
            }
            _ => None,
        })
    })
}

/// The cards of what the core peeked at, made once as its messages come.
pub fn sync(app: &mut App) {
    let state = app.client.state();
    let Some(peek) = state.peek.as_ref() else {
        app.peek.found.clear();
        app.peek.made_for = None;
        return;
    };
    let Some(Ok(messages)) = peek.messages.as_ref() else {
        return;
    };
    if app.peek.made_for.as_deref() == Some(peek.room.as_str()) {
        return;
    }
    let room = peek.room.clone();
    let found = messages
        .iter()
        .cloned()
        .map(|message| crate::found::found(app, Arc::new(message)))
        .collect();
    app.peek.found = found;
    app.peek.made_for = Some(room);
}

/// The room's newest messages beside the sidebar, newest at the bottom,
/// or what to say while they come; older ones that don't fit are cut off
/// at the top. It takes no clicks: the pointer stays on the row.
pub fn view<'a>(app: &'a App, ctx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let state = ctx.state;
    let peek = state.peek.as_ref()?;
    let (_, at) = app.peek.wanted.as_ref()?;
    let theme = ctx.look.theme;
    let room = state.rooms.iter().find(|room| room.id == peek.room)?;
    let name = if room.direct {
        room.name.clone()
    } else {
        format!("#{}", room.name)
    };
    let note = match &peek.messages {
        None => Some("Loading messages...".to_owned()),
        Some(Err(error)) => Some(error.clone()),
        Some(Ok(messages)) if messages.is_empty() => Some("No messages yet.".to_owned()),
        Some(Ok(_)) => None,
    };
    let body: Element<'a, Message> = match note {
        Some(note) => container(elide(note).size(14).color(theme.muted))
            .padding(16)
            .into(),
        None => {
            let cards = app
                .peek
                .found
                .iter()
                .map(|found| crate::found::found_view(ctx, found, Message::Nothing));
            // Kept to its newest at the bottom: when the cards are taller
            // than the room, the older ones go under the top, not the
            // newest under the foot.
            iced::widget::scrollable(column(cards).spacing(8).padding(12))
                .direction(iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::hidden(),
                ))
                .anchor_bottom()
                .height(Length::Shrink)
                .into()
        }
    };
    let ground = theme.panel;
    let head = column![
        container(
            row![
                ctx.look.glyph(Icon::Eye, 20, theme.secondary),
                elide(name).size(16).font(BOLD).color(theme.text),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding(Padding::ZERO.left(16).right(10))
        .height(HEAD - 1.0)
        .align_y(Alignment::Center)
        .style(move |_| {
            crate::style::rounded(ground, crate::style::corners([RADIUS, RADIUS, 0.0, 0.0]))
        }),
        bar(theme.edge, Length::Fill, 1),
    ];
    let window = ctx.look.window;
    let top = (at - HEAD).clamp(GAP, (window.height - 2.0 * HEAD - GAP).max(GAP));
    let most = (window.height - top - GAP).max(HEAD);
    let (sidebar, edge) = (theme.sidebar, theme.edge);
    let panel = container(column![head, body])
        .width(WIDTH.min(window.width - crate::conversation::navigation_left_ctx(ctx) - 2.0 * GAP))
        .max_height(most)
        .clip(true)
        .style(move |_| iced::widget::container::Style {
            shadow: raised_shadow(),
            ..crate::style::rounded(sidebar, RADIUS)
        });
    Some(placed(
        controls::edged(panel, edge, RADIUS),
        crate::conversation::navigation_left_ctx(ctx) + GAP,
        top,
    ))
}
