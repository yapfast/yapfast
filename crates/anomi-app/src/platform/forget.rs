//! Closing the window to the tray gives its memory back, and the app sitting
//! there behind a game does nothing (`drop_window` and `detach` in
//! `src/ui/shell.rs`, upstream's `forget_window`). The window goes, with its
//! widgets, its surface and the renderer's caches; what only a shown window
//! needs goes with it: the menus, pickers, cards and popovers open, Settings
//! through its own close, the timeline's and the lists' rows, the pictures'
//! copies made for drawing. Half a second later the heap gives back what that
//! freed. While there's no window nothing is projected: `App::show` keeps
//! only the layout and the tray up to date, and the window shown again is
//! made from the core's state in one refresh, back at the messages it was
//! closed on. What the core keeps (drafts, the reading place, the call)
//! stays, and so do the dialogs it opened.

use std::time::Duration;

use iced::{Task, window};

use anomi_vm::shell::Layer;
use anomi_widgets::list::Place;

use crate::app::{App, later};
use crate::message::Message;
use crate::project::chat::timeline::Ask;

/// How long after the window goes the heap is trimmed: iced lets go of the
/// window's last pieces as its loop goes on.
const RELEASE_AFTER: Duration = Duration::from_millis(500);

/// Where the closed window's timeline was: its room, and the message at its
/// top, unless the newest showed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    room: String,
    top: u64,
}

/// What only a shown window has, closed with it: the popups and popovers,
/// the cards, the pictures large, the call full screen and Settings. The
/// dialogs the core opened stay, and show again with the window.
fn only_shown(layer: Layer) -> bool {
    layer.popup()
        || matches!(
            layer,
            Layer::Settings
                | Layer::Topic
                | Layer::Pins
                | Layer::Threads
                | Layer::AddPeople
                | Layer::RoleCard
                | Layer::Profile
                | Layer::Card
                | Layer::MessageCard
                | Layer::Viewer
                | Layer::Video
                | Layer::FullStage
                | Layer::Inbox
        )
}

/// Closes the window into the tray, dropping what only it needs.
pub fn window(app: &mut App) -> Task<Message> {
    let Some(id) = app.window.take() else {
        return Task::none();
    };
    app.desktop.reading = reading(app);
    let behind = crate::window::focused(app, false);
    let mut tasks = vec![behind];
    let open: Vec<Layer> = app.vm.shell.layers.iter().map(|o| o.layer).collect();
    for layer in open.into_iter().filter(|&layer| only_shown(layer)) {
        use anomi_vm::shell::Intent;
        // Through each layer's own close, then gone at once: no way out
        // plays without a window.
        tasks.push(crate::intent::handle(app, Intent::Close(layer).into()));
        tasks.push(crate::intent::handle(app, Intent::Gone(layer).into()));
    }
    crate::intent::menus::close(app);
    // The call's pictures and meter go; its sound stays the core's (I18).
    let shown = &mut app.shown;
    crate::project::voice::hide(&app.core, &mut app.vm, &mut shown.voice, &mut shown.media);
    app.hidden = true;
    let vm = &mut app.vm;
    vm.rail = Default::default();
    vm.sidebar = Default::default();
    vm.chat.timeline = Default::default();
    vm.people.members = Default::default();
    app.pictures = Default::default();
    anomi_widgets::picture::clear();
    app.layout.store();
    tracing::info!("window closed to the tray");
    tasks.push(window::close(id));
    tasks.push(later(RELEASE_AFTER, Message::Release));
    Task::batch(tasks)
}

/// Half a second after the window went, unless it came back meanwhile.
pub fn release(app: &App) {
    if app.window.is_none() {
        anomi_desktop::memory::release();
    }
}

/// The window is shown again: everything it shows is made again from the
/// core's state, in one refresh.
pub fn shown_again(app: &mut App) {
    if std::mem::take(&mut app.hidden) {
        app.shown.forget();
        app.refresh();
    }
}

/// The message at the top of the timeline as it closes, unless the newest
/// showed, where the timeline opens anyway.
fn reading(app: &App) -> Option<Reading> {
    let timeline = &app.vm.chat.timeline;
    let span = timeline.shown.filter(|span| !span.near_end)?;
    Some(Reading {
        room: timeline.room.as_ref()?.id.clone(),
        top: span.top,
    })
}

/// The window opened again: its timeline goes back to the message it
/// was closed on, if that room is still open and the message there.
pub fn reading_again(app: &mut App) -> Task<Message> {
    let Some(reading) = app.desktop.reading.take() else {
        return Task::none();
    };
    let timeline = &app.vm.chat.timeline;
    let same = timeline
        .room
        .as_ref()
        .is_some_and(|room| room.id == reading.room);
    if same && timeline.rows.iter().any(|row| row.key == reading.top) {
        app.shown
            .timeline
            .asks
            .push(Ask::Jump(reading.top, Place::Top));
    }
    Task::none()
}
