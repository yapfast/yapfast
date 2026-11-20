//! How long this start of Yapfast took: three moments, each kept once, read by
//! the plugins that hold `app.timing` as one `startup_timing` event.
//!
//! Nothing here costs more than an `Instant` per moment, and nothing is kept
//! but the first time each was marked, so a window shown again or an account
//! signed in again changes nothing.

use std::sync::OnceLock;
use std::time::Instant;

static BEGAN: OnceLock<Instant> = OnceLock::new();
static WINDOW: OnceLock<Instant> = OnceLock::new();
static READY: OnceLock<Instant> = OnceLock::new();

/// The process began: the first thing `main` does.
pub fn began() {
    let _ = BEGAN.set(Instant::now());
}

/// The window was built.
pub fn window() {
    let _ = WINDOW.set(Instant::now());
}

/// The account signed in and its lists arrived.
pub fn ready() {
    let _ = READY.set(Instant::now());
}

/// What a plugin is told: milliseconds from the process beginning to the
/// window and to the account being ready, each `None` when it hasn't happened
/// or the beginning wasn't marked (a test, say).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Timing {
    pub window_ms: Option<u64>,
    pub ready_ms: Option<u64>,
}

/// The moments so far.
pub fn timing() -> Timing {
    let since = |mark: &OnceLock<Instant>| {
        let began = BEGAN.get()?;
        let at = mark.get()?;
        Some(u64::try_from(at.saturating_duration_since(*began).as_millis()).unwrap_or(u64::MAX))
    };
    Timing {
        window_ms: since(&WINDOW),
        ready_ms: since(&READY),
    }
}
