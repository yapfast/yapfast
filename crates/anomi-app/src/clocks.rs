//! Every clock: what in the window changes with time alone. One wait per
//! kind of clock, each set for the moment its words next change and set
//! again from its own message: the minute (a busy voice channel's call,
//! the game's time played, a poll's time left, the Activity panel's and
//! Active Now's times), the day (the timeline's "Today", an app's answer
//! giving up) and the second (the warning before a backup code, an
//! activity's time on a profile card and Active Now's rows).
//! They run only while the window is in front and something shows them:
//! going behind another window, or to the tray, drops them (each wait is
//! an abortable task), and coming back writes every clock again at once
//! ([`front`]). A tick reaches only what shows the time
//! ([`Reach::MINUTE`], [`Reach::DAY`]), which writes again only the rows
//! whose words changed.
//!
//! Whether someone uses the window is [`away`]'s: idle after 15 s, which
//! the core tells Discord, and which holds what only decorates
//! (`motion.idle`).
//!
//! The Slint app's `ActivityClock` (`src/ui/profile.rs:252`) ticked every
//! 15 s for the voice channels' rows; upstream's iced window
//! (`timeline/clocks.rs`) wakes at the next whole minute and at midnight,
//! which this follows.

pub mod away;
#[cfg(test)]
mod tests;

use std::time::Duration;

use iced::Task;
use iced::task::Handle;
use yapfast_core::clock::now_ms;

use anomi_vm::settings::mfa;

use crate::app::App;
use crate::message::Message;
use crate::project::Reach;

/// How far past the moment its words change a wait ends, so the clock it
/// reads has turned, in milliseconds.
const PAST: u64 = 50;
const MINUTE: u64 = 60_000;
const SECOND: u64 = 1_000;

/// What reaches `update` from the clocks: a wait set for that moment, in
/// Unix milliseconds, ended; or someone used the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    Minute(u64),
    Day(u64),
    Second(u64),
    Away(away::Event),
}

/// The clocks' waits while they run, and whether someone uses the window.
#[derive(Default)]
pub struct Clocks {
    minute: Option<Wait>,
    day: Option<Wait>,
    second: Option<Wait>,
    pub(crate) away: away::Away,
}

/// A wait set to end at `due`, in Unix milliseconds: dropped, its timer
/// goes.
struct Wait {
    due: u64,
    _timer: Handle,
}

impl Clocks {
    /// Which waits run: the minute's, the day's and the second's.
    #[cfg(test)]
    pub(crate) fn running(&self) -> [bool; 3] {
        [&self.minute, &self.day, &self.second].map(Option::is_some)
    }
}

/// A clock's wait ended, or someone used the window.
pub fn update(app: &mut App, tick: Tick) -> Task<Message> {
    let (wait, due) = match tick {
        Tick::Away(event) => return away::update(app, event),
        Tick::Minute(due) => (&mut app.clocks.minute, due),
        Tick::Day(due) => (&mut app.clocks.day, due),
        Tick::Second(due) => (&mut app.clocks.second, due),
    };
    // A wait dropped or set again since: its message came too late.
    if wait.as_ref().is_none_or(|wait| wait.due != due) {
        return Task::none();
    }
    *wait = None;
    match tick {
        Tick::Minute(_) => app.show(Reach::MINUTE),
        Tick::Day(_) => {
            app.shown.timeline.gave_up(now_ms() / SECOND);
            app.shown.aside.gave_up(now_ms() / SECOND);
            app.show(Reach::DAY);
        }
        Tick::Second(_) => {
            // An activity's time on a profile card, Active Now's rows (I16).
            crate::project::people::tick(&mut app.vm);
            // The editor's preview's activities (I17).
            crate::project::editor::tick(&mut app.vm);
            if crate::intent::sign_in::counting(app) {
                return crate::intent::sign_in::mfa(app, mfa::Intent::Tick);
            }
        }
        Tick::Away(_) => {}
    }
    Task::none()
}

/// The window came to the front: every clock says the time again at once,
/// and [`arm`] sets their waits after.
pub fn front(app: &mut App) {
    app.shown.timeline.gave_up(now_ms() / SECOND);
    app.shown.aside.gave_up(now_ms() / SECOND);
    crate::project::people::tick(&mut app.vm);
    crate::project::editor::tick(&mut app.vm);
    app.show(Reach::MINUTE | Reach::DAY);
}

/// Sets each clock's wait for when its words next change while the window
/// is in front and something shows them, and drops the rest. It runs after
/// every update, so what it reads was noted as the rows were built.
pub fn arm(app: &mut App) -> Task<Message> {
    let awake = app.motion.front && !app.hidden;
    let minute = awake.then(|| minute_due(app)).flatten();
    let day = awake.then(|| day_due(app)).flatten();
    let seconds = crate::intent::sign_in::counting(app)
        || crate::project::people::seconds(&app.vm, &app.shown.people)
        || crate::project::editor::seconds(&app.vm);
    let second = (awake && seconds).then(|| now_ms() + SECOND);
    let clocks = &mut app.clocks;
    let set = [
        keep(&mut clocks.minute, minute, Tick::Minute),
        keep(&mut clocks.day, day, Tick::Day),
        keep(&mut clocks.second, second, Tick::Second),
    ];
    // Most updates set none: no batch for them.
    if set.iter().all(|task| task.units() == 0) {
        return Task::none();
    }
    Task::batch(set)
}

/// When a clock showing minutes next changes, in Unix milliseconds: a busy
/// voice channel's call at the next whole minute, as Discord's client
/// counts it; the game's time played a whole minute on from its start; a
/// poll's time left as it next reads otherwise.
fn minute_due(app: &App) -> Option<u64> {
    let voice = app.shown.sidebar.timed;
    let game = crate::intent::account::game_started(app);
    let now = now_ms();
    let poll = crate::project::chat::poll::due(&app.shown.timeline, now);
    // The Activity panel's and Active Now's times (I16).
    let people = crate::project::people::minutes(&app.vm);
    let voice = (voice || people).then(|| next_minute(0, now));
    voice
        .into_iter()
        .chain(game.map(|started| next_minute(started, now)))
        .chain(poll)
        .min()
}

/// The next whole minute after `now`, counted from `from`; both in Unix
/// milliseconds.
fn next_minute(from: u64, now: u64) -> u64 {
    from + (now.saturating_sub(from) / MINUTE + 1) * MINUTE
}

/// When the timeline's words change by themselves, while it shows: the
/// next midnight, or an app's answer giving up; the thread beside the
/// channel's too.
fn day_due(app: &App) -> Option<u64> {
    let wake =
        |timeline: &crate::project::chat::timeline::Shown| timeline.room().and(timeline.wake);
    let (channel, aside) = (wake(&app.shown.timeline), wake(&app.shown.aside));
    channel
        .into_iter()
        .chain(aside)
        .min()
        .map(|wake| wake * SECOND)
}

/// Keeps `wait` to end by `due`: one set for later is set again for
/// sooner, and none runs without a due.
fn keep(wait: &mut Option<Wait>, due: Option<u64>, tick: fn(u64) -> Tick) -> Task<Message> {
    let Some(due) = due else {
        *wait = None;
        return Task::none();
    };
    if wait.as_ref().is_some_and(|wait| wait.due <= due) {
        return Task::none();
    }
    let delay = Duration::from_millis(due.saturating_sub(now_ms()) + PAST);
    let (task, timer) = later(delay, Message::Clock(tick(due))).abortable();
    *wait = Some(Wait {
        due,
        _timer: timer.abort_on_drop(),
    });
    task
}

/// `message` after `delay`: a wait of the app's own, such as the layout's
/// or a note's. It wakes nothing until it ends; the timer starts as the
/// executor runs it.
pub fn later(delay: Duration, message: Message) -> Task<Message> {
    Task::perform(async move { tokio::time::sleep(delay).await }, move |()| {
        message
    })
}
