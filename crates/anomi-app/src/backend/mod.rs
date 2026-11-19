//! The core's backend: started by `Effect::Start`, sent commands, and its
//! events brought to `update` as [`Message::Backend`] in batches.
//!
//! The events come on a Tokio receiver. A `Task` waits on it and hands
//! `update` whatever has arrived, up to 256 at once, as the Slint app's
//! event loop does (`EVENT_BATCH` in `src/ui.rs`); a quiet backend wakes
//! nothing. What of a batch the window doesn't show waits in the run's
//! [`gate`] instead, and a batch left empty makes no message. Each start
//! counts a run: events of an earlier run are dropped.

pub mod gate;
#[cfg(test)]
mod tests;

use iced::futures::{StreamExt, future, stream};
use iced::task::{self, Task};
use yapfast_core::application::Effect;
use yapfast_core::backend::Backend;
use yapfast_core::chat::{Command, Event, Start};

use crate::app::App;
use crate::effects;
use crate::message::{Message, Once};
use crate::project::Reach;

/// The most events applied together before the window shows them.
const BATCH: usize = 256;

/// The running backend, and the task bringing its events. Dropping it
/// aborts the task first, then releases the backend.
#[derive(Default)]
pub struct Connection {
    events: Option<task::Handle>,
    backend: Option<Backend>,
    run: u64,
    /// What of this run's events waits for the window to show it.
    pub(crate) gate: gate::Gate,
}

/// Starts a backend in place of any running one.
pub fn start(app: &mut App, start: Start) -> Task<Message> {
    stop(app);
    app.backend.run += 1;
    let run = app.backend.run;
    let (backend, events) = Backend::start(app.runtime.clone(), app.root.clone(), start);
    let gate = gate::Gate::default();
    let sifting = gate.clone();
    let events = stream::unfold(events, |mut events| async move {
        events.recv().await.map(|event| (event, events))
    })
    .ready_chunks(BATCH)
    .filter_map(move |batch| {
        let shown = sifting.sift(batch);
        future::ready((!shown.is_empty()).then_some(shown))
    });
    let (task, handle) = Task::run(events, move |batch| Message::Backend {
        run,
        events: Once::new(batch),
    })
    .chain(Task::done(Message::BackendClosed(run)))
    .abortable();
    app.backend.backend = Some(backend);
    app.backend.events = Some(handle.abort_on_drop());
    app.backend.gate = gate;
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

/// Applies a batch of events and shows once what they can have reached.
pub fn receive(app: &mut App, run: u64, events: Once<Vec<Event>>) -> Task<Message> {
    if run != app.backend.run {
        return Task::none();
    }
    let (task, reach) = apply(app, events.take().unwrap_or_default());
    Task::batch([task, app.show_events(reach)])
}

/// Hands the core what waited in the gate, before a message is handled:
/// whatever it does then reads the core's state as it is. Typing in rooms
/// that don't show changes no line; the member list, while it doesn't
/// show, waits for [`follow`] to be shown again.
pub fn drain(app: &mut App) -> Task<Message> {
    let waited = app.backend.gate.take();
    if waited.is_empty() {
        return Task::none();
    }
    let (task, mut reach) = apply(app, waited);
    if !app.backend.gate.shows_members() {
        reach = reach.without(Reach::MEMBERS);
    }
    Task::batch([task, app.show_events(reach)])
}

/// Tells the gate what the window shows now, after a message was handled:
/// the member list, or the draft and the suggestions that read the
/// members; and the rooms whose typing shows. Behind another window or in
/// the tray, none. When that's more than before, what waited is taken and
/// what shows it is shown again, as it is now.
pub fn follow(app: &mut App) -> Task<Message> {
    let state = &app.core.state;
    let front = app.motion.front && !app.hidden;
    let members = front
        && (app.vm.conversation.beside.members
            || !state.draft().is_empty()
            || state.mention_query.is_some());
    let typing = [
        state.selected.as_deref(),
        state.aside.as_ref().map(|aside| aside.room.as_str()),
    ]
    .map(|room| room.filter(|_| front));
    let Some(waited) = app.backend.gate.follow(members, typing) else {
        return Task::none();
    };
    let (task, reach) = apply(app, waited);
    Task::batch([
        task,
        app.show_events(reach | Reach::MEMBERS | Reach::TYPING),
    ])
}

/// Applies events, each followed by what it asks, as the Slint app's
/// `Adapter::apply` does (a command an event sends goes before the next
/// event applies), and says what they can have reached: an event whose
/// effects ask anything but a sound, such as opening a DM that just
/// started, can change anything.
fn apply(app: &mut App, events: Vec<Event>) -> (Task<Message>, Reach) {
    let mut tasks = Vec::new();
    let mut reach = Reach::NOTHING;
    for event in events {
        reach = reach | Reach::of(&event);
        let starts = crate::intent::voice::camera::starts_a_call(&app.core.state, &event);
        let asked = app.core.apply(event);
        if !asked
            .iter()
            .all(|effect| matches!(effect, Effect::Sound(_)))
        {
            reach = Reach::EVERYTHING;
        }
        tasks.push(effects::run(app, asked));
        if starts {
            tasks.push(crate::intent::voice::camera::list(app));
        }
    }
    (Task::batch(tasks), reach)
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
