//! What the blocking side watches for while it waits on a callback: a
//! device that stopped calling, and a microphone that delivers only silence.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use super::super::Format;

/// How long a device may stay quiet once it has called back, before it
/// counts as gone: a Bluetooth headset that left, or an unplugged USB card.
const STALL_AFTER: Duration = Duration::from_secs(2);

/// How long a device may take to call back the first time. A microphone's
/// first callback waits for the user to answer macOS's permission prompt.
const FIRST_CALL_WITHIN: Duration = Duration::from_secs(20);

/// How much exact digital silence from a microphone is worth a warning.
/// A working microphone, even a muted one in a quiet room, adds noise;
/// macOS hands an app without permission zeros.
const SILENCE_WARNING_AFTER: Duration = Duration::from_secs(3);

/// Notes whether a callback keeps being called.
pub struct Watchdog {
    beats: u64,
    since: Instant,
    called: bool,
}

impl Watchdog {
    pub fn start(now: Instant) -> Self {
        Self {
            beats: 0,
            since: now,
            called: false,
        }
    }

    /// Fails once `beats`, the callback's count, hasn't moved for too long.
    /// `failure` is the status the callback's last call failed with, 0 for
    /// none: a device that only fails is given up on as soon as one that
    /// went quiet is, and the error says why.
    pub fn check(&mut self, beats: u64, failure: i32, now: Instant) -> Result<()> {
        if beats != self.beats {
            self.beats = beats;
            self.since = now;
            self.called = true;
            return Ok(());
        }
        let failing = failure != 0;
        let patience = if self.called || failing {
            STALL_AFTER
        } else {
            FIRST_CALL_WITHIN
        };
        if now.duration_since(self.since) > patience {
            if failing {
                bail!(
                    "the device's callback fails with status {}",
                    super::devices::describe(failure)
                );
            }
            bail!("the device stopped calling back for {patience:?}");
        }
        Ok(())
    }
}

/// Counts the exact zeros a microphone delivers in a row.
pub struct SilenceWatch {
    limit: usize,
    run: usize,
    warned: bool,
}

impl SilenceWatch {
    pub fn new(format: Format) -> Self {
        Self {
            limit: format.bytes(SILENCE_WARNING_AFTER),
            run: 0,
            warned: false,
        }
    }

    /// Takes the next bytes read and says whether this is the moment to
    /// warn: once, when the zeros have lasted long enough.
    pub fn observe(&mut self, bytes: &[u8]) -> bool {
        let quiet_tail = bytes.iter().rev().take_while(|byte| **byte == 0).count();
        self.run = if quiet_tail == bytes.len() {
            self.run + bytes.len()
        } else {
            quiet_tail
        };
        if self.warned || self.limit == 0 || self.run < self.limit {
            return false;
        }
        self.warned = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORMAT: Format = Format {
        rate: 1_000,
        channels: 1,
        float: false,
    };

    #[test]
    fn a_device_that_keeps_calling_is_fine() {
        let start = Instant::now();
        let mut watchdog = Watchdog::start(start);
        for second in 1..30 {
            assert!(
                watchdog
                    .check(second, 0, start + Duration::from_secs(second))
                    .is_ok()
            );
        }
    }

    #[test]
    fn a_first_call_may_take_as_long_as_a_permission_prompt() {
        let start = Instant::now();
        let mut watchdog = Watchdog::start(start);
        assert!(
            watchdog
                .check(0, 0, start + Duration::from_secs(10))
                .is_ok()
        );
        assert!(
            watchdog
                .check(0, 0, start + Duration::from_secs(21))
                .is_err()
        );
    }

    #[test]
    fn a_device_that_called_and_went_quiet_is_gone_sooner() {
        let start = Instant::now();
        let mut watchdog = Watchdog::start(start);
        assert!(watchdog.check(5, 0, start).is_ok());
        assert!(watchdog.check(5, 0, start + Duration::from_secs(1)).is_ok());
        assert!(
            watchdog
                .check(5, 0, start + Duration::from_secs(3))
                .is_err()
        );
    }

    #[test]
    fn a_device_that_only_fails_is_given_up_on_with_its_status() {
        let start = Instant::now();
        let mut watchdog = Watchdog::start(start);
        assert!(
            watchdog
                .check(0, -10_868, start + Duration::from_secs(1))
                .is_ok()
        );
        let error = watchdog
            .check(0, -10_868, start + Duration::from_secs(3))
            .unwrap_err();
        assert!(error.to_string().contains("10868"), "{error}");
    }

    #[test]
    fn silence_warns_once_after_three_seconds() {
        let mut watch = SilenceWatch::new(FORMAT);
        // One second is 2000 bytes at 1 kHz in 16 bits.
        assert!(!watch.observe(&[0; 2_000]));
        assert!(!watch.observe(&[0; 2_000]));
        assert!(watch.observe(&[0; 2_000]));
        assert!(!watch.observe(&[0; 2_000]));
    }

    #[test]
    fn a_sound_starts_the_count_again() {
        let mut watch = SilenceWatch::new(FORMAT);
        assert!(!watch.observe(&[0; 5_000]));
        assert!(!watch.observe(&[0, 0, 7, 0, 0]));
        assert!(!watch.observe(&[0; 5_000]));
        assert!(watch.observe(&[0; 1_000]));
    }
}
