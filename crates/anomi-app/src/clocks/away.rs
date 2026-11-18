//! Whether someone uses the window, as Discord's web client judges it: a
//! key pressed, a button let go or the wheel counts, a passing pointer
//! doesn't (`src/ui/away.rs`). Once nobody has for [`QUIET`], the core
//! tells Discord since when (`Action::Inactive`, with the settings' Show
//! idle), and Discord goes AFK and then idle on its own clock; what only
//! decorates holds still meanwhile (`motion.idle`: the composer's caret,
//! moving pictures, a skeleton's sweep, the typing dots).
//!
//! Input while someone uses the window costs no message: the window's
//! events' filter (`crate::window::subscription`, one for all it hears)
//! notes the time in [`LAST_USE`] through [`input`], and one wait, due
//! [`QUIET`] after the last input it saw, reads it. Only the first input after going quiet
//! comes as a message, and no wait runs while nobody uses the window. The
//! wait runs behind other windows too, as Slint's did: presence going idle
//! behind a game is what it is for, and it ends once, 15 s after the last
//! input.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use iced::{Task, keyboard, mouse};
use yapfast_core::application::Action;
use yapfast_core::clock::now_ms;

use super::{Tick, later};
use crate::app::App;
use crate::message::Message;

/// How long without input before Discord hears: its shortest AFK timeout
/// that matters is a minute, and its timeouts count from the last input,
/// not from this.
pub const QUIET: Duration = Duration::from_secs(15);

/// When someone last used the window, in Unix milliseconds: a static, as
/// the events' filter is a function without captures.
pub(super) static LAST_USE: AtomicU64 = AtomicU64::new(0);

/// Whether Discord heard nobody uses the window, for the filter, as
/// [`Away::inactive`] says it.
static QUIET_NOW: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The wait: has it been quiet for long enough?
    Check,
    /// Someone used the window after it went quiet.
    Used,
}

/// Whether Discord heard nobody uses the window.
#[derive(Default)]
pub struct Away {
    pub(crate) inactive: bool,
}

/// Watches for nobody using the window, from now: as the app starts.
pub fn watch() -> Task<Message> {
    LAST_USE.store(now_ms(), Ordering::Relaxed);
    check(QUIET)
}

fn check(after: Duration) -> Task<Message> {
    later(after, Message::Clock(Tick::Away(Event::Check)))
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Check if app.clocks.away.inactive => Task::none(),
        Event::Check => {
            let last = LAST_USE.load(Ordering::Relaxed);
            let quiet = Duration::from_millis(now_ms().saturating_sub(last));
            if quiet < QUIET {
                return check(QUIET - quiet);
            }
            app.clocks.away.inactive = true;
            tell(app)
        }
        Event::Used => {
            LAST_USE.store(now_ms(), Ordering::Relaxed);
            if !app.clocks.away.inactive {
                return Task::none();
            }
            app.clocks.away.inactive = false;
            Task::batch([tell(app), check(QUIET)])
        }
    }
}

/// Tells the core again, as the settings' Show idle changed (I19).
pub fn show_idle_changed(app: &mut App) -> Task<Message> {
    if app.clocks.away.inactive {
        tell(app)
    } else {
        Task::none()
    }
}

/// What moves only while someone may be looking holds or goes on, and the
/// core hears since when nobody used the window, or that someone does.
fn tell(app: &mut App) -> Task<Message> {
    let inactive = app.clocks.away.inactive;
    QUIET_NOW.store(inactive, Ordering::Relaxed);
    app.motion.idle = inactive;
    let since = inactive.then(|| LAST_USE.load(Ordering::Relaxed));
    let show_idle = app.layout.show_idle.unwrap_or(true);
    app.dispatch(Action::Inactive { since, show_idle })
}

/// The window's input: noted without a message while someone uses it, and
/// the first after it went quiet as [`Event::Used`].
pub fn input(input: &iced::Event) -> Option<Message> {
    if !is_use(input) {
        return None;
    }
    if QUIET_NOW.load(Ordering::Relaxed) {
        return Some(Message::Clock(Tick::Away(Event::Used)));
    }
    LAST_USE.store(now_ms(), Ordering::Relaxed);
    None
}

/// Whether an event is input Discord's web client counts as someone using
/// it: a key pressed, a button let go, or the wheel.
fn is_use(input: &iced::Event) -> bool {
    use iced::Event::{Keyboard, Mouse};
    matches!(
        input,
        Keyboard(keyboard::Event::KeyPressed { .. })
            | Mouse(mouse::Event::ButtonReleased(_) | mouse::Event::WheelScrolled { .. })
    )
}
