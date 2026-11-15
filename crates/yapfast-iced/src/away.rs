//! Whether someone is using Yapfast, judged as Discord's web client judges
//! it and as the Slint client's `away.rs` does: a key, a click or the wheel
//! in the window counts, a passing pointer doesn't. Once nobody has for
//! [`QUIET`], Discord hears since when (`Action::Inactive`, with Settings'
//! Show idle), and goes AFK and then idle on its own clock; moving
//! pictures hold still until someone is back ([`App::plays`]).
//!
//! Input while someone is using it costs no message: the event filter
//! notes the time in [`LAST_USE`], and one timer, due [`QUIET`] after the
//! last check, reads it. Only the first input after going quiet comes as
//! a message, and no timer runs then.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use iced::{Subscription, Task};
use yapfast_core::application::Action;

use crate::app::{App, Message};

/// How long without input before Yapfast tells Discord. Discord's shortest
/// AFK timeout that matters is a minute; its timeouts count from the last
/// input, not from this.
pub const QUIET: Duration = Duration::from_secs(15);

/// When someone last used the window, in Unix milliseconds.
pub(crate) static LAST_USE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// The timer: has it been quiet for long enough?
    Check,
    /// Someone used the window after it went quiet.
    Used,
}

/// Whether Discord heard nobody is using Yapfast.
#[derive(Default)]
pub struct Away {
    pub inactive: bool,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// Whether a window event is input Discord's web client counts as someone
/// using it: a key pressed, a button released, or the wheel.
fn is_use(event: &iced::Event) -> bool {
    use iced::{keyboard, mouse};
    matches!(
        event,
        iced::Event::Keyboard(keyboard::Event::KeyPressed { .. })
            | iced::Event::Mouse(
                mouse::Event::ButtonReleased(_) | mouse::Event::WheelScrolled { .. }
            )
    )
}

impl App {
    /// Watches for nobody using Yapfast, from now.
    pub fn watch_away(&mut self) -> Task<Message> {
        LAST_USE.store(now(), Ordering::Relaxed);
        crate::after(QUIET, Message::Away(Event::Check))
    }

    pub fn away_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Check if self.away.inactive => Task::none(),
            Event::Check => {
                let quiet =
                    Duration::from_millis(now().saturating_sub(LAST_USE.load(Ordering::Relaxed)));
                if quiet < QUIET {
                    return crate::after(QUIET - quiet, Message::Away(Event::Check));
                }
                self.away.inactive = true;
                self.tell_away();
                Task::none()
            }
            Event::Used => {
                LAST_USE.store(now(), Ordering::Relaxed);
                if !self.away.inactive {
                    return Task::none();
                }
                self.away.inactive = false;
                self.tell_away();
                Task::batch([
                    self.sync(),
                    crate::after(QUIET, Message::Away(Event::Check)),
                ])
            }
        }
    }

    /// Tells Discord again, as Show idle changed.
    pub fn idle_setting_changed(&mut self) {
        if self.away.inactive {
            self.tell_away();
        }
    }

    fn tell_away(&mut self) {
        let since = self.away.inactive.then(|| LAST_USE.load(Ordering::Relaxed));
        let show_idle = self.layout.prefs.show_idle.unwrap_or(true);
        self.client.dispatch(Action::Inactive { since, show_idle });
    }

    /// The window's input: noted without a message while someone is using
    /// it, and the first once it went quiet as [`Event::Used`].
    pub fn away_subscription(&self) -> Subscription<Message> {
        if self.away.inactive {
            iced::event::listen_with(|event, _, _| {
                is_use(&event).then_some(Message::Away(Event::Used))
            })
        } else {
            iced::event::listen_with(|event, _, _| {
                if is_use(&event) {
                    LAST_USE.store(now(), Ordering::Relaxed);
                }
                None
            })
        }
    }
}
