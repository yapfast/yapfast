//! The caps the plugin system holds itself to.
//!
//! They live here so the parent and the helper refuse the same thing at the
//! same size, and so the docs can name one number.
//!
//! Most of them are fixed. The two the person may change live in [`Limits`],
//! which defaults to the same values these constants have always held.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// A `.wasm` file larger than this is refused before it is compiled.
pub const MAX_WASM_BYTES: u64 = 8 << 20;

/// The most one event or hook payload carries. Fields over it are truncated
/// and flagged rather than refused, so a plugin still sees the event.
pub const MAX_PAYLOAD_BYTES: usize = 64 << 10;

/// Pages of guest memory an instance gets when its manifest doesn't say: 16 MiB.
pub const DEFAULT_MEMORY_PAGES: u32 = 256;
/// Fewer pages than this can't hold a Rust PDK plugin and its input.
pub const MIN_MEMORY_PAGES: u32 = 32;
/// The most a manifest may ask for, with visible consent: 64 MiB.
pub const MAX_MEMORY_PAGES: u32 = 1024;

/// An HTTP response's default cap, and the one a manifest may ask for.
pub const MAX_HTTP_RESPONSE_BYTES: u64 = 4 << 20;
pub const MAX_HTTP_RESPONSE_CONSENTED_BYTES: u64 = 16 << 20;

/// Requests in flight, per plugin and in total.
pub const MAX_HTTP_IN_FLIGHT: usize = 4;
pub const MAX_HTTP_IN_FLIGHT_TOTAL: usize = 16;

/// A plugin's storage, by default and with a manifest request.
pub const DEFAULT_STORAGE_BYTES: u64 = 1 << 20;
pub const MAX_STORAGE_BYTES: u64 = 16 << 20;

/// Timers a plugin may hold, and the shortest interval one may ask for.
pub const MAX_TIMERS: usize = 16;
pub const MIN_TIMER_INTERVAL_MS: u64 = 1_000;

/// Topics one plugin may subscribe to, and how long a topic's name may be.
///
/// A topic is the publishing plugins' own namespace, so the cap is what keeps
/// a plugin from filling the parent's registry with names nobody publishes to.
pub const MAX_TOPICS: usize = 32;
pub const MAX_TOPIC_NAME_BYTES: usize = 128;

/// The most one `plugin.call` carries in either direction, base64 aside: a call
/// is a message between two plugins, and a megabyte of it is a payload that
/// belongs in storage, not in one frame each way.
pub const MAX_CALL_BYTES: usize = 256 << 10;

/// State handed from `yf_deactivate` to the next `yf_activate`.
pub const MAX_HANDOFF_BYTES: usize = 256 << 10;

/// The helper's own total cap, before it evicts or refuses an instance: the
/// default [`Limits`] budget, and the one a helper runs with until the person
/// changes it.
pub const MAX_HELPER_BYTES: u64 = 256 << 20;

/// A git fetch's cap on disk, and its time limit.
pub const MAX_FETCH_BYTES: u64 = 64 << 20;
pub const MAX_FETCH_SECONDS: u64 = 120;

/// The largest icon a plugin may bring: a picture the review draws at 64×64 to
/// 256×256.
pub const MAX_ICON_BYTES: u64 = 64 << 10;

/// An instance that has made no call for this long may be evicted, unless it
/// holds a timer or a request in flight: the default [`Limits`] idle timeout,
/// and the one a helper measures with until the person changes it.
pub const IDLE_EVICTION_MS: u64 = 5 * 60 * 1000;

/// Strikes within [`STRIKE_WINDOW_MS`] that suspend a plugin for the session.
pub const MAX_STRIKES: u32 = 3;
pub const STRIKE_WINDOW_MS: u64 = 60 * 1000;

/// How the parent backs off when the helper exits unexpectedly.
pub const RESTART_BACKOFF_MS: [u64; 3] = [1_000, 5_000, 30_000];
/// Crashes within this window in [`MAX_CRASHES`] leave plugins off.
pub const CRASH_WINDOW_MS: u64 = 60 * 1000;
pub const MAX_CRASHES: u32 = 3;

/// Consent dialogs one plugin may have open, and denials that silence it.
pub const MAX_DENIALS_BEFORE_SILENCE: u32 = 3;

/// How long an unanswered consent dialog counts as open. Past it the question is
/// put again rather than the host being refused for good: a window closed by
/// accident is not an answer, and the alternative is a plugin that only works
/// again once someone edits its grants file.
pub const CONSENT_WINDOW_MS: u64 = 120_000;

/// The memory budgets the Advanced settings offer, in MiB, smallest first.
pub const MEMORY_BUDGETS: [u32; 4] = [128, 256, 512, 1024];

/// The idle timeouts the Advanced settings offer, in minutes, shortest first.
pub const IDLE_MINUTES: [u32; 4] = [1, 5, 15, 60];

/// What the person allows the helper: its whole memory budget and how long an
/// instance may sit without a call before it is evicted.
///
/// This is what the two Advanced settings mean, wherever they travel: the
/// settings page keeps them, the parent sends them over the pipe as
/// [`crate::message::ToHelper::Limits`], and the helper evicts and refuses with
/// them. The offered values are [`MEMORY_BUDGETS`] and [`IDLE_MINUTES`], and a
/// number that isn't one of them is [`Limits::snapped`] to the nearest rather
/// than refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    /// The helper's whole budget for plugin memory, in MiB.
    pub memory_mib: u32,
    /// How long an instance may go without a call before it is evicted, in
    /// minutes.
    pub idle_minutes: u32,
}

impl Default for Limits {
    /// What the constants have always held: [`MAX_HELPER_BYTES`] and
    /// [`IDLE_EVICTION_MS`], 256 MiB and five minutes.
    fn default() -> Self {
        Limits {
            memory_mib: (MAX_HELPER_BYTES >> 20) as u32,
            idle_minutes: (IDLE_EVICTION_MS / 60_000) as u32,
        }
    }
}

impl Limits {
    /// The nearest step the settings offer for both numbers, the lower step on
    /// a tie, which is the same choice the settings page makes when it shows a
    /// stored value.
    ///
    /// Snapping is the whole of the defence against a number that isn't a step:
    /// a frame can carry anything, and zero MiB is a rounding question rather
    /// than a reason to empty the helper over a budget of nothing.
    pub fn snapped(self) -> Limits {
        Limits {
            memory_mib: nearest(&MEMORY_BUDGETS, self.memory_mib),
            idle_minutes: nearest(&IDLE_MINUTES, self.idle_minutes),
        }
    }

    /// The budget in bytes, which is what admission and eviction count.
    pub fn bytes(self) -> u64 {
        u64::from(self.memory_mib) << 20
    }

    /// The idle timeout, as the scheduler and the watchdog measure it.
    pub fn idle_after(self) -> Duration {
        Duration::from_secs(u64::from(self.idle_minutes) * 60)
    }
}

/// The offered step closest to `value`, the lower one on a tie.
fn nearest(steps: &[u32], value: u32) -> u32 {
    steps
        .iter()
        .copied()
        .min_by_key(|step| step.abs_diff(value))
        .unwrap_or(value)
}

#[cfg(test)]
mod limits_tests {
    use super::*;

    /// Guest memory comes in 64 KiB pages, which is what makes
    /// [`DEFAULT_MEMORY_PAGES`] 256 pages worth the 16 MiB its comment names.
    const PAGE_BYTES: u64 = 64 << 10;

    #[test]
    fn the_default_limits_are_the_caps_the_helper_always_had() {
        let limits = Limits::default();
        assert_eq!(limits.memory_mib, 256);
        assert_eq!(limits.idle_minutes, 5);
        assert_eq!(limits.bytes(), MAX_HELPER_BYTES);
        assert_eq!(
            u64::from(limits.idle_minutes) * 60_000,
            IDLE_EVICTION_MS,
            "five minutes, at the resolution the constant is in"
        );
    }

    #[test]
    fn a_limits_converts_to_the_bytes_and_duration_the_scheduler_counts() {
        let smallest = Limits {
            memory_mib: 128,
            idle_minutes: 1,
        };
        assert_eq!(smallest.bytes(), 128 << 20);
        assert_eq!(smallest.idle_after(), Duration::from_secs(60));
        let biggest = Limits {
            memory_mib: 1024,
            idle_minutes: 60,
        };
        assert_eq!(biggest.bytes(), 1024 << 20);
        assert_eq!(biggest.idle_after(), Duration::from_secs(3600));
    }

    #[test]
    fn snapping_takes_the_nearest_step_and_the_lower_one_on_a_tie() {
        // A step is itself.
        let on_step = Limits {
            memory_mib: 512,
            idle_minutes: 15,
        };
        assert_eq!(on_step.snapped(), on_step);
        // Zero snaps up to the smallest step rather than to nothing at all.
        assert_eq!(
            Limits {
                memory_mib: 0,
                idle_minutes: 0,
            }
            .snapped(),
            Limits {
                memory_mib: 128,
                idle_minutes: 1,
            }
        );
        // Something past every step snaps down to the largest one.
        assert_eq!(
            Limits {
                memory_mib: u32::MAX,
                idle_minutes: u32::MAX,
            }
            .snapped(),
            Limits {
                memory_mib: 1024,
                idle_minutes: 60,
            }
        );
        // 300 MiB is nearer 256 than 512, and three minutes is exactly between
        // one and five, where a tie goes to the lower step.
        assert_eq!(
            Limits {
                memory_mib: 300,
                idle_minutes: 3,
            }
            .snapped(),
            Limits {
                memory_mib: 256,
                idle_minutes: 1,
            }
        );
    }

    #[test]
    fn the_smallest_budget_offered_admits_one_maximum_page_instance() {
        let smallest = Limits {
            memory_mib: MEMORY_BUDGETS[0],
            idle_minutes: IDLE_MINUTES[0],
        };
        let biggest_instance = u64::from(MAX_MEMORY_PAGES) * PAGE_BYTES;
        assert!(
            smallest.bytes() >= biggest_instance,
            "{} bytes of budget must admit an instance capped at {} bytes",
            smallest.bytes(),
            biggest_instance
        );
    }
}
