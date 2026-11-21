//! Push-to-talk: in this input mode a call sends the microphone only while a
//! key is held, and for a short release delay after, rather than when the
//! voice gate hears speech. A push-to-mute key, held, sends nothing in either
//! mode. The desktop watches the keys and calls [`hold`] and [`hold_mute`];
//! Discord's microphone asks [`sending`] each 10 ms block, and a Matrix call
//! waits on [`changed`]. Both read atomics, so a block allocates nothing.
use std::{
    sync::{
        LazyLock,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use tokio::sync::watch;

/// How long the microphone keeps sending after the key comes up unless
/// changed, in milliseconds, as Discord's own default.
pub const DEFAULT_RELEASE_DELAY: u32 = 20;
/// The longest release delay Discord's slider offers, in milliseconds.
pub const MOST_RELEASE_DELAY: u32 = 2000;

static ON: AtomicBool = AtomicBool::new(false);
static HELD: AtomicBool = AtomicBool::new(false);
static MUTING: AtomicBool = AtomicBool::new(false);
static DELAY: AtomicU32 = AtomicU32::new(DEFAULT_RELEASE_DELAY);
/// When the key last came up, in milliseconds since [`START`].
static RELEASED: AtomicU64 = AtomicU64::new(0);
static START: LazyLock<Instant> = LazyLock::new(Instant::now);
/// Ticks whenever the mode or the key changes, for a call that waits on it.
static CHANGES: LazyLock<watch::Sender<()>> = LazyLock::new(|| watch::channel(()).0);

/// Chooses push-to-talk, or voice activation, for every call; a call going on
/// switches at once.
pub fn set_on(on: bool) {
    if ON.swap(on, Ordering::Relaxed) != on {
        CHANGES.send_replace(());
    }
}

/// Whether calls are in push-to-talk.
pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Sets how long the microphone keeps sending after the key comes up, in
/// milliseconds, up to [`MOST_RELEASE_DELAY`].
pub fn set_release_delay(milliseconds: u32) {
    DELAY.store(milliseconds.min(MOST_RELEASE_DELAY), Ordering::Relaxed);
}

fn release_delay() -> Duration {
    Duration::from_millis(DELAY.load(Ordering::Relaxed).into())
}

/// The key went down or came up. Repeats change nothing.
pub fn hold(held: bool) {
    if HELD.swap(held, Ordering::Relaxed) == held {
        return;
    }
    if !held {
        RELEASED.store(START.elapsed().as_millis() as u64, Ordering::Relaxed);
    }
    CHANGES.send_replace(());
}

/// Whether the key is down now.
pub fn held() -> bool {
    HELD.load(Ordering::Relaxed)
}

/// A push-to-mute key went down or came up. While one is held the microphone
/// sends nothing, in push-to-talk or voice activation, as Discord's Push to
/// Mute; it comes back with no release delay.
pub fn hold_mute(held: bool) {
    if MUTING.swap(held, Ordering::Relaxed) != held {
        CHANGES.send_replace(());
    }
}

/// Whether a push-to-mute key is down now.
pub fn muting() -> bool {
    MUTING.load(Ordering::Relaxed)
}

/// Whether the microphone sends now: no push-to-mute key is held, and the
/// push-to-talk key is down, or came up less than the release delay ago.
/// Outside push-to-talk the voice gate decides when nothing mutes it.
pub fn sending() -> bool {
    if muting() {
        return false;
    }
    if !on() {
        return true;
    }
    let held = HELD.load(Ordering::Relaxed);
    let released = Duration::from_millis(RELEASED.load(Ordering::Relaxed));
    transmits(
        held,
        START.elapsed().saturating_sub(released),
        release_delay(),
    )
}

/// Whether to send with the key `held`, or released `since` ago.
fn transmits(held: bool, since: Duration, delay: Duration) -> bool {
    held || since < delay
}

/// A receiver that sees each change of mode or keys from now on.
pub fn changes() -> watch::Receiver<()> {
    CHANGES.subscribe()
}

/// Waits until [`sending`] may have changed: the mode or the key changed, or
/// the release delay ran out.
pub async fn changed(changes: &mut watch::Receiver<()>) {
    let left = if on() && !held() {
        let released = Duration::from_millis(RELEASED.load(Ordering::Relaxed));
        release_delay().checked_sub(START.elapsed().saturating_sub(released))
    } else {
        None
    };
    match left {
        Some(left) => {
            tokio::select! {
                _ = changes.changed() => {}
                () = tokio::time::sleep(left) => {}
            }
        }
        None => {
            // The sender is static, so this never ends with an error.
            let _ = changes.changed().await;
        }
    }
}

/// Push-to-talk for Discord's microphone, block by block: whether each 10 ms
/// block is sent. It counts blocks rather than reading the clock, so the
/// release delay is in the microphone's own time.
#[cfg(feature = "discord")]
#[derive(Default)]
pub(crate) struct Talk {
    /// Blocks still to send after the key came up.
    left: u32,
}

#[cfg(feature = "discord")]
impl Talk {
    /// One 10 ms block with the key `held` or not; whether to send it.
    pub(crate) fn block(&mut self, held: bool, delay_ms: u32) -> bool {
        if held {
            // The block the key comes up in counts toward the delay.
            self.left = delay_ms.div_ceil(10);
            return true;
        }
        if self.left == 0 {
            return false;
        }
        self.left -= 1;
        true
    }
}

/// The release delay in milliseconds, for [`Talk`].
#[cfg(feature = "discord")]
pub(crate) fn release_delay_ms() -> u32 {
    DELAY.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "discord")]
    fn the_key_sends_while_held_and_for_the_delay_after() {
        let mut talk = Talk::default();
        assert!(!talk.block(false, 200), "nothing before the key");
        for _ in 0..5 {
            assert!(talk.block(true, 200));
        }
        // 200 ms is 20 more blocks after the key comes up, then silence.
        for index in 0..20 {
            assert!(talk.block(false, 200), "{index}");
        }
        assert!(!talk.block(false, 200));
        assert!(!talk.block(false, 200));
    }

    #[test]
    #[cfg(feature = "discord")]
    fn pressing_again_in_the_delay_keeps_sending_and_starts_it_over() {
        let mut talk = Talk::default();
        talk.block(true, 100);
        for _ in 0..7 {
            assert!(talk.block(false, 100));
        }
        assert!(talk.block(true, 100));
        for index in 0..10 {
            assert!(talk.block(false, 100), "{index}");
        }
        assert!(!talk.block(false, 100));
    }

    #[test]
    #[cfg(feature = "discord")]
    fn no_delay_stops_with_the_key_and_a_part_block_rounds_up() {
        let mut talk = Talk::default();
        assert!(talk.block(true, 0));
        assert!(!talk.block(false, 0));
        talk.block(true, 25);
        assert_eq!(
            (0..4).filter(|_| talk.block(false, 25)).count(),
            3,
            "25 ms is three blocks"
        );
    }

    #[test]
    fn push_to_mute_stops_sending_in_either_mode() {
        hold_mute(false);
        set_on(false);
        assert!(sending());
        let mut changes = changes();
        changes.mark_unchanged();
        hold_mute(true);
        assert!(changes.has_changed().unwrap(), "a call hears it at once");
        assert!(!sending(), "voice activity");
        set_on(true);
        hold(true);
        assert!(!sending(), "push to talk with its key held too");
        hold_mute(false);
        assert!(sending());
        hold(false);
        set_on(false);
    }

    #[test]
    fn the_clock_sends_while_held_and_within_the_delay() {
        let delay = Duration::from_millis(20);
        assert!(transmits(true, Duration::ZERO, delay));
        assert!(transmits(true, Duration::from_secs(5), delay));
        assert!(transmits(false, Duration::from_millis(19), delay));
        assert!(!transmits(false, delay, delay));
        assert!(!transmits(false, Duration::from_millis(5), Duration::ZERO));
    }
}
