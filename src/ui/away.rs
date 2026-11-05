//! Whether someone is using Yapfast, judged as Discord's web client judges it:
//! a key, a click or the wheel in the window counts, a passing pointer
//! doesn't. Once nobody has for [`QUIET`], Discord hears since when, and
//! goes AFK and then idle on its own clock; see
//! `yapfast_discord::settings::Away`. One timer runs while someone uses Yapfast,
//! and none once nobody does.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use slint::{Timer, TimerMode};
use yapfast_core::application::Action;

use super::{Adapter, YapfastWindow};

/// How long without input before Yapfast tells Discord. Discord's shortest AFK
/// timeout that matters is a minute; its timeouts count from the last
/// input, not from this.
const QUIET: Duration = Duration::from_secs(15);

pub(super) struct Away {
    last: Instant,
    /// Discord heard nobody is using Yapfast.
    inactive: bool,
    timer: Timer,
}

impl Default for Away {
    fn default() -> Self {
        Self {
            last: Instant::now(),
            inactive: false,
            timer: Timer::default(),
        }
    }
}

impl Adapter {
    /// Whether nobody has used Yapfast for a while, so animation holds still.
    pub(super) fn is_inactive(&self) -> bool {
        self.away.inactive
    }

    /// Watches for nobody using Yapfast, from now.
    pub(super) fn watch_away(&mut self) {
        self.away.last = Instant::now();
        self.wait_away(QUIET);
    }

    /// Someone used the window.
    pub(super) fn used(&mut self, window: &YapfastWindow) {
        self.away.last = Instant::now();
        if self.away.inactive {
            self.away.inactive = false;
            self.tell_away(window);
        }
        if !self.away.timer.running() {
            self.wait_away(QUIET);
        }
    }

    /// Tells Discord again, as the setting to show idle changed.
    pub(super) fn idle_setting_changed(&mut self, window: &YapfastWindow) {
        if self.away.inactive {
            self.tell_away(window);
        }
    }

    fn wait_away(&self, wait: Duration) {
        let (adapter, window) = (self.weak.clone(), self.window.clone());
        self.away.timer.start(TimerMode::SingleShot, wait, move || {
            let (Some(adapter), Some(window)) = (adapter.upgrade(), window.upgrade()) else {
                return;
            };
            let mut adapter = adapter.borrow_mut();
            let quiet = adapter.away.last.elapsed();
            if quiet < QUIET {
                adapter.wait_away(QUIET - quiet);
                return;
            }
            adapter.away.inactive = true;
            adapter.tell_away(&window);
        });
    }

    fn tell_away(&mut self, window: &YapfastWindow) {
        let since = self.away.inactive.then(|| {
            let last = SystemTime::now() - self.away.last.elapsed();
            last.duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_millis() as u64)
        });
        let show_idle = self.layout.show_idle.unwrap_or(true);
        self.action(window, Action::Inactive { since, show_idle });
    }
}

/// Whether a window event is input Discord's web client counts as someone
/// using it: a key, a click, or the wheel.
pub(super) fn is_use(event: &slint::winit_030::winit::event::WindowEvent) -> bool {
    use slint::winit_030::winit::event::{ElementState, WindowEvent};
    match event {
        WindowEvent::KeyboardInput { event, .. } => event.state == ElementState::Pressed,
        WindowEvent::MouseInput { state, .. } => *state == ElementState::Released,
        WindowEvent::MouseWheel { .. } => true,
        _ => false,
    }
}
