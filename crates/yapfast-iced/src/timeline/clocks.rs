//! The times the timeline shows, moved on as they turn: each minute while
//! something shows one, each midnight for the messages' days.

use std::time::Duration;

use iced::Task;
use yapfast_core::chat::ChatMessage;

use crate::app::{App, Message};

/// The timers of the clocks on screen: one to the next minute while
/// something shows minutes (a busy voice channel's call, an open poll,
/// the Activity panel's cards, Active Now, the account's game), one to the
/// next midnight while a timeline shows; and what of the first the lists
/// worked out as they were made.
#[derive(Default)]
pub struct Clocks {
    pub minute_armed: bool,
    pub day_armed: bool,
    /// A sidebar row shows its call's length; a loaded poll counts down.
    pub calls: bool,
    pub polls: bool,
}

/// How long until the next whole minute, a little past it.
pub(super) fn to_next_minute() -> Duration {
    let ms = yapfast_core::clock::now_ms();
    Duration::from_millis(60_000 - ms % 60_000 + 50)
}

/// How long until the next local midnight, a little past it.
pub(super) fn to_next_midnight() -> Duration {
    let now = chrono::Local::now();
    let tomorrow = now
        .date_naive()
        .succ_opt()
        .and_then(|day| day.and_hms_opt(0, 0, 1));
    tomorrow
        .and_then(|midnight| midnight.and_local_timezone(chrono::Local).earliest())
        .and_then(|midnight| (midnight - now).to_std().ok())
        .unwrap_or(Duration::from_secs(3600))
}

/// The clock message times are shown on: 12-hour, and with seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    pub twelve_hour: bool,
    pub seconds: bool,
}

impl Clock {
    /// When a message was sent, as its author line says it: the core's own
    /// words for one without a time.
    pub fn sent(self, message: &ChatMessage, now: u64) -> String {
        if message.timestamp == 0 {
            return message.time.clone();
        }
        yapfast_core::clock::sent(message.timestamp, now, self.twelve_hour, self.seconds)
    }
}

/// Whether something on screen shows minutes.
pub(super) fn minutes_shown(app: &App) -> bool {
    app.timeline.clocks.calls
        || app.timeline.clocks.polls
        || (crate::header::members_shown(app) && !app.roster.activity_cards.is_empty())
        || crate::active_now::active_now_minutes(app)
        || crate::status::popover_playing(app)
}

/// Arms the timer to the next minute while something shows minutes,
/// and to the next midnight while a timeline shows: one wake each,
/// no interval running for nothing.
pub fn arm_clocks(app: &mut App) -> Task<Message> {
    let mut tasks = Vec::new();
    if !app.timeline.clocks.minute_armed && minutes_shown(app) {
        app.timeline.clocks.minute_armed = true;
        app.minute = yapfast_core::clock::now_ms() / 1000 / 60 * 60;
        tasks.push(crate::after(to_next_minute(), Message::Minute));
    }
    if !app.timeline.clocks.day_armed && !app.timeline.messages.is_empty() {
        app.timeline.clocks.day_armed = true;
        tasks.push(crate::after(to_next_midnight(), Message::Day));
    }
    Task::batch(tasks)
}

/// The minute turned: the clocks that show it move on.
pub fn minute_turned(app: &mut App) -> Task<Message> {
    app.timeline.clocks.minute_armed = false;
    app.minute = yapfast_core::clock::now_ms() / 1000 / 60 * 60;
    app.timeline.clocks.polls = crate::polls::open_poll(&app.timeline.messages, app.minute);
    crate::members::make_activity_cards(app);
    arm_clocks(app)
}

/// The day turned: each message's time is worked out again, so
/// today's read as yesterday's. The rows keep their text and heights.
pub fn day_turned(app: &mut App) -> Task<Message> {
    app.timeline.clocks.day_armed = false;
    let prefs = &app.layout.prefs;
    let clock = Clock {
        twelve_hour: prefs.twelve_hour_clock,
        seconds: prefs.clock_seconds,
    };
    let now = yapfast_core::clock::now_ms() / 1000;
    for (look, message) in app
        .timeline
        .looks
        .iter_mut()
        .zip(app.timeline.messages.iter())
    {
        look.time = clock.sent(message, now).into();
    }
    if let Some(aside) = &mut app.aside {
        for (look, message) in aside.looks.iter_mut().zip(aside.messages.iter()) {
            look.time = clock.sent(message, now).into();
        }
    }
    arm_clocks(app)
}
