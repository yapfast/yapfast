//! The timeline's and the message's intents, with its reactions',
//! attachments' and embeds': the timeline's callbacks of the Slint window
//! (`media_viewport`, `earlier`, `later`, the jumps, `reading-changed`,
//! the keys; `src/ui/callbacks.rs:707`, `:960`, `:971`, `:1183`) and the
//! message's (`:1275`, `:2033`-`:2151`).
//!
//! The list reports the rows in view ([`shown`]): the core hears which
//! messages show, for their previews, and whether the newest is being read
//! ([`reading`]); their pictures are asked for. After every update
//! ([`after`]) the list does what the projection asked of it (a jump, the
//! end), more history loads at an edge the view nears, and the one clock
//! the rows need (midnight, or an app's answer giving up) is set while the
//! window is in front.

mod keys;
mod parts;
mod save;

use std::time::Duration;

use iced::Task;
use yapfast_core::application::Action;

use anomi_chat::timeline::list_id;
use anomi_vm::chat::Intent;
use anomi_vm::chat::message::Intent as MessageIntent;
use anomi_vm::chat::timeline::{self, Intent as TimelineIntent};
use anomi_widgets::list::operation;

use crate::app::{App, later};
use crate::message::Message;
use crate::project::chat::timeline::Ask;

/// How long a jumped-to message stays lit (`show_message`).
const JUMP_LIGHT: Duration = Duration::from_millis(2500);

/// A wait of the timeline's own, ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    /// A jump's light fades, unless another jump came since.
    Faded(u64),
    /// The pointer stayed on a reaction chip, unless it left since.
    Hovered(u64),
    /// A row's words change by themselves now: midnight, or an app gave up.
    Wake(u64),
}

/// `Timeline`, `Message`, `Reactions`, `Attachments` and `Embeds`.
pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Timeline(intent) => timeline(app, intent),
        Intent::Message(intent) => message(app, intent),
        Intent::Reactions(intent) => parts::reactions(app, intent),
        Intent::Attachments(intent) => parts::attachments(app, intent),
        Intent::Embeds(intent) => parts::embeds(app, intent),
        intent => {
            tracing::debug!(?intent, "not the timeline's");
            Task::none()
        }
    }
}

/// Alt+Up in the composer: the keyboard goes to the newest message.
pub(crate) fn browse(app: &mut App) -> Task<Message> {
    keys::browse(app)
}

fn timeline(app: &mut App, intent: TimelineIntent) -> Task<Message> {
    match intent {
        TimelineIntent::Shown(report) => shown(app, report),
        TimelineIntent::Earlier => app.dispatch(Action::Earlier),
        TimelineIntent::Later => app.dispatch(Action::Later),
        TimelineIntent::JumpToUnread => app.dispatch(Action::JumpToUnread),
        TimelineIntent::JumpToPresent => {
            app.shown.timeline.latest_due = true;
            let task = app.dispatch(Action::JumpToPresent);
            // Already at the newest: the list goes to its end at once.
            if !app.core.state.detached {
                app.shown.timeline.asks.push(Ask::End);
            }
            task
        }
        intent => keys::handle(app, intent),
    }
}

/// The list told which rows are in view: the core hears which messages
/// show, with six either side, and whether the newest is read; their
/// pictures are asked for.
fn shown(app: &mut App, report: timeline::Shown) -> Task<Message> {
    let Some(room) = app.shown.timeline.room().map(str::to_owned) else {
        return Task::none();
    };
    let timeline = &mut app.vm.chat.timeline;
    timeline.shown = Some(report.span);
    let items: Vec<String> = report
        .keys
        .iter()
        .filter_map(|&key| {
            let index = timeline.index_of(key)?;
            app.shown.timeline.message_at(index)
        })
        .map(|message| message.item.clone())
        .collect();
    let at_end = report.at_end;
    tracing::debug!(
        items = items.len(),
        at_end,
        following = report.following,
        "the rows in view"
    );
    app.shown.timeline.report = Some(report);
    let visible = app.dispatch(Action::VisibleMessages {
        room,
        items,
        at_end,
    });
    crate::project::chat::timeline::want(
        &app.core,
        &app.vm.chat.timeline,
        &app.shown.timeline,
        &mut app.pictures,
    );
    Task::batch([visible, reading(app)])
}

/// Tells the core whether the newest message is being read, as it changes:
/// the window in front, the list at its newest and following it, or a jump
/// placed at its end (`reading-latest`, `ui/yapfast.slint:1818`). Sent with
/// each report of the rows in view and as the window's focus changes.
pub fn reading(app: &mut App) -> Task<Message> {
    let reads = app
        .shown
        .timeline
        .report
        .as_ref()
        .is_some_and(|report| app.vm.chat.timeline.reads(report, app.motion.front));
    if reads == app.core.state.reading {
        return Task::none();
    }
    app.dispatch(Action::Reading(reads))
}

fn message(app: &mut App, intent: MessageIntent) -> Task<Message> {
    if let MessageIntent::Point(key) = intent {
        light(app, Some(key));
        return Task::none();
    }
    parts::message(app, intent)
}

/// The message the pointer is on lies lit, with its bar.
fn light(app: &mut App, key: Option<u64>) {
    if app.shown.timeline.lit == key {
        return;
    }
    app.shown.timeline.lit = key;
    crate::project::chat::timeline::light(
        &app.core,
        &mut app.vm.chat.timeline,
        &app.shown.timeline,
    );
}

/// A wait of the timeline's ended.
pub fn tick(app: &mut App, tick: Tick) -> Task<Message> {
    match tick {
        Tick::Faded(jumps) => {
            if app.shown.timeline.jumps == jumps {
                app.vm.chat.timeline.jump = None;
                return reading(app);
            }
            Task::none()
        }
        Tick::Hovered(hover) => parts::hovered(app, hover),
        Tick::Wake(at) => {
            if app.shown.timeline.armed != Some(at) {
                return Task::none();
            }
            app.shown.timeline.armed = None;
            if !app.motion.front {
                // Coming to the front sets it again, and redoes the rows.
                return Task::none();
            }
            app.shown.timeline.wake_rows();
            app.refresh();
            Task::none()
        }
    }
}

/// What follows any update: the list does what the projection asked, more
/// history loads where the view nears an edge that has it, and the rows'
/// clock is set while the window is in front.
pub fn after(app: &mut App) -> Task<Message> {
    let mut tasks = Vec::new();
    let room = app.shown.timeline.room().map(str::to_owned);
    for ask in std::mem::take(&mut app.shown.timeline.asks) {
        let Some(room) = &room else { break };
        tasks.push(match ask {
            Ask::Jump(key, place) => {
                let jumps = app.shown.timeline.jumps;
                Task::batch([
                    operation::jump_to(list_id(room), key, place),
                    later(JUMP_LIGHT, Message::Timeline(Tick::Faded(jumps))),
                ])
            }
            Ask::End => operation::to_end(list_id(room)),
        });
    }
    let timeline = &app.vm.chat.timeline;
    if timeline.wants_earlier() {
        tasks.push(app.dispatch(Action::Earlier));
    } else if timeline.wants_later() {
        tasks.push(app.dispatch(Action::Later));
    }
    tasks.push(clock(app));
    Task::batch(tasks)
}

/// The one wait the rows need, while a timeline shows and the window is in
/// front: to when their words change by themselves.
fn clock(app: &mut App) -> Task<Message> {
    let shown = &mut app.shown.timeline;
    let Some(wake) = shown
        .wake
        .filter(|_| app.motion.front && shown.room().is_some())
    else {
        return Task::none();
    };
    if shown.armed == Some(wake) {
        return Task::none();
    }
    shown.armed = Some(wake);
    let now = yapfast_core::clock::now_ms() / 1000;
    // A second past it, so the clock reads the new day.
    let wait = Duration::from_secs(wake.saturating_sub(now) + 1);
    later(wait, Message::Timeline(Tick::Wake(wake)))
}
