//! The core's backend: started by `Effect::Start`, sent commands, and its
//! events brought to `update` as [`Message::Backend`] in batches.
//!
//! The events come on a Tokio receiver. A `Task` waits on it and hands
//! `update` whatever has arrived, up to 256 at once, as the Slint app's
//! event loop does (`EVENT_BATCH` in `src/ui.rs`); a quiet backend wakes
//! nothing. Each start counts a run: events of an earlier run are dropped.

use iced::futures::{StreamExt, stream};
use iced::task::{self, Task};
use yapfast_core::backend::Backend;
use yapfast_core::chat::{Command, Event, Start};

use crate::app::App;
use crate::effects;
use crate::message::{Message, Once};

/// The most events applied together before the window shows them.
const BATCH: usize = 256;

/// The running backend, and the task bringing its events. Dropping it
/// aborts the task first, then releases the backend.
#[derive(Default)]
pub struct Connection {
    events: Option<task::Handle>,
    backend: Option<Backend>,
    run: u64,
}

/// Starts a backend in place of any running one.
pub fn start(app: &mut App, start: Start) -> Task<Message> {
    stop(app);
    app.backend.run += 1;
    let run = app.backend.run;
    let (backend, events) = Backend::start(app.runtime.clone(), app.root.clone(), start);
    let events = stream::unfold(events, |mut events| async move {
        events.recv().await.map(|event| (event, events))
    })
    .ready_chunks(BATCH);
    let (task, handle) = Task::run(events, move |batch| Message::Backend {
        run,
        events: Once::new(batch),
    })
    .chain(Task::done(Message::BackendClosed(run)))
    .abortable();
    app.backend.backend = Some(backend);
    app.backend.events = Some(handle.abort_on_drop());
    task
}

/// Stops the backend: its events first, then the backend.
pub fn stop(app: &mut App) {
    app.backend.events = None;
    app.backend.backend = None;
}

/// Sends a command; one that can't go is the core's to undo.
pub fn send(app: &mut App, command: Command) {
    let refused = match &app.backend.backend {
        Some(backend) => backend.send(command).err(),
        None => Some(command),
    };
    if let Some(command) = refused {
        app.core.backend_failed(&command);
    }
}

/// Applies a batch of events, each followed by what it asks, as the Slint
/// app's `Adapter::apply` does (a command an event sends goes before the
/// next event applies), then shows the result once.
pub fn receive(app: &mut App, run: u64, events: Once<Vec<Event>>) -> Task<Message> {
    if run != app.backend.run {
        return Task::none();
    }
    let mut tasks = Vec::new();
    for event in events.take().unwrap_or_default() {
        let asked = app.core.apply(event);
        tasks.push(effects::run(app, asked));
    }
    app.refresh();
    Task::batch(tasks)
}

/// The backend's events ended: it stopped.
pub fn closed(app: &mut App, run: u64) -> Task<Message> {
    if run == app.backend.run && app.backend.backend.is_some() {
        stop(app);
        app.core.stopped();
        app.refresh();
    }
    Task::none()
}
