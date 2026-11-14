//! Timers: asking to be woken again later.
//!
//! A timer is how a plugin does something with no one to trigger it: poll a
//! feed, expire a cache, post a reminder. The helper holds the timer, not the
//! plugin, so a timer survives eviction and an instance with one is not
//! evicted while it is pending. A plugin may hold a few, and a repeating one
//! asks for at least a second, so a timer can't be used to spin.
//!
//! A timer is named by the plugin's own tag, and the tag is what comes back
//! when it fires: two timers on one tag are one timer, and asking again
//! replaces nothing.

use crate::call::{self, method};
use crate::wire;
use crate::{Error, Result};

/// Wakes the plugin once, after this many milliseconds.
pub fn after(interval_ms: u64, tag: &str) -> Result<()> {
    schedule(method::TIMERS_AFTER, interval_ms, tag)
}

/// Wakes the plugin every this many milliseconds, the first time after one
/// interval has passed.
///
/// Below [`MIN_TIMER_INTERVAL_MS`](crate::wire::MIN_TIMER_INTERVAL_MS) this is an error rather than something the
/// helper has to refuse: a plugin that wants to run every 50 ms wants an event,
/// not a timer.
pub fn every(interval_ms: u64, tag: &str) -> Result<()> {
    if interval_ms < wire::MIN_TIMER_INTERVAL_MS {
        return Err(Error::msg(format!(
            "a repeating timer is at least {} ms",
            wire::MIN_TIMER_INTERVAL_MS
        )));
    }
    schedule(method::TIMERS_EVERY, interval_ms, tag)
}

/// Forgets the timer with this tag, if there is one.
pub fn cancel(tag: &str) -> Result<()> {
    call::ok(
        method::TIMERS_CANCEL,
        wire::TimerCancel {
            tag: tag.to_owned(),
        },
    )?;
    Ok(())
}

/// One timer, whichever kind it is: the helper distinguishes them by method.
fn schedule(method: &str, interval_ms: u64, tag: &str) -> Result<()> {
    let request = wire::TimerRequest {
        interval_ms,
        tag: tag.to_owned(),
    };
    match call::send(method, request)? {
        // A first timer is answered at once; a repeating one may be answered
        // later, in the same call the events are, and either way it is set.
        wire::HostReturn::Ok(_) | wire::HostReturn::Pending { .. } => Ok(()),
        answer => {
            call::value(method, answer)?;
            Ok(())
        }
    }
}
